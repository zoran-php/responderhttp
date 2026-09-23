// http_client/src/services/websocket.ts
//
// The only place invoke() is called and a Channel is created for WebSocket
// commands (CLAUDE.md section 11, rule 3). One Channel per connection
// carries its events; @tauri-apps/api re-orders channel messages by index,
// so they arrive here in the order Rust sent them.
import { Channel, invoke } from "@tauri-apps/api/core";

import { toApiError } from "@/lib/api-error";
import { createEventBatcher, type FlushScheduler } from "@/lib/event-batcher";
import { call } from "@/services/invoke";
import { logDebug, logWarn } from "@/services/logger";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";
import type { WebSocketRequest, WsEvent, WsPayload } from "@/types/websocket";

const CONNECT_COMMAND = "connect_web_socket";
const SEND_COMMAND = "send_web_socket_message";
const DISCONNECT_COMMAND = "disconnect_web_socket";
const DISCONNECT_ALL_COMMAND = "disconnect_all_web_sockets";

/**
 * Flush without waiting for a frame at this many queued events. Frames stop
 * while the window is in the tray; a busy connection must not grow the
 * queue without bound meanwhile.
 */
export const MAX_EVENTS_PER_BATCH = 500;

const nextFrame: FlushScheduler = (flush) => {
  requestAnimationFrame(() => flush());
};

/**
 * Messages are what arrive by the thousand; everything else changes the
 * connection's state and is shown at once.
 */
function isLifecycle(event: WsEvent): boolean {
  return event.type !== "sent" && event.type !== "received";
}

/**
 * Opens a connection. Resolves when the handshake is done, or with why it
 * was not. Every event, `connected` included, goes to `onEvents` in batches,
 * at most one per animation frame, except that lifecycle events (connected,
 * closed, error, reconnecting) flush at once. Do not rely on `connected`
 * arriving before or after this resolves: they travel on different paths.
 *
 * `request` is fully resolved; `logUrl` is the URL with secret variables
 * left as `{{placeholders}}`, as for `sendRequest`. Nothing else about the
 * connection is logged, and no message ever is (section 11, rule 6).
 */
export async function connectWebSocket(
  connectionId: string,
  request: WebSocketRequest,
  logUrl: string,
  onEvents: (events: WsEvent[]) => void,
  schedule: FlushScheduler = nextFrame,
): Promise<Result<void, ApiError>> {
  const batcher = createEventBatcher(onEvents, schedule, {
    isUrgent: isLifecycle,
    maxBatch: MAX_EVENTS_PER_BATCH,
  });
  const channel = new Channel<WsEvent>((event) => {
    if (event.type === "closed") {
      logDebug(`websocket ${logUrl} closed by ${event.by}, code ${event.code ?? "none"}`);
    }
    batcher.push(event);
  });

  try {
    await invoke(CONNECT_COMMAND, { connectionId, request, onEvent: channel });
    logDebug(`websocket ${logUrl} connected`);
    return { ok: true, value: undefined };
  } catch (error) {
    const mapped = toApiError(error);
    logWarn(`websocket ${logUrl} failed to connect: ${mapped.kind}: ${mapped.message}`);
    return { ok: false, error: mapped };
  }
}

/**
 * Queues a message. Success means it was accepted, not delivered: its
 * `sent` event follows once it is on the wire.
 */
export async function sendWebSocketMessage(
  connectionId: string,
  message: WsPayload,
): Promise<Result<void, ApiError>> {
  const result = await call<null>(SEND_COMMAND, { connectionId, message });
  return result.ok ? { ok: true, value: undefined } : result;
}

/**
 * Fire-and-forget, like `cancelRequest`: the `closed` event reports the
 * outcome. Also cancels a handshake in progress, whose connect then fails
 * with `cancelled`.
 */
export async function disconnectWebSocket(connectionId: string): Promise<void> {
  try {
    await invoke(DISCONNECT_COMMAND, { connectionId });
  } catch {
    // Nothing useful to do: the connection is closing or already gone.
  }
}

/**
 * Closes every connection Rust holds. Called once as the frontend starts: a
 * webview reload would otherwise leave the old page's connections running
 * with nobody to show their events to. Fire-and-forget, like the others.
 */
export async function disconnectAllWebSockets(): Promise<void> {
  try {
    await invoke(DISCONNECT_ALL_COMMAND);
  } catch {
    // Nothing to report: at worst an orphaned connection stays open.
  }
}

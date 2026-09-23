// http_client/src/lib/ws-status.ts
//
// What a WebSocket tab's status badge and primary button say in each
// connection state: the state matrix in PLAN-WEBSOCKET.md section 5, as data.
// The component maps tones to colour tokens and renders.
import type { WsLogFilter } from "@/lib/ws-log";

/** Where a WebSocket tab's connection is. `idle` covers never connected and
 * closed alike; the log tells the two apart. */
export type WsConnectionState =
  "idle" | "connecting" | "connected" | "disconnecting" | "reconnecting";

export type WsBadgeTone = "disconnected" | "pending" | "connected";

export interface WsBadge {
  label: string;
  tone: WsBadgeTone;
}

export function connectionBadge(
  state: WsConnectionState,
  reconnectAttempt: { attempt: number; maxAttempts: number } | null,
): WsBadge {
  switch (state) {
    case "idle":
      return { label: "Disconnected", tone: "disconnected" };
    case "connecting":
      return { label: "Connecting…", tone: "pending" };
    case "connected":
      return { label: "Connected", tone: "connected" };
    case "disconnecting":
      return { label: "Disconnecting…", tone: "pending" };
    case "reconnecting":
      return {
        label:
          reconnectAttempt === null
            ? "Reconnecting…"
            : `Reconnecting… ${reconnectAttempt.attempt}/${reconnectAttempt.maxAttempts}`,
        tone: "pending",
      };
  }
}

/** The button beside the URL: what it says and what pressing it does. */
export type WsPrimaryAction =
  | { kind: "connect"; label: "Connect" }
  | { kind: "cancel"; label: "Cancel" }
  | { kind: "disconnect"; label: "Disconnect" }
  | { kind: "busy"; label: "Disconnecting…" };

export function primaryAction(state: WsConnectionState): WsPrimaryAction {
  switch (state) {
    case "idle":
      return { kind: "connect", label: "Connect" };
    case "connecting":
    case "reconnecting":
      return { kind: "cancel", label: "Cancel" };
    case "connected":
      return { kind: "disconnect", label: "Disconnect" };
    case "disconnecting":
      return { kind: "busy", label: "Disconnecting…" };
  }
}

export const LOG_FILTER_OPTIONS: readonly { value: WsLogFilter; label: string }[] = [
  { value: "all", label: "All Messages" },
  { value: "sent", label: "Sent" },
  { value: "received", label: "Received" },
  { value: "system", label: "Connection events" },
];

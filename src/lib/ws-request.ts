// http_client/src/lib/ws-request.ts
//
// A WebSocket tab's saveable shape and its dirty check, the counterpart of
// request-defaults.ts for HTTP tabs.
import type { KeyValue } from "@/types/http";
import {
  DEFAULT_WS_SETTINGS,
  EMPTY_WS_DRAFT,
  type WebSocketRequest,
  type WebSocketSettings,
  type WsDraft,
} from "@/types/websocket";

/** Everything Save writes for a WebSocket request, and all that dirty
 * tracking compares. The log and the connection are never part of it. */
export interface WebSocketShape {
  request: WebSocketRequest;
  draft: WsDraft;
}

export function emptyWebSocketShape(): WebSocketShape {
  return {
    request: { url: "", headers: [], settings: DEFAULT_WS_SETTINGS },
    draft: EMPTY_WS_DRAFT,
  };
}

export function webSocketShapesEqual(a: WebSocketShape, b: WebSocketShape): boolean {
  return (
    a.request.url === b.request.url &&
    pairsEqual(a.request.headers, b.request.headers) &&
    settingsEqual(a.request.settings, b.request.settings) &&
    a.draft.format === b.draft.format &&
    a.draft.binaryEncoding === b.draft.binaryEncoding &&
    a.draft.text === b.draft.text
  );
}

function pairsEqual(a: readonly KeyValue[], b: readonly KeyValue[]): boolean {
  return (
    a.length === b.length &&
    a.every((pair, index) => {
      const other = b[index];
      return other !== undefined && pair.name === other.name && pair.value === other.value;
    })
  );
}

function settingsEqual(a: WebSocketSettings, b: WebSocketSettings): boolean {
  return (
    a.verifyTls === b.verifyTls &&
    a.proxy === b.proxy &&
    a.sendCookies === b.sendCookies &&
    a.connectTimeoutMs === b.connectTimeoutMs &&
    a.maxMessageBytes === b.maxMessageBytes &&
    a.autoReconnect === b.autoReconnect
  );
}

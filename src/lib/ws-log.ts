// http_client/src/lib/ws-log.ts
//
// A WebSocket tab's activity log: what goes in, what is kept, what is shown,
// and what the two Clear actions remove (PLAN.md Phase 13e, section 5). Pure;
// the store holds the result and components render it.
import {
  appendCapped as appendCappedList,
  emptyCappedList,
  type CappedCaps,
  type CappedList,
} from "@/lib/capped-list";
import { hexToBase64, payloadPreview } from "@/lib/ws-payload";
import type { WsBinaryEncoding, WsEvent, WsPayload } from "@/types/websocket";

export type WsLogDirection = "sent" | "received" | "system";
export type WsLogFilter = "all" | WsLogDirection;

export interface WsLogEntry {
  /** Stable for the entry's life, so React keys never use an index. */
  id: string;
  /** The connect that produced it. Reconnects keep their session's id. */
  connectionId: string;
  event: WsEvent;
}

/** Oldest first. The log view shows it newest first. */
export type WsLog = CappedList<WsLogEntry>;

export type WsLogCaps = CappedCaps;

/** Assumption 6 in PLAN-WEBSOCKET.md: a chatty server must not be able to
 * fill memory through the log. */
export const WS_LOG_CAPS: WsLogCaps = { maxEntries: 1000, maxBytes: 32 * 1024 * 1024 };

export const EMPTY_WS_LOG: WsLog = emptyCappedList();

let nextEntryId = 0;

function newEntryId(): string {
  nextEntryId += 1;
  return `wslog-${nextEntryId}`;
}

export function directionOf(event: WsEvent): WsLogDirection {
  return event.type === "sent" || event.type === "received" ? event.type : "system";
}

/**
 * What an entry costs to keep. Messages count their wire size; a hex payload
 * is held as twice that in characters, and a JS string is two bytes a
 * character, so this under-counts memory by a constant factor. The cap is a
 * guard against a runaway server, not an accountant, and a constant factor
 * does not change what it guards against.
 */
function entryBytes(event: WsEvent): number {
  switch (event.type) {
    case "sent":
    case "received":
      return event.byteLength;
    case "connected":
      return event.headers.reduce(
        (sum, header) => sum + header.name.length + header.value.length,
        0,
      );
    default:
      return 0;
  }
}

export function logEntries(connectionId: string, events: readonly WsEvent[]): WsLogEntry[] {
  return events.map((event) => ({ id: newEntryId(), connectionId, event }));
}

/** The shared list's rule, with the log's own idea of an entry's size. */
export function appendCapped(
  log: WsLog,
  added: readonly WsLogEntry[],
  caps: WsLogCaps = WS_LOG_CAPS,
): WsLog {
  return appendCappedList(log, added, (entry) => entryBytes(entry.event), caps);
}

/**
 * Clear Messages: every entry from every connection, the handshake details
 * and the dropped note with them. The connection itself, the draft, the
 * filter and the search are not the log's, and stay.
 */
export function clearAll(): WsLog {
  return EMPTY_WS_LOG;
}

/** The text a system entry shows, and what the search box matches. */
export function systemText(event: WsEvent): string {
  switch (event.type) {
    case "connected":
      return `Connected to ${event.url}`;
    case "closed": {
      const lead =
        event.by === "user"
          ? "Disconnected"
          : event.by === "server"
            ? "Closed by the server"
            : "Connection closed";
      const detail =
        event.code === null
          ? event.reason
          : event.reason === ""
            ? String(event.code)
            : `${event.code}: ${event.reason}`;
      return detail === "" ? lead : `${lead} (${detail})`;
    }
    case "error":
      return event.message;
    case "reconnecting":
      return `Reconnecting in ${formatDelay(event.delayMs)} (attempt ${event.attempt} of ${event.maxAttempts})`;
    case "sent":
    case "received":
      return payloadPreview(event.payload);
  }
}

function formatDelay(delayMs: number): string {
  return delayMs < 1000 ? `${delayMs} ms` : `${Math.round(delayMs / 100) / 10} s`;
}

/**
 * The entries the log shows for this filter and search. The search is a
 * case-insensitive substring match on text payloads and system text; a
 * binary payload matches on its hex, with spaces in the query ignored, so
 * `de ad` finds `dead`, and also on its Base64 when that is how the log is
 * showing binary.
 */
export function filterLog(
  entries: readonly WsLogEntry[],
  filter: { direction: WsLogFilter; query: string; binaryEncoding?: WsBinaryEncoding },
): WsLogEntry[] {
  const query = filter.query.trim().toLowerCase();
  const hexQuery = query.replace(/\s+/g, "");
  // Base64 is case-sensitive, so it is matched against the query as typed.
  const base64Query = filter.binaryEncoding === "base64" ? filter.query.replace(/\s+/g, "") : "";
  return entries.filter((entry) => {
    if (filter.direction !== "all" && directionOf(entry.event) !== filter.direction) {
      return false;
    }
    if (query === "") {
      return true;
    }
    const { event } = entry;
    if (event.type === "sent" || event.type === "received") {
      return event.payload.kind === "text"
        ? event.payload.text.toLowerCase().includes(query)
        : (hexQuery !== "" && event.payload.hex.includes(hexQuery)) ||
            (base64Query !== "" && hexToBase64(event.payload.hex).includes(base64Query));
    }
    return systemText(event).toLowerCase().includes(query);
  });
}

/** A message the store handed to the service, not yet reported as sent. */
export interface PendingSend {
  /** What went to Rust, and what its `sent` event will carry. */
  resolved: WsPayload;
  /** The same message with secret variables left as `{{placeholders}}`. */
  display: WsPayload;
}

/**
 * Pairs a `sent` event with the message the store queued, so the log shows
 * the display version and a secret variable's value never reaches it
 * (PLAN.md Phase 9). Messages go out in the order they were queued, so the
 * match is the first pending entry with the same resolved payload. Entries
 * before it were dropped by Rust (typed while a reconnect was pending) and
 * are discarded here too. With no match, the event stands as it came.
 */
export function takePendingSend(
  pending: readonly PendingSend[],
  sent: WsPayload,
): { display: WsPayload; rest: PendingSend[] } {
  const index = pending.findIndex((entry) => samePayload(entry.resolved, sent));
  const match = pending[index];
  if (match === undefined) {
    return { display: sent, rest: [...pending] };
  }
  return { display: match.display, rest: pending.slice(index + 1) };
}

function samePayload(a: WsPayload, b: WsPayload): boolean {
  if (a.kind === "text" && b.kind === "text") {
    return a.text === b.text;
  }
  if (a.kind === "binary" && b.kind === "binary") {
    return a.hex === b.hex;
  }
  return false;
}

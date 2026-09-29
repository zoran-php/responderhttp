// http_client/src/lib/grpc-log.ts
//
// A gRPC tab's message stream: what goes in, what is kept, what is shown
// (PLAN-GRPC.md 16h). The same shape and caps as the WebSocket log, so the
// shared log component (16i) can draw both. Pure; the store holds the
// result and components render it.
import {
  appendCapped as appendCappedList,
  emptyCappedList,
  type CappedCaps,
  type CappedList,
} from "@/lib/capped-list";
import type { GrpcEvent } from "@/types/grpc";

export type GrpcLogDirection = "sent" | "received" | "system";
export type GrpcLogFilter = "all" | GrpcLogDirection;

export interface GrpcLogEntry {
  /** Stable for the entry's life, so React keys never use an index. */
  id: string;
  /** The Invoke that produced it. The log keeps earlier calls until cleared. */
  callId: string;
  event: GrpcEvent;
}

/** Oldest first, like the WebSocket log. */
export type GrpcLog = CappedList<GrpcLogEntry>;

/** Assumption 5 in PLAN-GRPC.md: the WebSocket caps. */
export const GRPC_LOG_CAPS: CappedCaps = { maxEntries: 1000, maxBytes: 32 * 1024 * 1024 };

export const EMPTY_GRPC_LOG: GrpcLog = emptyCappedList();

let nextEntryId = 0;

function newEntryId(): string {
  nextEntryId += 1;
  return `grpclog-${nextEntryId}`;
}

export function directionOf(event: GrpcEvent): GrpcLogDirection {
  return event.type === "sent" || event.type === "received" ? event.type : "system";
}

/**
 * What an entry costs to keep: the JSON text it holds, which is what sits
 * in memory, rather than the protobuf size it reports. Metadata counts its
 * names and values. Like the WebSocket log, a guard against a runaway
 * stream, not an exact account.
 */
function entryBytes(event: GrpcEvent): number {
  switch (event.type) {
    case "sent":
    case "received":
      return event.json.length;
    case "responseMetadata":
      return pairsLength(event.metadata);
    case "ended":
      return pairsLength(event.trailers) + event.status.message.length;
    case "streamEnded":
      return 0;
  }
}

function pairsLength(pairs: readonly { name: string; value: string }[]): number {
  return pairs.reduce((sum, pair) => sum + pair.name.length + pair.value.length, 0);
}

export function logEntries(callId: string, events: readonly GrpcEvent[]): GrpcLogEntry[] {
  return events.map((event) => ({ id: newEntryId(), callId, event }));
}

export function appendCapped(
  log: GrpcLog,
  added: readonly GrpcLogEntry[],
  caps: CappedCaps = GRPC_LOG_CAPS,
): GrpcLog {
  return appendCappedList(log, added, (entry) => entryBytes(entry.event), caps);
}

/** Clear Messages: every entry from every call. The request is untouched. */
export function clearAll(): GrpcLog {
  return EMPTY_GRPC_LOG;
}

/** The line an informative entry shows, and what the search matches. */
export function systemText(event: GrpcEvent): string {
  switch (event.type) {
    case "responseMetadata": {
      const count = event.metadata.length;
      return count === 1 ? "Response metadata: 1 entry" : `Response metadata: ${count} entries`;
    }
    case "streamEnded":
      return "Streaming ended: the client sends nothing more";
    case "ended": {
      const lead = event.by === "server" ? "Call ended" : "Call ended by the app";
      const status = `${event.status.code} ${event.status.name}`;
      const message = event.status.message === "" ? "" : `: ${event.status.message}`;
      return `${lead}, ${status}${message} (${formatDuration(event.totalMs)})`;
    }
    case "sent":
    case "received":
      return event.json;
  }
}

function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${Math.round(ms / 100) / 10} s`;
}

/**
 * The entries shown for this filter and search: a case-insensitive
 * substring match on a message's JSON text or an informative line. A
 * metadata entry also matches on its names and values.
 */
export function filterLog(
  entries: readonly GrpcLogEntry[],
  filter: { direction: GrpcLogFilter; query: string },
): GrpcLogEntry[] {
  const query = filter.query.trim().toLowerCase();
  return entries.filter((entry) => {
    if (filter.direction !== "all" && directionOf(entry.event) !== filter.direction) {
      return false;
    }
    if (query === "") {
      return true;
    }
    const { event } = entry;
    if (systemText(event).toLowerCase().includes(query)) {
      return true;
    }
    const pairs =
      event.type === "responseMetadata"
        ? event.metadata
        : event.type === "ended"
          ? event.trailers
          : [];
    return pairs.some(
      (pair) => pair.name.toLowerCase().includes(query) || pair.value.toLowerCase().includes(query),
    );
  });
}

/** A message handed to Rust whose `sent` event has not arrived yet. */
export interface PendingSend {
  /** The text sent, variables substituted. */
  resolved: string;
  /** The same text with secret variables left as `{{placeholders}}`. */
  display: string;
}

/**
 * Keeps a secret variable's value out of the log (PLAN.md Phase 9). A
 * `sent` event carries the message as Rust re-encoded it, secrets and all.
 * Messages are sent in the order they were accepted and each accepted one
 * reports exactly one `sent`, so each `sent` belongs to the oldest pending
 * entry. When that entry's display differs from what was sent, the log
 * shows the display text as typed instead; otherwise Rust's canonical text
 * stands. Events other than `sent` pass through.
 */
export function applyPendingSends(
  pending: readonly PendingSend[],
  events: readonly GrpcEvent[],
): { events: GrpcEvent[]; rest: PendingSend[] } {
  const rest = [...pending];
  const shown = events.map((event) => {
    if (event.type !== "sent") {
      return event;
    }
    const match = rest.shift();
    if (match === undefined || match.display === match.resolved) {
      return event;
    }
    return { ...event, json: match.display };
  });
  return { events: shown, rest };
}

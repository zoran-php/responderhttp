// http_client/src/lib/sse-log.ts
//
// An event-stream response as the viewer shows it: the blocks kept, what
// they cost, and how they number (PLAN-SSE.md, 14d). Pure; the store holds
// the result and components render it.
import {
  appendCapped as appendCappedList,
  emptyCappedList,
  type CappedCaps,
  type CappedList,
} from "@/lib/capped-list";
import { essenceOf, findHeader } from "@/lib/content-type";
import type { HttpStreamEvent, KeyValue, SseBlock } from "@/types/http";

/** The media type that turns a response into a live stream. */
export const EVENT_STREAM = "text/event-stream";

export interface SseLogEntry {
  /** Stable for the entry's life, so React keys never use an index. */
  id: string;
  /**
   * Its place in the stream, counting from 1 and counting blocks the caps
   * have since evicted. Shown next to the event, so it stays truthful after
   * an eviction — which a position in `entries` would not.
   */
  index: number;
  /** Wall clock from Rust, milliseconds since the epoch. */
  atMs: number;
  /** The block's size on the wire, from Rust. */
  bytes: number;
  block: SseBlock;
}

/** Oldest first. The Events view shows it newest first. */
export type SseLog = CappedList<SseLogEntry>;

/**
 * The same ceiling the WebSocket log has, for the same reason: a stream that
 * never ends must not be able to fill memory through the viewer.
 */
export const SSE_LOG_CAPS: CappedCaps = { maxEntries: 1000, maxBytes: 32 * 1024 * 1024 };

export const EMPTY_SSE_LOG: SseLog = emptyCappedList();

let nextEntryId = 0;

function newEntryId(): string {
  nextEntryId += 1;
  return `sse-${nextEntryId}`;
}

/**
 * What the response says it is. Parameters are ignored: a stream arrives as
 * `text/event-stream; charset=utf-8` as often as not.
 */
export function isEventStream(headers: readonly KeyValue[]): boolean {
  return essenceOf(findHeader([...headers], "content-type") ?? "") === EVENT_STREAM;
}

/**
 * Numbers a batch of blocks on from what the log has already seen, dropped
 * ones included.
 */
export function logEntries(log: SseLog, events: readonly HttpStreamEvent[]): SseLogEntry[] {
  let index = log.droppedCount + log.entries.length;
  const entries: SseLogEntry[] = [];
  for (const event of events) {
    if (event.type !== "block") {
      continue;
    }
    index += 1;
    entries.push({
      id: newEntryId(),
      index,
      atMs: event.atMs,
      bytes: event.bytes,
      block: event.block,
    });
  }
  return entries;
}

/** The shared list's rule, with the log's own idea of an entry's size. */
export function appendCapped(
  log: SseLog,
  added: readonly SseLogEntry[],
  caps: CappedCaps = SSE_LOG_CAPS,
): SseLog {
  return appendCappedList(log, added, (entry) => entry.bytes, caps);
}

/**
 * The stream as it arrived, oldest first, for the Raw view while it is still
 * arriving — the Raw view is the transfer, not the list, so it is not
 * reversed.
 * Once the request finishes the response body is the authority — it is the
 * whole transfer, including anything the caps dropped.
 */
export function rawText(log: SseLog): string {
  return log.entries.map((entry) => entry.block.raw).join("");
}

/** What the Events list shows as an event's name. */
export function blockLabel(block: SseBlock): string {
  return block.kind === "comment" ? "comment" : block.name;
}

/**
 * What a request has reported so far. `head` arrives long before the body is
 * complete — on a stream that never ends, it is all the response there will
 * be for a while, which is the whole point of streaming.
 */
export interface ResponseStream {
  head: { status: number; headers: KeyValue[]; bytes: number } | null;
  /** Content-Type said `text/event-stream`, so the viewer shows events. */
  isEventStream: boolean;
  /**
   * Every block's bytes, counting the ones the caps have dropped: it is what
   * the server has sent, not what the viewer still holds.
   */
  receivedBytes: number;
  log: SseLog;
}

export const EMPTY_STREAM: ResponseStream = {
  head: null,
  isEventStream: false,
  receivedBytes: 0,
  log: EMPTY_SSE_LOG,
};

/** Folds one batch in. The store holds the result and renders it. */
export function applyStreamEvents(
  stream: ResponseStream,
  events: readonly HttpStreamEvent[],
  caps: CappedCaps = SSE_LOG_CAPS,
): ResponseStream {
  let head = stream.head;
  let eventStream = stream.isEventStream;
  let receivedBytes = stream.receivedBytes;
  for (const event of events) {
    if (event.type === "headers") {
      head = { status: event.status, headers: event.headers, bytes: event.bytes };
      eventStream = isEventStream(event.headers);
    } else {
      receivedBytes += event.bytes;
    }
  }
  return {
    head,
    isEventStream: eventStream,
    receivedBytes,
    log: appendCapped(stream.log, logEntries(stream.log, events), caps),
  };
}

import { describe, expect, it } from "vitest";

import {
  appendCapped,
  applyStreamEvents,
  blockLabel,
  EMPTY_SSE_LOG,
  EMPTY_STREAM,
  isEventStream,
  logEntries,
  rawText,
  type SseLog,
} from "@/lib/sse-log";
import type { HttpStreamEvent, SseBlock } from "@/types/http";

const event = (data: string): SseBlock => ({
  kind: "event",
  name: "message",
  data,
  id: null,
  retry: null,
  raw: `data: ${data}\n\n`,
});

/** Everything here is ASCII, so a raw block's length is its byte count. */
const arrived = (data: string, atMs = 1): HttpStreamEvent => {
  const block = event(data);
  return { type: "block", atMs, bytes: block.raw.length, block };
};

/** Appends in one step, the way the store does. */
function append(
  log: SseLog,
  events: readonly HttpStreamEvent[],
  caps?: Parameters<typeof appendCapped>[2],
): SseLog {
  return appendCapped(log, logEntries(log, events), caps);
}

describe("isEventStream", () => {
  it("recognises the media type whatever its parameters say", () => {
    expect(
      isEventStream([{ name: "Content-Type", value: "text/event-stream; charset=utf-8" }]),
    ).toBe(true);
  });

  it("is false for anything else, including a missing content type", () => {
    expect(isEventStream([{ name: "content-type", value: "application/json" }])).toBe(false);
    expect(isEventStream([])).toBe(false);
  });
});

describe("logEntries", () => {
  it("keeps only blocks, and numbers them from one", () => {
    const headers: HttpStreamEvent = { type: "headers", status: 200, headers: [], bytes: 0 };

    const entries = logEntries(EMPTY_SSE_LOG, [headers, arrived("a"), arrived("b")]);

    expect(entries.map((entry) => entry.index)).toEqual([1, 2]);
    expect(entries.map((entry) => entry.block.raw)).toEqual(["data: a\n\n", "data: b\n\n"]);
  });

  it("gives every entry an id of its own", () => {
    const entries = logEntries(EMPTY_SSE_LOG, [arrived("a"), arrived("a")]);

    expect(entries[0]?.id).not.toEqual(entries[1]?.id);
  });

  it("numbers on from what was dropped, so the count is what the server sent", () => {
    const caps = { maxEntries: 2, maxBytes: 1024 };
    let log = append(EMPTY_SSE_LOG, [arrived("a"), arrived("b"), arrived("c")], caps);

    log = append(log, [arrived("d")], caps);

    expect(log.droppedCount).toBe(2);
    expect(log.entries.map((entry) => entry.index)).toEqual([3, 4]);
  });
});

describe("what the log keeps", () => {
  it("drops the oldest blocks once it is over its caps", () => {
    const caps = { maxEntries: 10, maxBytes: 20 };

    const log = append(
      EMPTY_SSE_LOG,
      [arrived("aaaaaa"), arrived("bbbbbb"), arrived("cccccc")],
      caps,
    );

    expect(log.entries.map((entry) => entry.block.raw)).toEqual(["data: cccccc\n\n"]);
    expect(log.droppedCount).toBe(2);
  });

  it("joins the raw blocks back into the stream as it arrived", () => {
    const log = append(EMPTY_SSE_LOG, [arrived("a"), arrived("b")]);

    expect(rawText(log)).toBe("data: a\n\ndata: b\n\n");
  });
});

describe("blockLabel", () => {
  it("names an event by its type and a comment as one", () => {
    expect(blockLabel(event("x"))).toBe("message");
    expect(blockLabel({ kind: "comment", text: "keep-alive", raw: ": keep-alive\n\n" })).toBe(
      "comment",
    );
  });
});

describe("applyStreamEvents", () => {
  const headers = (contentType: string): HttpStreamEvent => ({
    type: "headers",
    status: 200,
    headers: [{ name: "content-type", value: contentType }],
    bytes: 120,
  });

  it("keeps the head and marks a stream as one", () => {
    const stream = applyStreamEvents(EMPTY_STREAM, [headers("text/event-stream"), arrived("a")]);

    expect(stream.head).toEqual({
      status: 200,
      headers: [{ name: "content-type", value: "text/event-stream" }],
      bytes: 120,
    });
    expect(stream.isEventStream).toBe(true);
    expect(stream.log.entries).toHaveLength(1);
  });

  it("leaves an ordinary response with a head and no events", () => {
    const stream = applyStreamEvents(EMPTY_STREAM, [headers("application/json")]);

    expect(stream.isEventStream).toBe(false);
    expect(stream.log.entries).toHaveLength(0);
  });

  it("counts the bytes of every block, including the ones the caps dropped", () => {
    const caps = { maxEntries: 1, maxBytes: 1024 };

    const stream = applyStreamEvents(
      EMPTY_STREAM,
      [headers("text/event-stream"), arrived("a"), arrived("b")],
      caps,
    );

    // "data: a\n\n" twice: what arrived, not what is still held.
    expect(stream.receivedBytes).toBe(18);
    expect(stream.log.entries).toHaveLength(1);
  });

  it("carries what it was given forward across batches", () => {
    const first = applyStreamEvents(EMPTY_STREAM, [headers("text/event-stream"), arrived("a")]);

    const second = applyStreamEvents(first, [arrived("b")]);

    expect(second.isEventStream).toBe(true);
    expect(second.head).toEqual(first.head);
    expect(second.log.entries.map((entry) => entry.index)).toEqual([1, 2]);
  });
});

describe("what a long stream costs", () => {
  it("folds ten thousand events in batches without growing past its caps", () => {
    const batch = Array.from({ length: 50 }, (_unused, index) => arrived(`event ${index}`));
    const started = performance.now();

    let stream = applyStreamEvents(EMPTY_STREAM, [
      {
        type: "headers",
        status: 200,
        headers: [{ name: "content-type", value: "text/event-stream" }],
        bytes: 120,
      },
    ]);
    for (let round = 0; round < 200; round += 1) {
      stream = applyStreamEvents(stream, batch);
    }

    expect(stream.log.entries).toHaveLength(1000);
    expect(stream.log.droppedCount).toBe(9000);
    expect(stream.log.entries[999]?.index).toBe(10_000);
    // Generous: this is a guard against an accidental quadratic fold, not a
    // benchmark. It runs in a few milliseconds when the fold is linear.
    expect(performance.now() - started).toBeLessThan(2000);
  });
});

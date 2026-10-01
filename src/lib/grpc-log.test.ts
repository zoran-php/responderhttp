import { describe, expect, it } from "vitest";

import {
  appendCapped,
  applyPendingSends,
  directionOf,
  EMPTY_GRPC_LOG,
  filterLog,
  logEntries,
  systemText,
} from "@/lib/grpc-log";
import type { GrpcEvent } from "@/types/grpc";

const sent = (json: string): GrpcEvent => ({ type: "sent", atMs: 1, json, bytes: json.length });
const received = (json: string): GrpcEvent => ({
  type: "received",
  atMs: 2,
  json,
  bytes: json.length,
});
const metadata: GrpcEvent = {
  type: "responseMetadata",
  atMs: 3,
  metadata: [{ name: "x-request-id", value: "abc-123" }],
};
const ended = (by: "server" | "client", code: number, name: string, message = ""): GrpcEvent => ({
  type: "ended",
  atMs: 4,
  status: { code, name, message },
  trailers: [{ name: "x-trace", value: "t-9" }],
  by,
  totalMs: 1234,
});

describe("directionOf", () => {
  it("puts messages in their direction and everything else under system", () => {
    expect(directionOf(sent("{}"))).toBe("sent");
    expect(directionOf(received("{}"))).toBe("received");
    expect(directionOf(metadata)).toBe("system");
    expect(directionOf({ type: "streamEnded", atMs: 1 })).toBe("system");
    expect(directionOf(ended("server", 0, "OK"))).toBe("system");
  });
});

describe("logEntries and appendCapped", () => {
  it("gives every entry its own id and keeps the call it came from", () => {
    const entries = logEntries("call-1", [sent("{}"), received("{}")]);

    expect(entries.map((entry) => entry.callId)).toEqual(["call-1", "call-1"]);
    expect(new Set(entries.map((entry) => entry.id)).size).toBe(2);
  });

  it("drops the oldest entries over the caps and counts them", () => {
    const log = appendCapped(
      EMPTY_GRPC_LOG,
      logEntries("c", [received("1"), received("2"), received("3")]),
      { maxEntries: 2, maxBytes: 1000 },
    );

    expect(log.entries.map((entry) => systemText(entry.event))).toEqual(["2", "3"]);
    expect(log.droppedCount).toBe(1);
  });

  it("counts a message's JSON text against the byte cap", () => {
    const log = appendCapped(EMPTY_GRPC_LOG, logEntries("c", [received("x".repeat(10))]), {
      maxEntries: 10,
      maxBytes: 100,
    });

    expect(log.retainedBytes).toBe(10);
  });
});

describe("systemText", () => {
  it("counts response metadata", () => {
    expect(systemText(metadata)).toBe("Response metadata: 1 entry");
    expect(systemText({ type: "responseMetadata", atMs: 1, metadata: [] })).toBe(
      "Response metadata: 0 entries",
    );
  });

  it("says who ended the call, with the status, its message and the time", () => {
    expect(systemText(ended("server", 0, "OK"))).toBe("Call ended, 0 OK (1.2 s)");
    expect(systemText(ended("client", 4, "DEADLINE_EXCEEDED", "deadline passed"))).toBe(
      "Call ended by the app, 4 DEADLINE_EXCEEDED: deadline passed (1.2 s)",
    );
  });
});

describe("filterLog", () => {
  const entries = logEntries("c", [
    sent('{"id":"Order-7"}'),
    metadata,
    received('{"total":12}'),
    ended("server", 0, "OK"),
  ]);

  it("filters by direction", () => {
    expect(filterLog(entries, { direction: "sent", query: "" })).toHaveLength(1);
    expect(filterLog(entries, { direction: "system", query: "" })).toHaveLength(2);
  });

  it("searches message text, informative lines and metadata, ignoring case", () => {
    expect(filterLog(entries, { direction: "all", query: "order-7" })).toHaveLength(1);
    expect(filterLog(entries, { direction: "all", query: "ABC-123" })).toHaveLength(1);
    expect(filterLog(entries, { direction: "all", query: "t-9" })).toHaveLength(1);
    expect(filterLog(entries, { direction: "all", query: "0 ok" })).toHaveLength(1);
    expect(filterLog(entries, { direction: "received", query: "order" })).toHaveLength(0);
  });
});

describe("applyPendingSends", () => {
  it("shows the message as typed when a secret was substituted into it", () => {
    const pending = [{ resolved: '{"token":"s3cret"}', display: '{"token":"{{token}}"}' }];

    const { events, rest } = applyPendingSends(pending, [sent('{"token": "s3cret"}')]);

    expect(events).toEqual([{ ...sent('{"token": "s3cret"}'), json: '{"token":"{{token}}"}' }]);
    expect(JSON.stringify(events)).not.toContain("s3cret");
    expect(rest).toEqual([]);
  });

  it("keeps Rust's text when nothing secret was in the message", () => {
    const pending = [{ resolved: '{"id": 1}', display: '{"id": 1}' }];

    const { events } = applyPendingSends(pending, [sent('{"id":1}')]);

    expect(events).toEqual([sent('{"id":1}')]);
  });

  it("pairs sends with pending entries in order and passes other events through", () => {
    const pending = [
      { resolved: "a", display: "A" },
      { resolved: "b", display: "B" },
      { resolved: "c", display: "C" },
    ];

    const { events, rest } = applyPendingSends(pending, [sent("a"), received("r"), sent("b")]);

    expect(events.map((event) => systemText(event))).toEqual(["A", "r", "B"]);
    expect(rest).toEqual([{ resolved: "c", display: "C" }]);
  });

  it("leaves a sent event alone when nothing is pending", () => {
    expect(applyPendingSends([], [sent("x")]).events).toEqual([sent("x")]);
  });
});

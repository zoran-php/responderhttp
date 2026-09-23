import { describe, expect, it } from "vitest";

import {
  appendCapped,
  clearAll,
  EMPTY_WS_LOG,
  filterLog,
  logEntries,
  systemText,
  takePendingSend,
  type WsLog,
} from "@/lib/ws-log";
import type { WsEvent } from "@/types/websocket";

const text = (type: "sent" | "received", value: string): WsEvent => ({
  type,
  atMs: 1,
  byteLength: value.length,
  payload: { kind: "text", text: value },
});

const binary = (hex: string): WsEvent => ({
  type: "received",
  atMs: 1,
  byteLength: hex.length / 2,
  payload: { kind: "binary", hex },
});

const connected: WsEvent = {
  type: "connected",
  atMs: 1,
  url: "wss://a.test/",
  status: 101,
  headers: [],
};

function logOf(connectionId: string, events: WsEvent[], log: WsLog = EMPTY_WS_LOG): WsLog {
  return appendCapped(log, logEntries(connectionId, events));
}

const texts = (log: WsLog) =>
  log.entries.map((entry) =>
    entry.event.type === "sent" || entry.event.type === "received"
      ? entry.event.payload.kind === "text"
        ? entry.event.payload.text
        : entry.event.payload.hex
      : entry.event.type,
  );

describe("appendCapped", () => {
  it("keeps entries oldest first with ids that are unique and stable", () => {
    const first = logOf("c1", [connected, text("sent", "a")]);
    const second = logOf("c1", [text("received", "a")], first);

    expect(texts(second)).toEqual(["connected", "a", "a"]);
    expect(second.entries[0]?.id).toBe(first.entries[0]?.id);
    expect(new Set(second.entries.map((entry) => entry.id)).size).toBe(3);
  });

  it("evicts the oldest entries past the entry cap and counts them", () => {
    const events = ["1", "2", "3", "4", "5"].map((value) => text("received", value));

    const log = appendCapped(EMPTY_WS_LOG, logEntries("c1", events), {
      maxEntries: 3,
      maxBytes: 1_000,
    });

    expect(texts(log)).toEqual(["3", "4", "5"]);
    expect(log.droppedCount).toBe(2);
  });

  it("evicts by retained bytes, but always keeps the newest entry", () => {
    const caps = { maxEntries: 100, maxBytes: 10 };
    const small = appendCapped(EMPTY_WS_LOG, logEntries("c1", [text("received", "12345")]), caps);

    const log = appendCapped(small, logEntries("c1", [text("received", "x".repeat(20))]), caps);

    expect(texts(log)).toEqual(["x".repeat(20)]);
    expect(log.retainedBytes).toBe(20);
    expect(log.droppedCount).toBe(1);
  });
});

describe("clearAll", () => {
  it("Clear Messages leaves nothing, whichever connection an entry came from", () => {
    expect(clearAll()).toEqual(EMPTY_WS_LOG);
  });
});

describe("filterLog", () => {
  const log = logOf("c1", [
    connected,
    text("sent", "Hello"),
    text("received", "hello back"),
    binary("deadbeef"),
    { type: "closed", atMs: 2, code: 1000, reason: "", by: "user" },
  ]);

  it("filters by direction, system meaning everything that is not a message", () => {
    const pick = (direction: "all" | "sent" | "received" | "system") =>
      texts({ ...log, entries: filterLog(log.entries, { direction, query: "" }) });

    expect(pick("sent")).toEqual(["Hello"]);
    expect(pick("received")).toEqual(["hello back", "deadbeef"]);
    expect(pick("system")).toEqual(["connected", "closed"]);
    expect(pick("all")).toHaveLength(5);
  });

  it("searches text payloads and system text without regard to case", () => {
    const found = filterLog(log.entries, { direction: "all", query: "HELLO" });
    const system = filterLog(log.entries, { direction: "all", query: "a.test" });

    expect(found).toHaveLength(2);
    expect(system.map((entry) => entry.event.type)).toEqual(["connected"]);
  });

  it("matches a binary payload on its Base64 when that is how binary is shown", () => {
    const found = filterLog(log.entries, {
      direction: "all",
      query: "3q2+",
      binaryEncoding: "base64",
    });
    const shownAsHex = filterLog(log.entries, { direction: "all", query: "3q2+" });

    expect(found.map((entry) => entry.event.type)).toEqual(["received"]);
    expect(shownAsHex).toEqual([]);
  });

  it("matches a binary payload on its hex, ignoring spaces in the query", () => {
    const found = filterLog(log.entries, { direction: "all", query: "AD BE" });

    expect(found.map((entry) => entry.event.type)).toEqual(["received"]);
  });
});

describe("systemText", () => {
  it("says who closed the connection, with the code and reason when there are any", () => {
    expect(systemText({ type: "closed", atMs: 1, code: 1000, reason: "", by: "user" })).toBe(
      "Disconnected (1000)",
    );
    expect(systemText({ type: "closed", atMs: 1, code: 1001, reason: "bye", by: "server" })).toBe(
      "Closed by the server (1001: bye)",
    );
    expect(
      systemText({ type: "closed", atMs: 1, code: null, reason: "dropped", by: "error" }),
    ).toBe("Connection closed (dropped)");
  });

  it("describes a reconnect attempt", () => {
    expect(
      systemText({ type: "reconnecting", atMs: 1, attempt: 2, maxAttempts: 10, delayMs: 2000 }),
    ).toBe("Reconnecting in 2 s (attempt 2 of 10)");
  });
});

describe("takePendingSend", () => {
  const secret = {
    resolved: { kind: "text" as const, text: "token=s3cret" },
    display: { kind: "text" as const, text: "token={{token}}" },
  };
  const plain = {
    resolved: { kind: "text" as const, text: "ping" },
    display: { kind: "text" as const, text: "ping" },
  };

  it("returns the display version of the message that was sent", () => {
    const { display, rest } = takePendingSend([secret, plain], secret.resolved);

    expect(display).toEqual(secret.display);
    expect(rest).toEqual([plain]);
  });

  it("discards messages before the match, which Rust dropped", () => {
    const { rest } = takePendingSend([secret, plain], plain.resolved);

    expect(rest).toEqual([]);
  });

  it("leaves an unmatched event as it came", () => {
    const stray = { kind: "text" as const, text: "unknown" };

    expect(takePendingSend([plain], stray)).toEqual({ display: stray, rest: [plain] });
  });
});

import { describe, expect, it } from "vitest";

import { quitSummary, quitWarning, type TabAtQuit } from "@/lib/quit-guard";

function tab(kind: TabAtQuit["kind"], flags: Partial<TabAtQuit> = {}): TabAtQuit {
  return { kind, dirty: false, running: false, ...flags };
}

describe("quitSummary", () => {
  it("counts unsaved tabs and each kind of running work", () => {
    const summary = quitSummary([
      tab("request", { dirty: true }),
      tab("request", { running: true }),
      tab("websocket", { dirty: true, running: true }),
      tab("grpc", { running: true }),
      tab("environment"),
    ]);

    expect(summary).toEqual({
      unsavedTabs: 2,
      runningRequests: 1,
      openConnections: 1,
      runningCalls: 1,
    });
  });

  it("never counts a docs tab, which autosaves and is flushed on quit", () => {
    expect(quitSummary([tab("docs", { dirty: true })]).unsavedTabs).toBe(0);
  });
});

describe("quitWarning", () => {
  it("says nothing when quitting loses nothing", () => {
    expect(quitWarning(quitSummary([tab("request"), tab("example")]))).toBeNull();
  });

  it("names a single loss in the singular", () => {
    expect(quitWarning(quitSummary([tab("request", { dirty: true })]))).toBe(
      "Quitting ResponderHTTP loses the unsaved changes in 1 tab.",
    );
  });

  it("joins several losses with commas and a final and", () => {
    const warning = quitWarning({
      unsavedTabs: 2,
      runningRequests: 1,
      openConnections: 3,
      runningCalls: 1,
    });

    expect(warning).toBe(
      "Quitting ResponderHTTP loses the unsaved changes in 2 tabs, stops 1 running request, " +
        "disconnects 3 WebSocket connections and cancels 1 gRPC call.",
    );
  });

  it("joins two losses with and alone", () => {
    const warning = quitWarning({
      unsavedTabs: 0,
      runningRequests: 0,
      openConnections: 1,
      runningCalls: 2,
    });

    expect(warning).toBe(
      "Quitting ResponderHTTP disconnects 1 WebSocket connection and cancels 2 gRPC calls.",
    );
  });
});

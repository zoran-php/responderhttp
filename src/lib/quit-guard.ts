// http_client/src/lib/quit-guard.ts
//
// What quitting would throw away, and the sentence that says so
// (PLAN-LINUX.md 17b-2). On a Linux desktop with no tray, closing the window
// quits the app, and open tabs live only in memory. So the close button asks
// first, by the same rule a single tab uses when it is closed (App.tsx): an
// unsaved tab, or anything still running. Docs tabs autosave, and the quit
// path flushes them, so they never count.
//
// Pure: it takes each tab's kind and two flags, never the store.

export interface TabAtQuit {
  kind: "request" | "environment" | "docs" | "example" | "websocket" | "grpc";
  /** Has changes that are not saved anywhere. */
  dirty: boolean;
  /** A request sending or streaming, a WebSocket not idle, a gRPC call running. */
  running: boolean;
}

export interface QuitSummary {
  unsavedTabs: number;
  runningRequests: number;
  openConnections: number;
  runningCalls: number;
}

export function quitSummary(tabs: readonly TabAtQuit[]): QuitSummary {
  const count = (test: (tab: TabAtQuit) => boolean) => tabs.filter(test).length;
  return {
    unsavedTabs: count((tab) => tab.dirty && tab.kind !== "docs"),
    runningRequests: count((tab) => tab.running && tab.kind === "request"),
    openConnections: count((tab) => tab.running && tab.kind === "websocket"),
    runningCalls: count((tab) => tab.running && tab.kind === "grpc"),
  };
}

/** The confirmation text, or null when quitting loses nothing. */
export function quitWarning(summary: QuitSummary): string | null {
  const clauses = [
    phrase(summary.unsavedTabs, "loses the unsaved changes in", "tab"),
    phrase(summary.runningRequests, "stops", "running request"),
    phrase(summary.openConnections, "disconnects", "WebSocket connection"),
    phrase(summary.runningCalls, "cancels", "gRPC call"),
  ].filter((clause): clause is string => clause !== null);
  if (clauses.length === 0) {
    return null;
  }
  return `Quitting ResponderHTTP ${joinWithAnd(clauses)}.`;
}

function phrase(count: number, verb: string, noun: string): string | null {
  if (count === 0) {
    return null;
  }
  return `${verb} ${count} ${noun}${count === 1 ? "" : "s"}`;
}

function joinWithAnd(parts: readonly string[]): string {
  if (parts.length <= 1) {
    return parts.join("");
  }
  return `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

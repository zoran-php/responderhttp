// http_client/src/lib/history-filter.test.ts
import { describe, expect, it } from "vitest";

import { filterHistory } from "@/lib/history-filter";
import type { HistoryEntry } from "@/types/history";
import { AUTH_NONE, DEFAULT_SETTINGS, type HttpMethod } from "@/types/http";

function entry(
  id: string,
  method: HttpMethod,
  resolvedUrl: string,
  status: number | null,
  errorKind: string | null = null,
): HistoryEntry {
  return {
    id,
    sentAt: "2026-09-14T08:00:00Z",
    resolvedUrl,
    status,
    errorKind,
    durationMs: 10,
    request: {
      method,
      url: resolvedUrl,
      headers: [{ name: "Authorization", value: "Bearer sekrit" }],
      queryParams: [],
      body: { kind: "none" },
      auth: AUTH_NONE,
      settings: DEFAULT_SETTINGS,
    },
  };
}

const entries: HistoryEntry[] = [
  entry("1", "GET", "https://api.example.com/users", 200),
  entry("2", "POST", "https://api.example.com/users", 500),
  entry("3", "GET", "https://other.test/health", null, "transport"),
];

describe("filterHistory", () => {
  it("returns everything for an empty or whitespace-only query", () => {
    expect(filterHistory(entries, "")).toHaveLength(3);
    expect(filterHistory(entries, "   ")).toHaveLength(3);
  });

  it("matches the method case-insensitively", () => {
    expect(filterHistory(entries, "post").map((found) => found.id)).toEqual(["2"]);
  });

  it("requires every term to match, so terms narrow rather than widen", () => {
    expect(filterHistory(entries, "users 500").map((found) => found.id)).toEqual(["2"]);
    expect(filterHistory(entries, "users health")).toEqual([]);
  });

  it("matches a status by its digits", () => {
    expect(filterHistory(entries, "200").map((found) => found.id)).toEqual(["1"]);
  });

  it("matches a failed send by its error kind, since it has no status", () => {
    expect(filterHistory(entries, "transport").map((found) => found.id)).toEqual(["3"]);
  });

  /** Searching must not be a way to surface credentials in a results list. */
  it("does not search headers or the body", () => {
    expect(filterHistory(entries, "sekrit")).toEqual([]);
  });
});

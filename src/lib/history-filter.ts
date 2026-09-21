// http_client/src/lib/history-filter.ts
//
// Searching history is a pure function over the capped list rather than a
// SQL query: at 500 rows the difference is imperceptible, and this is
// testable without a database (CLAUDE.md section 8).
import type { HistoryEntry } from "@/types/history";

/**
 * Case-insensitive, whitespace-separated AND: every term must appear
 * somewhere in the entry. That makes `post users 500` a useful query without
 * the user learning any syntax.
 *
 * Matched against method, the resolved URL and the status — not headers or
 * the body, which would put tokens and credentials one keystroke away from
 * being displayed in a search-results list.
 */
export function filterHistory(entries: HistoryEntry[], query: string): HistoryEntry[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) {
    return entries;
  }
  return entries.filter((entry) => {
    const haystack = searchableText(entry);
    return terms.every((term) => haystack.includes(term));
  });
}

function searchableText(entry: HistoryEntry): string {
  const status = entry.status === null ? (entry.errorKind ?? "") : String(entry.status);
  return `${entry.request.method} ${entry.resolvedUrl} ${status}`.toLowerCase();
}

// http_client/src/services/history.ts
//
// history commands. The boundary crossing lives in services/invoke.ts
// (CLAUDE.md section 11, rule 3).
import { call } from "@/services/invoke";
import type { HistoryEntry, RecordHistoryInput } from "@/types/history";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

export function listHistory(): Promise<Result<HistoryEntry[], ApiError>> {
  return call<HistoryEntry[]>("list_history");
}

/** Returns the stored entry, id and timestamp assigned by storage. */
export function recordHistory(entry: RecordHistoryInput): Promise<Result<HistoryEntry, ApiError>> {
  return call<HistoryEntry>("record_history", { entry });
}

export function deleteHistoryEntry(id: string): Promise<Result<void, ApiError>> {
  return call<void>("delete_history_entry", { id });
}

export function clearHistory(): Promise<Result<void, ApiError>> {
  return call<void>("clear_history");
}

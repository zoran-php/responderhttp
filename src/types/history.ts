// http_client/src/types/history.ts
//
// Mirrors HistoryEntryDto and RecordHistoryInput in
// src-tauri/src/commands/dto.rs.
import type { SendRequestInput } from "@/types/http";

/**
 * One sent request. `request` is the template as it was typed —
 * {{placeholders}} intact — so re-running resolves against whichever
 * environment is active then. `resolvedUrl` is what actually went over the
 * wire, kept for reading and searching.
 */
export interface HistoryEntry {
  id: string;
  /** Seconds-resolution UTC, e.g. 2026-09-14T08:31:05Z. */
  sentAt: string;
  resolvedUrl: string;
  /** Null when the request never produced a response. */
  status: number | null;
  /** Null when it did. Otherwise the ApiError kind that ended it. */
  errorKind: string | null;
  durationMs: number;
  request: SendRequestInput;
}

/** What the frontend reports once a send settles, either way. */
export interface RecordHistoryInput {
  resolvedUrl: string;
  status: number | null;
  errorKind: string | null;
  durationMs: number;
  request: SendRequestInput;
}

/**
 * Mirrors HISTORY_LIMIT in src-tauri/src/domain/services/history.rs. Storage
 * is the authority; this only keeps the in-memory list from growing past
 * what a reload would return.
 */
export const HISTORY_LIMIT = 500;

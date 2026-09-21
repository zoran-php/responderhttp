// http_client/src/store/history-store.ts
//
// Data and persistence actions for sent-request history. Searching is a
// pure function the panel applies to `entries` (lib/history-filter.ts), so
// no query state lives here.
import { create } from "zustand";

import * as historyService from "@/services/history";
import { HISTORY_LIMIT, type HistoryEntry, type RecordHistoryInput } from "@/types/history";
import type { ApiError } from "@/types/http";

interface HistoryState {
  entries: HistoryEntry[];
  error: ApiError | null;

  loadHistory: () => Promise<void>;
  /** Called once a send settles, success or failure. */
  recordEntry: (entry: RecordHistoryInput) => Promise<void>;
  deleteEntry: (id: string) => Promise<boolean>;
  clearHistory: () => Promise<boolean>;
  clearError: () => void;
}

export const useHistoryStore = create<HistoryState>((set) => ({
  entries: [],
  error: null,

  loadHistory: async () => {
    const result = await historyService.listHistory();
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    set({ entries: result.value, error: null });
  },

  recordEntry: async (entry) => {
    const result = await historyService.recordHistory(entry);
    if (!result.ok) {
      // A history write that fails must not look like a failed request:
      // the send already happened and its response is on screen. Surfaced
      // in the panel's banner, nowhere near the response.
      set({ error: result.error });
      return;
    }
    // Prepended rather than re-read: the list is already newest-first, and
    // a round trip per send would be wasted work.
    set((state) => ({
      entries: [result.value, ...state.entries].slice(0, HISTORY_LIMIT),
      error: null,
    }));
  },

  deleteEntry: async (id) => {
    const result = await historyService.deleteHistoryEntry(id);
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    set((state) => ({
      entries: state.entries.filter((entry) => entry.id !== id),
      error: null,
    }));
    return true;
  },

  clearHistory: async () => {
    const result = await historyService.clearHistory();
    if (!result.ok) {
      set({ error: result.error });
      return false;
    }
    set({ entries: [], error: null });
    return true;
  },

  clearError: () => set({ error: null }),
}));

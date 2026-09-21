// http_client/src/features/history/HistoryPanel.tsx
//
// The History tab of the sidebar. Clicking an entry opens its request in a
// new tab, where it can be re-sent or saved into a collection with the
// existing Save button — re-running is not a separate code path.
//
// Deliberately shows only method, URL, status and time: headers, auth and
// body are in the entry but belong on the builder, not in a list that is on
// screen while someone shares a window (CLAUDE.md section 11, rule 6).
import { useEffect, useMemo, useState } from "react";
import { Search, Trash2 } from "lucide-react";

import { ConfirmDialog } from "@/components/ConfirmDialog";
import { formatDuration } from "@/lib/format";
import { filterHistory } from "@/lib/history-filter";
import { methodTextColor } from "@/lib/http-method-colors";
import { statusTextColor } from "@/lib/status-colors";
import { useHistoryStore } from "@/store/history-store";
import type { HistoryEntry } from "@/types/history";
import type { SendRequestInput } from "@/types/http";

interface HistoryPanelProps {
  onOpenEntry: (request: SendRequestInput) => void;
}

export function HistoryPanel({ onOpenEntry }: HistoryPanelProps) {
  const entries = useHistoryStore((state) => state.entries);
  const error = useHistoryStore((state) => state.error);
  // Selected one by one rather than off the store object, which would be a
  // new reference every render and re-fire the effect below.
  const loadHistory = useHistoryStore((state) => state.loadHistory);
  const deleteEntry = useHistoryStore((state) => state.deleteEntry);
  const clearHistory = useHistoryStore((state) => state.clearHistory);

  const [query, setQuery] = useState("");
  const [confirmingClear, setConfirmingClear] = useState(false);

  // Loaded when the panel first mounts rather than at startup: nothing else
  // in the app reads history, and it is the one list that grows unbounded
  // between reads.
  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  const visible = useMemo(() => filterHistory(entries, query), [entries, query]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1 border-b border-border px-2 py-2">
        <div className="relative flex-1">
          <Search
            aria-hidden
            className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
          />
          <input
            aria-label="Search history"
            className="w-full rounded border border-border bg-input py-1 pl-7 pr-2 text-sm placeholder:text-muted-foreground"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search"
            value={query}
          />
        </div>
        <button
          aria-label="Clear history"
          className="shrink-0 rounded p-1 text-muted-foreground hover:bg-accent hover:text-destructive disabled:hover:bg-transparent disabled:hover:text-muted-foreground"
          disabled={entries.length === 0}
          onClick={() => setConfirmingClear(true)}
          type="button"
        >
          <Trash2 aria-hidden className="h-4 w-4" />
        </button>
      </div>

      {error && (
        <p className="border-b border-border px-3 py-2 text-xs text-destructive">{error.message}</p>
      )}

      <div className="min-h-0 flex-1 overflow-auto p-1">
        {entries.length === 0 ? (
          <p className="px-2 py-3 text-sm text-muted-foreground">
            Nothing sent yet. Every request you send shows up here.
          </p>
        ) : (
          visible.length === 0 && (
            <p className="px-2 py-3 text-sm text-muted-foreground">No match for “{query}”.</p>
          )
        )}

        {visible.map((entry) => (
          <div
            className="group flex items-center gap-2 rounded px-1 py-1 text-sm hover:bg-accent"
            key={entry.id}
          >
            <button
              className="flex min-w-0 flex-1 flex-col items-start text-left"
              onClick={() => onOpenEntry(entry.request)}
              type="button"
              title={entry.resolvedUrl}
            >
              <span className="flex w-full min-w-0 items-baseline gap-1.5">
                <span
                  className={`shrink-0 font-mono text-xs font-semibold ${methodTextColor(entry.request.method)}`}
                >
                  {entry.request.method}
                </span>
                <span className="min-w-0 flex-1 truncate">{entry.resolvedUrl}</span>
              </span>
              <span className="flex gap-2 text-xs text-muted-foreground">
                <span className={outcomeColor(entry)}>{outcomeLabel(entry)}</span>
                <span>{formatDuration(entry.durationMs)}</span>
                <span>{entry.sentAt.replace("T", " ").replace("Z", "")}</span>
              </span>
            </button>
            <button
              aria-label={`Remove ${entry.request.method} ${entry.resolvedUrl} from history`}
              className="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 hover:text-destructive group-hover:opacity-100"
              onClick={() => void deleteEntry(entry.id)}
              type="button"
            >
              <Trash2 aria-hidden className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </div>

      {confirmingClear && (
        <ConfirmDialog
          confirmLabel="Clear"
          message="Every entry will be removed. This cannot be undone."
          onCancel={() => setConfirmingClear(false)}
          onConfirm={() => {
            void clearHistory();
            setConfirmingClear(false);
          }}
          title="Clear history"
        />
      )}
    </div>
  );
}

function outcomeLabel(entry: HistoryEntry): string {
  return entry.status === null ? (entry.errorKind ?? "failed") : String(entry.status);
}

function outcomeColor(entry: HistoryEntry): string {
  return entry.status === null ? "text-destructive" : statusTextColor(entry.status);
}

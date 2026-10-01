// http_client/src/components/MessageStreamLog.tsx
//
// The message log both streaming protocols show: a header with a status
// badge, search, a direction filter, Clear Messages, and the entries newest
// first, each expandable (PLAN-GRPC.md 16i). Extracted from WebSocketLog so
// the gRPC view does not keep a second copy (CLAUDE.md section 6).
//
// It knows nothing about either protocol. The caller filters the entries
// (lib/ws-log.ts, lib/grpc-log.ts) and says how one looks: its icon, its
// one-line summary and its expanded detail. Which rows are expanded is the
// only state here.
//
// A plain list, no virtualisation: both logs are capped at 1,000 entries,
// and a collapsed row is one line of text.
import { memo, useCallback, useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight, Copy, Search, Trash2 } from "lucide-react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { formatBytes, formatClockTime } from "@/lib/format";
import { prettyPrint } from "@/lib/pretty-print";

export interface StreamLogEntry {
  /** Stable for the entry's life: the React key. */
  id: string;
}

/** How the caller's entries look. Keep the functions stable (module level
 * or memoised): a row re-renders when they change. */
export interface StreamLogRenderers<E extends StreamLogEntry> {
  icon: (entry: E) => ReactNode;
  summary: (entry: E) => string;
  detail: (entry: E) => ReactNode;
  atMs: (entry: E) => number;
}

export interface StreamLogBadge {
  label: string;
  /** Tailwind classes for the badge's colours. */
  className: string;
}

interface MessageStreamLogProps<E extends StreamLogEntry, F extends string> {
  /** Already filtered and searched, newest first. */
  rows: readonly E[];
  /** Everything the log holds, before the filter: picks the empty text. */
  totalCount: number;
  droppedCount: number;
  badge: StreamLogBadge;
  /** Shown when the log holds nothing at all. */
  emptyText: string;
  filter: F;
  filterOptions: readonly { value: F; label: string }[];
  query: string;
  renderers: StreamLogRenderers<E>;
  onFilterChange: (filter: F) => void;
  onQueryChange: (query: string) => void;
  onClearMessages: () => void;
}

export function MessageStreamLog<E extends StreamLogEntry, F extends string>(
  props: MessageStreamLogProps<E, F>,
) {
  const { rows, renderers } = props;
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  // Stable, so a memoised row does not re-render because its callback did.
  const toggle = useCallback((id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-sm">
        <span className="font-medium">Response</span>
        <span
          className={`ml-auto rounded px-2 py-0.5 text-xs font-medium ${props.badge.className}`}
          role="status"
        >
          {props.badge.label}
        </span>
      </div>

      <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-sm">
        <div className="relative min-w-0 flex-1">
          <Search
            aria-hidden
            className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
          />
          <input
            aria-label="Search messages"
            className="h-8 w-full rounded-md border border-input bg-background pl-7 pr-2 text-sm"
            onChange={(event) => props.onQueryChange(event.target.value)}
            placeholder="Search"
            spellCheck={false}
            value={props.query}
          />
        </div>
        <select
          aria-label="Filter messages"
          className="h-8 rounded-md border border-input bg-background px-2"
          onChange={(event) => {
            const chosen = props.filterOptions.find(
              (option) => option.value === event.target.value,
            );
            if (chosen !== undefined) {
              props.onFilterChange(chosen.value);
            }
          }}
          value={props.filter}
        >
          {props.filterOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <button
          className="inline-flex h-8 items-center gap-1.5 rounded-md border border-input px-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={props.onClearMessages}
          type="button"
        >
          <Trash2 aria-hidden className="h-3.5 w-3.5" />
          Clear Messages
        </button>
      </div>

      <ol aria-label="Messages" className="min-h-0 flex-1 overflow-auto">
        {rows.map((entry) => (
          <LogRow
            entry={entry}
            expanded={expanded.has(entry.id)}
            key={entry.id}
            onToggle={toggle}
            renderers={renderers}
          />
        ))}
        {rows.length === 0 && (
          <li className="px-4 py-6 text-center text-xs text-muted-foreground">
            {props.totalCount === 0 ? props.emptyText : "No messages match the search and filter."}
          </li>
        )}
        {props.droppedCount > 0 && (
          <li className="px-4 py-2 text-center text-xs text-muted-foreground">
            {props.droppedCount} earlier {props.droppedCount === 1 ? "entry" : "entries"} dropped
          </li>
        )}
      </ol>
    </div>
  );
}

interface LogRowProps<E extends StreamLogEntry> {
  entry: E;
  expanded: boolean;
  renderers: StreamLogRenderers<E>;
  onToggle: (id: string) => void;
}

function LogRowInner<E extends StreamLogEntry>({
  entry,
  expanded,
  renderers,
  onToggle,
}: LogRowProps<E>) {
  return (
    <li className="border-b border-border/60">
      <button
        aria-expanded={expanded}
        className="flex w-full items-center gap-2 px-4 py-1.5 text-left hover:bg-accent/50"
        onClick={() => onToggle(entry.id)}
        type="button"
      >
        {renderers.icon(entry)}
        <span className="min-w-0 flex-1 truncate font-mono text-xs">
          {renderers.summary(entry)}
        </span>
        <time className="shrink-0 font-mono text-xs text-muted-foreground">
          {formatClockTime(renderers.atMs(entry))}
        </time>
        {expanded ? (
          <ChevronDown aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )}
      </button>
      {expanded && renderers.detail(entry)}
    </li>
  );
}

/**
 * Memoised: every batch that arrives re-renders the log, and an entry never
 * changes once it exists, so only new rows and the one being toggled need
 * rendering (PLAN-WEBSOCKET.md 13h). `memo` drops the type parameter, hence
 * the cast back.
 */
const LogRow = memo(LogRowInner) as typeof LogRowInner;

/** An expanded text message: its size, Copy, and the text in a read-only
 * editor, pretty-printed when it is JSON. Shared by both logs. */
export function TextMessageDetail({
  text,
  byteLength,
  isJson,
}: {
  text: string;
  byteLength: number;
  isJson: boolean;
}) {
  return (
    <div className="px-4 pb-3 pl-10">
      <div className="mb-1 flex items-center gap-3 text-xs text-muted-foreground">
        <span>{formatBytes(byteLength)}</span>
        <CopyButton text={text} />
      </div>
      <div className="h-48 overflow-hidden rounded border border-border">
        <LazyCodeEditor
          language={isJson ? "json" : "plaintext"}
          readOnly
          value={isJson ? prettyPrint(text, "json") : text}
        />
      </div>
    </div>
  );
}

export function CopyButton({ text }: { text: string }) {
  return (
    <button
      className="ml-auto inline-flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-accent hover:text-foreground"
      onClick={() => void navigator.clipboard.writeText(text)}
      type="button"
    >
      <Copy aria-hidden className="h-3 w-3" />
      Copy
    </button>
  );
}

/** A table of name/value pairs: handshake headers, gRPC metadata, trailers.
 * Names can repeat, and the list never changes once shown, so the index is
 * part of the key. */
export function PairsTable({ pairs }: { pairs: readonly { name: string; value: string }[] }) {
  return (
    <table className="w-full">
      <tbody>
        {pairs.map((pair, index) => (
          <tr key={`${index}-${pair.name}`}>
            <td className="w-1/3 py-0.5 pr-3 align-top font-mono text-muted-foreground">
              {pair.name}
            </td>
            <td className="break-all py-0.5 font-mono">{pair.value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

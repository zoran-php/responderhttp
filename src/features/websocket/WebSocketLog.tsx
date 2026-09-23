// http_client/src/features/websocket/WebSocketLog.tsx
//
// The bottom half of a WebSocket tab: the status badge, search, the
// direction filter, Clear Messages, and the log itself, newest first. What
// is shown, kept and cleared is decided in lib/ws-log.ts; which rows are
// expanded is the only state here, because nothing else needs it.
//
// A plain list, no virtualisation: the log is capped at 1,000 entries
// (lib/ws-log.ts), and a collapsed row is one line of text.
import { memo, useCallback, useMemo, useState } from "react";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Copy,
  Info,
  Search,
  Trash2,
} from "lucide-react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { formatBytes, formatClockTime } from "@/lib/format";
import { prettyPrint } from "@/lib/pretty-print";
import { filterLog, systemText, type WsLog, type WsLogEntry, type WsLogFilter } from "@/lib/ws-log";
import { hexDump, hexToBase64, jsonWarning, payloadPreview } from "@/lib/ws-payload";
import {
  connectionBadge,
  LOG_FILTER_OPTIONS,
  type WsBadgeTone,
  type WsConnectionState,
} from "@/lib/ws-status";
import type { WsBinaryEncoding, WsEvent, WsPayload } from "@/types/websocket";

const BADGE_CLASS: Record<WsBadgeTone, string> = {
  disconnected: "bg-destructive/15 text-destructive",
  pending: "bg-muted text-muted-foreground",
  connected: "bg-ws-ok/15 text-ws-ok",
};

interface WebSocketLogProps {
  log: WsLog;
  filter: WsLogFilter;
  query: string;
  connection: WsConnectionState;
  reconnectAttempt: { attempt: number; maxAttempts: number } | null;
  /** How an expanded binary message is first shown: the composer's own
   * encoding. Each row can switch its own. */
  binaryEncoding: WsBinaryEncoding;
  onFilterChange: (filter: WsLogFilter) => void;
  onQueryChange: (query: string) => void;
  onClearMessages: () => void;
}

export function WebSocketLog(props: WebSocketLogProps) {
  const { log, filter, query } = props;
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const badge = connectionBadge(props.connection, props.reconnectAttempt);

  // Newest first, as the reference screenshots show.
  const rows = useMemo(
    () =>
      filterLog(log.entries, {
        direction: filter,
        query,
        binaryEncoding: props.binaryEncoding,
      }).reverse(),
    [log.entries, filter, query, props.binaryEncoding],
  );

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
          className={`ml-auto rounded px-2 py-0.5 text-xs font-medium ${BADGE_CLASS[badge.tone]}`}
          role="status"
        >
          {badge.label}
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
            value={query}
          />
        </div>
        <select
          aria-label="Filter messages"
          className="h-8 rounded-md border border-input bg-background px-2"
          onChange={(event) => props.onFilterChange(event.target.value as WsLogFilter)}
          value={filter}
        >
          {LOG_FILTER_OPTIONS.map((option) => (
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
            binaryEncoding={props.binaryEncoding}
            entry={entry}
            expanded={expanded.has(entry.id)}
            key={entry.id}
            onToggle={toggle}
          />
        ))}
        {rows.length === 0 && (
          <li className="px-4 py-6 text-center text-xs text-muted-foreground">
            {log.entries.length === 0
              ? "Connect to see messages here."
              : "No messages match the search and filter."}
          </li>
        )}
        {log.droppedCount > 0 && (
          <li className="px-4 py-2 text-center text-xs text-muted-foreground">
            {log.droppedCount} earlier {log.droppedCount === 1 ? "entry" : "entries"} dropped
          </li>
        )}
      </ol>
    </div>
  );
}

interface LogRowProps {
  entry: WsLogEntry;
  expanded: boolean;
  binaryEncoding: WsBinaryEncoding;
  onToggle: (id: string) => void;
}

/**
 * Memoised: every batch that arrives re-renders the log, and an entry never
 * changes once it exists, so only new rows and the one being toggled need
 * rendering (PLAN-WEBSOCKET.md 13h).
 */
const LogRow = memo(function LogRow({ entry, expanded, binaryEncoding, onToggle }: LogRowProps) {
  const { event } = entry;
  const summary =
    event.type === "sent" || event.type === "received"
      ? payloadPreview(event.payload, binaryEncoding)
      : systemText(event);

  return (
    <li className="border-b border-border/60">
      <button
        aria-expanded={expanded}
        className="flex w-full items-center gap-2 px-4 py-1.5 text-left hover:bg-accent/50"
        onClick={() => onToggle(entry.id)}
        type="button"
      >
        <EventIcon event={event} />
        <span className="min-w-0 flex-1 truncate font-mono text-xs">{summary}</span>
        <time className="shrink-0 font-mono text-xs text-muted-foreground">
          {formatClockTime(event.atMs)}
        </time>
        {expanded ? (
          <ChevronDown aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )}
      </button>
      {expanded && <EventDetail binaryEncoding={binaryEncoding} event={event} />}
    </li>
  );
});

function EventIcon({ event }: { event: WsEvent }) {
  switch (event.type) {
    case "sent":
      return <ArrowUp aria-label="Sent" className="h-3.5 w-3.5 shrink-0 text-ws-sent" />;
    case "received":
      return <ArrowDown aria-label="Received" className="h-3.5 w-3.5 shrink-0 text-ws-received" />;
    case "connected":
      return <CheckCircle2 aria-label="Connected" className="h-3.5 w-3.5 shrink-0 text-ws-ok" />;
    case "error":
      return <AlertCircle aria-label="Error" className="h-3.5 w-3.5 shrink-0 text-destructive" />;
    case "closed":
    case "reconnecting":
      return <Info aria-label="Event" className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />;
  }
}

function EventDetail({
  event,
  binaryEncoding,
}: {
  event: WsEvent;
  binaryEncoding: WsBinaryEncoding;
}) {
  if (event.type === "sent" || event.type === "received") {
    return (
      <PayloadDetail
        byteLength={event.byteLength}
        defaultEncoding={binaryEncoding}
        payload={event.payload}
      />
    );
  }
  if (event.type === "connected") {
    return (
      <div className="space-y-2 px-4 pb-3 pl-10 text-xs">
        <p>
          <span className="text-muted-foreground">Status </span>
          <span className="font-mono">{event.status} Switching Protocols</span>
        </p>
        <table className="w-full">
          <tbody>
            {event.headers.map((header, index) => (
              // Headers can repeat, so the name alone is not a key; the list
              // never changes once the event exists, so the index is stable.
              <tr key={`${index}-${header.name}`}>
                <td className="w-1/3 py-0.5 pr-3 align-top font-mono text-muted-foreground">
                  {header.name}
                </td>
                <td className="break-all py-0.5 font-mono">{header.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }
  return <p className="px-4 pb-3 pl-10 font-mono text-xs">{systemText(event)}</p>;
}

interface PayloadDetailProps {
  payload: WsPayload;
  byteLength: number;
  /** Read once, when the row is expanded. */
  defaultEncoding: WsBinaryEncoding;
}

function PayloadDetail({ payload, byteLength, defaultEncoding }: PayloadDetailProps) {
  const [encoding, setEncoding] = useState<WsBinaryEncoding>(defaultEncoding);
  const isJson =
    payload.kind === "text" && payload.text.trim() !== "" && jsonWarning(payload.text) === null;

  if (payload.kind === "binary") {
    // Copy takes the message in the encoding on screen, never the dump's
    // offsets and ASCII column.
    const base64 = encoding === "base64" ? hexToBase64(payload.hex) : null;
    return (
      <div className="px-4 pb-3 pl-10">
        <div className="mb-1 flex items-center gap-3 text-xs text-muted-foreground">
          <span>{formatBytes(byteLength)}</span>
          <span>Binary</span>
          <select
            aria-label="Show binary as"
            className="h-6 rounded border border-input bg-background px-1 text-xs"
            onChange={(event) => setEncoding(event.target.value as WsBinaryEncoding)}
            value={encoding}
          >
            <option value="base64">Base64</option>
            <option value="hex">Hexadecimal</option>
          </select>
          <CopyButton text={base64 ?? payload.hex} />
        </div>
        <pre
          className={`overflow-auto rounded border border-border bg-background p-2 font-mono text-xs ${
            base64 === null ? "" : "whitespace-pre-wrap break-all"
          }`}
        >
          {base64 ?? hexDump(payload.hex)}
        </pre>
      </div>
    );
  }

  return (
    <div className="px-4 pb-3 pl-10">
      <div className="mb-1 flex items-center gap-3 text-xs text-muted-foreground">
        <span>{formatBytes(byteLength)}</span>
        <CopyButton text={payload.text} />
      </div>
      <div className="h-48 overflow-hidden rounded border border-border">
        <LazyCodeEditor
          language={isJson ? "json" : "plaintext"}
          readOnly
          value={isJson ? prettyPrint(payload.text, "json") : payload.text}
        />
      </div>
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
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

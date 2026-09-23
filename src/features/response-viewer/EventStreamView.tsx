// http_client/src/features/response-viewer/EventStreamView.tsx
//
// A `text/event-stream` response, shown as it arrives (PLAN-SSE.md, 14d).
// Presentational: the parsing happened in Rust and the folding in
// lib/sse-log.ts; this renders what the store holds.
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { Radio } from "lucide-react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { SizeBadge } from "@/features/response-viewer/SizeBadge";
import {
  SaveExampleButton,
  type SaveExampleAction,
} from "@/features/response-viewer/SaveExampleButton";
import { formatClockTime, formatDuration } from "@/lib/format";
import { blockLabel, rawText, type ResponseStream, type SseLogEntry } from "@/lib/sse-log";
import { statusPillClass } from "@/lib/status-colors";
import { sizesOfResponse, sizesOfStream } from "@/lib/transfer-sizes";
import type { ApiError, HttpResponse, KeyValue } from "@/types/http";

type View = "Events" | "Raw" | "Headers";

/** Longer than this and a row is truncated: one event should never be able
 * to push the rest of the stream off the screen. */
const MAX_SHOWN_CHARS = 4000;

/** How close to the top still counts as following the stream. */
const STICK_THRESHOLD_PX = 40;

export interface EventStreamViewProps {
  stream: ResponseStream;
  /** Null until the request finishes — which, for a stream, may be never. */
  response: HttpResponse | null;
  error: ApiError | null;
  isSending: boolean;
  saveExample: SaveExampleAction;
}

export function EventStreamView({
  stream,
  response,
  error,
  isSending,
  saveExample,
}: EventStreamViewProps) {
  const [view, setView] = useState<View>("Events");
  const { log, head } = stream;
  const status = response?.status ?? head?.status;
  const headers: KeyValue[] = response?.headers ?? head?.headers ?? [];
  const total = log.droppedCount + log.entries.length;
  // The response's own numbers once it lands: they are libcurl's, and they
  // include anything the caps dropped.
  const sizes = response === null ? sizesOfStream(stream) : sizesOfResponse(response.sizes);

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-3 border-b border-border px-4 py-2 text-sm">
        {status !== undefined && (
          <span className={`rounded px-2 py-0.5 font-medium ${statusPillClass(status)}`}>
            {status}
          </span>
        )}
        {isSending ? (
          <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Radio aria-hidden className="h-3.5 w-3.5 animate-pulse text-primary" />
            Streaming
          </span>
        ) : (
          response !== null && (
            <span className="text-muted-foreground">{formatDuration(response.timing.totalMs)}</span>
          )
        )}
        <span className="text-xs text-muted-foreground">
          {total} {total === 1 ? "event" : "events"}
        </span>
        <SizeBadge sizes={sizes} />

        <div className="ml-auto flex items-center gap-1">
          <SaveExampleButton action={saveExample} />
          {(["Events", "Raw", "Headers"] as const).map((name) => (
            <button
              className={`rounded px-2 py-1 text-xs ${
                view === name ? "bg-accent text-foreground" : "text-muted-foreground"
              }`}
              key={name}
              onClick={() => setView(name)}
              type="button"
            >
              {name}
            </button>
          ))}
        </div>
      </div>

      {error !== null && (
        <p className="border-b border-border px-4 py-2 text-sm text-destructive">{error.message}</p>
      )}

      {view === "Headers" ? (
        <HeaderTable headers={headers} />
      ) : view === "Raw" ? (
        <RawView isSending={isSending} response={response} stream={stream} />
      ) : (
        <EventList isSending={isSending} stream={stream} />
      )}
    </div>
  );
}

function EventList({ stream, isSending }: { stream: ResponseStream; isSending: boolean }) {
  const { entries, droppedCount } = stream.log;
  const listRef = useRef<HTMLOListElement>(null);
  // Follow the newest event — which is at the top — unless the user has
  // scrolled down to read the older ones.
  const following = useRef(true);

  // Newest first, as the WebSocket log shows its messages. Copied before
  // reversing: the store's array is shared state, not this view's to turn
  // around.
  const rows = useMemo(() => entries.slice().reverse(), [entries]);

  useEffect(() => {
    const list = listRef.current;
    if (list !== null && following.current) {
      list.scrollTop = 0;
    }
  }, [entries.length]);

  return (
    <ol
      className="flex-1 overflow-auto"
      onScroll={(scroll) => {
        following.current = scroll.currentTarget.scrollTop < STICK_THRESHOLD_PX;
      }}
      ref={listRef}
    >
      {rows.map((entry) => (
        <EventRow entry={entry} key={entry.id} />
      ))}
      {entries.length === 0 && (
        <li className="px-4 py-6 text-center text-sm text-muted-foreground">
          {isSending
            ? "Waiting for the first event…"
            : "The stream ended without sending an event."}
        </li>
      )}
      {droppedCount > 0 && (
        <li className="px-4 py-2 text-center text-xs text-muted-foreground">
          {droppedCount} earlier {droppedCount === 1 ? "event" : "events"} dropped
        </li>
      )}
    </ol>
  );
}

/**
 * Memoised: every batch re-renders the list, and an event never changes once
 * it has arrived, so only the new rows need rendering (the WebSocket log
 * found this out the hard way, PLAN-WEBSOCKET.md 13h).
 */
const EventRow = memo(function EventRow({ entry }: { entry: SseLogEntry }) {
  const { block } = entry;
  const comment = block.kind === "comment";
  const body = comment ? block.text : block.data;
  const shown = body.length > MAX_SHOWN_CHARS ? body.slice(0, MAX_SHOWN_CHARS) : body;

  return (
    <li className="border-b border-border/60 px-4 py-1.5">
      <div className="flex items-baseline gap-2">
        <span className="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground">
          {entry.index}
        </span>
        <span
          className={`shrink-0 rounded px-1.5 py-0.5 font-mono text-xs ${
            comment ? "text-muted-foreground" : "bg-accent text-foreground"
          }`}
        >
          {blockLabel(block)}
        </span>
        {!comment && block.id !== null && (
          <span className="shrink-0 font-mono text-xs text-muted-foreground">id {block.id}</span>
        )}
        <time className="ml-auto shrink-0 font-mono text-xs text-muted-foreground">
          {formatClockTime(entry.atMs)}
        </time>
      </div>
      <pre
        className={`mt-0.5 whitespace-pre-wrap break-words pl-10 font-mono text-xs ${
          comment ? "text-muted-foreground" : ""
        }`}
      >
        {shown}
      </pre>
      {shown.length < body.length && (
        <p className="pl-10 text-xs text-muted-foreground">
          … {body.length - shown.length} more characters
        </p>
      )}
    </li>
  );
});

/**
 * While the stream runs this is a plain element: handing Monaco a document
 * that grows every frame is not what it is for. Once the request finishes,
 * the response body is the authority — it is the whole transfer, including
 * anything the log's caps dropped — and the editor takes over.
 */
function RawView({
  stream,
  response,
  isSending,
}: {
  stream: ResponseStream;
  response: HttpResponse | null;
  isSending: boolean;
}) {
  if (!isSending && response !== null && response.body.kind === "text") {
    return (
      <div className="flex-1">
        <LazyCodeEditor language="plaintext" readOnly value={response.body.text} />
      </div>
    );
  }
  return (
    <pre
      aria-label="Raw stream"
      className="flex-1 overflow-auto whitespace-pre-wrap break-words px-4 py-2 font-mono text-xs"
    >
      {rawText(stream.log)}
    </pre>
  );
}

function HeaderTable({ headers }: { headers: KeyValue[] }) {
  return (
    <div className="flex-1 overflow-auto">
      <table className="w-full text-left text-sm">
        <tbody>
          {headers.map((header) => (
            <tr className="border-b border-border/50" key={`${header.name}:${header.value}`}>
              <th className="w-64 px-4 py-1 align-top font-medium text-muted-foreground">
                {header.name}
              </th>
              <td className="px-4 py-1 font-mono text-xs">{header.value}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

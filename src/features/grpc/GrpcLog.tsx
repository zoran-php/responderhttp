// http_client/src/features/grpc/GrpcLog.tsx
//
// The message stream of a gRPC tab: the shared MessageStreamLog, told how
// a gRPC event looks. What is kept, shown and cleared is decided in
// lib/grpc-log.ts.
import { useMemo } from "react";
import { AlertCircle, ArrowDown, ArrowUp, CheckCircle2, Info } from "lucide-react";

import {
  MessageStreamLog,
  PairsTable,
  TextMessageDetail,
  type StreamLogBadge,
  type StreamLogRenderers,
} from "@/components/MessageStreamLog";
import {
  filterLog,
  systemText,
  type GrpcLog as GrpcLogData,
  type GrpcLogEntry,
  type GrpcLogFilter,
} from "@/lib/grpc-log";
import { GRPC_LOG_FILTER_OPTIONS } from "@/lib/grpc-status";
import type { GrpcEvent } from "@/types/grpc";

interface GrpcLogProps {
  log: GrpcLogData;
  filter: GrpcLogFilter;
  query: string;
  badge: StreamLogBadge;
  onFilterChange: (filter: GrpcLogFilter) => void;
  onQueryChange: (query: string) => void;
  onClearMessages: () => void;
}

/** Module level, so the memoised rows never see new functions. */
const RENDERERS: StreamLogRenderers<GrpcLogEntry> = {
  icon: (entry) => <EventIcon event={entry.event} />,
  summary: (entry) => systemText(entry.event),
  detail: (entry) => <EventDetail event={entry.event} />,
  atMs: (entry) => entry.event.atMs,
};

export function GrpcLog(props: GrpcLogProps) {
  const { log, filter, query } = props;
  const rows = useMemo(
    () => filterLog(log.entries, { direction: filter, query }).reverse(),
    [log.entries, filter, query],
  );

  return (
    <MessageStreamLog
      badge={props.badge}
      droppedCount={log.droppedCount}
      emptyText="Invoke the method to see messages here."
      filter={filter}
      filterOptions={GRPC_LOG_FILTER_OPTIONS}
      onClearMessages={props.onClearMessages}
      onFilterChange={props.onFilterChange}
      onQueryChange={props.onQueryChange}
      query={query}
      renderers={RENDERERS}
      rows={rows}
      totalCount={log.entries.length}
    />
  );
}

function EventIcon({ event }: { event: GrpcEvent }) {
  switch (event.type) {
    case "sent":
      return <ArrowUp aria-label="Sent" className="h-3.5 w-3.5 shrink-0 text-ws-sent" />;
    case "received":
      return <ArrowDown aria-label="Received" className="h-3.5 w-3.5 shrink-0 text-ws-received" />;
    case "ended":
      return event.status.code === 0 ? (
        <CheckCircle2 aria-label="Ended" className="h-3.5 w-3.5 shrink-0 text-ws-ok" />
      ) : (
        <AlertCircle aria-label="Ended" className="h-3.5 w-3.5 shrink-0 text-destructive" />
      );
    case "responseMetadata":
    case "streamEnded":
      return <Info aria-label="Event" className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />;
  }
}

function EventDetail({ event }: { event: GrpcEvent }) {
  switch (event.type) {
    case "sent":
    case "received":
      return <TextMessageDetail byteLength={event.bytes} isJson text={event.json} />;
    case "responseMetadata":
      return (
        <div className="px-4 pb-3 pl-10 text-xs">
          <PairsTable pairs={event.metadata} />
        </div>
      );
    case "ended":
      return (
        <div className="space-y-2 px-4 pb-3 pl-10 text-xs">
          <p className="font-mono">{systemText(event)}</p>
          {event.trailers.length > 0 && <PairsTable pairs={event.trailers} />}
        </div>
      );
    case "streamEnded":
      return <p className="px-4 pb-3 pl-10 font-mono text-xs">{systemText(event)}</p>;
  }
}

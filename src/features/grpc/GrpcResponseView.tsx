// http_client/src/features/grpc/GrpcResponseView.tsx
//
// The bottom half of a gRPC tab. A unary call shows its one message, its
// metadata and trailers, with the status, the time and the size; a
// streaming call shows the message stream. Why a call did not start is
// shown above either.
import { useState } from "react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { PairsTable, type StreamLogBadge } from "@/components/MessageStreamLog";
import { GrpcLog } from "@/features/grpc/GrpcLog";
import { formatBytes, formatDuration } from "@/lib/format";
import type { GrpcLog as GrpcLogData, GrpcLogFilter } from "@/lib/grpc-log";
import { grpcBadge, type GrpcBadgeTone, type GrpcCallState } from "@/lib/grpc-status";
import { prettyPrint } from "@/lib/pretty-print";
import type { GrpcResponse } from "@/store/request-store";

const BADGE_CLASS: Record<GrpcBadgeTone, string> = {
  idle: "bg-muted text-muted-foreground",
  pending: "bg-muted text-muted-foreground",
  ok: "bg-ws-ok/15 text-ws-ok",
  error: "bg-destructive/15 text-destructive",
};

const TABS = ["Body", "Metadata", "Trailers"] as const;
type Tab = (typeof TABS)[number];

interface GrpcResponseViewProps {
  /** The picked method streams on either side: show the message stream. */
  streaming: boolean;
  call: GrpcCallState;
  response: GrpcResponse | null;
  callError: string | null;
  log: GrpcLogData;
  logFilter: GrpcLogFilter;
  logQuery: string;
  onFilterChange: (filter: GrpcLogFilter) => void;
  onQueryChange: (query: string) => void;
  onClearMessages: () => void;
}

export function GrpcResponseView(props: GrpcResponseViewProps) {
  const outcome = props.response?.outcome ?? null;
  const tone = grpcBadge(props.call, outcome);
  const badge: StreamLogBadge = { label: tone.label, className: BADGE_CLASS[tone.tone] };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      {props.callError !== null && (
        <p className="border-b border-border px-4 py-2 text-sm text-destructive" role="alert">
          {props.callError}
        </p>
      )}
      {props.streaming ? (
        <GrpcLog
          badge={badge}
          filter={props.logFilter}
          log={props.log}
          onClearMessages={props.onClearMessages}
          onFilterChange={props.onFilterChange}
          onQueryChange={props.onQueryChange}
          query={props.logQuery}
        />
      ) : (
        <UnaryResponse badge={badge} response={props.response} />
      )}
    </div>
  );
}

function UnaryResponse({
  badge,
  response,
}: {
  badge: StreamLogBadge;
  response: GrpcResponse | null;
}) {
  const [tab, setTab] = useState<Tab>("Body");
  const outcome = response?.outcome ?? null;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1 border-b border-border px-3 text-sm">
        <div aria-label="Response" className="flex items-center gap-1" role="tablist">
          {TABS.map((name) => (
            <button
              aria-selected={tab === name}
              role="tab"
              className={`border-b-2 px-3 py-2 ${
                tab === name
                  ? "border-primary font-medium text-foreground"
                  : "border-transparent text-muted-foreground"
              }`}
              key={name}
              onClick={() => setTab(name)}
              type="button"
            >
              {name}
            </button>
          ))}
        </div>
        <div className="ml-auto flex items-center gap-3 text-xs text-muted-foreground">
          {outcome !== null && <span>{formatDuration(outcome.totalMs)}</span>}
          {response !== null && response.receivedCount > 0 && (
            <span>{formatBytes(response.receivedBytes)}</span>
          )}
          <span className={`rounded px-2 py-0.5 font-medium ${badge.className}`} role="status">
            {badge.label}
          </span>
        </div>
      </div>

      {outcome !== null && outcome.status.message !== "" && (
        <p className="border-b border-border px-4 py-2 font-mono text-xs">
          {outcome.status.message}
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === "Body" &&
          (response?.lastMessage == null ? (
            <p className="p-4 text-sm text-muted-foreground">
              {response === null
                ? "Invoke the method to see the response here."
                : "No message was received."}
            </p>
          ) : (
            <div aria-label="Response message" className="h-full">
              <LazyCodeEditor
                language="json"
                readOnly
                value={prettyPrint(response.lastMessage, "json")}
              />
            </div>
          ))}
        {tab === "Metadata" && <PairsOrNothing pairs={response?.metadata ?? []} />}
        {tab === "Trailers" && <PairsOrNothing pairs={outcome?.trailers ?? []} />}
      </div>
    </div>
  );
}

function PairsOrNothing({ pairs }: { pairs: readonly { name: string; value: string }[] }) {
  return pairs.length === 0 ? (
    <p className="p-4 text-sm text-muted-foreground">None.</p>
  ) : (
    <div className="p-4 text-xs">
      <PairsTable pairs={pairs} />
    </div>
  );
}

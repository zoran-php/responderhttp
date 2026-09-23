// http_client/src/features/response-viewer/SizeBadge.tsx
//
// The response's size, with the breakdown on hover (PLAN-SSE.md, 14f).
// Presentational: lib/transfer-sizes.ts decides what is known and
// lib/format.ts picks the unit.
//
// Hand-rolled rather than a popover primitive: components/ui/ is empty, and
// a hover panel that needs no state, no portal and no outside-click handling
// does not justify a new dependency.
import { ArrowDown, ArrowUp } from "lucide-react";

import { formatBytes } from "@/lib/format";
import type { SizeBreakdown } from "@/lib/transfer-sizes";

export function SizeBadge({ sizes }: { sizes: SizeBreakdown }) {
  const requestTotal =
    sizes.requestHeaders === null || sizes.requestBody === null
      ? null
      : sizes.requestHeaders + sizes.requestBody;

  return (
    <span className="group relative">
      <button
        aria-label="Response size"
        className="rounded px-1 py-0.5 text-xs text-muted-foreground underline decoration-dotted underline-offset-4 hover:text-foreground focus:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        type="button"
      >
        {formatBytes(sizes.responseTotal)}
      </button>

      <span
        className="pointer-events-none invisible absolute left-0 top-full z-20 mt-1 w-60 rounded-md border border-border bg-popover p-3 text-xs shadow-md group-focus-within:visible group-hover:visible"
        role="tooltip"
      >
        <Group
          body={sizes.responseBody}
          direction="in"
          headers={sizes.responseHeaders}
          title="Response Size"
          total={sizes.responseTotal}
        />
        <span className="mt-2 block">
          <Group
            body={sizes.requestBody}
            direction="out"
            headers={sizes.requestHeaders}
            title="Request Size"
            total={requestTotal}
          />
        </span>
      </span>
    </span>
  );
}

interface GroupProps {
  title: string;
  direction: "in" | "out";
  /** Null while the request is still running: unknown, not zero. */
  total: number | null;
  headers: number | null;
  body: number | null;
}

function Group({ title, direction, total, headers, body }: GroupProps) {
  return (
    <span className="block">
      <span className="flex items-center justify-between gap-4 font-medium text-foreground">
        <span className="flex items-center gap-1.5">
          {direction === "in" ? (
            <ArrowDown aria-hidden className="h-3.5 w-3.5 text-ws-received" />
          ) : (
            <ArrowUp aria-hidden className="h-3.5 w-3.5 text-ws-sent" />
          )}
          {title}
        </span>
        <span>{size(total)}</span>
      </span>
      <Row label="Headers" value={headers} />
      <Row label="Body" value={body} />
    </span>
  );
}

function Row({ label, value }: { label: string; value: number | null }) {
  return (
    <span className="flex justify-between gap-4 pl-5 text-muted-foreground">
      <span>{label}</span>
      <span>{size(value)}</span>
    </span>
  );
}

/** An em dash while the request is still running, rather than a zero that
 * would read as "it sent nothing". */
function size(bytes: number | null): string {
  return bytes === null ? "—" : formatBytes(bytes);
}

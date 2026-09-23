// http_client/src/features/response-viewer/ResponseViewer.tsx
//
// Presentational. Language selection and pretty-printing are pure helpers in
// lib/; this only renders what they return.
import { useState, type ReactNode } from "react";
import { Download, FileWarning } from "lucide-react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { EventStreamView } from "@/features/response-viewer/EventStreamView";
import {
  SaveExampleButton,
  type SaveExampleAction,
} from "@/features/response-viewer/SaveExampleButton";
import { SendingIndicator } from "@/features/response-viewer/SendingIndicator";
import { SizeBadge } from "@/features/response-viewer/SizeBadge";
import { findHeader, languageForContentType } from "@/lib/content-type";
import { formatBytes, formatDuration } from "@/lib/format";
import { prettyPrint } from "@/lib/pretty-print";
import type { ResponseStream } from "@/lib/sse-log";
import { statusPillClass } from "@/lib/status-colors";
import { sizesOfResponse } from "@/lib/transfer-sizes";
import type { ApiError, DownloadResult, HttpResponse } from "@/types/http";

type View = "Pretty" | "Raw" | "Headers";

interface ResponseViewerProps {
  response: HttpResponse | null;
  /** Set instead of `response` when the body went to a file. */
  download: DownloadResult | null;
  error: ApiError | null;
  isSending: boolean;
  /**
   * What the request has reported so far. A `text/event-stream` response is
   * shown as it arrives instead of as a finished body (PLAN-SSE.md).
   */
  stream: ResponseStream;
  /**
   * Carries the reason when the response cannot be saved as an example: an
   * example hangs off a stored request, and a binary body has nothing to
   * store.
   */
  saveExample: SaveExampleAction;
}

export function ResponseViewer({
  response,
  download,
  error,
  isSending,
  stream,
  saveExample,
}: ResponseViewerProps) {
  const [view, setView] = useState<View>("Pretty");

  // Before the sending check: a stream is worth watching precisely while the
  // request is still running, and after it ends the events remain the
  // readable form of a body that is one long concatenation.
  if (stream.isEventStream && download === null) {
    return (
      <EventStreamView
        error={error}
        isSending={isSending}
        response={response}
        saveExample={saveExample}
        stream={stream}
      />
    );
  }
  if (isSending) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <SendingIndicator />
      </div>
    );
  }
  if (error) {
    return (
      <div className="flex-1 overflow-auto p-4">
        <p className="mb-1 text-sm font-medium text-destructive">{errorTitle(error)}</p>
        <p className="text-sm text-muted-foreground">{error.message}</p>
      </div>
    );
  }
  if (download) {
    return <DownloadSummary download={download} />;
  }
  if (!response) {
    return <Placeholder>Send a request to see the response.</Placeholder>;
  }

  const { status, headers, body, timing } = response;
  const language = languageForContentType(findHeader(headers, "content-type"));
  const text = body.kind === "text" ? body.text : "";

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-3 border-b border-border px-4 py-2 text-sm">
        <span className={`rounded px-2 py-0.5 font-medium ${statusPillClass(status)}`}>
          {status}
        </span>
        <span className="text-muted-foreground">{formatDuration(timing.totalMs)}</span>
        <SizeBadge sizes={sizesOfResponse(response.sizes)} />
        <span className="text-xs text-muted-foreground">
          DNS {formatDuration(timing.dnsMs)} · connect {formatDuration(timing.connectMs)} · TLS{" "}
          {formatDuration(timing.tlsMs)} · TTFB {formatDuration(timing.timeToFirstByteMs)}
        </span>

        <div className="ml-auto flex items-center gap-1">
          <SaveExampleButton action={saveExample} />
          {(["Pretty", "Raw", "Headers"] as const).map((name) => (
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

      {view === "Headers" ? (
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
      ) : body.kind === "binary" ? (
        <Placeholder>Binary response ({formatBytes(body.byteLength)})</Placeholder>
      ) : (
        <div className="flex-1">
          <LazyCodeEditor
            language={view === "Pretty" ? language : "plaintext"}
            readOnly
            value={view === "Pretty" ? prettyPrint(text, language) : text}
          />
        </div>
      )}
    </div>
  );
}

/** No body view: the bytes went to disk rather than through IPC, so there is
 * nothing here to render even if the response was text. */
function DownloadSummary({ download }: { download: DownloadResult }) {
  const saved = download.savedTo !== null;
  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-3 border-b border-border px-4 py-2 text-sm">
        <span className={`rounded px-2 py-0.5 font-medium ${statusPillClass(download.status)}`}>
          {download.status}
        </span>
        <span className="text-muted-foreground">{formatDuration(download.timing.totalMs)}</span>
        <span className="text-xs text-muted-foreground">{formatBytes(download.byteLength)}</span>
      </div>

      <div className="flex flex-1 items-center justify-center px-6">
        <div className="flex max-w-full flex-col items-center gap-2 text-center">
          {saved ? (
            <Download aria-hidden className="h-6 w-6 text-primary" />
          ) : (
            <FileWarning aria-hidden className="h-6 w-6 text-muted-foreground" />
          )}
          <p className="text-sm">{saved ? "Response saved" : "Download cancelled"}</p>
          <p className="max-w-full break-all font-mono text-xs text-muted-foreground">
            {saved ? download.savedTo : "The request was sent; nothing was written to disk."}
          </p>
        </div>
      </div>

      <div className="border-t border-border">
        <table className="w-full text-left text-sm">
          <tbody>
            {download.headers.map((header) => (
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
    </div>
  );
}

function Placeholder({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
      {children}
    </div>
  );
}

function errorTitle(error: ApiError): string {
  switch (error.kind) {
    case "invalidRequest":
      return "Invalid request";
    case "transport":
      return "Request failed";
    case "cancelled":
      return "Request cancelled";
    case "notFound":
      return "Not found";
    case "storage":
      return "Storage error";
    case "internal":
      return "Something went wrong";
    case "secretStore":
      return "Credential store unavailable";
  }
}

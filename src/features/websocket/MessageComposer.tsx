// http_client/src/features/websocket/MessageComposer.tsx
//
// The Message tab: an editor, the format selector (with the encoding
// selector for Binary, and Beautify for JSON, XML and HTML) and Send. The
// rules — what a format sends, when binary refuses, what Beautify does — are
// in lib/ws-payload.ts and lib/beautify.ts; this renders their answers.
import { Paintbrush, Send } from "lucide-react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { isBeautifiable } from "@/lib/beautify";
import { editorLanguage, jsonWarning, parseBinary } from "@/lib/ws-payload";
import {
  WS_BINARY_ENCODINGS,
  WS_MESSAGE_FORMATS,
  type WsBinaryEncoding,
  type WsDraft,
  type WsMessageFormat,
} from "@/types/websocket";

const FORMAT_LABELS: Record<WsMessageFormat, string> = {
  text: "Text",
  json: "JSON",
  xml: "XML",
  html: "HTML",
  binary: "Binary",
};

const ENCODING_LABELS: Record<WsBinaryEncoding, string> = {
  base64: "Base64",
  hex: "Hexadecimal",
};

const SELECT = "h-8 rounded-md border border-input bg-background px-2 text-sm";

interface MessageComposerProps {
  draft: WsDraft;
  connected: boolean;
  /** Why the last Send or Beautify did not go, if it did not. */
  error: string | null;
  onDraftChange: (patch: Partial<WsDraft>) => void;
  onBeautify: () => void;
  onSend: () => void;
}

export function MessageComposer({
  draft,
  connected,
  error,
  onDraftChange,
  onBeautify,
  onSend,
}: MessageComposerProps) {
  const binary = draft.format === "binary" ? parseBinary(draft.binaryEncoding, draft.text) : null;
  const binaryError = binary !== null && !binary.ok ? binary.error : null;
  const warning = draft.format === "json" ? jsonWarning(draft.text) : null;
  const canSend = connected && binaryError === null;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="min-h-0 flex-1" aria-label="Compose message">
        <LazyCodeEditor
          language={editorLanguage(draft.format)}
          onChange={(text) => onDraftChange({ text })}
          value={draft.text}
        />
      </div>

      <div className="flex items-center gap-2 border-t border-border px-3 py-2">
        <select
          aria-label="Message format"
          className={SELECT}
          onChange={(event) => onDraftChange({ format: event.target.value as WsMessageFormat })}
          value={draft.format}
        >
          {WS_MESSAGE_FORMATS.map((format) => (
            <option key={format} value={format}>
              {FORMAT_LABELS[format]}
            </option>
          ))}
        </select>

        {draft.format === "binary" && (
          <select
            aria-label="Binary encoding"
            className={SELECT}
            onChange={(event) =>
              onDraftChange({ binaryEncoding: event.target.value as WsBinaryEncoding })
            }
            value={draft.binaryEncoding}
          >
            {WS_BINARY_ENCODINGS.map((encoding) => (
              <option key={encoding} value={encoding}>
                {ENCODING_LABELS[encoding]}
              </option>
            ))}
          </select>
        )}

        {isBeautifiable(draft.format) && (
          <button
            aria-label="Beautify"
            className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={onBeautify}
            title="Beautify"
            type="button"
          >
            <Paintbrush aria-hidden className="h-4 w-4" />
          </button>
        )}

        <p className="min-w-0 flex-1 truncate text-xs" role="status">
          {error !== null ? (
            <span className="text-destructive">{error}</span>
          ) : binaryError !== null ? (
            <span className="text-destructive">{binaryError}</span>
          ) : warning !== null ? (
            <span className="text-warning">{warning}</span>
          ) : !connected ? (
            <span className="text-muted-foreground">Connect to send messages.</span>
          ) : null}
        </p>

        <button
          className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={!canSend}
          onClick={onSend}
          type="button"
        >
          <Send aria-hidden className="h-3.5 w-3.5" />
          Send
        </button>
      </div>
    </div>
  );
}

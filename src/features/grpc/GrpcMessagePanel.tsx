// http_client/src/features/grpc/GrpcMessagePanel.tsx
//
// The Message tab: the JSON editor, Beautify and Use Example Message, and,
// for a method whose client streams, Send and End Streaming while the call
// is open. What each does is decided in the store; this renders it.
import { Paintbrush, Send, Square, WandSparkles } from "lucide-react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { jsonWarning } from "@/lib/ws-payload";

interface GrpcMessagePanelProps {
  message: string;
  error: string | null;
  /** A method is picked, so an example can be made for it. */
  hasMethod: boolean;
  /** The picked method's client sends a stream. */
  clientStreams: boolean;
  running: boolean;
  streamEnded: boolean;
  onMessageChange: (message: string) => void;
  onBeautify: () => void;
  onUseExample: () => void;
  onSend: () => void;
  onEndStream: () => void;
}

const ICON_BUTTON =
  "inline-flex h-8 items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50";

export function GrpcMessagePanel(props: GrpcMessagePanelProps) {
  const warning = jsonWarning(props.message);
  const canSend = props.clientStreams && props.running && !props.streamEnded;
  const hint = !props.clientStreams
    ? null
    : !props.running
      ? "Invoke opens the stream; then send messages one at a time."
      : props.streamEnded
        ? "Streaming ended: nothing more can be sent on this call."
        : null;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div aria-label="Compose message" className="min-h-0 flex-1">
        <LazyCodeEditor language="json" onChange={props.onMessageChange} value={props.message} />
      </div>

      <div className="flex items-center gap-2 border-t border-border px-3 py-2">
        <button
          aria-label="Beautify"
          className={ICON_BUTTON}
          onClick={props.onBeautify}
          title="Beautify"
          type="button"
        >
          <Paintbrush aria-hidden className="h-4 w-4" />
        </button>
        <button
          className={ICON_BUTTON}
          disabled={!props.hasMethod}
          onClick={props.onUseExample}
          type="button"
        >
          <WandSparkles aria-hidden className="h-3.5 w-3.5" />
          Use Example Message
        </button>

        <p className="min-w-0 flex-1 truncate text-xs" role="status">
          {props.error !== null ? (
            <span className="text-destructive">{props.error}</span>
          ) : warning !== null ? (
            <span className="text-warning">{warning}</span>
          ) : hint !== null ? (
            <span className="text-muted-foreground">{hint}</span>
          ) : null}
        </p>

        {props.clientStreams && (
          <>
            <button
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-input px-3 text-sm text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
              disabled={!canSend}
              onClick={props.onEndStream}
              type="button"
            >
              <Square aria-hidden className="h-3.5 w-3.5" />
              End Streaming
            </button>
            <button
              className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={!canSend}
              onClick={props.onSend}
              type="button"
            >
              <Send aria-hidden className="h-3.5 w-3.5" />
              Send
            </button>
          </>
        )}
      </div>
    </div>
  );
}

// http_client/src/features/websocket/WebSocketDocsPanel.tsx
//
// The Docs sub-tab (PLAN-WEBSOCKET.md, decision D4): the saved item's
// documentation, rendered, and a button into the existing Docs tab. There is
// deliberately no second way to edit documentation.
import { useEffect, useState } from "react";
import { FileText } from "lucide-react";

import { DocsPreview } from "@/features/docs/DocsPreview";
import { itemDocs } from "@/services/docs";

interface WebSocketDocsPanelProps {
  /** Null until the request is saved: documentation hangs off a stored item. */
  requestId: string | null;
  onOpenDocs: (requestId: string) => void;
}

type Loaded = { state: "loading" } | { state: "ready"; markdown: string } | { state: "failed" };

export function WebSocketDocsPanel({ requestId, onOpenDocs }: WebSocketDocsPanelProps) {
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });

  useEffect(() => {
    if (requestId === null) {
      return;
    }
    let current = true;
    setLoaded({ state: "loading" });
    void itemDocs({ kind: "request", id: requestId }).then((result) => {
      if (current) {
        setLoaded(result.ok ? { state: "ready", markdown: result.value } : { state: "failed" });
      }
    });
    // A request switched while the fetch was in flight must not show the
    // previous one's text.
    return () => {
      current = false;
    };
  }, [requestId]);

  if (requestId === null) {
    return (
      <p className="p-4 text-sm text-muted-foreground">Save the request to add documentation.</p>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex justify-end px-3 pt-2">
        <button
          className="inline-flex h-7 items-center gap-1.5 rounded border border-input px-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={() => onOpenDocs(requestId)}
          type="button"
        >
          <FileText aria-hidden className="h-3.5 w-3.5" />
          Edit documentation
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {loaded.state === "loading" && (
          <p className="p-4 text-sm text-muted-foreground">Loading documentation…</p>
        )}
        {loaded.state === "failed" && (
          <p className="p-4 text-sm text-destructive">The documentation could not be loaded.</p>
        )}
        {loaded.state === "ready" &&
          (loaded.markdown.trim() === "" ? (
            <p className="p-4 text-sm text-muted-foreground">No documentation yet.</p>
          ) : (
            <DocsPreview markdown={loaded.markdown} />
          ))}
      </div>
    </div>
  );
}

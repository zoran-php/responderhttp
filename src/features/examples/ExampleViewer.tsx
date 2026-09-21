// http_client/src/features/examples/ExampleViewer.tsx
//
// A saved response, opened as its own tab. Read-only on purpose: an example
// is a record of one exchange, so editing it would make it a record of
// nothing. To send the same request again, open the request itself.
//
// Not built on ResponseViewer: that one renders an HttpResponse, which
// carries a timing breakdown an example has no honest value for. Faking a
// Timing to reuse the component would put invented numbers on screen.
import { useEffect, useState } from "react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { findHeader, languageForContentType } from "@/lib/content-type";
import { methodTextColor } from "@/lib/http-method-colors";
import { prettyPrint } from "@/lib/pretty-print";
import { statusPillClass } from "@/lib/status-colors";
import { useCollectionsStore } from "@/store/collections-store";

type View = "Pretty" | "Raw" | "Headers";

interface ExampleViewerProps {
  exampleId: string;
}

export function ExampleViewer({ exampleId }: ExampleViewerProps) {
  const example = useCollectionsStore((state) => state.examplesById[exampleId]);
  // Selected on its own rather than off the store object, which would be a
  // new reference every render and re-fire the effect below.
  const loadExample = useCollectionsStore((state) => state.loadExample);
  const [view, setView] = useState<View>("Pretty");

  // The tree only carries summaries, so the body is fetched on open. Runs
  // again if the id changes, which it does when another example is opened
  // into the same tab position.
  useEffect(() => {
    void loadExample(exampleId);
  }, [exampleId, loadExample]);

  if (!example) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }

  const language = languageForContentType(findHeader(example.responseHeaders, "content-type"));

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-sm">
        <span
          className={`shrink-0 font-mono text-xs font-semibold ${methodTextColor(example.request.method)}`}
        >
          {example.request.method}
        </span>
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">
          {example.request.url}
        </span>
      </div>

      <div className="flex items-center gap-3 border-b border-border px-4 py-2 text-sm">
        <span className={`rounded px-2 py-0.5 font-medium ${statusPillClass(example.status)}`}>
          {example.status}
        </span>
        <span className="text-xs text-muted-foreground">
          Saved {example.createdAt.replace("T", " ").replace("Z", "")}
        </span>

        <div className="ml-auto flex gap-1">
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
              {example.responseHeaders.map((header) => (
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
      ) : (
        <div className="flex-1">
          <LazyCodeEditor
            language={view === "Pretty" ? language : "plaintext"}
            readOnly
            value={
              view === "Pretty"
                ? prettyPrint(example.responseBody, language)
                : example.responseBody
            }
          />
        </div>
      )}
    </div>
  );
}

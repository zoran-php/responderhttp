// http_client/src/features/docs/DocsEditor.tsx
//
// A Docs tab: toolbar, Monaco on the left, rendered preview on the right
// (PLAN.md Phase 12).
//
// The component owns no rules. Saving is the store's (it debounces and
// writes), formatting is lib/markdown-format.ts's, rendering is
// lib/markdown.ts's, and the divider arithmetic is lib/split-pane.ts's —
// the same functions the request and response panes already use, rather than
// a third implementation of a draggable split.
import { useCallback, useRef } from "react";

import { LazyCodeEditor } from "@/components/LazyCodeEditor";
import { ResizeHandle } from "@/components/ResizeHandle";
import { applyFormat, type MarkdownAction } from "@/lib/markdown-format";
import { clampPaneSize, MIN_DOCS_PANE_WIDTH } from "@/lib/split-pane";
import { matchesSaveShortcut } from "@/lib/shortcuts";
import { useLayoutStore } from "@/store/layout-store";
import { useTabsStore, type DocsTab } from "@/store/request-store";
import { DocsPreview } from "@/features/docs/DocsPreview";
import { DocsToolbar } from "@/features/docs/DocsToolbar";

interface DocsEditorProps {
  tab: DocsTab;
}

export function DocsEditor({ tab }: DocsEditorProps) {
  const setDocsMarkdown = useTabsStore((state) => state.setDocsMarkdown);
  const setDocsView = useTabsStore((state) => state.setDocsView);
  const saveDocs = useTabsStore((state) => state.saveDocs);
  const docsEditorWidth = useLayoutStore((state) => state.docsEditorWidth);
  const setDocsEditorWidth = useLayoutStore((state) => state.setDocsEditorWidth);
  const resetDocsEditorWidth = useLayoutStore((state) => state.resetDocsEditorWidth);

  // Measured rather than assumed: the room the split has depends on the
  // sidebar, which the user can drag or collapse at any moment.
  const split = useRef<HTMLDivElement>(null);

  const applyEditorWidth = useCallback(
    (desired: number) => {
      setDocsEditorWidth(
        clampPaneSize(desired, {
          available: split.current?.clientWidth ?? desired,
          minStart: MIN_DOCS_PANE_WIDTH,
          minEnd: MIN_DOCS_PANE_WIDTH,
        }),
      );
    },
    [setDocsEditorWidth],
  );

  // Monaco does not report a selection through the onChange prop, and the
  // toolbar needs one. Rather than reach into the editor instance, the
  // toolbar acts on the end of the document when there is no selection to
  // act on — which is what a user pressing a formatting button with no
  // selection means anyway: "insert one here".
  const lastLength = useRef(tab.markdown.length);
  lastLength.current = tab.markdown.length;

  const handleAction = useCallback(
    (action: MarkdownAction) => {
      const caret = lastLength.current;
      const result = applyFormat(tab.markdown, { start: caret, end: caret }, action);
      setDocsMarkdown(tab.id, result.text);
    },
    [setDocsMarkdown, tab.id, tab.markdown],
  );

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    // Ctrl+S forces the write rather than waiting out the debounce. The
    // browser's own save dialog would otherwise open over the app.
    if (matchesSaveShortcut(event)) {
      event.preventDefault();
      void saveDocs(tab.id);
    }
  }

  const showEditor = tab.view !== "preview";
  const showPreview = tab.view !== "edit";

  return (
    <div className="flex flex-1 flex-col overflow-hidden" onKeyDown={handleKeyDown}>
      <DocsToolbar
        disabled={!tab.loaded}
        onAction={handleAction}
        onViewChange={(view) => setDocsView(tab.id, view)}
        view={tab.view}
      />

      {tab.saveError !== null && (
        <div
          className="border-b border-destructive/40 bg-destructive/10 px-4 py-2 text-sm text-destructive"
          role="alert"
        >
          {tab.saveError}
        </div>
      )}

      <div className="flex flex-1 overflow-hidden" ref={split}>
        {showEditor && (
          <div
            className="flex min-w-0 flex-1 flex-col"
            style={showPreview ? { flex: "none", width: docsEditorWidth } : undefined}
          >
            <LazyCodeEditor
              language="markdown"
              onChange={(value) => setDocsMarkdown(tab.id, value)}
              readOnly={!tab.loaded}
              value={tab.markdown}
            />
          </div>
        )}

        {showEditor && showPreview && (
          <ResizeHandle
            axis="x"
            label="Resize the documentation editor"
            onReset={resetDocsEditorWidth}
            onSizeChange={applyEditorWidth}
            size={docsEditorWidth}
          />
        )}

        {showPreview && (
          <div className="min-w-0 flex-1">
            <DocsPreview markdown={tab.markdown} />
          </div>
        )}
      </div>
    </div>
  );
}

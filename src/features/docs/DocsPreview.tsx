// http_client/src/features/docs/DocsPreview.tsx
//
// The rendered half of a Docs tab.
//
// This is the only `dangerouslySetInnerHTML` in the app, and the string it
// receives comes from lib/markdown.ts, which sanitises against an allowlist.
// Nothing else may render Markdown — see that file for why the text is
// treated as hostile even though the user usually wrote it.
//
// Link clicks are caught here rather than being left to the browser. An
// anchor in a webview navigates the whole app away with no chrome to come
// back through, so lib/markdown.ts never emits an href at all; it leaves the
// URL in a data attribute and this component decides what happens. For now
// that is "copy it", because opening a URL in the user's real browser needs a
// way to ask the OS that this app does not have yet (PLAN.md Phase 12, open
// question).
import { useEffect, useMemo, useState } from "react";

import { LINK_CLASS, LINK_DATA_ATTRIBUTE, renderMarkdown } from "@/lib/markdown";

interface DocsPreviewProps {
  markdown: string;
}

/** How long the "copied" note stays up. */
const COPIED_NOTICE_MS = 1600;

export function DocsPreview({ markdown }: DocsPreviewProps) {
  const html = useMemo(() => renderMarkdown(markdown), [markdown]);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    if (copied === null) {
      return;
    }
    const timer = setTimeout(() => setCopied(null), COPIED_NOTICE_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  // One listener on the container rather than one per link: the links are
  // replaced wholesale on every keystroke in split view.
  function handleClick(event: React.MouseEvent<HTMLDivElement>) {
    const target = (event.target as HTMLElement).closest(`.${LINK_CLASS}`);
    const href = target?.getAttribute(LINK_DATA_ATTRIBUTE);
    if (href === null || href === undefined) {
      return;
    }
    event.preventDefault();
    void navigator.clipboard.writeText(href).then(() => setCopied(href));
  }

  if (markdown.trim().length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-center text-sm text-muted-foreground">
        Nothing documented yet. What is this for, what does it return, and what should the next
        person know?
      </div>
    );
  }

  return (
    <div className="relative h-full overflow-auto">
      <div
        className="markdown-preview p-4"
        // Sanitised in lib/markdown.ts against an explicit allowlist; see the
        // note at the top of this file.
        dangerouslySetInnerHTML={{ __html: html }}
        onClick={handleClick}
      />
      {copied !== null && (
        <div
          className="pointer-events-none sticky bottom-3 left-3 mr-3 w-fit rounded-md border border-border bg-popover px-3 py-1.5 text-xs text-popover-foreground shadow-lg"
          role="status"
        >
          Link copied: {copied}
        </div>
      )}
    </div>
  );
}

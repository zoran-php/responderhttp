// http_client/src/features/docs/DocsToolbar.tsx
//
// The Docs tab's toolbar: which pane is showing, and the formatting buttons.
//
// Presentational. It knows the names of the actions and nothing about what
// they do to the text — that is lib/markdown-format.ts, which is pure and
// tested on its own (CLAUDE.md section 6).
import {
  Bold,
  Code,
  Columns2,
  Eye,
  Heading,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  Pencil,
  Quote,
  SquareCode,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

import type { MarkdownAction } from "@/lib/markdown-format";
import type { DocsView } from "@/store/request-store";

interface DocsToolbarProps {
  view: DocsView;
  onViewChange: (view: DocsView) => void;
  onAction: (action: MarkdownAction) => void;
  /** Formatting is pointless while the document is still loading, and
   * impossible while it is read-only. */
  disabled: boolean;
}

const VIEWS: { view: DocsView; label: string; icon: LucideIcon }[] = [
  { view: "edit", label: "Edit", icon: Pencil },
  { view: "preview", label: "Preview", icon: Eye },
  { view: "split", label: "Split", icon: Columns2 },
];

interface ToolbarAction {
  action: MarkdownAction;
  label: string;
  icon: LucideIcon;
}

/**
 * Grouped the way the separators below divide them: emphasis, then code and
 * links, then block-level structure.
 *
 * Each group carries its own name rather than being keyed by its first
 * member, so the React key does not depend on an index the type system
 * cannot prove is there.
 */
const ACTION_GROUPS: { name: string; actions: ToolbarAction[] }[] = [
  {
    name: "emphasis",
    actions: [
      { action: "bold", label: "Bold", icon: Bold },
      { action: "italic", label: "Italic", icon: Italic },
    ],
  },
  {
    name: "code-and-links",
    actions: [
      { action: "code", label: "Inline code", icon: Code },
      { action: "codeBlock", label: "Code block", icon: SquareCode },
      { action: "link", label: "Link", icon: LinkIcon },
    ],
  },
  {
    name: "structure",
    actions: [
      { action: "heading", label: "Heading", icon: Heading },
      { action: "bulletList", label: "Bulleted list", icon: List },
      { action: "numberedList", label: "Numbered list", icon: ListOrdered },
      { action: "quote", label: "Quote", icon: Quote },
    ],
  },
];

export function DocsToolbar({ view, onViewChange, onAction, disabled }: DocsToolbarProps) {
  return (
    <div className="flex items-center gap-2 border-b border-border bg-card px-2 py-1">
      <div className="flex items-center gap-0.5">
        {VIEWS.map(({ view: candidate, label, icon: Icon }) => (
          <button
            aria-pressed={view === candidate}
            className={`flex items-center gap-1.5 rounded px-2 py-1 text-xs ${
              view === candidate
                ? "bg-background font-medium text-foreground"
                : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
            }`}
            key={candidate}
            onClick={() => onViewChange(candidate)}
            type="button"
          >
            <Icon aria-hidden className="h-3.5 w-3.5" />
            {label}
          </button>
        ))}
      </div>

      {/* Formatting acts on the editor, so it is hidden when the editor is
          not on screen rather than sitting there doing nothing. */}
      {view !== "preview" && (
        <>
          <span aria-hidden className="h-4 w-px bg-border" />
          {ACTION_GROUPS.map((group, index) => (
            <div className="flex items-center gap-0.5" key={group.name}>
              {index > 0 && <span aria-hidden className="mx-1 h-4 w-px bg-border" />}
              {group.actions.map(({ action, label, icon: Icon }) => (
                <button
                  aria-label={label}
                  className="rounded p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent"
                  disabled={disabled}
                  key={action}
                  onClick={() => onAction(action)}
                  title={label}
                  type="button"
                >
                  <Icon aria-hidden className="h-3.5 w-3.5" />
                </button>
              ))}
            </div>
          ))}
        </>
      )}
    </div>
  );
}

// http_client/src/features/request-builder/TabBar.tsx
//
// App.tsx owns the confirm-before-closing-a-dirty-tab prompt, since that
// needs the shared ConfirmDialog and knows what "dirty" means for a tab.
import type { ReactNode } from "react";
import { FileText, Layers, MessageSquare, Plus, X } from "lucide-react";

import { methodTextColor } from "@/lib/http-method-colors";
import type { HttpMethod } from "@/types/http";

/**
 * Mirrors the tab union in the store. An environment tab has no method and
 * cannot be dirty — it saves explicitly — so those fields are absent rather
 * than optional. A docs tab autosaves, so its dot is lit only between a
 * keystroke and the write landing, or when a write has failed.
 */
export type TabBarTab =
  | { kind: "request"; id: string; label: string; method: HttpMethod; isDirty: boolean }
  | { kind: "environment"; id: string; label: string }
  | { kind: "example"; id: string; label: string }
  | { kind: "docs"; id: string; label: string; isDirty: boolean };

/** Only two of the four kinds can be unsaved, so this asks rather than
 * assuming the field is there. */
function isDirty(tab: TabBarTab): boolean {
  return "isDirty" in tab && tab.isDirty;
}

interface TabBarProps {
  tabs: TabBarTab[];
  activeTabId: string;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onNew: () => void;
  /** Sits at the far right of the strip — the active-environment selector. */
  trailing?: ReactNode;
}

export function TabBar({ tabs, activeTabId, onSelect, onClose, onNew, trailing }: TabBarProps) {
  return (
    // `bg-card` (#2e3440, the sidebar surface), not `bg-muted/30`. An alpha
    // fill over `bg-background` landed within about one point of lightness of
    // the pane it sat on, so the strip, the tabs and the content behind them
    // were three shades of the same colour. A chrome strip should read as
    // chrome: a step *down* from the content it frames, matching the sidebar.
    <div className="flex items-center gap-1 border-b border-border bg-card px-2">
      <div className="flex flex-1 items-center gap-1 overflow-x-auto py-1">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          return (
            <div
              // The active tab is filled with the *content* colour, so it
              // reads as raised to the level of the pane below it — which it
              // now can, because the strip behind it is darker. The primary
              // underline is the same accent the Params/Auth/Headers row uses,
              // so the two levels of tabs agree on what "selected" looks like.
              //
              // Both states carry `border-b-2` so selecting a tab cannot shift
              // the row by a pixel. `shadow-sm` is gone: a drop shadow does
              // nothing legible on a dark surface, and it was carrying the
              // whole burden of separating the active tab before.
              //
              // Hover is a *fraction* of the active fill, not `bg-accent`.
              // `bg-accent` (#434c5e) is lighter than the active tab, so
              // hovering an inactive tab made it look more selected than the
              // selected one. The ladder has to read strip < hover < active,
              // and there is no token between `card` and `background`, so this
              // is the one place an alpha fill is right: the parent is
              // `bg-card`, set five lines above, and the intent is literally
              // "part of the way from the strip toward the active fill".
              className={`group flex h-8 shrink-0 items-center gap-2 rounded-md border-b-2 px-3 text-sm ${
                isActive
                  ? "border-primary bg-background font-medium text-foreground"
                  : "border-transparent text-muted-foreground hover:bg-background/60 hover:text-foreground"
              }`}
              key={tab.id}
            >
              <button
                className="flex max-w-48 items-center text-left"
                onClick={() => onSelect(tab.id)}
                type="button"
              >
                {/* Hoisted out of the per-kind branch below: two kinds of
                    tab can be unsaved now, and the dot means the same thing
                    on both. */}
                {isDirty(tab) && (
                  <span
                    aria-hidden
                    className="mr-1.5 inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-current align-middle"
                  />
                )}
                {tab.kind === "request" ? (
                  <span
                    className={`mr-1.5 shrink-0 font-mono text-xs font-semibold ${methodTextColor(tab.method)}`}
                  >
                    {tab.method}
                  </span>
                ) : tab.kind === "environment" ? (
                  <Layers aria-hidden className="mr-1.5 h-3.5 w-3.5 shrink-0 text-primary" />
                ) : tab.kind === "docs" ? (
                  <FileText
                    aria-hidden
                    className="mr-1.5 h-3.5 w-3.5 shrink-0 text-muted-foreground"
                  />
                ) : (
                  <MessageSquare
                    aria-hidden
                    className="mr-1.5 h-3.5 w-3.5 shrink-0 text-muted-foreground"
                  />
                )}
                <span className="truncate">{tab.label}</span>
              </button>
              <button
                aria-label={`Close ${tab.label}`}
                className="rounded p-0.5 text-muted-foreground opacity-0 hover:bg-accent hover:text-foreground group-hover:opacity-100"
                onClick={(event) => {
                  event.stopPropagation();
                  onClose(tab.id);
                }}
                type="button"
              >
                <X aria-hidden className="h-3 w-3" />
              </button>
            </div>
          );
        })}
      </div>

      <button
        aria-label="New tab"
        className="shrink-0 rounded p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
        onClick={onNew}
        type="button"
      >
        <Plus aria-hidden className="h-4 w-4" />
      </button>

      {trailing}
    </div>
  );
}

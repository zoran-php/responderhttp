// http_client/src/features/sidebar/Sidebar.tsx
//
// Owns the sidebar shell and which of its three panels is showing. The
// panels themselves know nothing about each other or about the tab strip.
import { useState } from "react";

import { CollectionsSidebar } from "@/features/collections/CollectionsSidebar";
import { EnvironmentsPanel } from "@/features/environments/EnvironmentsPanel";
import { HistoryPanel } from "@/features/history/HistoryPanel";
import type { SavedRequest } from "@/types/collections";
import type { DocsTarget } from "@/types/docs";
import type { SendRequestInput } from "@/types/http";

const PANELS = ["Collections", "Environments", "History"] as const;
type Panel = (typeof PANELS)[number];

interface SidebarProps {
  /** Dragged by the divider to its right; see lib/split-pane.ts for the
   * floor, which is the panel tab row below rather than a taste call. */
  width: number;
  loadedRequestId: string | null;
  onOpenRequest: (saved: SavedRequest) => void;
  onOpenEnvironment: (environmentId: string) => void;
  onOpenExample: (exampleId: string) => void;
  onOpenHistoryEntry: (request: SendRequestInput) => void;
  onOpenDocs: (target: DocsTarget) => void;
}

export function Sidebar({
  width,
  loadedRequestId,
  onOpenRequest,
  onOpenEnvironment,
  onOpenExample,
  onOpenHistoryEntry,
  onOpenDocs,
}: SidebarProps) {
  const [panel, setPanel] = useState<Panel>("Collections");

  return (
    // No border-r: the resize handle to the right is the separator now.
    <div className="flex shrink-0 flex-col" style={{ width }}>
      <div className="flex border-b border-border">
        {PANELS.map((name) => (
          <button
            className={`flex-1 border-b-2 px-2 py-2 text-[11px] font-semibold uppercase tracking-wide ${
              panel === name
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
            key={name}
            onClick={() => setPanel(name)}
            type="button"
          >
            {name}
          </button>
        ))}
      </div>

      {panel === "Collections" && (
        <CollectionsSidebar
          loadedRequestId={loadedRequestId}
          onOpenDocs={onOpenDocs}
          onOpenExample={onOpenExample}
          onOpenRequest={onOpenRequest}
        />
      )}
      {panel === "Environments" && <EnvironmentsPanel onOpenEnvironment={onOpenEnvironment} />}
      {panel === "History" && <HistoryPanel onOpenEntry={onOpenHistoryEntry} />}
    </div>
  );
}

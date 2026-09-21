// http_client/src/features/collections/ContextMenu.tsx
//
// One right-click menu shape for collections, folders and requests — the row
// that opened it decides which actions are offered (CollectionsSidebar).
import { useEffect, useRef } from "react";
import type { LucideIcon } from "lucide-react";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  destructive?: boolean;
  /** Optional, because most entries here have never needed one. Where it is
   * set, the label is indented to match so the column stays straight. */
  icon?: LucideIcon;
}

interface ContextMenuProps {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        onClose();
      }
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  return (
    <div
      className="fixed z-50 min-w-40 rounded-md border border-border bg-popover py-1 text-sm text-popover-foreground shadow-lg"
      ref={ref}
      style={{ left: x, top: y }}
    >
      {items.map((item) => {
        const Icon = item.icon;
        return (
          <button
            className={`flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-accent ${
              item.destructive ? "text-destructive" : ""
            }`}
            key={item.label}
            onClick={() => {
              item.onSelect();
              onClose();
            }}
            type="button"
          >
            {Icon === undefined ? (
              // A spacer, so an entry without an icon lines up with one that
              // has it rather than sitting four pixels to the left.
              <span aria-hidden className="h-3.5 w-3.5 shrink-0" />
            ) : (
              <Icon aria-hidden className="h-3.5 w-3.5 shrink-0" />
            )}
            {item.label}
          </button>
        );
      })}
    </div>
  );
}

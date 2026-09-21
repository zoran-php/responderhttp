// http_client/src/features/collections/ContextMenu.tsx
//
// One right-click menu shape for collections, folders and requests — the row
// that opened it decides which actions are offered (CollectionsSidebar).
import { useEffect, useRef } from "react";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  destructive?: boolean;
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
      {items.map((item) => (
        <button
          className={`block w-full px-3 py-1.5 text-left hover:bg-accent ${
            item.destructive ? "text-destructive" : ""
          }`}
          key={item.label}
          onClick={() => {
            item.onSelect();
            onClose();
          }}
          type="button"
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}

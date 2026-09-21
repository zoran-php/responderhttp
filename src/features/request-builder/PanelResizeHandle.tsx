// http_client/src/features/request-builder/PanelResizeHandle.tsx
//
// The grab strip between the request panel and the response viewer. Dumb by
// design: it turns pointer and key events into a *desired* height and hands
// that up. It does no clamping and knows nothing about what is below it —
// lib/split-pane.ts decides what is allowed (CLAUDE.md §6).
import { useRef, type KeyboardEvent, type PointerEvent } from "react";

/** One nudge of an arrow key. Roughly a table row. */
const KEYBOARD_STEP = 24;

interface PanelResizeHandleProps {
  /** Height of the panel above. A drag is a delta from where it started, so
   * the handle needs to know where that was. */
  height: number;
  label: string;
  onHeightChange: (height: number) => void;
  onReset: () => void;
}

export function PanelResizeHandle({
  height,
  label,
  onHeightChange,
  onReset,
}: PanelResizeHandleProps) {
  const drag = useRef<{ pointerY: number; height: number } | null>(null);

  function handlePointerDown(event: PointerEvent<HTMLDivElement>) {
    drag.current = { pointerY: event.clientY, height };
    // Capture, so a fast drag that outruns the 6px strip keeps sending moves
    // here instead of to whatever the pointer is now over.
    event.currentTarget.setPointerCapture(event.pointerId);
    // Without this, dragging selects the table text either side of the strip.
    document.body.style.userSelect = "none";
  }

  function handlePointerMove(event: PointerEvent<HTMLDivElement>) {
    const start = drag.current;
    if (!start) {
      return;
    }
    onHeightChange(start.height + (event.clientY - start.pointerY));
  }

  function endDrag(event: PointerEvent<HTMLDivElement>) {
    if (!drag.current) {
      return;
    }
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    document.body.style.userSelect = "";
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "ArrowUp") {
      event.preventDefault();
      onHeightChange(height - KEYBOARD_STEP);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      onHeightChange(height + KEYBOARD_STEP);
    } else if (event.key === "Home") {
      event.preventDefault();
      onReset();
    }
  }

  return (
    <div
      aria-label={label}
      aria-orientation="horizontal"
      // The separator pattern wants a value; min and max are the caller's
      // business and change with the window, so only the current one is
      // reported rather than a pair that would go stale.
      aria-valuenow={Math.round(height)}
      className="h-1.5 shrink-0 cursor-row-resize border-b border-border transition-colors hover:border-primary hover:bg-primary/20 focus-visible:border-primary focus-visible:bg-primary/20 focus-visible:outline-none"
      onDoubleClick={onReset}
      onKeyDown={handleKeyDown}
      onPointerCancel={endDrag}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      role="separator"
      tabIndex={0}
      title="Drag to resize. Double-click to reset."
    />
  );
}

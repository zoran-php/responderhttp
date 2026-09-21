// http_client/src/components/ResizeHandle.tsx
//
// The strip between two panes: drag it to resize, click the chevron on it to
// collapse the pane it belongs to, click again to restore.
//
// Shared rather than feature-local because both dividers need it and the
// pointer-capture, keyboard and collapse logic is identical — only the axis
// changes. Two copies of ninety lines is the duplication §7 forbids.
//
// Dumb by design: it turns pointer and key events into a *desired* size and
// hands that up. It does no clamping and knows nothing about what sits either
// side — lib/split-pane.ts decides what is allowed (CLAUDE.md §6).
import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { ChevronDown, ChevronLeft, ChevronRight, ChevronUp } from "lucide-react";

/** One nudge of an arrow key. Roughly a table row. */
const KEYBOARD_STEP = 24;

/**
 * Which way the strip moves. `x` is a vertical bar dragged left and right,
 * sizing a width; `y` is a horizontal bar dragged up and down, sizing a
 * height. Named for the axis rather than for the bar's own orientation,
 * because "horizontal splitter" means opposite things to different people.
 */
type Axis = "x" | "y";

interface ResizeHandleProps {
  axis: Axis;
  /** Size of the pane before the handle. A drag is a delta from where it
   * started, so the handle needs to know where that was. */
  size: number;
  label: string;
  onSizeChange: (size: number) => void;
  onReset: () => void;
  /**
   * Collapsing is optional. The documentation split has its own Edit /
   * Preview / Split switcher, which already does what a chevron here would,
   * and two ways to hide the same pane is one too many. Leave these out and
   * the strip is a plain divider.
   */
  collapsed?: boolean;
  collapseLabel?: string;
  onToggleCollapse?: () => void;
}

export function ResizeHandle({
  axis,
  size,
  label,
  onSizeChange,
  onReset,
  collapsed = false,
  collapseLabel,
  onToggleCollapse,
}: ResizeHandleProps) {
  const drag = useRef<{ pointer: number; size: number } | null>(null);
  const horizontal = axis === "x";

  function pointerPosition(event: PointerEvent<HTMLDivElement>) {
    return horizontal ? event.clientX : event.clientY;
  }

  function handlePointerDown(event: PointerEvent<HTMLDivElement>) {
    // A collapsed pane has no size to drag. Restoring is the chevron's job,
    // which keeps "how do I get it back" to exactly one answer.
    if (collapsed) {
      return;
    }
    drag.current = { pointer: pointerPosition(event), size };
    // Capture, so a fast drag that outruns the 6px strip keeps sending moves
    // here instead of to whatever the pointer is now over.
    event.currentTarget.setPointerCapture(event.pointerId);
    // Without this, dragging selects the text either side of the strip.
    document.body.style.userSelect = "none";
  }

  function handlePointerMove(event: PointerEvent<HTMLDivElement>) {
    const start = drag.current;
    if (!start) {
      return;
    }
    onSizeChange(start.size + (pointerPosition(event) - start.pointer));
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
    if (collapsed) {
      return;
    }
    const decrease = horizontal ? "ArrowLeft" : "ArrowUp";
    const increase = horizontal ? "ArrowRight" : "ArrowDown";
    if (event.key === decrease) {
      event.preventDefault();
      onSizeChange(size - KEYBOARD_STEP);
    } else if (event.key === increase) {
      event.preventDefault();
      onSizeChange(size + KEYBOARD_STEP);
    } else if (event.key === "Home") {
      event.preventDefault();
      onReset();
    }
  }

  // The chevron points the way the pane is about to go: away when expanded,
  // back when collapsed.
  const Chevron = horizontal
    ? collapsed
      ? ChevronRight
      : ChevronLeft
    : collapsed
      ? ChevronUp
      : ChevronDown;

  return (
    <div
      aria-label={label}
      aria-orientation={horizontal ? "vertical" : "horizontal"}
      // The separator pattern wants a value; min and max are the caller's
      // business and change with the window, so only the current one is
      // reported rather than a pair that would go stale.
      aria-valuenow={Math.round(size)}
      className={`group relative shrink-0 border-border transition-colors ${
        horizontal ? "w-1.5 border-r" : "h-1.5 border-b"
      } ${collapsed ? "" : "hover:border-primary hover:bg-primary/20 focus-visible:border-primary focus-visible:bg-primary/20"} ${
        horizontal ? "cursor-col-resize" : "cursor-row-resize"
      } ${collapsed ? "cursor-default" : ""} focus-visible:outline-none`}
      onDoubleClick={collapsed ? undefined : onReset}
      onKeyDown={handleKeyDown}
      onPointerCancel={endDrag}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      role="separator"
      tabIndex={collapsed ? -1 : 0}
      title={collapsed ? collapseLabel : "Drag to resize. Double-click to reset."}
    >
      {/* A real button rather than a click-versus-drag guess on a 6px strip.
          It sits on top of the handle and stops pointerdown reaching it, so
          pressing the chevron never starts a drag. Always rendered: when the
          pane is collapsed this is the only way back.

          Deliberately not centred on the strip. A 12px button centred on 6px
          hangs 3px over each side, and in both collapsed states the strip ends
          up flush against the window edge — the sidebar's at the far left, the
          response's at the very bottom — where that overhang is clipped and
          the only control that brings the pane back is half invisible. So it
          overlaps inward instead, into the pane that is always there. */}
      {onToggleCollapse !== undefined && collapseLabel !== undefined && (
        <button
          aria-label={collapseLabel}
          className={`absolute z-10 flex items-center justify-center rounded border border-border bg-card text-muted-foreground opacity-0 transition-opacity hover:bg-accent hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100 ${
            horizontal ? "left-0 top-8 h-8 w-3" : "bottom-0 left-8 h-3 w-8"
          } ${collapsed ? "opacity-100" : ""}`}
          onClick={onToggleCollapse}
          onDoubleClick={(event) => event.stopPropagation()}
          onPointerDown={(event) => event.stopPropagation()}
          title={collapseLabel}
          type="button"
        >
          <Chevron aria-hidden className="h-3 w-3" />
        </button>
      )}
    </div>
  );
}

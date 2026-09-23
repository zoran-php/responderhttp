// http_client/src/hooks/useBuilderPanelHeight.ts
//
// The draggable height of a builder's tab panel, shared by the HTTP request
// builder and the WebSocket builder: both sit above a pane that shares the
// window's height with them, and both keep the divider inside the same
// limits. Extracted when the second builder arrived, rather than copied.
import { useCallback, useEffect, useRef } from "react";

import { clampPaneSize, MIN_BUILDER_PANEL_HEIGHT, MIN_RESPONSE_HEIGHT } from "@/lib/split-pane";

interface BuilderPanelHeight {
  panelHeight: number;
  responseCollapsed: boolean;
  /** Receives already-clamped heights only. */
  onPanelHeightChange: (height: number) => void;
}

export function useBuilderPanelHeight({
  panelHeight,
  responseCollapsed,
  onPanelHeightChange,
}: BuilderPanelHeight) {
  const panelRef = useRef<HTMLDivElement>(null);

  const applyHeight = useCallback(
    (desired: number) => {
      const panel = panelRef.current;
      onPanelHeightChange(
        clampPaneSize(desired, {
          // Space from the top of the panel to the bottom of the area it
          // shares with the pane below. The app is one h-screen column with
          // nothing below that pane, so the window bottom *is* that edge. Add
          // a status bar down there and this has to become a measured
          // container instead.
          //
          // The panel's top does not move while dragging — the rows above it
          // are fixed — so reading it mid-drag is stable.
          available: panel
            ? window.innerHeight - panel.getBoundingClientRect().top
            : desired + MIN_RESPONSE_HEIGHT,
          minStart: MIN_BUILDER_PANEL_HEIGHT,
          minEnd: MIN_RESPONSE_HEIGHT,
        }),
      );
    },
    [onPanelHeightChange],
  );

  // Shrinking the window can make a stored height illegal, which would push
  // the lower pane off the bottom with no way back. Re-clamping on resize is
  // what stops the divider being a one-way trip.
  useEffect(() => {
    // Pointless while the lower pane is collapsed: the panel is flex-sized
    // then, and re-clamping would fight the layout rather than help it.
    if (responseCollapsed) {
      return;
    }
    function reclamp() {
      applyHeight(panelHeight);
    }
    window.addEventListener("resize", reclamp);
    return () => window.removeEventListener("resize", reclamp);
  }, [applyHeight, panelHeight, responseCollapsed]);

  return { panelRef, applyHeight };
}

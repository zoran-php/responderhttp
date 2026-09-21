// http_client/src/lib/split-pane.ts
//
// The arithmetic behind the app's draggable dividers. Pure, so it can be
// tested without a DOM — the components above it only turn pointer events into
// a desired size and paint the result (CLAUDE.md §6).
//
// One function serves both dividers. A horizontal split and a vertical one are
// the same problem with the axis renamed: clamp a desired size between a
// minimum for the pane being sized and whatever is left after the other pane
// keeps its own minimum. Two copies of that would drift (§7, DRY).

/** What `h-64` was before the divider existed, so the app opens unchanged. */
export const DEFAULT_BUILDER_PANEL_HEIGHT = 256;

/** Roughly a table header plus two rows: below this the panel is not usable. */
export const MIN_BUILDER_PANEL_HEIGHT = 96;

/** Enough for the response's own status bar and a line or two of body. */
export const MIN_RESPONSE_HEIGHT = 120;

/** `w-64`, the sidebar's width before it could be dragged. */
export const DEFAULT_SIDEBAR_WIDTH = 256;

/**
 * The sidebar cannot usefully be narrower than it already is, so the divider
 * only widens it. The floor is not taste: the panel tab row is three `flex-1`
 * buttons reading COLLECTIONS / ENVIRONMENTS / HISTORY at 11px uppercase with
 * `tracking-wide`, and they already fill 256px. Narrower clips them.
 *
 * Collapsing to nothing is the answer to "I want this out of the way", and it
 * is a separate control.
 */
export const MIN_SIDEBAR_WIDTH = 256;

/**
 * Left for the request builder when the sidebar is dragged wide. The method
 * select, the URL field and the cURL / Cookies / Save / Send row need real
 * room; a fixed cap on the sidebar instead would be wrong on every monitor
 * except the one it was chosen on.
 */
export const MIN_BUILDER_WIDTH = 640;

/** Half a typical window: the documentation split opens even. */
export const DEFAULT_DOCS_EDITOR_WIDTH = 520;

/**
 * Narrower than this and Markdown stops being editable — a fenced block or a
 * table wraps at every second word. The same floor serves the preview, which
 * has the same problem for the same reason.
 */
export const MIN_DOCS_PANE_WIDTH = 280;

export interface PaneBounds {
  /**
   * Distance from the start of the pane being sized to the end of the area the
   * two panes share. Deliberately not "the container size": the pane is not
   * always the only thing before the other one, so a container size alone
   * cannot say how much room it may take.
   */
  available: number;
  /** Floor for the pane being sized. */
  minStart: number;
  /** Floor for whatever sits after it. */
  minEnd: number;
}

/**
 * The size the pane should actually get, given where the user dragged to.
 *
 * Fractional input is expected — a pointer delta is not integral on a scaled
 * display — and is rounded here rather than at the call site, so the value in
 * the store is always a whole pixel.
 */
export function clampPaneSize(desired: number, bounds: PaneBounds): number {
  const upper = bounds.available - bounds.minEnd;

  // Too small to honour both minimums at once. The pane that comes *after*
  // wins: a cramped sidebar or request panel is annoying, but a response you
  // cannot see at all is the thing the window was opened for.
  if (upper <= bounds.minStart) {
    return Math.max(0, Math.round(upper));
  }

  return Math.round(Math.min(Math.max(desired, bounds.minStart), upper));
}

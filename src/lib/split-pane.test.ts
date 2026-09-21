// http_client/src/lib/split-pane.test.ts
import { describe, expect, it } from "vitest";

import {
  clampPaneSize,
  DEFAULT_BUILDER_PANEL_HEIGHT,
  MIN_BUILDER_PANEL_HEIGHT,
  MIN_BUILDER_WIDTH,
  MIN_RESPONSE_HEIGHT,
  MIN_SIDEBAR_WIDTH,
  type PaneBounds,
} from "@/lib/split-pane";

const BOUNDS: PaneBounds = {
  available: 600,
  minStart: MIN_BUILDER_PANEL_HEIGHT,
  minEnd: MIN_RESPONSE_HEIGHT,
};

describe("clampPaneSize (heights)", () => {
  it("passes a height that fits through untouched", () => {
    expect(clampPaneSize(300, BOUNDS)).toBe(300);
  });

  it("will not let the panel collapse past its minimum", () => {
    expect(clampPaneSize(10, BOUNDS)).toBe(MIN_BUILDER_PANEL_HEIGHT);
    expect(clampPaneSize(-400, BOUNDS)).toBe(MIN_BUILDER_PANEL_HEIGHT);
  });

  /** Dragging down must never push the response off the bottom of the window. */
  it("leaves the response its minimum no matter how far the drag goes", () => {
    expect(clampPaneSize(10_000, BOUNDS)).toBe(600 - MIN_RESPONSE_HEIGHT);
  });

  it("rounds, because a pointer delta on a scaled display is not integral", () => {
    expect(clampPaneSize(300.6, BOUNDS)).toBe(301);
  });

  /**
   * A window short enough that both minimums cannot hold. The response keeps
   * its share and the panel takes what is left — the opposite choice would
   * hide the response entirely, which is the one thing the split exists to
   * show.
   */
  it("gives the response its minimum first when the window is too short", () => {
    const cramped: PaneBounds = { ...BOUNDS, available: 160 };

    expect(clampPaneSize(300, cramped)).toBe(40);
  });

  it("never returns a negative height, however short the window", () => {
    const tiny: PaneBounds = { ...BOUNDS, available: 20 };

    expect(clampPaneSize(300, tiny)).toBe(0);
  });

  it("opens at the height the panel had before the divider existed", () => {
    expect(clampPaneSize(DEFAULT_BUILDER_PANEL_HEIGHT, BOUNDS)).toBe(256);
  });
});

describe("clampPaneSize (widths)", () => {
  const BOUNDS: PaneBounds = {
    available: 1200,
    minStart: MIN_SIDEBAR_WIDTH,
    minEnd: MIN_BUILDER_WIDTH,
  };

  /** The same function, the other axis. That is the point of the rename. */
  it("widens the sidebar up to whatever the builder does not need", () => {
    expect(clampPaneSize(400, BOUNDS)).toBe(400);
    expect(clampPaneSize(10_000, BOUNDS)).toBe(1200 - MIN_BUILDER_WIDTH);
  });

  /** The sidebar's floor is its panel tab row, not taste. */
  it("never narrows the sidebar below the width its tab row needs", () => {
    expect(clampPaneSize(100, BOUNDS)).toBe(MIN_SIDEBAR_WIDTH);
  });

  /** At the app's minimum window width there is almost no room to drag. */
  it("gives the builder its minimum first in a window at the size floor", () => {
    const narrow: PaneBounds = { ...BOUNDS, available: 1000 };

    expect(clampPaneSize(500, narrow)).toBe(360);
  });
});

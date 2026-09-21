// http_client/src/store/layout-store.ts
//
// Where panes sit and whether they are showing, as opposed to what is in them.
// Session-only by decision (2026-09-15): everything here returns to its default
// on restart. Persisting it would mean a ui_state table, a migration, a
// repository and a command for a handful of numbers — worth doing once there is
// more layout state than this, and not before.
//
// Separate from request-store.ts because these are global preferences, not
// per-tab request data: dragging a divider on one tab moves it on all of them,
// which is what every editor with a split view does.
import { create } from "zustand";

import {
  DEFAULT_BUILDER_PANEL_HEIGHT,
  DEFAULT_DOCS_EDITOR_WIDTH,
  DEFAULT_SIDEBAR_WIDTH,
} from "@/lib/split-pane";

interface LayoutState {
  /** Height in pixels of the Params/Auth/Headers/Body/Settings panel. */
  builderPanelHeight: number;
  /** Width in pixels of the collections/environments/history sidebar. */
  sidebarWidth: number;
  sidebarCollapsed: boolean;
  responseCollapsed: boolean;
  /** Width in pixels of the editor half of a Docs tab's split view. Global
   * like the others: dragging it on one Docs tab moves it on all of them. */
  docsEditorWidth: number;

  setBuilderPanelHeight: (height: number) => void;
  resetBuilderPanelHeight: () => void;
  setSidebarWidth: (width: number) => void;
  resetSidebarWidth: () => void;
  setDocsEditorWidth: (width: number) => void;
  resetDocsEditorWidth: () => void;
  toggleSidebar: () => void;
  toggleResponse: () => void;
}

export const useLayoutStore = create<LayoutState>((set) => ({
  builderPanelHeight: DEFAULT_BUILDER_PANEL_HEIGHT,
  sidebarWidth: DEFAULT_SIDEBAR_WIDTH,
  sidebarCollapsed: false,
  responseCollapsed: false,
  docsEditorWidth: DEFAULT_DOCS_EDITOR_WIDTH,

  setBuilderPanelHeight: (builderPanelHeight) => set({ builderPanelHeight }),
  resetBuilderPanelHeight: () => set({ builderPanelHeight: DEFAULT_BUILDER_PANEL_HEIGHT }),
  setSidebarWidth: (sidebarWidth) => set({ sidebarWidth }),
  resetSidebarWidth: () => set({ sidebarWidth: DEFAULT_SIDEBAR_WIDTH }),
  setDocsEditorWidth: (docsEditorWidth) => set({ docsEditorWidth }),
  resetDocsEditorWidth: () => set({ docsEditorWidth: DEFAULT_DOCS_EDITOR_WIDTH }),

  // Collapsing keeps the dragged width rather than resetting it, so restoring
  // puts the pane back where the user left it — the whole point of a collapse
  // toggle as opposed to dragging it shut.
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  toggleResponse: () => set((state) => ({ responseCollapsed: !state.responseCollapsed })),
}));

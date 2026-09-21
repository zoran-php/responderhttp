// http_client/src/store/docs-tab.test.ts
//
// The Docs tab half of the tabs store (PLAN.md Phase 12). Kept in its own
// file rather than appended to request-store.test.ts: these tests need the
// docs service mocked and fake timers for the autosave, and neither should
// apply to the request-tab tests.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DOCS_AUTOSAVE_MS, useTabsStore, type DocsTab } from "@/store/request-store";
import type { DocsTarget } from "@/types/docs";

const itemDocs = vi.hoisted(() => vi.fn());
const setItemDocs = vi.hoisted(() => vi.fn());

vi.mock("@/services/docs", () => ({ itemDocs, setItemDocs }));

const collection: DocsTarget = { kind: "collection", id: "col_1" };
const request: DocsTarget = { kind: "request", id: "req_1" };

function docsTabs(): DocsTab[] {
  return useTabsStore.getState().tabs.filter((tab): tab is DocsTab => tab.kind === "docs");
}

function onlyDocsTab(): DocsTab {
  const tabs = docsTabs();
  expect(tabs).toHaveLength(1);
  const [only] = tabs;
  if (only === undefined) {
    throw new Error("expected exactly one docs tab");
  }
  return only;
}

/** Lets the mocked service's already-resolved promise run its `.then`. */
async function settle(): Promise<void> {
  await vi.waitFor(() => {
    expect(itemDocs).toHaveBeenCalled();
  });
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  vi.useFakeTimers();
  itemDocs.mockReset().mockResolvedValue({ ok: true, value: "" });
  setItemDocs.mockReset().mockResolvedValue({ ok: true, value: undefined });
  for (const tab of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(tab.id);
  }
  // After the teardown, not before it: closing a tab flushes its pending
  // save, so a dirty tab left by the previous test would otherwise show up
  // here as a call this test did not make.
  setItemDocs.mockClear();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("openDocs", () => {
  it("opens a tab for the item and makes it active", () => {
    useTabsStore.getState().openDocs(collection);

    const tab = onlyDocsTab();
    expect(tab.target).toEqual(collection);
    expect(useTabsStore.getState().activeTabId).toBe(tab.id);
  });

  /** The spec's single-instance rule. */
  it("focuses the tab already open for that item rather than opening a second", () => {
    useTabsStore.getState().openDocs(collection);
    const first = onlyDocsTab();
    useTabsStore.getState().openBlankTab();

    useTabsStore.getState().openDocs(collection);

    expect(docsTabs()).toHaveLength(1);
    expect(useTabsStore.getState().activeTabId).toBe(first.id);
  });

  /** Two items can share an id only by accident, but a folder and a request
   * are different documents either way. */
  it("treats different items as different tabs", () => {
    useTabsStore.getState().openDocs(collection);
    useTabsStore.getState().openDocs(request);

    expect(docsTabs()).toHaveLength(2);
  });

  it("fills the tab with what storage holds", async () => {
    itemDocs.mockResolvedValue({ ok: true, value: "# Overview" });

    useTabsStore.getState().openDocs(collection);
    await settle();

    const tab = onlyDocsTab();
    expect(tab.markdown).toBe("# Overview");
    expect(tab.savedMarkdown).toBe("# Overview");
    expect(tab.loaded).toBe(true);
    expect(useTabsStore.getState().isDirty(tab.id)).toBe(false);
  });

  /**
   * The fetch is slower than a fast typist, and the window between opening a
   * tab and its text arriving is a race with a destructive outcome: a
   * keystroke landing first would leave a one-character draft that autosave
   * then writes over documentation the user has never seen.
   *
   * The tab is read-only until the text arrives — the editor enforces it and
   * the store enforces it again — so the stored text wins and nothing is
   * lost, because there was nowhere to type.
   */
  it("ignores typing that arrives before the stored text does", async () => {
    let resolveFetch: (value: { ok: true; value: string }) => void = () => {};
    itemDocs.mockReturnValue(
      new Promise<{ ok: true; value: string }>((resolve) => {
        resolveFetch = resolve;
      }),
    );

    useTabsStore.getState().openDocs(collection);
    const tab = onlyDocsTab();
    useTabsStore.getState().setDocsMarkdown(tab.id, "typed too early");
    expect(onlyDocsTab().markdown).toBe("");

    resolveFetch({ ok: true, value: "documentation already written" });
    await Promise.resolve();
    await Promise.resolve();

    expect(onlyDocsTab().markdown).toBe("documentation already written");
    expect(setItemDocs).not.toHaveBeenCalled();
  });

  it("surfaces a failed load rather than showing an empty document", async () => {
    itemDocs.mockResolvedValue({
      ok: false,
      error: { kind: "storage", message: "database is locked" },
    });

    useTabsStore.getState().openDocs(collection);
    await settle();

    expect(onlyDocsTab().saveError).toBe("database is locked");
  });
});

describe("autosave", () => {
  it("writes once the typing stops", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();

    useTabsStore.getState().setDocsMarkdown(tab.id, "some notes");
    expect(setItemDocs).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(DOCS_AUTOSAVE_MS);

    expect(setItemDocs).toHaveBeenCalledWith(collection, "some notes");
    expect(onlyDocsTab().savedMarkdown).toBe("some notes");
  });

  /** A sentence should be one write, not forty. */
  it("coalesces a burst of keystrokes into one write", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();

    for (const text of ["a", "ab", "abc"]) {
      useTabsStore.getState().setDocsMarkdown(tab.id, text);
      await vi.advanceTimersByTimeAsync(DOCS_AUTOSAVE_MS / 4);
    }
    await vi.advanceTimersByTimeAsync(DOCS_AUTOSAVE_MS);

    expect(setItemDocs).toHaveBeenCalledTimes(1);
    expect(setItemDocs).toHaveBeenCalledWith(collection, "abc");
  });

  it("marks the tab dirty until the write lands", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();

    useTabsStore.getState().setDocsMarkdown(tab.id, "unsaved");
    expect(useTabsStore.getState().isDirty(tab.id)).toBe(true);

    await vi.advanceTimersByTimeAsync(DOCS_AUTOSAVE_MS);

    expect(useTabsStore.getState().isDirty(tab.id)).toBe(false);
  });

  it("keeps the tab dirty and says why when the write fails", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();
    setItemDocs.mockResolvedValue({
      ok: false,
      error: { kind: "invalidRequest", message: "too long" },
    });

    useTabsStore.getState().setDocsMarkdown(tab.id, "x");
    await vi.advanceTimersByTimeAsync(DOCS_AUTOSAVE_MS);

    expect(useTabsStore.getState().isDirty(tab.id)).toBe(true);
    expect(onlyDocsTab().saveError).toBe("too long");
  });

  it("writes nothing when the text has not changed", async () => {
    itemDocs.mockResolvedValue({ ok: true, value: "same" });
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();

    useTabsStore.getState().setDocsMarkdown(tab.id, "same");
    await vi.advanceTimersByTimeAsync(DOCS_AUTOSAVE_MS);

    expect(setItemDocs).not.toHaveBeenCalled();
  });

  /** Ctrl+S should mean something rather than waiting out the debounce. */
  it("saveDocs writes immediately and cancels the pending autosave", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();

    useTabsStore.getState().setDocsMarkdown(tab.id, "now please");
    await useTabsStore.getState().saveDocs(tab.id);

    expect(setItemDocs).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(DOCS_AUTOSAVE_MS * 2);

    expect(setItemDocs).toHaveBeenCalledTimes(1);
  });

  it("flushes when the tab is left for another", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();
    const other = useTabsStore.getState().openBlankTab();

    useTabsStore.getState().setActiveTab(tab.id);
    useTabsStore.getState().setDocsMarkdown(tab.id, "written on the way out");
    useTabsStore.getState().setActiveTab(other);
    await vi.advanceTimersByTimeAsync(0);

    expect(setItemDocs).toHaveBeenCalledWith(collection, "written on the way out");
  });

  it("flushes when the tab is closed", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();

    useTabsStore.getState().setDocsMarkdown(tab.id, "written on the way out");
    useTabsStore.getState().closeTab(tab.id);
    await vi.advanceTimersByTimeAsync(0);

    expect(setItemDocs).toHaveBeenCalledWith(collection, "written on the way out");
  });

  /** Nothing has been fetched yet, so an empty draft is not evidence the
   * user cleared the documentation. Writing it would erase what is there. */
  it("never writes a tab whose text has not arrived", async () => {
    itemDocs.mockReturnValue(new Promise(() => {}));
    useTabsStore.getState().openDocs(collection);
    const tab = onlyDocsTab();

    await useTabsStore.getState().saveDocs(tab.id);

    expect(setItemDocs).not.toHaveBeenCalled();
  });
});

describe("setDocsView", () => {
  it("opens in split view and switches on request", async () => {
    useTabsStore.getState().openDocs(collection);
    await settle();
    const tab = onlyDocsTab();
    expect(tab.view).toBe("split");

    useTabsStore.getState().setDocsView(tab.id, "preview");

    expect(onlyDocsTab().view).toBe("preview");
  });
});

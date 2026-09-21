// http_client/src/store/request-store.test.ts
import { beforeEach, describe, expect, it } from "vitest";

import { useTabsStore } from "@/store/request-store";
import type { SavedRequest } from "@/types/collections";
import { AUTH_NONE, DEFAULT_SETTINGS, type SendRequestInput } from "@/types/http";

const requestBody: SendRequestInput = {
  method: "GET",
  url: "https://example.com/users",
  headers: [],
  queryParams: [],
  body: { kind: "none" },
  auth: AUTH_NONE,
  settings: DEFAULT_SETTINGS,
};

function savedRequest(overrides: Partial<SavedRequest> = {}): SavedRequest {
  return {
    id: "req_1",
    collectionId: "col_1",
    folderId: null,
    name: "List users",
    request: requestBody,
    secretState: "ok",
    ...overrides,
  };
}

/** Every test starts from exactly one blank tab. closeTab always leaves at
 * least one tab open, so closing everything that exists gets there for
 * free without reaching into the store's internals. */
beforeEach(() => {
  for (const tab of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(tab.id);
  }
});

describe("openBlankTab", () => {
  it("adds a new tab and makes it active", () => {
    const before = useTabsStore.getState().tabs.length;

    const id = useTabsStore.getState().openBlankTab();

    expect(useTabsStore.getState().tabs).toHaveLength(before + 1);
    expect(useTabsStore.getState().activeTabId).toBe(id);
  });
});

describe("openSavedRequest", () => {
  it("opens a new tab loaded with the request", () => {
    useTabsStore.getState().openSavedRequest(savedRequest());

    const tab = useTabsStore.getState().activeRequestTab();
    expect(tab?.loadedRequest).toEqual({
      id: "req_1",
      collectionId: "col_1",
      folderId: null,
      name: "List users",
    });
    expect(tab?.url).toBe("https://example.com/users");
  });

  it("focuses the existing tab instead of opening a duplicate", () => {
    useTabsStore.getState().openSavedRequest(savedRequest());
    const firstTabId = useTabsStore.getState().activeTabId;
    useTabsStore.getState().openBlankTab();
    const countBeforeReopening = useTabsStore.getState().tabs.length;

    useTabsStore.getState().openSavedRequest(savedRequest());

    expect(useTabsStore.getState().activeTabId).toBe(firstTabId);
    expect(useTabsStore.getState().tabs).toHaveLength(countBeforeReopening);
  });
});

describe("closeTab", () => {
  it("removing a non-active tab leaves the active tab untouched", () => {
    const first = useTabsStore.getState().activeTabId;
    const second = useTabsStore.getState().openBlankTab();
    useTabsStore.getState().setActiveTab(first);

    useTabsStore.getState().closeTab(second);

    expect(useTabsStore.getState().activeTabId).toBe(first);
    expect(useTabsStore.getState().tabs.map((tab) => tab.id)).toEqual([first]);
  });

  it("removing the active tab activates a neighbor", () => {
    const first = useTabsStore.getState().activeTabId;
    const second = useTabsStore.getState().openBlankTab();

    useTabsStore.getState().closeTab(second);

    expect(useTabsStore.getState().activeTabId).toBe(first);
  });

  it("closing the last tab leaves one fresh blank tab open", () => {
    const onlyTabId = useTabsStore.getState().activeTabId;

    useTabsStore.getState().closeTab(onlyTabId);

    const state = useTabsStore.getState();
    expect(state.tabs).toHaveLength(1);
    expect(state.tabs[0]?.id).not.toBe(onlyTabId);
    expect(state.activeTabId).toBe(state.tabs[0]?.id);
  });
});

describe("dirty tracking", () => {
  it("a freshly opened saved request is not dirty", () => {
    useTabsStore.getState().openSavedRequest(savedRequest());

    expect(useTabsStore.getState().isDirty(useTabsStore.getState().activeTabId)).toBe(false);
  });

  it("editing a field marks the active tab dirty", () => {
    useTabsStore.getState().openSavedRequest(savedRequest());

    useTabsStore.getState().setUrl("https://example.com/users?page=2");

    expect(useTabsStore.getState().isDirty(useTabsStore.getState().activeTabId)).toBe(true);
  });

  it("markSaved clears the dirty flag and records the new location", () => {
    useTabsStore.getState().openSavedRequest(savedRequest());
    useTabsStore.getState().setUrl("https://example.com/users?page=2");
    const activeId = useTabsStore.getState().activeTabId;

    useTabsStore.getState().markSaved({
      id: "req_1",
      collectionId: "col_1",
      folderId: "fld_1",
      name: "List users (v2)",
    });

    expect(useTabsStore.getState().isDirty(activeId)).toBe(false);
    expect(useTabsStore.getState().activeRequestTab()?.loadedRequest).toEqual({
      id: "req_1",
      collectionId: "col_1",
      folderId: "fld_1",
      name: "List users (v2)",
    });
  });

  it("a never-saved blank tab is not dirty until something is typed", () => {
    useTabsStore.getState().openBlankTab();

    expect(useTabsStore.getState().isDirty(useTabsStore.getState().activeTabId)).toBe(false);

    useTabsStore.getState().setUrl("https://example.com");

    expect(useTabsStore.getState().isDirty(useTabsStore.getState().activeTabId)).toBe(true);
  });
});

describe("openHistoryEntry", () => {
  it("opens the request in a new, unsaved tab", () => {
    useTabsStore.getState().openHistoryEntry(requestBody);

    const tab = useTabsStore.getState().activeRequestTab();
    expect(tab?.url).toBe("https://example.com/users");
    // Saving it has to ask where it goes: a history entry is not a stored
    // request, so Save must not overwrite anything.
    expect(tab?.loadedRequest).toBeNull();
  });

  /** Re-running from history must never replace what the user is looking
   * at, even if the same request is already open. */
  it("always opens another tab rather than focusing an existing one", () => {
    useTabsStore.getState().openHistoryEntry(requestBody);
    const before = useTabsStore.getState().tabs.length;

    useTabsStore.getState().openHistoryEntry(requestBody);

    expect(useTabsStore.getState().tabs).toHaveLength(before + 1);
  });

  /** Glancing at an entry and closing it again should not prompt. */
  it("arrives clean, since nothing has been edited yet", () => {
    useTabsStore.getState().openHistoryEntry(requestBody);

    const id = useTabsStore.getState().activeTabId;
    expect(useTabsStore.getState().isDirty(id)).toBe(false);
  });
});

describe("secret state", () => {
  it("carries a saved request's secret state into its tab", () => {
    useTabsStore.getState().openSavedRequest(savedRequest({ secretState: "needsReentry" }));

    expect(useTabsStore.getState().activeRequestTab()?.secretState).toBe("needsReentry");
  });

  it("typing a replacement answers needs-reentry", () => {
    useTabsStore.getState().openSavedRequest(savedRequest({ secretState: "needsReentry" }));

    useTabsStore.getState().setAuth({ kind: "bearer", token: "new" });

    expect(useTabsStore.getState().activeRequestTab()?.secretState).toBe("ok");
  });

  it("an unreachable credential store stays reported until a save succeeds", () => {
    useTabsStore.getState().openSavedRequest(savedRequest({ secretState: "unavailable" }));

    useTabsStore.getState().setAuth({ kind: "bearer", token: "new" });
    expect(useTabsStore.getState().activeRequestTab()?.secretState).toBe("unavailable");

    useTabsStore.getState().markSaved({
      id: "req_1",
      collectionId: "col_1",
      folderId: null,
      name: "List users",
    });
    expect(useTabsStore.getState().activeRequestTab()?.secretState).toBe("ok");
  });

  it("a history entry opens with nothing to report", () => {
    useTabsStore.getState().openHistoryEntry(requestBody);

    expect(useTabsStore.getState().activeRequestTab()?.secretState).toBe("ok");
  });
});

describe("URL and Params stay in step", () => {
  function active() {
    const tab = useTabsStore.getState().activeRequestTab();
    if (tab === null) {
      throw new Error("expected a request tab");
    }
    return tab;
  }

  function paramPairs() {
    return active()
      .paramRows.filter((row) => row.name !== "" || row.value !== "")
      .map((row) => ({ name: row.name, value: row.value }));
  }

  it("typing a query in the URL fills the table", () => {
    useTabsStore.getState().setUrl("https://x.test/s?q=cats&page=2");

    expect(paramPairs()).toEqual([
      { name: "q", value: "cats" },
      { name: "page", value: "2" },
    ]);
    // Always a blank row to type into.
    expect(active().paramRows[active().paramRows.length - 1]).toMatchObject({
      name: "",
      value: "",
    });
  });

  it("keeps row ids while the URL is being typed", () => {
    useTabsStore.getState().setUrl("https://x.test/s?q=c");
    const before = active().paramRows[0]?.id;

    useTabsStore.getState().setUrl("https://x.test/s?q=ca");

    expect(active().paramRows[0]?.id).toBe(before);
    expect(paramPairs()).toEqual([{ name: "q", value: "ca" }]);
  });

  it("editing the table rewrites the URL's query and keeps the rest", () => {
    useTabsStore.getState().setUrl("https://x.test/s?old=1#top");
    const [first, blank] = active().paramRows;
    if (!first || !blank) {
      throw new Error("expected a row and a blank row");
    }

    useTabsStore.getState().setParamRows([
      { ...first, value: "2" },
      { ...blank, name: "q", value: "a b" },
    ]);

    expect(active().url).toBe("https://x.test/s?old=2&q=a b#top");
  });

  it("removing the last row removes the ?", () => {
    useTabsStore.getState().setUrl("https://x.test/s?q=1");

    useTabsStore.getState().setParamRows([]);

    expect(active().url).toBe("https://x.test/s");
    expect(active().paramRows).toHaveLength(1);
  });

  it("never sends or saves the table separately from the URL", () => {
    useTabsStore.getState().setUrl("https://x.test/s?q=1");

    const input = useTabsStore.getState().currentInput();

    expect(input.url).toBe("https://x.test/s?q=1");
    expect(input.queryParams).toEqual([]);
  });

  it("folds an old request's Params rows into the URL and opens clean", () => {
    useTabsStore.getState().openSavedRequest(
      savedRequest({
        request: {
          ...requestBody,
          url: "https://example.com/users?page=2",
          queryParams: [{ name: "q", value: "a b" }],
        },
      }),
    );

    expect(active().url).toBe("https://example.com/users?page=2&q=a%20b");
    expect(paramPairs()).toEqual([
      { name: "page", value: "2" },
      { name: "q", value: "a%20b" },
    ]);
    expect(useTabsStore.getState().currentInput().queryParams).toEqual([]);
    expect(useTabsStore.getState().isDirty(useTabsStore.getState().activeTabId)).toBe(false);
  });
});

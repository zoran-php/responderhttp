// http_client/src/store/request-store.ts
//
// Session state for every open request tab (Postman-style). Each tab holds
// what request-store.ts used to hold for the single request that existed
// before: what the user typed, and what came back. The work itself belongs
// to services/.
//
// File kept at its original path/name to avoid an orphaned file while the
// device link that would let us delete one is down; the export is now
// useTabsStore rather than useRequestStore.
//
// Field setters (setMethod, setUrl, ...) always act on the active tab — the
// builder only ever edits the tab currently on screen — so RequestBuilder
// and ResponseViewer need no changes beyond reading from the active tab in
// App.tsx.
//
// Tabs are in-memory only and reset to a single blank tab on relaunch
// (decided 2026-09-13, see PLAN.md) — persisting the open-tab layout across
// restarts would need a new migration and repository, out of scope for this
// addition to Phase 3.
import { create } from "zustand";

import {
  emptyRow,
  rowsFromKeyValues,
  rowsKeepingIds,
  toKeyValues,
  withTrailingBlank,
  type KeyValueRow,
} from "@/lib/key-values";
import {
  emptyMultipartRow,
  rowsFromMultipartParts,
  toMultipartParts,
  withTrailingBlankPart,
  type MultipartRow,
} from "@/lib/multipart-rows";
import { foldLegacyParams, queryPairs, withQueryPairs } from "@/lib/query-sync";
import { emptyRequestInput, requestInputsEqual } from "@/lib/request-defaults";
import { substituteRequestInput } from "@/lib/variables";
// Reading the active environment here, rather than threading it through
// every caller, keeps `currentInput()` the raw template that Save stores.
import { useEnvironmentsStore } from "@/store/environments-store";
import { useHistoryStore } from "@/store/history-store";
import { cancelRequest, sendAndDownload, sendRequest } from "@/services/http-client";
import type { SavedRequest } from "@/types/collections";
import {
  AUTH_NONE,
  DEFAULT_SETTINGS,
  type ApiError,
  type Auth,
  type DownloadResult,
  type HttpMethod,
  type HttpResponse,
  type RequestBody,
  type RequestSettings,
  type SecretState,
  type SendRequestInput,
} from "@/types/http";

export type BodyKind = RequestBody["kind"];
type RequestStatus = "idle" | "sending";

/** What ties a tab's fields back to a saved request, if any — enough for
 * Save to overwrite in place instead of asking where to save. */
export interface LoadedRequestRef {
  id: string;
  collectionId: string;
  folderId: string | null;
  name: string;
}

export interface RequestTab {
  kind: "request";
  id: string;

  method: HttpMethod;
  url: string;
  headerRows: KeyValueRow[];
  /** A view of `url`'s query string, kept in step both ways (see
   * lib/query-sync.ts). Never sent or saved on its own. */
  paramRows: KeyValueRow[];
  bodyKind: BodyKind;
  auth: Auth;
  /**
   * Whether the auth secret loaded from storage. Anything but "ok" means the
   * field arrived empty; the Auth tab says why (PLAN.md Phase 9).
   */
  secretState: SecretState;
  rawContentType: string;
  rawText: string;
  formRows: KeyValueRow[];
  multipartRows: MultipartRow[];
  settings: RequestSettings;

  status: RequestStatus;
  response: HttpResponse | null;
  /** Set instead of `response` when the body went to a file. The two are
   * mutually exclusive: a download has no body to render. */
  download: DownloadResult | null;
  error: ApiError | null;
  /**
   * The request that produced `response`, captured at send time, as an
   * example will keep it. Saving an example has to record what was actually
   * sent, not what is in the fields now — the user may well have edited them
   * since. Resolved except for secret variables, which stay as
   * `{{placeholders}}` so no secret reaches the example (PLAN.md Phase 9).
   */
  sentRequest: SendRequestInput | null;

  /** Set once the tab has been saved or loaded from a collection; null for
   * a request that has never been saved. */
  loadedRequest: LoadedRequestRef | null;
  /** The request shape as last saved or loaded, so dirty-checking (isDirty)
   * doesn't need to inspect every field individually. */
  savedSnapshot: SendRequestInput;
}

/**
 * An environment opened for editing. It holds only the id: the name and the
 * variables live in the environments store, so a rename made in the sidebar
 * shows up on the tab without this having to be kept in step.
 */
export interface EnvironmentTab {
  kind: "environment";
  id: string;
  environmentId: string;
}

/**
 * A saved response opened for reading. Holds only the id, like
 * EnvironmentTab: the example itself lives in the collections store, so a
 * rename in the sidebar shows on the tab without this being kept in step.
 */
export interface ExampleTab {
  kind: "example";
  id: string;
  exampleId: string;
}

export type Tab = RequestTab | EnvironmentTab | ExampleTab;

interface TabsState {
  tabs: Tab[];
  activeTabId: string;

  activeTab: () => Tab;
  /** Null while an environment tab is in front — the builder is not rendered then. */
  activeRequestTab: () => RequestTab | null;
  isDirty: (tabId: string) => boolean;

  openBlankTab: () => string;
  /** Focuses the tab already open for this saved request, if there is one;
   * otherwise opens a new tab loaded with it. */
  openSavedRequest: (saved: SavedRequest) => void;
  openEnvironment: (environmentId: string) => void;
  openExample: (exampleId: string) => void;
  /** Opens a history entry's request in a new, unsaved tab. Always a new
   * tab: the entry is a record of something already sent, so re-running it
   * must not overwrite whatever the user has in front of them. */
  openHistoryEntry: (request: SendRequestInput) => void;
  setActiveTab: (id: string) => void;
  /** Always leaves at least one tab open — closing the last one opens a
   * fresh blank tab rather than leaving the builder with nothing to show. */
  closeTab: (id: string) => void;

  setMethod: (method: HttpMethod) => void;
  setUrl: (url: string) => void;
  setHeaderRows: (rows: KeyValueRow[]) => void;
  setParamRows: (rows: KeyValueRow[]) => void;
  setBodyKind: (kind: BodyKind) => void;
  setRawContentType: (contentType: string) => void;
  setRawText: (text: string) => void;
  setFormRows: (rows: KeyValueRow[]) => void;
  setMultipartRows: (rows: MultipartRow[]) => void;
  setAuth: (auth: Auth) => void;
  setSettings: (patch: Partial<RequestSettings>) => void;

  currentInput: () => SendRequestInput;
  send: () => Promise<void>;
  /** Same request, but the body is written to a file the user picks rather
   * than rendered. Cancellable through the same path as send. */
  sendAndDownload: () => Promise<void>;
  cancel: () => void;

  /** Records that the active tab's current fields now match what's in
   * storage — called right after Save succeeds. Clears the dirty flag and
   * updates the tab's name/location without touching any builder field. */
  markSaved: (loaded: LoadedRequestRef) => void;
}

let nextRequestId = 0;

/** Only has to be unique among requests this session can cancel. */
function newRequestId(): string {
  nextRequestId += 1;
  return `req-${nextRequestId}`;
}

let nextTabId = 0;

function newTabId(): string {
  nextTabId += 1;
  return `tab-${nextTabId}`;
}

function buildBlankTab(): RequestTab {
  return {
    kind: "request",
    id: newTabId(),
    method: "GET",
    url: "",
    headerRows: [emptyRow()],
    paramRows: [emptyRow()],
    bodyKind: "none",
    auth: AUTH_NONE,
    secretState: "ok",
    rawContentType: "application/json",
    rawText: "",
    formRows: [emptyRow()],
    multipartRows: [emptyMultipartRow()],
    settings: DEFAULT_SETTINGS,
    status: "idle",
    response: null,
    download: null,
    error: null,
    sentRequest: null,
    loadedRequest: null,
    savedSnapshot: emptyRequestInput(),
  };
}

/**
 * The shape a tab works with: every query parameter lives in the URL. A
 * request saved before the Params tab became a view of the URL has its rows
 * folded in here, so it opens, sends and saves exactly as before — and its
 * next save stores the new shape.
 */
function withParamsInUrl(request: SendRequestInput): SendRequestInput {
  if (request.queryParams.length === 0) {
    return request;
  }
  return {
    ...request,
    url: foldLegacyParams(request.url, request.queryParams, request.settings.encodeUrl),
    queryParams: [],
  };
}

/** A tab holding this request and nothing else — not tied to any stored
 * row, so Save asks where to put it. */
function buildTabFromRequest(stored: SendRequestInput): RequestTab {
  const request = withParamsInUrl(stored);
  return {
    kind: "request",
    id: newTabId(),
    method: request.method,
    url: request.url,
    headerRows: rowsFromKeyValues(request.headers),
    paramRows: rowsFromKeyValues(queryPairs(request.url)),
    auth: request.auth,
    secretState: "ok",
    settings: request.settings,
    ...bodyFieldsFromRequestBody(request.body),
    status: "idle",
    response: null,
    download: null,
    error: null,
    sentRequest: null,
    loadedRequest: null,
    // Clean on arrival: the tab matches what it was opened with, so
    // closing it without touching anything does not prompt. Save still
    // asks where to put it, since loadedRequest is null.
    savedSnapshot: request,
  };
}

function buildTabFromSaved(saved: SavedRequest): RequestTab {
  const { request } = saved;
  return {
    ...buildTabFromRequest(request),
    secretState: saved.secretState,
    loadedRequest: {
      id: saved.id,
      collectionId: saved.collectionId,
      folderId: saved.folderId,
      name: saved.name,
    },
  };
}

/** An invariant helper, not a domain lookup: every call site only reaches
 * here with an index it just computed against a known-non-empty array. A
 * thrown error (rather than a silent fallback) is deliberate — if this ever
 * fires, a caller's bookkeeping is wrong and hiding it would be worse. */
function requireTab(tabs: Tab[], index: number): Tab {
  const tab = tabs[index];
  if (!tab) {
    throw new Error(`request-store: expected a tab at index ${index}`);
  }
  return tab;
}

function findActive(state: Pick<TabsState, "tabs" | "activeTabId">): Tab {
  const found = state.tabs.find((tab) => tab.id === state.activeTabId);
  return found ?? requireTab(state.tabs, 0);
}

function findActiveRequest(state: Pick<TabsState, "tabs" | "activeTabId">): RequestTab | null {
  const found = state.tabs.find((tab) => tab.id === state.activeTabId);
  return found !== undefined && found.kind === "request" ? found : null;
}

/**
 * Patches the active tab only when it is a request tab, which makes every
 * field setter a no-op while an environment tab is in front instead of each
 * one needing its own guard.
 */
function replaceActiveTab(tabs: Tab[], activeTabId: string, patch: Partial<RequestTab>): Tab[] {
  return tabs.map((tab) =>
    tab.id === activeTabId && tab.kind === "request" ? { ...tab, ...patch } : tab,
  );
}

const initialTab = buildBlankTab();

export const useTabsStore = create<TabsState>((set, get) => ({
  tabs: [initialTab],
  activeTabId: initialTab.id,

  activeTab: () => findActive(get()),

  activeRequestTab: () => findActiveRequest(get()),

  isDirty: (tabId) => {
    const tab = get().tabs.find((candidate) => candidate.id === tabId);
    // An environment tab saves explicitly, so it is never "unsaved".
    if (tab === undefined || tab.kind !== "request") {
      return false;
    }
    return !requestInputsEqual(inputFromTab(tab), tab.savedSnapshot);
  },

  openBlankTab: () => {
    const tab = buildBlankTab();
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
    return tab.id;
  },

  openSavedRequest: (saved) => {
    const existing = get().tabs.find(
      (tab) => tab.kind === "request" && tab.loadedRequest?.id === saved.id,
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return;
    }
    const tab = buildTabFromSaved(saved);
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
  },

  openEnvironment: (environmentId) => {
    const existing = get().tabs.find(
      (tab) => tab.kind === "environment" && tab.environmentId === environmentId,
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return;
    }
    const tab: EnvironmentTab = { kind: "environment", id: newTabId(), environmentId };
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
  },

  openExample: (exampleId) => {
    const existing = get().tabs.find(
      (tab) => tab.kind === "example" && tab.exampleId === exampleId,
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return;
    }
    const tab: ExampleTab = { kind: "example", id: newTabId(), exampleId };
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
  },

  openHistoryEntry: (request) => {
    const tab = buildTabFromRequest(request);
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
  },

  setActiveTab: (id) => set({ activeTabId: id }),

  closeTab: (id) => {
    set((state) => {
      const index = state.tabs.findIndex((tab) => tab.id === id);
      if (index === -1) {
        return state;
      }
      const remaining = state.tabs.filter((tab) => tab.id !== id);
      if (remaining.length === 0) {
        const fresh = buildBlankTab();
        return { tabs: [fresh], activeTabId: fresh.id };
      }
      if (state.activeTabId !== id) {
        return { tabs: remaining, activeTabId: state.activeTabId };
      }
      const nextIndex = Math.min(index, remaining.length - 1);
      return { tabs: remaining, activeTabId: requireTab(remaining, nextIndex).id };
    });
  },

  setMethod: (method) =>
    set((state) => ({ tabs: replaceActiveTab(state.tabs, state.activeTabId, { method }) })),
  // The URL and the Params tab are one thing shown twice. Typing in the URL
  // re-reads the table from it; the table is never written back from here,
  // so what the user is typing is never rewritten under the cursor.
  setUrl: (url) =>
    set((state) => {
      const active = findActiveRequest(state);
      if (active === null) {
        return {};
      }
      return {
        tabs: replaceActiveTab(state.tabs, state.activeTabId, {
          url,
          paramRows: rowsKeepingIds(active.paramRows, queryPairs(url)),
        }),
      };
    }),
  setHeaderRows: (headerRows) =>
    set((state) => ({
      tabs: replaceActiveTab(state.tabs, state.activeTabId, {
        headerRows: withTrailingBlank(headerRows),
      }),
    })),
  // …and editing the table rewrites the URL's query from the rows.
  setParamRows: (paramRows) =>
    set((state) => {
      const active = findActiveRequest(state);
      if (active === null) {
        return {};
      }
      const rows = withTrailingBlank(paramRows);
      return {
        tabs: replaceActiveTab(state.tabs, state.activeTabId, {
          paramRows: rows,
          url: withQueryPairs(active.url, rows),
        }),
      };
    }),
  setBodyKind: (bodyKind) =>
    set((state) => ({ tabs: replaceActiveTab(state.tabs, state.activeTabId, { bodyKind }) })),
  setRawContentType: (rawContentType) =>
    set((state) => ({
      tabs: replaceActiveTab(state.tabs, state.activeTabId, { rawContentType }),
    })),
  setRawText: (rawText) =>
    set((state) => ({ tabs: replaceActiveTab(state.tabs, state.activeTabId, { rawText }) })),
  setFormRows: (formRows) =>
    set((state) => ({
      tabs: replaceActiveTab(state.tabs, state.activeTabId, {
        formRows: withTrailingBlank(formRows),
      }),
    })),
  setMultipartRows: (multipartRows) =>
    set((state) => ({
      tabs: replaceActiveTab(state.tabs, state.activeTabId, {
        multipartRows: withTrailingBlankPart(multipartRows),
      }),
    })),
  setAuth: (auth) =>
    set((state) => {
      const active = findActiveRequest(state);
      // Typing a replacement answers "needs re-entering". "unavailable" stays:
      // saving is still refused until the credential store can be reached.
      const secretState =
        active?.secretState === "needsReentry" ? "ok" : (active?.secretState ?? "ok");
      return {
        tabs: replaceActiveTab(state.tabs, state.activeTabId, { auth, secretState }),
      };
    }),

  setSettings: (patch) =>
    set((state) => {
      const active = findActiveRequest(state);
      if (active === null) {
        return {};
      }
      return {
        tabs: replaceActiveTab(state.tabs, state.activeTabId, {
          settings: { ...active.settings, ...patch },
        }),
      };
    }),

  currentInput: () => {
    const active = findActiveRequest(get());
    return active === null ? emptyRequestInput() : inputFromTab(active);
  },

  send: async () => {
    const activeTabId = get().activeTabId;
    const active = findActiveRequest(get());
    if (active === null || active.status === "sending") {
      return;
    }
    const id = newRequestId();
    inFlightByTab.set(activeTabId, id);
    // Clear the previous outcome so a slow request cannot leave a stale
    // response on screen next to a spinner.
    set((state) => ({
      tabs: replaceActiveTab(state.tabs, activeTabId, {
        status: "sending",
        response: null,
        download: null,
        error: null,
        sentRequest: null,
      }),
    }));

    const template = get().currentInput();
    const variables = useEnvironmentsStore.getState().activeVariables();
    const resolved = substituteRequestInput(template, variables);
    const snapshot = substituteRequestInput(template, variables, { leaveSecrets: true });
    const startedAt = performance.now();
    const result = await sendRequest(id, resolved, snapshot.url);

    set((state) => ({
      tabs: replaceActiveTab(state.tabs, activeTabId, {
        status: "idle",
        response: result.ok ? result.value : null,
        error: result.ok ? null : result.error,
        sentRequest: result.ok ? snapshot : null,
      }),
    }));
    inFlightByTab.delete(activeTabId);

    // Recorded from here rather than inside send_request so the stored
    // request is the template the user typed: substitution happens in the
    // frontend, so Rust only ever sees the resolved version. The cost is
    // that a crash between the response landing and this call loses the
    // entry — acceptable for a log that exists to be re-run from.
    void useHistoryStore.getState().recordEntry({
      // With secret variables left as placeholders: this column is stored in
      // plain text, and history keeps no secrets (PLAN.md Phase 9).
      resolvedUrl: snapshot.url,
      status: result.ok ? result.value.status : null,
      errorKind: result.ok ? null : result.error.kind,
      // libcurl's own total is the truthful number when there is one; a
      // failure has no timing, so the wall clock stands in.
      durationMs: result.ok
        ? result.value.timing.totalMs
        : Math.round(performance.now() - startedAt),
      request: template,
    });
  },

  sendAndDownload: async () => {
    const activeTabId = get().activeTabId;
    const active = findActiveRequest(get());
    if (active === null || active.status === "sending") {
      return;
    }
    const id = newRequestId();
    inFlightByTab.set(activeTabId, id);
    set((state) => ({
      tabs: replaceActiveTab(state.tabs, activeTabId, {
        status: "sending",
        response: null,
        download: null,
        error: null,
        sentRequest: null,
      }),
    }));

    const template = get().currentInput();
    const variables = useEnvironmentsStore.getState().activeVariables();
    const resolved = substituteRequestInput(template, variables);
    const snapshot = substituteRequestInput(template, variables, { leaveSecrets: true });
    const startedAt = performance.now();
    // The Save As dialog opens inside this call, after the response lands —
    // which is what lets it suggest a name from Content-Disposition, and why
    // this promise stays pending while the dialog is on screen.
    const result = await sendAndDownload(id, resolved, snapshot.url);

    set((state) => ({
      tabs: replaceActiveTab(state.tabs, activeTabId, {
        status: "idle",
        download: result.ok ? result.value : null,
        error: result.ok ? null : result.error,
      }),
    }));
    inFlightByTab.delete(activeTabId);

    void useHistoryStore.getState().recordEntry({
      // With secret variables left as placeholders: this column is stored in
      // plain text, and history keeps no secrets (PLAN.md Phase 9).
      resolvedUrl: snapshot.url,
      status: result.ok ? result.value.status : null,
      errorKind: result.ok ? null : result.error.kind,
      durationMs: result.ok
        ? result.value.timing.totalMs
        : Math.round(performance.now() - startedAt),
      request: template,
    });
  },

  cancel: () => {
    const id = inFlightByTab.get(get().activeTabId);
    if (id) {
      void cancelRequest(id);
    }
  },

  markSaved: (loaded) =>
    set((state) => {
      const active = findActiveRequest(state);
      if (active === null) {
        return {};
      }
      return {
        tabs: replaceActiveTab(state.tabs, state.activeTabId, {
          loadedRequest: loaded,
          savedSnapshot: inputFromTab(active),
          // A save that succeeded wrote whatever the field holds now.
          secretState: "ok",
        }),
      };
    }),
}));

/**
 * Outside the store: per-tab transport bookkeeping, not something any
 * component renders, and keeping it out of state avoids a re-render per
 * send. Keyed by tab id so cancelling one tab's request can't cancel
 * another's.
 */
const inFlightByTab = new Map<string, string>();

function inputFromTab(tab: RequestTab): SendRequestInput {
  return {
    method: tab.method,
    url: tab.url.trim(),
    headers: toKeyValues(tab.headerRows),
    // Already in the URL: the table is a view of it.
    queryParams: [],
    body: currentBody(tab),
    auth: tab.auth,
    settings: tab.settings,
  };
}

function currentBody(tab: RequestTab): RequestBody {
  switch (tab.bodyKind) {
    case "none":
      return { kind: "none" };
    case "raw":
      return { kind: "raw", contentType: tab.rawContentType, text: tab.rawText };
    case "formUrlEncoded":
      return { kind: "formUrlEncoded", fields: toKeyValues(tab.formRows) };
    case "multipart":
      return { kind: "multipart", parts: toMultipartParts(tab.multipartRows) };
  }
}

/** The inverse of currentBody, for buildTabFromSaved. Only one body kind is
 * active at a time, so loading a request resets the other kinds' rows to
 * blank rather than trying to preserve them. */
function bodyFieldsFromRequestBody(
  body: RequestBody,
): Pick<RequestTab, "bodyKind" | "rawContentType" | "rawText" | "formRows" | "multipartRows"> {
  switch (body.kind) {
    case "none":
      return {
        bodyKind: "none",
        rawContentType: "application/json",
        rawText: "",
        formRows: [emptyRow()],
        multipartRows: [emptyMultipartRow()],
      };
    case "raw":
      return {
        bodyKind: "raw",
        rawContentType: body.contentType,
        rawText: body.text,
        formRows: [emptyRow()],
        multipartRows: [emptyMultipartRow()],
      };
    case "formUrlEncoded":
      return {
        bodyKind: "formUrlEncoded",
        rawContentType: "application/json",
        rawText: "",
        formRows: rowsFromKeyValues(body.fields),
        multipartRows: [emptyMultipartRow()],
      };
    case "multipart":
      return {
        bodyKind: "multipart",
        rawContentType: "application/json",
        rawText: "",
        formRows: [emptyRow()],
        multipartRows: rowsFromMultipartParts(body.parts),
      };
  }
}

// http_client/src/store/request-store.ts
//
// Session state for every open request tab. Each tab holds
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
import {
  substituteMessage,
  substituteRequestInput,
  substituteWebSocketRequest,
} from "@/lib/variables";
import {
  appendCapped,
  clearAll,
  EMPTY_WS_LOG,
  logEntries,
  takePendingSend,
  type PendingSend,
  type WsLog,
  type WsLogFilter,
} from "@/lib/ws-log";
import { beautify, isBeautifiable } from "@/lib/beautify";
import { applyStreamEvents, EMPTY_STREAM, type ResponseStream } from "@/lib/sse-log";
import { encodeOutgoing } from "@/lib/ws-payload";
import type { WsConnectionState } from "@/lib/ws-status";
import { webSocketShapesEqual, type WebSocketShape } from "@/lib/ws-request";
// Reading the active environment here, rather than threading it through
// every caller, keeps `currentInput()` the raw template that Save stores.
import { useEnvironmentsStore } from "@/store/environments-store";
import { useHistoryStore } from "@/store/history-store";
import { itemDocs, setItemDocs } from "@/services/docs";
import { cancelRequest, sendAndDownload, sendRequest } from "@/services/http-client";
import { connectWebSocket, disconnectWebSocket, sendWebSocketMessage } from "@/services/websocket";
import type { SavedRequest, SavedWebSocket } from "@/types/collections";
import { sameDocsTarget, type DocsTarget } from "@/types/docs";
import {
  AUTH_NONE,
  DEFAULT_SETTINGS,
  type ApiError,
  type Auth,
  type DownloadResult,
  type HttpMethod,
  type HttpResponse,
  type HttpStreamEvent,
  type RequestBody,
  type RequestSettings,
  type SecretState,
  type SendRequestInput,
} from "@/types/http";
import {
  DEFAULT_WS_SETTINGS,
  EMPTY_WS_DRAFT,
  type WebSocketSettings,
  type WsDraft,
  type WsEvent,
  type WsPayload,
} from "@/types/websocket";

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
  /**
   * What the in-flight (or last) request reported before it finished: its
   * status and headers as soon as they arrived, and, for a
   * `text/event-stream` response, every block as it was parsed (PLAN-SSE.md,
   * 14d). Reset at the start of every send.
   */
  stream: ResponseStream;

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

/** Which pane a Docs tab is showing. */
export type DocsView = "edit" | "preview" | "split";

/**
 * An item's Markdown documentation, open for editing (PLAN.md Phase 12).
 *
 * Unlike EnvironmentTab and ExampleTab this carries its own text, because the
 * editor needs a draft that is not yet what storage holds. It still holds
 * only the *target* rather than the item's name, so renaming a collection in
 * the sidebar retitles its Docs tab for free — the same reason the other two
 * hold only an id.
 */
export interface DocsTab {
  kind: "docs";
  id: string;
  target: DocsTarget;
  /** What the editor shows. */
  markdown: string;
  /** What storage last confirmed, so dirty is a string comparison. */
  savedMarkdown: string;
  view: DocsView;
  /** False until the first fetch lands; the editor stays read-only until then
   * so a keystroke cannot be overwritten by the arriving text. */
  loaded: boolean;
  /** A save that failed, shown rather than swallowed (CLAUDE.md section 7). */
  saveError: string | null;
}

/**
 * A WebSocket request (PLAN.md Phase 13e). The store owns the connection's
 * lifetime: closing the tab disconnects it, so leaving a tab never leaks a
 * socket.
 */
export interface WebSocketTab {
  kind: "websocket";
  id: string;

  url: string;
  headerRows: KeyValueRow[];
  /** A view of `url`'s query string, as on a request tab. */
  paramRows: KeyValueRow[];
  settings: WebSocketSettings;
  draft: WsDraft;

  connection: WsConnectionState;
  /** Set while reconnecting, for the badge's "n/N". */
  reconnectAttempt: { attempt: number; maxAttempts: number } | null;
  /** The live connection, or the most recent one. Null before the first
   * connect. */
  connectionId: string | null;
  log: WsLog;
  logFilter: WsLogFilter;
  logQuery: string;
  /** Why the last Send did not go: hex that does not parse, or a refusal. */
  composerError: string | null;

  loadedRequest: LoadedRequestRef | null;
  savedSnapshot: WebSocketShape;
}

export type Tab = RequestTab | EnvironmentTab | ExampleTab | DocsTab | WebSocketTab;

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
  /** Focuses the Docs tab already open for this item, if there is one;
   * otherwise opens one and fetches its text. */
  openDocs: (target: DocsTarget) => void;
  setDocsMarkdown: (tabId: string, markdown: string) => void;
  setDocsView: (tabId: string, view: DocsView) => void;
  /** Writes a Docs tab now, cancelling any pending autosave for it. Awaited
   * by tests; callers in the UI fire and forget. */
  saveDocs: (tabId: string) => Promise<void>;
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

  // WebSocket tabs. Like the request setters, these act on the active tab
  // and do nothing while it is not a WebSocket tab.
  activeWebSocketTab: () => WebSocketTab | null;
  openBlankWebSocketTab: () => string;
  /** Focuses the tab already open for this saved WebSocket, if there is one;
   * otherwise opens a new tab loaded with it. */
  openSavedWebSocket: (saved: SavedWebSocket) => void;
  /** Ignored unless disconnected: Params is a view of the URL, and the URL
   * of a live connection cannot change under it. */
  setWsUrl: (url: string) => void;
  setWsParamRows: (rows: KeyValueRow[]) => void;
  setWsHeaderRows: (rows: KeyValueRow[]) => void;
  setWsSettings: (patch: Partial<WebSocketSettings>) => void;
  setWsDraft: (patch: Partial<WsDraft>) => void;
  /** Reformats the draft (JSON, XML, HTML). Text that does not parse is left
   * exactly as typed, and the reason shows as the composer's error. */
  beautifyWsDraft: () => void;
  /** What Save writes: the template, `{{placeholders}}` intact. */
  currentWebSocketShape: () => WebSocketShape;
  connect: () => Promise<void>;
  /** Also cancels a handshake or a reconnect in progress. */
  disconnect: () => void;
  sendMessage: () => Promise<void>;
  /** Empties the log: every entry from every connection. Leaves the
   * connection, the draft, the filter and the search alone. */
  clearMessages: () => void;
  setLogFilter: (filter: WsLogFilter) => void;
  setLogQuery: (query: string) => void;
  markWebSocketSaved: (loaded: LoadedRequestRef) => void;
}

let nextRequestId = 0;

/** Only has to be unique among requests this session can cancel. */
function newRequestId(): string {
  nextRequestId += 1;
  return `req-${nextRequestId}`;
}

let nextConnectionId = 0;

/** Minted here, before the handshake, so Disconnect can cancel a connect
 * that has not finished. */
function newConnectionId(): string {
  nextConnectionId += 1;
  return `ws-${nextConnectionId}`;
}

let nextTabId = 0;

function newTabId(): string {
  nextTabId += 1;
  return `tab-${nextTabId}`;
}

/**
 * How long after the last keystroke a Docs tab writes itself.
 *
 * Long enough that a sentence is one write rather than forty, short enough
 * that the dot on the tab never sits there long enough to worry anyone.
 */
export const DOCS_AUTOSAVE_MS = 800;

/**
 * Pending autosaves, by tab id.
 *
 * Module-level rather than in the store because a timer handle is not state
 * anything renders — putting it in the store would mean every keystroke
 * published a new object to every subscriber for no visible reason.
 */
const pendingDocsSaves = new Map<string, ReturnType<typeof setTimeout>>();

function scheduleDocsSave(tabId: string, save: () => void): void {
  cancelDocsSave(tabId);
  pendingDocsSaves.set(tabId, setTimeout(save, DOCS_AUTOSAVE_MS));
}

function cancelDocsSave(tabId: string): void {
  const pending = pendingDocsSaves.get(tabId);
  if (pending !== undefined) {
    clearTimeout(pending);
    pendingDocsSaves.delete(tabId);
  }
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
    stream: EMPTY_STREAM,
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
    stream: EMPTY_STREAM,
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

function buildBlankWebSocketTab(): WebSocketTab {
  return {
    kind: "websocket",
    id: newTabId(),
    url: "",
    headerRows: [emptyRow()],
    paramRows: [emptyRow()],
    settings: DEFAULT_WS_SETTINGS,
    draft: EMPTY_WS_DRAFT,
    connection: "idle",
    reconnectAttempt: null,
    connectionId: null,
    log: EMPTY_WS_LOG,
    logFilter: "all",
    logQuery: "",
    composerError: null,
    loadedRequest: null,
    savedSnapshot: shapeFromTabFields("", [], DEFAULT_WS_SETTINGS, EMPTY_WS_DRAFT),
  };
}

function buildWebSocketTabFromSaved(saved: SavedWebSocket): WebSocketTab {
  const { request, draft } = saved;
  return {
    ...buildBlankWebSocketTab(),
    url: request.url,
    headerRows: rowsFromKeyValues(request.headers),
    paramRows: rowsFromKeyValues(queryPairs(request.url)),
    settings: request.settings,
    draft,
    loadedRequest: {
      id: saved.id,
      collectionId: saved.collectionId,
      folderId: saved.folderId,
      name: saved.name,
    },
    savedSnapshot: { request, draft },
  };
}

function shapeFromTabFields(
  url: string,
  headers: WebSocketShape["request"]["headers"],
  settings: WebSocketSettings,
  draft: WsDraft,
): WebSocketShape {
  return { request: { url: url.trim(), headers, settings }, draft };
}

function webSocketShapeFromTab(tab: WebSocketTab): WebSocketShape {
  return shapeFromTabFields(tab.url, toKeyValues(tab.headerRows), tab.settings, tab.draft);
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

function findActiveWebSocket(state: Pick<TabsState, "tabs" | "activeTabId">): WebSocketTab | null {
  const found = state.tabs.find((tab) => tab.id === state.activeTabId);
  return found !== undefined && found.kind === "websocket" ? found : null;
}

/** Patches one WebSocket tab, by id. Events arrive for a tab whether or not
 * it is in front, so unlike the request setters this is not "active only". */
function updateWebSocketTab(
  tabs: Tab[],
  tabId: string,
  update: (tab: WebSocketTab) => Partial<WebSocketTab>,
): Tab[] {
  return tabs.map((tab) =>
    tab.id === tabId && tab.kind === "websocket" ? { ...tab, ...update(tab) } : tab,
  );
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

/**
 * The same, for a patch built from the tab's own previous state. A stream
 * arrives in batches, each folded into what the last one left — and by a
 * tab id rather than the active tab, because the user is free to switch
 * tabs while a request is still running.
 */
function updateRequestTab(
  tabs: Tab[],
  tabId: string,
  patch: (tab: RequestTab) => Partial<RequestTab>,
): Tab[] {
  return tabs.map((tab) =>
    tab.id === tabId && tab.kind === "request" ? { ...tab, ...patch(tab) } : tab,
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
    if (tab === undefined) {
      return false;
    }
    // A Docs tab autosaves, so this is true only for the moment between a
    // keystroke and the write landing — which is exactly what the dot on the
    // tab is for. A failed save keeps it lit.
    if (tab.kind === "docs") {
      return tab.markdown !== tab.savedMarkdown;
    }
    if (tab.kind === "websocket") {
      return !webSocketShapesEqual(webSocketShapeFromTab(tab), tab.savedSnapshot);
    }
    // An environment tab saves explicitly, so it is never "unsaved".
    if (tab.kind !== "request") {
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

  openDocs: (target) => {
    const existing = get().tabs.find(
      (tab) => tab.kind === "docs" && sameDocsTarget(tab.target, target),
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return;
    }

    const tab: DocsTab = {
      kind: "docs",
      id: newTabId(),
      target,
      markdown: "",
      savedMarkdown: "",
      view: "split",
      loaded: false,
      saveError: null,
    };
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));

    void itemDocs(target).then((result) => {
      set((state) => ({
        tabs: state.tabs.map((candidate) => {
          // Only fills a tab that is still empty and unloaded: the fetch is
          // slower than a fast typist, and text that arrives late must not
          // overwrite what was typed while it was in flight.
          if (candidate.id !== tab.id || candidate.kind !== "docs" || candidate.loaded) {
            return candidate;
          }
          if (!result.ok) {
            return { ...candidate, loaded: true, saveError: result.error.message };
          }
          return {
            ...candidate,
            markdown: result.value,
            savedMarkdown: result.value,
            loaded: true,
          };
        }),
      }));
    });
  },

  setDocsMarkdown: (tabId, markdown) => {
    const tab = get().tabs.find((candidate) => candidate.id === tabId);
    // Ignored until the stored text has arrived. The editor is read-only for
    // that moment, so nothing typed is lost here — and without this rule a
    // keystroke landing first would autosave an empty draft over
    // documentation the user has not seen yet.
    if (tab === undefined || tab.kind !== "docs" || !tab.loaded) {
      return;
    }

    set((state) => ({
      tabs: state.tabs.map((candidate) =>
        candidate.id === tabId && candidate.kind === "docs"
          ? { ...candidate, markdown, saveError: null }
          : candidate,
      ),
    }));
    scheduleDocsSave(tabId, () => void get().saveDocs(tabId));
  },

  setDocsView: (tabId, view) => {
    set((state) => ({
      tabs: state.tabs.map((tab) =>
        tab.id === tabId && tab.kind === "docs" ? { ...tab, view } : tab,
      ),
    }));
  },

  saveDocs: async (tabId) => {
    cancelDocsSave(tabId);
    const tab = get().tabs.find((candidate) => candidate.id === tabId);
    if (tab === undefined || tab.kind !== "docs" || !tab.loaded) {
      return;
    }
    if (tab.markdown === tab.savedMarkdown) {
      return;
    }

    // Captured before the await: the user keeps typing while the write is in
    // flight, and what was written is what may be marked saved.
    const written = tab.markdown;
    const result = await setItemDocs(tab.target, written);

    set((state) => ({
      tabs: state.tabs.map((candidate) => {
        if (candidate.id !== tabId || candidate.kind !== "docs") {
          return candidate;
        }
        if (!result.ok) {
          return { ...candidate, saveError: result.error.message };
        }
        return { ...candidate, savedMarkdown: written, saveError: null };
      }),
    }));
  },

  openHistoryEntry: (request) => {
    const tab = buildTabFromRequest(request);
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
  },

  setActiveTab: (id) => {
    // Leaving a Docs tab is a save point: the user has stopped looking at it,
    // and waiting out the debounce would mean a write landing after they have
    // moved on.
    const leaving = get().activeTabId;
    if (leaving !== id) {
      void get().saveDocs(leaving);
    }
    set({ activeTabId: id });
  },

  closeTab: (id) => {
    // Flushed before the tab goes: the pending write holds the tab's id, and
    // once it is gone there is nothing for the save to write back into.
    // Fire-and-forget, because closing must not wait on a disk write.
    void get().saveDocs(id);

    // A tab's connection ends with the tab. Its last events find no tab and
    // are dropped; the connection's bookkeeping goes with its `closed`.
    const closingTab = get().tabs.find((tab) => tab.id === id);
    if (
      closingTab?.kind === "websocket" &&
      closingTab.connection !== "idle" &&
      closingTab.connectionId !== null
    ) {
      void disconnectWebSocket(closingTab.connectionId);
    }

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
        stream: EMPTY_STREAM,
      }),
    }));

    const template = get().currentInput();
    const variables = useEnvironmentsStore.getState().activeVariables();
    const resolved = substituteRequestInput(template, variables);
    const snapshot = substituteRequestInput(template, variables, { leaveSecrets: true });
    const startedAt = performance.now();
    // Batched by the service: one update a frame however fast the stream is.
    const onStream = (events: HttpStreamEvent[]): void => {
      set((state) => ({
        tabs: updateRequestTab(state.tabs, activeTabId, (tab) => ({
          stream: applyStreamEvents(tab.stream, events),
        })),
      }));
    };
    const result = await sendRequest(id, resolved, snapshot.url, onStream);

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
        stream: EMPTY_STREAM,
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

  activeWebSocketTab: () => findActiveWebSocket(get()),

  openBlankWebSocketTab: () => {
    const tab = buildBlankWebSocketTab();
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
    return tab.id;
  },

  openSavedWebSocket: (saved) => {
    const existing = get().tabs.find(
      (tab) => tab.kind === "websocket" && tab.loadedRequest?.id === saved.id,
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return;
    }
    const tab = buildWebSocketTabFromSaved(saved);
    set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id }));
  },

  setWsUrl: (url) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      if (active === null || active.connection !== "idle") {
        return {};
      }
      return {
        tabs: updateWebSocketTab(state.tabs, active.id, (tab) => ({
          url,
          paramRows: rowsKeepingIds(tab.paramRows, queryPairs(url)),
        })),
      };
    }),

  setWsParamRows: (paramRows) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      if (active === null || active.connection !== "idle") {
        return {};
      }
      const rows = withTrailingBlank(paramRows);
      return {
        tabs: updateWebSocketTab(state.tabs, active.id, (tab) => ({
          paramRows: rows,
          url: withQueryPairs(tab.url, rows),
        })),
      };
    }),

  // Headers, settings and the draft stay editable while connected; they
  // apply from the next connect (assumption 10).
  setWsHeaderRows: (headerRows) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      return active === null
        ? {}
        : {
            tabs: updateWebSocketTab(state.tabs, active.id, () => ({
              headerRows: withTrailingBlank(headerRows),
            })),
          };
    }),

  setWsSettings: (patch) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      return active === null
        ? {}
        : {
            tabs: updateWebSocketTab(state.tabs, active.id, (tab) => ({
              settings: { ...tab.settings, ...patch },
            })),
          };
    }),

  setWsDraft: (patch) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      return active === null
        ? {}
        : {
            tabs: updateWebSocketTab(state.tabs, active.id, (tab) => ({
              draft: { ...tab.draft, ...patch },
              composerError: null,
            })),
          };
    }),

  beautifyWsDraft: () =>
    set((state) => {
      const active = findActiveWebSocket(state);
      if (active === null || !isBeautifiable(active.draft.format)) {
        return {};
      }
      const result = beautify(active.draft.format, active.draft.text);
      return {
        tabs: updateWebSocketTab(state.tabs, active.id, (tab) =>
          result.ok
            ? { draft: { ...tab.draft, text: result.text }, composerError: null }
            : { composerError: result.reason },
        ),
      };
    }),

  currentWebSocketShape: () => {
    const active = findActiveWebSocket(get());
    return active === null
      ? shapeFromTabFields("", [], DEFAULT_WS_SETTINGS, EMPTY_WS_DRAFT)
      : webSocketShapeFromTab(active);
  },

  connect: async () => {
    const active = findActiveWebSocket(get());
    if (active === null || active.connection !== "idle") {
      return;
    }
    const tabId = active.id;
    const connectionId = newConnectionId();
    set((state) => ({
      tabs: updateWebSocketTab(state.tabs, tabId, () => ({
        connection: "connecting",
        connectionId,
        reconnectAttempt: null,
      })),
    }));

    const template = webSocketShapeFromTab(active).request;
    const variables = useEnvironmentsStore.getState().activeVariables();
    const resolved = substituteWebSocketRequest(template, variables);
    // What the log and the log file show: a secret variable's value never
    // reaches either (PLAN.md Phase 9).
    const display = substituteWebSocketRequest(template, variables, { leaveSecrets: true });
    displayUrlByConnection.set(connectionId, display.url);

    const result = await connectWebSocket(connectionId, resolved, display.url, (events) => {
      const shown = events.map((event) => forDisplay(connectionId, event));
      set((state) => ({
        tabs: updateWebSocketTab(state.tabs, tabId, (tab) => applyEvents(tab, connectionId, shown)),
      }));
      if (events.some((event) => event.type === "closed")) {
        forgetConnection(connectionId);
      }
    });
    if (result.ok) {
      return;
    }

    // A failed handshake reports nothing on the channel, so its row in the
    // log is written here.
    const failure: WsEvent = {
      type: "error",
      atMs: Date.now(),
      message:
        result.error.kind === "cancelled"
          ? "Connection cancelled"
          : `Could not connect: ${result.error.message}`,
    };
    set((state) => ({
      tabs: updateWebSocketTab(state.tabs, tabId, (tab) => ({
        log: appendCapped(tab.log, logEntries(connectionId, [failure])),
        ...(tab.connectionId === connectionId
          ? { connection: "idle" as const, reconnectAttempt: null }
          : {}),
      })),
    }));
    forgetConnection(connectionId);
  },

  disconnect: () => {
    const active = findActiveWebSocket(get());
    if (
      active === null ||
      active.connectionId === null ||
      active.connection === "idle" ||
      active.connection === "disconnecting"
    ) {
      return;
    }
    const { connectionId } = active;
    set((state) => ({
      tabs: updateWebSocketTab(state.tabs, active.id, () => ({ connection: "disconnecting" })),
    }));
    void disconnectWebSocket(connectionId);
  },

  sendMessage: async () => {
    const active = findActiveWebSocket(get());
    if (active === null || active.connection !== "connected" || active.connectionId === null) {
      return;
    }
    const { connectionId, draft } = active;
    const variables = useEnvironmentsStore.getState().activeVariables();
    const outgoing = encodeOutgoing(
      draft.format,
      draft.binaryEncoding,
      substituteMessage(draft.text, variables),
    );
    if (!outgoing.ok) {
      set((state) => ({
        tabs: updateWebSocketTab(state.tabs, active.id, () => ({ composerError: outgoing.error })),
      }));
      return;
    }

    const pending: PendingSend = {
      resolved: outgoing.payload,
      display: displayPayload(
        draft,
        substituteMessage(draft.text, variables, { leaveSecrets: true }),
      ),
    };
    pendingSendsByConnection.set(connectionId, [
      ...(pendingSendsByConnection.get(connectionId) ?? []),
      pending,
    ]);
    set((state) => ({
      tabs: updateWebSocketTab(state.tabs, active.id, () => ({ composerError: null })),
    }));

    const result = await sendWebSocketMessage(connectionId, outgoing.payload);
    if (!result.ok) {
      const queued = pendingSendsByConnection.get(connectionId);
      if (queued !== undefined) {
        pendingSendsByConnection.set(
          connectionId,
          queued.filter((entry) => entry !== pending),
        );
      }
      set((state) => ({
        tabs: updateWebSocketTab(state.tabs, active.id, () => ({
          composerError: result.error.message,
        })),
      }));
    }
  },

  clearMessages: () =>
    set((state) => {
      const active = findActiveWebSocket(state);
      return active === null
        ? {}
        : { tabs: updateWebSocketTab(state.tabs, active.id, () => ({ log: clearAll() })) };
    }),

  setLogFilter: (logFilter) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      return active === null
        ? {}
        : { tabs: updateWebSocketTab(state.tabs, active.id, () => ({ logFilter })) };
    }),

  setLogQuery: (logQuery) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      return active === null
        ? {}
        : { tabs: updateWebSocketTab(state.tabs, active.id, () => ({ logQuery })) };
    }),

  markWebSocketSaved: (loaded) =>
    set((state) => {
      const active = findActiveWebSocket(state);
      return active === null
        ? {}
        : {
            tabs: updateWebSocketTab(state.tabs, active.id, (tab) => ({
              loadedRequest: loaded,
              savedSnapshot: webSocketShapeFromTab(tab),
            })),
          };
    }),
}));

/**
 * Per-connection bookkeeping, outside the store for the same reason as
 * `inFlightByTab`: nothing renders it. Both are dropped when the
 * connection's `closed` arrives, or when its connect fails.
 */
const pendingSendsByConnection = new Map<string, PendingSend[]>();
const displayUrlByConnection = new Map<string, string>();

function forgetConnection(connectionId: string): void {
  pendingSendsByConnection.delete(connectionId);
  displayUrlByConnection.delete(connectionId);
}

/**
 * The event as the log should keep it. Rust reports what went over the
 * wire, resolved; the log keeps secret variables as `{{placeholders}}`. A
 * message the server echoes back is the server's data and is shown as it
 * arrived.
 */
function forDisplay(connectionId: string, event: WsEvent): WsEvent {
  if (event.type === "connected") {
    return { ...event, url: displayUrlByConnection.get(connectionId) ?? event.url };
  }
  if (event.type === "sent") {
    const { display, rest } = takePendingSend(
      pendingSendsByConnection.get(connectionId) ?? [],
      event.payload,
    );
    pendingSendsByConnection.set(connectionId, rest);
    return { ...event, payload: display };
  }
  return event;
}

/**
 * The display twin of a message about to be sent. Hex with a secret
 * placeholder in it does not parse as hex, and the secret must still not
 * show, so it is kept as the text that was typed.
 */
function displayPayload(draft: WsDraft, displayText: string): WsPayload {
  const encoded = encodeOutgoing(draft.format, draft.binaryEncoding, displayText);
  return encoded.ok ? encoded.payload : { kind: "text", text: displayText };
}

/**
 * Appends a batch to the log and moves the connection state. Events of an
 * earlier connection, arriving after a new connect, still join the log (they
 * belong to their own session) but no longer move the state.
 */
function applyEvents(
  tab: WebSocketTab,
  connectionId: string,
  events: readonly WsEvent[],
): Partial<WebSocketTab> {
  let { connection, reconnectAttempt } = tab;
  if (tab.connectionId === connectionId) {
    for (const event of events) {
      if (event.type === "connected") {
        connection = "connected";
        reconnectAttempt = null;
      } else if (event.type === "reconnecting") {
        // Disconnect pressed during the wait wins over the next attempt.
        if (connection !== "disconnecting") {
          connection = "reconnecting";
        }
        reconnectAttempt = { attempt: event.attempt, maxAttempts: event.maxAttempts };
      } else if (event.type === "closed") {
        connection = "idle";
        reconnectAttempt = null;
      }
    }
  }
  return {
    connection,
    reconnectAttempt,
    log: appendCapped(tab.log, logEntries(connectionId, events)),
  };
}

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

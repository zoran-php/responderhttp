// http_client/src/App.tsx
//
// Composition only: wires session state to the feature components.
import { useCallback, useEffect, useRef, useState } from "react";

import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ResizeHandle } from "@/components/ResizeHandle";
import { buildCurlCommand } from "@/lib/curl-string-builder";
import { docsTabLabel, docsTargetName } from "@/lib/docs-title";
import { requestPathSegments } from "@/lib/request-path";
import { matchesDocsShortcut, matchesSaveShortcut } from "@/lib/shortcuts";
import { clampPaneSize, MIN_BUILDER_WIDTH, MIN_SIDEBAR_WIDTH } from "@/lib/split-pane";
import { updateMultipartRow, withTrailingBlankPart } from "@/lib/multipart-rows";
import { chooseFile } from "@/services/files";
import { disconnectAllWebSockets } from "@/services/websocket";
import { substituteRequestInput } from "@/lib/variables";
import { SaveRequestDialog, type SavePayload } from "@/features/collections/SaveRequestDialog";
import { DocsEditor } from "@/features/docs/DocsEditor";
import { ExampleViewer } from "@/features/examples/ExampleViewer";
import { SaveExampleDialog } from "@/features/examples/SaveExampleDialog";
import { EnvironmentEditor } from "@/features/environments/EnvironmentEditor";
import { CookieManagerDialog } from "@/features/cookies/CookieManagerDialog";
import { EnvironmentSelector } from "@/features/environments/EnvironmentSelector";
import { RequestBuilder } from "@/features/request-builder/RequestBuilder";
import { TabBar, type TabBarTab } from "@/features/request-builder/TabBar";
import { ResponseViewer } from "@/features/response-viewer/ResponseViewer";
import type { SaveExampleAction } from "@/features/response-viewer/SaveExampleButton";
import { Sidebar } from "@/features/sidebar/Sidebar";
import { WebSocketView } from "@/features/websocket/WebSocketView";
import { useCollectionsStore } from "@/store/collections-store";
import { useEnvironmentsStore } from "@/store/environments-store";
import { useLayoutStore } from "@/store/layout-store";
import { useTabsStore, type RequestTab, type WebSocketTab } from "@/store/request-store";

export default function App() {
  const tabsStore = useTabsStore();
  const environments = useEnvironmentsStore((state) => state.environments);
  const examplesById = useCollectionsStore((state) => state.examplesById);
  const collections = useCollectionsStore((state) => state.collections);
  const contentsById = useCollectionsStore((state) => state.contentsById);
  // In a store rather than in RequestBuilder's own state so the divider keeps
  // its position across a trip to an environment or example tab, which
  // unmounts the builder.
  const builderPanelHeight = useLayoutStore((state) => state.builderPanelHeight);
  const setBuilderPanelHeight = useLayoutStore((state) => state.setBuilderPanelHeight);
  const resetBuilderPanelHeight = useLayoutStore((state) => state.resetBuilderPanelHeight);
  const sidebarWidth = useLayoutStore((state) => state.sidebarWidth);
  const setSidebarWidth = useLayoutStore((state) => state.setSidebarWidth);
  const resetSidebarWidth = useLayoutStore((state) => state.resetSidebarWidth);
  const sidebarCollapsed = useLayoutStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useLayoutStore((state) => state.toggleSidebar);
  const responseCollapsed = useLayoutStore((state) => state.responseCollapsed);
  const toggleResponse = useLayoutStore((state) => state.toggleResponse);
  // Selected on its own rather than off the store object, which would be a
  // new reference every render and re-fire the effect below.
  const loadEnvironments = useEnvironmentsStore((state) => state.loadEnvironments);
  const activeTab = tabsStore.activeTab();
  const requestTab = activeTab.kind === "request" ? activeTab : null;
  const webSocketTab = activeTab.kind === "websocket" ? activeTab : null;

  // The breadcrumb is built here rather than in the builder because the names
  // it needs live in the collections store, and RequestBuilder is props-in,
  // events-out. A tab that has never been saved has no collection and no
  // folders, so this comes back as the bare name.
  const loaded = requestTab?.loadedRequest ?? webSocketTab?.loadedRequest ?? null;
  const pathSegments = requestPathSegments({
    collectionName:
      collections.find((collection) => collection.id === loaded?.collectionId)?.name ?? null,
    folders: (loaded && contentsById[loaded.collectionId]?.folders) || [],
    folderId: loaded?.folderId ?? null,
    requestName: loaded?.name ?? (webSocketTab ? "Untitled WebSocket" : "Untitled Request"),
  });
  const isSending = requestTab?.status === "sending";
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);
  // A tab with unsaved changes asks before closing; a clean one just closes.
  const [closingTabId, setClosingTabId] = useState<string | null>(null);
  const [cookiesOpen, setCookiesOpen] = useState(false);
  const [savingExample, setSavingExample] = useState(false);

  // The environment selector sits in the tab strip and is visible from
  // launch, so the list cannot wait for the sidebar panel to be opened.
  useEffect(() => {
    void loadEnvironments();
  }, [loadEnvironments]);

  // A page that is starting has no connections of its own, so any Rust still
  // holds belong to a page that was reloaded away (PLAN-WEBSOCKET.md 13h).
  useEffect(() => {
    void disconnectAllWebSockets();
  }, []);

  // The sidebar's own width is the whole distance from the window's left edge,
  // so the window width is the space the two panes share.
  const applySidebarWidth = useCallback(
    (desired: number) => {
      setSidebarWidth(
        clampPaneSize(desired, {
          available: window.innerWidth,
          minStart: MIN_SIDEBAR_WIDTH,
          minEnd: MIN_BUILDER_WIDTH,
        }),
      );
    },
    [setSidebarWidth],
  );

  // Shrinking the window can strand a width that no longer leaves the builder
  // its minimum. Same reasoning as the request panel's re-clamp.
  useEffect(() => {
    if (sidebarCollapsed) {
      return;
    }
    function reclamp() {
      applySidebarWidth(sidebarWidth);
    }
    window.addEventListener("resize", reclamp);
    return () => window.removeEventListener("resize", reclamp);
  }, [applySidebarWidth, sidebarWidth, sidebarCollapsed]);

  // Ctrl+S / Cmd+S saves the active request (decided 2026-09-15: scoped by
  // which tab is active, not by where the caret happens to be — the
  // precondition that matters is "the thing on screen is a request", and there
  // is exactly one active tab, so the target is never ambiguous).
  //
  // Listening on window rather than on the builder's subtree is also what
  // makes this survive Monaco: the body editor preventDefaults keys it has
  // bound, but it does not stopPropagation ones it has not, so the event still
  // arrives here.
  //
  // The handler is held in a ref because it closes over the whole tabs store,
  // which is a new object every render; depending on it directly would
  // resubscribe the listener on every keystroke in the URL field.
  const saveAction = useRef(handleSaveClick);
  useEffect(() => {
    saveAction.current = handleSaveClick;
  });

  const canSaveWithShortcut = activeTab.kind === "request" || activeTab.kind === "websocket";
  useEffect(() => {
    if (!canSaveWithShortcut) {
      return;
    }
    function onKeyDown(event: KeyboardEvent) {
      if (!matchesSaveShortcut(event)) {
        return;
      }
      // A modal owns the keyboard while it is open. Queried rather than passed
      // down because these dialogs' open flags live in four different
      // components — including the export dialog, whose state App never sees.
      if (document.querySelector("[data-modal]") !== null) {
        return;
      }
      // Unconditional: a webview may otherwise offer "Save page as".
      event.preventDefault();
      void saveAction.current();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [canSaveWithShortcut]);

  // Ctrl/Cmd+Shift+D opens the documentation for whatever the builder has
  // loaded. "Selected item" means the request in front of the user: the
  // sidebar's own selection is a hover-and-click affair that does not
  // survive a click elsewhere, so binding to it would make the shortcut
  // depend on something invisible.
  const docsTargetForShortcut =
    activeTab.kind === "request" || activeTab.kind === "websocket"
      ? (activeTab.loadedRequest?.id ?? null)
      : null;
  const openDocs = tabsStore.openDocs;
  useEffect(() => {
    if (docsTargetForShortcut === null) {
      return;
    }
    function onKeyDown(event: KeyboardEvent) {
      if (!matchesDocsShortcut(event) || docsTargetForShortcut === null) {
        return;
      }
      if (document.querySelector("[data-modal]") !== null) {
        return;
      }
      event.preventDefault();
      openDocs({ kind: "request", id: docsTargetForShortcut });
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [docsTargetForShortcut, openDocs]);

  async function handleSaveClick() {
    const socket = tabsStore.activeWebSocketTab();
    if (socket !== null) {
      await saveWebSocketTab(socket.loadedRequest);
      return;
    }
    const active = tabsStore.activeRequestTab();
    if (active === null) {
      return;
    }
    const loaded = active.loadedRequest;
    if (!loaded) {
      setSaveDialogOpen(true);
      return;
    }
    // Already saved: overwrite in place rather than asking again. A failure
    // here (e.g. the row was deleted elsewhere) surfaces in the sidebar's
    // error banner, since that is the store this call goes through.
    const saved = await useCollectionsStore.getState().saveRequest({
      id: loaded.id,
      collectionId: loaded.collectionId,
      folderId: loaded.folderId,
      name: loaded.name,
      request: tabsStore.currentInput(),
    });
    if (saved) {
      tabsStore.markSaved({
        id: saved.id,
        collectionId: saved.collectionId,
        folderId: saved.folderId,
        name: saved.name,
      });
    }
  }

  /** The WebSocket half of Save: the dialog for a new one, an overwrite in
   * place for one already saved. */
  async function saveWebSocketTab(loadedSocket: WebSocketTab["loadedRequest"]) {
    if (loadedSocket === null) {
      setSaveDialogOpen(true);
      return;
    }
    const { request, draft } = tabsStore.currentWebSocketShape();
    const saved = await useCollectionsStore.getState().saveWebSocket({
      id: loadedSocket.id,
      collectionId: loadedSocket.collectionId,
      folderId: loadedSocket.folderId,
      name: loadedSocket.name,
      request,
      draft,
    });
    if (saved) {
      tabsStore.markWebSocketSaved({
        id: saved.id,
        collectionId: saved.collectionId,
        folderId: saved.folderId,
        name: saved.name,
      });
    }
  }

  /** What the Save dialog saves: whichever kind of tab is in front. */
  function savePayload(): SavePayload {
    return webSocketTab !== null
      ? { kind: "websocket", shape: tabsStore.currentWebSocketShape() }
      : { kind: "http", request: tabsStore.currentInput() };
  }

  /** An example hangs off a stored request and keeps a text body, so both
   * have to be true before the response can be saved. The reason travels with
   * the refusal so the disabled button can explain itself. */
  function saveExampleAction(tab: RequestTab): SaveExampleAction {
    if (tab.loadedRequest === null) {
      return { disabledReason: "Save the request to a collection first" };
    }
    if (tab.response !== null && tab.response.body.kind === "binary") {
      return { disabledReason: "A binary response cannot be saved as an example" };
    }
    if (tab.sentRequest === null) {
      return { disabledReason: "Send the request first" };
    }
    return { onSave: () => setSavingExample(true) };
  }

  /** Opens the native chooser and writes the result back into that row. A
   * dismissed dialog resolves to null and leaves the row untouched. */
  async function handleChooseMultipartFile(rowId: string) {
    const active = tabsStore.activeRequestTab();
    if (active === null) {
      return;
    }
    const result = await chooseFile();
    if (!result.ok || result.value === null) {
      return;
    }
    const chosen = result.value;
    tabsStore.setMultipartRows(
      withTrailingBlankPart(
        updateMultipartRow(active.multipartRows, rowId, {
          kind: "file",
          path: chosen.path,
          fileName: chosen.fileName,
          contentType: chosen.contentType,
        }),
      ),
    );
  }

  function handleCloseTab(id: string) {
    const tab = tabsStore.tabs.find((candidate) => candidate.id === id);
    // A Docs tab autosaves, and closeTab flushes whatever is still pending,
    // so there is nothing to lose and nothing to ask about. Only a request
    // tab, which saves explicitly, reaches the prompt.
    // A live WebSocket asks too: closing the tab closes the connection
    // (assumption 11).
    const live = tab?.kind === "websocket" && tab.connection !== "idle";
    if ((tab?.kind !== "docs" && tabsStore.isDirty(id)) || live) {
      setClosingTabId(id);
      return;
    }
    tabsStore.closeTab(id);
  }

  const tabBarTabs: TabBarTab[] = tabsStore.tabs.map((tab) => {
    if (tab.kind === "request") {
      return {
        kind: "request",
        id: tab.id,
        label: tab.loadedRequest?.name ?? "Untitled Request",
        method: tab.method,
        isDirty: tabsStore.isDirty(tab.id),
      };
    }
    if (tab.kind === "environment") {
      return {
        kind: "environment",
        id: tab.id,
        label:
          environments.find((environment) => environment.id === tab.environmentId)?.name ??
          "Environment",
      };
    }
    if (tab.kind === "docs") {
      return {
        kind: "docs",
        id: tab.id,
        label: docsTabLabel(docsTargetName(tab.target, collections, contentsById)),
        isDirty: tabsStore.isDirty(tab.id),
      };
    }
    if (tab.kind === "websocket") {
      return {
        kind: "websocket",
        id: tab.id,
        label: tab.loadedRequest?.name ?? "Untitled WebSocket",
        isDirty: tabsStore.isDirty(tab.id),
        live: tab.connection === "connected",
      };
    }
    // "Example" until the body arrives; the store caches it after the first
    // open, so this only shows for a moment.
    return { kind: "example", id: tab.id, label: examplesById[tab.exampleId]?.name ?? "Example" };
  });

  const closing = tabsStore.tabs.find((tab) => tab.id === closingTabId) ?? null;
  // Only request and WebSocket tabs save explicitly, so only they reach the
  // prompt.
  const closingName =
    closing?.kind === "request"
      ? (closing.loadedRequest?.name ?? "Untitled Request")
      : closing?.kind === "websocket"
        ? (closing.loadedRequest?.name ?? "Untitled WebSocket")
        : null;
  const closingMessage =
    closing === null || closingName === null
      ? ""
      : closing.kind === "websocket" && closing.connection !== "idle"
        ? `"${closingName}" is connected. Closing it disconnects${
            tabsStore.isDirty(closing.id) ? " and loses its unsaved changes" : ""
          }.`
        : `"${closingName}" has unsaved changes. Close it anyway?`;

  return (
    <div className="flex h-screen bg-background text-foreground">
      {!sidebarCollapsed && (
        <Sidebar
          loadedRequestId={loaded?.id ?? null}
          onOpenDocs={(target) => tabsStore.openDocs(target)}
          onOpenEnvironment={(environmentId) => tabsStore.openEnvironment(environmentId)}
          onOpenExample={(exampleId) => tabsStore.openExample(exampleId)}
          onOpenHistoryEntry={(request) => tabsStore.openHistoryEntry(request)}
          onOpenRequest={(saved) => tabsStore.openSavedRequest(saved)}
          onOpenWebSocket={(saved) => tabsStore.openSavedWebSocket(saved)}
          width={sidebarWidth}
        />
      )}

      {/* Outside the conditional above: collapsed, this strip and its chevron
          are the only way the sidebar comes back. */}
      <ResizeHandle
        axis="x"
        collapseLabel={sidebarCollapsed ? "Show the sidebar" : "Hide the sidebar"}
        collapsed={sidebarCollapsed}
        label="Resize sidebar"
        onReset={resetSidebarWidth}
        onSizeChange={applySidebarWidth}
        onToggleCollapse={toggleSidebar}
        size={sidebarWidth}
      />

      <div className="flex flex-1 flex-col overflow-hidden">
        <TabBar
          activeTabId={tabsStore.activeTabId}
          onClose={handleCloseTab}
          onNew={(kind) =>
            kind === "websocket" ? tabsStore.openBlankWebSocketTab() : tabsStore.openBlankTab()
          }
          onSelect={tabsStore.setActiveTab}
          tabs={tabBarTabs}
          trailing={<EnvironmentSelector />}
        />

        {activeTab.kind === "environment" ? (
          <EnvironmentEditor environmentId={activeTab.environmentId} />
        ) : activeTab.kind === "example" ? (
          <ExampleViewer exampleId={activeTab.exampleId} />
        ) : activeTab.kind === "docs" ? (
          <DocsEditor tab={activeTab} />
        ) : activeTab.kind === "websocket" ? (
          <WebSocketView
            onManageCookies={() => setCookiesOpen(true)}
            onSave={() => void handleSaveClick()}
            pathSegments={pathSegments}
            tab={activeTab}
          />
        ) : (
          <>
            <RequestBuilder
              auth={activeTab.auth}
              bodyKind={activeTab.bodyKind}
              formRows={activeTab.formRows}
              headerRows={activeTab.headerRows}
              isSending={isSending === true}
              method={activeTab.method}
              multipartRows={activeTab.multipartRows}
              onAuthChange={tabsStore.setAuth}
              onBodyKindChange={tabsStore.setBodyKind}
              onCancel={tabsStore.cancel}
              onCopyAsCurl={() => {
                // Resolved, so the pasted command reproduces the request
                // rather than asking a shell to understand {{placeholders}}.
                const resolved = substituteRequestInput(
                  tabsStore.currentInput(),
                  useEnvironmentsStore.getState().activeVariables(),
                );
                void navigator.clipboard.writeText(buildCurlCommand(resolved));
              }}
              onFormRowsChange={tabsStore.setFormRows}
              onHeaderRowsChange={tabsStore.setHeaderRows}
              onManageCookies={() => setCookiesOpen(true)}
              onMethodChange={tabsStore.setMethod}
              onChooseMultipartFile={(rowId) => void handleChooseMultipartFile(rowId)}
              onMultipartRowsChange={tabsStore.setMultipartRows}
              onPanelHeightChange={setBuilderPanelHeight}
              onParamRowsChange={tabsStore.setParamRows}
              onRawContentTypeChange={tabsStore.setRawContentType}
              onRawTextChange={tabsStore.setRawText}
              onResetPanelHeight={resetBuilderPanelHeight}
              onToggleResponse={toggleResponse}
              onSave={() => void handleSaveClick()}
              onSend={() => {
                void tabsStore.send();
              }}
              onSendAndDownload={() => {
                void tabsStore.sendAndDownload();
              }}
              onSettingsChange={tabsStore.setSettings}
              onUrlChange={tabsStore.setUrl}
              panelHeight={builderPanelHeight}
              pathSegments={pathSegments}
              secretState={activeTab.secretState}
              responseCollapsed={responseCollapsed}
              paramRows={activeTab.paramRows}
              rawContentType={activeTab.rawContentType}
              rawText={activeTab.rawText}
              settings={activeTab.settings}
              url={activeTab.url}
            />
            {/* The divider and its chevron live at the bottom of
                RequestBuilder, so they survive this being unmounted. */}
            {!responseCollapsed && (
              <ResponseViewer
                download={activeTab.download}
                error={activeTab.error}
                isSending={isSending === true}
                response={activeTab.response}
                saveExample={saveExampleAction(activeTab)}
                stream={activeTab.stream}
              />
            )}
          </>
        )}
      </div>

      {cookiesOpen && <CookieManagerDialog onClose={() => setCookiesOpen(false)} />}

      {savingExample && requestTab?.loadedRequest && requestTab.response && (
        <SaveExampleDialog
          collectionId={requestTab.loadedRequest.collectionId}
          onClose={() => setSavingExample(false)}
          onSaved={(example) => {
            setSavingExample(false);
            tabsStore.openExample(example.id);
          }}
          request={requestTab.sentRequest ?? tabsStore.currentInput()}
          requestId={requestTab.loadedRequest.id}
          response={requestTab.response}
        />
      )}

      {saveDialogOpen && (
        <SaveRequestDialog
          defaultName={loaded?.name ?? ""}
          onClose={() => setSaveDialogOpen(false)}
          onSaved={(saved) => {
            const location = {
              id: saved.id,
              collectionId: saved.collectionId,
              folderId: saved.folderId,
              name: saved.name,
            };
            if (webSocketTab !== null) {
              tabsStore.markWebSocketSaved(location);
            } else {
              tabsStore.markSaved(location);
            }
            setSaveDialogOpen(false);
          }}
          payload={savePayload()}
        />
      )}

      {closing && closingName !== null && (
        <ConfirmDialog
          confirmLabel={
            closing.kind === "websocket" && closing.connection !== "idle"
              ? "Close and disconnect"
              : "Close without saving"
          }
          message={closingMessage}
          onCancel={() => setClosingTabId(null)}
          onConfirm={() => {
            tabsStore.closeTab(closing.id);
            setClosingTabId(null);
          }}
          title="Close tab"
        />
      )}
    </div>
  );
}

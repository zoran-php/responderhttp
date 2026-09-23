// http_client/src/features/websocket/WebSocketView.tsx
//
// A WebSocket tab: the builder above, the log below. Wires the tab to the
// store the way DocsEditor does; the two halves stay presentational. Save
// and the cookie manager open dialogs App owns, so they come in as props.
import { WebSocketBuilder } from "@/features/websocket/WebSocketBuilder";
import { WebSocketLog } from "@/features/websocket/WebSocketLog";
import { useLayoutStore } from "@/store/layout-store";
import { useTabsStore, type WebSocketTab } from "@/store/request-store";

interface WebSocketViewProps {
  tab: WebSocketTab;
  pathSegments: string[];
  onSave: () => void;
  onManageCookies: () => void;
}

export function WebSocketView({ tab, pathSegments, onSave, onManageCookies }: WebSocketViewProps) {
  const store = useTabsStore();
  const panelHeight = useLayoutStore((state) => state.builderPanelHeight);
  const setPanelHeight = useLayoutStore((state) => state.setBuilderPanelHeight);
  const resetPanelHeight = useLayoutStore((state) => state.resetBuilderPanelHeight);
  const responseCollapsed = useLayoutStore((state) => state.responseCollapsed);
  const toggleResponse = useLayoutStore((state) => state.toggleResponse);

  return (
    <>
      <WebSocketBuilder
        composerError={tab.composerError}
        connection={tab.connection}
        draft={tab.draft}
        headerRows={tab.headerRows}
        onBeautify={store.beautifyWsDraft}
        onConnect={() => void store.connect()}
        onDisconnect={store.disconnect}
        onDraftChange={store.setWsDraft}
        onHeaderRowsChange={store.setWsHeaderRows}
        onManageCookies={onManageCookies}
        onOpenDocs={(requestId) => store.openDocs({ kind: "request", id: requestId })}
        onPanelHeightChange={setPanelHeight}
        onParamRowsChange={store.setWsParamRows}
        onResetPanelHeight={resetPanelHeight}
        onSave={onSave}
        onSend={() => void store.sendMessage()}
        onSettingsChange={store.setWsSettings}
        onToggleResponse={toggleResponse}
        onUrlChange={store.setWsUrl}
        panelHeight={panelHeight}
        paramRows={tab.paramRows}
        pathSegments={pathSegments}
        responseCollapsed={responseCollapsed}
        savedRequestId={tab.loadedRequest?.id ?? null}
        settings={tab.settings}
        url={tab.url}
      />
      {!responseCollapsed && (
        <WebSocketLog
          binaryEncoding={tab.draft.binaryEncoding}
          connection={tab.connection}
          filter={tab.logFilter}
          log={tab.log}
          onClearMessages={store.clearMessages}
          onFilterChange={store.setLogFilter}
          onQueryChange={store.setLogQuery}
          query={tab.logQuery}
          reconnectAttempt={tab.reconnectAttempt}
        />
      )}
    </>
  );
}

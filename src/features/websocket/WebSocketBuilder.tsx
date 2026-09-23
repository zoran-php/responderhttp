// http_client/src/features/websocket/WebSocketBuilder.tsx
//
// The top half of a WebSocket tab: breadcrumb and Save, the URL with its
// Connect/Disconnect/Cancel button, and the Docs, Message, Params, Headers
// and Settings sub-tabs. Presentational like RequestBuilder: props in,
// events out.
import { useState } from "react";
import { Cookie, Loader2, X } from "lucide-react";

import { KeyValueTable } from "@/components/KeyValueTable";
import { ResizeHandle } from "@/components/ResizeHandle";
import { RequestToolbar } from "@/features/request-builder/RequestToolbar";
import { MessageComposer } from "@/features/websocket/MessageComposer";
import { WebSocketDocsPanel } from "@/features/websocket/WebSocketDocsPanel";
import { WebSocketSettingsPanel } from "@/features/websocket/WebSocketSettingsPanel";
import { useBuilderPanelHeight } from "@/hooks/useBuilderPanelHeight";
import { IANA_HEADER_NAMES } from "@/lib/http-header-names";
import { headerValueSuggestions } from "@/lib/http-header-values";
import type { KeyValueRow } from "@/lib/key-values";
import { primaryAction, type WsConnectionState } from "@/lib/ws-status";
import type { WebSocketSettings, WsDraft } from "@/types/websocket";

const TABS = ["Docs", "Message", "Params", "Headers", "Settings"] as const;
type Tab = (typeof TABS)[number];

interface WebSocketBuilderProps {
  url: string;
  paramRows: KeyValueRow[];
  headerRows: KeyValueRow[];
  settings: WebSocketSettings;
  draft: WsDraft;
  connection: WsConnectionState;
  composerError: string | null;
  /** The saved request's id, for its Docs; null while unsaved. */
  savedRequestId: string | null;
  /** Outside in, ending with the request name. See lib/request-path.ts. */
  pathSegments: string[];
  panelHeight: number;
  responseCollapsed: boolean;
  onUrlChange: (url: string) => void;
  onParamRowsChange: (rows: KeyValueRow[]) => void;
  onHeaderRowsChange: (rows: KeyValueRow[]) => void;
  onSettingsChange: (patch: Partial<WebSocketSettings>) => void;
  onDraftChange: (patch: Partial<WsDraft>) => void;
  onBeautify: () => void;
  onConnect: () => void;
  /** Also what Cancel does: it cancels a handshake or a reconnect wait. */
  onDisconnect: () => void;
  onSend: () => void;
  onSave: () => void;
  onManageCookies: () => void;
  onOpenDocs: (requestId: string) => void;
  onPanelHeightChange: (height: number) => void;
  onResetPanelHeight: () => void;
  onToggleResponse: () => void;
}

export function WebSocketBuilder(props: WebSocketBuilderProps) {
  const [tab, setTab] = useState<Tab>("Message");
  const { connection, panelHeight, responseCollapsed, onPanelHeightChange } = props;
  const { panelRef, applyHeight } = useBuilderPanelHeight({
    panelHeight,
    responseCollapsed,
    onPanelHeightChange,
  });
  // The URL, and the Params view of it, are what the connection was opened
  // with; they stay put until it is closed (assumption 10).
  const urlLocked = connection !== "idle";
  const action = primaryAction(connection);

  return (
    <div className={`flex flex-col ${responseCollapsed ? "min-h-0 flex-1" : "shrink-0"}`}>
      <RequestToolbar onSave={props.onSave} pathSegments={props.pathSegments} />

      <form
        className="flex items-center gap-2 px-4 pb-3 pt-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (action.kind === "connect") {
            props.onConnect();
          }
        }}
      >
        <input
          aria-label="WebSocket URL"
          className={`h-9 flex-1 rounded-md border border-input bg-background px-3 text-sm ${
            urlLocked ? "opacity-60" : ""
          }`}
          disabled={urlLocked}
          onChange={(event) => props.onUrlChange(event.target.value)}
          placeholder="wss://echo.websocket.org"
          spellCheck={false}
          value={props.url}
        />

        {action.kind === "connect" && (
          <button
            className="inline-flex h-9 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 active:bg-primary/80"
            type="submit"
          >
            {action.label}
          </button>
        )}
        {action.kind === "cancel" && (
          <button
            className="inline-flex h-9 items-center gap-2 rounded-md border border-destructive px-4 text-sm font-medium text-destructive hover:bg-destructive/10 active:bg-destructive/20"
            onClick={props.onDisconnect}
            type="button"
          >
            <X aria-hidden className="h-4 w-4" />
            {action.label}
          </button>
        )}
        {action.kind === "disconnect" && (
          <button
            className="inline-flex h-9 items-center rounded-md bg-secondary px-4 text-sm font-medium text-secondary-foreground hover:bg-secondary/80"
            onClick={props.onDisconnect}
            type="button"
          >
            {action.label}
          </button>
        )}
        {action.kind === "busy" && (
          <button
            className="inline-flex h-9 items-center gap-2 rounded-md bg-secondary px-4 text-sm font-medium text-secondary-foreground opacity-60"
            disabled
            type="button"
          >
            <Loader2 aria-hidden className="h-4 w-4 animate-spin" />
            {action.label}
          </button>
        )}
      </form>

      <div className="flex items-center gap-1 border-t border-border px-3">
        {TABS.map((name) => (
          <button
            className={`border-b-2 px-3 py-2 text-sm ${
              tab === name
                ? "border-primary font-medium text-foreground"
                : "border-transparent text-muted-foreground"
            }`}
            key={name}
            onClick={() => setTab(name)}
            type="button"
          >
            {name}
          </button>
        ))}
        <button
          className="ml-auto inline-flex items-center gap-1.5 px-2 py-2 text-xs text-muted-foreground hover:text-foreground"
          onClick={props.onManageCookies}
          type="button"
        >
          <Cookie aria-hidden className="h-3.5 w-3.5" />
          Cookies
        </button>
      </div>

      <div
        className={`overflow-auto ${responseCollapsed ? "min-h-0 flex-1" : ""}`}
        ref={panelRef}
        style={responseCollapsed ? undefined : { height: panelHeight }}
      >
        {tab === "Docs" && (
          <WebSocketDocsPanel onOpenDocs={props.onOpenDocs} requestId={props.savedRequestId} />
        )}
        {tab === "Message" && (
          <MessageComposer
            connected={connection === "connected"}
            draft={props.draft}
            error={props.composerError}
            onBeautify={props.onBeautify}
            onDraftChange={props.onDraftChange}
            onSend={props.onSend}
          />
        )}
        {tab === "Params" && (
          <>
            {urlLocked && <LockedNote>Disconnect to change the URL and its parameters.</LockedNote>}
            {/* A disabled fieldset disables every input inside it, so the
                shared table needs no read-only mode of its own. */}
            <fieldset className="min-w-0" disabled={urlLocked}>
              <KeyValueTable
                nameLabel="Parameter"
                onChange={props.onParamRowsChange}
                rows={props.paramRows}
              />
            </fieldset>
          </>
        )}
        {tab === "Headers" && (
          <>
            {urlLocked && <LockedNote>Changes apply the next time you connect.</LockedNote>}
            <KeyValueTable
              nameLabel="Header"
              nameSuggestions={IANA_HEADER_NAMES}
              onChange={props.onHeaderRowsChange}
              rows={props.headerRows}
              valueSuggestionsFor={headerValueSuggestions}
            />
          </>
        )}
        {tab === "Settings" && (
          <>
            {urlLocked && <LockedNote>Changes apply the next time you connect.</LockedNote>}
            <WebSocketSettingsPanel onChange={props.onSettingsChange} settings={props.settings} />
          </>
        )}
      </div>

      <ResizeHandle
        axis="y"
        collapseLabel={responseCollapsed ? "Show the messages" : "Hide the messages"}
        collapsed={responseCollapsed}
        label="Resize request panel"
        onReset={props.onResetPanelHeight}
        onSizeChange={applyHeight}
        onToggleCollapse={props.onToggleResponse}
        size={panelHeight}
      />
    </div>
  );
}

function LockedNote({ children }: { children: string }) {
  return <p className="px-4 pt-2 text-xs text-muted-foreground">{children}</p>;
}

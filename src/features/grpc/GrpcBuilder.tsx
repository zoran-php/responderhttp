// http_client/src/features/grpc/GrpcBuilder.tsx
//
// The top half of a gRPC tab: breadcrumb and Save; the lock, the server,
// the method and Invoke (Cancel while running); and the Message, Metadata,
// Authorization, Service definition, Settings and Docs sub-tabs.
// Presentational: the tab in, events out.
import { useState } from "react";
import { Loader2, Lock, LockOpen, X } from "lucide-react";

import { KeyValueTable } from "@/components/KeyValueTable";
import { ResizeHandle } from "@/components/ResizeHandle";
import { AuthPanel } from "@/features/request-builder/AuthPanel";
import { RequestToolbar } from "@/features/request-builder/RequestToolbar";
import { GrpcMessagePanel } from "@/features/grpc/GrpcMessagePanel";
import { GrpcMethodPicker } from "@/features/grpc/GrpcMethodPicker";
import { GrpcSchemaPanel } from "@/features/grpc/GrpcSchemaPanel";
import { GrpcSettingsPanel } from "@/features/grpc/GrpcSettingsPanel";
import { WebSocketDocsPanel } from "@/features/websocket/WebSocketDocsPanel";
import { useBuilderPanelHeight } from "@/hooks/useBuilderPanelHeight";
import { clientStreams, findMethod } from "@/lib/grpc-request";
import { grpcPrimaryAction } from "@/lib/grpc-status";
import type { KeyValueRow } from "@/lib/key-values";
import type { GrpcTab } from "@/store/request-store";
import type { GrpcSettings } from "@/types/grpc";
import type { Auth } from "@/types/http";

const TABS = [
  "Message",
  "Metadata",
  "Authorization",
  "Service definition",
  "Settings",
  "Docs",
] as const;
type SubTab = (typeof TABS)[number];

/** Everything the builder can ask for; GrpcView wires these to the store. */
export interface GrpcBuilderActions {
  setUrl: (url: string) => void;
  /** The URL field lost focus: take a scheme off and reflect if stale. */
  leaveUrl: () => void;
  setTls: (tls: boolean) => void;
  selectMethod: (methodPath: string) => void;
  /** The method list opened: reflect if stale. */
  openMethods: () => void;
  invoke: () => void;
  cancel: () => void;
  setMetadataRows: (rows: KeyValueRow[]) => void;
  setAuth: (auth: Auth) => void;
  setSettings: (patch: Partial<GrpcSettings>) => void;
  setMessage: (message: string) => void;
  beautify: () => void;
  useExample: () => void;
  send: () => void;
  endStream: () => void;
  reflect: () => void;
  importSchema: (importPaths: string[]) => void;
  chooseImportFolder: () => Promise<string | null>;
  save: () => void;
  copyAsGrpcurl: () => void;
  openDocs: (requestId: string) => void;
}

interface GrpcBuilderProps {
  tab: GrpcTab;
  actions: GrpcBuilderActions;
  /** Outside in, ending with the request name. See lib/request-path.ts. */
  pathSegments: string[];
  panelHeight: number;
  responseCollapsed: boolean;
  onPanelHeightChange: (height: number) => void;
  onResetPanelHeight: () => void;
  onToggleResponse: () => void;
}

export function GrpcBuilder({ tab, actions, ...layout }: GrpcBuilderProps) {
  const [subTab, setSubTab] = useState<SubTab>("Message");
  const { panelHeight, responseCollapsed, onPanelHeightChange } = layout;
  const { panelRef, applyHeight } = useBuilderPanelHeight({
    panelHeight,
    responseCollapsed,
    onPanelHeightChange,
  });
  const action = grpcPrimaryAction(tab.call);
  const method = findMethod(tab.schema, tab.methodPath);
  const LockIcon = tab.tls ? Lock : LockOpen;

  return (
    <div className={`flex flex-col ${responseCollapsed ? "min-h-0 flex-1" : "shrink-0"}`}>
      <RequestToolbar
        onCopyAsGrpcurl={actions.copyAsGrpcurl}
        onSave={actions.save}
        pathSegments={layout.pathSegments}
      />

      <form
        className="flex items-center gap-2 px-4 pb-3 pt-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (action.kind === "invoke") {
            actions.invoke();
          }
        }}
      >
        <button
          aria-label={tab.tls ? "TLS on" : "TLS off"}
          aria-pressed={tab.tls}
          className={`inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-input hover:bg-accent ${
            tab.tls ? "text-ws-ok" : "text-muted-foreground"
          }`}
          onClick={() => actions.setTls(!tab.tls)}
          title={tab.tls ? "TLS on: the call is encrypted" : "TLS off: plain text"}
          type="button"
        >
          <LockIcon aria-hidden className="h-4 w-4" />
        </button>
        <input
          aria-label="gRPC server URL"
          className="h-9 min-w-0 flex-1 rounded-md border border-input bg-background px-3 text-sm"
          onBlur={actions.leaveUrl}
          onChange={(event) => actions.setUrl(event.target.value)}
          placeholder="localhost:50051"
          spellCheck={false}
          value={tab.url}
        />
        <GrpcMethodPicker
          loading={tab.schemaStatus === "loading"}
          methodPath={tab.methodPath}
          onChange={actions.selectMethod}
          onOpen={actions.openMethods}
          schema={tab.schema}
        />

        {action.kind === "invoke" && (
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
            onClick={actions.cancel}
            type="button"
          >
            <X aria-hidden className="h-4 w-4" />
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
              subTab === name
                ? "border-primary font-medium text-foreground"
                : "border-transparent text-muted-foreground"
            }`}
            key={name}
            onClick={() => setSubTab(name)}
            type="button"
          >
            {name}
          </button>
        ))}
      </div>

      <div
        className={`overflow-auto ${responseCollapsed ? "min-h-0 flex-1" : ""}`}
        ref={panelRef}
        style={responseCollapsed ? undefined : { height: panelHeight }}
      >
        {subTab === "Message" && (
          <GrpcMessagePanel
            clientStreams={method !== null && clientStreams(method.kind)}
            error={tab.composerError}
            hasMethod={method !== null}
            message={tab.message}
            onBeautify={actions.beautify}
            onEndStream={actions.endStream}
            onMessageChange={actions.setMessage}
            onSend={actions.send}
            onUseExample={actions.useExample}
            running={tab.call === "running"}
            streamEnded={tab.streamEnded}
          />
        )}
        {subTab === "Metadata" && (
          <KeyValueTable
            nameLabel="Key"
            onChange={actions.setMetadataRows}
            rows={tab.metadataRows}
          />
        )}
        {subTab === "Authorization" && (
          <AuthPanel auth={tab.auth} onChange={actions.setAuth} secretState={tab.secretState} />
        )}
        {subTab === "Service definition" && (
          <GrpcSchemaPanel
            error={tab.schemaError}
            onChooseImportFolder={actions.chooseImportFolder}
            onImport={actions.importSchema}
            onReflect={actions.reflect}
            schema={tab.schema}
            status={tab.schemaStatus}
          />
        )}
        {subTab === "Settings" && (
          <GrpcSettingsPanel onChange={actions.setSettings} settings={tab.settings} />
        )}
        {subTab === "Docs" && (
          <WebSocketDocsPanel
            onOpenDocs={actions.openDocs}
            requestId={tab.loadedRequest?.id ?? null}
          />
        )}
      </div>

      <ResizeHandle
        axis="y"
        collapseLabel={responseCollapsed ? "Show the response" : "Hide the response"}
        collapsed={responseCollapsed}
        label="Resize request panel"
        onReset={layout.onResetPanelHeight}
        onSizeChange={applyHeight}
        onToggleCollapse={layout.onToggleResponse}
        size={panelHeight}
      />
    </div>
  );
}

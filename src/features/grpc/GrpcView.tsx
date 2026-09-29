// http_client/src/features/grpc/GrpcView.tsx
//
// A gRPC tab: the builder above, the response below. Wires the tab to the
// store and the schema choosers to the service, as WebSocketView does for
// its tab; the two halves stay presentational. Save opens a dialog App
// owns, so it comes in as a prop.
import { useMemo } from "react";

import { GrpcBuilder, type GrpcBuilderActions } from "@/features/grpc/GrpcBuilder";
import { GrpcResponseView } from "@/features/grpc/GrpcResponseView";
import { findMethod, clientStreams, serverStreams } from "@/lib/grpc-request";
import { chooseImportFolder, chooseProtoFiles } from "@/services/grpc";
import { useLayoutStore } from "@/store/layout-store";
import { useTabsStore, type GrpcTab } from "@/store/request-store";

interface GrpcViewProps {
  tab: GrpcTab;
  pathSegments: string[];
  onSave: () => void;
}

export function GrpcView({ tab, pathSegments, onSave }: GrpcViewProps) {
  const store = useTabsStore();
  const panelHeight = useLayoutStore((state) => state.builderPanelHeight);
  const setPanelHeight = useLayoutStore((state) => state.setBuilderPanelHeight);
  const resetPanelHeight = useLayoutStore((state) => state.resetBuilderPanelHeight);
  const responseCollapsed = useLayoutStore((state) => state.responseCollapsed);
  const toggleResponse = useLayoutStore((state) => state.toggleResponse);

  const actions: GrpcBuilderActions = {
    setUrl: (url) => {
      store.setGrpcUrl(url);
      // A pasted grpcs:// moves the lock at once rather than on blur.
      store.normalizeGrpcUrl();
    },
    leaveUrl: () => {
      store.normalizeGrpcUrl();
      void store.reflectGrpcSchemaIfStale();
    },
    setTls: store.setGrpcTls,
    selectMethod: store.selectGrpcMethod,
    openMethods: () => void store.reflectGrpcSchemaIfStale(),
    invoke: () => void store.invokeGrpc(),
    cancel: store.cancelGrpc,
    setMetadataRows: store.setGrpcMetadataRows,
    setAuth: store.setGrpcAuth,
    setSettings: store.setGrpcSettings,
    setMessage: store.setGrpcMessage,
    beautify: store.beautifyGrpcMessage,
    useExample: () => void store.fillExampleGrpcMessage(),
    send: () => void store.sendGrpcMessage(),
    endStream: () => void store.endGrpcStream(),
    reflect: () => void store.reflectGrpcSchema(),
    importSchema: (importPaths) => {
      void chooseProtoFiles().then((chosen) => {
        if (chosen.ok && chosen.value.length > 0) {
          void store.importGrpcSchema(chosen.value, importPaths);
        }
      });
    },
    chooseImportFolder: async () => {
      const chosen = await chooseImportFolder();
      return chosen.ok ? chosen.value : null;
    },
    save: onSave,
    copyAsGrpcurl: () => {
      const command = store.grpcurlCommand();
      if (command !== null) {
        void navigator.clipboard.writeText(command);
      }
    },
    openDocs: (requestId) => store.openDocs({ kind: "request", id: requestId }),
  };

  const streaming = useMemo(() => {
    const method = findMethod(tab.schema, tab.methodPath);
    return method !== null && (clientStreams(method.kind) || serverStreams(method.kind));
  }, [tab.schema, tab.methodPath]);

  return (
    <>
      <GrpcBuilder
        actions={actions}
        onPanelHeightChange={setPanelHeight}
        onResetPanelHeight={resetPanelHeight}
        onToggleResponse={toggleResponse}
        panelHeight={panelHeight}
        pathSegments={pathSegments}
        responseCollapsed={responseCollapsed}
        tab={tab}
      />
      {!responseCollapsed && (
        <GrpcResponseView
          call={tab.call}
          callError={tab.callError}
          log={tab.log}
          logFilter={tab.logFilter}
          logQuery={tab.logQuery}
          onClearMessages={store.clearGrpcMessages}
          onFilterChange={store.setGrpcLogFilter}
          onQueryChange={store.setGrpcLogQuery}
          response={tab.response}
          streaming={streaming}
        />
      )}
    </>
  );
}

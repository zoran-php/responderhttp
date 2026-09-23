// http_client/src/features/request-builder/RequestBuilder.tsx
//
// Presentational: props in, events out. It does not know about invoke(),
// the store, or how a request is assembled.
import { useState } from "react";
import { X } from "lucide-react";

import { KeyValueTable } from "@/components/KeyValueTable";
import { useBuilderPanelHeight } from "@/hooks/useBuilderPanelHeight";
import { ResizeHandle } from "@/components/ResizeHandle";
import { IANA_HEADER_NAMES } from "@/lib/http-header-names";
import { headerValueSuggestions } from "@/lib/http-header-values";
import { methodTextColor } from "@/lib/http-method-colors";
import type { KeyValueRow } from "@/lib/key-values";
import type { MultipartRow } from "@/lib/multipart-rows";
import { AuthPanel } from "@/features/request-builder/AuthPanel";
import { BodyEditor } from "@/features/request-builder/BodyEditor";
import { RequestToolbar } from "@/features/request-builder/RequestToolbar";
import { SendButton } from "@/features/request-builder/SendButton";
import { SettingsPanel } from "@/features/request-builder/SettingsPanel";
import type { BodyKind } from "@/store/request-store"; // exports useTabsStore now; see that file's header
import {
  HTTP_METHODS,
  type Auth,
  type HttpMethod,
  type RequestSettings,
  type SecretState,
} from "@/types/http";

const TABS = ["Params", "Auth", "Headers", "Body", "Settings"] as const;
type Tab = (typeof TABS)[number];

interface RequestBuilderProps {
  method: HttpMethod;
  url: string;
  isSending: boolean;
  headerRows: KeyValueRow[];
  paramRows: KeyValueRow[];
  bodyKind: BodyKind;
  auth: Auth;
  /** Whether the saved auth secret could be read back. */
  secretState: SecretState;
  rawContentType: string;
  rawText: string;
  formRows: KeyValueRow[];
  multipartRows: MultipartRow[];
  settings: RequestSettings;
  /** Height of the tab panel below the URL bar, dragged by the divider.
   * Ignored while the response is collapsed, when the panel takes the room. */
  panelHeight: number;
  responseCollapsed: boolean;
  /** Outside in, ending with the request name. See lib/request-path.ts. */
  pathSegments: string[];
  onMethodChange: (method: HttpMethod) => void;
  onUrlChange: (url: string) => void;
  onHeaderRowsChange: (rows: KeyValueRow[]) => void;
  onParamRowsChange: (rows: KeyValueRow[]) => void;
  onAuthChange: (auth: Auth) => void;
  onBodyKindChange: (kind: BodyKind) => void;
  onRawContentTypeChange: (contentType: string) => void;
  onRawTextChange: (text: string) => void;
  onFormRowsChange: (rows: KeyValueRow[]) => void;
  onMultipartRowsChange: (rows: MultipartRow[]) => void;
  onChooseMultipartFile: (rowId: string) => void;
  onSettingsChange: (patch: Partial<RequestSettings>) => void;
  onSend: () => void;
  onSendAndDownload: () => void;
  onCancel: () => void;
  onCopyAsCurl: () => void;
  onManageCookies: () => void;
  onSave: () => void;
  /** Already-clamped heights only — this component does the clamping. */
  onPanelHeightChange: (height: number) => void;
  onResetPanelHeight: () => void;
  onToggleResponse: () => void;
}

export function RequestBuilder(props: RequestBuilderProps) {
  const [tab, setTab] = useState<Tab>("Params");
  const { method, url, isSending, panelHeight, responseCollapsed, onPanelHeightChange } = props;
  const { panelRef, applyHeight } = useBuilderPanelHeight({
    panelHeight,
    responseCollapsed,
    onPanelHeightChange,
  });

  return (
    // No border-b: the resize handle at the bottom is the separator now.
    // With the response collapsed the builder takes the whole column, so it
    // grows rather than sizing to its content.
    <div className={`flex flex-col ${responseCollapsed ? "min-h-0 flex-1" : "shrink-0"}`}>
      <RequestToolbar
        onCopyAsCurl={props.onCopyAsCurl}
        onManageCookies={props.onManageCookies}
        onSave={props.onSave}
        pathSegments={props.pathSegments}
      />

      <form
        className="flex items-center gap-2 px-4 pb-3 pt-2"
        onSubmit={(event) => {
          event.preventDefault();
          props.onSend();
        }}
      >
        <select
          aria-label="HTTP method"
          className={`h-9 rounded-md border border-input bg-background px-2 text-sm font-semibold ${methodTextColor(method)}`}
          onChange={(event) => props.onMethodChange(event.target.value as HttpMethod)}
          value={method}
        >
          {HTTP_METHODS.map((value) => (
            // Colored per-option too: most browsers/webviews render an
            // <option>'s own text color, even though the closed select
            // above is what normally carries the className's color.
            <option className={methodTextColor(value)} key={value} value={value}>
              {value}
            </option>
          ))}
        </select>

        <input
          aria-label="Request URL"
          className="h-9 flex-1 rounded-md border border-input bg-background px-3 text-sm"
          onChange={(event) => props.onUrlChange(event.target.value)}
          placeholder="https://api.example.com/users"
          spellCheck={false}
          value={url}
        />

        {isSending ? (
          <button
            className="inline-flex h-9 items-center gap-2 rounded-md border border-destructive px-4 text-sm font-medium text-destructive hover:bg-destructive/10 active:bg-destructive/20"
            onClick={props.onCancel}
            type="button"
          >
            <X className="h-4 w-4" aria-hidden />
            Cancel
          </button>
        ) : (
          <SendButton onSend={props.onSend} onSendAndDownload={props.onSendAndDownload} />
        )}
      </form>

      <div className="flex gap-1 border-t border-border px-3">
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
      </div>

      <div
        className={`overflow-auto ${responseCollapsed ? "min-h-0 flex-1" : ""}`}
        ref={panelRef}
        style={responseCollapsed ? undefined : { height: panelHeight }}
      >
        {tab === "Params" && (
          <KeyValueTable
            nameLabel="Parameter"
            onChange={props.onParamRowsChange}
            rows={props.paramRows}
          />
        )}
        {tab === "Auth" && (
          <AuthPanel
            auth={props.auth}
            onChange={props.onAuthChange}
            secretState={props.secretState}
          />
        )}
        {tab === "Headers" && (
          <KeyValueTable
            nameLabel="Header"
            nameSuggestions={IANA_HEADER_NAMES}
            onChange={props.onHeaderRowsChange}
            rows={props.headerRows}
            valueSuggestionsFor={headerValueSuggestions}
          />
        )}
        {tab === "Body" && (
          <BodyEditor
            bodyKind={props.bodyKind}
            formRows={props.formRows}
            multipartRows={props.multipartRows}
            onChooseMultipartFile={props.onChooseMultipartFile}
            onBodyKindChange={props.onBodyKindChange}
            onFormRowsChange={props.onFormRowsChange}
            onMultipartRowsChange={props.onMultipartRowsChange}
            onRawContentTypeChange={props.onRawContentTypeChange}
            onRawTextChange={props.onRawTextChange}
            rawContentType={props.rawContentType}
            rawText={props.rawText}
          />
        )}
        {tab === "Settings" && (
          <SettingsPanel onChange={props.onSettingsChange} settings={props.settings} />
        )}
      </div>

      <ResizeHandle
        axis="y"
        collapseLabel={responseCollapsed ? "Show the response" : "Hide the response"}
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

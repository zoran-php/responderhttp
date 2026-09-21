// http_client/src/features/collections/ImportOpenApiDialog.tsx
//
// "Import OpenAPI…" (PLAN.md Phase 8d). Opens the file chooser straight away,
// then shows either why the file was refused or what importing it would
// create, with the options that change that.
//
// Local state only: the stores are told to reload once, through `onImported`,
// after something was actually written.
import { useCallback, useEffect, useRef, useState } from "react";
import { KeyRound } from "lucide-react";

import { Modal } from "@/components/Modal";
import {
  ALWAYS_LISTED_VARIABLES,
  countLabel,
  defaultImportOptions,
  environmentSummary,
  folderSummary,
  groupingPreview,
  refusalReport,
} from "@/lib/openapi-import";
import {
  discardOpenApiImport,
  importOpenApi,
  pickOpenApiImport,
  previewOpenApiImport,
} from "@/services/openapi-import";
import type { ApiError } from "@/types/http";
import {
  IMPORT_GROUPING_LABELS,
  IMPORT_GROUPINGS,
  type EnvironmentPreview,
  type ImportGrouping,
  type OpenApiImportOptions,
  type OpenApiImportPreview,
  type OpenApiImportRefusal,
  type OpenApiImportResult,
} from "@/types/openapi-import";

interface ImportOpenApiDialogProps {
  onClose: () => void;
  onImported: (result: OpenApiImportResult) => void;
}

type Stage =
  | { kind: "picking" }
  | { kind: "refused"; refusal: OpenApiImportRefusal }
  | { kind: "preview"; preview: OpenApiImportPreview; options: OpenApiImportOptions }
  | { kind: "done"; result: OpenApiImportResult };

export function ImportOpenApiDialog({ onClose, onImported }: ImportOpenApiDialogProps) {
  const [stage, setStage] = useState<Stage>({ kind: "picking" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [copied, setCopied] = useState(false);
  // The token Rust is holding a document for, so closing can release it.
  const tokenRef = useRef<string | null>(null);
  // Guards the one automatic pick against React's double-invoked effects.
  const pickedOnOpen = useRef(false);
  // Escape can close the dialog while Rust is still reading a large file.
  const closedRef = useRef(false);

  const pick = useCallback(async () => {
    setBusy(true);
    setError(null);
    setCopied(false);
    setStage({ kind: "picking" });
    try {
      const result = await pickOpenApiImport();
      if (!result.ok) {
        setError(result.error);
        return;
      }
      const loaded = result.value;
      if (closedRef.current) {
        if (loaded.kind === "ready") {
          void discardOpenApiImport(loaded.token);
        }
        return;
      }
      if (loaded.kind === "cancelled") {
        // Nothing to show. Any file shown before was released before this
        // pick started, so there is nothing to go back to either.
        onClose();
        return;
      }
      if (loaded.kind === "refused") {
        tokenRef.current = null;
        setStage({ kind: "refused", refusal: loaded });
        return;
      }
      tokenRef.current = loaded.token;
      setStage({ kind: "preview", preview: loaded, options: defaultImportOptions(loaded) });
    } finally {
      setBusy(false);
    }
  }, [onClose]);

  useEffect(() => {
    if (pickedOnOpen.current) {
      return;
    }
    pickedOnOpen.current = true;
    void pick();
  }, [pick]);

  const close = useCallback(() => {
    closedRef.current = true;
    if (tokenRef.current !== null) {
      void discardOpenApiImport(tokenRef.current);
      tokenRef.current = null;
    }
    onClose();
  }, [onClose]);

  async function changeOptions(preview: OpenApiImportPreview, options: OpenApiImportOptions) {
    // Shown at once; the report and counts follow when Rust has re-mapped.
    setStage({ kind: "preview", preview, options });
    setBusy(true);
    setError(null);
    try {
      const result = await previewOpenApiImport(preview.token, options);
      if (!result.ok) {
        setError(result.error);
        return;
      }
      setStage({ kind: "preview", preview: result.value, options });
    } finally {
      setBusy(false);
    }
  }

  async function runImport(preview: OpenApiImportPreview, options: OpenApiImportOptions) {
    setBusy(true);
    setError(null);
    try {
      const result = await importOpenApi(preview.token, options);
      if (!result.ok) {
        // The token is still valid after a failed write, so Import can be
        // pressed again once the cause (a locked credential store) is fixed.
        setError(result.error);
        return;
      }
      tokenRef.current = null;
      setStage({ kind: "done", result: result.value });
      onImported(result.value);
    } finally {
      setBusy(false);
    }
  }

  async function copyRefusal(refusal: OpenApiImportRefusal) {
    await navigator.clipboard.writeText(refusalReport(refusal));
    setCopied(true);
  }

  const footer = (
    <>
      {/* In the footer so it is seen however far the report is scrolled. */}
      {error && <p className="mr-auto text-xs text-destructive">{error.message}</p>}
      {stage.kind === "done" ? (
        <button
          className="h-8 rounded-md bg-primary px-3 font-medium text-primary-foreground"
          onClick={onClose}
          type="button"
        >
          Done
        </button>
      ) : (
        <>
          <button
            className="h-8 rounded-md border border-input px-3"
            disabled={busy}
            onClick={close}
            type="button"
          >
            Cancel
          </button>
          {stage.kind !== "picking" && (
            <button
              className="h-8 rounded-md border border-input px-3"
              disabled={busy}
              onClick={() => {
                if (tokenRef.current !== null) {
                  void discardOpenApiImport(tokenRef.current);
                  tokenRef.current = null;
                }
                void pick();
              }}
              type="button"
            >
              Choose another file…
            </button>
          )}
          {stage.kind === "preview" && (
            <button
              className="h-8 rounded-md bg-primary px-3 font-medium text-primary-foreground disabled:opacity-50"
              disabled={busy}
              onClick={() => void runImport(stage.preview, stage.options)}
              type="button"
            >
              {busy
                ? "Working…"
                : `Import ${countLabel(stage.preview.requestCount, "request", "requests")}`}
            </button>
          )}
        </>
      )}
    </>
  );

  return (
    <Modal footer={footer} onClose={close} size="wide" title="Import OpenAPI">
      <div className="space-y-3 text-sm">
        {stage.kind === "picking" && (
          <p className="text-muted-foreground">
            {busy ? "Reading and checking the file…" : "Choose a JSON or YAML OpenAPI file."}
          </p>
        )}

        {stage.kind === "refused" && (
          <RefusalView
            copied={copied}
            onCopy={() => void copyRefusal(stage.refusal)}
            refusal={stage.refusal}
          />
        )}

        {stage.kind === "preview" && (
          <PreviewView
            busy={busy}
            onChange={(options) => void changeOptions(stage.preview, options)}
            options={stage.options}
            preview={stage.preview}
          />
        )}

        {stage.kind === "done" && <DoneView result={stage.result} />}
      </div>
    </Modal>
  );
}

function RefusalView({
  refusal,
  copied,
  onCopy,
}: {
  refusal: OpenApiImportRefusal;
  copied: boolean;
  onCopy: () => void;
}) {
  const hidden = refusal.totalDetails - refusal.details.length;
  return (
    <div className="space-y-2">
      <p className="break-all font-mono text-xs text-muted-foreground">{refusal.fileName}</p>
      <p className="font-medium text-destructive">{refusal.title}</p>
      <p>{refusal.reason}</p>
      {refusal.details.length > 0 && (
        <ul className="list-disc space-y-1 pl-5 font-mono text-xs text-muted-foreground">
          {/* Rendered once and never reordered; two lines can read the same. */}
          {refusal.details.map((detail, index) => (
            <li className="break-all" key={`${index}-${detail}`}>
              {detail}
            </li>
          ))}
        </ul>
      )}
      {hidden > 0 && <p className="text-xs text-muted-foreground">…and {hidden} more.</p>}
      <button className="text-xs underline" onClick={onCopy} type="button">
        {copied ? "Copied" : "Copy details"}
      </button>
    </div>
  );
}

function PreviewView({
  preview,
  options,
  busy,
  onChange,
}: {
  preview: OpenApiImportPreview;
  options: OpenApiImportOptions;
  busy: boolean;
  onChange: (options: OpenApiImportOptions) => void;
}) {
  return (
    <div className="space-y-3">
      <div>
        <p className="font-medium">{preview.title}</p>
        <p className="text-xs text-muted-foreground">
          <span className="break-all font-mono">{preview.fileName}</span> · OpenAPI{" "}
          {preview.version} · {preview.format} · valid
        </p>
      </div>

      <p>
        A new collection with {countLabel(preview.requestCount, "request", "requests")}
        {preview.exampleCount > 0 &&
          ` and ${countLabel(preview.exampleCount, "saved response", "saved responses")}`}
        .
      </p>

      <label className="block">
        <span className="mb-1 block text-muted-foreground">Folders</span>
        <select
          className="h-8 w-full rounded-md border border-input bg-background px-2"
          disabled={busy}
          onChange={(event) =>
            onChange({ ...options, grouping: event.target.value as ImportGrouping })
          }
          value={options.grouping}
        >
          {IMPORT_GROUPINGS.map((grouping) => (
            <option key={grouping} value={grouping}>
              {IMPORT_GROUPING_LABELS[grouping]}
              {grouping === preview.defaultGrouping ? " (suggested)" : ""}
            </option>
          ))}
        </select>
        <span className="mt-1 block text-xs text-muted-foreground">
          {folderSummary(groupingPreview(preview, options.grouping))}
        </span>
      </label>

      <label className="flex items-start gap-2">
        <input
          checked={options.includeExamples}
          className="mt-0.5"
          disabled={busy}
          onChange={(event) => onChange({ ...options, includeExamples: event.target.checked })}
          type="checkbox"
        />
        <span>
          <span className="block">Import response examples as saved responses</span>
          <span className="block text-xs text-muted-foreground">
            They come from the document, not from a server, so they hold no live data.
          </span>
        </span>
      </label>

      <label className="flex items-start gap-2">
        <input
          checked={options.createEnvironment}
          className="mt-0.5"
          disabled={busy}
          onChange={(event) => onChange({ ...options, createEnvironment: event.target.checked })}
          type="checkbox"
        />
        <span>
          <span className="block">Create an environment for the base URL and credentials</span>
          <span className="block text-xs text-muted-foreground">
            Off writes the server URL into every request instead. Credentials always stay{" "}
            <code className="rounded bg-muted px-1">{"{{placeholders}}"}</code> - the document never
            fills them in.
          </span>
        </span>
      </label>

      {preview.environment && <EnvironmentSection environment={preview.environment} />}

      <NotesList notes={preview.notes} heading="Not imported, or guessed" />
    </div>
  );
}

/**
 * One line always; the chips only on request once there are more than a
 * handful — a large spec can bring well over a hundred (PLAN.md, Phase 8d
 * follow-up, option B1).
 */
function EnvironmentSection({ environment }: { environment: EnvironmentPreview }) {
  const listedByDefault = environment.variables.length <= ALWAYS_LISTED_VARIABLES;
  const [expanded, setExpanded] = useState(listedByDefault);
  if (environment.variables.length === 0) {
    return null;
  }
  return (
    <div>
      <p className="text-xs text-muted-foreground">
        Environment “{environment.name}” (not activated): {environmentSummary(environment)}.
        {!listedByDefault && (
          <>
            {" "}
            <button
              aria-expanded={expanded}
              className="underline"
              onClick={() => setExpanded(!expanded)}
              type="button"
            >
              {expanded ? "Hide" : "Show all"}
            </button>
          </>
        )}
      </p>
      {expanded && (
        // max-h-24 is four rows of chips: each is 20px tall with a 4px gap.
        <ul
          className="mt-1 flex max-h-24 flex-wrap gap-1 overflow-y-auto pr-1"
          data-testid="environment-variables"
        >
          {environment.variables.map((variable) => (
            <li
              className="flex items-center gap-1 rounded bg-muted px-1.5 py-0.5 font-mono text-xs"
              key={variable.name}
            >
              {variable.secret && (
                <KeyRound
                  aria-label="secret, starts empty"
                  className="h-3 w-3 text-muted-foreground"
                />
              )}
              {variable.name}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function DoneView({ result }: { result: OpenApiImportResult }) {
  return (
    <div className="space-y-3">
      <p>
        Imported {countLabel(result.requestCount, "request", "requests")} into “
        {result.collectionName}”
        {result.environmentId !== null && ", with an environment of the same name"}.
      </p>
      {result.environmentId !== null && (
        <p className="text-xs text-muted-foreground">
          Select the environment and fill in its secret values before sending.
        </p>
      )}
      <NotesList notes={result.notes} heading="Not imported, or guessed" />
    </div>
  );
}

function NotesList({ notes, heading }: { notes: string[]; heading: string }) {
  if (notes.length === 0) {
    return null;
  }
  return (
    <div>
      <p className="mb-1 font-medium">
        {heading} ({notes.length}):
      </p>
      <ul className="list-disc space-y-1 pl-5 text-xs text-muted-foreground">
        {/* One immutable list rendered once and never reordered. */}
        {notes.map((note, index) => (
          <li key={`${index}-${note}`}>{note}</li>
        ))}
      </ul>
    </div>
  );
}

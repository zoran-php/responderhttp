// http_client/src/features/collections/ExportOpenApiDialog.tsx
//
// "Export as OpenAPI" for one collection.
//
// Local state only: an export writes a file and changes nothing the rest of
// the app displays, so there is no store for it (CLAUDE.md section 6).
import { useState } from "react";

import { Modal } from "@/components/Modal";
import { exportCollectionOpenApi } from "@/services/openapi";
import type { ApiError } from "@/types/http";
import {
  DEFAULT_EXPORT_FORMAT,
  EXPORT_FORMAT_LABELS,
  EXPORT_FORMATS,
  OPENAPI_VERSION_LABELS,
  OPENAPI_VERSIONS,
  type ExportFormat,
  type OpenApiVersion,
} from "@/types/openapi";

interface ExportOpenApiDialogProps {
  collectionId: string;
  collectionName: string;
  onClose: () => void;
}

export function ExportOpenApiDialog({
  collectionId,
  collectionName,
  onClose,
}: ExportOpenApiDialogProps) {
  // Newest by default. Pick an older one for tooling that has not caught up:
  // the documents are otherwise identical apart from how example bodies are
  // carried, because nothing richer than `type` is inferred yet.
  const [version, setVersion] = useState<OpenApiVersion>("3.2");
  // JSON unless the user picks otherwise, every time the dialog opens.
  const [format, setFormat] = useState<ExportFormat>(DEFAULT_EXPORT_FORMAT);
  const [includeExamples, setIncludeExamples] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [notes, setNotes] = useState<string[] | null>(null);
  const [savedTo, setSavedTo] = useState<string | null>(null);

  async function handleExport() {
    setBusy(true);
    setError(null);
    try {
      const result = await exportCollectionOpenApi(collectionId, version, format, includeExamples);
      if (!result.ok) {
        setError(result.error);
        return;
      }
      // Dismissing the save dialog is not an error, but it is also not a
      // success worth closing on — the user may have meant to pick a
      // different folder.
      if (result.value.savedTo === null && result.value.notes.length === 0) {
        onClose();
        return;
      }
      setSavedTo(result.value.savedTo);
      setNotes(result.value.notes);
    } finally {
      setBusy(false);
    }
  }

  const footer =
    notes === null ? (
      <>
        {error && <p className="mr-auto text-xs text-destructive">{error.message}</p>}
        <button
          className="h-8 rounded-md border border-input px-3"
          disabled={busy}
          onClick={onClose}
          type="button"
        >
          Cancel
        </button>
        <button
          className="h-8 rounded-md bg-primary px-3 font-medium text-primary-foreground disabled:opacity-50"
          disabled={busy}
          onClick={() => void handleExport()}
          type="button"
        >
          {busy ? "Exporting…" : "Export"}
        </button>
      </>
    ) : (
      <button
        className="h-8 rounded-md bg-primary px-3 font-medium text-primary-foreground"
        onClick={onClose}
        type="button"
      >
        Done
      </button>
    );

  return (
    <Modal footer={footer} onClose={onClose} title={`Export "${collectionName}" as OpenAPI`}>
      <div className="space-y-3 text-sm">
        {notes === null ? (
          <>
            <p className="text-muted-foreground">
              Requests whose URL uses{" "}
              <code className="rounded bg-muted px-1">{"{{variables}}"}</code> become templated
              paths; everything else is exported literally.
            </p>

            <label className="block">
              <span className="mb-1 block text-muted-foreground">OpenAPI version</span>
              <select
                className="h-8 w-full rounded-md border border-input bg-background px-2"
                disabled={busy}
                onChange={(event) => setVersion(event.target.value as OpenApiVersion)}
                value={version}
              >
                {OPENAPI_VERSIONS.map((value) => (
                  <option key={value} value={value}>
                    {OPENAPI_VERSION_LABELS[value]}
                  </option>
                ))}
              </select>
            </label>

            <label className="block">
              <span className="mb-1 block text-muted-foreground">Format</span>
              <select
                className="h-8 w-full rounded-md border border-input bg-background px-2"
                disabled={busy}
                onChange={(event) => setFormat(event.target.value as ExportFormat)}
                value={format}
              >
                {EXPORT_FORMATS.map((value) => (
                  <option key={value} value={value}>
                    {EXPORT_FORMAT_LABELS[value]}
                  </option>
                ))}
              </select>
            </label>

            <label className="flex items-start gap-2">
              <input
                checked={includeExamples}
                className="mt-0.5"
                disabled={busy}
                onChange={(event) => setIncludeExamples(event.target.checked)}
                type="checkbox"
              />
              <span>
                <span className="block">Include saved responses as examples</span>
                <span className="block text-xs text-muted-foreground">
                  Richer documentation, but a saved response body can contain a token the server
                  returned. Leave this off for a file you are going to commit.
                </span>
              </span>
            </label>

            <p className="text-xs text-muted-foreground">
              Auth credentials and Authorization, Cookie and Proxy-Authorization headers are never
              exported. Environment variables are exported as placeholders, never resolved.
            </p>
          </>
        ) : (
          <>
            <p>
              {savedTo === null ? (
                "Not saved - the file dialog was dismissed."
              ) : (
                <>
                  Saved to <span className="break-all font-mono text-xs">{savedTo}</span>
                </>
              )}
            </p>

            {notes.length > 0 && (
              <div>
                <p className="mb-1 font-medium">
                  {notes.length} thing{notes.length === 1 ? "" : "s"} the document could not say:
                </p>
                <ul className="list-disc space-y-1 pl-5 text-xs text-muted-foreground">
                  {/* One immutable list rendered once and never reordered, so
                      position is a stable identity — and two notes can read
                      identically when two requests share a name. */}
                  {notes.map((note, index) => (
                    <li key={`${index}-${note}`}>{note}</li>
                  ))}
                </ul>
              </div>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}

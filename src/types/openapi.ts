// http_client/src/types/openapi.ts
//
// Mirrors OpenApiExportResultDto in src-tauri/src/commands/dto.rs.
//
// The exported document itself is deliberately not modelled here. It is
// written to disk in Rust and never crosses the boundary — shipping a whole
// OpenAPI document through IPC so the UI could show it would mean a second
// definition of the spec to keep in step with the first.

/**
 * The versions the exporter can emit. Mirrors `OpenApiVersion::from_wire` in
 * src-tauri/src/openapi/document.rs — a string the Rust side does not
 * recognise comes back as an invalidRequest rather than silently defaulting.
 */
export type OpenApiVersion = "3.0" | "3.1" | "3.2";

/** Newest first, which is also the order the picker shows them in. */
export const OPENAPI_VERSIONS: readonly OpenApiVersion[] = ["3.2", "3.1", "3.0"];

/**
 * What the picker calls each one. Here rather than inline in the dialog so
 * the list and its labels cannot fall out of step.
 */
export const OPENAPI_VERSION_LABELS: Record<OpenApiVersion, string> = {
  "3.2": "3.2.0 - newest",
  "3.1": "3.1.0",
  "3.0": "3.0.0 - widest tool support",
};

/**
 * The text format the document is written in. Mirrors
 * `ExportFormat::from_wire` in src-tauri/src/openapi/format.rs; anything else
 * comes back as an invalidRequest.
 */
export type ExportFormat = "json" | "yaml";

/** JSON first: it is the default the dialog opens with. */
export const EXPORT_FORMATS: readonly ExportFormat[] = ["json", "yaml"];

export const DEFAULT_EXPORT_FORMAT: ExportFormat = "json";

export const EXPORT_FORMAT_LABELS: Record<ExportFormat, string> = {
  json: "JSON",
  yaml: "YAML",
};

export interface OpenApiExportResult {
  /** null when the user dismissed the Save As dialog. Not an error. */
  savedTo: string | null;
  /**
   * What the export could not represent faithfully, already phrased as
   * sentences by the command layer. Worth showing even when `savedTo` is
   * null: they are the reason to change the collection and export again.
   */
  notes: string[];
}

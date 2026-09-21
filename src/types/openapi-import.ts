// http_client/src/types/openapi-import.ts
//
// Mirrors the DTOs in src-tauri/src/commands/openapi_import.rs.
//
// The document itself never crosses the boundary: Rust reads the file, keeps
// it, and hands the dialog a token. Notes and refusals arrive as finished
// sentences, like the export's notes.

/** Mirrors `Grouping::from_wire` in src-tauri/src/openapi/import/grouping.rs. */
export type ImportGrouping = "tags" | "paths" | "flat";

/** In the order the picker lists them. */
export const IMPORT_GROUPINGS: readonly ImportGrouping[] = ["tags", "paths", "flat"];

export const IMPORT_GROUPING_LABELS: Record<ImportGrouping, string> = {
  tags: "By tag",
  paths: "By path",
  flat: "No folders",
};

export interface OpenApiImportOptions {
  grouping: ImportGrouping;
  includeExamples: boolean;
  createEnvironment: boolean;
}

export interface GroupingPreview {
  grouping: ImportGrouping;
  /** Every folder, nested ones included. */
  folderCount: number;
  topLevelCount: number;
  /** Capped on the Rust side; `topLevelCount` is the full count. */
  topLevel: string[];
}

export interface PreviewVariable {
  name: string;
  secret: boolean;
}

export interface EnvironmentPreview {
  name: string;
  variables: PreviewVariable[];
}

export interface OpenApiImportPreview {
  /** Names the document Rust is holding. Spent by a successful import. */
  token: string;
  fileName: string;
  title: string;
  version: "3.0" | "3.1" | "3.2";
  format: "JSON" | "YAML";
  requestCount: number;
  exampleCount: number;
  defaultGrouping: ImportGrouping;
  groupings: GroupingPreview[];
  /** null when the options say not to create one. */
  environment: EnvironmentPreview | null;
  notes: string[];
}

export interface OpenApiImportRefusal {
  fileName: string;
  title: string;
  reason: string;
  details: string[];
  /** How many details there were before the list was capped. */
  totalDetails: number;
}

export type OpenApiImportLoadResult =
  | { kind: "cancelled" }
  | ({ kind: "refused" } & OpenApiImportRefusal)
  | ({ kind: "ready" } & OpenApiImportPreview);

export interface OpenApiImportResult {
  collectionId: string;
  environmentId: string | null;
  collectionName: string;
  requestCount: number;
  notes: string[];
}

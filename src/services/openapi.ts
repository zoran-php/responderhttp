// http_client/src/services/openapi.ts
//
// One function per command in src-tauri/src/commands/openapi.rs. The boundary
// crossing lives in services/invoke.ts (CLAUDE.md section 11, rule 3).
import { call } from "@/services/invoke";
import type { ApiError } from "@/types/http";
import type { ExportFormat, OpenApiExportResult, OpenApiVersion } from "@/types/openapi";
import type { Result } from "@/types/result";

/**
 * Builds the document and opens a Save As dialog, both in Rust. The file
 * chooser is why this resolves slowly and why it can succeed with
 * `savedTo: null`.
 *
 * `includeExamples` is off by default at the call site: a saved response body
 * can carry a token the server handed back, and an export is a file that
 * leaves the machine.
 */
export function exportCollectionOpenApi(
  collectionId: string,
  version: OpenApiVersion,
  format: ExportFormat,
  includeExamples: boolean,
): Promise<Result<OpenApiExportResult, ApiError>> {
  return call<OpenApiExportResult>("export_collection_openapi", {
    collectionId,
    version,
    format,
    includeExamples,
  });
}

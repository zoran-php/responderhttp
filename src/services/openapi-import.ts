// http_client/src/services/openapi-import.ts
//
// One function per command in src-tauri/src/commands/openapi_import.rs. The
// boundary crossing lives in services/invoke.ts (CLAUDE.md section 11,
// rule 3).
import { call } from "@/services/invoke";
import type { ApiError } from "@/types/http";
import type {
  OpenApiImportLoadResult,
  OpenApiImportOptions,
  OpenApiImportPreview,
  OpenApiImportResult,
} from "@/types/openapi-import";
import type { Result } from "@/types/result";

/**
 * Opens the file chooser, then reads, detects and validates the chosen file,
 * all in Rust. Resolves slowly for a large spec. A dismissed chooser is
 * `cancelled` and an unusable file is `refused` — neither is an error.
 */
export function pickOpenApiImport(): Promise<Result<OpenApiImportLoadResult, ApiError>> {
  return call<OpenApiImportLoadResult>("pick_openapi_import");
}

/** What `importOpenApi` would create with these options. */
export function previewOpenApiImport(
  token: string,
  options: OpenApiImportOptions,
): Promise<Result<OpenApiImportPreview, ApiError>> {
  return call<OpenApiImportPreview>("preview_openapi_import", { token, options });
}

/** Writes everything in one transaction; nothing is kept if it fails. */
export function importOpenApi(
  token: string,
  options: OpenApiImportOptions,
): Promise<Result<OpenApiImportResult, ApiError>> {
  return call<OpenApiImportResult>("import_openapi", { token, options });
}

/** Lets Rust drop the document when the dialog closes without importing. */
export async function discardOpenApiImport(token: string): Promise<void> {
  await call<void>("discard_openapi_import", { token });
}

// http_client/src/services/files.ts
//
// The file chooser. The boundary crossing lives in services/invoke.ts
// (CLAUDE.md section 11, rule 3). Exists so no component reaches for
// @tauri-apps/plugin-dialog directly — the dialog is a Rust-side dependency
// and this keeps the call typed.
import { call } from "@/services/invoke";
import type { ApiError, ChosenFile } from "@/types/http";
import type { Result } from "@/types/result";

/** Resolves to null when the dialog is dismissed, which is not an error. */
export function chooseFile(): Promise<Result<ChosenFile | null, ApiError>> {
  return call<ChosenFile | null>("choose_file");
}

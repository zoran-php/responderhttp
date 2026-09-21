// http_client/src/services/invoke.ts
//
// The shared invoke() wrapper. Four services had a byte-identical private copy
// of this function; adding "log every failed command" to each of them would
// have meant four call sites and four chances to forget one, so the copies are
// now this (CLAUDE.md section 7, DRY).
//
// http-client.ts keeps its own bespoke functions — sendRequest and
// sendAndDownload have different shapes and their own logging — but everything
// that is just "call a command, map the error" comes through here.
import { invoke } from "@tauri-apps/api/core";

import { toApiError } from "@/lib/api-error";
import { logWarn } from "@/services/logger";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

export async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<Result<T, ApiError>> {
  try {
    const value = await invoke<T>(command, args);
    return { ok: true, value };
  } catch (error) {
    const mapped = toApiError(error);
    // The command name and the error kind, never `args` — those carry request
    // bodies, auth values and cookie names (section 11, rule 6).
    logWarn(`command ${command} failed: ${mapped.kind}: ${mapped.message}`);
    return { ok: false, error: mapped };
  }
}

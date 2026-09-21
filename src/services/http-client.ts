// http_client/src/services/http-client.ts
//
// The only place invoke() may appear for HTTP commands (CLAUDE.md section 11,
// rule 3). Components call these functions, never invoke().
import { invoke } from "@tauri-apps/api/core";

// Was a local copy of this mapping, kept from before lib/api-error.ts
// existed. The copy had drifted: it was missing "notFound" and "storage", so
// a storage failure would have been flattened to an internal error with the
// wrong message — which send_and_download can actually produce.
import { toApiError } from "@/lib/api-error";
import { logDebug, logWarn } from "@/services/logger";
import type { ApiError, DownloadResult, HttpResponse, SendRequestInput } from "@/types/http";
import type { Result } from "@/types/result";

const SEND_REQUEST_COMMAND = "send_request";
const CANCEL_REQUEST_COMMAND = "cancel_request";
const SEND_AND_DOWNLOAD_COMMAND = "send_and_download";

/**
 * `logUrl` is what the log line shows, separate from `input.url` because the
 * two differ: `input` is fully resolved, and a secret variable's value must
 * not reach the log file (PLAN.md Phase 9). Callers pass the URL with secret
 * variables left as `{{placeholders}}`. Required rather than defaulted, so a
 * new caller cannot log a resolved URL by forgetting it.
 */
export async function sendRequest(
  requestId: string,
  input: SendRequestInput,
  logUrl: string,
): Promise<Result<HttpResponse, ApiError>> {
  try {
    const value = await invoke<HttpResponse>(SEND_REQUEST_COMMAND, {
      requestId,
      request: input,
    });
    // The URL is the whole point of a request log, and it can carry an API key
    // when the Auth tab puts one in the query string — logging.rs redacts that
    // on the way to the file. Nothing else about the request is logged
    // (CLAUDE.md section 11, rule 6).
    logDebug(`${input.method} ${logUrl} -> ${value.status} in ${value.timing.totalMs}ms`);
    return { ok: true, value };
  } catch (error) {
    const mapped = toApiError(error);
    logWarn(`${input.method} ${logUrl} failed: ${mapped.kind}: ${mapped.message}`);
    return { ok: false, error: mapped };
  }
}

/**
 * Fire-and-forget: the cancelled request itself reports the outcome, so a
 * failure to deliver the cancel is not worth its own error state.
 */
export async function cancelRequest(requestId: string): Promise<void> {
  try {
    await invoke(CANCEL_REQUEST_COMMAND, { requestId });
  } catch {
    // Nothing useful to do — the request either finished or never started.
  }
}

/**
 * Sends, then lets the user choose where the body is written. The body never
 * comes back: it is written to disk in Rust, so nothing here has to hold a
 * download-sized string. A dismissed Save As dialog returns `savedTo: null`
 * rather than an error — the request still happened.
 */
export async function sendAndDownload(
  requestId: string,
  input: SendRequestInput,
  logUrl: string,
): Promise<Result<DownloadResult, ApiError>> {
  try {
    const value = await invoke<DownloadResult>(SEND_AND_DOWNLOAD_COMMAND, {
      requestId,
      request: input,
    });
    logDebug(
      `${input.method} ${logUrl} -> ${value.status}, ` +
        `${value.savedTo === null ? "download dismissed" : "saved"}`,
    );
    return { ok: true, value };
  } catch (error) {
    const mapped = toApiError(error);
    logWarn(`${input.method} ${logUrl} download failed: ${mapped.kind}`);
    return { ok: false, error: mapped };
  }
}

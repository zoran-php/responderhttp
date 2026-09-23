// http_client/src/services/http-client.ts
//
// The only place invoke() may appear for HTTP commands (CLAUDE.md section 11,
// rule 3). Components call these functions, never invoke().
import { Channel, invoke } from "@tauri-apps/api/core";

// Was a local copy of this mapping, kept from before lib/api-error.ts
// existed. The copy had drifted: it was missing "notFound" and "storage", so
// a storage failure would have been flattened to an internal error with the
// wrong message — which send_and_download can actually produce.
import { toApiError } from "@/lib/api-error";
import { createEventBatcher, type FlushScheduler } from "@/lib/event-batcher";
import { logDebug, logWarn } from "@/services/logger";
import type {
  ApiError,
  DownloadResult,
  HttpResponse,
  HttpStreamEvent,
  SendRequestInput,
} from "@/types/http";
import type { Result } from "@/types/result";

const SEND_REQUEST_COMMAND = "send_request";
const CANCEL_REQUEST_COMMAND = "cancel_request";
const SEND_AND_DOWNLOAD_COMMAND = "send_and_download";

/**
 * Deliver without waiting for a frame at this many queued events, for the
 * same reason the WebSocket service has a ceiling: frames stop while the
 * window is hidden, and a fast stream must not grow the queue without bound.
 */
export const MAX_STREAM_EVENTS_PER_BATCH = 500;

const nextFrame: FlushScheduler = (flush) => {
  requestAnimationFrame(() => flush());
};

/**
 * The status and headers are one event and the whole point of streaming —
 * they show a stream is open. Blocks are what arrive by the thousand.
 */
function isHeaders(event: HttpStreamEvent): boolean {
  return event.type === "headers";
}

/**
 * `logUrl` is what the log line shows, separate from `input.url` because the
 * two differ: `input` is fully resolved, and a secret variable's value must
 * not reach the log file (PLAN.md Phase 9). Callers pass the URL with secret
 * variables left as `{{placeholders}}`. Required rather than defaulted, so a
 * new caller cannot log a resolved URL by forgetting it.
 *
 * `onStream` receives what arrives before the response does: the status and
 * headers at once, then, for a `text/event-stream` response, every block as
 * it is parsed, in batches of at most one per animation frame (PLAN-SSE.md,
 * 14c). It is called for every request, streaming or not, and is always done
 * being called by the time this resolves. Nothing it carries is logged — the
 * blocks are the response body (CLAUDE.md section 11, rule 6).
 */
export async function sendRequest(
  requestId: string,
  input: SendRequestInput,
  logUrl: string,
  onStream: (events: HttpStreamEvent[]) => void,
  schedule: FlushScheduler = nextFrame,
): Promise<Result<HttpResponse, ApiError>> {
  const batcher = createEventBatcher(onStream, schedule, {
    isUrgent: isHeaders,
    maxBatch: MAX_STREAM_EVENTS_PER_BATCH,
  });
  const onEvent = new Channel<HttpStreamEvent>((event) => {
    batcher.push(event);
  });

  try {
    const value = await invoke<HttpResponse>(SEND_REQUEST_COMMAND, {
      requestId,
      request: input,
      onEvent,
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
  } finally {
    // A cancelled or failed stream still reported blocks; the caller sees
    // them before it sees the outcome, rather than a frame after it.
    batcher.flush();
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

// http_client/src/services/grpc.ts
//
// The only place invoke() is called and a Channel is created for the gRPC
// commands in src-tauri/src/commands/grpc.rs (CLAUDE.md section 11, rule 3).
// One Channel per call carries its events; @tauri-apps/api re-orders channel
// messages by index, so they arrive here in the order Rust sent them.
//
// Nothing a call carries is logged: not the metadata, the auth or any
// message (section 11, rule 6). A call is logged by its target and method
// path, and its end by the status code and name — the status message is
// server text and may echo what was sent. For the same reason a refused
// message is logged by its error kind only: a JSON mapping error can quote
// the value it could not read (`quietCall` below).
import { Channel, invoke } from "@tauri-apps/api/core";

import { toApiError } from "@/lib/api-error";
import { createEventBatcher, type FlushScheduler } from "@/lib/event-batcher";
import { call } from "@/services/invoke";
import { logDebug, logWarn } from "@/services/logger";
import type {
  GrpcCallOutcome,
  GrpcEvent,
  GrpcRequestInput,
  ProtoSchema,
  ProtoSchemaSummary,
} from "@/types/grpc";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

const INVOKE_COMMAND = "grpc_invoke";
const CANCEL_COMMAND = "grpc_cancel";
const CANCEL_ALL_COMMAND = "grpc_cancel_all";

/** As for WebSocket: flush without waiting for a frame at this many queued
 * events, so a busy stream cannot grow the queue while frames are paused. */
export const MAX_EVENTS_PER_BATCH = 500;

const nextFrame: FlushScheduler = (flush) => {
  requestAnimationFrame(() => flush());
};

/** Messages arrive by the thousand; metadata, End Streaming and the end of
 * the call change what the tab shows and go at once. */
function isLifecycle(event: GrpcEvent): boolean {
  return event.type !== "sent" && event.type !== "received";
}

// --- Schemas -----------------------------------------------------------------

/** Asks the server for its schema. `request.methodPath` is ignored. */
export function reflectGrpc(request: GrpcRequestInput): Promise<Result<ProtoSchema, ApiError>> {
  return call<ProtoSchema>("grpc_reflect", { request });
}

/** The native chooser for an import's root files. Empty when cancelled. */
export function chooseProtoFiles(): Promise<Result<string[], ApiError>> {
  return call<string[]>("grpc_choose_proto_files");
}

/** The native chooser for one import path. null when cancelled. */
export function chooseImportFolder(): Promise<Result<string | null, ApiError>> {
  return call<string | null>("grpc_choose_import_folder");
}

/** Compiles the chosen files, reading only what their imports need. The
 * schema is loaded for the session; `saveProtoSchema` keeps it. */
export function importProto(
  roots: string[],
  importPaths: string[],
): Promise<Result<ProtoSchema, ApiError>> {
  return call<ProtoSchema>("grpc_import_proto", { roots, importPaths });
}

/** A schema loaded this session, or one in the library. */
export function getProtoSchema(schemaId: string): Promise<Result<ProtoSchema, ApiError>> {
  return call<ProtoSchema>("grpc_schema", { schemaId });
}

export function listProtoSchemas(): Promise<Result<ProtoSchemaSummary[], ApiError>> {
  return call<ProtoSchemaSummary[]>("grpc_list_schemas");
}

/** Saves a loaded schema to the library. A reflected schema comes back
 * with an id of its own, which is the one to store from then on. */
export function saveProtoSchema(
  schemaId: string,
  name: string,
): Promise<Result<ProtoSchema, ApiError>> {
  return call<ProtoSchema>("grpc_save_schema", { schemaId, name });
}

export async function renameProtoSchema(
  schemaId: string,
  name: string,
): Promise<Result<void, ApiError>> {
  const result = await call<null>("grpc_rename_schema", { schemaId, name });
  return result.ok ? { ok: true, value: undefined } : result;
}

/** Refused, naming the saved requests, while any of them uses the schema. */
export async function deleteProtoSchema(schemaId: string): Promise<Result<void, ApiError>> {
  const result = await call<null>("grpc_delete_schema", { schemaId });
  return result.ok ? { ok: true, value: undefined } : result;
}

/** "Use Example Message": JSON text for the method's input type. */
export function exampleMessage(
  schemaId: string,
  methodPath: string,
): Promise<Result<string, ApiError>> {
  return call<string>("grpc_example_message", { schemaId, methodPath });
}

// --- Calls -------------------------------------------------------------------

/**
 * Runs a call and resolves when it ends, with the same facts as its `ended`
 * event. A call that ends with a non-OK status is still `ok: true` here: the
 * status is the result. `ok: false` means the call never started (bad
 * metadata, an unknown method, a message that is not the input type, a
 * duplicate id).
 *
 * `message` is the one message of a unary or server-streaming call, fully
 * resolved; the client-streaming kinds send theirs with `sendGrpcMessage`
 * and pass an empty string. Events reach `onEvents` in batches, at most one
 * per animation frame, except that everything but messages flushes at once.
 *
 * `logTarget` is the authority with secret variables left as placeholders.
 */
export async function invokeGrpc(
  callId: string,
  schemaId: string,
  request: GrpcRequestInput,
  message: string,
  logTarget: string,
  onEvents: (events: GrpcEvent[]) => void,
  schedule: FlushScheduler = nextFrame,
): Promise<Result<GrpcCallOutcome, ApiError>> {
  const batcher = createEventBatcher(onEvents, schedule, {
    isUrgent: isLifecycle,
    maxBatch: MAX_EVENTS_PER_BATCH,
  });
  const channel = new Channel<GrpcEvent>((event) => batcher.push(event));
  const label = `grpc ${logTarget}${request.methodPath}`;

  try {
    const outcome = await invoke<GrpcCallOutcome>(INVOKE_COMMAND, {
      callId,
      schemaId,
      request,
      message,
      onEvent: channel,
    });
    logDebug(`${label} ended by the ${outcome.by}: ${outcome.status.code} ${outcome.status.name}`);
    return { ok: true, value: outcome };
  } catch (error) {
    const mapped = toApiError(error);
    logWarn(`${label} did not start: ${mapped.kind}`);
    return { ok: false, error: mapped };
  }
}

/**
 * Queues one message of a client-streaming or bidirectional call. Success
 * means it was accepted, not sent: its `sent` event follows. A message that
 * is not valid for the input type is refused here and the call goes on.
 */
export async function sendGrpcMessage(
  callId: string,
  message: string,
): Promise<Result<void, ApiError>> {
  const result = await quietCall<null>("grpc_send", { callId, message });
  return result.ok ? { ok: true, value: undefined } : result;
}

/**
 * `call` from services/invoke.ts, except that a failure is logged by the
 * command and the error kind only. For commands whose error text can quote
 * part of a message: the caller still gets the whole error to show.
 */
async function quietCall<T>(
  command: string,
  args: Record<string, unknown>,
): Promise<Result<T, ApiError>> {
  try {
    return { ok: true, value: await invoke<T>(command, args) };
  } catch (error) {
    const mapped = toApiError(error);
    logWarn(`command ${command} failed: ${mapped.kind}`);
    return { ok: false, error: mapped };
  }
}

/** End Streaming: the client has nothing more to send. Its `streamEnded`
 * event follows once it takes effect. */
export async function endGrpcStream(callId: string): Promise<Result<void, ApiError>> {
  const result = await call<null>("grpc_end_stream", { callId });
  return result.ok ? { ok: true, value: undefined } : result;
}

/** Fire-and-forget, like `disconnectWebSocket`: the call then ends with
 * CANCELLED, by the client, and `invokeGrpc` resolves with that. */
export async function cancelGrpc(callId: string): Promise<void> {
  try {
    await invoke(CANCEL_COMMAND, { callId });
  } catch {
    // Nothing useful to do: the call is ending or already over.
  }
}

/** Cancels every call Rust holds. Called once as the frontend starts, as
 * `disconnectAllWebSockets` is, so a reloaded webview leaves nothing
 * running with nobody to show its events to. */
export async function cancelAllGrpc(): Promise<void> {
  try {
    await invoke(CANCEL_ALL_COMMAND);
  } catch {
    // Nothing to report: at worst an orphaned call runs to its end.
  }
}

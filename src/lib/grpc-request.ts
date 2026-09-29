// http_client/src/lib/grpc-request.ts
//
// A gRPC tab's saveable shape and the small questions the tab asks about a
// schema, the counterpart of ws-request.ts for WebSocket tabs. Pure.
import type { Auth, KeyValue } from "@/types/http";
import {
  DEFAULT_GRPC_SETTINGS,
  type GrpcMethod,
  type GrpcMethodKind,
  type GrpcRequestDraft,
  type GrpcSchemaRef,
  type GrpcSettings,
  type ProtoSchema,
} from "@/types/grpc";

/** A new tab: TLS on (most public gRPC servers use it) and no schema. */
export function emptyGrpcDraft(): GrpcRequestDraft {
  return {
    url: "",
    tls: true,
    methodPath: "",
    schema: { kind: "none" },
    metadata: [],
    auth: { kind: "none" },
    message: "",
    settings: DEFAULT_GRPC_SETTINGS,
  };
}

/** Everything Save writes, and all that dirty tracking compares. */
export function grpcDraftsEqual(a: GrpcRequestDraft, b: GrpcRequestDraft): boolean {
  return (
    a.url === b.url &&
    a.tls === b.tls &&
    a.methodPath === b.methodPath &&
    schemaRefsEqual(a.schema, b.schema) &&
    pairsEqual(a.metadata, b.metadata) &&
    authEqual(a.auth, b.auth) &&
    a.message === b.message &&
    settingsEqual(a.settings, b.settings)
  );
}

/**
 * What a saved request records for a loaded schema. A reflected schema not
 * saved to the library is asked for again next time; anything else is
 * referenced by id. An imported schema that is not in the library yet gets
 * its session id here, which the backend refuses to save until the schema
 * itself is saved, so the Save flow (16i) saves the schema first.
 */
export function schemaRefFor(schema: ProtoSchema): GrpcSchemaRef {
  return schema.origin === "reflection" && schema.name === null
    ? { kind: "reflection" }
    : { kind: "library", schemaId: schema.id };
}

export function findMethod(schema: ProtoSchema | null, path: string): GrpcMethod | null {
  if (schema === null || path === "") {
    return null;
  }
  for (const service of schema.services) {
    const found = service.methods.find((method) => method.path === path);
    if (found !== undefined) {
      return found;
    }
  }
  return null;
}

/** Whether the client sends a stream: Send and End Streaming apply. */
export function clientStreams(kind: GrpcMethodKind): boolean {
  return kind === "clientStreaming" || kind === "bidirectional";
}

/** Whether the server answers with a stream rather than one message. */
export function serverStreams(kind: GrpcMethodKind): boolean {
  return kind === "serverStreaming" || kind === "bidirectional";
}

function schemaRefsEqual(a: GrpcSchemaRef, b: GrpcSchemaRef): boolean {
  if (a.kind === "library" && b.kind === "library") {
    return a.schemaId === b.schemaId;
  }
  return a.kind === b.kind;
}

function pairsEqual(a: readonly KeyValue[], b: readonly KeyValue[]): boolean {
  return (
    a.length === b.length &&
    a.every((pair, index) => {
      const other = b[index];
      return other !== undefined && pair.name === other.name && pair.value === other.value;
    })
  );
}

/** Field by field over whichever variant both are; the variants are flat
 * records of strings, so this is exact. */
function authEqual(a: Auth, b: Auth): boolean {
  if (a.kind !== b.kind) {
    return false;
  }
  const left: Record<string, unknown> = a;
  const right: Record<string, unknown> = b;
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  return [...keys].every((key) => left[key] === right[key]);
}

function settingsEqual(a: GrpcSettings, b: GrpcSettings): boolean {
  return (
    a.verifyTls === b.verifyTls &&
    a.proxy === b.proxy &&
    a.connectTimeoutMs === b.connectTimeoutMs &&
    a.deadlineMs === b.deadlineMs &&
    a.maxReceiveBytes === b.maxReceiveBytes &&
    a.includeDefaults === b.includeDefaults
  );
}

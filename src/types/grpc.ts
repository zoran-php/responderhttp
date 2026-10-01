// http_client/src/types/grpc.ts
//
// Mirrors the gRPC DTOs in src-tauri/src/commands/grpc_dto.rs. Any change
// there has to land here in the same commit — these two files are one
// contract, and grpc_dto.rs pins the key names in its tests.
//
// Messages cross as JSON **text**, never as parsed values, so a 64-bit
// integer never passes through JavaScript's number type (PLAN-GRPC.md
// section 3, point 5). Durations are whole milliseconds.
import type { Auth, KeyValue, SecretState } from "@/types/http";

// --- Settings and requests ---------------------------------------------------

/** Mirrors GrpcSettingsDto. */
export interface GrpcSettings {
  /** Off is an explicit per-request opt-in, never the default. */
  verifyTls: boolean;
  proxy: string | null;
  connectTimeoutMs: number;
  /** null: no deadline (D5). */
  deadlineMs: number | null;
  maxReceiveBytes: number;
  includeDefaults: boolean;
}

/** Mirrors `GrpcSettings::default()` in domain/models.rs. */
export const DEFAULT_GRPC_SETTINGS: GrpcSettings = {
  verifyTls: true,
  proxy: null,
  connectTimeoutMs: 30_000,
  deadlineMs: null,
  maxReceiveBytes: 4 * 1024 * 1024,
  includeDefaults: false,
};

/** Mirrors GrpcTargetDto: `host:port`, as HTTP/2 puts it in `:authority`. */
export interface GrpcTarget {
  authority: string;
  tls: boolean;
}

/** Mirrors GrpcRequestInput: one call or reflection, fully resolved. */
export interface GrpcRequestInput {
  target: GrpcTarget;
  /** `/package.Service/Method`. Ignored by reflection. */
  methodPath: string;
  metadata: KeyValue[];
  auth: Auth;
  settings: GrpcSettings;
}

/** Mirrors GrpcSchemaRefDto: which schema a saved request uses. */
export type GrpcSchemaRef =
  { kind: "none" } | { kind: "reflection" } | { kind: "library"; schemaId: string };

/** Mirrors GrpcRequestDraftDto: a gRPC tab's request, unresolved. */
export interface GrpcRequestDraft {
  /** As typed, `{{variables}}` and all. */
  url: string;
  tls: boolean;
  /** `/package.Service/Method`; empty until a method is picked. */
  methodPath: string;
  schema: GrpcSchemaRef;
  metadata: KeyValue[];
  auth: Auth;
  /** The Message tab's JSON text, exactly as typed. */
  message: string;
  settings: GrpcSettings;
}

/**
 * Mirrors SavedGrpcRequestDto. Shares the request id space: rename, move,
 * delete and docs reach it through the request commands by id.
 */
export interface SavedGrpcRequest {
  id: string;
  collectionId: string;
  folderId: string | null;
  name: string;
  request: GrpcRequestDraft;
  secretState: SecretState;
}

// --- Schemas -----------------------------------------------------------------

/** Mirrors MethodKindDto. */
export type GrpcMethodKind = "unary" | "serverStreaming" | "clientStreaming" | "bidirectional";

/** Mirrors GrpcMethodDto. */
export interface GrpcMethod {
  name: string;
  /** `/package.Service/Method`. */
  path: string;
  kind: GrpcMethodKind;
  inputType: string;
  outputType: string;
}

/** Mirrors GrpcServiceDto. */
export interface GrpcService {
  /** Fully qualified, e.g. `shop.v1.Shop`. */
  name: string;
  methods: GrpcMethod[];
}

/** Mirrors SchemaOriginDto. */
export type SchemaOrigin = "import" | "reflection";

/** Mirrors ProtoSchemaDto: a loaded schema as the method picker needs it. */
export interface ProtoSchema {
  id: string;
  /** Its name in the library; null until saved. */
  name: string | null;
  origin: SchemaOrigin;
  services: GrpcService[];
  /** Import names of the files an import brought in; empty when reflected. */
  files: string[];
}

/** Mirrors ProtoSchemaSummaryDto: one entry of the schema library. */
export interface ProtoSchemaSummary {
  id: string;
  name: string;
  origin: SchemaOrigin;
  updatedAt: string;
}

// --- Calls -------------------------------------------------------------------

/** Mirrors GrpcStatusDto. The name comes from Rust, where the table of
 * codes lives. */
export interface GrpcStatus {
  code: number;
  name: string;
  message: string;
}

/** Mirrors GrpcEndedByDto. `client` marks a status the app produced itself:
 * Cancel, a passed deadline, a message over the limit, a broken response or
 * a transport failure. */
export type GrpcEndedBy = "server" | "client";

/** Mirrors GrpcEventDto, tagged by `type`. `atMs` is Unix time in ms;
 * `bytes` is the protobuf size, measured in Rust. */
export type GrpcEvent =
  | { type: "responseMetadata"; atMs: number; metadata: KeyValue[] }
  | { type: "sent"; atMs: number; json: string; bytes: number }
  | { type: "received"; atMs: number; json: string; bytes: number }
  /** End Streaming took effect. */
  | { type: "streamEnded"; atMs: number }
  | {
      type: "ended";
      atMs: number;
      status: GrpcStatus;
      trailers: KeyValue[];
      by: GrpcEndedBy;
      totalMs: number;
    };

/** Mirrors GrpcCallOutcomeDto: what `grpc_invoke` resolves with, the same
 * facts as the `ended` event. */
export interface GrpcCallOutcome {
  status: GrpcStatus;
  by: GrpcEndedBy;
  trailers: KeyValue[];
  totalMs: number;
}

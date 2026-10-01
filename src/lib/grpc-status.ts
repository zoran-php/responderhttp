// http_client/src/lib/grpc-status.ts
//
// What a gRPC tab's badge, button and method list say, from the store's
// state. Pure, the counterpart of ws-status.ts.
import type { GrpcLogFilter } from "@/lib/grpc-log";
import type { GrpcCallOutcome, GrpcMethodKind } from "@/types/grpc";

export type GrpcBadgeTone = "idle" | "pending" | "ok" | "error";

export interface GrpcBadge {
  label: string;
  tone: GrpcBadgeTone;
}

export type GrpcCallState = "idle" | "running" | "cancelling";

/**
 * The response badge: the status once a call has ended (`0 OK`, `5
 * NOT_FOUND`), what is happening while one runs, and a neutral word before
 * the first. Any code but OK is an error tone, whoever produced it.
 */
export function grpcBadge(call: GrpcCallState, outcome: GrpcCallOutcome | null): GrpcBadge {
  if (call === "running") {
    return { label: "Running…", tone: "pending" };
  }
  if (call === "cancelling") {
    return { label: "Cancelling…", tone: "pending" };
  }
  if (outcome === null) {
    return { label: "Not invoked", tone: "idle" };
  }
  const { code, name } = outcome.status;
  return { label: `${code} ${name}`, tone: code === 0 ? "ok" : "error" };
}

export type GrpcPrimaryAction =
  | { kind: "invoke"; label: "Invoke" }
  | { kind: "cancel"; label: "Cancel" }
  | { kind: "busy"; label: "Cancelling…" };

/** Invoke, or Cancel while a call runs. */
export function grpcPrimaryAction(call: GrpcCallState): GrpcPrimaryAction {
  switch (call) {
    case "idle":
      return { kind: "invoke", label: "Invoke" };
    case "running":
      return { kind: "cancel", label: "Cancel" };
    case "cancelling":
      return { kind: "busy", label: "Cancelling…" };
  }
}

const KIND_LABELS: Record<GrpcMethodKind, string> = {
  unary: "Unary",
  serverStreaming: "Server streaming",
  clientStreaming: "Client streaming",
  bidirectional: "Bidirectional",
};

/** The badge beside a method in the picker. */
export function methodKindLabel(kind: GrpcMethodKind): string {
  return KIND_LABELS[kind];
}

export const GRPC_LOG_FILTER_OPTIONS: readonly { value: GrpcLogFilter; label: string }[] = [
  { value: "all", label: "All Messages" },
  { value: "sent", label: "Sent" },
  { value: "received", label: "Received" },
  { value: "system", label: "Call events" },
];

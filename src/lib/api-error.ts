// http_client/src/lib/api-error.ts
//
// Every invoke() wrapper in services/ has to turn a rejected promise into a
// typed ApiError: a rejected invoke() carries whatever the command
// serialised, but it can also reject with a plain string if the command
// itself never ran (bad arguments, unknown command). One mapping, shared by
// every service, rather than reimplementing it per command file.
import type { ApiError } from "@/types/http";

const API_ERROR_KINDS: ApiError["kind"][] = [
  "invalidRequest",
  "transport",
  "cancelled",
  "notFound",
  "storage",
  "internal",
  "secretStore",
];

export function toApiError(error: unknown): ApiError {
  if (isApiError(error)) {
    return error;
  }
  return {
    kind: "internal",
    message: typeof error === "string" ? error : "Unexpected error from the backend",
  };
}

function isApiError(error: unknown): error is ApiError {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  const candidate = error as { kind?: unknown; message?: unknown };
  return (
    API_ERROR_KINDS.some((kind) => kind === candidate.kind) &&
    typeof candidate.message === "string"
  );
}

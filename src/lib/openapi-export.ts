// http_client/src/lib/openapi-export.ts
//
// What the OpenAPI export dialog says about the requests it leaves out:
// WebSocket (PLAN-WEBSOCKET.md 13g) and gRPC (PLAN-GRPC.md D8). One
// sentence with the counts, shown before the export, instead of a note per
// request after it.

/** Null when there is nothing to say. */
export function nonHttpOmissionNotice(webSocketCount: number, grpcCount = 0): string | null {
  const parts: string[] = [];
  if (webSocketCount > 0) {
    parts.push(plural(webSocketCount, "WebSocket request"));
  }
  if (grpcCount > 0) {
    parts.push(plural(grpcCount, "gRPC request"));
  }
  if (parts.length === 0) {
    return null;
  }
  const them =
    webSocketCount > 0 && grpcCount > 0
      ? "them"
      : webSocketCount > 0
        ? "WebSocket requests"
        : "gRPC requests";
  return (
    `${parts.join(" and ")} will not be exported: the OpenAPI specification describes ` +
    `HTTP requests only and has no way to describe ${them}.`
  );
}

function plural(count: number, noun: string): string {
  return count === 1 ? `1 ${noun}` : `${count} ${noun}s`;
}

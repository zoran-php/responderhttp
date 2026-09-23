// http_client/src/lib/openapi-export.ts
//
// What the OpenAPI export dialog says about the WebSocket requests it leaves
// out (PLAN-WEBSOCKET.md 13g). One sentence with the count, shown before the
// export, instead of a note per request after it.

/** Null when there is nothing to say. */
export function webSocketOmissionNotice(count: number): string | null {
  if (count <= 0) {
    return null;
  }
  const subject = count === 1 ? "1 WebSocket request" : `${count} WebSocket requests`;
  return (
    `${subject} will not be exported: the OpenAPI specification describes ` +
    "HTTP requests only and has no way to describe WebSocket requests."
  );
}

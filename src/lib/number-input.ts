// http_client/src/lib/number-input.ts
//
// What a numeric settings field stores for what was typed. Shared by the
// WebSocket and gRPC settings panels.

/** An integer within [min, max]; anything that does not parse is `min`. */
export function clampInt(raw: string, min: number, max: number): number {
  const parsed = Number.parseInt(raw, 10);
  if (Number.isNaN(parsed)) {
    return min;
  }
  return Math.min(max, Math.max(min, parsed));
}

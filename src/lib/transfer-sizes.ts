// http_client/src/lib/transfer-sizes.ts
//
// What the size badge and its breakdown show (PLAN-SSE.md, 14f). Pure: the
// numbers come from Rust, this only decides which ones are known yet.
import type { ResponseStream } from "@/lib/sse-log";
import type { TransferSizes } from "@/types/http";

export interface SizeBreakdown {
  /**
   * Null while the request is still running. libcurl reports what it sent
   * only once the transfer ends, and a stream may stay open for minutes —
   * so the request half is unknown rather than zero until then.
   */
  requestHeaders: number | null;
  requestBody: number | null;
  responseHeaders: number;
  responseBody: number;
  /** What the badge itself shows: the response, headers and body together. */
  responseTotal: number;
}

/** A finished request: every number is known. */
export function sizesOfResponse(sizes: TransferSizes): SizeBreakdown {
  return {
    requestHeaders: sizes.requestHeaders,
    requestBody: sizes.requestBody,
    responseHeaders: sizes.responseHeaders,
    responseBody: sizes.responseBody,
    responseTotal: sizes.responseHeaders + sizes.responseBody,
  };
}

/**
 * A stream still arriving. The body is the blocks reported so far, which is
 * every byte the parser has finished with — a block still being read counts
 * once it is complete, which is at most one block behind.
 */
export function sizesOfStream(stream: ResponseStream): SizeBreakdown {
  const responseHeaders = stream.head?.bytes ?? 0;
  return {
    requestHeaders: null,
    requestBody: null,
    responseHeaders,
    responseBody: stream.receivedBytes,
    responseTotal: responseHeaders + stream.receivedBytes,
  };
}

import { describe, expect, it } from "vitest";

import { EMPTY_STREAM, applyStreamEvents } from "@/lib/sse-log";
import { sizesOfResponse, sizesOfStream } from "@/lib/transfer-sizes";
import type { HttpStreamEvent, TransferSizes } from "@/types/http";

const sizes: TransferSizes = {
  requestHeaders: 241,
  requestBody: 0,
  responseHeaders: 352,
  responseBody: 757,
};

const headers: HttpStreamEvent = {
  type: "headers",
  status: 200,
  headers: [{ name: "content-type", value: "text/event-stream" }],
  bytes: 120,
};

const block = (data: string): HttpStreamEvent => {
  const raw = `data: ${data}\n\n`;
  return {
    type: "block",
    atMs: 1,
    bytes: raw.length,
    block: { kind: "event", name: "message", data, id: null, retry: null, raw },
  };
};

describe("sizesOfResponse", () => {
  it("totals the response halves and keeps the request ones", () => {
    expect(sizesOfResponse(sizes)).toEqual({
      requestHeaders: 241,
      requestBody: 0,
      responseHeaders: 352,
      responseBody: 757,
      responseTotal: 1109,
    });
  });
});

describe("sizesOfStream", () => {
  it("counts the header block and every event so far", () => {
    const stream = applyStreamEvents(EMPTY_STREAM, [headers, block("one"), block("two")]);

    expect(sizesOfStream(stream)).toEqual({
      requestHeaders: null,
      requestBody: null,
      responseHeaders: 120,
      responseBody: 22,
      responseTotal: 142,
    });
  });

  it("is all zeroes before the headers arrive, not a broken number", () => {
    expect(sizesOfStream(EMPTY_STREAM)).toEqual({
      requestHeaders: null,
      requestBody: null,
      responseHeaders: 0,
      responseBody: 0,
      responseTotal: 0,
    });
  });
});

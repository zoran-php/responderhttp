// http_client/src/features/response-viewer/EventStreamView.test.tsx
//
// The Events and Raw views of a live stream (PLAN-SSE.md, 14d). Built from
// the real fold in lib/sse-log.ts rather than a hand-made stream, so the
// test breaks if the two stop agreeing.
//
// Monaco is stubbed, as in WebSocketView.test.tsx, and plain assertions are
// used for the same reason: this project has no jest-dom setup.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { EventStreamView } from "@/features/response-viewer/EventStreamView";
import { applyStreamEvents, EMPTY_STREAM, type ResponseStream } from "@/lib/sse-log";
import type { HttpResponse, HttpStreamEvent } from "@/types/http";

vi.mock("@/components/LazyCodeEditor", () => ({
  LazyCodeEditor: ({ value }: { value: string }) => (
    <textarea aria-label="Response body" readOnly value={value} />
  ),
}));

const headers: HttpStreamEvent = {
  type: "headers",
  status: 200,
  headers: [{ name: "content-type", value: "text/event-stream" }],
  bytes: 120,
};

/** Everything here is ASCII, so a raw block's length is its byte count. */
const event = (data: string, name = "message", id: string | null = null): HttpStreamEvent => {
  const raw = `data: ${data}\n\n`;
  return {
    type: "block",
    atMs: Date.UTC(2026, 8, 22, 10, 0, 1, 250),
    bytes: raw.length,
    block: { kind: "event", name, data, id, retry: null, raw },
  };
};

const comment: HttpStreamEvent = {
  type: "block",
  atMs: Date.UTC(2026, 8, 22, 10, 0, 1, 500),
  bytes: ": keep-alive\n\n".length,
  block: { kind: "comment", text: "keep-alive", raw: ": keep-alive\n\n" },
};

const finished: HttpResponse = {
  status: 200,
  headers: [{ name: "content-type", value: "text/event-stream" }],
  body: { kind: "text", text: "data: one\n\ndata: two\n\n" },
  timing: { dnsMs: 1, connectMs: 1, tlsMs: 1, timeToFirstByteMs: 2, totalMs: 40 },
  sizes: { requestHeaders: 241, requestBody: 0, responseHeaders: 352, responseBody: 757 },
};

function show(
  stream: ResponseStream,
  {
    isSending = true,
    response = null,
  }: { isSending?: boolean; response?: HttpResponse | null } = {},
) {
  render(
    <EventStreamView
      error={null}
      isSending={isSending}
      response={response}
      saveExample={{ disabledReason: "Save the request first" }}
      stream={stream}
    />,
  );
}

const streamed = (...events: HttpStreamEvent[]): ResponseStream =>
  applyStreamEvents(EMPTY_STREAM, [headers, ...events]);

afterEach(() => {
  cleanup();
});

describe("while the stream is running", () => {
  it("lists every event as it arrives", () => {
    show(streamed(event("one"), event("two", "end", "7")));

    expect(screen.getByText("one")).toBeDefined();
    expect(screen.getByText("two")).toBeDefined();
    expect(screen.getByText("end")).toBeDefined();
    expect(screen.getByText("id 7")).toBeDefined();
    expect(screen.getByText(/2 events/)).toBeDefined();
  });

  it("puts the newest event at the top, as the WebSocket log does", () => {
    show(streamed(event("one"), event("two"), event("three")));

    const rows = screen.getAllByRole("listitem").map((row) => row.textContent ?? "");
    expect(rows.map((row) => row.includes("three"))).toEqual([true, false, false]);
    expect(rows[2]?.includes("one")).toBe(true);
  });

  it("shows how much has arrived, headers included", () => {
    show(streamed(event("one")));

    // 120 bytes of headers plus 11 of "data: one\n\n".
    expect(screen.getByText("1 event")).toBeDefined();
    expect(screen.getByRole("button", { name: "Response size" }).textContent).toBe("131 B");
  });

  it("breaks the size down, with the request half not known yet", () => {
    show(streamed(event("one")));

    const card = screen.getByRole("tooltip").textContent ?? "";
    expect(card).toContain("Response Size");
    expect(card).toContain("Request Size");
    // libcurl reports what it sent only once the transfer has ended.
    expect(card).toContain("—");
  });

  it("says so, and keeps the status it already has", () => {
    show(streamed(event("one")));

    expect(screen.getByText("Streaming")).toBeDefined();
    expect(screen.getByText("200")).toBeDefined();
  });

  it("shows a comment as one rather than as an empty event", () => {
    show(streamed(comment));

    expect(screen.getByText("comment")).toBeDefined();
    expect(screen.getByText("keep-alive")).toBeDefined();
  });

  it("says nothing has arrived yet rather than showing an empty list", () => {
    show(streamed());

    expect(screen.getByText("Waiting for the first event…")).toBeDefined();
  });

  it("shows the blocks exactly as they arrived in the Raw view", () => {
    show(streamed(event("one"), event("two")));

    fireEvent.click(screen.getByRole("button", { name: "Raw" }));

    expect(screen.getByLabelText("Raw stream").textContent).toBe("data: one\n\ndata: two\n\n");
  });
});

describe("once it has ended", () => {
  it("keeps the events, and takes the Raw view from the response body", () => {
    show(streamed(event("one")), { isSending: false, response: finished });

    expect(screen.getByText("one")).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: "Raw" }));
    expect((screen.getByLabelText("Response body") as HTMLTextAreaElement).value).toBe(
      "data: one\n\ndata: two\n\n",
    );
  });

  it("takes every size from the response, which knows what was sent too", () => {
    show(streamed(event("one")), { isSending: false, response: finished });

    expect(screen.getByRole("button", { name: "Response size" }).textContent).toBe("1.1 KB");
    const card = screen.getByRole("tooltip").textContent ?? "";
    expect(card).toContain("241 B");
    expect(card).toContain("757 B");
    expect(card).not.toContain("—");
  });

  it("lists the response headers", () => {
    show(streamed(event("one")), { isSending: false, response: finished });

    fireEvent.click(screen.getByRole("button", { name: "Headers" }));

    expect(screen.getByText("content-type")).toBeDefined();
    expect(screen.getByText("text/event-stream")).toBeDefined();
  });
});

describe("when the caps have dropped something", () => {
  it("says how many events are missing", () => {
    const caps = { maxEntries: 1, maxBytes: 1024 };
    const stream = applyStreamEvents(EMPTY_STREAM, [headers, event("one"), event("two")], caps);

    show(stream);

    expect(screen.getByText("1 earlier event dropped")).toBeDefined();
    // At the bottom: it is the oldest end of a newest-first list.
    const rows = screen.getAllByRole("listitem");
    expect(rows[rows.length - 1]?.textContent).toBe("1 earlier event dropped");
  });
});

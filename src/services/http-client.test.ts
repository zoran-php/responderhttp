// http_client/src/services/http-client.test.ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { cancelRequest, MAX_STREAM_EVENTS_PER_BATCH, sendRequest } from "@/services/http-client";
import {
  AUTH_NONE,
  DEFAULT_SETTINGS,
  type HttpResponse,
  type HttpStreamEvent,
  type SendRequestInput,
} from "@/types/http";

interface FakeChannel {
  deliver: (event: HttpStreamEvent) => void;
}

const { invoke, channels } = vi.hoisted(() => ({
  invoke: vi.fn(),
  channels: [] as FakeChannel[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  isTauri: () => false,
  // Stands in for the real Channel, which needs the Tauri host. It keeps the
  // handler so a test can play the part of Rust.
  Channel: class {
    readonly deliver: (event: HttpStreamEvent) => void;
    constructor(onmessage: (event: HttpStreamEvent) => void) {
      this.deliver = onmessage;
      channels.push(this);
    }
  },
}));
vi.mock("@tauri-apps/plugin-log", () => ({
  trace: vi.fn(),
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}));

const input: SendRequestInput = {
  method: "GET",
  url: "https://example.com/",
  headers: [],
  queryParams: [],
  body: { kind: "none" },
  auth: AUTH_NONE,
  settings: DEFAULT_SETTINGS,
};

const response: HttpResponse = {
  status: 200,
  headers: [{ name: "content-type", value: "text/plain" }],
  body: { kind: "text", text: "hello" },
  timing: { dnsMs: 1, connectMs: 2, tlsMs: 3, timeToFirstByteMs: 4, totalMs: 10 },
  sizes: { requestHeaders: 241, requestBody: 0, responseHeaders: 352, responseBody: 757 },
};

/** A stream nobody is watching: what every non-streaming test wants. */
const ignore = (): void => undefined;

describe("sendRequest", () => {
  beforeEach(() => {
    invoke.mockReset();
    channels.length = 0;
  });

  it("passes the id and input under the argument names the command expects", async () => {
    invoke.mockResolvedValue(response);

    const result = await sendRequest("req-1", input, input.url, ignore);

    expect(invoke).toHaveBeenCalledWith("send_request", {
      requestId: "req-1",
      request: input,
      onEvent: channels[0],
    });
    expect(result).toEqual({ ok: true, value: response });
  });

  it("returns a typed failure instead of throwing when the command rejects", async () => {
    invoke.mockRejectedValue({ kind: "transport", message: "connection reset" });

    const result = await sendRequest("req-1", input, input.url, ignore);

    expect(result).toEqual({
      ok: false,
      error: { kind: "transport", message: "connection reset" },
    });
  });

  it("recognises a cancellation as its own error kind", async () => {
    invoke.mockRejectedValue({ kind: "cancelled", message: "request cancelled" });

    const result = await sendRequest("req-1", input, input.url, ignore);

    expect(result).toEqual({
      ok: false,
      error: { kind: "cancelled", message: "request cancelled" },
    });
  });

  it("maps a non-ApiError rejection to an internal error", async () => {
    invoke.mockRejectedValue("command send_request not found");

    const result = await sendRequest("req-1", input, input.url, ignore);

    expect(result).toEqual({
      ok: false,
      error: { kind: "internal", message: "command send_request not found" },
    });
  });
});

/** Outside Tauri the logger writes to the console only, so the console is
 * where the lines are checked. The file sink receives the same string. */
describe("what reaches the log", () => {
  let debugLines: ReturnType<typeof vi.spyOn>;
  let warnLines: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    invoke.mockReset();
    debugLines = vi.spyOn(console, "debug").mockImplementation(() => undefined);
    warnLines = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const resolved: SendRequestInput = { ...input, url: "https://example.com/?key=live-secret" };
  const logUrl = "https://example.com/?key={{secret}}";

  it("logs the URL it is given, never the resolved one", async () => {
    invoke.mockResolvedValue(response);

    await sendRequest("req-1", resolved, logUrl, ignore);

    expect(invoke).toHaveBeenCalledWith(
      "send_request",
      // The channel is checked where it is the point; here only the URL is.
      expect.objectContaining({ requestId: "req-1", request: resolved }),
    );
    const lines = JSON.stringify(debugLines.mock.calls);
    expect(lines).toContain("{{secret}}");
    expect(lines).not.toContain("live-secret");
  });

  it("keeps the resolved URL out of a failure line too", async () => {
    invoke.mockRejectedValue({ kind: "transport", message: "connection reset" });

    await sendRequest("req-1", resolved, logUrl, ignore);

    const lines = JSON.stringify(warnLines.mock.calls);
    expect(lines).toContain("{{secret}}");
    expect(lines).not.toContain("live-secret");
  });
});

/**
 * The streaming half (PLAN-SSE.md, 14c). The command resolves with the whole
 * response either way; these check what arrives before it does.
 */
describe("what a request reports while it is running", () => {
  beforeEach(() => {
    invoke.mockReset();
    channels.length = 0;
  });

  const headers: HttpStreamEvent = {
    type: "headers",
    status: 200,
    headers: [{ name: "content-type", value: "text/event-stream" }],
    bytes: 120,
  };

  const block = (data: string): HttpStreamEvent => ({
    type: "block",
    atMs: 1,
    bytes: `data: ${data}\n\n`.length,
    block: {
      kind: "event",
      name: "message",
      data,
      id: null,
      retry: null,
      raw: `data: ${data}\n\n`,
    },
  });

  /** Sends with a scheduler the test runs by hand, like websocket.test.ts. */
  function sending() {
    const frames: (() => void)[] = [];
    const batches: HttpStreamEvent[][] = [];
    let resolve: (value: HttpResponse) => void = () => undefined;
    invoke.mockReturnValue(
      new Promise<HttpResponse>((settle) => {
        resolve = settle;
      }),
    );

    const sent = sendRequest(
      "req-1",
      input,
      input.url,
      (batch) => batches.push(batch),
      (flush) => frames.push(flush),
    );
    const channel = channels[0];
    if (channel === undefined) {
      throw new Error("no channel was created");
    }
    return { sent, channel, frames, batches, finish: () => resolve(response) };
  }

  it("reports the headers without waiting for a frame", async () => {
    const { sent, channel, frames, batches, finish } = sending();

    channel.deliver(headers);

    expect(batches).toEqual([[headers]]);
    expect(frames).toHaveLength(0);
    finish();
    await sent;
  });

  it("coalesces blocks into one batch a frame, in arrival order", async () => {
    const { sent, channel, frames, batches, finish } = sending();

    channel.deliver(block("one"));
    channel.deliver(block("two"));

    expect(batches).toEqual([]);
    frames[0]?.();
    expect(batches).toEqual([[block("one"), block("two")]]);
    finish();
    await sent;
  });

  it("stops waiting for a frame once the queue is full", async () => {
    const { sent, channel, batches, finish } = sending();

    for (let index = 0; index < MAX_STREAM_EVENTS_PER_BATCH; index += 1) {
      channel.deliver(block(String(index)));
    }

    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(MAX_STREAM_EVENTS_PER_BATCH);
    finish();
    await sent;
  });

  it("delivers the last blocks before it resolves, not a frame later", async () => {
    const { sent, channel, batches, finish } = sending();

    channel.deliver(block("last"));
    finish();
    await sent;

    expect(batches).toEqual([[block("last")]]);
  });

  it("delivers what arrived before a failure too", async () => {
    const frames: (() => void)[] = [];
    const batches: HttpStreamEvent[][] = [];
    let reject: (error: unknown) => void = () => undefined;
    invoke.mockReturnValue(
      new Promise<HttpResponse>((_settle, fail) => {
        reject = fail;
      }),
    );
    const sent = sendRequest(
      "req-1",
      input,
      input.url,
      (batch) => batches.push(batch),
      (flush) => frames.push(flush),
    );
    channels[0]?.deliver(block("before the cut"));

    reject({ kind: "cancelled", message: "request cancelled" });
    const result = await sent;

    expect(result.ok).toBe(false);
    expect(batches).toEqual([[block("before the cut")]]);
  });

  it("keeps the blocks out of the log: they are the response body", async () => {
    const debugLines = vi.spyOn(console, "debug").mockImplementation(() => undefined);
    const { sent, channel, finish } = sending();

    channel.deliver(block("a secret the server streamed"));
    finish();
    await sent;

    expect(JSON.stringify(debugLines.mock.calls)).not.toContain("a secret the server streamed");
    debugLines.mockRestore();
  });
});

describe("cancelRequest", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes the id through", async () => {
    invoke.mockResolvedValue(undefined);

    await cancelRequest("req-1");

    expect(invoke).toHaveBeenCalledWith("cancel_request", { requestId: "req-1" });
  });

  it("stays quiet when the command rejects", async () => {
    invoke.mockRejectedValue("no such request");

    await expect(cancelRequest("req-1")).resolves.toBeUndefined();
  });
});

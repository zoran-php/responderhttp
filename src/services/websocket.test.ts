import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  connectWebSocket,
  disconnectAllWebSockets,
  disconnectWebSocket,
  MAX_EVENTS_PER_BATCH,
  sendWebSocketMessage,
} from "@/services/websocket";
import { DEFAULT_WS_SETTINGS, type WebSocketRequest, type WsEvent } from "@/types/websocket";

interface FakeChannel {
  deliver: (event: WsEvent) => void;
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
    readonly deliver: (event: WsEvent) => void;
    constructor(onmessage: (event: WsEvent) => void) {
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

const request: WebSocketRequest = {
  url: "wss://example.com/socket",
  headers: [],
  settings: DEFAULT_WS_SETTINGS,
};

const received = (text: string): WsEvent => ({
  type: "received",
  atMs: 1,
  byteLength: text.length,
  payload: { kind: "text", text },
});

const closed: WsEvent = { type: "closed", atMs: 2, code: 1000, reason: "", by: "server" };

/** Connects with a scheduler the test runs by hand, and returns the channel. */
async function connect() {
  const frames: (() => void)[] = [];
  const batches: WsEvent[][] = [];
  invoke.mockResolvedValue(null);

  const result = await connectWebSocket(
    "ws-1",
    request,
    request.url,
    (batch) => batches.push(batch),
    (flush) => frames.push(flush),
  );

  const channel = channels[channels.length - 1];
  if (channel === undefined) {
    throw new Error("connect should create a channel");
  }
  const runFrame = () => {
    for (const flush of frames.splice(0)) {
      flush();
    }
  };
  return { result, channel, batches, runFrame };
}

describe("connectWebSocket", () => {
  beforeEach(() => {
    invoke.mockReset();
    channels.length = 0;
  });

  it("passes the id, request and channel under the names the command expects", async () => {
    const { result, channel } = await connect();

    expect(invoke).toHaveBeenCalledWith("connect_web_socket", {
      connectionId: "ws-1",
      request,
      onEvent: channel,
    });
    expect(result).toEqual({ ok: true, value: undefined });
  });

  it("delivers messages once per frame, in the order they arrived", async () => {
    const { channel, batches, runFrame } = await connect();

    channel.deliver(received("1"));
    channel.deliver(received("2"));
    channel.deliver(received("3"));
    expect(batches).toEqual([]);

    runFrame();
    expect(batches).toEqual([[received("1"), received("2"), received("3")]]);
  });

  it("flushes a closed event at once, after the messages before it", async () => {
    const { channel, batches } = await connect();

    channel.deliver(received("last"));
    channel.deliver(closed);

    expect(batches).toEqual([[received("last"), closed]]);
  });

  it("does not wait for a frame once the batch is full", async () => {
    const { channel, batches } = await connect();

    for (let index = 0; index < MAX_EVENTS_PER_BATCH; index += 1) {
      channel.deliver(received(String(index)));
    }

    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(MAX_EVENTS_PER_BATCH);
  });

  it("returns a typed failure when the handshake is refused", async () => {
    invoke.mockRejectedValue({
      kind: "transport",
      message: "server refused the WebSocket upgrade: HTTP 403",
    });

    const result = await connectWebSocket("ws-1", request, request.url, () => undefined);

    expect(result).toEqual({
      ok: false,
      error: { kind: "transport", message: "server refused the WebSocket upgrade: HTTP 403" },
    });
  });
});

describe("sendWebSocketMessage", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes the message under the name the command expects", async () => {
    invoke.mockResolvedValue(null);

    const result = await sendWebSocketMessage("ws-1", { kind: "binary", hex: "dead" });

    expect(invoke).toHaveBeenCalledWith("send_web_socket_message", {
      connectionId: "ws-1",
      message: { kind: "binary", hex: "dead" },
    });
    expect(result).toEqual({ ok: true, value: undefined });
  });

  it("returns a typed failure when the connection is not open", async () => {
    invoke.mockRejectedValue({ kind: "invalidRequest", message: "connection ws-1 is not open" });

    const result = await sendWebSocketMessage("ws-1", { kind: "text", text: "hi" });

    expect(result).toEqual({
      ok: false,
      error: { kind: "invalidRequest", message: "connection ws-1 is not open" },
    });
  });
});

describe("disconnectWebSocket", () => {
  it("never throws, even when the command fails", async () => {
    invoke.mockRejectedValue("gone");

    await expect(disconnectWebSocket("ws-1")).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("disconnect_web_socket", { connectionId: "ws-1" });
  });
});

describe("disconnectAllWebSockets", () => {
  it("calls the command with no arguments and never throws", async () => {
    invoke.mockReset().mockRejectedValue("gone");

    await expect(disconnectAllWebSockets()).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("disconnect_all_web_sockets");
  });
});

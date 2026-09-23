// http_client/src/services/collections.test.ts
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as collections from "@/services/collections";
import { AUTH_NONE, DEFAULT_SETTINGS, type SendRequestInput } from "@/types/http";
import { DEFAULT_WS_SETTINGS, type WebSocketRequest, type WsDraft } from "@/types/websocket";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/plugin-log", () => ({
  trace: vi.fn(),
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}));

const request: SendRequestInput = {
  method: "GET",
  url: "https://example.com/",
  headers: [],
  queryParams: [],
  body: { kind: "none" },
  auth: AUTH_NONE,
  settings: DEFAULT_SETTINGS,
};

beforeEach(() => {
  invoke.mockReset();
});

describe("listCollections", () => {
  it("calls the command with no arguments", async () => {
    invoke.mockResolvedValue([{ id: "col_1", name: "Work" }]);

    const result = await collections.listCollections();

    expect(invoke).toHaveBeenCalledWith("list_collections", undefined);
    expect(result).toEqual({ ok: true, value: [{ id: "col_1", name: "Work" }] });
  });
});

describe("collectionContents", () => {
  it("passes the collection id as collectionId", async () => {
    // Mirrors what collection_contents actually returns; the mock is typed
    // as any, so a stale shape here would never fail the type check.
    invoke.mockResolvedValue({ folders: [], requests: [], examples: [], webSockets: [] });

    await collections.collectionContents("col_1");

    expect(invoke).toHaveBeenCalledWith("collection_contents", { collectionId: "col_1" });
  });
});

describe("createFolder", () => {
  it("passes collectionId, parentFolderId and name under those exact keys", async () => {
    invoke.mockResolvedValue({
      id: "fld_1",
      collectionId: "col_1",
      parentFolderId: null,
      name: "Auth",
    });

    await collections.createFolder("col_1", null, "Auth");

    expect(invoke).toHaveBeenCalledWith("create_folder", {
      collectionId: "col_1",
      parentFolderId: null,
      name: "Auth",
    });
  });
});

describe("saveRequest", () => {
  it("passes every field the command expects, including a null id for a new request", async () => {
    invoke.mockResolvedValue({
      id: "req_1",
      collectionId: "col_1",
      folderId: null,
      name: "List users",
      request,
    });

    await collections.saveRequest({
      id: null,
      collectionId: "col_1",
      folderId: null,
      name: "List users",
      request,
    });

    expect(invoke).toHaveBeenCalledWith("save_request", {
      id: null,
      collectionId: "col_1",
      folderId: null,
      name: "List users",
      request,
    });
  });
});

const webSocketRequest: WebSocketRequest = {
  url: "wss://echo.websocket.org",
  headers: [{ name: "Sec-WebSocket-Protocol", value: "chat" }],
  settings: DEFAULT_WS_SETTINGS,
};

const draft: WsDraft = { format: "json", binaryEncoding: "base64", text: '{"hello":1}' };

describe("saveWebSocket", () => {
  // The command takes one grouped `input` argument, unlike save_request's
  // loose ones. Getting the wrapper key wrong fails only at runtime.
  it("sends every field grouped under `input`, including a null id for a new one", async () => {
    const args = {
      id: null,
      collectionId: "col_1",
      folderId: "fld_1",
      name: "Echo",
      request: webSocketRequest,
      draft,
    };
    invoke.mockResolvedValue({ ...args, id: "req_1" });

    const result = await collections.saveWebSocket(args);

    expect(invoke).toHaveBeenCalledWith("save_web_socket", { input: args });
    expect(result).toEqual({ ok: true, value: { ...args, id: "req_1" } });
  });

  it("returns a typed failure when the id belongs to an HTTP request", async () => {
    invoke.mockRejectedValue({
      kind: "invalidRequest",
      message: "request req_1 is an HTTP request and cannot be saved as a WebSocket request",
    });

    const result = await collections.saveWebSocket({
      id: "req_1",
      collectionId: "col_1",
      folderId: null,
      name: "Echo",
      request: webSocketRequest,
      draft,
    });

    expect(result.ok).toBe(false);
  });
});

describe("loadWebSocket", () => {
  it("passes the id", async () => {
    invoke.mockResolvedValue({
      id: "req_1",
      collectionId: "col_1",
      folderId: null,
      name: "Echo",
      request: webSocketRequest,
      draft,
    });

    await collections.loadWebSocket("req_1");

    expect(invoke).toHaveBeenCalledWith("load_web_socket", { id: "req_1" });
  });
});

describe("moveRequest", () => {
  it("passes id and folderId", async () => {
    invoke.mockResolvedValue(undefined);

    await collections.moveRequest("req_1", "fld_2");

    expect(invoke).toHaveBeenCalledWith("move_request", { id: "req_1", folderId: "fld_2" });
  });
});

describe("error mapping", () => {
  it("returns a typed failure instead of throwing when a command rejects", async () => {
    invoke.mockRejectedValue({ kind: "notFound", message: "collection col_1 does not exist" });

    const result = await collections.deleteCollection("col_1");

    expect(result).toEqual({
      ok: false,
      error: { kind: "notFound", message: "collection col_1 does not exist" },
    });
  });
});

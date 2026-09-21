// http_client/src/services/collections.test.ts
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as collections from "@/services/collections";
import { AUTH_NONE, DEFAULT_SETTINGS, type SendRequestInput } from "@/types/http";

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
    invoke.mockResolvedValue({ folders: [], requests: [], examples: [] });

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

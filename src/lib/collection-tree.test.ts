// http_client/src/lib/collection-tree.test.ts
import { describe, expect, it } from "vitest";

import { buildFolderTree, flattenFolders } from "@/lib/collection-tree";
import type { CollectionContents, Folder, SavedRequest, SavedWebSocket } from "@/types/collections";
import { AUTH_NONE, DEFAULT_SETTINGS } from "@/types/http";
import { DEFAULT_WS_SETTINGS, EMPTY_WS_DRAFT } from "@/types/websocket";

function folder(id: string, name: string, parentFolderId: string | null = null): Folder {
  return { id, collectionId: "col_1", parentFolderId, name };
}

function request(id: string, name: string, folderId: string | null): SavedRequest {
  return {
    id,
    collectionId: "col_1",
    folderId,
    name,
    request: {
      method: "GET",
      url: "https://example.com/",
      headers: [],
      queryParams: [],
      body: { kind: "none" },
      auth: AUTH_NONE,
      settings: DEFAULT_SETTINGS,
    },
    secretState: "ok",
  };
}

function webSocket(id: string, name: string, folderId: string | null): SavedWebSocket {
  return {
    id,
    collectionId: "col_1",
    folderId,
    name,
    request: { url: "wss://example.com/", headers: [], settings: DEFAULT_WS_SETTINGS },
    draft: EMPTY_WS_DRAFT,
  };
}

describe("buildFolderTree", () => {
  it("nests folders under their parent and sorts case-insensitively", () => {
    const contents: CollectionContents = {
      folders: [
        folder("fld_b", "beta"),
        folder("fld_a", "Alpha"),
        folder("fld_c", "child", "fld_a"),
      ],
      requests: [],
      examples: [],
      webSockets: [],
    };

    const tree = buildFolderTree(contents);

    expect(tree.rootFolders.map((node) => node.folder.name)).toEqual(["Alpha", "beta"]);
    expect(tree.rootFolders[0]?.children.map((node) => node.folder.name)).toEqual(["child"]);
  });

  it("puts requests under their folder, or at the collection root with no folder", () => {
    const contents: CollectionContents = {
      folders: [folder("fld_a", "Alpha")],
      requests: [request("req_1", "In folder", "fld_a"), request("req_2", "At root", null)],
      examples: [],
      webSockets: [],
    };

    const tree = buildFolderTree(contents);

    expect(tree.rootRequests.map((r) => r.name)).toEqual(["At root"]);
    expect(tree.rootFolders[0]?.requests.map((r) => r.name)).toEqual(["In folder"]);
  });

  it("groups WebSocket requests by folder beside the HTTP ones, sorted by name", () => {
    const contents: CollectionContents = {
      folders: [folder("fld_a", "Alpha")],
      requests: [request("req_1", "HTTP", "fld_a")],
      examples: [],
      webSockets: [
        webSocket("req_3", "zeta", "fld_a"),
        webSocket("req_2", "Alpha feed", "fld_a"),
        webSocket("req_4", "At root", null),
        webSocket("req_5", "Orphan", "fld_missing"),
      ],
    };

    const tree = buildFolderTree(contents);

    expect(tree.rootWebSockets.map((w) => w.name)).toEqual(["At root", "Orphan"]);
    expect(tree.rootFolders[0]?.webSockets.map((w) => w.name)).toEqual(["Alpha feed", "zeta"]);
    expect(tree.rootFolders[0]?.requests.map((r) => r.name)).toEqual(["HTTP"]);
  });

  it("falls a request with an unresolvable folder id back to the root, rather than dropping it", () => {
    const contents: CollectionContents = {
      folders: [],
      requests: [request("req_1", "Orphan", "fld_missing")],
      examples: [],
      webSockets: [],
    };

    const tree = buildFolderTree(contents);

    expect(tree.rootRequests.map((r) => r.name)).toEqual(["Orphan"]);
  });
});

describe("flattenFolders", () => {
  it("lists nested folders depth-first, each with its depth", () => {
    const contents: CollectionContents = {
      folders: [
        folder("fld_a", "Alpha"),
        folder("fld_b", "Beta", "fld_a"),
        folder("fld_c", "Gamma"),
      ],
      requests: [],
      examples: [],
      webSockets: [],
    };

    const flat = flattenFolders(buildFolderTree(contents).rootFolders);

    expect(flat.map((entry) => [entry.folder.name, entry.depth])).toEqual([
      ["Alpha", 0],
      ["Beta", 1],
      ["Gamma", 0],
    ]);
  });
});

describe("examplesByRequestId", () => {
  it("groups examples under their request, keeping the order they arrived in", () => {
    const contents: CollectionContents = {
      folders: [],
      requests: [request("req_1", "List users", null), request("req_2", "Create user", null)],
      examples: [
        { id: "exa_1", requestId: "req_1", name: "Success", status: 200 },
        { id: "exa_2", requestId: "req_2", name: "Validation error", status: 422 },
        { id: "exa_3", requestId: "req_1", name: "Not found", status: 404 },
      ],
      webSockets: [],
    };

    const { examplesByRequestId } = buildFolderTree(contents);

    // Creation order, not alphabetical: examples read as a sequence of cases,
    // and renaming one must not reshuffle them.
    expect(examplesByRequestId.get("req_1")?.map((e) => e.name)).toEqual(["Success", "Not found"]);
    expect(examplesByRequestId.get("req_2")?.map((e) => e.name)).toEqual(["Validation error"]);
  });

  it("has no entry for a request with no examples, so the node stays a leaf", () => {
    const contents: CollectionContents = {
      folders: [],
      requests: [request("req_1", "List users", null)],
      examples: [],
      webSockets: [],
    };

    expect(buildFolderTree(contents).examplesByRequestId.has("req_1")).toBe(false);
  });
});

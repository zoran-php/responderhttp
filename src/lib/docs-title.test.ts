// http_client/src/lib/docs-title.test.ts
import { describe, expect, it } from "vitest";

import { docsTabLabel, docsTargetName, UNNAMED_DOCS_ITEM } from "@/lib/docs-title";
import type { Collection, CollectionContents } from "@/types/collections";
import { AUTH_NONE, DEFAULT_SETTINGS } from "@/types/http";
import { DEFAULT_WS_SETTINGS, EMPTY_WS_DRAFT } from "@/types/websocket";

const collections: Collection[] = [
  { id: "col_1", name: "Auth Service" },
  { id: "col_2", name: "Billing" },
];

function contents(overrides: Partial<CollectionContents> = {}): CollectionContents {
  return { folders: [], requests: [], examples: [], webSockets: [], ...overrides };
}

const contentsById: Record<string, CollectionContents> = {
  col_1: contents({
    folders: [{ id: "fld_1", collectionId: "col_1", parentFolderId: null, name: "Users" }],
  }),
  col_2: contents({
    requests: [
      {
        id: "req_1",
        collectionId: "col_2",
        folderId: null,
        name: "GET /invoices",
        request: {
          method: "GET",
          url: "https://example.com/invoices",
          headers: [],
          queryParams: [],
          body: { kind: "none" },
          auth: AUTH_NONE,
          settings: DEFAULT_SETTINGS,
        },
        secretState: "ok",
      },
    ],
  }),
};

describe("docsTargetName", () => {
  it("names a collection", () => {
    expect(docsTargetName({ kind: "collection", id: "col_1" }, collections, contentsById)).toBe(
      "Auth Service",
    );
  });

  /** A DocsTarget carries no collection id, so folders and requests are found
   * by searching every loaded collection. */
  it("finds a folder and a request in whichever collection holds them", () => {
    expect(docsTargetName({ kind: "folder", id: "fld_1" }, collections, contentsById)).toBe(
      "Users",
    );
    expect(docsTargetName({ kind: "request", id: "req_1" }, collections, contentsById)).toBe(
      "GET /invoices",
    );
  });

  /** A folder and a request could share an id only by accident, but the kind
   * decides which list is searched either way. */
  it("does not find a request when asked for a folder", () => {
    expect(docsTargetName({ kind: "folder", id: "req_1" }, collections, contentsById)).toBeNull();
  });

  /** Without this a WebSocket's Docs tab is titled "Docs: Untitled". */
  it("finds a WebSocket request as a request", () => {
    const withSocket = {
      col_3: contents({
        webSockets: [
          {
            id: "req_ws",
            collectionId: "col_3",
            folderId: null,
            name: "Live prices",
            request: { url: "wss://a.test", headers: [], settings: DEFAULT_WS_SETTINGS },
            draft: EMPTY_WS_DRAFT,
          },
        ],
      }),
    };

    expect(docsTargetName({ kind: "request", id: "req_ws" }, collections, withSocket)).toBe(
      "Live prices",
    );
  });

  /** True while a collection's contents are still loading, and after the item
   * has been deleted from under an open tab. */
  it("returns null when the item is not loaded", () => {
    expect(docsTargetName({ kind: "request", id: "req_gone" }, collections, {})).toBeNull();
  });
});

describe("docsTabLabel", () => {
  it("reads as the spec writes it", () => {
    expect(docsTabLabel("Auth Service")).toBe("Docs: Auth Service");
    expect(docsTabLabel("GET /users")).toBe("Docs: GET /users");
  });

  it("falls back rather than showing an empty label", () => {
    expect(docsTabLabel(null)).toBe(`Docs: ${UNNAMED_DOCS_ITEM}`);
  });
});

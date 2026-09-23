// http_client/src/features/collections/ExportOpenApiDialog.test.tsx
//
// The export dialog says, before exporting, how many WebSocket requests the
// OpenAPI document will leave out (PLAN-WEBSOCKET.md 13g), and says nothing
// when there are none.
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ExportOpenApiDialog } from "@/features/collections/ExportOpenApiDialog";
import { useCollectionsStore } from "@/store/collections-store";
import type { CollectionContents, SavedWebSocket } from "@/types/collections";
import { DEFAULT_WS_SETTINGS, EMPTY_WS_DRAFT } from "@/types/websocket";

const collections = vi.hoisted(() => ({ collectionContents: vi.fn() }));
vi.mock("@/services/collections", () => collections);
vi.mock("@/services/openapi", () => ({ exportCollectionOpenApi: vi.fn() }));

function webSocket(id: string): SavedWebSocket {
  return {
    id,
    collectionId: "col_1",
    folderId: null,
    name: id,
    request: { url: "wss://a.test", headers: [], settings: DEFAULT_WS_SETTINGS },
    draft: EMPTY_WS_DRAFT,
  };
}

function contents(webSockets: SavedWebSocket[]): CollectionContents {
  return { folders: [], requests: [], examples: [], webSockets };
}

beforeEach(() => {
  useCollectionsStore.setState({ contentsById: {} });
});

afterEach(() => {
  cleanup();
});

describe("ExportOpenApiDialog", () => {
  it("counts the WebSocket requests it will leave out, once the contents load", async () => {
    collections.collectionContents.mockResolvedValue({
      ok: true,
      value: contents([webSocket("req_1"), webSocket("req_2")]),
    });

    render(<ExportOpenApiDialog collectionId="col_1" collectionName="Mixed" onClose={vi.fn()} />);

    const note = await screen.findByRole("note");
    expect(note.textContent).toMatch(/^2 WebSocket requests will not be exported/);
    expect(collections.collectionContents).toHaveBeenCalledWith("col_1");
  });

  it("says nothing about WebSocket requests when there are none", async () => {
    collections.collectionContents.mockResolvedValue({ ok: true, value: contents([]) });

    render(
      <ExportOpenApiDialog collectionId="col_1" collectionName="HTTP only" onClose={vi.fn()} />,
    );

    await screen.findByRole("button", { name: "Export" });
    expect(screen.queryByRole("note")).toBeNull();
  });
});

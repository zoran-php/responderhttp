// http_client/src/features/collections/CollectionsSidebar.test.tsx
//
// gRPC requests in the sidebar (PLAN-GRPC.md 16i-2): listed in the tree
// with their own icon, opened by a fresh load, created from a collection's
// menu, and renamed through the request path like every other row. The
// collections service is mocked; the store is real.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { CollectionsSidebar } from "@/features/collections/CollectionsSidebar";
import { emptyGrpcDraft } from "@/lib/grpc-request";
import { useCollectionsStore } from "@/store/collections-store";
import type { CollectionContents } from "@/types/collections";
import type { SavedGrpcRequest } from "@/types/grpc";

const service = vi.hoisted(() => ({
  listCollections: vi.fn(),
  collectionContents: vi.fn(),
  loadGrpcRequest: vi.fn(),
  saveGrpcRequest: vi.fn(),
  renameRequest: vi.fn(),
}));
vi.mock("@/services/collections", () => service);

const saved: SavedGrpcRequest = {
  id: "req_g",
  collectionId: "col_1",
  folderId: null,
  name: "Get order",
  request: { ...emptyGrpcDraft(), url: "localhost:50051" },
  secretState: "ok",
};

const contents: CollectionContents = {
  folders: [],
  requests: [],
  examples: [],
  webSockets: [],
  grpcRequests: [saved],
};

function renderSidebar(onOpenGrpc = vi.fn()) {
  render(
    <CollectionsSidebar
      loadedRequestId={null}
      onOpenDocs={vi.fn()}
      onOpenExample={vi.fn()}
      onOpenGrpc={onOpenGrpc}
      onOpenRequest={vi.fn()}
      onOpenWebSocket={vi.fn()}
    />,
  );
  return onOpenGrpc;
}

beforeEach(() => {
  for (const mock of Object.values(service)) {
    mock.mockReset();
  }
  service.listCollections.mockResolvedValue({
    ok: true,
    value: [{ id: "col_1", name: "Shop" }],
  });
  service.collectionContents.mockResolvedValue({ ok: true, value: contents });
  useCollectionsStore.setState({
    collections: [],
    contentsById: {},
    expandedCollectionIds: new Set(),
    expandedFolderIds: new Set(),
    error: null,
  });
});

afterEach(() => {
  cleanup();
});

async function expandShop() {
  fireEvent.click(await screen.findByRole("button", { name: "Expand Shop" }));
  await screen.findByText("Get order");
}

describe("gRPC requests in the sidebar", () => {
  it("lists a saved gRPC request with its icon and opens a fresh copy", async () => {
    service.loadGrpcRequest.mockResolvedValue({ ok: true, value: saved });
    const onOpenGrpc = renderSidebar();
    await expandShop();

    expect(screen.getByLabelText("gRPC")).toBeTruthy();
    fireEvent.click(screen.getByText("Get order"));

    await waitFor(() => expect(onOpenGrpc).toHaveBeenCalledWith(saved));
    expect(service.loadGrpcRequest).toHaveBeenCalledWith("req_g");
  });

  it("creates an empty gRPC request from the collection's menu and opens it", async () => {
    const created: SavedGrpcRequest = { ...saved, id: "req_new", name: "Chat" };
    service.saveGrpcRequest.mockResolvedValue({ ok: true, value: created });
    const onOpenGrpc = renderSidebar();
    await expandShop();

    fireEvent.contextMenu(screen.getByText("Shop"));
    fireEvent.click(screen.getByRole("button", { name: "New gRPC request" }));
    const input = screen.getByPlaceholderText("gRPC request name");
    fireEvent.change(input, { target: { value: "Chat" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(onOpenGrpc).toHaveBeenCalledWith(created));
    expect(service.saveGrpcRequest).toHaveBeenCalledWith({
      id: null,
      collectionId: "col_1",
      folderId: null,
      name: "Chat",
      request: emptyGrpcDraft(),
    });
  });

  it("renames a gRPC request through the request path", async () => {
    service.renameRequest.mockResolvedValue({ ok: true, value: undefined });
    renderSidebar();
    await expandShop();

    fireEvent.contextMenu(screen.getByText("Get order"));
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    const input = screen.getByDisplayValue("Get order");
    fireEvent.change(input, { target: { value: "Get one order" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(service.renameRequest).toHaveBeenCalled());
    expect(service.renameRequest.mock.calls[0]?.slice(0, 2)).toEqual(["req_g", "Get one order"]);
  });
});

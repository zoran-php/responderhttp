// http_client/src/features/collections/SaveRequestDialog.test.tsx
//
// A WebSocket request saves into a collection that already holds HTTP
// requests, into any folder, with no warning: nothing on the save path looks
// at protocol (PLAN-WEBSOCKET.md spec section 3).
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SaveRequestDialog } from "@/features/collections/SaveRequestDialog";
import { emptyWebSocketShape } from "@/lib/ws-request";
import type { CollectionContents } from "@/types/collections";
import { AUTH_NONE, DEFAULT_SETTINGS } from "@/types/http";

const service = vi.hoisted(() => ({
  listCollections: vi.fn(),
  collectionContents: vi.fn(),
  saveRequest: vi.fn(),
  saveWebSocket: vi.fn(),
}));
vi.mock("@/services/collections", () => service);

const mixed: CollectionContents = {
  folders: [{ id: "fld_1", collectionId: "col_1", parentFolderId: null, name: "Users" }],
  requests: [
    {
      id: "req_1",
      collectionId: "col_1",
      folderId: null,
      name: "GET /users",
      request: {
        method: "GET",
        url: "https://example.com/users",
        headers: [],
        queryParams: [],
        body: { kind: "none" },
        auth: AUTH_NONE,
        settings: DEFAULT_SETTINGS,
      },
      secretState: "ok",
    },
  ],
  examples: [],
  webSockets: [],
};

beforeEach(() => {
  service.listCollections.mockReset().mockResolvedValue({
    ok: true,
    value: [{ id: "col_1", name: "Mixed" }],
  });
  service.collectionContents.mockReset().mockResolvedValue({ ok: true, value: mixed });
  service.saveRequest.mockReset();
  service.saveWebSocket.mockReset().mockImplementation((args: { folderId: string | null }) =>
    Promise.resolve({
      ok: true,
      value: { id: "req_ws", collectionId: "col_1", folderId: args.folderId, name: "Live" },
    }),
  );
});

afterEach(() => {
  cleanup();
});

describe("SaveRequestDialog with a WebSocket request", () => {
  it("saves into a folder of a collection that holds HTTP requests, without a warning", async () => {
    const onSaved = vi.fn();
    const shape = emptyWebSocketShape();
    render(
      <SaveRequestDialog
        defaultName="Live"
        onClose={() => undefined}
        onSaved={onSaved}
        payload={{ kind: "websocket", shape }}
      />,
    );

    const collection = await screen.findByRole("option", { name: "Mixed" });
    fireEvent.change(collection.closest("select") as HTMLSelectElement, {
      target: { value: "col_1" },
    });
    const folder = await screen.findByRole("option", { name: "Users" });
    fireEvent.change(folder.closest("select") as HTMLSelectElement, {
      target: { value: "fld_1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1));
    expect(service.saveWebSocket).toHaveBeenCalledWith({
      id: null,
      collectionId: "col_1",
      folderId: "fld_1",
      name: "Live",
      request: shape.request,
      draft: shape.draft,
    });
    expect(service.saveRequest).not.toHaveBeenCalled();
    expect(screen.queryByText(/Could not/)).toBeNull();
  });
});

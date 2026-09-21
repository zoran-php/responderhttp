// http_client/src/features/collections/ImportOpenApiDialog.test.tsx
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ImportOpenApiDialog } from "@/features/collections/ImportOpenApiDialog";
import type { OpenApiImportPreview } from "@/types/openapi-import";

const service = vi.hoisted(() => ({
  pickOpenApiImport: vi.fn(),
  previewOpenApiImport: vi.fn(),
  importOpenApi: vi.fn(),
  discardOpenApiImport: vi.fn(),
}));
vi.mock("@/services/openapi-import", () => service);

const preview: OpenApiImportPreview = {
  token: "imp_1",
  fileName: "pets.yaml",
  title: "Pet Store",
  version: "3.1",
  format: "YAML",
  requestCount: 3,
  exampleCount: 2,
  defaultGrouping: "tags",
  groupings: [
    { grouping: "tags", folderCount: 2, topLevelCount: 2, topLevel: ["pets", "store"] },
    { grouping: "paths", folderCount: 1, topLevelCount: 1, topLevel: ["api/v3"] },
    { grouping: "flat", folderCount: 0, topLevelCount: 0, topLevel: [] },
  ],
  environment: {
    name: "Pet Store",
    variables: [
      { name: "baseUrl", secret: false },
      { name: "token", secret: true },
    ],
  },
  notes: ['"petstore_auth" (oauth2) is not an auth type this app has.'],
};

beforeEach(() => {
  for (const mock of Object.values(service)) {
    mock.mockReset();
  }
  service.discardOpenApiImport.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
});

function renderDialog() {
  const onClose = vi.fn();
  const onImported = vi.fn();
  render(<ImportOpenApiDialog onClose={onClose} onImported={onImported} />);
  return { onClose, onImported };
}

describe("ImportOpenApiDialog", () => {
  it("opens the chooser at once and closes quietly when it is dismissed", async () => {
    service.pickOpenApiImport.mockResolvedValue({ ok: true, value: { kind: "cancelled" } });

    const { onClose } = renderDialog();

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(service.pickOpenApiImport).toHaveBeenCalledTimes(1);
  });

  it("shows why a file was refused, with its details", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: {
        kind: "refused",
        fileName: "bad.json",
        title: "The document is not valid OpenAPI",
        reason: "It does not match the official OpenAPI 3.1 schema.",
        details: ["info: missing properties 'title'"],
        totalDetails: 4,
      },
    });

    renderDialog();

    expect(await screen.findByText("The document is not valid OpenAPI")).toBeTruthy();
    expect(screen.getByText("info: missing properties 'title'")).toBeTruthy();
    expect(screen.getByText("…and 3 more.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Import/ })).toBeNull();
  });

  it("previews the collection, folders, environment and notes", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview },
    });

    renderDialog();

    expect(await screen.findByText("Pet Store")).toBeTruthy();
    expect(screen.getByText("2 folders: pets, store")).toBeTruthy();
    expect(screen.getByText(/2 variables - baseUrl, 1 secret \(token\)/)).toBeTruthy();
    expect(screen.getByText(/petstore_auth/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import 3 requests" })).toBeTruthy();
  });

  it("keeps the buttons and errors outside the scrolling body", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview },
    });
    service.importOpenApi.mockResolvedValue({
      ok: false,
      error: { kind: "storage", message: "disk full" },
    });

    renderDialog();
    const button = await screen.findByRole("button", { name: "Import 3 requests" });
    fireEvent.click(button);
    const error = await screen.findByText("disk full");

    for (const element of [button, error]) {
      expect(element.closest("[data-modal-footer]")).not.toBeNull();
      expect(element.closest("[data-modal-body]")).toBeNull();
    }
  });

  it("lists a small environment's variables straight away", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview },
    });

    renderDialog();

    expect(await screen.findByText("token")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show all" })).toBeNull();
  });

  it("hides a large environment's variables behind Show all, in a four-row box", async () => {
    const variables = [
      { name: "baseUrl", secret: false },
      { name: "token", secret: true },
      ...Array.from({ length: 150 }, (_, index) => ({ name: `param_${index}`, secret: false })),
    ];
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview, environment: { name: "GitHub", variables } },
    });

    renderDialog();

    expect(
      await screen.findByText(/152 variables - baseUrl, 1 secret \(token\), 150 others/),
    ).toBeTruthy();
    expect(screen.queryByTestId("environment-variables")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Show all" }));
    const list = screen.getByTestId("environment-variables");
    expect(list.className).toContain("max-h-24");
    expect(list.className).toContain("overflow-y-auto");
    expect(screen.getByText("param_149")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Hide" }));
    expect(screen.queryByTestId("environment-variables")).toBeNull();
  });

  it("asks Rust for a new preview when an option changes", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview },
    });
    service.previewOpenApiImport.mockResolvedValue({
      ok: true,
      value: { ...preview, environment: null },
    });

    renderDialog();
    const select = await screen.findByRole("combobox");
    await act(async () => {
      fireEvent.change(select, { target: { value: "paths" } });
    });

    expect(service.previewOpenApiImport).toHaveBeenCalledWith("imp_1", {
      grouping: "paths",
      includeExamples: true,
      createEnvironment: true,
    });
    expect(await screen.findByText("1 folder: api/v3")).toBeTruthy();
  });

  it("imports with the chosen options and reports the result", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview },
    });
    const result = {
      collectionId: "col_9",
      environmentId: "env_9",
      collectionName: "Pet Store",
      requestCount: 3,
      notes: [],
    };
    service.importOpenApi.mockResolvedValue({ ok: true, value: result });

    const { onImported } = renderDialog();
    fireEvent.click(await screen.findByRole("button", { name: "Import 3 requests" }));

    await waitFor(() => expect(onImported).toHaveBeenCalledWith(result));
    expect(service.importOpenApi).toHaveBeenCalledWith("imp_1", {
      grouping: "tags",
      includeExamples: true,
      createEnvironment: true,
    });
    expect(screen.getByText(/Imported 3 requests into/)).toBeTruthy();
    expect(service.discardOpenApiImport).not.toHaveBeenCalled();
  });

  it("keeps the preview open after a failed write so it can be retried", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview },
    });
    service.importOpenApi.mockResolvedValue({
      ok: false,
      error: { kind: "secretStore", message: "The credential store is locked." },
    });

    const { onImported } = renderDialog();
    fireEvent.click(await screen.findByRole("button", { name: "Import 3 requests" }));

    expect(await screen.findByText("The credential store is locked.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import 3 requests" })).toBeTruthy();
    expect(onImported).not.toHaveBeenCalled();
  });

  it("releases the held document when cancelled", async () => {
    service.pickOpenApiImport.mockResolvedValue({
      ok: true,
      value: { kind: "ready", ...preview },
    });

    const { onClose } = renderDialog();
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(service.discardOpenApiImport).toHaveBeenCalledWith("imp_1");
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

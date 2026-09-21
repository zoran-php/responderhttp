// http_client/src/services/openapi.test.ts
import { beforeEach, describe, expect, it, vi } from "vitest";

import { exportCollectionOpenApi } from "@/services/openapi";
import { DEFAULT_EXPORT_FORMAT, EXPORT_FORMATS } from "@/types/openapi";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/plugin-log", () => ({
  trace: vi.fn(),
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}));

beforeEach(() => {
  invoke.mockReset();
});

describe("exportCollectionOpenApi", () => {
  it("sends the format under the argument name the command expects", async () => {
    invoke.mockResolvedValue({ savedTo: "C:\\out\\work_api.openapi-3.2.yaml", notes: [] });

    const result = await exportCollectionOpenApi("col_1", "3.2", "yaml", false);

    expect(invoke).toHaveBeenCalledWith("export_collection_openapi", {
      collectionId: "col_1",
      version: "3.2",
      format: "yaml",
      includeExamples: false,
    });
    expect(result).toEqual({
      ok: true,
      value: { savedTo: "C:\\out\\work_api.openapi-3.2.yaml", notes: [] },
    });
  });

  it("surfaces a format the backend refuses as a typed failure", async () => {
    invoke.mockRejectedValue({
      kind: "invalidRequest",
      message: "unsupported export format toml",
    });

    const result = await exportCollectionOpenApi("col_1", "3.2", "json", false);

    expect(result).toEqual({
      ok: false,
      error: { kind: "invalidRequest", message: "unsupported export format toml" },
    });
  });
});

describe("export formats", () => {
  it("defaults to JSON and lists it first", () => {
    expect(DEFAULT_EXPORT_FORMAT).toBe("json");
    expect(EXPORT_FORMATS[0]).toBe("json");
    expect(EXPORT_FORMATS).toEqual(["json", "yaml"]);
  });
});

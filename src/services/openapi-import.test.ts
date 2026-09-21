// http_client/src/services/openapi-import.test.ts
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  discardOpenApiImport,
  importOpenApi,
  pickOpenApiImport,
  previewOpenApiImport,
} from "@/services/openapi-import";
import {
  IMPORT_GROUPING_LABELS,
  IMPORT_GROUPINGS,
  type OpenApiImportOptions,
} from "@/types/openapi-import";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/plugin-log", () => ({
  trace: vi.fn(),
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}));

const options: OpenApiImportOptions = {
  grouping: "paths",
  includeExamples: false,
  createEnvironment: true,
};

beforeEach(() => {
  invoke.mockReset();
});

describe("pickOpenApiImport", () => {
  it("takes no arguments and passes a refusal through as a value", async () => {
    const refusal = {
      kind: "refused",
      fileName: "notes.toml",
      title: "This file is not JSON or YAML",
      reason: "Only OpenAPI documents written in JSON or YAML can be imported.",
      details: [],
      totalDetails: 0,
    };
    invoke.mockResolvedValue(refusal);

    const result = await pickOpenApiImport();

    expect(invoke).toHaveBeenCalledWith("pick_openapi_import", undefined);
    expect(result).toEqual({ ok: true, value: refusal });
  });

  it("reports a dismissed chooser as cancelled, not as a failure", async () => {
    invoke.mockResolvedValue({ kind: "cancelled" });

    expect(await pickOpenApiImport()).toEqual({ ok: true, value: { kind: "cancelled" } });
  });
});

describe("previewOpenApiImport and importOpenApi", () => {
  it("send the token and options under the names the commands expect", async () => {
    invoke.mockResolvedValue({});

    await previewOpenApiImport("imp_1", options);
    await importOpenApi("imp_1", options);

    expect(invoke).toHaveBeenNthCalledWith(1, "preview_openapi_import", {
      token: "imp_1",
      options: { grouping: "paths", includeExamples: false, createEnvironment: true },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "import_openapi", {
      token: "imp_1",
      options: { grouping: "paths", includeExamples: false, createEnvironment: true },
    });
  });

  it("surfaces a spent token as a typed failure", async () => {
    invoke.mockRejectedValue({
      kind: "notFound",
      message: "that import is no longer open; choose the file again",
    });

    const result = await importOpenApi("imp_old", options);

    expect(result).toEqual({
      ok: false,
      error: {
        kind: "notFound",
        message: "that import is no longer open; choose the file again",
      },
    });
  });
});

describe("discardOpenApiImport", () => {
  it("resolves even when the command fails, which call() has already logged", async () => {
    invoke.mockRejectedValue({ kind: "internal", message: "gone" });

    await expect(discardOpenApiImport("imp_1")).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("discard_openapi_import", { token: "imp_1" });
  });
});

describe("import groupings", () => {
  it("lists every grouping once, each with a label", () => {
    expect(IMPORT_GROUPINGS).toEqual(["tags", "paths", "flat"]);
    for (const grouping of IMPORT_GROUPINGS) {
      expect(IMPORT_GROUPING_LABELS[grouping]).not.toBe("");
    }
  });
});

// http_client/src/lib/openapi-import.test.ts
import { describe, expect, it } from "vitest";

import {
  countLabel,
  defaultImportOptions,
  environmentSummary,
  folderSummary,
  groupingPreview,
  MAX_LISTED_FOLDERS,
  MAX_NAMED_SECRETS,
  refusalReport,
} from "@/lib/openapi-import";
import type { OpenApiImportPreview } from "@/types/openapi-import";

const preview: OpenApiImportPreview = {
  token: "imp_1",
  fileName: "pets.yaml",
  title: "Pets",
  version: "3.1",
  format: "YAML",
  requestCount: 3,
  exampleCount: 1,
  defaultGrouping: "paths",
  groupings: [
    { grouping: "tags", folderCount: 0, topLevelCount: 0, topLevel: [] },
    { grouping: "paths", folderCount: 3, topLevelCount: 2, topLevel: ["pets", "store"] },
    { grouping: "flat", folderCount: 0, topLevelCount: 0, topLevel: [] },
  ],
  environment: null,
  notes: [],
};

describe("defaultImportOptions", () => {
  it("opens with the grouping Rust suggests and everything else on", () => {
    expect(defaultImportOptions(preview)).toEqual({
      grouping: "paths",
      includeExamples: true,
      createEnvironment: true,
    });
  });
});

describe("folderSummary", () => {
  it("says when there are no folders", () => {
    expect(folderSummary(groupingPreview(preview, "flat"))).toBe(
      "No folders - every request goes straight into the collection.",
    );
    expect(folderSummary(undefined)).toContain("No folders");
  });

  it("lists top-level names and counts nested ones", () => {
    expect(folderSummary(groupingPreview(preview, "paths"))).toBe(
      "3 folders: pets, store, and 1 nested",
    );
  });

  it("caps the list", () => {
    const names = Array.from({ length: 20 }, (_, index) => `t${index}`);

    const summary = folderSummary({
      grouping: "tags",
      folderCount: 20,
      topLevelCount: 20,
      topLevel: names,
    });

    expect(summary).toBe(
      `20 folders: ${names.slice(0, MAX_LISTED_FOLDERS).join(", ")}, and ${20 - MAX_LISTED_FOLDERS} more`,
    );
  });

  it("counts names the backend did not send as more, not as nested", () => {
    const summary = folderSummary({
      grouping: "paths",
      folderCount: 150,
      topLevelCount: 140,
      topLevel: Array.from({ length: 100 }, (_, index) => `p${index}`),
    });

    expect(summary.endsWith(`, and ${140 - MAX_LISTED_FOLDERS} more and 10 nested`)).toBe(true);
  });

  it("uses the singular for one folder", () => {
    expect(
      folderSummary({ grouping: "tags", folderCount: 1, topLevelCount: 1, topLevel: ["only"] }),
    ).toBe("1 folder: only");
  });
});

describe("countLabel", () => {
  it("picks singular or plural", () => {
    expect(countLabel(1, "request", "requests")).toBe("1 request");
    expect(countLabel(0, "request", "requests")).toBe("0 requests");
  });
});

describe("refusalReport", () => {
  it("includes every detail and how many were cut", () => {
    const report = refusalReport({
      fileName: "bad.json",
      title: "The document is not valid OpenAPI",
      reason: "It does not match the official OpenAPI 3.1 schema.",
      details: ["info: missing properties 'title'"],
      totalDetails: 3,
    });

    expect(report).toBe(
      [
        "bad.json: The document is not valid OpenAPI",
        "It does not match the official OpenAPI 3.1 schema.",
        "",
        "- info: missing properties 'title'",
        "(2 more not shown)",
      ].join("\n"),
    );
  });

  it("is two lines for a refusal without details", () => {
    expect(
      refusalReport({
        fileName: "a.toml",
        title: "This file is not JSON or YAML",
        reason: "Only JSON or YAML.",
        details: [],
        totalDetails: 0,
      }),
    ).toBe("a.toml: This file is not JSON or YAML\nOnly JSON or YAML.");
  });
});

describe("environmentSummary", () => {
  const plain = (name: string) => ({ name, secret: false });
  const secret = (name: string) => ({ name, secret: true });

  it("names the base URL and the secrets and counts the rest", () => {
    const variables = [
      plain("baseUrl"),
      plain("owner"),
      secret("key"),
      plain("repo"),
      secret("token"),
    ];

    expect(environmentSummary({ name: "GitHub", variables })).toBe(
      "5 variables - baseUrl, 2 secret (key, token), 2 others",
    );
  });

  it("leaves out the parts that are empty", () => {
    expect(environmentSummary({ name: "t", variables: [plain("baseUrl")] })).toBe(
      "1 variable - baseUrl",
    );
    expect(environmentSummary({ name: "t", variables: [plain("base_url"), plain("id")] })).toBe(
      "2 variables - base_url, 1 other",
    );
    expect(environmentSummary({ name: "t", variables: [] })).toBe("no variables");
  });

  it("counts a leading secret as a secret, not as the base URL", () => {
    expect(environmentSummary({ name: "t", variables: [secret("token")] })).toBe(
      "1 variable - 1 secret (token)",
    );
  });

  it("caps how many secrets it names", () => {
    const secrets = Array.from({ length: MAX_NAMED_SECRETS + 2 }, (_, index) =>
      secret(`s${index}`),
    );

    const summary = environmentSummary({ name: "t", variables: [plain("baseUrl"), ...secrets] });

    expect(summary).toBe(
      `${MAX_NAMED_SECRETS + 3} variables - baseUrl, ${MAX_NAMED_SECRETS + 2} secret (s0, s1, s2, s3, s4 and 2 more)`,
    );
  });
});

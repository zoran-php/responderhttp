// http_client/src/lib/content-type.test.ts
import { describe, expect, it } from "vitest";

import { essenceOf, findHeader, languageForContentType } from "@/lib/content-type";

describe("languageForContentType", () => {
  it("recognises JSON including suffixed types", () => {
    expect(languageForContentType("application/json")).toBe("json");
    expect(languageForContentType("application/json; charset=utf-8")).toBe("json");
    expect(languageForContentType("application/problem+json")).toBe("json");
  });

  it("recognises XML and HTML", () => {
    expect(languageForContentType("application/xml")).toBe("xml");
    expect(languageForContentType("image/svg+xml")).toBe("xml");
    expect(languageForContentType("text/html")).toBe("html");
  });

  it("falls back to plain text for anything else or nothing", () => {
    expect(languageForContentType("text/csv")).toBe("plaintext");
    expect(languageForContentType(undefined)).toBe("plaintext");
  });
});

describe("essenceOf", () => {
  it("drops parameters and normalises case", () => {
    expect(essenceOf("APPLICATION/JSON; charset=utf-8")).toBe("application/json");
  });
});

describe("findHeader", () => {
  it("matches case-insensitively", () => {
    const headers = [{ name: "Content-Type", value: "application/json" }];

    expect(findHeader(headers, "content-type")).toBe("application/json");
    expect(findHeader(headers, "missing")).toBeUndefined();
  });
});

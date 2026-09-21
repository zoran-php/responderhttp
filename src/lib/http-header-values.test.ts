// http_client/src/lib/http-header-values.test.ts
import { describe, expect, it } from "vitest";

import { IANA_HEADER_NAMES } from "@/lib/http-header-names";
import { headerValueSuggestions, suggestedValueHeaderNames } from "@/lib/http-header-values";

describe("headerValueSuggestions", () => {
  /** Header names are case-insensitive (RFC 9110 §5.1) and nobody types them
   * consistently. */
  it("matches whatever casing the user typed", () => {
    const canonical = headerValueSuggestions("Accept-Encoding");

    expect(canonical).toContain("gzip");
    expect(headerValueSuggestions("accept-encoding")).toEqual(canonical);
    expect(headerValueSuggestions("ACCEPT-ENCODING")).toEqual(canonical);
  });

  it("ignores surrounding whitespace, which a pasted name often carries", () => {
    expect(headerValueSuggestions("  Connection  ")).toContain("keep-alive");
  });

  it("returns nothing for a header with no closed set of values", () => {
    expect(headerValueSuggestions("Host")).toEqual([]);
    expect(headerValueSuggestions("X-Whatever")).toEqual([]);
    expect(headerValueSuggestions("")).toEqual([]);
  });

  it("reuses the shared method list for the CORS preflight header", () => {
    expect(headerValueSuggestions("Access-Control-Request-Method")).toContain("PATCH");
  });

  it("keeps the trailing space on auth scheme prefixes so a token follows cleanly", () => {
    expect(headerValueSuggestions("Authorization")).toContain("Bearer ");
  });
});

describe("the two lists stay in step", () => {
  /** A value suggestion for a name that is never suggested would be a quiet
   * inconsistency — the user could only reach it by typing the name exactly. */
  it("suggests values only for headers that are themselves suggested", () => {
    const known = new Set(IANA_HEADER_NAMES.map((name) => name.toLowerCase()));

    const orphans = suggestedValueHeaderNames().filter((name) => !known.has(name));

    expect(orphans).toEqual([]);
  });

  it("keys the value table in lowercase, which is what the lookup assumes", () => {
    const wrongCase = suggestedValueHeaderNames().filter((name) => name !== name.toLowerCase());

    expect(wrongCase).toEqual([]);
  });

  it("never offers a blank suggestion", () => {
    const blanks = suggestedValueHeaderNames().filter((name) =>
      headerValueSuggestions(name).some((value) => value.trim() === ""),
    );

    expect(blanks).toEqual([]);
  });
});

describe("IANA_HEADER_NAMES", () => {
  it("carries the registry's own spelling, duplicate-free", () => {
    expect(IANA_HEADER_NAMES).toContain("Content-Type");
    expect(IANA_HEADER_NAMES).toContain("Sec-Fetch-Mode");
    expect(new Set(IANA_HEADER_NAMES).size).toBe(IANA_HEADER_NAMES.length);
  });

  /** Obsoleted and deprecated entries are filtered out when the list is
   * generated; typing one still works, it is just not offered. */
  it("leaves out what the registry marks obsoleted or deprecated", () => {
    expect(IANA_HEADER_NAMES).not.toContain("Warning");
    expect(IANA_HEADER_NAMES).not.toContain("Content-MD5");
    expect(IANA_HEADER_NAMES).not.toContain("Pragma");
  });
});

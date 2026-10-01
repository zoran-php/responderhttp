import { describe, expect, it } from "vitest";

import { nonHttpOmissionNotice } from "@/lib/openapi-export";

describe("nonHttpOmissionNotice", () => {
  it("says nothing when the collection has only HTTP requests", () => {
    expect(nonHttpOmissionNotice(0, 0)).toBeNull();
  });

  it("gives the WebSocket count, singular or plural", () => {
    expect(nonHttpOmissionNotice(1)).toMatch(/^1 WebSocket request will not be exported: /);
    expect(nonHttpOmissionNotice(3)).toMatch(/^3 WebSocket requests will not be exported: /);
  });

  it("gives the gRPC count, and both together", () => {
    expect(nonHttpOmissionNotice(0, 2)).toBe(
      "2 gRPC requests will not be exported: the OpenAPI specification describes HTTP requests only and has no way to describe gRPC requests.",
    );
    expect(nonHttpOmissionNotice(1, 1)).toBe(
      "1 WebSocket request and 1 gRPC request will not be exported: the OpenAPI specification describes HTTP requests only and has no way to describe them.",
    );
  });
});

import { describe, expect, it } from "vitest";

import { webSocketOmissionNotice } from "@/lib/openapi-export";

describe("webSocketOmissionNotice", () => {
  it("says nothing when the collection has no WebSocket requests", () => {
    expect(webSocketOmissionNotice(0)).toBeNull();
  });

  it("gives the count, singular or plural", () => {
    expect(webSocketOmissionNotice(1)).toMatch(/^1 WebSocket request will not be exported: /);
    expect(webSocketOmissionNotice(3)).toMatch(/^3 WebSocket requests will not be exported: /);
  });
});

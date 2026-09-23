import { describe, expect, it } from "vitest";

import { emptyWebSocketShape, webSocketShapesEqual } from "@/lib/ws-request";

describe("webSocketShapesEqual", () => {
  it("is true for two empty shapes", () => {
    expect(webSocketShapesEqual(emptyWebSocketShape(), emptyWebSocketShape())).toBe(true);
  });

  it("notices a change to any saved field", () => {
    const base = emptyWebSocketShape();
    const changes = [
      { ...base, request: { ...base.request, url: "wss://a.test" } },
      { ...base, request: { ...base.request, headers: [{ name: "X", value: "1" }] } },
      {
        ...base,
        request: { ...base.request, settings: { ...base.request.settings, autoReconnect: true } },
      },
      { ...base, draft: { ...base.draft, text: "hello" } },
      { ...base, draft: { ...base.draft, format: "json" as const } },
      { ...base, draft: { ...base.draft, binaryEncoding: "hex" as const } },
    ];

    for (const changed of changes) {
      expect(webSocketShapesEqual(base, changed)).toBe(false);
    }
  });
});

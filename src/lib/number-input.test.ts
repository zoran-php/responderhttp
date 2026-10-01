import { describe, expect, it } from "vitest";

import { clampInt } from "@/lib/number-input";

describe("clampInt", () => {
  it("keeps a number in range, clamps one outside it, and floors to min when blank", () => {
    expect(clampInt("30", 1, 300)).toBe(30);
    expect(clampInt("0", 1, 300)).toBe(1);
    expect(clampInt("9999", 1, 300)).toBe(300);
    expect(clampInt("", 1, 300)).toBe(1);
    expect(clampInt("12.9", 1, 300)).toBe(12);
  });
});

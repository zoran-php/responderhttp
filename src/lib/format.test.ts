// http_client/src/lib/format.test.ts
import { describe, expect, it } from "vitest";

import { formatBytes, formatDuration, statusClass } from "@/lib/format";

describe("formatDuration", () => {
  it("shows whole milliseconds below a second", () => {
    expect(formatDuration(0)).toBe("0 ms");
    expect(formatDuration(942.4)).toBe("942 ms");
  });

  it("switches to seconds at a second", () => {
    expect(formatDuration(1000)).toBe("1.00 s");
    expect(formatDuration(12_345)).toBe("12.35 s");
  });

  it("renders a placeholder for values that cannot be a duration", () => {
    expect(formatDuration(-1)).toBe("—");
    expect(formatDuration(Number.NaN)).toBe("—");
  });
});

describe("formatBytes", () => {
  it("keeps bytes exact and scales larger units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5 MB");
  });

  it("stops at the largest known unit", () => {
    expect(formatBytes(1024 ** 4)).toBe("1024 GB");
  });
});

describe("statusClass", () => {
  it("classifies each status range", () => {
    expect(statusClass(204)).toBe("success");
    expect(statusClass(301)).toBe("redirect");
    expect(statusClass(404)).toBe("clientError");
    expect(statusClass(503)).toBe("serverError");
    expect(statusClass(0)).toBe("unknown");
  });
});

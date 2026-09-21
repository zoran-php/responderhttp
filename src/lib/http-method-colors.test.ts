// http_client/src/lib/http-method-colors.test.ts
import { describe, expect, it } from "vitest";

import { methodTextColor } from "@/lib/http-method-colors";
import { HTTP_METHODS } from "@/types/http";

describe("methodTextColor", () => {
  it("gives every HTTP method its own color class", () => {
    const classes = HTTP_METHODS.map(methodTextColor);

    expect(new Set(classes).size).toBe(HTTP_METHODS.length);
  });

  it("names the class after the method", () => {
    expect(methodTextColor("GET")).toBe("text-method-get");
    expect(methodTextColor("DELETE")).toBe("text-method-delete");
  });
});

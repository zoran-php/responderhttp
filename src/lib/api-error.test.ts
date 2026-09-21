// http_client/src/lib/api-error.test.ts
import { describe, expect, it } from "vitest";

import { toApiError } from "@/lib/api-error";

describe("toApiError", () => {
  it("passes a well-formed ApiError through unchanged", () => {
    const error = { kind: "notFound", message: "collection col_1 does not exist" };

    expect(toApiError(error)).toEqual(error);
  });

  it("keeps a credential-store failure as its own kind", () => {
    const error = { kind: "secretStore", message: "Credential Manager refused access" };

    expect(toApiError(error)).toEqual(error);
  });

  it("maps a plain string rejection to an internal error", () => {
    expect(toApiError("command not found")).toEqual({
      kind: "internal",
      message: "command not found",
    });
  });

  it("maps anything unrecognisable to a generic internal error", () => {
    expect(toApiError(undefined)).toEqual({
      kind: "internal",
      message: "Unexpected error from the backend",
    });
  });

  it("rejects an object missing a message as not a real ApiError", () => {
    expect(toApiError({ kind: "transport" })).toEqual({
      kind: "internal",
      message: "Unexpected error from the backend",
    });
  });
});

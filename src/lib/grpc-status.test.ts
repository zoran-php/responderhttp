import { describe, expect, it } from "vitest";

import { grpcBadge, grpcPrimaryAction, methodKindLabel } from "@/lib/grpc-status";
import type { GrpcCallOutcome } from "@/types/grpc";

const outcome = (code: number, name: string): GrpcCallOutcome => ({
  status: { code, name, message: "" },
  by: "server",
  trailers: [],
  totalMs: 5,
});

describe("grpcBadge", () => {
  it("shows the status once the call has ended, OK in the ok tone and anything else as an error", () => {
    expect(grpcBadge("idle", outcome(0, "OK"))).toEqual({ label: "0 OK", tone: "ok" });
    expect(grpcBadge("idle", outcome(5, "NOT_FOUND"))).toEqual({
      label: "5 NOT_FOUND",
      tone: "error",
    });
  });

  it("says what is happening while a call runs, whatever the last outcome was", () => {
    expect(grpcBadge("running", outcome(0, "OK"))).toEqual({ label: "Running…", tone: "pending" });
    expect(grpcBadge("cancelling", null)).toEqual({ label: "Cancelling…", tone: "pending" });
  });

  it("is neutral before the first call", () => {
    expect(grpcBadge("idle", null)).toEqual({ label: "Not invoked", tone: "idle" });
  });
});

describe("grpcPrimaryAction and methodKindLabel", () => {
  it("offers Invoke, then Cancel, then waits", () => {
    expect(grpcPrimaryAction("idle").label).toBe("Invoke");
    expect(grpcPrimaryAction("running").label).toBe("Cancel");
    expect(grpcPrimaryAction("cancelling").kind).toBe("busy");
  });

  it("names every method kind", () => {
    expect(methodKindLabel("serverStreaming")).toBe("Server streaming");
    expect(methodKindLabel("bidirectional")).toBe("Bidirectional");
  });
});

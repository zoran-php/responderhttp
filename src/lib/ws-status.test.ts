import { describe, expect, it } from "vitest";

import { connectionBadge, primaryAction } from "@/lib/ws-status";

describe("connectionBadge", () => {
  it("follows the state matrix", () => {
    expect(connectionBadge("idle", null)).toEqual({ label: "Disconnected", tone: "disconnected" });
    expect(connectionBadge("connecting", null)).toEqual({ label: "Connecting…", tone: "pending" });
    expect(connectionBadge("connected", null)).toEqual({ label: "Connected", tone: "connected" });
    expect(connectionBadge("disconnecting", null).label).toBe("Disconnecting…");
  });

  it("counts reconnect attempts as n/N", () => {
    expect(connectionBadge("reconnecting", { attempt: 3, maxAttempts: 10 }).label).toBe(
      "Reconnecting… 3/10",
    );
    expect(connectionBadge("reconnecting", null).label).toBe("Reconnecting…");
  });
});

describe("primaryAction", () => {
  it("connects, cancels a pending connect, disconnects, then waits", () => {
    expect(primaryAction("idle").kind).toBe("connect");
    expect(primaryAction("connecting").kind).toBe("cancel");
    expect(primaryAction("reconnecting").kind).toBe("cancel");
    expect(primaryAction("connected").kind).toBe("disconnect");
    expect(primaryAction("disconnecting").kind).toBe("busy");
  });
});

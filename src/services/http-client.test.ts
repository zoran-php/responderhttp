// http_client/src/services/http-client.test.ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { cancelRequest, sendRequest } from "@/services/http-client";
import { AUTH_NONE, DEFAULT_SETTINGS, type HttpResponse, type SendRequestInput } from "@/types/http";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/plugin-log", () => ({
  trace: vi.fn(),
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}));

const input: SendRequestInput = {
  method: "GET",
  url: "https://example.com/",
  headers: [],
  queryParams: [],
  body: { kind: "none" },
  auth: AUTH_NONE,
  settings: DEFAULT_SETTINGS,
};

const response: HttpResponse = {
  status: 200,
  headers: [{ name: "content-type", value: "text/plain" }],
  body: { kind: "text", text: "hello" },
  timing: { dnsMs: 1, connectMs: 2, tlsMs: 3, timeToFirstByteMs: 4, totalMs: 10 },
};

describe("sendRequest", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes the id and input under the argument names the command expects", async () => {
    invoke.mockResolvedValue(response);

    const result = await sendRequest("req-1", input, input.url);

    expect(invoke).toHaveBeenCalledWith("send_request", { requestId: "req-1", request: input });
    expect(result).toEqual({ ok: true, value: response });
  });

  it("returns a typed failure instead of throwing when the command rejects", async () => {
    invoke.mockRejectedValue({ kind: "transport", message: "connection reset" });

    const result = await sendRequest("req-1", input, input.url);

    expect(result).toEqual({
      ok: false,
      error: { kind: "transport", message: "connection reset" },
    });
  });

  it("recognises a cancellation as its own error kind", async () => {
    invoke.mockRejectedValue({ kind: "cancelled", message: "request cancelled" });

    const result = await sendRequest("req-1", input, input.url);

    expect(result).toEqual({
      ok: false,
      error: { kind: "cancelled", message: "request cancelled" },
    });
  });

  it("maps a non-ApiError rejection to an internal error", async () => {
    invoke.mockRejectedValue("command send_request not found");

    const result = await sendRequest("req-1", input, input.url);

    expect(result).toEqual({
      ok: false,
      error: { kind: "internal", message: "command send_request not found" },
    });
  });
});

/** Outside Tauri the logger writes to the console only, so the console is
 * where the lines are checked. The file sink receives the same string. */
describe("what reaches the log", () => {
  let debugLines: ReturnType<typeof vi.spyOn>;
  let warnLines: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    invoke.mockReset();
    debugLines = vi.spyOn(console, "debug").mockImplementation(() => undefined);
    warnLines = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const resolved: SendRequestInput = { ...input, url: "https://example.com/?key=live-secret" };
  const logUrl = "https://example.com/?key={{secret}}";

  it("logs the URL it is given, never the resolved one", async () => {
    invoke.mockResolvedValue(response);

    await sendRequest("req-1", resolved, logUrl);

    expect(invoke).toHaveBeenCalledWith("send_request", { requestId: "req-1", request: resolved });
    const lines = JSON.stringify(debugLines.mock.calls);
    expect(lines).toContain("{{secret}}");
    expect(lines).not.toContain("live-secret");
  });

  it("keeps the resolved URL out of a failure line too", async () => {
    invoke.mockRejectedValue({ kind: "transport", message: "connection reset" });

    await sendRequest("req-1", resolved, logUrl);

    const lines = JSON.stringify(warnLines.mock.calls);
    expect(lines).toContain("{{secret}}");
    expect(lines).not.toContain("live-secret");
  });
});

describe("cancelRequest", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes the id through", async () => {
    invoke.mockResolvedValue(undefined);

    await cancelRequest("req-1");

    expect(invoke).toHaveBeenCalledWith("cancel_request", { requestId: "req-1" });
  });

  it("stays quiet when the command rejects", async () => {
    invoke.mockRejectedValue("no such request");

    await expect(cancelRequest("req-1")).resolves.toBeUndefined();
  });
});

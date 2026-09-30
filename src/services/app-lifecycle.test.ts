// http_client/src/services/app-lifecycle.test.ts
import { beforeEach, describe, expect, it, vi } from "vitest";

import { onQuitRequested, QUIT_REQUESTED_EVENT, quitApp } from "@/services/app-lifecycle";

const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => false }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/plugin-log", () => ({
  trace: vi.fn(),
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}));

beforeEach(() => {
  invoke.mockReset();
  listen.mockReset();
});

describe("onQuitRequested", () => {
  it("listens for the event Rust emits and calls the handler for each one", async () => {
    const unlisten = vi.fn();
    let deliver: (event: unknown) => void = () => {};
    listen.mockImplementation((_event: string, callback: (event: unknown) => void) => {
      deliver = callback;
      return Promise.resolve(unlisten);
    });
    const handler = vi.fn();

    const stop = await onQuitRequested(handler);

    expect(listen).toHaveBeenCalledWith(QUIT_REQUESTED_EVENT, expect.any(Function));
    deliver({ event: QUIT_REQUESTED_EVENT, payload: null });
    deliver({ event: QUIT_REQUESTED_EVENT, payload: null });
    expect(handler).toHaveBeenCalledTimes(2);
    expect(stop).toBe(unlisten);
  });
});

describe("quitApp", () => {
  it("calls the quit command", async () => {
    invoke.mockResolvedValue(undefined);

    const result = await quitApp();

    expect(invoke).toHaveBeenCalledWith("quit_app", undefined);
    expect(result).toEqual({ ok: true, value: undefined });
  });

  it("returns a typed failure when the command fails", async () => {
    invoke.mockRejectedValue({ kind: "internal", message: "nope" });

    const result = await quitApp();

    expect(result.ok).toBe(false);
  });
});

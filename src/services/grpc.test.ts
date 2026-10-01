import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  cancelAllGrpc,
  cancelGrpc,
  deleteProtoSchema,
  endGrpcStream,
  exampleMessage,
  importProto,
  invokeGrpc,
  MAX_EVENTS_PER_BATCH,
  reflectGrpc,
  saveProtoSchema,
  sendGrpcMessage,
} from "@/services/grpc";
import {
  DEFAULT_GRPC_SETTINGS,
  type GrpcCallOutcome,
  type GrpcEvent,
  type GrpcRequestInput,
} from "@/types/grpc";

interface FakeChannel {
  deliver: (event: GrpcEvent) => void;
}

const { invoke, channels, logDebug, logWarn } = vi.hoisted(() => ({
  invoke: vi.fn(),
  channels: [] as FakeChannel[],
  logDebug: vi.fn(),
  logWarn: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  isTauri: () => false,
  // Stands in for the real Channel, which needs the Tauri host. It keeps the
  // handler so a test can play the part of Rust.
  Channel: class {
    readonly deliver: (event: GrpcEvent) => void;
    constructor(onmessage: (event: GrpcEvent) => void) {
      this.deliver = onmessage;
      channels.push(this);
    }
  },
}));
vi.mock("@/services/logger", () => ({ logDebug, logWarn }));

const TOKEN = "s3cret-token";

const request: GrpcRequestInput = {
  target: { authority: "localhost:50051", tls: false },
  methodPath: "/shop.v1.Shop/GetOrder",
  metadata: [{ name: "x-tenant", value: "acme" }],
  auth: { kind: "bearer", token: TOKEN },
  settings: DEFAULT_GRPC_SETTINGS,
};

const outcome: GrpcCallOutcome = {
  status: { code: 0, name: "OK", message: "" },
  by: "server",
  trailers: [],
  totalMs: 12,
};

const received = (json: string): GrpcEvent => ({ type: "received", atMs: 1, json, bytes: 3 });
const ended: GrpcEvent = { type: "ended", atMs: 2, ...outcome };

/** Starts a call whose invoke stays pending until `finish` is called, with a
 * scheduler the test runs by hand. */
function start() {
  const frames: (() => void)[] = [];
  const batches: GrpcEvent[][] = [];
  let finish: (value: GrpcCallOutcome) => void = () => undefined;
  invoke.mockImplementation(
    () =>
      new Promise<GrpcCallOutcome>((resolve) => {
        finish = resolve;
      }),
  );

  const result = invokeGrpc(
    "call-1",
    "proto_1",
    request,
    '{"id":"1"}',
    "localhost:50051",
    (batch) => batches.push(batch),
    (flush) => frames.push(flush),
  );

  const channel = channels[channels.length - 1];
  if (channel === undefined) {
    throw new Error("invoke should create a channel");
  }
  const runFrame = () => {
    for (const flush of frames.splice(0)) {
      flush();
    }
  };
  return { result, channel, batches, runFrame, finish: (value: GrpcCallOutcome) => finish(value) };
}

beforeEach(() => {
  invoke.mockReset();
  logDebug.mockReset();
  logWarn.mockReset();
  channels.length = 0;
});

describe("invokeGrpc", () => {
  it("passes every argument under the name the command expects", async () => {
    const { result, channel, finish } = start();
    finish(outcome);

    expect(await result).toEqual({ ok: true, value: outcome });
    expect(invoke).toHaveBeenCalledWith("grpc_invoke", {
      callId: "call-1",
      schemaId: "proto_1",
      request,
      message: '{"id":"1"}',
      onEvent: channel,
    });
  });

  it("delivers messages once per frame, in order, and the end at once", async () => {
    const { channel, batches, runFrame, finish, result } = start();

    channel.deliver(received("1"));
    channel.deliver(received("2"));
    expect(batches).toEqual([]);
    runFrame();
    expect(batches).toEqual([[received("1"), received("2")]]);

    channel.deliver(received("3"));
    channel.deliver(ended);
    expect(batches[1]).toEqual([received("3"), ended]);

    finish(outcome);
    await result;
  });

  it("does not wait for a frame once the batch is full", async () => {
    const { channel, batches, finish, result } = start();

    for (let index = 0; index < MAX_EVENTS_PER_BATCH; index += 1) {
      channel.deliver(received(String(index)));
    }

    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(MAX_EVENTS_PER_BATCH);
    finish(outcome);
    await result;
  });

  it("treats a non-OK status as a result, not a failure", async () => {
    const { result, finish } = start();
    const denied: GrpcCallOutcome = {
      ...outcome,
      status: { code: 7, name: "PERMISSION_DENIED", message: "no" },
    };
    finish(denied);

    expect(await result).toEqual({ ok: true, value: denied });
  });

  it("returns a typed failure when the call never starts", async () => {
    invoke.mockRejectedValue({
      kind: "invalidRequest",
      message: 'metadata key "te" is reserved by gRPC or HTTP/2 and cannot be set',
    });

    const result = await invokeGrpc("c", "s", request, "{}", "localhost:50051", () => undefined);

    expect(result).toEqual({
      ok: false,
      error: {
        kind: "invalidRequest",
        message: 'metadata key "te" is reserved by gRPC or HTTP/2 and cannot be set',
      },
    });
  });

  /** CLAUDE.md section 11, rule 6. */
  it("logs the target, method and status, never the auth, metadata or message", async () => {
    const { result, finish } = start();
    finish(outcome);
    await result;
    invoke.mockRejectedValue({ kind: "transport", message: "refused" });
    await invokeGrpc("c", "s", request, '{"id":"1"}', "localhost:50051", () => undefined);

    const logged = [...logDebug.mock.calls, ...logWarn.mock.calls].flat().join("\n");
    expect(logged).toContain("localhost:50051/shop.v1.Shop/GetOrder");
    expect(logged).toContain("0 OK");
    expect(logged).not.toContain(TOKEN);
    expect(logged).not.toContain("acme");
    expect(logged).not.toContain('{"id":"1"}');
  });
});

describe("a refused message", () => {
  /** A JSON mapping error can quote the value it could not read. */
  it("is logged by its error kind only, and the caller still gets the whole error", async () => {
    const error = {
      kind: "invalidRequest",
      message: 'invalid value: string "s3cret", expected i64',
    };
    invoke.mockRejectedValue(error);

    const sent = await sendGrpcMessage("call-1", '{"id":"s3cret"}');
    const started = await invokeGrpc("c", "s", request, '{"id":"s3cret"}', "t", () => undefined);

    expect(sent).toEqual({ ok: false, error });
    expect(started).toEqual({ ok: false, error });
    const logged = [...logDebug.mock.calls, ...logWarn.mock.calls].flat().join("\n");
    expect(logged).toContain("invalidRequest");
    expect(logged).not.toContain("s3cret");
  });
});

describe("the other commands", () => {
  it("pass their arguments under the names the commands expect", async () => {
    invoke.mockResolvedValue(null);

    await reflectGrpc(request);
    await importProto(["C:\\protos\\shop.proto"], ["C:\\protos"]);
    await saveProtoSchema("reflection:plain:localhost:50051", "Shop");
    await deleteProtoSchema("proto_1");
    await exampleMessage("proto_1", "/shop.v1.Shop/GetOrder");
    await sendGrpcMessage("call-1", "{}");
    await endGrpcStream("call-1");

    expect(invoke.mock.calls).toEqual([
      ["grpc_reflect", { request }],
      ["grpc_import_proto", { roots: ["C:\\protos\\shop.proto"], importPaths: ["C:\\protos"] }],
      ["grpc_save_schema", { schemaId: "reflection:plain:localhost:50051", name: "Shop" }],
      ["grpc_delete_schema", { schemaId: "proto_1" }],
      ["grpc_example_message", { schemaId: "proto_1", methodPath: "/shop.v1.Shop/GetOrder" }],
      ["grpc_send", { callId: "call-1", message: "{}" }],
      ["grpc_end_stream", { callId: "call-1" }],
    ]);
  });

  it("return a typed failure, not a throw, when the command rejects", async () => {
    invoke.mockRejectedValue({
      kind: "invalidRequest",
      message: "schema proto_1 is used by: Get order",
    });

    expect(await deleteProtoSchema("proto_1")).toEqual({
      ok: false,
      error: { kind: "invalidRequest", message: "schema proto_1 is used by: Get order" },
    });
  });
});

describe("cancelGrpc and cancelAllGrpc", () => {
  it("never throw, even when the command fails", async () => {
    invoke.mockRejectedValue("gone");

    await expect(cancelGrpc("call-1")).resolves.toBeUndefined();
    await expect(cancelAllGrpc()).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("grpc_cancel", { callId: "call-1" });
    expect(invoke).toHaveBeenCalledWith("grpc_cancel_all");
  });
});

// http_client/src/store/grpc-tab.test.ts
//
// The gRPC half of the tabs store (PLAN-GRPC.md 16h), in its own file like
// websocket-tab.test.ts: it needs the gRPC service mocked.
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useEnvironmentsStore } from "@/store/environments-store";
import { useTabsStore, type GrpcTab } from "@/store/request-store";
import {
  DEFAULT_GRPC_SETTINGS,
  type GrpcCallOutcome,
  type GrpcEvent,
  type GrpcRequestInput,
  type ProtoSchema,
  type SavedGrpcRequest,
} from "@/types/grpc";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

type OnEvents = (events: GrpcEvent[]) => void;

const service = vi.hoisted(() => ({
  getProtoSchema: vi.fn(),
  reflectGrpc: vi.fn(),
  importProto: vi.fn(),
  saveProtoSchema: vi.fn(),
  exampleMessage: vi.fn(),
  invokeGrpc: vi.fn(),
  sendGrpcMessage: vi.fn(),
  endGrpcStream: vi.fn(),
  cancelGrpc: vi.fn(),
}));

vi.mock("@/services/grpc", () => service);

const schema: ProtoSchema = {
  id: "proto_session_1",
  name: null,
  origin: "import",
  files: ["shop.proto"],
  services: [
    {
      name: "shop.v1.Shop",
      methods: [
        {
          name: "GetOrder",
          path: "/shop.v1.Shop/GetOrder",
          kind: "unary",
          inputType: "shop.v1.GetOrderRequest",
          outputType: "shop.v1.Order",
        },
        {
          name: "Chat",
          path: "/shop.v1.Shop/Chat",
          kind: "bidirectional",
          inputType: "shop.v1.Line",
          outputType: "shop.v1.Line",
        },
      ],
    },
  ],
};

const outcome: GrpcCallOutcome = {
  status: { code: 0, name: "OK", message: "" },
  by: "server",
  trailers: [{ name: "x-trace", value: "t-1" }],
  totalMs: 12,
};
const ok: Result<void, ApiError> = { ok: true, value: undefined };
const sent = (json: string): GrpcEvent => ({ type: "sent", atMs: 1, json, bytes: 3 });
const received = (json: string): GrpcEvent => ({ type: "received", atMs: 2, json, bytes: 3 });
const metadata: GrpcEvent = {
  type: "responseMetadata",
  atMs: 1,
  metadata: [{ name: "x-request-id", value: "r-1" }],
};
const ended: GrpcEvent = { type: "ended", atMs: 3, ...outcome };

function tab(): GrpcTab {
  const active = useTabsStore.getState().activeGrpcTab();
  if (active === null) {
    throw new Error("expected a gRPC tab in front");
  }
  return active;
}

/** The call in flight: what the store passed, and Rust's two ways back. */
interface Call {
  callId: string;
  schemaId: string;
  request: GrpcRequestInput;
  message: string;
  logTarget: string;
  onEvents: OnEvents;
  finish: (result: Result<GrpcCallOutcome, ApiError>) => void;
}

let calls: Call[] = [];

function lastCall(): Call {
  const call = calls[calls.length - 1];
  if (call === undefined) {
    throw new Error("expected an invoke");
  }
  return call;
}

/** A tab on `url` with the test schema loaded and `method` picked. */
async function ready(method = "/shop.v1.Shop/GetOrder", url = "localhost:50051"): Promise<void> {
  useTabsStore.getState().openBlankGrpcTab();
  useTabsStore.getState().setGrpcUrl(url);
  useTabsStore.getState().setGrpcTls(false);
  await useTabsStore.getState().importGrpcSchema(["C:\\protos\\shop.proto"], []);
  useTabsStore.getState().selectGrpcMethod(method);
}

/** Starts Invoke and returns the promise, still pending until `finish`. */
function invoke(): Promise<void> {
  return useTabsStore.getState().invokeGrpc();
}

beforeEach(() => {
  calls = [];
  for (const mock of Object.values(service)) {
    mock.mockReset();
  }
  service.importProto.mockResolvedValue({ ok: true, value: schema });
  service.sendGrpcMessage.mockResolvedValue(ok);
  service.endGrpcStream.mockResolvedValue(ok);
  service.cancelGrpc.mockResolvedValue(undefined);
  service.invokeGrpc.mockImplementation(
    (
      callId: string,
      schemaId: string,
      request: GrpcRequestInput,
      message: string,
      logTarget: string,
      onEvents: OnEvents,
    ) =>
      new Promise<Result<GrpcCallOutcome, ApiError>>((resolve) => {
        calls.push({ callId, schemaId, request, message, logTarget, onEvents, finish: resolve });
      }),
  );
  useEnvironmentsStore.setState({ activeEnvironmentId: null, variablesById: {} });
  for (const open of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(open.id);
  }
});

describe("a unary call", () => {
  it("invokes with the target, method and message, and fills the response", async () => {
    await ready();
    useTabsStore.getState().setGrpcMessage('{"id":"o-1"}');

    const running = invoke();
    expect(tab().call).toBe("running");
    const call = lastCall();
    expect(call.schemaId).toBe("proto_session_1");
    expect(call.request.target).toEqual({ authority: "localhost:50051", tls: false });
    expect(call.request.methodPath).toBe("/shop.v1.Shop/GetOrder");
    expect(call.message).toBe('{"id":"o-1"}');

    call.onEvents([sent('{"id":"o-1"}'), metadata, received('{"total":"12"}'), ended]);
    call.finish({ ok: true, value: outcome });
    await running;

    const { response } = tab();
    expect(tab().call).toBe("idle");
    expect(response?.metadata).toEqual([{ name: "x-request-id", value: "r-1" }]);
    expect(response?.lastMessage).toBe('{"total":"12"}');
    expect(response?.receivedCount).toBe(1);
    expect(response?.outcome).toEqual(outcome);
    expect(tab().log.entries.map((entry) => entry.event.type)).toEqual([
      "sent",
      "responseMetadata",
      "received",
      "ended",
    ]);
  });

  it("ends the call from the outcome when it arrives before the ended event", async () => {
    await ready();
    const running = invoke();
    lastCall().finish({ ok: true, value: outcome });
    await running;

    expect(tab().call).toBe("idle");
    expect(tab().response?.outcome).toEqual(outcome);
  });

  it("sends {} for an empty message", async () => {
    await ready();
    const running = invoke();

    expect(lastCall().message).toBe("{}");
    lastCall().finish({ ok: true, value: outcome });
    await running;
  });

  it("shows why a call did not start and goes back to idle", async () => {
    await ready();
    const running = invoke();
    lastCall().finish({
      ok: false,
      error: { kind: "invalidRequest", message: 'metadata key "te" is reserved' },
    });
    await running;

    expect(tab().call).toBe("idle");
    expect(tab().response).toBeNull();
    expect(tab().callError).toBe('metadata key "te" is reserved');
  });

  it("refuses to invoke with no schema, no method or a bad target", async () => {
    useTabsStore.getState().openBlankGrpcTab();
    await invoke();
    expect(tab().callError).toContain("Load a schema first");

    await useTabsStore.getState().importGrpcSchema(["shop.proto"], []);
    await invoke();
    expect(tab().callError).toBe("Pick a method to call.");

    useTabsStore.getState().selectGrpcMethod("/shop.v1.Shop/GetOrder");
    useTabsStore.getState().setGrpcUrl("api.test/shop.v1.Shop/GetOrder");
    await invoke();
    expect(tab().callError).toContain("no path");

    expect(service.invokeGrpc).not.toHaveBeenCalled();
    expect(tab().call).toBe("idle");
  });
});

describe("a bidirectional call", () => {
  it("opens with no message, then sends, ends the stream and can be cancelled", async () => {
    await ready("/shop.v1.Shop/Chat");
    const running = invoke();
    const call = lastCall();
    expect(call.message).toBe("");

    useTabsStore.getState().setGrpcMessage('{"text":"hi"}');
    await useTabsStore.getState().sendGrpcMessage();
    expect(service.sendGrpcMessage).toHaveBeenCalledWith(call.callId, '{"text":"hi"}');
    call.onEvents([sent('{"text":"hi"}'), received('{"text":"hello"}')]);

    await useTabsStore.getState().endGrpcStream();
    expect(service.endGrpcStream).toHaveBeenCalledWith(call.callId);
    expect(tab().streamEnded).toBe(true);
    // Nothing more can be sent once the stream has ended.
    await useTabsStore.getState().sendGrpcMessage();
    expect(service.sendGrpcMessage).toHaveBeenCalledTimes(1);

    useTabsStore.getState().cancelGrpc();
    expect(tab().call).toBe("cancelling");
    expect(service.cancelGrpc).toHaveBeenCalledWith(call.callId);

    call.onEvents([
      {
        type: "ended",
        atMs: 9,
        status: { code: 1, name: "CANCELLED", message: "" },
        trailers: [],
        by: "client",
        totalMs: 40,
      },
    ]);
    call.finish({ ok: true, value: outcome });
    await running;
    expect(tab().call).toBe("idle");
    expect(tab().response?.outcome?.status.name).toBe("CANCELLED");
  });

  it("shows a refused message in the composer and keeps the call going", async () => {
    await ready("/shop.v1.Shop/Chat");
    const running = invoke();
    service.sendGrpcMessage.mockResolvedValue({
      ok: false,
      error: { kind: "invalidRequest", message: "unknown field `txt`" },
    });

    await useTabsStore.getState().sendGrpcMessage();

    expect(tab().composerError).toBe("unknown field `txt`");
    expect(tab().call).toBe("running");
    lastCall().finish({ ok: true, value: outcome });
    await running;
  });

  it("does not send on a unary call", async () => {
    await ready();
    const running = invoke();

    await useTabsStore.getState().sendGrpcMessage();

    expect(service.sendGrpcMessage).not.toHaveBeenCalled();
    lastCall().finish({ ok: true, value: outcome });
    await running;
  });
});

describe("secret variables", () => {
  beforeEach(() => {
    useEnvironmentsStore.setState({
      activeEnvironmentId: "env_1",
      variablesById: {
        env_1: [
          { name: "host", value: "localhost", secret: false, secretState: "ok" },
          { name: "token", value: "s3cret", secret: true, secretState: "ok" },
        ],
      },
    });
  });

  it("are resolved for Rust but never reach the log", async () => {
    await ready("/shop.v1.Shop/GetOrder", "{{host}}:50051");
    useTabsStore.getState().setGrpcMetadataRows([{ id: "m", name: "x-key", value: "{{token}}" }]);
    useTabsStore.getState().setGrpcMessage('{"token":"{{token}}"}');

    const running = invoke();
    const call = lastCall();
    expect(call.request.target.authority).toBe("localhost:50051");
    expect(call.request.metadata).toEqual([{ name: "x-key", value: "s3cret" }]);
    expect(call.message).toBe('{"token":"s3cret"}');

    call.onEvents([sent('{"token": "s3cret"}'), ended]);
    call.finish({ ok: true, value: outcome });
    await running;

    expect(JSON.stringify(tab().log)).not.toContain("s3cret");
    expect(tab().log.entries[0]?.event).toMatchObject({ json: '{"token":"{{token}}"}' });
  });
});

describe("the grpcurl snippet", () => {
  it("is the resolved call, with the imported schema's files and the method", async () => {
    useEnvironmentsStore.setState({
      activeEnvironmentId: "env_1",
      variablesById: {
        env_1: [{ name: "token", value: "s3cret", secret: true, secretState: "ok" }],
      },
    });
    await ready();
    useTabsStore.getState().setGrpcMetadataRows([{ id: "m", name: "x-key", value: "{{token}}" }]);
    useTabsStore.getState().setGrpcMessage('{"id":"1"}');

    const command = useTabsStore.getState().grpcurlCommand();

    expect(command).toContain("-plaintext");
    expect(command).toContain("-proto 'shop.proto'");
    expect(command).toContain("-H 'x-key: s3cret'");
    expect(command).toContain(`-d '{"id":"1"}'`);
    expect(command).toContain("'localhost:50051'");
    expect(command).toContain("'shop.v1.Shop/GetOrder'");
  });

  it("is refused, with the reason on the tab, when no method is picked", async () => {
    await ready("");

    expect(useTabsStore.getState().grpcurlCommand()).toBeNull();
    expect(tab().callError).toBe("Pick a method to call.");
  });
});

describe("the URL field and the message editor", () => {
  it("takes a pasted scheme off and moves the lock", () => {
    useTabsStore.getState().openBlankGrpcTab();
    useTabsStore.getState().setGrpcTls(false);
    useTabsStore.getState().setGrpcUrl("grpcs://api.test:443");

    useTabsStore.getState().normalizeGrpcUrl();

    expect(tab().url).toBe("api.test:443");
    expect(tab().tls).toBe(true);
  });

  it("beautifies without changing a 64-bit number, and says why when it cannot", () => {
    useTabsStore.getState().openBlankGrpcTab();
    useTabsStore.getState().setGrpcMessage('{"id":9007199254740993}');

    useTabsStore.getState().beautifyGrpcMessage();
    expect(tab().message).toBe('{\n  "id": 9007199254740993\n}');

    useTabsStore.getState().setGrpcMessage("{");
    useTabsStore.getState().beautifyGrpcMessage();
    expect(tab().message).toBe("{");
    expect(tab().composerError).toContain("Not valid JSON");
  });

  it("puts the example message in the editor", async () => {
    await ready();
    service.exampleMessage.mockResolvedValue({ ok: true, value: '{\n  "id": ""\n}' });

    await useTabsStore.getState().fillExampleGrpcMessage();

    expect(service.exampleMessage).toHaveBeenCalledWith(
      "proto_session_1",
      "/shop.v1.Shop/GetOrder",
    );
    expect(tab().message).toBe('{\n  "id": ""\n}');
  });
});

describe("schemas", () => {
  it("reflects with the tab's target and metadata, and records reflection", async () => {
    useTabsStore.getState().openBlankGrpcTab();
    useTabsStore.getState().setGrpcUrl("api.test");
    useTabsStore.getState().setGrpcMetadataRows([{ id: "m", name: "x-tenant", value: "acme" }]);
    service.reflectGrpc.mockResolvedValue({
      ok: true,
      value: { ...schema, id: "reflection:tls:api.test:443", origin: "reflection" },
    });

    await useTabsStore.getState().reflectGrpcSchema();

    const [request] = service.reflectGrpc.mock.calls[0] as [GrpcRequestInput];
    expect(request.target).toEqual({ authority: "api.test:443", tls: true });
    expect(request.metadata).toEqual([{ name: "x-tenant", value: "acme" }]);
    expect(tab().schemaStatus).toBe("loaded");
    expect(tab().schemaRef).toEqual({ kind: "reflection" });
  });

  it("shows why reflection failed", async () => {
    useTabsStore.getState().openBlankGrpcTab();
    useTabsStore.getState().setGrpcUrl("api.test");
    service.reflectGrpc.mockResolvedValue({
      ok: false,
      error: { kind: "transport", message: "the server does not support reflection" },
    });

    await useTabsStore.getState().reflectGrpcSchema();

    expect(tab().schemaStatus).toBe("error");
    expect(tab().schemaError).toBe("the server does not support reflection");
  });

  it("records an imported schema by its id", async () => {
    await ready();

    expect(tab().schemaRef).toEqual({ kind: "library", schemaId: "proto_session_1" });
  });

  it("reflects on leaving the URL only when the server changed", async () => {
    service.reflectGrpc.mockResolvedValue({
      ok: true,
      value: { ...schema, id: "reflection:plain:a:1", origin: "reflection" },
    });
    useTabsStore.getState().openBlankGrpcTab();
    useTabsStore.getState().setGrpcTls(false);
    useTabsStore.getState().setGrpcUrl("a:1");

    await useTabsStore.getState().reflectGrpcSchemaIfStale();
    await useTabsStore.getState().reflectGrpcSchemaIfStale();
    expect(service.reflectGrpc).toHaveBeenCalledTimes(1);

    useTabsStore.getState().setGrpcUrl("b:2");
    await useTabsStore.getState().reflectGrpcSchemaIfStale();
    expect(service.reflectGrpc).toHaveBeenCalledTimes(2);
  });

  it("does not reflect on leaving the URL for a library schema, an empty URL or a bad one", async () => {
    await ready();
    await useTabsStore.getState().reflectGrpcSchemaIfStale();

    useTabsStore.getState().openBlankGrpcTab();
    await useTabsStore.getState().reflectGrpcSchemaIfStale();
    useTabsStore.getState().setGrpcUrl("a:99999");
    await useTabsStore.getState().reflectGrpcSchemaIfStale();

    expect(service.reflectGrpc).not.toHaveBeenCalled();
  });

  it("saves a schema imported this session to the library before a request is saved", async () => {
    await ready();
    service.saveProtoSchema.mockResolvedValue({
      ok: true,
      value: { ...schema, name: "Get order" },
    });

    expect(await useTabsStore.getState().ensureGrpcSchemaSaved("Get order")).toBeNull();

    expect(service.saveProtoSchema).toHaveBeenCalledWith("proto_session_1", "Get order");
    expect(tab().schema?.name).toBe("Get order");
    expect(useTabsStore.getState().currentGrpcDraft().schema).toEqual({
      kind: "library",
      schemaId: "proto_session_1",
    });
    // Already in the library now: nothing more to do.
    expect(await useTabsStore.getState().ensureGrpcSchemaSaved("Again")).toBeNull();
    expect(service.saveProtoSchema).toHaveBeenCalledTimes(1);
  });

  it("says why the schema could not be saved", async () => {
    await ready();
    service.saveProtoSchema.mockResolvedValue({
      ok: false,
      error: { kind: "invalidRequest", message: "a schema needs a name" },
    });

    expect(await useTabsStore.getState().ensureGrpcSchemaSaved(" ")).toBe(
      "The schema could not be saved to the library: a schema needs a name",
    );
  });
});

describe("saved gRPC requests", () => {
  const saved: SavedGrpcRequest = {
    id: "req_g",
    collectionId: "col_1",
    folderId: null,
    name: "Get order",
    request: {
      url: "{{host}}:50051",
      tls: false,
      methodPath: "/shop.v1.Shop/GetOrder",
      schema: { kind: "library", schemaId: "proto_1" },
      metadata: [{ name: "x-tenant", value: "acme" }],
      auth: { kind: "none" },
      message: '{"id":"1"}',
      settings: DEFAULT_GRPC_SETTINGS,
    },
    secretState: "ok",
  };

  it("opens clean, fetches its library schema, and focuses the open tab", async () => {
    service.getProtoSchema.mockResolvedValue({
      ok: true,
      value: { ...schema, id: "proto_1", name: "Shop" },
    });

    useTabsStore.getState().openSavedGrpcRequest(saved);
    expect(tab().schemaStatus).toBe("loading");
    await vi.waitFor(() => expect(tab().schemaStatus).toBe("loaded"));

    expect(service.getProtoSchema).toHaveBeenCalledWith("proto_1");
    expect(tab().metadataRows[0]).toMatchObject({ name: "x-tenant", value: "acme" });
    expect(useTabsStore.getState().isDirty(tab().id)).toBe(false);

    const first = tab().id;
    useTabsStore.getState().openBlankTab();
    const count = useTabsStore.getState().tabs.length;
    useTabsStore.getState().openSavedGrpcRequest(saved);
    expect(useTabsStore.getState().activeTabId).toBe(first);
    expect(useTabsStore.getState().tabs).toHaveLength(count);
  });

  it("does not send anything on open for a reflection request", () => {
    useTabsStore.getState().openSavedGrpcRequest({
      ...saved,
      request: { ...saved.request, schema: { kind: "reflection" } },
    });

    expect(service.getProtoSchema).not.toHaveBeenCalled();
    expect(service.reflectGrpc).not.toHaveBeenCalled();
    expect(tab().schemaStatus).toBe("none");
  });

  it("is dirty after an edit and clean again once saved", () => {
    service.getProtoSchema.mockResolvedValue({ ok: true, value: schema });
    useTabsStore.getState().openSavedGrpcRequest(saved);
    const id = tab().id;

    useTabsStore.getState().setGrpcMessage("{}");
    expect(useTabsStore.getState().isDirty(id)).toBe(true);
    expect(useTabsStore.getState().currentGrpcDraft().message).toBe("{}");

    useTabsStore.getState().markGrpcSaved({
      id: saved.id,
      collectionId: saved.collectionId,
      folderId: null,
      name: saved.name,
    });
    expect(useTabsStore.getState().isDirty(id)).toBe(false);
  });

  it("is untouched by the HTTP and WebSocket setters", () => {
    service.getProtoSchema.mockResolvedValue({ ok: true, value: schema });
    useTabsStore.getState().openSavedGrpcRequest(saved);

    useTabsStore.getState().setUrl("https://elsewhere.test/");
    useTabsStore.getState().setWsUrl("wss://elsewhere.test/");

    expect(tab().url).toBe(saved.request.url);
    expect(useTabsStore.getState().isDirty(tab().id)).toBe(false);
  });
});

describe("tab lifetime", () => {
  it("cancels a running call when its tab is closed", async () => {
    await ready();
    void invoke();
    const { callId } = lastCall();

    useTabsStore.getState().closeTab(tab().id);

    expect(service.cancelGrpc).toHaveBeenCalledWith(callId);
    lastCall().finish({ ok: true, value: outcome });
  });
});

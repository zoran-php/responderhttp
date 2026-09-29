// http_client/src/features/grpc/GrpcView.test.tsx
//
// The gRPC tab driven through the real store with the gRPC service mocked:
// the test plays Rust's part by handing events to the callback the store
// gave `invokeGrpc`. Monaco is stubbed as in WebSocketView.test.tsx.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { GrpcView } from "@/features/grpc/GrpcView";
import { useTabsStore, type GrpcTab } from "@/store/request-store";
import type { GrpcCallOutcome, GrpcEvent, ProtoSchema } from "@/types/grpc";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";

type OnEvents = (events: GrpcEvent[]) => void;
type Finish = (result: Result<GrpcCallOutcome, ApiError>) => void;

const service = vi.hoisted(() => ({
  getProtoSchema: vi.fn(),
  reflectGrpc: vi.fn(),
  importProto: vi.fn(),
  chooseProtoFiles: vi.fn(),
  chooseImportFolder: vi.fn(),
  saveProtoSchema: vi.fn(),
  exampleMessage: vi.fn(),
  invokeGrpc: vi.fn(),
  sendGrpcMessage: vi.fn(),
  endGrpcStream: vi.fn(),
  cancelGrpc: vi.fn(),
}));
vi.mock("@/services/grpc", () => service);

const docs = vi.hoisted(() => ({ itemDocs: vi.fn(), setItemDocs: vi.fn() }));
vi.mock("@/services/docs", () => docs);

vi.mock("@/components/LazyCodeEditor", () => ({
  LazyCodeEditor: ({
    value,
    onChange,
    readOnly,
  }: {
    value: string;
    onChange?: (value: string) => void;
    readOnly?: boolean;
  }) => (
    <textarea
      aria-label={readOnly === true ? "Read-only content" : "Message editor"}
      onChange={(event) => onChange?.(event.target.value)}
      readOnly={readOnly}
      value={value}
    />
  ),
}));

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

const ok = (code = 0, name = "OK", message = ""): GrpcCallOutcome => ({
  status: { code, name, message },
  by: "server",
  trailers: [{ name: "x-trace", value: "t-1" }],
  totalMs: 42,
});

let pending: { onEvents: OnEvents; finish: Finish }[] = [];

function lastCall() {
  const call = pending[pending.length - 1];
  if (call === undefined) {
    throw new Error("expected an invoke");
  }
  return call;
}

/** Renders the live tab, so a store update re-renders the way App does. */
function Host() {
  const tab = useTabsStore((state) =>
    state.tabs.find((candidate): candidate is GrpcTab => candidate.kind === "grpc"),
  );
  return tab === undefined ? null : (
    <GrpcView onSave={() => undefined} pathSegments={["Untitled gRPC"]} tab={tab} />
  );
}

async function withSchemaAndMethod(methodPath: string) {
  await act(async () => {
    await useTabsStore.getState().importGrpcSchema(["C:\\protos\\shop.proto"], []);
  });
  fireEvent.change(screen.getByLabelText("Method"), { target: { value: methodPath } });
}

async function pressInvoke() {
  const before = pending.length;
  fireEvent.click(screen.getByRole("button", { name: "Invoke" }));
  await waitFor(() => expect(pending.length).toBe(before + 1));
  return lastCall();
}

beforeEach(() => {
  pending = [];
  for (const mock of Object.values(service)) {
    mock.mockReset();
  }
  service.importProto.mockResolvedValue({ ok: true, value: schema });
  service.sendGrpcMessage.mockResolvedValue({ ok: true, value: undefined });
  service.endGrpcStream.mockResolvedValue({ ok: true, value: undefined });
  service.cancelGrpc.mockResolvedValue(undefined);
  service.invokeGrpc.mockImplementation(
    (
      _callId: string,
      _schemaId: string,
      _request: unknown,
      _message: string,
      _logTarget: string,
      onEvents: OnEvents,
    ) =>
      new Promise((resolve: Finish) => {
        pending.push({ onEvents, finish: resolve });
      }),
  );
  docs.itemDocs.mockReset().mockResolvedValue({ ok: true, value: "" });
  for (const tab of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(tab.id);
  }
  useTabsStore.getState().openBlankGrpcTab();
  useTabsStore.getState().setGrpcUrl("localhost:50051");
  useTabsStore.getState().setGrpcTls(false);
});

afterEach(() => {
  cleanup();
});

describe("GrpcView", () => {
  it("runs a unary call and shows the message, status, metadata and trailers", async () => {
    render(<Host />);
    await withSchemaAndMethod("/shop.v1.Shop/GetOrder");
    expect(screen.getByText("Not invoked")).toBeTruthy();

    const call = await pressInvoke();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
    act(() =>
      call.onEvents([
        { type: "sent", atMs: 1, json: "{}", bytes: 0 },
        { type: "responseMetadata", atMs: 2, metadata: [{ name: "x-request-id", value: "r-1" }] },
        { type: "received", atMs: 3, json: '{"id":9007199254740993}', bytes: 12 },
        { type: "ended", atMs: 4, ...ok() },
      ]),
    );
    await act(async () => call.finish({ ok: true, value: ok() }));

    expect(screen.getByText("0 OK")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Invoke" })).toBeTruthy();
    const body = within(screen.getByLabelText("Response message")).getByLabelText(
      "Read-only content",
    ) as HTMLTextAreaElement;
    // Shown exactly as received: not rounded to 9007199254740992.
    expect(body.value).toBe('{\n  "id": 9007199254740993\n}');

    fireEvent.click(screen.getByRole("tab", { name: "Metadata" }));
    expect(screen.getAllByText("x-request-id").length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("tab", { name: "Trailers" }));
    expect(screen.getByText("t-1")).toBeTruthy();
  });

  it("shows a non-OK status as the result, with its message", async () => {
    render(<Host />);
    await withSchemaAndMethod("/shop.v1.Shop/GetOrder");

    const call = await pressInvoke();
    const denied = ok(7, "PERMISSION_DENIED", "caller may not read orders");
    act(() => call.onEvents([{ type: "ended", atMs: 4, ...denied }]));
    await act(async () => call.finish({ ok: true, value: denied }));

    expect(screen.getByText("7 PERMISSION_DENIED")).toBeTruthy();
    expect(screen.getByText("caller may not read orders")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says why Invoke did nothing when no method is picked", async () => {
    render(<Host />);
    await act(async () => {
      await useTabsStore.getState().importGrpcSchema(["shop.proto"], []);
    });

    fireEvent.click(screen.getByRole("button", { name: "Invoke" }));

    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toBe("Pick a method to call."),
    );
    expect(service.invokeGrpc).not.toHaveBeenCalled();
  });

  it("sends, ends the stream and shows the messages for a bidirectional call", async () => {
    render(<Host />);
    await withSchemaAndMethod("/shop.v1.Shop/Chat");
    const send = () => screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
    const end = () => screen.getByRole("button", { name: "End Streaming" }) as HTMLButtonElement;
    expect(send().disabled).toBe(true);

    const call = await pressInvoke();
    expect(send().disabled).toBe(false);
    fireEvent.change(screen.getByLabelText("Message editor"), {
      target: { value: '{"text":"hi"}' },
    });
    fireEvent.click(send());
    await waitFor(() => expect(service.sendGrpcMessage).toHaveBeenCalled());
    act(() =>
      call.onEvents([
        { type: "sent", atMs: 1, json: '{"text":"hi"}', bytes: 4 },
        { type: "received", atMs: 2, json: '{"text":"hello"}', bytes: 7 },
      ]),
    );

    const log = screen.getByRole("list", { name: "Messages" });
    expect(within(log).getByText('{"text":"hello"}')).toBeTruthy();
    expect(within(log).getByText('{"text":"hi"}')).toBeTruthy();

    fireEvent.click(end());
    await waitFor(() => expect(service.endGrpcStream).toHaveBeenCalled());
    await waitFor(() => expect(send().disabled).toBe(true));
    expect(end().disabled).toBe(true);

    await act(async () => call.finish({ ok: true, value: ok() }));
  });

  it("offers Send and End Streaming only for a method whose client streams", async () => {
    render(<Host />);
    await withSchemaAndMethod("/shop.v1.Shop/Chat");
    expect(screen.queryByRole("button", { name: "Send" })).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Method"), {
      target: { value: "/shop.v1.Shop/GetOrder" },
    });
    expect(screen.queryByRole("button", { name: "Send" })).toBeNull();
    expect(screen.queryByRole("button", { name: "End Streaming" })).toBeNull();

    fireEvent.change(screen.getByLabelText("Method"), { target: { value: "/shop.v1.Shop/Chat" } });
    expect(screen.queryByRole("button", { name: "End Streaming" })).toBeTruthy();
  });

  it("moves the lock when a grpcs:// URL is pasted", () => {
    render(<Host />);
    const lock = () => screen.getByRole("button", { name: /TLS/ });
    expect(lock().getAttribute("aria-pressed")).toBe("false");

    fireEvent.change(screen.getByLabelText("gRPC server URL"), {
      target: { value: "grpcs://api.test:443" },
    });

    expect((screen.getByLabelText("gRPC server URL") as HTMLInputElement).value).toBe(
      "api.test:443",
    );
    expect(lock().getAttribute("aria-pressed")).toBe("true");
  });

  it("imports .proto files from the Service definition tab and lists the methods", async () => {
    service.chooseProtoFiles.mockResolvedValue({ ok: true, value: ["C:\\protos\\shop.proto"] });
    render(<Host />);

    fireEvent.click(screen.getByRole("button", { name: "Service definition" }));
    fireEvent.click(screen.getByRole("button", { name: "Import .proto files…" }));

    await waitFor(() =>
      expect(service.importProto).toHaveBeenCalledWith(["C:\\protos\\shop.proto"], []),
    );
    await waitFor(() => expect(screen.getByText("shop.v1.Shop")).toBeTruthy());
    expect(screen.getByText("Imported from .proto files")).toBeTruthy();
  });
});

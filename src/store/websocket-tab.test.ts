// http_client/src/store/websocket-tab.test.ts
//
// The WebSocket half of the tabs store (PLAN.md Phase 13e), in its own file
// like docs-tab.test.ts: it needs the WebSocket service mocked, and the
// request-tab tests should not.
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useEnvironmentsStore } from "@/store/environments-store";
import { useTabsStore, type WebSocketTab } from "@/store/request-store";
import type { SavedWebSocket } from "@/types/collections";
import type { ApiError } from "@/types/http";
import type { Result } from "@/types/result";
import {
  DEFAULT_WS_SETTINGS,
  type WebSocketRequest,
  type WsEvent,
  type WsPayload,
} from "@/types/websocket";

type OnEvents = (events: WsEvent[]) => void;

const service = vi.hoisted(() => ({
  connectWebSocket: vi.fn(),
  sendWebSocketMessage: vi.fn(),
  disconnectWebSocket: vi.fn(),
}));

vi.mock("@/services/websocket", () => service);

/** Plays Rust's part: the last connect's id, request and event callback. */
function lastConnect(): { connectionId: string; request: WebSocketRequest; onEvents: OnEvents } {
  const calls = service.connectWebSocket.mock.calls;
  const call = calls[calls.length - 1];
  if (call === undefined) {
    throw new Error("expected a connect");
  }
  const [connectionId, request, , onEvents] = call as [string, WebSocketRequest, string, OnEvents];
  return { connectionId, request, onEvents };
}

function tab(): WebSocketTab {
  const active = useTabsStore.getState().activeWebSocketTab();
  if (active === null) {
    throw new Error("expected a WebSocket tab in front");
  }
  return active;
}

const ok: Result<void, ApiError> = { ok: true, value: undefined };
const text = (value: string): WsPayload => ({ kind: "text", text: value });
const connectedEvent: WsEvent = {
  type: "connected",
  atMs: 1,
  url: "wss://echo.test/",
  status: 101,
  headers: [],
};
const message = (type: "sent" | "received", value: string): WsEvent => ({
  type,
  atMs: 2,
  byteLength: value.length,
  payload: text(value),
});
const closedEvent: WsEvent = { type: "closed", atMs: 3, code: 1000, reason: "", by: "user" };

/** Opens a tab on `url` and connects it, with the handshake done. */
async function connected(url = "wss://echo.test/"): Promise<OnEvents> {
  useTabsStore.getState().openBlankWebSocketTab();
  useTabsStore.getState().setWsUrl(url);
  await useTabsStore.getState().connect();
  const { onEvents } = lastConnect();
  onEvents([connectedEvent]);
  return onEvents;
}

function logTypes(): string[] {
  return tab().log.entries.map((entry) => entry.event.type);
}

beforeEach(() => {
  service.connectWebSocket.mockReset().mockResolvedValue(ok);
  service.sendWebSocketMessage.mockReset().mockResolvedValue(ok);
  service.disconnectWebSocket.mockReset().mockResolvedValue(undefined);
  useEnvironmentsStore.setState({ activeEnvironmentId: null, variablesById: {} });
  for (const open of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(open.id);
  }
  service.disconnectWebSocket.mockClear();
});

describe("a WebSocket session", () => {
  it("connects, sends, receives and disconnects through the service", async () => {
    useTabsStore.getState().openBlankWebSocketTab();
    useTabsStore.getState().setWsUrl("wss://echo.test/");

    const connecting = useTabsStore.getState().connect();
    expect(tab().connection).toBe("connecting");
    await connecting;
    const { connectionId, request, onEvents } = lastConnect();
    expect(request.url).toBe("wss://echo.test/");

    onEvents([connectedEvent]);
    expect(tab().connection).toBe("connected");

    useTabsStore.getState().setWsDraft({ text: "hello" });
    await useTabsStore.getState().sendMessage();
    expect(service.sendWebSocketMessage).toHaveBeenCalledWith(connectionId, text("hello"));

    onEvents([message("sent", "hello"), message("received", "hello")]);
    useTabsStore.getState().disconnect();
    expect(tab().connection).toBe("disconnecting");
    expect(service.disconnectWebSocket).toHaveBeenCalledWith(connectionId);

    onEvents([closedEvent]);
    expect(tab().connection).toBe("idle");
    expect(logTypes()).toEqual(["connected", "sent", "received", "closed"]);
    // The draft stays for the next send.
    expect(tab().draft.text).toBe("hello");
  });

  it("logs a failed handshake and returns to idle", async () => {
    service.connectWebSocket.mockResolvedValue({
      ok: false,
      error: { kind: "transport", message: "server refused the WebSocket upgrade: HTTP 403" },
    });
    useTabsStore.getState().openBlankWebSocketTab();

    await useTabsStore.getState().connect();

    expect(tab().connection).toBe("idle");
    const [entry] = tab().log.entries;
    expect(entry?.event).toMatchObject({
      type: "error",
      message: "Could not connect: server refused the WebSocket upgrade: HTTP 403",
    });
  });

  it("shows the badge's attempt count while reconnecting", async () => {
    const onEvents = await connected();

    onEvents([
      { type: "error", atMs: 4, message: "dropped" },
      { type: "reconnecting", atMs: 4, attempt: 2, maxAttempts: 10, delayMs: 2000 },
    ]);

    expect(tab().connection).toBe("reconnecting");
    expect(tab().reconnectAttempt).toEqual({ attempt: 2, maxAttempts: 10 });
    onEvents([connectedEvent]);
    expect(tab().connection).toBe("connected");
    expect(tab().reconnectAttempt).toBeNull();
  });

  it("refuses hex that does not parse, and sends nothing", async () => {
    await connected();
    useTabsStore.getState().setWsDraft({ format: "binary", binaryEncoding: "hex", text: "de zz" });

    await useTabsStore.getState().sendMessage();

    expect(service.sendWebSocketMessage).not.toHaveBeenCalled();
    expect(tab().composerError).toBe('Byte 1: "zz" is not a hex byte');
    useTabsStore.getState().setWsDraft({ text: "de ad" });
    expect(tab().composerError).toBeNull();
  });

  it("sends Base64 as a binary frame, carried as hex", async () => {
    await connected();
    useTabsStore
      .getState()
      .setWsDraft({ format: "binary", binaryEncoding: "base64", text: "3q2+7w==" });

    await useTabsStore.getState().sendMessage();

    expect(service.sendWebSocketMessage).toHaveBeenCalledWith(expect.any(String), {
      kind: "binary",
      hex: "deadbeef",
    });
  });

  it("sends XML and HTML as text, exactly as typed", async () => {
    await connected();
    useTabsStore.getState().setWsDraft({ format: "xml", text: "<a><b></a>" });

    await useTabsStore.getState().sendMessage();

    expect(service.sendWebSocketMessage).toHaveBeenCalledWith(
      expect.any(String),
      text("<a><b></a>"),
    );
  });

  it("does not send while disconnected", async () => {
    useTabsStore.getState().openBlankWebSocketTab();
    useTabsStore.getState().setWsDraft({ text: "hello" });

    await useTabsStore.getState().sendMessage();

    expect(service.sendWebSocketMessage).not.toHaveBeenCalled();
  });
});

describe("secret variables", () => {
  beforeEach(() => {
    useEnvironmentsStore.setState({
      activeEnvironmentId: "env_1",
      variablesById: {
        env_1: [
          { name: "host", value: "echo.test", secret: false, secretState: "ok" },
          { name: "token", value: "s3cret", secret: true, secretState: "ok" },
        ],
      },
    });
  });

  it("are resolved for Rust but never reach the log", async () => {
    const onEvents = await connected("wss://{{host}}/?t={{token}}");
    const { request } = lastConnect();
    expect(request.url).toBe("wss://echo.test/?t=s3cret");
    expect(service.connectWebSocket).toHaveBeenLastCalledWith(
      expect.any(String),
      expect.anything(),
      "wss://echo.test/?t={{token}}",
      expect.any(Function),
    );

    useTabsStore.getState().setWsDraft({ text: "auth {{token}}" });
    await useTabsStore.getState().sendMessage();
    expect(service.sendWebSocketMessage).toHaveBeenLastCalledWith(
      expect.any(String),
      text("auth s3cret"),
    );
    onEvents([message("sent", "auth s3cret")]);

    const shown = tab().log.entries.map((entry) => entry.event);
    expect(shown[0]).toMatchObject({ type: "connected", url: "wss://echo.test/?t={{token}}" });
    expect(shown[1]).toMatchObject({ type: "sent", payload: text("auth {{token}}") });
  });
});

describe("Beautify", () => {
  it("reformats the draft", () => {
    useTabsStore.getState().openBlankWebSocketTab();
    useTabsStore.getState().setWsDraft({ format: "json", text: '{"a":1}' });

    useTabsStore.getState().beautifyWsDraft();

    expect(tab().draft.text).toBe('{\n  "a": 1\n}');
    expect(tab().composerError).toBeNull();
  });

  it("leaves text that does not parse as typed, and says why", () => {
    useTabsStore.getState().openBlankWebSocketTab();
    useTabsStore.getState().setWsDraft({ format: "xml", text: "<a><b></a>" });

    useTabsStore.getState().beautifyWsDraft();

    expect(tab().draft.text).toBe("<a><b></a>");
    expect(tab().composerError).toBe("Not valid XML: </a> closes <b>");
  });

  it("does nothing for Text or Binary", () => {
    useTabsStore.getState().openBlankWebSocketTab();
    useTabsStore.getState().setWsDraft({ format: "text", text: '{"a":1}' });

    useTabsStore.getState().beautifyWsDraft();

    expect(tab().draft.text).toBe('{"a":1}');
  });
});

describe("clearing the log", () => {
  it("Clear Messages empties every connection's entries, without disconnecting", async () => {
    const first = await connected();
    first([message("received", "old"), closedEvent]);
    await useTabsStore.getState().connect();
    const second = lastConnect().onEvents;
    second([connectedEvent, message("received", "new")]);

    useTabsStore.getState().clearMessages();

    expect(tab().log.entries).toEqual([]);
    expect(tab().connection).toBe("connected");
    expect(service.disconnectWebSocket).not.toHaveBeenCalled();
  });

  it("new frames keep arriving after Clear Messages", async () => {
    const onEvents = await connected();
    onEvents([message("received", "a")]);

    useTabsStore.getState().clearMessages();
    expect(tab().log.entries).toEqual([]);

    onEvents([message("received", "b")]);
    expect(logTypes()).toEqual(["received"]);
    expect(tab().connection).toBe("connected");
  });

  it("does not touch the draft, the filter or the search", async () => {
    const onEvents = await connected();
    onEvents([message("received", "a")]);
    useTabsStore.getState().setWsDraft({ text: "keep" });
    useTabsStore.getState().setLogFilter("received");
    useTabsStore.getState().setLogQuery("a");

    useTabsStore.getState().clearMessages();

    expect(tab().draft.text).toBe("keep");
    expect(tab().logFilter).toBe("received");
    expect(tab().logQuery).toBe("a");
  });
});

describe("tab lifetime", () => {
  it("disconnects a connected tab when it is closed", async () => {
    await connected();
    const { connectionId } = lastConnect();

    useTabsStore.getState().closeTab(tab().id);

    expect(service.disconnectWebSocket).toHaveBeenCalledWith(connectionId);
  });

  it("locks the URL and params while connected, but not the headers or draft", async () => {
    await connected("wss://echo.test/?a=1");

    useTabsStore.getState().setWsUrl("wss://other.test/");
    useTabsStore.getState().setWsHeaderRows([{ id: "h", name: "X-Trace", value: "1" }]);
    useTabsStore.getState().setWsDraft({ text: "typed" });

    expect(tab().url).toBe("wss://echo.test/?a=1");
    expect(tab().headerRows[0]?.name).toBe("X-Trace");
    expect(tab().draft.text).toBe("typed");
  });
});

describe("saved WebSocket requests", () => {
  const saved: SavedWebSocket = {
    id: "req_ws",
    collectionId: "col_1",
    folderId: null,
    name: "Live prices",
    request: {
      url: "wss://prices.test/?symbol=ACME",
      headers: [{ name: "X-Key", value: "{{key}}" }],
      settings: DEFAULT_WS_SETTINGS,
    },
    draft: { format: "json", binaryEncoding: "base64", text: '{"subscribe": true}' },
  };

  it("opens loaded and clean, and focuses the open tab instead of a duplicate", () => {
    useTabsStore.getState().openSavedWebSocket(saved);
    const first = tab();
    expect(first.paramRows[0]).toMatchObject({ name: "symbol", value: "ACME" });
    expect(first.draft.format).toBe("json");
    expect(useTabsStore.getState().isDirty(first.id)).toBe(false);

    useTabsStore.getState().openBlankTab();
    const count = useTabsStore.getState().tabs.length;
    useTabsStore.getState().openSavedWebSocket(saved);

    expect(useTabsStore.getState().activeTabId).toBe(first.id);
    expect(useTabsStore.getState().tabs).toHaveLength(count);
  });

  it("is dirty after a draft edit and clean again once saved", () => {
    useTabsStore.getState().openSavedWebSocket(saved);
    const id = tab().id;

    useTabsStore.getState().setWsDraft({ text: "{}" });
    expect(useTabsStore.getState().isDirty(id)).toBe(true);

    useTabsStore.getState().markWebSocketSaved({
      id: saved.id,
      collectionId: saved.collectionId,
      folderId: null,
      name: saved.name,
    });
    expect(useTabsStore.getState().isDirty(id)).toBe(false);
  });

  it("is untouched by the HTTP setters", () => {
    useTabsStore.getState().openSavedWebSocket(saved);

    useTabsStore.getState().setUrl("https://elsewhere.test/");
    useTabsStore.getState().setMethod("POST");

    expect(tab().url).toBe(saved.request.url);
    expect(useTabsStore.getState().isDirty(tab().id)).toBe(false);
  });
});

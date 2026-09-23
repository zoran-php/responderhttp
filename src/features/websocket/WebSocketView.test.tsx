// http_client/src/features/websocket/WebSocketView.test.tsx
//
// The spec's WebSocket scenarios, driven through the real store with the
// WebSocket service mocked: the test plays Rust's part by handing events to
// the callback the store gave `connectWebSocket`.
//
// Monaco is stubbed, as in DocsEditor.test.tsx, and plain assertions are used
// for the same reason: this project has no jest-dom setup.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WebSocketView } from "@/features/websocket/WebSocketView";
import { useTabsStore, type WebSocketTab } from "@/store/request-store";
import type { WsEvent } from "@/types/websocket";

type OnEvents = (events: WsEvent[]) => void;

const service = vi.hoisted(() => ({
  connectWebSocket: vi.fn(),
  sendWebSocketMessage: vi.fn(),
  disconnectWebSocket: vi.fn(),
}));
vi.mock("@/services/websocket", () => service);

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
      aria-label={readOnly === true ? "Message content" : "Message editor"}
      onChange={(event) => onChange?.(event.target.value)}
      readOnly={readOnly}
      value={value}
    />
  ),
}));

const connected: WsEvent = {
  type: "connected",
  atMs: Date.UTC(2026, 8, 22, 10, 0, 0, 1),
  url: "wss://echo.test/",
  status: 101,
  headers: [{ name: "upgrade", value: "websocket" }],
};
const message = (type: "sent" | "received", text: string): WsEvent => ({
  type,
  atMs: Date.UTC(2026, 8, 22, 10, 0, 1, 250),
  byteLength: text.length,
  payload: { kind: "text", text },
});
const closed: WsEvent = {
  type: "closed",
  atMs: Date.UTC(2026, 8, 22, 10, 0, 2),
  code: 1000,
  reason: "",
  by: "user",
};

/** Renders the live tab, so a store update re-renders the way App does. */
function Host() {
  const tab = useTabsStore((state) =>
    state.tabs.find((candidate): candidate is WebSocketTab => candidate.kind === "websocket"),
  );
  return tab === undefined ? null : (
    <WebSocketView
      onManageCookies={() => undefined}
      onSave={() => undefined}
      pathSegments={["Untitled WebSocket"]}
      tab={tab}
    />
  );
}

function lastOnEvents(): OnEvents {
  const calls = service.connectWebSocket.mock.calls;
  const call = calls[calls.length - 1];
  if (call === undefined) {
    throw new Error("expected a connect");
  }
  return call[3] as OnEvents;
}

/** Presses Connect and plays a successful handshake. */
async function connect(): Promise<OnEvents> {
  const before = service.connectWebSocket.mock.calls.length;
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  await waitFor(() => expect(service.connectWebSocket.mock.calls.length).toBe(before + 1));
  const onEvents = lastOnEvents();
  act(() => onEvents([connected]));
  return onEvents;
}

function rows(): HTMLElement[] {
  return within(screen.getByRole("list", { name: "Messages" })).queryAllByRole("button", {
    expanded: false,
  });
}

function rowTexts(): string[] {
  return rows().map((row) => row.textContent ?? "");
}

beforeEach(() => {
  service.connectWebSocket.mockReset().mockResolvedValue({ ok: true, value: undefined });
  service.sendWebSocketMessage.mockReset().mockResolvedValue({ ok: true, value: undefined });
  service.disconnectWebSocket.mockReset().mockResolvedValue(undefined);
  docs.itemDocs.mockReset().mockResolvedValue({ ok: true, value: "" });
  for (const tab of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(tab.id);
  }
  // After the teardown: closing the previous test's connected tab
  // disconnects it, which is not a call this test made.
  service.disconnectWebSocket.mockClear();
  useTabsStore.getState().openBlankWebSocketTab();
  useTabsStore.getState().setWsUrl("wss://echo.test/");
});

afterEach(() => {
  cleanup();
});

describe("WebSocketView", () => {
  it("Connect flips the badge to Connected and the button to Disconnect", async () => {
    render(<Host />);
    expect(screen.getByText("Disconnected")).toBeTruthy();

    await connect();

    expect(screen.getByText("Connected")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
    expect((screen.getByLabelText("WebSocket URL") as HTMLInputElement).disabled).toBe(true);
  });

  it("keeps Send disabled until connected", async () => {
    render(<Host />);
    const send = () => screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
    expect(send().disabled).toBe(true);

    await connect();

    expect(send().disabled).toBe(false);
  });

  it("shows a sent and a received entry with millisecond times after a send and its echo", async () => {
    render(<Host />);
    const onEvents = await connect();

    fireEvent.change(screen.getByLabelText("Message editor"), { target: { value: "hello" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(service.sendWebSocketMessage).toHaveBeenCalled());
    act(() => onEvents([message("sent", "hello"), message("received", "hello")]));

    const list = screen.getByRole("list", { name: "Messages" });
    expect(within(list).getByLabelText("Sent")).toBeTruthy();
    expect(within(list).getByLabelText("Received")).toBeTruthy();
    const times = within(list)
      .getAllByText(/^\d{2}:\d{2}:\d{2}\.\d{3}$/)
      .map((time) => time.textContent);
    expect(times).toHaveLength(3);
    // Newest first.
    expect(rowTexts()[0]).toContain("hello");
    expect(rowTexts()[2]).toContain("Connected to wss://echo.test/");
  });

  it("narrows the rows with the filter and the search", async () => {
    render(<Host />);
    const onEvents = await connect();
    act(() =>
      onEvents([message("sent", "ping"), message("received", "pong"), message("received", "news")]),
    );

    fireEvent.change(screen.getByLabelText("Filter messages"), { target: { value: "received" } });
    expect(rowTexts()).toHaveLength(2);

    fireEvent.change(screen.getByLabelText("Search messages"), { target: { value: "new" } });
    expect(rowTexts()).toHaveLength(1);
    expect(rowTexts()[0]).toContain("news");
  });

  it("Clear Messages empties the messages of every connection, and there is no Clear Response", async () => {
    render(<Host />);
    const first = await connect();
    act(() => first([message("received", "from the first"), closed]));
    const second = await connect();
    act(() => second([message("received", "from the second")]));

    fireEvent.click(screen.getByRole("button", { name: "Clear Messages" }));

    expect(rowTexts()).toHaveLength(0);
    expect(screen.getByText("Connect to see messages here.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "More actions" })).toBeNull();
    expect(screen.queryByText("Clear Response")).toBeNull();
  });

  it("keeps the connection when cleared, and the next echo still lands", async () => {
    render(<Host />);
    const onEvents = await connect();
    act(() => onEvents([message("received", "before")]));

    fireEvent.click(screen.getByRole("button", { name: "Clear Messages" }));
    act(() => onEvents([message("received", "after")]));

    expect(screen.getByText("Connected")).toBeTruthy();
    expect(service.disconnectWebSocket).not.toHaveBeenCalled();
    expect(rowTexts()).toHaveLength(1);
    expect(rowTexts()[0]).toContain("after");
  });

  it("expands a message to its full text and byte size", async () => {
    render(<Host />);
    const onEvents = await connect();
    act(() => onEvents([message("received", '{"a":1}')]));

    fireEvent.click(rows()[0] as HTMLElement);

    const content = screen.getByLabelText("Message content") as HTMLTextAreaElement;
    expect(content.value).toBe('{\n  "a": 1\n}');
    expect(screen.getByText("7 B")).toBeTruthy();
  });

  it("offers Beautify for JSON, XML and HTML, and the encoding for Binary", () => {
    render(<Host />);
    const format = screen.getByLabelText("Message format");

    expect(screen.queryByRole("button", { name: "Beautify" })).toBeNull();
    for (const value of ["json", "xml", "html"]) {
      fireEvent.change(format, { target: { value } });
      expect(screen.getByRole("button", { name: "Beautify" })).toBeTruthy();
      expect(screen.queryByLabelText("Binary encoding")).toBeNull();
    }

    fireEvent.change(format, { target: { value: "binary" } });
    expect(screen.queryByRole("button", { name: "Beautify" })).toBeNull();
    const encoding = screen.getByLabelText("Binary encoding") as HTMLSelectElement;
    expect(encoding.value).toBe("base64");
    expect(Array.from(encoding.options).map((option) => option.text)).toEqual([
      "Base64",
      "Hexadecimal",
    ]);
  });

  it("Beautify reformats the message, or says why it could not", () => {
    render(<Host />);
    fireEvent.change(screen.getByLabelText("Message format"), { target: { value: "xml" } });
    const editor = screen.getByLabelText("Message editor") as HTMLTextAreaElement;

    fireEvent.change(editor, { target: { value: "<a><b>x</b></a>" } });
    fireEvent.click(screen.getByRole("button", { name: "Beautify" }));
    expect((screen.getByLabelText("Message editor") as HTMLTextAreaElement).value).toBe(
      "<a>\n  <b>x</b>\n</a>",
    );

    fireEvent.change(screen.getByLabelText("Message editor"), { target: { value: "<a>" } });
    fireEvent.click(screen.getByRole("button", { name: "Beautify" }));
    expect((screen.getByLabelText("Message editor") as HTMLTextAreaElement).value).toBe("<a>");
    expect(screen.getByText("Not valid XML: <a> is never closed")).toBeTruthy();
  });

  it("shows a received binary message in the composer's encoding, switchable per row", async () => {
    render(<Host />);
    const onEvents = await connect();
    act(() =>
      onEvents([
        { type: "received", atMs: 1, byteLength: 4, payload: { kind: "binary", hex: "deadbeef" } },
      ]),
    );

    // Collapsed, the row already reads in the composer's encoding.
    expect(rowTexts()[0]).toContain("Binary, 4 bytes: 3q2+7w==");

    fireEvent.click(rows()[0] as HTMLElement);
    expect(screen.getByText("3q2+7w==")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Show binary as"), { target: { value: "hex" } });
    // Testing Library folds runs of spaces when matching text.
    expect(screen.getByText(/^00000000 de ad be ef/)).toBeTruthy();
  });

  it("asks for a save before documentation, and opens the Docs tab once saved", async () => {
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Docs" }));
    expect(screen.getByText("Save the request to add documentation.")).toBeTruthy();

    act(() =>
      useTabsStore.getState().markWebSocketSaved({
        id: "req_ws",
        collectionId: "col_1",
        folderId: null,
        name: "Live",
      }),
    );
    docs.itemDocs.mockResolvedValue({ ok: true, value: "# Live prices" });
    cleanup();
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Docs" }));

    expect((await screen.findByRole("heading", { level: 1 })).textContent).toBe("Live prices");
    fireEvent.click(screen.getByRole("button", { name: "Edit documentation" }));
    expect(useTabsStore.getState().activeTab().kind).toBe("docs");
  });
});

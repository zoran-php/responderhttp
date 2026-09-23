// http_client/src/store/stream-tab.test.ts
//
// What a tab does with what a request reports before it finishes
// (PLAN-SSE.md, 14d), in its own file like websocket-tab.test.ts: it needs
// the HTTP service mocked, and the request-tab tests should not.
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useTabsStore, type RequestTab } from "@/store/request-store";
import type { ApiError, HttpResponse, HttpStreamEvent } from "@/types/http";
import type { Result } from "@/types/result";

type OnStream = (events: HttpStreamEvent[]) => void;

const service = vi.hoisted(() => ({
  sendRequest: vi.fn(),
  cancelRequest: vi.fn(),
  sendAndDownload: vi.fn(),
}));

vi.mock("@/services/http-client", () => service);

const response: HttpResponse = {
  status: 200,
  headers: [{ name: "content-type", value: "text/event-stream" }],
  body: { kind: "text", text: "data: a\n\n" },
  timing: { dnsMs: 1, connectMs: 1, tlsMs: 1, timeToFirstByteMs: 2, totalMs: 9 },
  sizes: { requestHeaders: 241, requestBody: 0, responseHeaders: 352, responseBody: 757 },
};

const headers = (contentType: string): HttpStreamEvent => ({
  type: "headers",
  status: 200,
  headers: [{ name: "content-type", value: contentType }],
  bytes: 120,
});

const block = (data: string): HttpStreamEvent => ({
  type: "block",
  atMs: 1,
  bytes: `data: ${data}\n\n`.length,
  block: {
    kind: "event",
    name: "message",
    data,
    id: null,
    retry: null,
    raw: `data: ${data}\n\n`,
  },
});

/** Sends without letting the request finish, and hands back its stream sink. */
function startSending(): { onStream: OnStream; finish: () => Promise<void> } {
  let settle: (result: Result<HttpResponse, ApiError>) => void = () => undefined;
  service.sendRequest.mockImplementation(
    () =>
      new Promise<Result<HttpResponse, ApiError>>((resolve) => {
        settle = resolve;
      }),
  );
  const sending = useTabsStore.getState().send();
  const call = service.sendRequest.mock.calls[service.sendRequest.mock.calls.length - 1];
  if (call === undefined) {
    throw new Error("expected a send");
  }
  return {
    onStream: call[3] as OnStream,
    finish: async () => {
      settle({ ok: true, value: response });
      await sending;
    },
  };
}

function tabById(id: string): RequestTab {
  const tab = useTabsStore.getState().tabs.find((candidate) => candidate.id === id);
  if (tab === undefined || tab.kind !== "request") {
    throw new Error("expected a request tab");
  }
  return tab;
}

beforeEach(() => {
  service.sendRequest.mockReset();
  service.cancelRequest.mockReset();
  service.sendAndDownload.mockReset();
  for (const tab of [...useTabsStore.getState().tabs]) {
    useTabsStore.getState().closeTab(tab.id);
  }
  useTabsStore.getState().setUrl("https://example.com/stream");
});

describe("a streaming response", () => {
  it("fills the tab's log as the blocks arrive, before the request finishes", async () => {
    const tabId = useTabsStore.getState().activeTabId;
    const { onStream, finish } = startSending();

    onStream([headers("text/event-stream"), block("a")]);
    onStream([block("b")]);

    const streaming = tabById(tabId);
    expect(streaming.status).toBe("sending");
    expect(streaming.stream.isEventStream).toBe(true);
    expect(streaming.stream.head?.status).toBe(200);
    expect(streaming.stream.log.entries.map((entry) => entry.block.raw)).toEqual([
      "data: a\n\n",
      "data: b\n\n",
    ]);
    await finish();
    expect(tabById(tabId).stream.log.entries).toHaveLength(2);
  });

  it("keeps filling the tab it belongs to after the user moves to another", async () => {
    const tabId = useTabsStore.getState().activeTabId;
    const { onStream, finish } = startSending();
    useTabsStore.getState().openBlankTab();

    onStream([headers("text/event-stream"), block("a")]);

    expect(tabById(tabId).stream.log.entries).toHaveLength(1);
    const other = useTabsStore.getState().activeRequestTab();
    expect(other?.stream.log.entries).toHaveLength(0);
    await finish();
  });

  it("starts the next send from an empty log", async () => {
    const tabId = useTabsStore.getState().activeTabId;
    const first = startSending();
    first.onStream([headers("text/event-stream"), block("a")]);
    await first.finish();

    const second = startSending();

    expect(tabById(tabId).stream.log.entries).toHaveLength(0);
    expect(tabById(tabId).stream.isEventStream).toBe(false);
    await second.finish();
  });
});

describe("an ordinary response", () => {
  it("records its head and nothing else", async () => {
    const tabId = useTabsStore.getState().activeTabId;
    const { onStream, finish } = startSending();

    onStream([headers("application/json")]);
    await finish();

    const tab = tabById(tabId);
    expect(tab.stream.isEventStream).toBe(false);
    expect(tab.stream.head?.status).toBe(200);
    expect(tab.stream.log.entries).toHaveLength(0);
  });
});

# PLAN-SSE.md — Phase 14: Server-sent events

Written 2026-09-22. The plan for showing an `text/event-stream` response as it
arrives, the way the WebSocket log already shows messages.

Companion to `PLAN.md` (the phased build plan) and `PLAN-WEBSOCKET.md`. The
same rules apply: `CLAUDE.md` wins, `verify.bat` is the gate, and nothing here
is done until it is green on your machine.

---

## 1. What you asked for

> Currently our http requests waits for sse stream to end to display the
> entire response. It should show as it receives like websocket.

Reference stream: `https://sse.tools.typinks.com/api/story`, which sends one
`data:` line per word and ends with `event: end`.

**Decision D1, confirmed:** a request becomes a live event stream **when the
response says `Content-Type: text/event-stream`**. Nothing to switch on, and
Send stays one button.

**D2 to D4** were offered with the same question and not answered, so the
recommended answers stand. Say so if any of them is wrong; each is a small
change while the phase is being built, and a larger one after.

| # | Question | Taken as |
|---|---|---|
| **D2** | What the response pane shows | **Events and Raw.** A live list of events (time, name, data, id) plus a Raw view of the stream as it arrived, beside the existing Headers view. |
| **D3** | The request timeout | **Stops applying once the response is an event stream.** It still guards connecting and waiting for the first byte. A stream then runs until the server ends it, it breaks, or you press Cancel. |
| **D4** | Retention | **1,000 events and 32 MiB**, the WebSocket log's caps, with a note when older events are dropped. |

---

## 2. What SSE is, in the amount this needs

A `text/event-stream` body is UTF-8 text in blocks separated by a blank line
(RFC: the WHATWG HTML "server-sent events" section):

```
: this is a comment

event: end
id: 42
retry: 3000
data: first line
data: second line

```

- A field is `name: value`, one space after the colon optional.
- `data` lines of one block join with `\n`.
- A line starting with `:` is a comment. Servers send them as keep-alives.
- A block with no `data` field dispatches nothing, but `id` and `retry` still
  apply.
- Line endings may be `\n`, `\r\n` or `\r`, and may split across chunks.
- A leading byte-order mark is stripped once.
- `event` defaults to `message`.

**Not in this phase:** the automatic reconnect a browser's `EventSource`
does (with `Last-Event-ID` and the server's `retry`). This is an API client:
the user presses Send again. `retry` and `id` are shown, not acted on.

---

## 3. Where the streaming has to reach

Today a request is one blocking call. `CurlClient::send` fills a `Vec<u8>`
and returns an `HttpResponse` when the transfer ends, which is exactly why a
stream shows nothing until the server hangs up.

```
ResponseViewer ← request-store ← services/http-client.ts   (invoke, one promise)
                                        │
                            commands/request.rs send_request
                                        │
                        SendRequest::execute → HttpClient port
                                        │
                     CurlClient::perform → Easy2<Collector>::write
```

Four changes, and nothing else moves:

1. **The port grows a streaming method with a default.** `HttpClient` gets
   `send_streaming(request, cancel, on_update)` whose default implementation
   calls `send`, so every existing mock, the cookie decorator and every test
   keep compiling and behaving. `CurlClient` overrides it; `CookieClient`
   forwards it and keeps its jar behaviour.
2. **The write callback parses.** Once the header callback has seen
   `content-type: text/event-stream`, each chunk goes through the parser and
   every complete block is handed to `on_update`. The bytes are still
   collected for the final `HttpResponse`, up to a cap (below).
3. **The command streams.** `send_request` takes a per-request
   `Channel<HttpStreamEventDto>`, exactly as `connect_web_socket` does, and
   still resolves with the whole `HttpResponse` at the end. History, saved
   responses, Copy as cURL and every other consumer are untouched.
4. **The viewer gains an Events view**, fed by the store as batches arrive.

---

## 4. Decisions this raises, and how they are settled

**The timeout has to move (D3).** libcurl's `CURLOPT_TIMEOUT` cannot be
relaxed once `perform` has started, and the app cannot know a response is a
stream until its headers arrive. So the total timeout stops being libcurl's
and becomes the progress callback's: it aborts when the deadline passes,
unless the response has turned out to be an event stream. `CONNECTTIMEOUT`
stays libcurl's.
- A timed-out request still fails as `Transport`, with a message naming the
  timeout rather than libcurl's "aborted by callback".
- The existing integration test (a 2 s response against a 200 ms timeout)
  keeps passing.

**A stream that never ends must not fill memory.** Two caps:
- Rust keeps at most **8 MiB** of raw body for the final `HttpResponse`, then
  stops collecting and marks it truncated. The events already streamed are
  unaffected, and the Raw view is fed by the events, not by this.
- The frontend keeps **1,000 events and 32 MiB** (D4), the WebSocket caps,
  through the same code: `appendCapped` moves out of `lib/ws-log.ts` into a
  shared `lib/capped-list.ts` that both use.

**Batching.** Events arrive one per word on the reference stream. The
frontend reuses `lib/event-batcher.ts` from the WebSocket work: one store
update per animation frame, flushed early at 500 events.

**No new dependency.** The parser is a small state machine in
`domain/sse.rs`, tested against the WHATWG rules. `libcurl` already gives the
chunks.

---

## 5. What it looks like

The response pane, while a stream is running:

```
┌ Response ────────────────────────────────────────────────────────────┐
│ [200] 1.2 s · 63 events · Streaming…              Pretty Raw Headers │
│                                                   ^ Events tab added │
│ 12:18:32.896  message   artwork.                                  ⌄  │
│ 12:18:32.812  message   the                                       ⌄  │
│ 12:18:32.749  message   of                                        ⌄  │
│ …                                                                    │
└──────────────────────────────────────────────────────────────────────┘
```

- **Events** is the default view for an event stream, and is not offered for
  any other response. Newest first, like the WebSocket log.
- A row is time, event name, and the first line of data. Expanding shows the
  whole `data`, the `id` and `retry` when present, and a Copy button.
- A comment block shows as a dim `:` row, so a keep-alive is visible but
  quiet.
- The header line shows the status as soon as the headers arrive, the event
  count, and "Streaming…" until the stream ends. The existing Cancel button
  in the builder stops it; the log then shows how it ended.
- **Raw** shows the stream as it arrived, appended live.
- **Pretty** is not offered for an event stream: there is nothing to
  pretty-print that the Events view does not show better.
- When the stream ends, the pane keeps everything on screen and the status
  line says how it ended (the server closed it, it broke, or you cancelled).

---

## 6. Sub-phases

Each ends green on `verify.bat`.

### 14a — The parser (pure, no I/O)

`src-tauri/src/domain/sse.rs`: `SseParser::push(&[u8]) -> Vec<SseBlock>` plus
`finish()` for the last block if the server ends without a blank line.

- `SseBlock { at_ms, kind: Event { name, data, id, retry } | Comment { text }, raw }`.
- Line endings split across chunks, the BOM, `retry` that is not a number
  (ignored, per the spec), a field with no colon (the whole line is the name,
  empty value), and `data` joining are all covered by tests.
- `raw` is what the Raw view shows, so nothing the server sent is lost.

**Tests:** the reference stream word by word, chunk boundaries inside a line
and inside a `\r\n`, comments, multi-line data, a block with only an `id`, a
100 000-line stream for cost.

### 14b — Transport

- `domain/ports.rs`: `HttpStreamUpdate { Headers { status, headers }, Block(SseBlock) }`
  and `HttpClient::send_streaming` with the default that calls `send`.
- `http/curl_client.rs`: the collector parses when the content type says so,
  the progress callback owns the timeout, and the body cap lands.
- `http/cookie_client.rs`: forwards `send_streaming`.
- `domain/services/send_request.rs`: `execute_streaming(id, request, sink)`.

**Tests:** an `httpmock` route that sends an event stream in chunks, a route
that says `text/event-stream` and sends nothing (the parser must not fire), a
non-SSE response (no blocks at all, byte-identical `HttpResponse`), the
timeout test, and a stream cancelled mid-flight.

### 14c — Command and IPC

- `commands/dto.rs`: `HttpStreamEventDto` (`headers`, `block`), mirroring
  `WsEventDto`'s shape.
- `commands/request.rs`: `send_request` takes `on_event: Channel<…>`;
  `send_and_download` is unchanged (a download is not a stream to watch).
- `services/http-client.ts`: creates the channel, batches through
  `lib/event-batcher.ts`, and reports batches to the caller.

**Tests:** the service against a fake channel — batching, order, and that a
plain request reports nothing.

### 14d — Store and UI

- `store/request-store.ts`: `RequestTab.stream { events, droppedCount, streaming, status }`,
  reset on each send, capped through the shared `lib/capped-list.ts`.
- `features/response-viewer/`: the Events view, the Raw view fed by the
  stream, the live status line.
- `lib/sse-log.ts`: the pure helpers the view needs (a row's one-line
  preview, filtering if it earns its place).

**Tests:** the store driving a stream from a fake service, and a component
test of the scenario in section 5 — connect, three events, cancel.

### 14e — Hardening and docs

- Measure the reference stream and a 10 000-event stream: one render per
  frame, memory level at the cap.
- `CLAUDE.md` §4 gains a short paragraph on streaming responses; `PLAN.md`
  gets a Phase 14 section; the project copies are re-synced.
- `verify.bat` and `release.bat` green. The static-link check must still
  report 29 imports: nothing here adds a dependency.

---

## 6b. Built so far — 2026-09-22, `verify.bat` not yet run

### 14a — the parser, as built

`src-tauri/src/domain/sse.rs`, pure and with 17 tests.

- `SseParser::push(bytes) -> Vec<SseBlock>` and `finish()`.
- `SseBlock { kind, raw }`, where `kind` is `Event { name, data, id, retry }`
  or `Comment { text }`. The block is stamped with a time by the transport,
  not here, so the parser has no clock.
- Two deliberate differences from the spec, both written at the top of the
  file:
  - a block with fields but no `data` is still reported, so an `id:`-only or
    `retry:`-only block does not vanish;
  - a last block with no closing blank line is reported when the stream ends.
- `raw` carries the block's own lines with their terminators, so the Raw
  view will be the stream as it arrived.
- Covered: the reference stream, `data` joining, one leading space only, a
  `retry` that is not a number, an `id` with a NUL, a line with no colon,
  comments, blank runs, `\n` / `\r\n` / `\r`, a line split across chunks, a
  `\r\n` split across chunks, a multi-byte character split across chunks, a
  byte-order mark arriving in pieces, and 20 000 events for cost.

### 14b — the transport, as built

- **`domain/ports.rs`**: `HttpStreamUpdate { Headers { status, headers }, Block { at_ms, block } }`,
  the sink type, and `HttpClient::send_streaming` with a default that calls
  `send`. Every existing client, mock and decorator keeps working untouched.
- **`domain/clock.rs`** (new): `now_ms()`, which the WebSocket service now
  uses too rather than keeping its own copy.
- **`http/curl_client.rs`**:
  - The collector parses when the response says `text/event-stream`, and
    reports each block as it is parsed. `finish()` runs after the transfer,
    so a stream cut off mid-block still reports what arrived.
  - Headers are reported the moment the header block ends, per hop, so the
    status shows while the body is still arriving.
  - **The total timeout moved into the progress callback** and is dropped
    once the response turns out to be a stream (D3). `CURLOPT_TIMEOUT` is
    gone. A timed-out request now says "the request timed out after N ms"
    instead of libcurl's wording.
  - A stream's raw body is kept to **8 MiB** for the final response. The
    events themselves are already out by then.
  - A sink that returns false aborts the transfer.
- **`http/cookie_client.rs`**: forwards `send_streaming`, with the jar work
  factored into one `with_cookies` used by both paths.
- **`domain/services/send_request.rs`**: `execute_streaming`, sharing one
  `run` with `execute`, so validation, the in-flight registry and the
  cancelled-not-failed rule are identical. Two tests.
- **`tests/sse.rs`** (new, 8 tests) against a local server written in std
  that writes a stream piece by piece: blocks in order with the comment kept,
  headers first, a stream outliving its timeout, a hanging request still
  timing out, a plain response reporting only headers, a stream cut
  mid-block, a stream that sends nothing, a sink that stops listening, and
  cancel.

**Checked here:** the domain compiles and passes in the offline harness (118
tests), clippy is clean, and rustfmt is applied to every changed file. The
libcurl half needs Windows, so `verify.bat` is the gate.

### 14c — the command and the streaming service, as built

- **`commands/dto.rs`**: `SseBlockDto` (tagged on `kind`: `event` / `comment`,
  both carrying `raw`) and `HttpStreamEventDto` (tagged on `type`: `headers` /
  `block` with `atMs`), with one test pinning the wire shape — the same
  arrangement `WsEventDto` uses, and for the same reason: the block inside
  already uses `kind`.
- **`commands/request.rs`**: `send_request` takes `on_event: Channel<HttpStreamEventDto>`
  and calls `execute_streaming`. A failed `send` on the channel means the
  webview is gone, which ends the transfer rather than letting it run unseen.
  `send_and_download` is untouched, and the command still resolves with the
  whole `HttpResponse`, so history, examples and Copy as cURL are unchanged.
- **`src/types/http.ts`**: `SseBlock` and `HttpStreamEvent` mirror the two
  DTOs.
- **`src/lib/capped-list.ts`** (new): the WebSocket log's keep-the-newest,
  count-what-was-dropped rule, lifted out now that there is a second caller.
  `lib/ws-log.ts` delegates to it and behaves exactly as before.
- **`src/lib/event-batcher.ts`**: gains `flush()`. A WebSocket runs until it
  is closed and never needed one; a request ends, and its last blocks must
  not arrive a frame after the caller has already seen the finished response.
- **`src/services/http-client.ts`**: `sendRequest` creates the channel,
  batches through the event batcher (headers flush at once, a 500-event
  ceiling for a window in the tray) and reports batches to an `onStream`
  callback, flushing in a `finally` so a cancelled stream still delivers what
  arrived. Still the only place `invoke` is called for HTTP. Nothing the
  stream carries is logged: the blocks are the response body.

**Tests:** 6 new service tests — headers without waiting for a frame, blocks
coalesced per frame in order, the ceiling, the last batch arriving before the
promise resolves, a failure still delivering what arrived, and the log
carrying no block data.

### 14d — the store and the Events view, as built

- **`src/lib/sse-log.ts`** (new, pure): `SseLogEntry` with an index that
  counts blocks the caps have already evicted, so the numbering stays
  truthful; `SSE_LOG_CAPS` (1 000 events / 32 MiB, the WebSocket log's
  ceiling); `isEventStream`, `rawText`, `blockLabel`; and `ResponseStream` +
  `applyStreamEvents`, which folds one batch into what the last one left.
- **`store/request-store.ts`**: `RequestTab.stream: ResponseStream`, reset at
  the start of every send and every send-and-download. A new
  `updateRequestTab` patches one tab **by id** from its own previous state —
  a stream arrives in batches, and the user is free to switch tabs while the
  request is still running.
- **`features/response-viewer/EventStreamView.tsx`** (new): Events, Raw and
  Headers, a status pill as soon as the headers land, a Streaming badge while
  it runs, memoised rows (the lesson from 13h), a truncation note past 4 000
  characters and an "N earlier events dropped" note. The list runs **newest
  first**, as the WebSocket log does, and auto-follows the top unless the
  user has scrolled down to older events. The Raw view is not reversed: it is
  the transfer, not the list. Raw is a plain element while the stream
  runs — handing Monaco a document that grows every frame is not what it is
  for — and the editor takes over once the body is final.
- **`features/response-viewer/SaveExampleButton.tsx`** (new): the Save
  response button, now shared by both viewers rather than duplicated.
- **`ResponseViewer`** routes to the stream view whenever the response says
  it is an event stream, *before* the is-sending check: the events are worth
  watching precisely while the request is still running, and afterwards they
  remain the readable form of a body that is one long concatenation.

**Tests:** `lib/sse-log.test.ts` (11), `store/stream-tab.test.ts` (4: blocks
landing while the request is in flight, a stream still filling a tab the user
has left, the next send starting empty, and an ordinary response recording
only its head), `features/response-viewer/EventStreamView.test.tsx` (8).

**Checked here:** `tsc --noEmit`, eslint and prettier are clean on the
device; the pure `sse-log` suite passes in the offline runner. vitest itself
needs the Windows install, so `verify.bat` is the gate.

### 14e — hardening and docs, as built

- **A cost test rather than a benchmark:** 10 000 events folded in batches of
  50 stay at the cap — 1 000 kept, 9 000 counted as dropped, the last one
  still numbered 10 000 — and the fold runs in milliseconds. It guards
  against an accidentally quadratic append, which is the failure mode that
  would actually hurt.
- One store update per animation frame is the batcher's own property, pinned
  in `event-batcher.test.ts` and again in the new service tests, so there was
  nothing new to measure there.
- `CLAUDE.md`: §1 says a `text/event-stream` response is shown as it arrives,
  §3 lists `domain/sse.rs` and `domain/clock.rs`, and §4 gains a *Streaming
  responses* subsection next to the WebSocket one.
- `PLAN.md` gains a Phase 14 section. The project copies of all three
  documents are re-synced.
- **Clickthrough, 2026-09-22:** the Events list was built oldest-first, which
  is how a stream reads but not how this app shows a log. Corrected to newest
  first, matching the WebSocket log, with the auto-follow and the dropped-
  events note turned around to match.
- **Clickthrough, 2026-09-22 (second round):** the status line now carries a
  running total of what has arrived, beside the event count, formatted by the
  existing `formatBytes` (bytes under 1 KB, then KB, then MB). The count is
  the sum of each block's own bytes, and it counts blocks the caps have
  dropped — it is what the server sent, not what the viewer still holds. Two
  consequences worth knowing: bytes sitting in the parser as an unfinished
  block are not counted until the block completes, and the total is the
  events, not the whole transfer (headers are not in it).
  - **The byte count is measured in Rust**, on `SseBlock::raw`, and crosses as
    `bytes` on the block event. Measuring it in the UI would have used
    `String.length`, which counts UTF-16 units and undercounts every
    multi-byte character a stream carries. The viewer's cap now weighs the
    same number, so it is exact too.
- **`Transfer-Encoding: chunked` does not show here — not a bug.**
  Nothing filters headers; `curl_client.rs` keeps every line of the final
  hop. This app's default HTTP version is `auto`, which offers h2 over TLS,
  and **HTTP/2 has no `Transfer-Encoding`** — framing is the protocol's own.
  Setting the request's HTTP
  version to HTTP/1.1 in Settings brings the header back, which is also how
  to confirm it.
- **Left for the in-app clickthrough:** scrolling and typing while
  `https://sse.tools.typinks.com/api/story` is running.

### 14f — the size badge and its breakdown, as built

Asked for after the second clickthrough: a total size beside the event count,
and a hover card breaking it into response and request, headers and body. 
Two decisions were put to the user first.

- **Where the numbers come from.** The request half is libcurl's
  (`request_size`, `upload_size` via getinfo, both `u64` in curl 0.4.50) — it
  is the only side that knows what it actually sent, including the headers it
  adds itself. The response half is counted in the collector: `header_bytes`
  per hop in the header callback, `body_bytes` in the write callback, counted
  even past the 8 MiB stream cap. Counting the response half ourselves is
  what makes the running total and the final total the same number, which
  `tests/sse.rs` now pins.
- **What is known when.** getinfo has nothing to say until the transfer ends,
  and a stream may stay open for minutes, so `SizeBreakdown` types the
  request half as `number | null` and the card shows an em dash rather than a
  zero, which would read as "it sent nothing". *(Decided with the user: the
  alternative was counting the outgoing bytes in libcurl's debug callback,
  which would need `CURLOPT_VERBOSE` on for every request.)*
- **Both viewers** (also decided with the user): the numbers ride on
  `HttpResponse`, and the ordinary response bar had no size display at all,
  so `SizeBadge` is shared. `SaveExampleButton` set the precedent.
- `TransferSizes` (domain) → `TransferSizesDto` → `src/types/http.ts`, and the
  `Headers` stream update gained `bytes` so the live total has both halves.
- The hover card is hand-rolled: `components/ui/` is empty, this project has
  no popover primitive, and a panel that needs no state, no portal and no
  outside-click handling does not justify a dependency. It shows on hover and
  on keyboard focus (`group-hover` / `group-focus-within`).
- A gzip response counts **decoded**, because `accept_encoding("")` means
  libcurl decompresses it and the decoded bytes are what the viewer holds and
  what Save Response writes.
- Tests: `lib/transfer-sizes.test.ts` (3), one Rust integration test that the
  running size and the finished size agree, the dto wire-shape test extended,
  and three component tests — the badge total, the breakdown with the request
  half unknown, and every number present once the response lands.

### Verify runs

- **2026-09-22, first run:** everything green except one assertion in
  `http-client.test.ts` that still expected `invoke` to be called with two
  arguments — `send_request` now also gets the channel. Rust 503 unit tests,
  the new `sse` binary at 8, `cargo fmt` and clippy clean, 430 of 431
  frontend tests. Fixed by matching the two arguments the test is actually
  about.
- **2026-09-23, with 14f:** the whole frontend green — 440 tests in 47 files,
  `tsc`, eslint and `cargo fmt` clean. Rust would not compile: **libcurl's
  `upload_size()` returns `f64`, not `u64`**, though `request_size()` and
  `header_size()` do return `u64`. The docs page for curl 0.4.50 was read
  before writing the call and reported all four as `u64`; only the build
  settles it. Fixed with `easy.upload_size()?.max(0.0) as u64`, where the
  `max` also folds away a NaN.
- **2026-09-23, after the `f64` fix:** everything green except the new
  integration test, which did its job — `the_running_size_and_the_finished_one_agree`
  measured 69 bytes of blocks against 73 bytes received. **The parser was
  dropping the blank line that ends each block**, one byte per block, so
  `raw` never added back up to the stream. The header of `domain/sse.rs` had
  claimed the opposite since 14a. Two consequences were live: the running
  size read low, and the Raw view of a stream in flight showed its events
  glued together with no blank lines between them.
  - Fixed in `domain/sse.rs`: the blank line that ends a block is now part of
    that block's `raw`, and a blank line that ends nothing — the gap after a
    comment, or a run between blocks — is carried onto the front of the next
    block, since it cannot be given to one already reported. The one byte
    still unattributed is a blank at the very end of a stream, which has no
    next block; that is written down at the top of the file.
  - The test that would have caught it is now in `sse.rs`:
    `concatenating_every_raw_gives_the_stream_back`. Six `raw` assertions in
    the existing unit tests gained the blank line they had been missing.
- **2026-09-23, green.** 504 Rust unit tests, 14 curl, 69 repository, **9
  SSE**, 19 WebSocket, 440 vitest in 47 files; `cargo fmt`, clippy, `tsc` and
  eslint all clean.
- **`release.bat`, 2026-09-23, green.** NSIS and MSI both built, and the
  static-link check still reports **29 imported DLLs, all Windows system
  DLLs** — SSE added none, as expected. Phase 14 is done.

---

## 7. Risks

| Risk | Retired by |
|---|---|
| Moving the timeout out of libcurl changes behaviour for ordinary requests | 14b, against the existing timeout test plus a new one that a slow *stream* is not cut off |
| A chunk splits a line or a `\r\n` | 14a, tested at every boundary |
| The channel floods the UI on a fast stream | The batcher and caps from Phase 13, reused rather than rewritten |
| `send_request`'s signature changes and something else calls it | Only `services/http-client.ts` may call it (CLAUDE.md §11 rule 3), so the change is one file plus the command |
| A server sends `text/event-stream` for a finite body | Nothing breaks: the Events view shows what came, and the final response is still there |

---

## 8. What is deliberately not in this phase

- Automatic reconnect with `Last-Event-ID`.
- Saving a stream as an example, or recording every event in history. History
  records the request as it does today.
- A dedicated SSE request type in the sidebar. An SSE request is an HTTP
  request; it is the response that differs.
- Streaming for `send_and_download`.

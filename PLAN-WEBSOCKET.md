# PLAN-WEBSOCKET.md — Phase 13: WebSocket requests

Planned 2026-09-21, decisions taken the same day (section 3). 13a done 2026-09-21: D1 confirmed. 13d done and verified 2026-09-21. 13b done and verified 2026-09-21. 13c written 2026-09-22. `verify.bat` was green on the second run. The smoke test found a bug, now fixed. `verify.bat` run 3 and the second smoke test both passed. **13c done 2026-09-22.** **13e done and verified 2026-09-22.** **13f and the two 2026-09-22 decisions verified: `verify.bat` green on the first run.** The manual click-through is pending. The one-Clear change is verified. 13g verified 2026-09-22, and the manual click-through was done by you. **13h done 2026-09-22: `verify.bat` and `release.bat` green. Phase 13 is complete.** **13i (more message formats) verified 2026-09-22.**

Ready to paste into `PLAN.md` as **Phase 13**. `PLAN.md` itself was not touched: it has uncommitted changes of yours.

Sources: the local `PLAN.md` and `CLAUDE.md` (both newer than the copies in the Claude Project), the source tree, the built libcurl in `src-tauri/target/`, your feature specification.

---

## 1. Where the repo stands

Everything numbered is done except Phase 10.

| Phase | State |
|---|---|
| 0–7 | Done. Phase 7's clean-machine install is still outstanding. |
| 8a–8e | OpenAPI export (3.0/3.1/3.2, JSON/YAML) and import, done. |
| 9 | Secrets sealed at rest with a key in Credential Manager, done. |
| 10 | Microsoft Store, planned, waiting on the Partner Center account. |
| 11 | Close-to-tray toast, done. |
| 12 | Item-level Markdown docs, done. Last verified 2026-09-21: 411 + 14 + 59 Rust tests, 295 Vitest, clippy and fmt clean, `release.bat` green, 29 imported DLLs. |

The app is now called **ResponderHTTP**. Migrations run through `0009_item_docs.sql`. Only two commits exist in git and `PLAN.md` is modified but uncommitted.

**Nothing WebSocket-shaped exists.** A search of `src/` and `src-tauri/src/` finds only the `Sec-WebSocket-*` names in the header autocomplete lists.

**Baseline to keep green:** every count above must still pass after every sub-phase. `verify.bat` on Windows is the only real gate (PLAN.md, Phase 5 note).

---

## 2. Findings that change the plan

**F1. There is no native export format.** The spec's export table and its fourth Gherkin scenario ("Export as [Native Format]") assume one. The only export is `export_collection_openapi`, and PLAN.md Phase 12 says outright that "OpenAPI is this app's only export format". Acceptance scenario 4 therefore needs a new feature, not a filter. **Resolved (D2):** no WebSocket import or export in this phase; it comes later with AsyncAPI. Scenario 4 is deferred with it.

**F2. The `curl` crate cannot do WebSocket. The libcurl inside your binary can.** Checked three ways against the exact build in `src-tauri/target/debug/`:

- `curl-sys 0.4.90+curl-8.21.0`'s compiled metadata contains no `curl_ws_send` or `curl_ws_recv`. The same grep finds `curl_easy_send`, `curl_easy_recv` and `curl_multi_wait`, so the check is sound.
- The statically built `libcurl.a` does contain both symbols, and `include/curl/websockets.h` is present.
- `Easy2::connect_only(bool)` cannot express the `2` that libcurl's WebSocket mode requires. It has to be set through `curl_sys::curl_easy_setopt`.

The exact signatures from the built header, which the FFI would mirror:

```c
CURLcode curl_ws_recv(CURL *curl, void *buffer, size_t buflen, size_t *recv,
                      const struct curl_ws_frame **metap);
CURLcode curl_ws_send(CURL *curl, const void *buffer_arg, size_t buflen,
                      size_t *sent, curl_off_t fragsize, unsigned int flags);
struct curl_ws_frame { int age; int flags; curl_off_t offset;
                       curl_off_t bytesleft; size_t len; };
/* CURLWS_TEXT 1<<0, BINARY 1<<1, CONT 1<<2, CLOSE 1<<3, PING 1<<4, OFFSET 1<<5, PONG 1<<6
   CURLOPT_WS_OPTIONS = 320, CURLWS_NOAUTOPONG = 1<<1, CURLE_AGAIN = 81 */
```

The version string in the built header is `8.21.0-DEV`, a snapshot bundled by `curl-sys`. `Cargo.lock` pins it, but a `curl-sys` bump can change it, so the spike (13a) records exactly what was tested.

**F3. The app has no streaming IPC pattern.** No `Channel`, no `emit`, no `listen` anywhere. Every command today is request/response. A WebSocket pushes events for as long as it lives. This is a new pattern, which your instructions say to ask about (D3).

**F4. Everything that touches a saved request assumes HTTP.** `SavedRequest.request` is an `HttpRequest`. The mentions: 11 Rust files, 13 TypeScript files. The `requests` table has `method NOT NULL`, and the `SELECT` in `saved_requests.rs` decodes every row as HTTP. A WebSocket row that reaches those paths would load as a `GET` to a `wss://` URL, and the OpenAPI exporter would report it as an "unmappable URL". The plan below keeps WebSocket rows out of every HTTP path by construction (13d).

**F5. This would be the first `unsafe` in the codebase.** A search of `src-tauri/src` finds none, and there is no `[lints]` table or `deny(unsafe_code)`. It is contained to one file (13b). Separately, `Database::migrate` still returns Ok when a database is newer than the binary, an item already open in PLAN.md Phase 7. Migration `0010` makes that gap matter: an older build would list WebSocket rows as broken HTTP requests. Fix it in 13d.

**F6. The Claude Project's copies of `PLAN.md` and `CLAUDE.md` are stale.** They date from 2026-09-15 and predate Phases 9 to 12, the rename and the CLAUDE.md §5 change on secrets. Re-upload them, or this project's assistant keeps reasoning from a plan that stops at Phase 8.

---

## 3. Decisions (taken 2026-09-21)

| # | Question | Decision |
|---|---|---|
| **D1** | Which WebSocket engine? | **libcurl's WS API through hand-written FFI, contained in one file.** Confirmed by the 13a spike on Windows, 2026-09-21: all gates passed. Rationale: CLAUDE.md §4 makes libcurl the one transport, so this keeps one TLS stack, one proxy path, one CA setup and one verify-TLS toggle, at the cost of the codebase's first `unsafe`. |
| **D2** | WebSocket export and import? | **Neither, in this phase.** Both come later with AsyncAPI. OpenAPI export keeps working on mixed collections and leaves WebSockets out (13g). There is no native export to build. |
| **D3** | How do events reach the UI? | **A per-connection Tauri `ipc::Channel<WsEvent>` passed into `ws_connect`.** Keeps everything behind `invoke()` in `services/` (rule 3), one ordered stream per connection. Exact Tauri 2.11 signatures to be checked when building. |
| **D4** | The Docs sub-tab | **Rendered preview plus an "Edit documentation" button that calls the existing `openDocs`.** No second editing path. |
| **D5** | Credentials in exports | **Moot** — no WebSocket export this phase. |
| **D6** | The "…" menu next to the badge | ~~One item, "Clear Response"~~ **Superseded 2026-09-22: no "…" menu and no Clear Response. Clear Messages clears everything from every connection** (section 5). |

### Assumptions (confirmed 2026-09-21)

1. **URL is a plain input** that must start with `ws://` or `wss://`. Your screenshots show no scheme selector, so I did not add one.
2. **No Auth tab.** The screenshots have none. Credentials go in Headers, ideally as a `{{secret variable}}`, so Phase 9's substitution rules apply.
3. **No Share button.** The app has no share feature, and there is no "…" menu (D6, as revised).
4. **History does not record WebSocket sessions**, and saved responses (examples) do not apply to them.
5. **The message log is in memory, per tab.** It is lost when the tab closes or the app exits, like open tabs today.
6. **Log retention is capped** at 1,000 entries and 32 MiB of retained payload, evicting the oldest (named constants). Without a cap, a chatty server at 1 MiB per message could take a gigabyte.
7. **Max message size** defaults to 1 MiB, the same figure as the existing example cap. Exceeding it closes the connection with code 1009.
8. **Auto-reconnect** is off by default. It applies only after an unexpected drop, never after the user's own Disconnect, and never after a rejected handshake (a 401 must not be hammered). Backoff is 1 s doubling to 30 s, at most 10 attempts, each as a system entry in the log.
9. **JSON format** sends whatever is typed. An invalid document shows a non-blocking warning. **Hex format** blocks Send on a parse error and names the offending byte.
10. **URL and Params lock while connecting or connected**, since Params is a view of the URL. Headers, Settings and the draft stay editable, with a note that changes apply on the next connect.
11. **Closing a connected tab asks first**, using the existing `ConfirmDialog`.
12. **No HTTP/2 WebSockets** (RFC 8441), **no `permessage-deflate`**, and no subprotocol picker. The handshake is forced to HTTP/1.1, and a `Sec-WebSocket-Protocol` header can still be typed.

---

## 4. Scope

| Spec requirement | Section |
|---|---|
| URL, `ws://`/`wss://`, Connect/Disconnect | 13b, 13f |
| Sub-tabs: Docs, Message, Params, Headers, Settings, Cookies | 13f (Params, Headers and Settings reuse existing components) |
| Composer: syntax highlighting, Text/JSON/Hex, Send only while connected | 13e, 13f |
| Live status badge | 13e, 13f |
| Activity log: direction, expandable payload, timestamp | 13c, 13e, 13f |
| Filter dropdown, search, Clear Messages | 13e, 13f |
| Save into any collection or folder, no protocol restriction | 13d, 13f |
| OpenAPI exports exclude WebSockets | 13g |
| Native export includes them | **Deferred** to the AsyncAPI phase (D2) |
| Cookie inheritance for handshakes | 13b |
| Docs integration | 13f (D4) |

**Out of scope**, from the spec: AsyncAPI, load testing, pre-request scripts and message mutation.
**Out of scope**, added here: any WebSocket import or export (native or AsyncAPI; D2), history entries, examples, an Auth tab, `permessage-deflate`, HTTP/2 WebSocket, and macOS or Linux. Windows is still the only target (PLAN.md Phase 0).

**Sequencing:** 13a → 13b → 13c → 13e → 13f → 13g → 13h. **13d is independent of 13b and 13c** and can run beside them, but must finish before 13f.

---

## 5. UI skeleton, from the screenshots

```
┌ TabBar ───────────────────────────────────────────────────────────┐
│ [⚡ Websocket example •] [+]                      [Environment ▾]  │
├ WebSocketBuilder ─────────────────────────────────────────────────┤
│ Path breadcrumb (existing)                 [Cookies] [Save]       │
│ [ wss://echo.websocket.org                     ] [ Connect ]      │
│ Docs | Message | Params | Headers | Settings            Cookies   │
│ ┌ panel ──────────────────────────────────────────────────────┐   │
│ │ Message: Monaco, line numbers, placeholder "Compose message"│   │
│ │ [Text ▾]                                            [ Send ]│   │
│ └─────────────────────────────────────────────────────────────┘   │
├ ResizeHandle (existing) ──────────────────────────────────────────┤
│ WebSocketLog   Response                    [Connected] | […]      │
│ [Search…] [All Messages ▾] [🗑 Clear Messages]                    │
│ ↓ hello                                    12:18:32.896  ⌄        │
│ ↑ hello                                    12:18:32.849  ⌄        │
│ ↓ Request served by 4d896d95b55478         12:18:21.505  ⌄        │
│ ✓ Connected to wss://echo.websocket.org    12:18:21.503  ⌄        │
└───────────────────────────────────────────────────────────────────┘
```

**State matrix**, read directly off the two screenshots plus the transitional states they imply:

| State | Primary button | URL input | Send | Badge |
|---|---|---|---|---|
| Disconnected | **Connect** (primary blue) | editable | disabled | Disconnected (destructive token) |
| Connecting | Cancel (outlined destructive, as the HTTP Cancel) | locked | disabled | Connecting… (neutral) |
| Connected | **Disconnect** (neutral grey) | locked and dimmed | enabled | Connected (green) |
| Reconnecting | Cancel | locked | disabled | Reconnecting… n/N |
| Disconnecting | disabled | locked | disabled | Disconnecting… |

**Log anatomy.** Newest first, as the screenshots show.

- Each row is icon, single-line mono payload, time, chevron.
- Sent is an amber up-arrow and received a blue down-arrow. System rows use a green check for connected and a grey info icon for closed, errors and reconnects.
- The time is `HH:mm:ss.SSS` local. It is stamped in Rust when the frame is written or read, so a slow render cannot reorder or distort it.
- An expanded row shows the full payload in read-only Monaco (JSON auto-pretty-printed through `lib/pretty-print.ts`), its byte length and a Copy button.
- A binary payload shows a hex dump.
- The expanded "Connected" row shows the 101 status and the handshake response headers, which are free because libcurl already gives them to the header callback.

**Clear Messages (revised 2026-09-22, replacing D6 as confirmed 2026-09-21).** Clear Messages empties **everything from every connection**, including the handshake details of each "Connected" row and the "N earlier entries dropped" note. There is no Clear Response and no "…" menu beside the badge. Each entry still carries the `connectionId` of the connect that produced it, which the store uses to tell a late event from an earlier connection apart from the current one.
- It does not touch the connection itself, the draft, the search text or the filter. Clearing while connected keeps the socket open, and new frames keep arriving into an empty list.
- It does not ask for confirmation: the log is in-memory scratch (assumption 5), not stored data.

**Colours.** `CLAUDE.md` §6 forbids ad-hoc hex. Use the existing tokens where they exist (`destructive`, the `--frost-*` pair). Add only what is missing (`--ws-sent`, `--ws-received`, `--ws-ok`) to `src/index.css` and `tailwind.config.ts`, mapped to Nord values, in the same way the `--method-*` tokens were added.

**Icon.** Lucide `Zap` stands in for the ⚡ in your spec, in the tab bar and in the sidebar tree in place of the method label. It goes green while connected. The tab bar and tree never show a method for a WebSocket.

---

## 6. Architecture and contracts

```
React   features/websocket/*  ── props in, events out
        store/request-store.ts (WebSocketTab)  store/ws-log? (see 13e)
        services/websocket.ts   ── the ONLY invoke() + Channel site
        lib/ws-payload.ts  lib/ws-log.ts        ── pure, tested
Tauri   commands/websocket.rs   ── thin: parse, delegate, map
Domain  domain/services/websocket.rs  WebSocketSessions (registry, owner loop, reconnect policy)
        domain/ports.rs   WebSocketConnector / WebSocketConnection  (mockable)
        domain/ws_frames.rs   frame reassembly, pure
Infra   http/curl_websocket.rs      connector over libcurl
        http/curl_ws_ffi.rs         the only `unsafe`
```

**The port earns its place.** CLAUDE.md §7 says a trait with one implementation and no test seam is speculative. This one has a seam: `WebSocketSessions` (registry, owner loop, reconnect policy, message-size enforcement) is tested against a scripted mock connection with no network, exactly as `SendRequest` is against `HttpClient`.

**One owner thread per connection.** A libcurl easy handle is not thread-safe. The thread owns the handle. On each turn it drains a command channel (send, close), polls `curl_ws_recv`, and emits events. It is a plain `std::thread`, not `spawn_blocking`, because it lives as long as the connection.

**Commands** (all `#[tauri::command]`, all thin):

```rust
// src-tauri/src/commands/websocket.rs — shape, not final
ws_connect(connection_id: String, request: WebSocketRequestInput,
           on_event: Channel<WsEventDto>) -> Result<(), ApiError>
ws_send(connection_id: String, payload: WsOutgoingDto) -> Result<(), ApiError>
ws_disconnect(connection_id: String)            // returns at once; `closed` follows
save_web_socket / load_web_socket               // 13d
```

`ws_connect` returns after the handshake. Success emits `connected` on the channel and returns Ok. Failure returns Err and emits nothing. `ws_disconnect` also cancels a handshake in progress, the same registry-plus-`CancellationToken` shape as `cancel_request`. A failed `channel.send` means the UI is gone (a dev reload, say), so the thread closes the connection rather than leaking it.

**Events**, mirrored in `src/types/websocket.ts`:

```ts
// src/types/websocket.ts
export type WsPayload =
  | { kind: "text"; text: string; byteLength: number }
  | { kind: "binary"; base64: string; byteLength: number };

export type WsEvent =
  | { type: "connected"; at: number; url: string; status: number; headers: KeyValue[] }
  | { type: "sent"; at: number; payload: WsPayload }
  | { type: "received"; at: number; payload: WsPayload }
  | { type: "closed"; at: number; code: number | null; reason: string;
      by: "user" | "server" | "error" }
  | { type: "error"; at: number; message: string }
  | { type: "reconnecting"; at: number; attempt: number; maxAttempts: number; delayMs: number };
```

- `at` is epoch milliseconds, which JavaScript holds exactly.
- The `sent` event is emitted by the owner thread after the frame is actually written. That keeps "sent, then echo" in the right order even though the echo can arrive before the `ws_send` promise resolves.
- Binary crosses IPC as base64 (the `base64` crate is already a dependency). The UI renders the hex dump from it.

**Errors.** No new `ApiError` kind. Sending while not connected is `invalidRequest`. A new kind would ripple through `commands/error.rs`, `lib/api-error.ts` and every exhaustive consumer for no gain.

---

## 7. The sub-phases

### 13a — Spike: prove libcurl WebSocket from Rust (throwaway, `spikes/ws-libcurl/`)

The same discipline as the Phase 0 static-link proof: the single riskiest assumption, proved before anything is built on it. **It has to run on your machine.** It needs the real toolchain and a real network, and the offline harness cannot link libcurl.

**Goal.** A throwaway binary with the app's exact `curl` line and release profile, plus `curl-sys`.

**Steps**

1. `Easy2` handle, `http_version(V11)`, `CURLOPT_CONNECT_ONLY = 2` through `curl_sys::curl_easy_setopt`, the native CA option, then `perform()`.
2. Read the status and headers of the 101. Then send text and binary, and receive echoes.
3. Declare `curl_ws_send` and `curl_ws_recv` by hand from the header in section 2.
4. Handle `CURLE_AGAIN` by polling. Measure two options: a 10 ms sleep loop, and waiting on `CURLINFO_ACTIVESOCKET`. Record echo latency and idle CPU for each.
5. Exercise each of these cases and write down what libcurl actually does:
   - a fragmented message (reassembly through `bytesleft`);
   - a server PING (auto-pong, and whether the app sees it);
   - a server-initiated CLOSE (do we have to send the close reply ourselves?);
   - a client-initiated CLOSE;
   - a handshake refused with HTTP 401 and 403 (which `CURLcode`, and is the status still readable?);
   - an unreachable host;
   - a TLS failure on `wss://`;
   - a proxy;
   - `CURLOPT_TIMEOUT_MS` after `perform()` has returned (does it kill an idle connection?).
6. Run `spikes/static-link-proof/check-windows.ps1` against the spike exe. The import list must not change.

**Local server for the spike and later tests.** A hand-rolled echo server needs SHA-1 and base64. `aws-lc-rs` (SHA-1 is exposed as a legacy digest) and `base64` are already dependencies, so no dev-dependency is required. Verify the digest constant name when writing it.

**Done when** every case in step 5 has a recorded outcome, the import list is unchanged, and D1 is confirmed or reversed. If the FFI path fails a gate, 13b is redrawn around `tungstenite` and this plan is reissued with the new dependency list.

#### 13a results — cloud run, 2026-09-21

Built: `spikes/ws-libcurl/` (std-only local test server, hand-declared `curl_ws_*` FFI, 18 scenarios, `run-spike.bat`). The same source builds two ways: with cargo against the app's exact `curl`/`curl-sys` line and a copy of `src-tauri/Cargo.lock` (the build whose verdict counts), and with plain `rustc --cfg raw_libcurl` against a self-built libcurl, which is what could run here. The cloud has no crates.io access, so the cloud run used **curl 8.21.0 built from the git tag without TLS**. The app links curl-sys's `8.21.0-DEV` snapshot with rustls.

**Every gate passed in the cloud build** (`cloud-report-libcurl-8.21.0.txt`). The Windows cargo run is recorded below.

Also confirmed on the app's own build artefacts: its `libcurl.a` contains `ws.o` and the handshake string `Sec-WebSocket-Key`, so it is the real implementation, not the `CURLE_NOT_BUILT_IN` stub a `CURL_DISABLE_WEBSOCKETS` build would have.

What libcurl actually does, and what it changes in 13b:

| # | Finding | Consequence for 13b |
|---|---|---|
| 1 | **Automatic pong is lazy.** libcurl queues the pong and flushes it only when `curl_ws_recv` next returns a data frame, or on the next `curl_ws_send` (`lib/ws.c`). A connection that is only listening never answers a PING, and the test server gave up after 3 s. | Set `CURLOPT_WS_OPTIONS = CURLWS_NOAUTOPONG`. PINGs then reach the owner loop (proved in S06b), which answers with `CURLWS_PONG` and the same payload at once. PINGs and PONGs are not shown in the log. |
| 2 | **A server close is not answered by libcurl.** `recv` returns a CLOSE message with the code and reason. If the app sends nothing, the server never gets a close reply. | The owner loop sends the close reply itself, then treats the next `CURLE_GOT_NOTHING` (52) as the end. The `closed` event carries the server's code and reason. |
| 3 | **A refused upgrade is `CURLE_HTTP_RETURNED_ERROR` (22).** The error buffer says `Refused WebSocket upgrade: 401`, and `CURLINFO_RESPONSE_CODE` still reads 401, 403 or 200. | The failure names the status: `server refused the upgrade: HTTP 401`. Read it from the status, not by parsing the error buffer. Never retry (assumption 8). |
| 4 | **`CURLOPT_TIMEOUT_MS` does not limit an open WebSocket.** After 2.5 s idle on a 1.5 s timeout, send and echo still worked. | `connect_timeout` maps to `CURLOPT_CONNECTTIMEOUT_MS` only. Do not set `CURLOPT_TIMEOUT_MS`; it adds nothing, and its behaviour could change between libcurl versions. |
| 5 | **Waiting on the socket beats sleeping, by a lot.** Echo median 47 µs (p95 77 µs) with `poll` on `CURLINFO_ACTIVESOCKET`, against 10.3 ms with a 10 ms sleep loop. Two idle seconds cost 1 wake-up against 197. | The owner loop drains `curl_ws_recv` until `CURLE_AGAIN`, then polls the socket. The poll needs a timeout so the loop can pick up send and close commands from the UI: a named constant around 25 ms, which bounds send latency and still leaves idle cost negligible. |
| 6 | **Reassembly works as documented.** Fragments arrive with `TEXT\|CONT`, then a final `TEXT`. A 200,000-byte frame came through a 64 KiB buffer in 4 chunks, with `bytesleft` counting down to 0. A message is complete when `bytesleft == 0` and there is no `CONT`. | This is the rule `domain/ws_frames.rs` implements. The max-message-size check runs as chunks accumulate, not after. |
| 7 | **Partial sends are real.** 8 MiB to a reader stalled for 1.5 s took 7 calls: 3 returned `CURLE_OK` with a short `sent` count, and 3 returned `CURLE_AGAIN`. The server received every byte, hash confirmed. | The send loop follows the contract exactly: after a short `OK`, continue from the rest; after `AGAIN`, wait until the socket is writable and repeat the same buffer. No other send may start in between. |
| 8 | **A dropped TCP connection is `CURLE_GOT_NOTHING` (52)** on the next `recv`, the same code as after a clean close. The first `send` after the drop still succeeds, which is normal for TCP. | Unexpected drop versus clean close is decided by whether a CLOSE was seen first. Only an unexpected drop can trigger auto-reconnect. |
| 9 | Handshake extras reach the server: a custom header, a `Cookie` header and `Sec-WebSocket-Protocol`. The subprotocol comes back in the 101 headers. Unreachable host is curl 7, and an unresolvable name is curl 6. | No change. The jar's `Cookie` header goes in the same header list (13b step 8). |

#### 13a results — Windows run, 2026-09-21: **D1 confirmed, libcurl FFI stays**

Run with `run-spike.bat` on the development machine. Build: `libcurl/8.21.0-DEV rustls-ffi/0.15.3/rustls/0.23.39/aws-lc-rs zlib/1.3.2 nghttp2/1.68.1`, i.e. the app's exact libcurl. The build took 47.8 s. **All 18 gated scenarios passed, 0 failures.** Everything found in the cloud run reproduced unchanged on Windows: findings 1–9 above hold. The five scenarios only this build could run:

| # | Scenario | Result |
|---|---|---|
| S15 | Handshake through the curl crate's `Easy2`, frames over `easy.raw()` | Pass. This is the exact integration 13b uses. |
| S16 | `wss://echo.websocket.org` with the Windows certificate store (`CURLSSLOPT_NATIVE_CA`) | Pass. The server sends `Request served by …` first, as in your screenshots, then the echo. |
| S17 | Expired and self-signed certificates | Both refused with curl 60 and a rustls reason (`Expired`, `UnknownIssuer`), which is readable enough to show the user. |
| S17 | Verification off | Got past TLS to the HTTP layer, so the per-request opt-in works on the WebSocket path. |
| S18 | Proxy | Not run: no proxy available. The option is the same `CURLOPT_PROXY` the HTTP path uses. Covered by the manual click-through. |

**The static-link check failed on `VCRUNTIME140.dll`, and that is an artefact of the spike, not of WebSocket support.** The app's release exe does not import it (29 imports, `release.bat`, 2026-09-21), because `tauri-build` links the VC runtime statically and the UCRT dynamically (`tauri-build/src/static_vcruntime.rs`: `/NODEFAULTLIB:vcruntime.lib`, `/DEFAULTLIB:libvcruntime.lib`, `/DEFAULTLIB:ucrt.lib`). The spike is a plain cargo binary without `tauri-build`, so it gets Rust's default dynamic `vcruntime140.dll`. This corrects PLAN.md Phase 7's reading that "tauri-build does NOT pass +crt-static", which was true but incomplete: it does not make the whole CRT static, but it does make the VC runtime static.

**The question the check was actually asking is answered:** apart from the C runtime, the spike imports exactly the eight DLLs the Phase 0 static-link proof imported (advapi32, api-ms-win-core-synch, bcryptprimitives, crypt32, IPHLPAPI, kernel32, ntdll, ws2_32). WebSocket support adds no import. The final proof remains 13h: `release.bat` on the real app must still report 29.

**One observation that does not block anything:** on Windows, the socket wait answered data within 56 µs (p95 65 µs), but two idle seconds cost 196 wake-ups, about the same as sleeping, against 1 on Linux. WSAPoll appears to return roughly every 10 ms while idle. It does not matter for 13b, because the owner loop polls with a short timeout anyway (finding 5) to pick up UI commands, which sets its own idle rate. Worth a `revents` check if idle CPU ever shows up in a profile.

**13a is done.** 13b proceeds as planned, with the design changes from findings 1–8.



---

### 13b — Domain and transport

**Steps, in dependency order**

1. `Cargo.toml`: add `curl-sys` (same version as the lock, no features beyond what `curl` already enables). In `lib.rs`, add `#![deny(unsafe_code)]` and allow it only on `http/curl_ws_ffi.rs`, so containment is enforced, not conventional.
2. `domain/models.rs`: `WebSocketRequest { url, headers, settings }`, `WebSocketSettings { verify_tls, proxy, send_cookies, connect_timeout, max_message_bytes, auto_reconnect }`, `WsPayload`, `WsEvent`, `CloseInfo`. Defaults as named constants. The domain uses no serde, as elsewhere.
3. `domain/ports.rs`: `WebSocketConnector::connect(&WebSocketRequest, &CancellationToken) -> Result<Box<dyn WebSocketConnection>, AppError>`. `WebSocketConnection` has `send`, `poll(timeout) -> Option<Frame>` and `close`.
4. `domain/ws_frames.rs`, pure: `Reassembler::push(flags, offset, bytes_left, chunk)` returns complete messages, enforces `max_message_bytes` (an oversize message becomes a 1009 close) and parses a close payload (two-byte code plus UTF-8 reason, an empty payload meaning no code).
5. `http/curl_ws_ffi.rs`: the `unsafe` block, about 80 lines. Every `unsafe` carries a `SAFETY:` comment. It exposes a small safe wrapper: `send(flags, &[u8])`, `recv(&mut [u8]) -> Recv`, `set_ws_mode()`.
6. Extract the shared TLS and proxy setup from `curl_client.rs::apply_settings` into a helper used by both clients. `verify_tls` off remains an explicit per-request opt-in, with the same visible warning (rule 7). **This edits shipping code** and is covered by the existing 14 curl integration tests, which must stay green.
7. `http/curl_websocket.rs`: `CurlWebSocketConnector`. It forces HTTP/1.1 and reuses `build_url` and `encode_url_for_send`. It maps a refused upgrade to `Transport("server refused the upgrade: HTTP 401")` if 13a shows the status is readable. The message carries no header or cookie values.
8. **Cookies for the handshake.** `CookieClient` is a decorator over `HttpClient`, and the WebSocket connector does not implement that trait. Extract its two halves, `cookie_header_for` and `store_from`, into a small `CookieSession` helper in `http/`. `CookieClient` and the WebSocket path both call it. `set_cookies` from the 101 response are captured the same way. The user's own `Cookie` header still wins, and `send_cookies: false` still skips the jar. `CookieClient`'s existing tests (six today) stay untouched.
9. `domain/services/websocket.rs`: `WebSocketSessions`. It registers a connection by id, validates the scheme (`ws://`/`wss://`) and the proxy, and runs the owner loop. It applies the reconnect policy as a pure `next_delay(attempt, &settings) -> Option<Duration>`, and sends events to an injected sink.

**Tests**

- `ws_frames`: table tests for a single frame, a fragmented text message, a ping between fragments, the exact-limit and over-limit cases, a close with code and reason, and a close with no payload.
- `WebSocketSessions` against a scripted mock connection: connect, send, receive, user disconnect, server close, transport error, an unexpected drop with reconnect on and off, and a rejected handshake that never retries.
- `next_delay`: the doubling, the 30 s cap, and the attempt limit.
- `tests/websocket.rs`: integration against the local echo server from 13a. Cover text and binary echo, a fragmented message, a server close with code, ping and pong, an oversize message closing with 1009, an unreachable host, a non-101 response, cancel during connect, a custom header arriving on the handshake, and a jar cookie arriving on the handshake.
- The log line for a connect names the URL with redaction (`logging.rs` already redacts query credentials) and never a payload, header or cookie.

**Done when** the mock-driven and integration suites pass, clippy is clean with `unsafe_code` denied outside the one module, and the existing 14 curl integration tests and the cookie-decorator tests still pass.

#### 13b as built — 2026-09-21, **verified: `verify.bat` green on the first run**

**Files.** New: `domain/ws_frames.rs`, `domain/services/websocket.rs`, `http/curl_ws_ffi.rs`, `http/curl_websocket.rs`, `http/cookie_websocket.rs`, `tests/websocket.rs`, `tests/support/{mod,ws_server}.rs`. Changed: `domain/models.rs` (WsPayload, WsHandshake, ClosedBy, WsEvent), `domain/ports.rs` (WebSocketConnector, WebSocketConnection, WsConnectError, WsPoll), `http/curl_client.rs` (`apply_tls_and_proxy` extracted), `http/cookie_client.rs` (`jar_cookie_header` and `store_set_cookies` extracted), `send_request.rs` (`validate_proxy` is `pub(crate)`), `lib.rs` (`#![deny(unsafe_code)]`), the three `mod.rs` files. **Not wired into `AppState` or any command yet**: that is 13c.

Where it departs from the plan above:

- **No new dependency at all.** The plan expected `curl-sys` as a direct dependency. It is not needed: `curl_ws_ffi.rs` declares its own prototypes, and the symbols resolve against the libcurl the `curl` crate already links. `Easy2::raw()` is `&self -> *mut curl_sys::CURL`, cast to `*mut c_void` without naming the type. `Cargo.toml` is untouched.
- **`unsafe` is enforced, not conventional.** `#![deny(unsafe_code)]` in `lib.rs`, `#[allow(unsafe_code)]` on `mod curl_ws_ffi` in `http/mod.rs`. Tauri's macros were checked first: only the mobile entry-point macro emits `unsafe`, and this app does not use it.
- **Cookies are a decorator, not a helper.** `CookieWebSocketConnector` wraps any `WebSocketConnector`, as `CookieClient` wraps `HttpClient`. The two share `jar_cookie_header` and `store_set_cookies`, so there is one rule for both. `ws://` is matched as `http://` and `wss://` as `https://`, so a Secure cookie goes over `wss://` and never over `ws://`. `CookieClient`'s six tests pass unchanged.
- **The event sink is a boxed closure** (`FnMut(WsEvent) -> bool`), not a trait. Returning false means nobody is listening, and the connection closes with a close frame instead of running on unseen. 13c wraps the Tauri `Channel` in one.
- **`ReconnectPolicy` is a value** with a `Default` (1 s doubling to 30 s, 10 attempts) and `WebSocketSessions::with_policy` as the test seam, so reconnect tests take milliseconds.
- **Disconnect is a stop flag, not a command.** It cancels a handshake in progress, a reconnect wait, or a live connection. A live connection sends a normal close (1000) and waits up to 3 s for the server's answer; the Closed event carries the server's code if it answered.
- **Invalid UTF-8 in a text message closes with 1007**, as RFC 6455 section 8.1 requires, rather than being shown lossily. **The size limit is checked against the size a frame declares**, so a server announcing a gigabyte is refused on its first chunk.
- **Events from the handshake:** `connect` returns only after the handshake. A refusal returns the error and emits nothing. The connection's own thread forgets its id *before* emitting the final `Closed`, so a UI that reconnects on `Closed` under the same id is never refused.

**Verified — a real build and real tests, not an offline approximation.** crates.io is blocked in the cloud, but GitHub is not. So the `curl` crate 0.4.50 and its dependencies (libc, socket2, libz-sys, cc, pkg-config) were built from their tagged sources, with their manifests rewired to local paths and the Windows-only dependencies stripped. They were linked against libcurl 8.21.0 built from its git tag. The app's own 13b sources were compiled unchanged in that harness, plus the domain and HTTP modules they depend on. Result:

- **141 unit tests pass**, including all 47 new ones: `ws_frames` 14, `WebSocketSessions` 21, `cookie_websocket` 6, `curl_websocket` 5, `curl_ws_ffi` 1. The existing `CookieClient`, `send_request` and mapping tests pass against the refactors.
- **18 integration tests pass against the real libcurl**, plus 1 ignored (the opt-in public `wss://` test). Covered: text and binary echo, handshake headers, a fragmented message, a 200,000-byte frame through the 64 KiB buffer, a server ping answered while only listening, a server close answered with its code, Disconnect seen by the server as 1000, 1009 on an oversized message, 401/403/200 refusals with their status, an unreachable host, a cancelled handshake to a server that never answers (well under its 10 s hang), custom headers and a subprotocol, the cookie jar both ways, 8 MiB to a stalled reader byte-exact, a drop without a close frame, and a reconnect after a drop.
- **Ten repeated runs of the whole suite: no flakes.**
- **`clippy --all-targets -D warnings` clean**, with `unsafe_code` denied outside the FFI file.

**One finding, harness only:** a libcurl built without TLS rejects the TLS options with `CURLE_UNKNOWN_OPTION` (48), even for a `ws://` URL. The first harness build had no TLS and failed every connect for that reason. The app's libcurl always has rustls, so nothing changes in the code, but it is why the harness uses an OpenSSL build.

**What the harness could not show:** rustls instead of OpenSSL, Windows (`WSAPoll`, the `usize` socket type), and clippy 1.97 (the harness has 1.95). The Tauri, serde and rusqlite parts of the app were not in the harness, but 13b does not touch them. **`verify.bat` is the gate.** Expected: Rust unit tests up by 47 (426 to 473), a new `websocket` integration binary with 18 passed and 1 ignored, and every existing count unchanged.

**`verify.bat`, 2026-09-21: green on the first run.** Build, eslint, `cargo fmt --check`, `clippy --all-targets -D warnings` (1.97, so the harness's older clippy missed nothing), and every test: **473 Rust unit** (up 47 from 426, as predicted), **14 curl integration** (unchanged), **69 repository** (unchanged; 1 ignored benchmark), **298 vitest** (unchanged), and the new **`websocket` integration binary: 18 passed, 1 ignored** (the opt-in public `wss://` test). That binary is the first run of the transport on Windows with rustls and `WSAPoll`: fragments, a frame bigger than the receive buffer, 1009 on oversize, pings answered while only listening, server close echoed, a drop reported as an error, reconnect, a refused upgrade with its status, a cancelled hanging handshake, cookies both ways, and a big message to a slow reader byte-exact. Nothing needed a fix.

To run the public `wss://` test on Windows: `cd src-tauri && cargo test --test websocket -- --ignored`.


---

### 13c — Commands and IPC streaming

**Steps**

1. `commands/dto.rs`: `WebSocketRequestInput`, `WsOutgoingDto`, `WsEventDto` (`#[serde(tag = "type", rename_all = "camelCase")]`).
2. `commands/websocket.rs`: the three commands. Register them in `lib.rs`. Add `AppState.websockets: WebSocketSessions`, built beside `send_request`.
3. `src/types/websocket.ts`, mirroring the DTOs exactly.
4. `src/services/websocket.ts`, the only place a `Channel` is created and `invoke()` is called for these commands:
   - `connect(connectionId, input, onEvents)`, `send`, `disconnect`.
   - **Event coalescing.** Incoming events are buffered and flushed once per animation frame as a batch, so 1,000 messages a second is 60 store updates a second and not 1,000.
   - Logging follows `services/http-client.ts`: the URL through the redacted path, never `args`, never a payload.

**Tests**

- The service against a mocked `Channel`: batching preserves order, a `closed` event flushes immediately, and a failed connect surfaces as a typed `Result`.
- **Verify** that the messages of one channel arrive in send order before relying on it. If the docs do not guarantee it, add a sequence number to `WsEvent` and re-sort in the service.

**Done when** a connect, send and disconnect round trip works from a throwaway harness in the webview, with `verify.bat` green.

#### 13c as built — 2026-09-22, **verified: `verify.bat` green (run 3) and smoke test passed (run 2)**

**Rust.**

- `commands/dto.rs` gained `WsPayloadDto` (tagged on `kind`: `text {text}` or `binary {hex}`; used both ways), `WsClosedByDto` and `WsEventDto` (tagged on `type`, since the payload inside already uses `kind`). Sent and received events carry `byteLength`, the size on the wire, because for text that is not the JS string length.
- Binary crosses as lowercase hex with no separators. It is half the size of a JSON number array, and the log shows binary as hex anyway. Rust decodes strictly and refuses anything else as `invalidRequest`, so the composer's leniency (spaces, `0x`) has one home, the frontend (13e).
- There is no separate `WebSocketRequestInput`. `WebSocketRequestDto` from 13d already has the exact shape, so the connect command takes it.
- `commands/websocket.rs` has the three commands:
  - `connect_web_socket(connectionId, request, onEvent: Channel<WsEventDto>)` runs the blocking handshake on `spawn_blocking`, as `send_request` does. The sink is the channel, and a failed channel `send` ends the connection.
  - `send_web_socket_message(connectionId, message)`.
  - `disconnect_web_socket(connectionId)`.
- `AppState.websockets` is `WebSocketSessions` over `CookieWebSocketConnector(CurlWebSocketConnector)`, using the same jar as `CookieClient`.
- 4 new dto tests pin every event variant's keys, the outgoing message shape, the hex round trip, and malformed hex.

**Frontend.**

- `types/websocket.ts` gained `WsPayload`, `WsClosedBy` and `WsEvent`.
- `lib/event-batcher.ts` is generic and pure. It batches until the scheduler fires. An urgent event flushes at once, together with everything queued before it. It also flushes at `maxBatch` without waiting, because animation frames stop while the window sits in the tray and a busy connection must not grow the queue without bound. 5 tests.
- `services/websocket.ts`:
  - `connectWebSocket(connectionId, request, logUrl, onEvents, schedule = requestAnimationFrame)`.
  - Only `sent` and `received` wait for a frame. Every lifecycle event (connected, closed, error, reconnecting) flushes at once. The batch cap is `MAX_EVENTS_PER_BATCH = 500`.
  - `sendWebSocketMessage` goes through the shared `call()`. `disconnectWebSocket` is fire-and-forget.
  - Logging is the `logUrl` plus close codes; no message is ever logged.
  - 8 tests against a fake `Channel`.

**Ordering, verified rather than assumed.** The installed `@tauri-apps/api` 2.11.1 `Channel` stamps every message with an index and holds back any that arrive early until the gap fills ("the index is used as a mechanism to preserve message order", core.js). So no sequence number was added. The `connected` event and the resolution of the connect `invoke` travel different paths, though, so the service documents that their relative order is not defined, and 13e must not depend on it. `package.json` still allows `^2.0.0`. The lockfile pins 2.11.1, but raising the floor would make the guarantee explicit (13h).

**Known limit, for 13h.** If the webview reloads (dev only, since the shipped app never reloads), the old channel's `send` may still succeed on the Rust side, so a connection can outlive the page that opened it. A `disconnect_all` on frontend start would close that gap.

**Checked here:**
- `tsc --noEmit`, eslint (`--max-warnings 0`) and prettier on the TS files.
- The batcher tests, run with node against a small shim.
- The hex codec under `rustc --test` and clippy.
- rustfmt on every Rust change.

**`verify.bat` run 1, 2026-09-22:** everything up to clippy passed: build, eslint, fmt, and vitest at 311 tests in 33 files, exactly as expected. The Rust compile then failed. `WsEventDto` derived `PartialEq, Eq`, but `KeyValueDto` has neither. Nothing compares events, since the tests go through `serde_json::Value`, so the derive was dropped rather than adding traits to `KeyValueDto`. 

**`verify.bat` run 2, 2026-09-22: green.** Build, eslint, fmt and clippy passed, along with every test: **477 Rust unit** (up 4, as predicted), **14 curl**, **69 repository** (1 ignored), **websocket 18** (1 ignored), and **311 vitest** in 33 files (up 13). The rest of the uncompiled Tauri code compiled as written, including the `Channel<WsEventDto>` sink moved into the owner thread, the three commands, and the `AppState` wiring.

**Not checked here:** vitest (the Linux rollup binary is missing on this side), and anything that needs `tauri`, `serde` or `rusqlite`. **`verify.bat` is the gate.** Expected: Rust unit tests up by 4 (473 to 477), vitest up by 13 across 2 new files (298 to 311, 33 files), every other count unchanged.

**Smoke test, in place of a throwaway harness.** In `npm run tauri dev`, the webview's devtools console can load the service straight from Vite, so no harness code is written or committed:

```js
const ws = await import("/src/services/websocket.ts");
const { DEFAULT_WS_SETTINGS } = await import("/src/types/websocket.ts");
const url = "wss://echo.websocket.org";
await ws.connectWebSocket("smoke", { url, headers: [], settings: DEFAULT_WS_SETTINGS }, url, (events) => console.log(events));
await ws.sendWebSocketMessage("smoke", { kind: "text", text: "hello" });
await ws.sendWebSocketMessage("smoke", { kind: "binary", hex: "deadbeef" });
await ws.disconnectWebSocket("smoke");
```

Pass means:
- connect resolves `{ ok: true }` and a `connected` batch (status 101) appears;
- `sent`/`received` pairs follow for both messages, the binary one echoed as `deadbeef`;
- disconnect ends with `closed` from `user`, code 1000.

**Smoke test run 1, 2026-09-22: failed.** Three parts worked:
- Connect resolved with a `connected` event: status 101, seven handshake headers, and the right `sec-websocket-accept`.
- The server's 32-byte greeting arrived as `received`.
- Disconnect ended with `closed` from `user`, code 1000, 67 ms later.

**Neither message was ever sent**, though both sends had returned success: no `sent` event and no echo appeared. The cause was in 13b's owner loop, which checked the stop flag *before* draining the outgoing queue. The owner thread spends almost all its time inside the 25 ms socket wait. When Send, Send and Disconnect all landed within one wait, the next pass saw the stop flag first and closed the connection, and the two accepted messages disappeared without an event. The log never showed anything false, since `sent` only fires once a message is on the wire. But a Send that returns success and then silently goes nowhere is a bug.

**Fix.** `serve` now reads the stop flag first, drains the queue, and only then acts on the flag. Everything the command layer accepted before Disconnect goes out ahead of the close frame. Two things make this safe:
- `CancellationToken` is `SeqCst`, so seeing the cancel means seeing every queued message sent before it.
- The frontend awaits each send before it calls disconnect.

**Regression test.** `messages_queued_before_disconnect_go_out_before_the_close` makes its sink queue two messages and press Disconnect on `connected`, before the owner thread starts. The old order therefore fails it every time, not just when the timing happens to line up. It expects `connected`, `sent` twice, then `closed` by user, and the wire frames Text, Text, Close.

**Expected on the rerun:** 478 Rust unit tests (up 1), everything else unchanged. After that, run the smoke test again: it should show a `sent`/`received` pair for each message before `closed`.

**`verify.bat` run 3, 2026-09-22: green.** 478 Rust unit tests (the new regression test passes), 14 curl, 69 repository (1 ignored), websocket 18 (1 ignored), and 311 vitest. 
**Smoke test run 2, 2026-09-22: passed.** The events arrived in this order:
- `connected` (101);
- the greeting as `received`;
- **both messages as `sent`**: `hello` (5 bytes) and binary `deadbeef` (4 bytes), in one batch 25 ms later, which is one owner-loop pass;
- `closed` from `user`, code 1000, 49 ms after that.

The echoes do not appear, and that is by design, not a bug. The snippet disconnects immediately, so the echoes arrived during the close handshake, and `close_by_user` drops messages that arrive after Disconnect (a 13b decision). Whether that rule should stay is an open question for 13e (see below). **13c is done.**

**Open question from the smoke test.** Should messages that arrive between Disconnect and the server's close frame be logged, or dropped as they are now? Logging them makes the log a truthful record of what the server sent: RFC 6455 lets a peer keep sending data until it answers the close. It would be a small change to `await_close_answer`.

---

### 13d — Persistence and the collection tree

Independent of 13b and 13c.

**Design.** WebSocket requests live in the **same `requests` table**, so move, rename, delete, docs and the `ON DELETE CASCADE` from folders and collections work unchanged, with no second table to keep in step (spec section 3). A `kind` column tells the two apart. Sibling Rust types, not a union: a union on `SavedRequest` would force an exhaustive match through the 24 files that mention it (11 Rust, 13 TypeScript) for no behavioural gain.

**Steps**

1. `0010_web_sockets.sql`, additive only:
   ```sql
   -- src-tauri/src/persistence/migrations/0010_web_sockets.sql
   ALTER TABLE requests ADD COLUMN kind    TEXT NOT NULL DEFAULT 'http';
   ALTER TABLE requests ADD COLUMN ws_json TEXT NOT NULL DEFAULT '';
   ```
   A WebSocket row stores `method = 'GET'` (which is truthful: the handshake is a GET), its URL in `url`, its headers in `headers_json`, and `body_json`, `query_params_json`, `settings_json` and `auth_json` at their empty defaults. Its own fields (draft message, format, settings) live in `ws_json`. `MIGRATIONS` becomes `[&str; 10]`. `kind` is a closed Rust enum, in the style of `DocsTable`, so no caller passes a free string.
2. **Every `SELECT` that yields an `HttpRequest` filters `kind = 'http'`.** That is `list_by_collection` and `get`. `get` on a WebSocket id returns a typed `NotFound`, never a mis-decoded GET. The `DO UPDATE` never touches `kind` or `docs_md`, and an HTTP save aimed at a WebSocket id is refused.
3. `domain/models.rs`: `SavedWebSocket { id, collection_id, folder_id, name, request: WebSocketRequest, draft: WsDraft }`.
4. `domain/ports.rs`: `WebSocketRepository` (`list_by_collection`, `get`, `save`). Rename, move, delete and docs stay on `SavedRequestRepository`, which acts by id and is kind-agnostic. A comment on the trait says why.
5. `persistence/repositories/json.rs`: `StoredWebSocket` with explicit serde defaults. Every new key that is a bool defaults to its documented value, never the type's default (the `send_cookies` and `encode_url` lesson, PLAN.md Phase 5 and 7).
6. `persistence/repositories/web_sockets.rs`, following `saved_requests.rs`.
7. `Collections` service: `contents()` also returns `web_sockets`. Save and load commands. `CollectionContentsDto` gains `webSockets`, and `EMPTY_COLLECTION_CONTENTS` in `src/types/collections.ts` gains the same field (that constant exists so this is one edit).
8. Fix `Database::migrate` to refuse a database whose `user_version` exceeds `MIGRATIONS.len()` with a clear error.
9. `ExampleRepository::create` refuses a request id whose kind is not `http`.

**Tests**

- Migration `0010` on a database that already holds HTTP rows: every existing row reads back as `http`, with an empty `ws_json`.
- A WebSocket round trip through every field, including a draft with non-ASCII text and a saved `wss://` URL.
- The HTTP list excludes WebSocket rows, and `get` on a WebSocket id is `NotFound`.
- A WebSocket save leaves `docs_md` alone (the Phase 12 regression test, repeated for this path).
- A blob in `ws_json` written before a later field existed still loads.
- Move, rename, delete and cascade all work on WebSocket rows.
- A future database is refused.
- No example can be created under a WebSocket row.

**Done when** a WebSocket request can be saved into a collection that holds HTTP requests, listed beside them and reloaded with every field intact.

#### 13d as built — 2026-09-21, **verified: `verify.bat` green on the first run**

Written as planned. Files: `0010_web_sockets.sql`; `persistence/repositories/request_kind.rs` and `web_sockets.rs` (new); `database.rs`, `saved_requests.rs`, `examples.rs`, `json.rs`, `mod.rs`; `domain/models.rs`, `ports.rs`, `services/collections.rs`; `commands/dto.rs`, `commands/collections.rs`, `lib.rs`; `tests/repositories.rs`. Frontend: `types/websocket.ts` (new), `types/collections.ts`, `services/collections.ts` and three test files.

Where it went beyond the plan, or chose between options:

- **The kind guard is in SQL, not a read-then-write.** Both upserts end in `ON CONFLICT(id) DO UPDATE ... WHERE requests.kind = excluded.kind`. An id of the other kind then changes 0 rows, and `refuse_other_kind` turns that into `InvalidRequest` naming both kinds. One statement, no race. The SQLite behaviour was checked against the real migrations before relying on it: a guarded update that does not match reports 0 changes and leaves the row as it was.
- **`kind` is always bound, never formatted.** `RequestKind` is a closed enum whose `as_str()` goes in as a parameter, so unlike `DocsTable` no identifier is ever formatted into SQL.
- **WebSocket ids use the `req_` prefix.** The rows share a table, and rename, move, delete and docs reach them through the request commands by id. A separate prefix would have implied a separation that does not exist.
- **`save_web_socket` takes one grouped `input`**, like `save_example`, rather than `save_request`'s loose parameters. The TypeScript wrapper and a test pin the key.
- **A WebSocket row's HTTP-only columns** are filled with an empty HTTP request's values (`method = GET`, no params, no body, default settings). `auth_json` and `docs_md` are left to their column defaults and never updated, which is what keeps Save from wiping docs, tested for this path as it was for HTTP in Phase 12.
- **`ws_json` defaults are the migration.** Every field of `StoredWebSocket` is defaulted, `verify_tls` and `send_cookies` explicitly to true. An empty column reads as all defaults, not an error.
- **The newer-schema guard** (`refuse_newer_schema`) closes the PLAN.md Phase 7 open item. **Not done:** at startup the refusal ends the app through the normal setup error path, with nothing shown to the user. The installer already blocks downgrades, so this only affects a copied app-data folder. A dialog like `desktop/webview.rs::report_unavailable` is the obvious follow-up, left for a decision rather than added unasked.
- **OpenAPI is already safe.** A test exports a collection before and after a WebSocket is added and asserts the document is byte-identical. The omission *note* is still 13g.

Tests added: 25 Rust (request_kind 3, json 4, database 3, Collections service 3, DTO wire shape 2, repositories 10) and 3 TypeScript (service wrappers).

Verified so far:

- Domain layer (models, ports, the `Collections` service) and `request_kind.rs` compiled offline with a stand-in for `thiserror`: 24 tests pass, including the 3 new service tests and the 3 `request_kind` tests; `clippy -D warnings` clean.
- `tsc --noEmit` clean on the whole frontend; eslint clean on the changed files; prettier clean. Prettier also reformatted two lines in `lib/collection-tree.test.ts` that were already unformatted at HEAD.
- rustfmt applied to every changed Rust file.
- **Not compiled here:** everything that needs rusqlite, serde or tauri — `json.rs`, `web_sockets.rs`, `saved_requests.rs`, `examples.rs`, `database.rs`, `dto.rs`, `commands/collections.rs`, `lib.rs` and `tests/repositories.rs`. vitest also needs Windows' native rollup binary. **`verify.bat` is the gate.**

**`verify.bat`, 2026-09-21: green on the first run.** Build, eslint, `cargo fmt --check`, `clippy --all-targets -D warnings`, and every test: **426 Rust unit** (up 15 from 411), **14 curl integration** (unchanged), **69 repository** (up 10 from 59; the 1 ignored is the existing opt-in large-spec benchmark), **298 vitest** (up 3 from 295). All 28 new tests ran and passed. Nothing in the uncompiled half needed a fix, the same result as the Phase 8 slices written the same way.


---

### 13e — Frontend state and pure logic

**Steps, types first**

1. `src/types/collections.ts`: `SavedWebSocket`, and `CollectionContents.webSockets`.
2. `src/lib/ws-payload.ts` and tests, all pure: `encodeOutgoing(format, text)` returns a text or a binary payload, `parseHex` is whitespace and `0x` tolerant with an error naming the byte offset, `hexDump(bytes)`, `payloadPreview(payload)` for the one-line log text, and `jsonWarning(text)`.
3. `src/lib/ws-log.ts` and tests, all pure:
   - `filterLog(entries, { direction: all | sent | received | system, query })`. The query matches text payloads and system messages. Binary matches on its hex.
   - `appendCapped(log, entries, caps)`, enforcing the 1,000-entry and 32 MiB caps, evicting the oldest, and noting "N earlier entries dropped".
   - `clearSession(log, connectionId)` for Clear Messages and `clearAll()` for Clear Response (section 5). Each entry carries its `connectionId`; a reconnect keeps the session's id.
   - Every entry has a stable `id`, so React keys never use array indices (CLAUDE.md §6).
4. `src/lib/format.ts`: `formatClockTime(epochMs)` giving `HH:mm:ss.SSS`.
5. `src/lib/variables.ts`: a WebSocket variant of the substitution, applied to the URL, header values and the message on connect and on send. The same single-pass, first-row-wins and unknown-names-left-as-written rules. **A secret variable's resolved value never reaches the log or the saved draft**, consistent with Phase 9.
6. `src/store/request-store.ts`:
   - `WebSocketTab { kind: "websocket"; id; url; headerRows; paramRows; settings; draft: { format; text }; connection: idle | connecting | connected | disconnecting | reconnecting; connectionId; log; logFilter; logQuery; loadedRequest; savedSnapshot }`.
   - `Tab` becomes a five-way union. `replaceActiveTab` stays HTTP-only, so no HTTP setter can touch a WebSocket tab.
   - New actions: `openBlankWebSocketTab`, `connect`, `disconnect`, `sendMessage`, `clearMessages` (current session), `clearResponse` (all sessions), `setLogFilter`, `setLogQuery` and the field setters.
   - `openSavedRequest` and `isDirty` dispatch on the tab's kind. The dirty snapshot covers URL, headers, settings, draft text and format.
   - `closeTab` on a connected tab disconnects first. The store owns the connection lifetime, so leaving a tab never leaks a socket.
7. `src/store/collections-store.ts`: carry `webSockets` through `contentsById`; `src/lib/collection-tree.ts`: group them by folder beside requests.
8. `src/lib/docs-title.ts`: `docsTargetName` also looks in `webSockets`. Otherwise a WebSocket's Docs tab is titled with a fallback, not its name.

**Tests**

- The pure libs, the highest coverage in the phase.
- Store: connect, connected, send, receive, disconnect through a mocked service; dirty tracking on a draft edit; `openSavedRequest` opens a WebSocket tab for a WebSocket item and focuses an existing one; closing a connected tab disconnects; `replaceActiveTab` ignores a WebSocket tab.
- `request-store.test.ts` has 13 `SavedRequest` mentions and will need touching. Keep its existing cases as they are.

**Done when** the store drives a full session against a mocked service, and `tsc --noEmit`, eslint and Vitest are clean.

#### 13e as built — 2026-09-22, **verified: `verify.bat` green on the first run**

Step 1 was already done in 13d. Steps 2–8 were written as planned; where they differ:

- **`lib/ws-payload.ts`.**
  - `parseHex` accepts whitespace between bytes and a `0x` prefix on any group. It returns the lowercase wire spelling that Rust's strict decoder accepts, and names the offset of the first bad byte from 0 (for example `Byte 3: "Xf" is not a hex byte`).
  - `encodeOutgoing`: text and JSON are sent as typed, and invalid JSON is only a `jsonWarning`. Hex that does not parse refuses to send.
  - `hexDump` takes the hex string, not bytes, because hex is what the log holds end to end.
  - `payloadPreview` folds whitespace and cuts at 200 characters. Binary shows as `Binary, N bytes: de ad …`.
- **`lib/ws-log.ts`.**
  - The log is `{ entries (oldest first), retainedBytes, droppedCount }`. Each entry is `{ id, connectionId, event }`, and the raw `WsEvent` is kept rather than a derived row.
  - `appendCapped` evicts the oldest entries to hold 1,000 entries and 32 MiB, but always keeps the newest.
  - `clearSession` (Clear Messages) keeps the dropped note. `clearAll` (Clear Response) resets everything.
  - `filterLog`: binary matches on hex, with spaces in the query ignored.
  - `systemText` produces the text for system rows.
- **`lib/ws-request.ts`** (new, not in the plan): `WebSocketShape` (request plus draft) and `webSocketShapesEqual` for dirty tracking. It is the WebSocket counterpart of `request-defaults.ts`.
- **Secrets never reach the log.**
  - Connect: the store keeps the display URL (secrets left as placeholders) and puts it on the `connected` event.
  - Send: the store queues a `{ resolved, display }` pair per message. `takePendingSend` pairs each `sent` event with the first matching queued message. Any queued message before the match was dropped by Rust during a reconnect, and is discarded here too.
  - An echo from the server is the server's data and is shown as it arrived.
  - Hex containing a secret placeholder is logged as the typed text, because the placeholder does not parse as hex.
- **Store.**
  - `WebSocketTab` has the fields listed in the plan, plus `reconnectAttempt` (for the badge's n/N) and `composerError`.
  - `openSavedWebSocket` is a separate action, not a branch inside `openSavedRequest`. The two saved types are different shapes, and a separate action keeps both signatures exact.
  - URL and Params setters are ignored unless the tab is idle.
  - Events from an earlier connection still join the log but no longer change the connection state.
  - A failed handshake writes its own error row (`Could not connect: …` or `Connection cancelled`), because Rust reports nothing on the channel for it.
  - `closeTab` disconnects a live tab. The per-connection bookkeeping is dropped when its `closed` event arrives or its connect fails.
- **Beyond the plan, needed to keep `tsc` clean or for 13f:**
  - `App.tsx` gives a WebSocket tab a label, an entry in the close prompt, and an empty pane in place of the builder (13f). Nothing in the UI opens such a tab yet.
  - `TabBar` gained the `websocket` kind with the `Zap` icon. It turns green when connected in 13f, with the `--ws-ok` token.
  - `collections-store.saveWebSocket` was added for 13f's Save dialog.
- **Unchanged pending your answer:** messages that arrive after Disconnect are still dropped (13b rule, question raised after the 13c smoke test).

**Checked here:**
- `tsc --noEmit`, eslint on all of `src`, and prettier on every changed file.
- **Every new and changed test run with node** against a small vitest stand-in, including zustand's real vanilla store:
  - ws-payload 13, ws-log 13, ws-request 2, format 8, docs-title 7, collection-tree 7;
  - variables 17, where the one failure was a matcher the stand-in lacks;
  - **websocket-tab 14/14**;
  - the existing request-store 23/23.
- Two deliberate breaks in the store were each caught by the tests: redaction disabled, and `closed` not returning to idle.

**Expected from `verify.bat`:** vitest 359 tests (up 48) in 37 files (up 4), Rust unchanged at 478.

**`verify.bat`, 2026-09-22: green on the first run.** Build, eslint, fmt and clippy passed. Vitest ran **359 tests in 37 files**, exactly as predicted. Rust: 478 unit, 14 curl, 69 repository (1 ignored), websocket 18 (1 ignored).

---

### 13f — UI

**Steps**

1. `features/websocket/WebSocketBuilder.tsx`: props-in, events-out, like `RequestBuilder`. It composes `RequestToolbar`, a URL row with the contextual Connect/Disconnect/Cancel button, and the sub-tab row. `RequestToolbar` currently always renders **cURL** and **Cookies** buttons, so `onCopyAsCurl` and `onManageCookies` become optional props and the WebSocket toolbar shows only the breadcrumb and Save, as the screenshots do. The **Cookies** link at the right of the sub-tab row opens the existing `CookieManagerDialog`. There is no Copy-as-cURL for a WebSocket.
2. Sub-tabs:
   - **Message** (`MessageComposer.tsx`): `LazyCodeEditor` with a language that follows the format (`plaintext`, `json`, hex as `plaintext`), the format `<select>`, and Send. Send is disabled unless connected, or when the hex does not parse.
   - **Params:** the existing `KeyValueTable`, kept in step with the URL through `lib/query-sync.ts`, locked while connecting or connected.
   - **Headers:** the existing `KeyValueTable` with the IANA name autocomplete and value suggestions. The `Sec-WebSocket-*` names are already in the list.
   - **Settings:** connect timeout, max message size, auto-reconnect, verify TLS (with the existing visible warning when off), proxy and send cookies. It follows `SettingsPanel`'s conventions and does **not** fork it: WebSocket settings are a different set.
   - **Docs** (D4): a rendered preview of the saved item's docs and an "Edit documentation" button that calls `openDocs`. An unsaved request shows "Save the request to add documentation", the same pattern as the disabled Save Response reason.
3. `features/websocket/WebSocketLog.tsx`: the status badge, the "…" button opening a `ContextMenu` with **Clear Response**, search, filter select, Clear Messages, and the rows with their expandable detail. A plain list, no virtualisation dependency, which the 1,000-entry cap makes safe. Measure it during 13h.
4. `TabBar.tsx`: a `websocket` arm in `TabBarTab` with the `Zap` icon. The "+" button opens the existing `ContextMenu` with **HTTP request** and **WebSocket request**.
5. `CollectionsSidebar.tsx`, `CollectionTreeNode.tsx`, `tree-types.ts`: a "New WebSocket request" entry beside "New request" on collections and folders, using the same inline-name flow. A WebSocket leaf shows `Zap` where an HTTP one shows a method. Rename, Move, Docs and Delete already work by id. Deleting one uses the plain delete prompt, since there are no saved responses underneath.
6. `SaveRequestDialog.tsx`: accept a WebSocket payload. **No collection or folder is ever hidden or disabled by protocol** (spec section 3).
7. `App.tsx`: render `WebSocketBuilder` for a `websocket` tab. Extend `canSaveWithShortcut`, `handleSaveClick` and the Ctrl+Shift+D target from `kind === "request"` to include WebSocket tabs. The three places are listed here because they are exactly what a "one more tab kind" change forgets.
8. Theme tokens (section 5).

**Tests**

- Component tests with a mocked service, written as the spec's scenarios:
  - Connect flips the badge to Connected and the button to Disconnect.
  - Send stays disabled until connected.
  - After a send and an echo, the log shows an up-arrow entry and a down-arrow entry with timestamps.
  - Saving into a collection that holds HTTP requests succeeds with no warning.
  - The filter and search narrow the rows.
  - After two connects, Clear Messages empties only the second session, and "…" → Clear Response empties both.
  - Clearing while connected keeps the badge at Connected, and the next echo still lands.
- A lib-level test that no protocol check exists in the save path.

**Done when** the two screenshots can be reproduced state for state, in both connected and disconnected states.

#### Decisions taken 2026-09-22, built with 13f

- **Messages that arrive between Disconnect and the server's close are logged.** This replaces the 13b rule that dropped them.
  - `close_by_user` and `await_close_answer` in `domain/services/websocket.rs` now report each text or binary message they see while waiting for the close answer, through the connection's sink.
  - Pings in that window go unanswered, because our close frame is already on its way.
  - A message that breaks a rule in that window ends the wait.
  - Tests:
    - `a_message_that_arrives_while_closing_is_reported` uses a new scripted step, `Step::AwaitClose`, which holds the server's message back until the client's close frame has gone. The message can therefore only arrive in that window.
    - `messages_queued_before_disconnect_go_out_before_the_close` now also expects both echoes before `closed`.
    - The integration test `an_echo_that_arrives_while_closing_is_still_reported` runs the same case against the local server.
- **A database the app cannot open now gets an error dialog at startup.** This covers the newer-schema case and any other open failure.
  - The new `desktop/startup_error.rs` holds `database_message(details, data_dir)`, which is pure and tested, and `report_database_failure`, which shows it with rfd as `webview.rs` does. It uses the same "ResponderHTTP cannot start" title.
  - The dialog says nothing was changed, gives the reason (for a newer database: "…update the app to open it") and names the data folder.
  - `lib.rs` shows it when `Database::open` fails, then ends setup with the error as before.

#### 13f as built — 2026-09-22, **verified: `verify.bat` green on the first run**

- **Components**, all in `features/websocket/`:
  - `WebSocketView` is the container, wired to the store like `DocsEditor`. Save and the cookie manager come in as props, because App owns their dialogs.
  - `WebSocketBuilder` holds the breadcrumb and Save, the URL and the state-matrix button (Connect / Cancel / Disconnect / Disconnecting…), the Docs · Message · Params · Headers · Settings tabs, and a Cookies link on the right of that row.
  - `MessageComposer` holds the editor, the Text/JSON/Hex selector and Send.
    - Send needs a connection and parseable hex.
    - Invalid JSON shows a warning but does not block Send.
    - A failed send's reason appears beside the selector.
  - `WebSocketSettingsPanel` holds the connect timeout, max message size in KB, auto-reconnect, proxy, send cookies, and verify TLS with the existing warning.
  - `WebSocketDocsPanel` shows the rendered docs and an "Edit documentation" button that calls `openDocs`. An unsaved request shows "Save the request to add documentation."
  - `WebSocketLog` holds the status badge, the "…" `ContextMenu` with Clear Response, search, the filter, Clear Messages, and the rows.
    - Rows run newest first, each an icon, a one-line preview, `HH:mm:ss.SSS` and a chevron.
    - An expanded message shows its byte size, a Copy button and read-only Monaco, with JSON pretty-printed. Binary shows a hex dump.
    - An expanded Connected row shows the 101 status and the handshake headers.
    - The note "N earlier entries dropped" appears when the cap has evicted entries.
- **Params lock** through a disabled `<fieldset>`, so `KeyValueTable` needed no read-only mode. While connected, Headers and Settings show "Changes apply the next time you connect."
- **`lib/ws-status.ts`** (new) holds the state matrix as data: the badge label and tone, the primary action, and the filter labels. Tested.
- **`hooks/useBuilderPanelHeight.ts`** (new) holds the panel-height clamp and re-clamp-on-resize, moved out of `RequestBuilder` rather than copied into a second builder. It is the first file in the existing `hooks/` folder.
- **Shared components changed:**
  - `RequestToolbar`'s cURL and Cookies buttons are now optional.
  - `TabBar`'s "+" opens a `ContextMenu` with HTTP request and WebSocket request. A WebSocket tab's `Zap` turns `text-ws-ok` while connected.
  - `SaveRequestDialog` takes a `SavePayload` union (`http` or `websocket`). Nothing in it looks at protocol, which a test pins.
- **Sidebar.**
  - Collections and folders have "New WebSocket request" beside "New request", using the same inline-name flow.
  - A WebSocket leaf shows `Zap` in place of a method. Its menu has Rename, Docs, Move and Delete, which act by id through the request commands. There is no Duplicate yet.
  - Delete uses the plain prompt, titled "Delete WebSocket request".
  - Opening one re-fetches it by id, like HTTP requests (`collections-store.loadWebSocket`). Creating one is `createWebSocket`.
- **App.**
  - Renders `WebSocketView`.
  - The three places named in step 7 all include WebSocket tabs: Ctrl+S, `handleSaveClick` (overwrite in place when saved, the dialog when not) and Ctrl+Shift+D.
  - The breadcrumb and sidebar highlight follow the WebSocket tab.
  - Closing a live WebSocket tab asks first, with "Close and disconnect" (assumption 11).
- **Tokens:** `--ws-sent` (nord13), `--ws-received` (nord9) and `--ws-ok` (nord14) are in both theme blocks, plus `colors.ws.*` in Tailwind.

**Checked here:**
- `tsc --noEmit`, eslint on all of `src`, and prettier on the changed files.
- The component tests ran with node + jsdom + Testing Library against a vitest stand-in:
  - **WebSocketView 8/8**: Connect flips the badge and button; Send is disabled until connected; sent and received rows with ms times, newest first; filter and search; Clear Messages vs Clear Response across two sessions; clearing while connected keeps the connection and the next echo lands; row expansion; the Docs sub-tab.
  - **SaveRequestDialog 1/1**: saves into a folder of a collection holding HTTP requests with no warning.
  - websocket-tab 14/14, request-store 23/23, ws-status 3/3.
- The Rust domain layer (`domain/`, with stand-ins for `thiserror` and `zeroize`) compiled here. All 90 of its tests pass, including the new one, and `clippy -D warnings` is clean.
- Re-breaking the close wait so it drops messages made both close tests fail.
- `database_message` passed its test under `rustc --test`, and rustfmt is clean on every Rust change.

**Not checked here:** vitest itself, the Rust that needs tauri/rfd (`lib.rs`, `report_database_failure`), and the integration test.

**Expected from `verify.bat`:**
- Rust unit tests 480 (up 2: one close test, one startup-message test).
- The `websocket` integration binary 19 passed, 1 ignored.
- vitest 371 tests (up 12) in 40 files (up 3).

**`verify.bat`, 2026-09-22: green on the first run.** Every count was exactly as predicted. Rust: 480 unit tests, 14 curl, 69 repository (1 ignored), and websocket 19 (1 ignored), including `an_echo_that_arrives_while_closing_is_still_reported`. Vitest: 371 tests in 40 files. The code I could not compile here also built as written: the `lib.rs` startup dialog wiring and `report_database_failure`.

#### Change after 13f — 2026-09-22: one Clear action

At your request, **Clear Messages now clears everything from every connection, and Clear Response and its "…" button are gone.**
- `lib/ws-log.ts`: `clearSession` is removed. `clearAll` is the one clear, and resets the entries, the byte count and the dropped note.
- Store: `clearMessages` calls `clearAll`. `clearResponse` is removed.
- `WebSocketLog`: the "…" button, its `ContextMenu` and the `onClearResponse` prop are removed. The header row is "Response" and the badge.
- Tests:
  - The store test now checks that Clear Messages empties the entries of two connections without disconnecting.
  - The component test checks that there is no "More actions" button and no "Clear Response".
- Checked here: `tsc`, eslint, prettier, and ws-log 12/12, websocket-tab 14/14 and WebSocketView 8/8 with the stand-in runner.
- Expected from `verify.bat`: vitest 370 tests (down 1, because `clearSession`'s test is gone). Rust is unchanged.
- **`verify.bat`, 2026-09-22: green.** Vitest ran 370 tests in 40 files, as expected. Rust: 480 unit, 14 curl, 69 repository (1 ignored), websocket 19 (1 ignored).

---

### 13g — OpenAPI export on mixed collections

WebSockets are left out of every OpenAPI export, without a crash and without silence.

1. WebSocket rows never reach `to_document`: `OpenApiExport` reads only the HTTP list (13d), so a `wss://` URL can never hit `map_url` and be misreported as unmappable.
2. `OpenApiExport` gains a `WebSocketRepository` handle (and its constructor call in `lib.rs`), used only to read WebSocket names, and adds `ExportNote::WebSocketOmitted { request }`. The wording is finished in `commands/openapi.rs::describe_note`: *"\"{request}\" is a WebSocket request and was left out. OpenAPI 3.x cannot describe WebSocket."* One note per request, matching `UnmappableUrl`.
3. **Folders.** The exporter emits one tag per folder. Check what it does today for a folder with no HTTP requests and keep that behaviour rather than special-casing.

4. **Import is untouched.** An OpenAPI import can never create a WebSocket row: `ImportRepository` writes through `upsert_request`, which always stores `kind = 'http'` (the column default).

**Tests:** a collection with both kinds exports only the HTTP paths at 3.0, 3.1 and 3.2, in JSON and YAML, with one note per WebSocket. A collection of only WebSockets exports a valid empty-paths document. The 3.0 `paths` is still always present. The `export → import → export` test is unchanged.

**No native export and no WebSocket import in this phase (D2).** Both arrive with the AsyncAPI phase. Until then a WebSocket request lives only in the app's own database.

#### 13g as built — 2026-09-22, **verified: `verify.bat` green (374 vitest, 480 Rust)**

**Changed at your request: one sentence with a count, shown in the export dialog before exporting, instead of a note per WebSocket request after it.** Step 2 is therefore dropped. `OpenApiExport` does not gain a `WebSocketRepository`, there is no `ExportNote::WebSocketOmitted`, and **no Rust changed**.

- `lib/openapi-export.ts` (new): `webSocketOmissionNotice(count)`.
  - It returns null for 0.
  - Otherwise it returns: *"N WebSocket requests will not be exported: the OpenAPI specification describes HTTP requests only and has no way to describe WebSocket requests."* The subject is singular for 1.
- `ExportOpenApiDialog`:
  - Reads the count from the collections store, `contentsById[collectionId].webSockets.length`.
  - Calls `refreshContents` on open. A collection never expanded in the sidebar has no contents loaded yet, and would otherwise read as zero.
  - Shows the sentence under the intro text with an info icon, only when the count is above zero.
- Steps 1, 3 and 4 already held from 13d:
  - The exporter reads only the HTTP list.
  - The 13d test pins that the document is byte-identical before and after a WebSocket is added.
  - Import only ever writes `kind = 'http'`.
- Tests: `openapi-export.test.ts` (2) and `ExportOpenApiDialog.test.tsx` (2). The dialog tests check that the count shows once the contents load, and that nothing shows when there are no WebSocket requests.
- Checked here:
  - `tsc`, eslint and prettier.
  - Both new test files, 4/4, with the stand-in runner.
- Expected from `verify.bat`: vitest 374 tests (up 4) in 42 files (up 2). Rust is unchanged at 480.

---

### 13h — Hardening and release

1. `verify.bat` green: build, eslint, Vitest, `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, all Rust tests.
2. `release.bat` green. The static-link check must still report **29 imports, unchanged**. WebSocket support is inside libcurl, so no new DLL should appear. If one does, stop and find out why.
3. Measure the log with 1,000 rapid messages and one 1 MiB message. Confirm scrolling and typing stay responsive, and that memory levels off at the cap.
4. Work through the click-through in section 9.
5. Update `PLAN.md` (a Phase 13 section with what was built and what was found) and `CLAUDE.md`: §3 layout, §4 gains a paragraph on the WebSocket engine and the contained `unsafe`, and §11 gains one rule ("`unsafe` only in `http/curl_ws_ffi.rs`").
6. Re-sync the Claude Project's copies of both files (F6).

#### 13h as built — 2026-09-22, **verified: `verify.bat` and `release.bat` green**

1. **`verify.bat`**: green through 13g. A rerun is needed for the changes below.
2. **`release.bat`**: yours to run. Nothing in 13h touches the build or linking, so the static-link check should still report 29 imports.
3. **Measurement.** The log's pure code was measured with node, using the real modules.
   - 1,020 messages in 60 batches took 1 ms to append, the 1,000-entry cap held, and searching 1,000 rows took 0.2 ms.
   - Forty 1 MiB messages levelled off at the 32 MiB cap: 32 kept, 8 dropped.
   - **Found: rendering that full log spent 1.6 s building the collapsed-row previews, on every batch.** `payloadPreview` folded whitespace over the whole message and split a whole binary payload into bytes, only to show 200 characters or 16 bytes.
   - Fixed. The preview reads a bounded slice (text: 1,600 characters; binary: the 16 bytes shown, with the size taken from the length). `LogRow` is `memo`ised with a stable toggle callback, so a new batch renders only the new rows.
   - After: 1.8 ms for the same full log, and a single 1 MiB preview went from 53–79 ms to under 0.1 ms.
   - Two tests pin this. One checks that a huge message previews within a generous time bound and still shows as cut. The other checks that text whose whitespace folds short is still marked as cut.
   - Memory note: a JS string is 2 bytes per character, and binary is held as hex, so the 32 MiB cap costs up to about 64 MiB of text or 128 MiB of binary in the webview. That is bounded, which is what the cap is for.
   - The in-app scroll and typing check under load is still yours.
4. **Click-through**: done by you.
5. **Docs.**
   - `CLAUDE.md`:
     - §1 mentions WebSocket.
     - §3 lists the new files.
     - §4 gains a *WebSocket* subsection covering the engine, the contained `unsafe`, the owner thread, the libcurl findings the design rests on, and the local test server.
     - §11 gains rule 10: no `unsafe` outside `http/curl_ws_ffi.rs`.
   - `PLAN.md` has a Phase 13 summary pointing here.
6. **Project docs**: re-synced (F6). The Claude Project's `CLAUDE.md` and `claude/PLAN.md` are now the repo's files.

**Also closed in 13h, both carried from 13c:**
- **A reloaded webview no longer leaves connections running unseen.**
  - `WebSocketSessions::disconnect_all` closes every connection through its normal close.
  - The command is `disconnect_all_web_sockets`, and the service function is `disconnectAllWebSockets`.
  - `App` calls it once on mount. In a shipped build the page loads once, so it finds nothing.
  - Tested in the domain with two connections, and in the service.
- **`@tauri-apps/api` floor raised to `^2.11.0`** in `package.json`, with the lockfile's specifier updated to match. The resolved version stays 2.11.1. The channel ordering 13c relies on was verified in that release, so the floor now says so. `pnpm install` should report "Already up to date".

**Checked here:**
- `tsc`, eslint and prettier.
- The Rust domain harness: 24 WebSocket service tests, including the new one, with clippy clean and rustfmt applied.
- The stand-in runner: ws-payload 15/15, the WebSocket service 9/9 and WebSocketView 8/8.

**Expected from `verify.bat`:** Rust unit tests 481 (up 1), and vitest 377 tests (up 3) in 42 files.

**`verify.bat` and `release.bat`, 2026-09-22: green.**
- Every count was exactly as predicted. Rust: 481 unit, 14 curl, 69 repository (1 ignored), websocket 19 (1 ignored). Vitest: 377 tests in 42 files.
- `pnpm tauri build` produced the NSIS installer and the MSI.
- **The static-link check reports 29 imported DLLs, unchanged, all Windows system DLLs. PASS.** WebSocket support added no import, as expected: it lives inside the statically linked libcurl.
- Still open: the in-app scroll and typing check under a fast message stream. The pure-code measurement above covers the cause it was guarding against.

---

### 13i — More message formats (after Phase 13) — **verified: `verify.bat` green on the first run**

Asked for after the release gate, following: **XML, HTML and Binary (Base64 or Hexadecimal)**, and **Beautify for JSON, XML and HTML**. Two decisions you made up front:
- An expanded binary message in the log opens in the composer's encoding and can be switched per row.
- Beautify leaves text that does not parse exactly as typed, and shows the reason next to the selector.

**Formats.**
- The selector offers Text, JSON, XML, HTML and Binary. Hex is no longer a format of its own: it is Binary with the Hexadecimal encoding.
- Every format except Binary goes out as a text frame exactly as typed. The format sets the editor's language (`json`, `xml`, `html` or plain text, all already bundled with Monaco) and whether Beautify shows.
- Binary shows a second selector, Base64 (the default) or Hexadecimal, and refuses to send until the text parses. The error names the offending position, as hex did before.
- The encoding is kept while another format is selected, so switching back restores it.

**Wire and storage.**
- Binary still crosses IPC as hex, so the frontend converts Base64 to hex before sending (`parseBase64`) and hex to Base64 for the log (`hexToBase64`). Rust's `WsPayloadDto` is unchanged.
- Domain: `WsMessageFormat` is now `Text | Json | Xml | Html | Binary`, plus a new `WsBinaryEncoding { Base64 (default), Hex }` and `WsDraft.binary_encoding`.
- DTO: `WsMessageFormatDto` uses `xml`/`html`/`binary`. `WsDraftDto.binaryEncoding` defaults to base64 when absent.
- **No migration.** The draft lives in the `ws_json` blob. `StoredWsMessageFormat` keeps `Hex` as a spelling that is read but never written. `draft_binary_encoding` is a new defaulted field.
- **A draft saved as `Hex` before this change opens as Binary in Hexadecimal**, which a test pins. Another test round-trips every format with both encodings and checks that the old spelling is never written.

**Beautify** (`lib/beautify.ts`, new).
- JSON is parsed and re-indented by two spaces.
- XML and HTML use a small scanner and one-element-per-line layout. An element holding only short text stays on one line. Quoted `>` characters in attributes, comments, CDATA, declarations and DOCTYPE are kept whole.
- XML must balance, and the reason names the tag: `</a> closes <b>`, `<a> is never closed`, `</a> has no opening tag`, or text outside the root element.
- HTML forgives what browsers forgive: void elements (`br`, `img`, …) need no closing tag, script and style content is raw text, a closing tag that skips open elements closes them, and elements left open at the end are allowed. It refuses only text with no element in it.
- The Beautify button is `Paintbrush`, with the tooltip "Beautify". It calls the new store action `beautifyWsDraft`, which sets the text or `composerError`.
- The response viewer's own `prettyPrint` is untouched.

**Log.** An expanded binary row has a "Show binary as" selector, initialised from the composer's encoding when the row opens.
- Base64 shows the encoded string, wrapped.
- Hexadecimal shows the existing hex dump.
- Copy takes the message in the encoding on screen.
- The collapsed preview stays hex.

**Tests.**
- Rust:
  - `json.rs`: the legacy `Hex` draft, and every format and encoding round trip.
  - `dto.rs`: every format and encoding spelling, the missing-encoding default, and the save input with Base64.
  - Existing constructors updated.
  - The domain harness compiles, with 100 tests passing. rustfmt is clean on every changed file.
- Frontend:
  - `beautify.test.ts` (11).
  - `ws-payload`: Base64 parse, errors, a 256-byte round trip, `encodeOutgoing` for all formats and both encodings, and `editorLanguage`.
  - `ws-request`: the encoding counts toward dirty.
  - Store: Base64 sends as binary, XML sends as typed, and three Beautify tests.
  - `WebSocketView`: which controls show per format, Beautify success and failure, and the log's per-row encoding switch.
- Checked here: `tsc`, eslint and prettier. With the stand-in runner: beautify 11/11, ws-payload 19/19, ws-request 2/2, websocket-tab 19/19 and WebSocketView 11/11.

**Expected from `verify.bat`:** Rust unit tests 484 (up 3), and vitest 400 tests (up 23) in 43 files (up 1). Repositories and integration tests are unchanged.

**`verify.bat`, 2026-09-22: green on the first run.** Every count was exactly as predicted. Rust: 484 unit, 14 curl, 69 repository (1 ignored), websocket 19 (1 ignored). Vitest: 400 tests in 43 files. The storage and DTO changes, which I could not compile here, built as written.

**Found in use, 2026-09-22: a message sent as Base64 read back as hex in the log.** The collapsed row's preview was always hex. It was a choice I made in 13i, and the wrong one: the row should read in the encoding you are using.
- `payloadPreview(payload, encoding)` now shows binary in the composer's encoding.
  - Base64 previews encode the first 48 bytes, a multiple of 3, so the prefix is exactly the start of the full Base64. Longer messages end in "…".
  - The cost stays bounded, as 13h required.
- The log passes the composer's encoding to every row, so switching Base64/Hexadecimal re-reads the whole log.
- Search follows too. With Base64 selected, a binary message also matches on its Base64, case-sensitively. Hex matching always works.
- Tests:
  - A Base64 preview test (short and long messages).
  - A `filterLog` test that Base64 matches only when Base64 is the encoding in use.
  - The WebSocketView binary test now checks the collapsed row as well.
- Checked here: `tsc`, eslint, prettier, and ws-log 13/13, ws-payload 20/20 and WebSocketView 11/11 with the stand-in runner.
- Expected from `verify.bat`: vitest 402 tests (up 2). Rust is unchanged.

---

## 8. Hard-rule check

| Rule | How this plan keeps it |
|---|---|
| 1. No system `curl`, no `Command` for HTTP | The handshake and frames go through the linked libcurl. |
| 2. No dynamic libcurl or OpenSSL | No new native library. 13h re-runs the import check. |
| 3. `invoke()` only in `services/` | `services/websocket.ts` is the only `invoke` and `Channel` site (D3). |
| 4. SQL only in `persistence/repositories/` | `web_sockets.rs` there; migration `0010` is a new file. |
| 5. Never edit a committed migration | `0010` is new, `0001` to `0009` untouched. |
| 6. Never log bodies, auth, tokens or cookies | Message payloads and handshake headers are never logged. Connect logs go through `logging.rs` redaction. |
| 7. TLS verification is an explicit opt-in | `verify_tls` defaults to on, and off shows the existing warning. |
| 8. No hand-edits in `components/ui/` | Nothing there is touched. |
| 9. Ask before inventing an abstraction | The one new trait has a test seam. D1 to D6 were asked and answered, not assumed. |
| §5 secrets | No auth secret is stored for a WebSocket. Header values follow the Phase 9 rule (plaintext, steered to secret variables). Docs stay plaintext, as Phase 12 decided. |
| §7 no `unwrap`/`expect` | Enforced by clippy as today. The FFI wrapper returns typed errors. |

---

## 9. Acceptance criteria to tests

| Gherkin scenario | Where it is proved |
|---|---|
| Connecting and sending | `ws_frames` and `WebSocketSessions` tests (13b), the echo integration test (13b), the store test and the component scenarios (13e, 13f) |
| Saving into an HTTP collection | Repository tests (13d), the `SaveRequestDialog` component test (13f) |
| Exporting to OpenAPI | 13g |
| Exporting to the native format | **Deferred** with AsyncAPI (D2) |
| (added) Clear Messages | `ws-log` tests (13e), component test (13f) |

---

## 10. Manual click-through (nothing here is covered by an automated test)

Same standing caveat as the cookie, auth and history features: passing its tests is not the same as having met a real server.

1. Connect to a public echo server over `wss://`, send text, and confirm the sent and received rows and the millisecond times. A local `ws://` echo server is the safer target if a public one is down.
2. Disconnect from the button, and separately kill the server. Confirm the closed row shows a code and reason and the badge goes red.
3. Send a binary (hex) message and confirm the echo shows a hex dump.
4. Send a 2 MiB message with the default cap and confirm the connection closes with 1009.
5. Connect to a server that answers 401, and confirm the message names the status and nothing retries.
6. Turn auto-reconnect on, drop the server, and watch the backoff entries. Confirm the user's own Disconnect never reconnects.
7. Log in over HTTP so a cookie is stored, then connect to a WebSocket on that host, and confirm the handshake carries it. Untick Send cookies and confirm it does not.
8. Put a `Sec-WebSocket-Protocol` header in and confirm the server sees it.
9. Save a WebSocket into a collection that has HTTP requests, restart the app, and confirm it reopens with its draft, headers and settings.
10. Export that collection as OpenAPI (all three versions) and confirm only the HTTP paths and one omission note per WebSocket.
15. Connect, disconnect, connect again. Clear Messages leaves nothing from either connection, and the connection stays open.
11. Close a connected tab and confirm the prompt, and that the server sees the socket close.
12. Confirm the Docs sub-tab shows the rendered docs and that "Edit documentation" opens the existing Docs tab.
13. Open a database copy in an **older** build and confirm it now refuses instead of running.
14. Confirm no message text appears in the app log file.

---

## 11. Risks and unknowns, and where each is retired

| Risk | Retired by |
|---|---|
| The FFI does not work on Windows, or the bundled libcurl snapshot behaves unexpectedly | 13a, before anything depends on it. The plan for the failure case is `tungstenite`, with the new-dependency list reissued. |
| Polling for frames costs latency or idle CPU | 13a measures the sleep loop against waiting on the socket. |
| Server-initiated close needs a reply from us that libcurl does not send | 13a, case 5. |
| A refused upgrade gives an unreadable status | 13a, cases 5. |
| `Channel` ordering and throughput | 13c: a test, and coalescing in the service. |
| Log memory | Assumption 6's caps, the tests in 13e, the measurement in 13h. |
| `curl-sys` upgrade changes the WebSocket API | `Cargo.lock` pins it. 13a records the version tested. |
| An older build opens a `0010` database | The `migrate` guard in 13d. |
| Cloud checks cannot link libcurl, so the FFI is unverified there | The spike and `verify.bat` run on your machine. Treat them as the only gate, as PLAN.md already says. |

---

## 12. Next step

**13a, the spike.** It needs your machine (real toolchain, real network), so the deliverable is a `spikes/ws-libcurl/` crate plus a run script, and you send back its output. Every step after it waits on that result: pass means 13b as written, fail means 13b is reissued around `tungstenite`.

13d (persistence) does not depend on the spike and can be built alongside it.

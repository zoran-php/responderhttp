# CLAUDE.md

Guidance for Claude Code (and any AI assistant) working in this repository.

---

## 1. Project Overview

**ResponderHTTP** is a desktop API client that ships as **one single executable with zero external dependencies**.

The app is a **UI shell over an embedded cURL engine**. It collects request parameters from the UI (URL, HTTP method, headers, query params, body, authentication), passes them to the cURL engine, and formats the response (status code, timing, response headers, response body) back to the user. WebSocket requests (`ws://`, `wss://`) go through the same libcurl, using its WebSocket API (§4). A response that arrives as `text/event-stream` is shown event by event while it is still arriving, rather than after it ends (§4).

**Non-negotiable constraint:** `curl` must **not** be required on the user's machine. `libcurl` is **statically linked into the binary**. Never shell out to a `curl` process, and never assume any system library is present.

| | |
|---|---|
| Shell | Tauri 2.x (Rust) |
| Frontend | React 18 + TypeScript + Vite |
| HTTP engine | `libcurl` via the `curl` crate, statically linked; WebSocket through libcurl's own WS API |
| Persistence | SQLite via `rusqlite` (bundled, statically linked) |
| Output | `app.exe` (Windows) / single ELF binary (Linux) / `.app` (macOS) |

---

## 2. Architecture

Strict three-layer separation. **Business logic never lives in a React component, and HTTP logic never lives in a Tauri command handler.**

```
┌──────────────────────────────────────────────┐
│ Presentation (React)                         │
│  components/ — dumb, props in / events out   │
│  features/   — feature UI + local state      │
└──────────────────┬───────────────────────────┘
                   │ typed service functions only
┌──────────────────▼───────────────────────────┐
│ Frontend application layer (TypeScript)      │
│  services/ — orchestration, no JSX           │
│  lib/      — pure helpers, parsers, mappers  │
│  types/    — shared DTOs mirroring Rust      │
└──────────────────┬───────────────────────────┘
                   │ invoke() — the ONLY boundary crossing
┌──────────────────▼───────────────────────────┐
│ Tauri commands (thin adapters)               │
│  commands/ — validate, map DTO, delegate     │
└──────────────────┬───────────────────────────┘
┌──────────────────▼───────────────────────────┐
│ Rust domain + infrastructure                 │
│  domain/         — models, traits, errors    │
│  http/           — libcurl executor          │
│  persistence/    — SQLite repositories       │
└──────────────────────────────────────────────┘
```

### Rules for the boundary

- The React side talks to Rust **only** through functions in `src/services/`. No component calls `invoke()` directly.
- Every `invoke()` call is wrapped in a typed service function with an explicit return type and explicit error mapping.
- Tauri commands are **adapters**: parse input, call a domain service, map the result. No `curl` calls, no SQL, no `if` chains of business rules inside a command.
- DTOs crossing the boundary are defined once in Rust with `#[derive(Serialize, Deserialize)]` and mirrored in `src/types/`. Use `#[serde(rename_all = "camelCase")]` so TypeScript stays idiomatic.

---

## 3. Repository Layout

```
/
├── src/                          # React frontend
│   ├── components/ui/            # shadcn/ui primitives (generated — do not hand-edit)
│   ├── components/               # shared presentational components
│   ├── features/
│   │   ├── request-builder/      # URL bar, method select, headers/params tables, body
│   │   ├── response-viewer/      # status bar, headers table, Monaco body viewer, live SSE events
│   │   ├── collections/          # sidebar tree, save/rename/delete
│   │   ├── history/
│   │   ├── environments/         # variables + {{substitution}}
│   │   └── websocket/            # WebSocket builder, composer, message log
│   ├── services/                 # invoke() wrappers — the ONLY place invoke appears
│   ├── store/                    # Zustand stores (UI + session state)
│   ├── lib/                      # pure functions: validators, formatters, curl-string builder
│   ├── hooks/
│   └── types/                    # DTOs mirroring Rust structs
│
├── src-tauri/
│   ├── src/
│   │   ├── main.rs               # binary entry point — calls lib::run(), nothing else
│   │   ├── lib.rs                # bootstrap, plugin registration, DI wiring
│   │   ├── commands/             # #[tauri::command] adapters, grouped by feature
│   │   ├── desktop/              # shell only: tray, window lifecycle, single instance — no domain logic
│   │   │   ├── menu.rs           # menu bar: File > Quit, Help > Privacy Policy / Terms / About
│   │   │   ├── notices.rs        # About / Privacy / Terms dialog text — no links, fits a message box
│   │   │   ├── tray.rs           # tray icon + Show/Quit menu
│   │   │   ├── window.rs         # show/focus main window, close-to-tray
│   │   │   └── startup_error.rs  # dialog when the database cannot be opened
│   │   ├── domain/
│   │   │   ├── models.rs         # HttpRequest, HttpResponse, Auth, Collection…
│   │   │   ├── ports.rs          # traits: HttpClient, CollectionRepository…
│   │   │   ├── services/         # use-cases orchestrating ports (websocket.rs: live connections)
│   │   │   ├── ws_frames.rs      # WebSocket frame reassembly and close codes, pure
│   │   │   ├── sse.rs            # server-sent-events parser, pure
│   │   │   ├── clock.rs          # now_ms(), the one wall clock the domain uses
│   │   │   └── error.rs          # AppError + thiserror
│   │   ├── http/
│   │   │   ├── curl_client.rs    # libcurl implementation of HttpClient
│   │   │   ├── auth.rs           # auth strategies
│   │   │   ├── mapping.rs        # domain ⇄ libcurl option mapping
│   │   │   ├── curl_websocket.rs # libcurl implementation of WebSocketConnector
│   │   │   ├── cookie_websocket.rs # cookie jar for WebSocket handshakes (decorator)
│   │   │   └── curl_ws_ffi.rs    # hand-written libcurl WebSocket bindings — the ONLY unsafe
│   │   └── persistence/
│   │       ├── migrations/       # NNNN_name.sql, append-only
│   │       └── repositories/     # SQLite implementations of repo traits
│   ├── Cargo.toml
│   └── tauri.conf.json
└── CLAUDE.md
```

---

## 4. The cURL Engine

### Static linking

`src-tauri/Cargo.toml`:

```toml
[dependencies]
curl = { version = "0.4", default-features = false, features = ["static-curl", "rustls", "http2"] }
```

- `static-curl` builds and links libcurl from source into the binary.
- `rustls` avoids a system OpenSSL dependency. If a platform forces OpenSSL, use `static-ssl` — **never** dynamic linking.
- After any change to these features, verify the binary has no unexpected dynamic deps: `ldd` (Linux) / `otool -L` (macOS) / Dependency Walker (Windows).

### Design

- `HttpClient` is a **trait in `domain/ports.rs`**. `CurlClient` in `http/curl_client.rs` is one implementation. Domain services depend on the trait, never on `curl::Easy`.
- A `curl::Easy2` handle with a custom `Handler` collects the body and header stream. Do not buffer into `String` — bodies may be binary; collect `Vec<u8>` and decide encoding at the mapping layer.
- Use `curl::multi` or a blocking handle on a background task so the UI thread is never blocked. Long requests must be cancellable — support an abort token wired to libcurl's progress callback.
- Capture and return timing breakdown (DNS, connect, TLS, TTFB, total) from `Easy2::*_time()`. Users expect it, and it is free.
- Redirect following, timeout, max redirects, SSL verification toggle, and proxy settings are **per-request settings**, modelled in the domain, not hardcoded.
- `lib/curl-string-builder.ts` (frontend) generates the human-readable `curl ...` command for the "Copy as cURL" button. That is a **display artifact only** — it is never executed.

### WebSocket

WebSocket requests use **libcurl's WebSocket API** (`CURLOPT_CONNECT_ONLY = 2`, `curl_ws_recv`, `curl_ws_send`), so they share the HTTP path's TLS stack, proxy handling, CA setup and verify-TLS toggle. Nothing else was added to the binary. The decision and the spike that proved it on Windows are in `PLAN-WEBSOCKET.md` (13a).

- `curl-sys` has no bindings for that API, so `http/curl_ws_ffi.rs` declares them by hand against the libcurl that `curl-sys` builds. **It is the only file allowed `unsafe`.** `lib.rs` has `#![deny(unsafe_code)]`, and `http/mod.rs` lifts it for that one module. Every `unsafe` block carries a `SAFETY:` comment. The declarations must be re-checked against `include/curl/websockets.h` whenever `curl-sys` is upgraded; `Cargo.lock` pins the version that was proven.
- `WebSocketConnector` / `WebSocketConnection` are traits in `domain/ports.rs`. `WebSocketSessions` (`domain/services/websocket.rs`) owns the rules — registry, reconnect policy, message-size limit, close handshake — and is tested against a scripted connection with no network.
- **One owner thread per connection.** An easy handle is not thread-safe, so only that thread touches it. The UI reaches it through a message queue and a stop token. It reports back through an event sink, which the command layer backs with a per-connection Tauri `ipc::Channel`.
- libcurl behaviour the design depends on (PLAN-WEBSOCKET.md 13a findings):
  - Automatic pong is turned off, and the owner answers pings itself. libcurl's own pong is only sent lazily.
  - The owner answers a server's close frame itself; libcurl does not.
  - `CURLOPT_TIMEOUT_MS` does not limit an open connection, so only the handshake has a timeout.
- Messages that arrive after Disconnect, before the server's close answer, are still reported. Messages queued before Disconnect go out before the close frame.
- Integration tests run against the std-only local server in `src-tauri/tests/support/ws_server.rs`, never a public echo server. The one public `wss://` test is `#[ignore]`d.

### Streaming responses (server-sent events)

Any response whose `Content-Type` is `text/event-stream` is reported to the UI as it arrives. There is nothing to turn on and Send stays one button — it is the *response* that differs, not the request. The plan and its decisions are in `PLAN-SSE.md`.

- `domain/sse.rs` is the parser: pure, no clock, no I/O. It follows the WHATWG rules with two deliberate differences, written at the top of the file — a block with fields but no `data` is still reported, and a last block with no closing blank line is reported when the stream ends. Every block keeps its own `raw` lines **including the blank line that ends it**, so concatenating every block's `raw` gives the stream back byte for byte — the Raw view and the byte total both rest on that, and a unit test pins it. A blank line that ends nothing is carried onto the next block rather than dropped.
- `HttpClient::send_streaming` (`domain/ports.rs`) carries a sink and **has a default that calls `send`**, so every existing client, mock and decorator kept working untouched. `CurlClient` reports the status and headers the moment the header block ends, then each block as it is parsed, and runs `finish()` after the transfer so a stream cut off mid-block still reports what arrived.
- **The total timeout lives in libcurl's progress callback, not `CURLOPT_TIMEOUT`**, and stops applying once a response proves to be an event stream: a timeout guards *getting* a response, not keeping one. A request that never answers still times out, which two integration tests pin.
- A stream's raw body is kept to 8 MiB for the final `HttpResponse`. The events themselves are already out by then.
- The command layer backs the sink with a **per-request `ipc::Channel`**, as WebSocket does per connection. A failed send on the channel means the webview is gone, which ends the transfer. `send_request` still resolves with the whole `HttpResponse`, so history, examples and Copy as cURL are unchanged. `send_and_download` does not stream — a download is not something to watch.
- The frontend batches channel events into one store update per animation frame (`lib/event-batcher.ts`) and caps the viewer at 1 000 events / 32 MiB (`lib/capped-list.ts`): the same two guards the WebSocket log uses, shared rather than copied. A block's size crosses as `bytes` measured in Rust — `String.length` in the UI counts UTF-16 units and would undercount every multi-byte character — and both the running total and the cap weigh that number.
- `Transfer-Encoding` does not appear on an HTTP/2 response, because framing is the protocol's own. Nothing filters headers; the default `auto` HTTP version simply offers h2 over TLS. A client that shows the header was talking HTTP/1.1.
- Every response carries `TransferSizes` for the response pane's size badge. The **request** half comes from libcurl's getinfo (`request_size`, `upload_size`) — only libcurl knows what it sent, headers it added included — and so is unknown until the transfer ends; the breakdown shows an em dash for it while a stream is open. The **response** half is counted in the collector as the bytes arrive, not taken from libcurl, so that the size shown while a stream runs and the size shown once it ends are the same number. Because `accept_encoding("")` decompresses, the body figure is the decoded size, which is what the viewer holds.
- Integration tests run against a std-only local server in `src-tauri/tests/sse.rs` that writes a stream piece by piece. `httpmock` answers in one go, which is exactly what those tests must not do.

### Auth

Auth is a strategy, not a switch statement sprayed across the client:

```rust
pub enum Auth { None, Basic { .. }, Bearer { .. }, ApiKey { .. }, Custom { .. } }

trait AuthStrategy { fn apply(&self, easy: &mut Easy2<C>, req: &mut HttpRequest) -> Result<()>; }
```

Adding a new auth type must mean adding one file, not editing five.

---

## 5. Persistence

SQLite through the `rusqlite` crate (`bundled` feature, so SQLite is compiled into the binary), stored in the OS app-data directory.

> Changed 2026-09-13: this section previously named `tauri-plugin-sql`. That plugin exposes SQL to the frontend, which contradicts §11 rules 3 and 4 — repositories live in Rust, and no SQL may appear outside `persistence/repositories/`. `rusqlite` is synchronous, matching the blocking `HttpClient` trait, and statically links SQLite.

- Repository traits live in `domain/ports.rs`; SQLite implementations in `persistence/repositories/`. Domain code never sees SQL.
- Migrations are **numbered, append-only files**. Never edit a shipped migration; add a new one.
- Core tables: `collections`, `folders`, `requests`, `environments`, `environment_variables`, `history`.
- Requests store headers/params/body as JSON columns — the schema should not need a migration every time a request feature is added.
- Secrets (tokens, passwords) are **never** written in plaintext. Since Phase 9 (PLAN.md), one data key lives in the OS credential store (`secrets/keychain.rs`) and every secret is sealed with it (`secrets/envelope.rs`) before it reaches SQLite. Which fields are secrets is decided once, in `domain/secrets.rs`. History and saved responses keep none. Never add a code path that writes a secret without going through `json::encode_auth` or the environment repository.
- Every write path runs in a transaction. Every query is parameterised — string-concatenated SQL is a bug, always.

---

## 6. Frontend Conventions

- **Monaco** (`@monaco-editor/react`) for the request body editor and the response body viewer. Load it once, lazily, behind `React.Suspense`. Language is derived from `Content-Type` in a pure helper — do not put sniffing logic inside the component. Pretty-print JSON/XML through a dedicated formatter in `lib/`, and keep a "Raw" toggle.
- **shadcn/ui + Tailwind** for tabs, tables, selects, dialogs, toasts. Use the CLI to add primitives into `components/ui/`; treat them as generated code. Extend via wrapper components, do not fork the primitive.
- **lucide-react** for all icons. One icon set, no mixing, no inline SVG.
- Styling is Tailwind utilities plus design tokens from the theme. No ad-hoc hex colours, no separate CSS modules.
- Key-value tables (headers, params, form data) are **one reusable component** used in every place that needs them. If a second copy appears, that is a review failure.
- Components receive data and callbacks. A component that builds a request payload, parses a response, or formats a header is doing the wrong job — move it to `lib/` or `services/`.
- State: Zustand for app/session state, local `useState` for ephemeral UI. No global store for things one component owns.
- Every list of user-generated rows needs stable IDs, not array indices as keys.

---

## 7. Code Principles

These are enforced in review; do not ask before applying them.

**SOLID**
- *Single responsibility* — a module does one thing. `curl_client.rs` executes requests; it does not persist history or resolve environment variables.
- *Open/closed* — new auth types, new body types, new response formatters are new implementations of an existing trait/interface.
- *Liskov* — any `HttpClient` implementation (including the test mock) must be substitutable without the caller knowing.
- *Interface segregation* — small, focused traits. A repository trait per aggregate, not one `Database` god-trait.
- *Dependency inversion* — domain depends on traits; concrete `CurlClient` and SQLite repos are injected at startup in `lib.rs` (`run()`).

**DRY** — one source of truth for HTTP method lists, status-code metadata, content-type mappings, and validation rules. Shared between Rust and TS by generating TS types from Rust where practical.

**Clean code**
- Functions are short and named for intent. No `handleStuff`, no `data2`, no `utils.ts` dumping ground.
- No magic numbers or strings — named constants.
- Errors are values, not surprises: `thiserror` for domain errors in Rust, discriminated union results across the boundary. Never `unwrap()` / `expect()` outside tests and `main.rs` bootstrap.
- Never swallow an error to keep the UI quiet. Surface it as a typed failure the UI can render.
- Comments explain *why*, never *what*.

**YAGNI / KISS** — build what the current feature needs. No speculative abstraction layers.

---

## 8. Testing

- **Rust unit tests** for domain services against mock `HttpClient` / mock repositories. These must not touch the network or the filesystem.
- **Rust integration tests** for `CurlClient` against a local mock server (`wiremock` or `httpmock`). Cover redirects, timeouts, TLS failure, large bodies, binary bodies, gzip.
- **Repository tests** against an in-memory SQLite database, running the real migrations.
- **Frontend unit tests** (Vitest) for everything in `lib/` and `services/` — these are pure and should have the highest coverage.
- **Component tests** (React Testing Library) for the request builder and response viewer.
- A bug fix ships with the test that would have caught it.

---

## 9. Commands

```bash
pnpm install               # install frontend deps
pnpm tauri dev             # run app in dev mode
pnpm tauri build           # production single-file build
pnpm test                  # vitest
pnpm lint && pnpm format   # eslint + prettier

cd src-tauri
cargo test                 # rust tests
cargo clippy -- -D warnings
cargo fmt
```

`cargo clippy -D warnings` and a clean `tsc --noEmit` are required before any change is considered done.

---

## 10. Definition of Done

- [ ] `cargo clippy -- -D warnings` clean, `cargo fmt` applied
- [ ] `tsc --noEmit` clean, eslint clean
- [ ] Tests added/updated and passing on both sides
- [ ] No `invoke()` outside `src/services/`
- [ ] No business logic inside React components or Tauri command handlers
- [ ] No new runtime dependency on anything installed on the user's machine
- [ ] Release build still produces a single self-contained binary

---

## 11. Hard Rules

1. **Never** invoke a system `curl` binary. Never use `std::process::Command` for HTTP.
2. **Never** dynamically link libcurl or OpenSSL. One file, no external deps.
3. **Never** call `invoke()` from a React component.
4. **Never** put SQL outside `persistence/repositories/`.
5. **Never** edit an already-committed migration file.
6. **Never** log request bodies, auth headers, tokens, or cookies.
7. **Never** disable TLS verification by default — it is an explicit, visible, per-request user opt-in.
8. **Never** hand-edit files in `components/ui/` — regenerate via the shadcn CLI.
9. When a requirement is ambiguous, ask before inventing an abstraction.
10. **Never** write `unsafe` outside `src-tauri/src/http/curl_ws_ffi.rs`. A second exception has to be argued for in review, not slipped in.

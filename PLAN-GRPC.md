# PLAN-GRPC.md — Phase 16: gRPC requests

Written 2026-09-25. **16a–16f done and verified** (last `verify.bat` 2026-09-28: 617 library tests, 14 gRPC integration tests). **16g-1 done and verified** (`verify.bat` 2026-09-28: 623 library tests, 14 gRPC and 9 schema-repository integration tests). **16g and 16h-1 done and verified** (`verify.bat` and `release.bat` 2026-09-28: 631 library tests, 488 frontend tests). **16h done and verified** (`verify.bat` 2026-09-28: 631 library tests, 527 frontend tests). **16i-1 done and verified** (`verify.bat` 2026-09-28: 546 frontend tests). **16i done and verified** (`verify.bat` 2026-09-28: 551 frontend tests). **16j done: `verify.bat` and `release.bat` green 2026-09-28, the manual click-through (section 8) passed 2026-09-29, re-verified green with its two fixes (636 library tests). Phase 16 is done.** 16k items are each their own decision.

Companion to `PLAN.md`, `PLAN-WEBSOCKET.md` and `PLAN-SSE.md`. The same rules apply: `CLAUDE.md` wins, `verify.bat` on Windows is the only real gate, and nothing here is done until it is green on your machine.

**Naming.** The feature set is modelled on the gRPC client of the market-leading desktop API client. This document calls that product **Letterbox**, and that alias is the only name for it anywhere in this repository. Its behaviour is summarised from its public documentation, read 2026-09-25: the gRPC request interface, service definitions and saved examples.

---

## 1. What you asked for

gRPC support that mimics Letterbox: any `.proto` or server reflection, all four method types, JSON in the editor and binary on the wire, metadata and trailers visible, streaming in real time. You supplied a draft architecture (tonic + protox + prost-reflect, Tauri `Channel` for streaming). Section 3 reviews it; this plan keeps its core and changes the transport.

---

## 2. What Letterbox does, and what we take

| Letterbox feature | Here | Where |
|---|---|---|
| Server URL, **Enable TLS** lock toggle, TLS off by default | Yes (D6) | 16h, 16i |
| **Server reflection**: the service list loads automatically after the URL is entered | Yes, v1 then v1alpha | 16e |
| **Import `.proto`**: single or multi-file, with **import paths** for imports that are not relative | Yes, files read once and stored (D3) | 16b, 16f, 16g |
| Method selector populated from the service definition | Yes | 16i |
| **Message** tab: JSON editor, **Beautify**, **Use Example Message** | Yes | 16b, 16i |
| **Authorization**: API key, Basic, Bearer, OAuth 2.0 | Existing `Auth` types sent as metadata; no OAuth 2.0 (the app has none) | 16e |
| **Metadata** tab: key-value pairs | Yes, the shared key-value table | 16i |
| **Settings**: verify server certificate, include default values in the response, max response message size (0 = unlimited), connection timeout (0 = none) | Yes, and a deadline (D5). Server-name override deferred. | 16c, 16e |
| **Invoke**. For streaming methods: **Send**, **End Streaming**, and cancelling the call | Yes | 16e, 16i |
| Response: body, **Metadata**, **Trailers**, status code (`0 OK`) and time | Yes | 16i |
| Streaming: message stream with sent, received and informative entries; expand/collapse; filter; search; **Clear Messages** | Yes, sharing the WebSocket log component | 16i |
| **Save Response** as an example (only for requests in a collection) | D4 | 16k |
| **Code snippet**: the equivalent `grpcurl` command | Yes, display only, like Copy as cURL | 16h |
| Paste a `grpcurl` command into the URL bar | Later (16k) | — |
| Variables in URL, metadata and message | Yes, the existing `{{var}}` substitution | 16h |
| Scripts, mock servers, share links, comments, AI, import `.proto` from a URL, a shared working directory | **No** | — |

Two places where this plan deliberately differs from Letterbox:

- **Imported schemas are self-contained.** Letterbox resolves imports from paths on the user's disk every time. Here the resolved files are read once and stored in SQLite, so a saved request still works after the `.proto` folder moves.
- **Stream order follows the WebSocket log.** Letterbox shows the stream newest-first. One app should have one log convention, so gRPC uses whatever the WebSocket log does.

---

## 3. Review of the proposed summary

The difficulty estimate (7/10) and the four pillars are right. The static-vs-dynamic distinction is the heart of it. The corrections:

1. **Transport: libcurl, not tonic, pending a spike.** CLAUDE.md §4 makes libcurl the one engine, and Phase 13 (D1) kept WebSocket on libcurl for this reason: one TLS stack, one proxy path, one CA setup, one verify-TLS toggle. gRPC is HTTP/2 POST with length-prefixed messages and trailers, and libcurl with nghttp2 is already in the binary. The costs of tonic:
   - as far as I know it has no built-in HTTP proxy support;
   - turning TLS verification off needs a custom rustls verifier;
   - it adds `h2`, `tokio-rustls` and a tower/hyper client stack (hyper 1 and tower already appear in `Cargo.lock`, `h2` does not).

   The open question is whether libcurl hands us HTTP/2 **trailers** and runs **full-duplex** bidirectional streams. The curl maintainer has said gRPC over libcurl is "doable" with no identified show-stopper, but nobody has proved it against our build. **16a proves it or reverses D1**, exactly as 13a did.
2. **The versions are stale** (checked on docs.rs 2026-09-25):

   | Crate | Proposed | Current |
   |---|---|---|
   | tonic | 0.12 | 0.14.6 |
   | prost-reflect | 0.14 | 0.16.5, on prost 0.14 |
   | protox | 0.7 | 0.9.1 |
   | prost-types | 0.13 | 0.14 |

   `tokio = { features = ["full"] }` is not needed: Tauri already brings tokio (1.53 in the lock), and with libcurl gRPC needs no async runtime at all.
3. **`tonic-reflection` is a server crate.** A client needs only the reflection protocol's messages. We vendor `reflection.proto` (Apache-2.0, like the vendored OpenAPI schemas) and encode it with prost-reflect at runtime, using the same machinery as every other message: no code generation. The summary also names only `v1alpha`, which is deprecated. Try `grpc.reflection.v1` first and fall back on `UNIMPLEMENTED`.
4. **Reflection does not return a `FileDescriptorSet`.** It returns individual `FileDescriptorProto`s: the file containing a symbol, and then, on request, each dependency by file name. The client assembles the set itself and stops when every import is resolved. Well-known types a server leaves out are filled in from protox's bundled `google/protobuf` files.
5. **int64 is half-solved by the library.** Stringifying 64-bit integers is already the default in the proto3 JSON mapping and in prost-reflect. The other half is the frontend: if TypeScript ever runs `JSON.parse` on a message, a typed `9007199254740993` is corrupted before Rust sees it. **Rule: messages cross the boundary as JSON text, never as parsed objects.** Only Rust parses them.
6. **h2c is not a tonic setting.** tonic always uses prior knowledge on `http://`. In libcurl it is `CURL_HTTP_VERSION_2_PRIOR_KNOWLEDGE`, and TLS uses ALPN `h2`. `grpc://` is not a real scheme, only a UI convention (D6).
7. **Missing from the summary**, and all part of the protocol:
   - deadlines (`grpc-timeout`);
   - `grpc-message` percent-decoding;
   - **trailers-only** responses (an error in the header block, with no body);
   - mapping a non-200 HTTP status to a gRPC code;
   - the compressed flag;
   - cancel as `RST_STREAM`;
   - max receive size, checked from the length prefix before buffering;
   - reserved metadata keys, and base64 for `-bin` keys;
   - `Any` in JSON, which needs its type in the pool;
   - the "include default values" switch.
8. **A non-OK gRPC status is a result, not an error.** `UNAVAILABLE` from the server is shown like an HTTP 503: it is a response. `AppError::Transport` is for calls that never got a status (DNS, TLS, a refused connection).

---

## 4. Decisions — all accepted as recommended, 2026-09-27

| # | Question | Recommendation |
|---|---|---|
| **D1** | Transport | **libcurl HTTP/2 via the `curl` crate's `Multi` API, one owner thread per call.** Confirmed or reversed by 16a. If reversed: tonic 0.14 with `tls-aws-lc`, the proxy gap accepted and documented, and this plan reissued. |
| **D2** | New runtime dependencies | **`protox 0.9`, `prost-reflect 0.16` (`serde` feature), `prost 0.14`.** All pure Rust, Apache-2.0 OR MIT, and no system dependency. Binary-size delta measured in 16b and reported before continuing. |
| **D3** | Where schemas live | **A workspace-level schema library** (`proto_schemas` table, managed like environments), which requests reference by id. It holds the encoded `FileDescriptorSet` and, for imports, the source texts for viewing and re-import. The alternative, a copy inside each request, duplicates one schema across every method of a service. |
| **D4** | Saved examples and history for gRPC | **Neither in the core phase; examples in 16k.** Assumption 4 of Phase 13 set the same precedent for WebSocket, and both tables are HTTP-shaped. |
| **D5** | Deadline setting (`grpc-timeout`) | **Yes, off by default.** Letterbox has only a connection timeout, but a deadline is how a gRPC call is meant to be bounded. |
| **D6** | URL and TLS | **A host:port input plus the lock toggle, as in Letterbox.** `grpc://` and `grpcs://` are accepted when typed or pasted and set the toggle. A pure normaliser in `lib/` with tests. |
| **D7** | Test server | ~~Dev-dependencies `tonic`, `tonic-reflection` and `tokio`~~ **Revised 2026-09-27: the spike's raw h2 server (section 6, 16d).** Originally: dev-dependencies `tonic`, `tonic-reflection` and `tokio` for the integration-test server only. They never reach the release binary. An HTTP/2 server with HPACK by hand, as the std-only WebSocket server was, is not sensible. Its schema comes from `protox::compile` at test time, so no `protoc` and no build-time codegen. |
| **D8** | Import and export | **None in this phase.** OpenAPI export already reads `WHERE kind = 'http'`, so gRPC rows are excluded by construction. 16g adds a test pinning that. |

### Assumptions (confirm or correct)

1. **Cookies are not sent** with gRPC calls. gRPC does not use them.
2. **No client certificates (mTLS).** The HTTP path has none either. If wanted, add them to both paths together, in a later phase.
3. **No request compression.** We do not advertise `grpc-accept-encoding`, so a compliant server does not compress. A compressed message with an unknown encoding ends the call locally with `INTERNAL`.
4. **Max response message size defaults to 4 MiB**, gRPC's own default. There is no true "unlimited": the setting's highest value is a named cap.
5. **The stream log uses the WebSocket caps**, 1,000 entries and 32 MiB, through `lib/capped-list.ts`.
6. **Reflection runs when the URL field loses focus and when the method picker opens**, cached in memory per (URL, TLS) for the session, with a Refresh button. It sends the request's metadata and auth, since reflection endpoints are often authenticated.
7. **Windows only**, like everything else (PLAN.md Phase 0).

---

## 5. Architecture and contracts

```
React   features/grpc/*                    ── props in, events out
        features/shared/MessageStreamLog  ── extracted from WebSocketLog (16i), used by both
        store/request-store.ts (GrpcTab)
        services/grpc.ts                   ── the ONLY invoke() + Channel site
        lib/grpc-url.ts  lib/grpcurl-string-builder.ts  lib/grpc-log.ts   ── pure, tested
Tauri   commands/grpc.rs                   ── thin: parse, delegate, map
Domain  domain/services/grpc.rs            GrpcCalls (registry, owner loop, limits, deadline)
        domain/services/grpc_reflection.rs reflection conversation, dependency closure
        domain/services/proto_schemas.rs   schema library use-cases + in-memory pool cache
        domain/ports.rs                    GrpcTransport / GrpcCall, ProtoSchemaRepository
        domain/grpc_wire.rs                framing, status, trailers, metadata rules — pure
Proto   proto/compile.rs  proto/catalog.rs  proto/codec.rs  proto/example.rs  proto/reflection.rs
                                           ── protox + prost-reflect, pure, no I/O
Infra   http/curl_grpc.rs                  GrpcTransport over libcurl Multi (no unsafe expected)
        persistence/repositories/proto_schemas.rs, grpc_requests.rs
```

**Why `proto/` is a top-level module:** it has the same standing as `openapi/`, a pure mapping layer over a third-party library, with no I/O and no port. A trait in front of prost-reflect would have one implementation and no test seam, which is the speculative abstraction CLAUDE.md §7 rejects. The domain passes it bytes and descriptor handles.

**The transport port earns its place**, as WebSocket's did. `GrpcCalls` (registry, owner loop, deadline, size limit, event order) and the reflection conversation are both tested against a scripted mock call with no network.

```rust
// src-tauri/src/domain/ports.rs — shape, not final
pub trait GrpcTransport: Send + Sync {
    fn open(&self, call: &GrpcCallRequest, cancel: &CancellationToken)
        -> Result<Box<dyn GrpcCall>, AppError>;
}
pub trait GrpcCall: Send {
    fn send(&mut self, framed: &[u8]) -> Result<(), AppError>;
    fn half_close(&mut self) -> Result<(), AppError>;          // End Streaming
    fn poll(&mut self, timeout: Duration) -> Result<Option<WireEvent>, AppError>;
    fn cancel(&mut self);                                       // RST_STREAM
}
pub enum WireEvent { Headers(Vec<KeyValue>, u16), Data(Vec<u8>), Trailers(Vec<KeyValue>), Ended }
```

`GrpcCallRequest` carries the authority, the TLS flag, `/package.Service/Method`, metadata (already merged with auth), the deadline and `GrpcSettings`. The transport knows nothing of protobuf. It moves bytes and headers.

**Commands** (all thin):

```rust
// src-tauri/src/commands/grpc.rs — shape, not final
grpc_reflect(target: GrpcTargetInput) -> Result<SchemaCatalogDto, ApiError>
grpc_import_proto() -> Result<Option<ProtoImportPreviewDto>, ApiError>   // dialogs in Rust, as files.rs
grpc_save_schema(preview_id, name) / list / rename / delete
grpc_catalog(schema_ref) -> Result<SchemaCatalogDto, ApiError>
grpc_example_message(schema_ref, method) -> Result<String, ApiError>     // JSON text
grpc_invoke(call_id, request: GrpcRequestInput, on_event: Channel<GrpcEventDto>)
    -> Result<GrpcCallSummaryDto, ApiError>                               // resolves when the call ends
grpc_send(call_id, message_json: String) / grpc_end_stream(call_id) / grpc_cancel(call_id)
save_grpc_request / load_grpc_request                                      // 16g
```

**Events**, mirrored in `src/types/grpc.ts`. Every message is JSON **text** (section 3, point 5):

```ts
// src/types/grpc.ts
export type GrpcStatus = { code: number; name: string; message: string };
export type GrpcEvent =
  | { type: "responseMetadata"; at: number; metadata: KeyValue[] }
  | { type: "sent"; at: number; json: string; bytes: number }
  | { type: "received"; at: number; json: string; bytes: number }
  | { type: "streamEnded"; at: number }                    // End Streaming took effect
  | { type: "ended"; at: number; status: GrpcStatus; trailers: KeyValue[];
      by: "server" | "client"; totalMs: number }
  | { type: "error"; at: number; message: string };
```

- The status **name** comes from Rust, so the table of 17 codes exists once (CLAUDE.md §7, DRY).
- `by: "client"` marks a status we produced ourselves: deadline exceeded, message too large, or Cancel.
- `bytes` is measured in Rust, for the same reason SSE measures its bytes there.
- `-bin` metadata values cross as base64.

---

## 6. The sub-phases

**Sequencing:** 16a and 16b first, independently. Then 16c → 16d → 16e → 16f → 16h → 16i → 16j. **16g** can run beside 16d–16f and must finish before 16i. **16k** comes after the phase is verified.

### 16a — Spike: gRPC over the app's libcurl (throwaway, `spikes/grpc-libcurl/`)

Runs on your machine with the app's exact `curl` line, `Cargo.lock` and release profile, like 13a.

**Server.** The dev-dependency tonic server from D7, serving an echo service with all four method types plus reflection. It is also built here, so 16d starts from something proven.

**Gates**, each recorded with what libcurl actually did:

1. **Unary over h2c** (`V2PriorKnowledge`) and **over TLS** (ALPN `h2`, the Windows CA store).
2. **Trailers reach the `Handler::header` callback**, and can be told apart from response headers. This is the gate most likely to fail.
3. **A trailers-only response** (the server returns an error without a body): `grpc-status` arrives in the header block.
4. **Server streaming:** messages arrive as sent, not at the end. Timestamps prove it.
5. **Client streaming:** the read callback returns `ReadError::Pause`, a later `unpause_read()` sends the next message, and returning 0 ends the stream (END_STREAM).
6. **Bidirectional, full duplex:** echo each message before the next is sent, 1,000 round trips with no deadlock. Record the latency.
7. **Cancel mid-stream:** removing the handle from `Multi` resets the stream, and the server sees CANCELLED.
8. **Headers libcurl adds or omits.** No `Expect`, no `Transfer-Encoding`, `te: trailers` sent, and our `content-type` wins.
9. **A non-200 answer** (HTTP 404 from a plain HTTP server): readable status.
10. **A 16 MiB message**, both directions. Partial reads and writes handled.
11. **Proxy:** HTTP CONNECT then h2 over TLS, and h2c prior knowledge through the tunnel. Manual if no proxy is available, as in 13a.
12. **Is `unsafe` needed at all?** Everything above should fit the safe `curl` crate API. If any gate needs a raw `curl_sys` call, record which. It would go in the one FFI file, and D1 is re-discussed.
13. `check-windows.ps1`: the spike imports no DLL the app does not already import (the C-runtime caveat from 13a applies).

**Done when** every gate has a recorded outcome and D1 is confirmed or reversed.

#### 16a as written — 2026-09-27

`spikes/grpc-libcurl/`, run with `run-spike.bat`, which writes `spike-log.txt` and `spike-report.txt`. `Cargo.lock` starts as a copy of `src-tauri`'s.

- **The client** (`src/client.rs`) is the shape 16d would take: one `Easy2<Collector>` in a `Multi`, POST with no size, a read callback that pauses when nothing is queued, and trailers told apart from headers by coming after the header block's blank line. The crate sets `unsafe_code = "forbid"`, so gate 12 passes if it compiles.
- **The server is raw `h2` + tokio, not tonic.** This is a deviation from "built here, so 16d starts from something proven". The spike has to misbehave on purpose (a trailers-only answer, a plain 404, a stream that never ends) and has to log exactly which headers libcurl sent, which is awkward through tonic. 16d still uses the tonic dev-dependency from D7, because it needs real proto services and reflection.
- **Gate 1's TLS half** calls the public `grpcb.in:9001` (`/grpcbin.GRPCBin/Empty`). If the server is unreachable the gate reports SKIP, not FAIL.
- The frame decoder's unit test (a message split at every offset) passes in the cloud. Nothing else could be compiled there: the cloud has no crates.io access.

#### 16a Windows run 1 — 2026-09-27: 11 of 12 gates passed, G06 failed on a spike bug

The build compiled first time (1 min 00 s). libcurl `8.21.0-DEV`, rustls-ffi 0.15.3 / rustls 0.23.39 / aws-lc-rs, the app's exact build.

| Gate | Result | What it establishes |
|---|---|---|
| G01 | Pass | Unary over h2c (`V2PriorKnowledge`): status line `HTTP/2 200`, message echoed. |
| G01b | Pass | Unary over TLS to `grpcb.in:9001` with the Windows CA store and ALPN h2. |
| **G02** | **Pass** | **HTTP/2 trailers reach the header callback**, after the blank line that ends the header block: `status → content-type → end-of-headers → data → TRAILER[grpc-status=0]`. The deciding gate. |
| G03 | Pass | Trailers-only: `grpc-status: 5` and `grpc-message: not%20found` arrive in the header block, zero body bytes, curl result OK. |
| G04 | Pass | Server streaming arrives live: gaps of 201–216 ms against 200 ms sends. |
| G05 | Pass | Client streaming: the read callback pauses, unpausing resumes it, and returning 0 ends the stream. 3 messages, 4 pauses. |
| **G06** | **Fail** | **`curl 43` (bad function argument) on the first send.** See below. |
| G07 | Pass | Cancel by removing the handle from the multi: the server's next send fails ("stream closed"), so the stream is reset. |
| G08 | Pass | On the wire: `POST`, `content-type: application/grpc`, `te: trailers`, our `user-agent`, and libcurl's own `accept: */*`. No `Expect`, no `Transfer-Encoding`. |
| G09 | Pass | A plain HTTP 404 is readable as status 404 with its body. |
| G10 | Pass | 16 MiB out and back intact in 69 ms. |
| G11 | Skip | No proxy set. |
| G12 | Pass | Built with `unsafe_code = "forbid"`: the safe `curl` crate API is enough. |
| G13 | Pass in substance | Apart from the C runtime, the imports are exactly 13a's eight DLLs (advapi32, api-ms-win-core-synch, bcryptprimitives, crypt32, IPHLPAPI, kernel32, ntdll, ws2_32). The `VCRUNTIME140.dll` failure is the same spike artefact as in 13a: a plain cargo binary without `tauri-build`, which is what links the VC runtime statically in the app. |

**G06 diagnosis.** It was the only scenario that called `send` before the transfer had ever been pumped. G05, which pumps for 300 ms first, passed. `curl_easy_pause` needs a live connection on the handle, so before the first `perform` it returns `CURLE_BAD_FUNCTION_ARGUMENT`. The server log confirms that nothing else was wrong: the frame queued by the failed send still went out on the first read, and the server echoed it.

**Fix (in the spike, and the rule 16d inherits):** the collector records when the read callback answered `Pause`, and `send` / `end_stream` unpause **only** a read side that is actually paused. A message queued before libcurl first asks for data is simply taken by that first read. The flag is cleared before unpausing, because `curl_easy_pause` may call the read callback immediately, and that call may pause again.

**Consequences for 16d**, whatever run 2 says about G06:

- Send `Accept:` (empty) to drop libcurl's `accept: */*`. It is harmless, but it is not part of the gRPC request.
- Some servers announce their trailers in a `trailer:` response header (grpcb.in does). It is informational. The status still comes from the trailers themselves.

#### 16a Windows run 2 — 2026-09-27: every gate passed. **D1 confirmed.**

The same build with the G06 fix, 14 s incremental. Every gate from run 1 passed again with the same findings, and:

- **G06: 1,000 bidirectional round trips, each echo received before the next send, ending with `grpc-status 0`.** Full duplex over one HTTP/2 stream works with the paused read callback.
- **The round trip is slow: median 33.2 ms, p95 34.7 ms, 32.4 s for 1,000.** WebSocket's was 56 µs. The likely cause is the loop, not libcurl: after `unpause_read` the spike goes straight into `multi.wait(25 ms)`, and on Windows that wait seems to wake late. 13a found WSAPoll returning only about every 10 ms while idle. Receiving is not affected: G04's messages arrived on time, and G10 moved 16 MiB in 120 ms.
  - **It does not block anything.** A person pressing Send cannot tell 33 ms from 0.
  - **16d must measure it and fix it:** call `perform()` straight after unpausing, and wake the wait when a UI command arrives (libcurl has `curl_multi_wakeup`; whether the `curl` crate exposes it is to be checked) rather than waiting out the timeout. 16d's integration tests get a latency assertion, so a regression shows.
- G13: the same eight DLLs plus the spike-only `VCRUNTIME140.dll`, as in run 1.

**16a is done.** 16d builds on the client in `spikes/grpc-libcurl/src/client.rs`, including the paused-read rule from run 1.

### 16b as written — 2026-09-27

`src-tauri/src/proto/`, with `pub mod proto;` in `lib.rs` and the three D2 crates in `Cargo.toml` (`protox 0.9`, `prost-reflect 0.16` with `serde`, `prost 0.14`). The exact APIs were checked on docs.rs before writing, but none of it has been compiled: the cloud has no crates.io access, so `verify.bat` is the first build.

| File | What it does | Tests |
|---|---|---|
| `mod.rs` | `ProtoError` (`Schema`, `Message`, `NotInSchema`), mapped to `AppError::InvalidRequest`. A shared test fixture: two files, an import, a well-known type, all four method kinds. | — |
| `compile.rs` | `SourceMap` (import name → text), `compile(&SourceMap, roots)` → `CompiledSchema { pool, encoded }`, `load(&[u8])`. The resolver reads only the map and protox's Google files, and the Google files resolve first so they cannot be shadowed. | 7 |
| `catalog.rs` | `MethodKind`, `catalog(pool)` → services and methods with their paths and types, `find_method(pool, "/pkg.Svc/Method")`. | 4 |
| `codec.rs` | `encode(type, json)` → bytes, `decode(type, bytes, include_defaults)` → pretty JSON. Unknown fields are refused. An empty editor sends the empty message. | 9, including an `int64` of 2^53 + 1 typed as a bare number |
| `example.rs` | `example_json(type)`: every field filled, the first member of each oneof, one element per list and map, recursion stopped at depth 3, `Any` and `Value` left unset. | 4, including "every example encodes as its own type" |
| `reflection.rs` | The reflection protocol's messages, the v1 and v1alpha paths, request builders, `parse_reply`. | 4, pinned to hand-built bytes |

**Deviations from the plan, all small:**

- **No vendored `reflection.proto`.** The five messages are declared with prost's derive, with the protocol's field numbers pinned by byte-level tests. Nothing needs compiling at startup, and there is no third-party file to license.
- **Schema errors show `file:line:column`** (approved 2026-09-27). `miette = { version = "7", default-features = false }` is named in `Cargo.toml` only to read protox's position labels. It is already in the binary through protox, so it adds no code. The byte offset becomes a 1-based line and a character column, clamped to a character boundary. That conversion is tested here; the two tests that compile broken schemas pin only the line, since the exact column is protox's choice.
- **The binary-size check moves to 16f.** With LTO, code nothing calls is dropped from the release exe, so measuring now would show no change. The first honest measurement is once commands use the module.

### 16c as written — 2026-09-27

`src-tauri/src/domain/grpc_wire.rs`, with `pub mod grpc_wire;` in `domain/mod.rs`. No new dependency.

**Checked in the cloud** with the module's two outside types (`KeyValue`, `Auth`) stubbed and thiserror's `Display` replaced by a stand-in: **25 tests pass, and `cargo clippy --all-targets -- -D warnings` is clean on edition 2021.** The real crate still needs `verify.bat`.

- **Framing:** `frame(message)`, and `FrameDecoder::new(max)` with `push(chunk)` and `finish()`. The declared length is checked before anything is buffered. The limit defaults to 4 MiB and is capped at 256 MiB, because a declared length is an allocation a server asks for with five bytes. A compressed flag, an undefined flag or a body that ends mid-message is an error. `WireError::client_status()` turns a receive-side error into the status the client ends the call with (`RESOURCE_EXHAUSTED` or `INTERNAL`).
- **Status:** `Code`, all 17 codes with numbers and names, the only such table in the app. `status_of(http, headers, trailers)` takes `grpc-status` from the trailers, then from the headers (trailers-only), then maps a non-200 per the protocol's table. A 200 that is not `application/grpc` is UNKNOWN and names its content type. `percent_decode` for `grpc-message` keeps malformed escapes as written.
- **Metadata:** `check_metadata` drops empty rows and lowercases names. It refuses characters outside `0-9 a-z _ . -`, and refuses `grpc-*` and the HTTP/2 connection headers by name. Text values must be printable ASCII, and `-bin` values must be base64. `check_auth` refuses an API key placed in the query string, which gRPC does not have.
- **Request headers:** `request_headers(metadata, deadline)` returns `content-type`, `te: trailers` and `user-agent: ResponderHTTP/<version>` (replaced by a user-set one), then `grpc-timeout` and the metadata. `encode_timeout` uses the finest unit that fits eight digits, rounding up.
- **Auth stays in `http/auth.rs`.** The domain cannot depend on `http/`. So in 16d the transport turns `Auth` into headers with the existing `strategy_for`, as `CurlClient` does, and runs them through `check_metadata` with the user's metadata. Nothing is copied.

### `verify.bat`, 2026-09-27

- **Run 1:** one compile error. `DynamicMessage::descriptor()` comes from the `ReflectMessage` trait, which `codec.rs` did not import. Fixed.
- **Run 2: green.** tsc, eslint, 440 Vitest tests, `cargo fmt`, and `cargo clippy --all-targets -D warnings` all passed. `cargo test`: 563 library tests, including **all 28 of 16b and all 25 of 16c**; the integration suites were unchanged.
- **Not in run 2: the miette change.** Its commit to the repo landed as the old `compile.rs`, so run 2 built the file without the line and column. It is rewritten now, and every gRPC file on disk has been checked byte for byte against what was written. **The next `verify.bat` covers it:** three changed tests in `compile.rs` (`a_syntax_error_names_the_file_and_the_line`, `an_unknown_type_points_at_its_use`, `offsets_become_one_based_lines_and_character_columns`).
- **Run 3: green, 565 library tests.** It includes the miette change, whose three tests pass. protox does attach a position to both a syntax error and an unknown type. **16b and 16c are done.**

### 16d — before the transport: where the 33 ms comes from

The `curl` crate 0.4.50 has no `curl_multi_poll` or `curl_multi_wakeup` (checked on docs.rs). A UI command therefore cannot wake a sleeping `multi.wait`, and binding the call by hand would be a second `unsafe` file. The owner loop has to live with a bounded wait, so it matters whether that wait also delays *received* data. **Spike gate G14** (info only) measures:

- (a) a server stream sent 5 ms apart, to see whether `wait` wakes when data arrives;
- (b) 200 round trips under each of five loop styles: the baseline, `perform()` right after queuing a message, the wait capped by `curl_multi_timeout`, both together, and a 1 ms wait.

16d's owner loop is designed from those numbers.

**D7 revised 2026-09-27: the test server is the spike's raw h2 server, not tonic.** It is proven against the app's libcurl (16a), it can misbehave on purpose, and it shows the headers libcurl sent, none of which tonic makes easy. It moves to `src-tauri/tests/support/grpc_server.rs`. It answers reflection itself, using `proto/reflection.rs` and a protox-compiled schema. Dev-dependencies: `h2`, `http`, `bytes`, and `tokio` (already in the tree through Tauri); **no tonic**. `tests/grpc.rs` includes the file by `#[path]` rather than through `support/mod.rs`, so the WebSocket test binary does not compile it, and its unused functions cannot trip clippy there.

### 16d as written — 2026-09-27

Written before G14 ran, as you asked. The loop already does what G14 tests: `perform()` straight after a message is queued, and a wait capped by libcurl's own timeout. The wait length is the caller's (`poll(timeout)`), so G14's numbers only tune a constant in 16e.

**A constraint found while writing it: `curl::multi::Multi` and `Easy2Handle` are neither `Send` nor `Sync`** (curl 0.4.50, docs.rs). A call cannot be opened on one thread and handed to its owner thread. So `GrpcTransport::open` must be called **on** the owner thread, and `GrpcCall` has no `Send` bound. 16e's `GrpcCalls` moves the `Arc<dyn GrpcTransport>` into the thread and opens the call there.

| File | What it does |
|---|---|
| `domain/models.rs` | `GrpcSettings` (verify TLS, proxy, connect timeout 30 s, deadline off, 4 MiB receive limit, include defaults off), `GrpcTarget { authority, tls }`, `GrpcCallRequest`. |
| `domain/ports.rs` | `GrpcWireEvent` (`Headers`, `Data`, `Trailers`, `Ended`, `Failed`), `GrpcTransport::open`, `GrpcCall` (`send`, `end_stream`, `poll`, `cancel`). |
| `http/curl_grpc.rs` | `CurlGrpcTransport`. Headers come from `grpc_wire` (`check_auth`, `check_metadata` over the user's metadata plus `strategy_for(auth)`, `request_headers`), plus an empty `Accept:` (G08). TLS and proxy go through the existing `apply_tls_and_proxy`. It unpauses only when libcurl has paused. A header block is reported only once body data, a trailer or the end proves it is the last one, so a proxy's CONNECT answer is never taken for the response. **7 unit tests** on the header, data and read-side state machine, no network. |
| `tests/support/grpc_server.rs` | The spike server, framing with the app's `grpc_wire`. Paths under `/test.Echo/`. |
| `tests/grpc.rs` | **14 integration tests**: unary; trailers-only; an unknown method; a plain 404 mapped to UNIMPLEMENTED; server streaming arriving live; client streaming; 200 bidi round trips; cancel seen by the server; metadata, auth, `grpc-timeout` and the protocol headers on the wire with no `accept`; reserved metadata and a query API key refused before sending; an unreachable server as `Failed`; 16 MiB out and back; sending after End Streaming refused. |
| `Cargo.toml` | Dev-dependencies `h2`, `http`, `bytes`, `tokio`. |

**The latency assertion is loose for now:** median round trip under 50 ms, which catches a return to 33 ms or worse. The test prints the real median, and it gets tightened once G14's numbers and this run's are in.

Nothing here could be compiled in the cloud: the `curl` and `h2` crates are not available there.

#### G14 results — 2026-09-27: the floor is the Windows timer tick

| Loop | Median round trip |
|---|---|
| wait 25 ms (baseline) | 31.8 ms |
| `perform()` right after send | 31.9 ms |
| wait capped by libcurl's timeout | 31.9 ms |
| both | 31.9 ms |
| wait 1 ms | **15.9 ms** |

(a), the server stream 5 ms apart, arrived 15.9 ms apart. But the spike server's `tokio::time::sleep(5 ms)` is itself rounded to the tick on Windows, so (a) alone proves nothing.

(b) is conclusive. **On Windows, `multi.wait` does not return early when data arrives. It sleeps its whole timeout, rounded up to the 15.6 ms system timer tick:** 25 ms becomes two ticks and 1 ms becomes one. Sending straight after queuing and capping the wait change nothing, because the wait itself is the delay. WebSocket does not have this problem because it waits on the socket directly with WSAPoll (56 µs, 13a).

**Consequences:**

- Receiving is not throttled in throughput. Each wake drains everything buffered: 16 MiB in 73 ms.
- Latency per message is up to one tick, about 16 ms, which nobody pressing Send can notice.
- Beating the tick without `unsafe` is not possible with curl crate 0.4.50: there is no `curl_multi_poll` or `curl_multi_wakeup`.

**Decided 2026-09-27: accept the ~16 ms floor.** No new `unsafe`, and `curl_ws_ffi.rs` stays WebSocket-only. The rejected alternative was waiting on the call's socket with the WebSocket FFI's WSAPoll helper. It would have widened the one unsafe file, and needed a spike gate first, because `CURLINFO_ACTIVESOCKET` may not be valid for an HTTP/2 transfer inside a multi. 16e's owner loop polls with a short named wait (one tick), which also bounds how long a Send or Cancel from the UI waits to be picked up.

#### `verify.bat` run 1 for 16d — 2026-09-27

**Compiled first time. clippy clean, 572 library tests (the 7 new transport unit tests included), 13 of 14 gRPC integration tests.** The round-trip test passed its 50 ms bound.

**The failure was a real bug the domain would have inherited.** `a_plain_http_404_maps_to_unimplemented` fed the 404's text body (`no such thing`) to the frame decoder, which read `n` (110) as a message flag and failed with `UnknownFlag(110)` instead of reporting the 404.

**Fix:** `grpc_wire::is_grpc_response(http_status, headers)` (HTTP 200 and `application/grpc…`). Only then is the body framed; otherwise it is skipped and `status_of` explains the call from the HTTP status. The test driver follows the rule, which is the one `GrpcCalls` follows in 16e. The test now also asserts that no messages were read. There is one new unit test in `grpc_wire`, checked in the cloud: 26 tests, clippy clean.

**The round-trip bound is now 25 ms:** one tick with margin. It fails on a slide back to two ticks.

#### `verify.bat` run 2 for 16d — 2026-09-27: green. **16d is done.**

573 library tests, all 14 gRPC integration tests (the 25 ms round-trip bound included), clippy and fmt clean.

### 16e part 1 as written — `GrpcCalls` (2026-09-27)

`domain/services/grpc.rs`, plus `GrpcEvent` and `GrpcEndedBy` in `domain/models.rs`.

**Two changes from section 5, both forced by 16d:**

- **`invoke` runs the owner loop on the calling thread and returns when the call ends.** No thread is spawned. The call must be opened, and must live, on one thread (curl's `Multi` is not `Send`). The command layer already runs `invoke` on a blocking task, as `send_request` does, so that task *is* the owner. `grpc_invoke` resolving at the end matches section 5 anyway.
- **A transport failure is an `Ended` event with `UNAVAILABLE` by the client,** as gRPC clients report it, not a separate `error` event. Every call now ends with exactly one `Ended`, and `invoke` returns Err only when nothing was sent.

**The rules it owns:**

- A unary or server-streaming call sends its one message with Invoke, then ends the request stream. The client-streaming kinds take `send` and `end_stream`; a second End Streaming is harmless.
- `send` encodes before queueing, so bad JSON comes back to the caller and the call goes on. The logged `json` is re-serialized from the encoded bytes: what the server got, not what was typed.
- Only a gRPC body is framed. A received message that is not the output type ends the call with INTERNAL, and one over the limit with RESOURCE_EXHAUSTED.
- The deadline gives DEADLINE_EXCEEDED, Cancel gives CANCELLED, and a sink that refuses an event cancels the call silently. The client-side endings all cancel the transport call.
- `POLL_WAIT` is 10 ms, one Windows tick.

**17 unit tests**, against a scripted transport (echo, script, hang, trailers-only, plain 404), no network.

**`verify.bat` for part 1, 2026-09-27:** all 591 library tests passed, including the 18 in `grpc.rs`. Two gates failed on style only: `cargo fmt` (the `Sent` and `Received` variants in `models.rs`) and clippy's `useless_vec` in one test. Both were fixed on disk and checked with `rustfmt --check` here.

### 16e part 2 as written — reflection (2026-09-27)

`domain/services/grpc_reflection.rs`, plus three helpers in `proto/reflection.rs`: `describe_file` (a file's name and imports), `well_known_file` (protox's copy of a `google/protobuf/*` file), and `encode_set` (the server's file bytes joined into one `FileDescriptorSet`, stored exactly as sent).

- **`GrpcReflection::reflect(target)`:**
  - lists the services on v1, and asks again on v1alpha if v1 answers UNIMPLEMENTED; neither means "import its .proto files instead";
  - leaves out `grpc.reflection.*`;
  - asks for the file that defines each service, then follows imports in name order until nothing is missing. Well-known files come from protox and are never asked for, and the walk stops at 1,000 files;
  - proves the set loads before returning it.
- **One short call per question,** on the calling thread, like `invoke`, and for the same reason. Each question times out after 15 s. A non-OK call status is the server refusing reflection as a whole: an auth failure, typically.
- The target's metadata and auth go with every question, as assumption 6 says.
- **Tests:** 6 service tests against an in-memory reflection server built from the fixture schema: v1, v1alpha fallback, no reflection, a missing import named, only reflection listed, transport failure. 4 helper tests.

**Moved to 16g: the schema library** (import, list, rename, delete, the pool cache). It is storage almost end to end, and its repository port and SQLite repository belong with migration 0011. Building its service first would mean a port with no implementation to test it against for a whole sub-phase.

#### `verify.bat` for 16e part 2 — 2026-09-28: green. **16e is done.**

601 library tests, including the 6 reflection-service tests and the 4 new helper tests. fmt and clippy clean.

### 16f as written — commands and IPC (2026-09-28)

| File | What it does |
|---|---|
| `proto/compile.rs` | `imports_of(name, source)`: a file's imports without compiling it, with a syntax error reported at its line. 2 tests. |
| `domain/services/proto_schemas.rs` | `ProtoSchemas`: `reflect(target)`, `import_files(roots, import_paths)`, `get`, `method`, `example`. Compiled schemas are kept in memory by id. A reflected schema's id is its target's (`reflection:tls:host:port`), so reflecting again replaces it. **9 tests** (temp directories, as the OpenAPI import's tests use). |
| `commands/grpc_dto.rs` | The DTOs: target, settings (ms), request input, schema with services, methods and kinds, status with its name, events tagged by `type`, outcome. Messages cross as JSON text. 5 tests. |
| `commands/grpc.rs` | `grpc_reflect`, `grpc_choose_proto_files`, `grpc_choose_import_folder`, `grpc_import_proto`, `grpc_schema`, `grpc_example_message`, `grpc_invoke` (a per-call `Channel`, resolving at the end), `grpc_send`, `grpc_end_stream`, `grpc_cancel`, `grpc_cancel_all`. Network and disk work runs on a blocking task, which for `grpc_invoke` is the call's owner thread. |
| `lib.rs` | `AppState.grpc_calls` and `AppState.proto_schemas`, sharing one `CurlGrpcTransport`. The 11 commands are registered. |

**Import from disk:**

- Imports are looked up in the chosen import folders, then in each root's own folder, in that order, as `protoc -I` would.
- Only files the compiler asks for are read. Limits: 4 MiB per file, 32 MiB in total, 500 files.
- `google/protobuf/*` is never looked for on disk.
- An import that climbs out of the folders (`../x.proto`) is refused, with the advice to add its folder as an import path. **This differs from Letterbox**, which resolves such paths relative to the importing file. Refusing keeps import names stable, which matters once the schema is stored in 16g. Revisit if it proves annoying.

**Import is a single step for now,** with no preview-then-save: the schema is loaded for the session. Saving it to the library is 16g, where the preview step comes back.

**Binary size:** the gRPC code is now reachable from commands, so a release build carries it. It gets measured in 16j with `release.bat`, as planned.

#### `verify.bat` run 1 for 16f — 2026-09-28

It compiled. 616 of 617 library tests passed, and fmt was clean.

- **clippy:** `cloned_ref_to_slice_refs` twice in one test. Fixed with `std::slice::from_ref`.
- **One test failed on wording, not behaviour.** protox refuses `import "../money.proto"` itself, while parsing, with `shop.proto:6:8: invalid group name`, before `locate` ever runs. The import is refused, and the message points at the right line and column, but the wording is protox's. The test now pins the refusal and the position. `locate` keeps its own check, so the rule does not depend on protox. **Known quirk:** the user sees protox's wording for this case. A clearer message would mean scanning imports before protox does, which is not worth the heuristic.

### 16b — Schema core, pure (`proto/`)

Starts with the dependencies from D2. **Measure the release exe before and after**; today it is 10.93 MB (`msix-stage`, 2026-09-24).

1. `proto/compile.rs`: `compile(files: &SourceMap, roots: &[&str]) -> Result<FileDescriptorSet, SchemaError>`. Its resolver reads **only** from the in-memory map, the way the OpenAPI import's loader can read only the vendored set. Well-known types come from protox's bundled Google files (confirm the resolver name when writing). protox diagnostics become `file:line:col message`.
2. `proto/catalog.rs`: services, then methods, each with its kind (unary, server, client or bidi, from `client_streaming`/`server_streaming`) and input and output type names. It also produces the `SchemaCatalogDto` for the UI.
3. `proto/codec.rs`:
   - `encode(pool, input_type, json: &str) -> Result<Vec<u8>, CodecError>`, whose errors name the JSON path;
   - `decode(pool, output_type, &[u8], include_defaults: bool) -> Result<String, CodecError>`, which produces pretty JSON with 64-bit integers stringified;
   - `Any` resolved through the pool.
4. `proto/example.rs`, for **Use Example Message**:
   - every field set to a typed placeholder;
   - the first member of each oneof, and the first value of each enum;
   - a repeated field or map with one entry;
   - well-known types in their JSON form (`Timestamp` as RFC 3339, `Duration` as `"1s"`, wrappers as the bare value, `Struct` as `{}`);
   - recursion cut at a named depth.
5. `proto/reflection.rs`: the vendored `reflection.proto` (v1 and v1alpha, Apache-2.0, `schemas/grpc/` with its LICENSE), compiled once into a static pool. It encodes and decodes the reflection messages and nothing else.

**Tests:**

- compile a multi-file schema with a relative import and an import-path import;
- report a syntax error with its position;
- refuse an import that is not in the map;
- round-trip every scalar type, including `int64` max and min and `bytes`;
- an unknown field is an error;
- enum by name and by number;
- the example is valid for its own type (`encode(example)` succeeds) for every message in a fixture set, recursion included.

### 16c — Wire protocol, pure (`domain/grpc_wire.rs`)

1. **Framing:** a 5-byte prefix (compressed flag, big-endian `u32` length). A `FrameDecoder` accepts arbitrary chunks, checks the declared length against the maximum **before** buffering, and yields complete messages. This is the `ws_frames.rs` pattern.
2. **Status:**
   - the 17 codes with their names;
   - `grpc-status` and `grpc-message` from trailers, or from headers when the response is trailers-only;
   - `grpc-message` percent-decoded;
   - a missing status becomes `UNKNOWN`;
   - an HTTP status other than 200 maps to a gRPC code as the gRPC HTTP/2 spec lists (401 → `UNAUTHENTICATED`, 403 → `PERMISSION_DENIED`, 404 → `UNIMPLEMENTED`, 429/502/503/504 → `UNAVAILABLE`, otherwise `UNKNOWN`).
3. **Metadata:**
   - keys lowercased and validated against the spec's character set;
   - reserved keys refused, naming the key (`grpc-*`, `content-type`, `te`, pseudo-headers);
   - `-bin` values base64-encoded going out and shown as base64 coming in.
4. **Request headers** (`content-type: application/grpc`, `te: trailers`, `grpc-timeout` in the spec's unit format, a `user-agent`) come from one function, so the transport cannot forget one.
5. **Auth** becomes metadata. This reuses `http/auth.rs` only if its strategies can target a header list without an `Easy2`. If not, add a thin adapter rather than a second copy of the rules. Check in 16c.

**Tests:**

- a frame split at every byte offset;
- an oversized length is refused before allocation;
- trailers-only and normal responses;
- a percent-encoded message;
- every HTTP status in the mapping table;
- a reserved key is refused;
- a `-bin` round trip;
- the `grpc-timeout` unit boundaries.

### 16d — Transport (`http/curl_grpc.rs`)

Built on 16a's findings. One owner thread per call, holding an `Easy2` inside a `Multi`. The loop:

1. drains a command queue (send, end stream, cancel);
2. unpauses the read side when a message is queued;
3. runs `multi.perform()` and `wait()` with a short named timeout, which bounds send latency (WebSocket finding 5);
4. turns the header callback's output into `Headers` and then `Trailers`, and body chunks into `Data`.

TLS verification, the proxy and the CA store map through the same helpers as the HTTP path. Nothing is duplicated.

**Integration tests** (`tests/grpc.rs` against the D7 server, never a public one):

- all four method types;
- TLS with a self-signed certificate, refused when verification is on and accepted when it is off;
- a deadline expires;
- Cancel;
- trailers-only;
- `UNIMPLEMENTED` for an unknown method;
- a 16 MiB message;
- h2c against a TLS-only port, and the reverse, both with readable errors.

One `#[ignore]`d test against a public reflection-enabled server (candidate `grpcb.in`, to confirm it is still up).

### 16e — Domain services

1. **`GrpcCalls`** (`domain/services/grpc.rs`):
   - a registry keyed by call id;
   - it encodes JSON through `proto/codec` and frames it;
   - it enforces the maximum receive size and the deadline, producing a client-side status;
   - it emits `sent` only after the frame is written, which is the WebSocket ordering rule;
   - Unary and server-streaming calls send one message and half-close. Client and bidi calls wait for Send and End Streaming;
   - a failed event sink cancels the call.
2. **Reflection** (`domain/services/grpc_reflection.rs`):
   - `list_services`, then `file_containing_symbol` for each service, then `file_by_filename` for each missing dependency until the set is closed;
   - v1 first, and v1alpha on `UNIMPLEMENTED`;
   - each round trip is a short bidi call over the same port;
   - the result is a `FileDescriptorSet` plus its origin.
3. **Schema library** (`domain/services/proto_schemas.rs`):
   - import (compile, store sources and descriptors), list, rename, delete, re-import;
   - an in-memory `DescriptorPool` cache keyed by schema id, so each Send does not recompile.

**Tests:**

- scripted-mock tests for every method kind, the deadline, the size limit and cancel;
- reflection with a missing dependency, v1alpha fallback, a server without reflection (a clear "use a .proto file" message), and a dependency cycle that stops.

### 16f — Commands and IPC

- The commands and DTOs from section 5, with `#[serde(rename_all = "camelCase")]`.
- A per-call `ipc::Channel`, as WebSocket does per connection and SSE per request.
- `grpc_import_proto` opens the file dialogs in Rust (root `.proto` files, then optional import-path folders) and reads **only** the files the compiler asks for, within the chosen folders, capped at a named total size and file count. It returns a preview (services found, errors) before anything is saved.
- Register everything in `lib.rs`.

**Tests:** DTO mapping, and an import refusing a path outside the chosen folders.

#### `verify.bat` run 2 for 16f — 2026-09-28: green. **16f is done.**

617 library tests and 14 gRPC integration tests. fmt and clippy clean.

### 16g-1 as written — the schema library (2026-09-28)

16g is split: **16g-1** is the schema library, **16g-2** is saved gRPC requests in collections.

| File | What it does |
|---|---|
| `persistence/migrations/0011_grpc.sql` | The `proto_schemas` table and `requests.grpc_json` (default `''`), as planned below. `MIGRATIONS` is now 11. |
| `domain/models.rs` | `SchemaOrigin`, `StoredProtoSchema` (encoded `FileDescriptorSet` plus the imported sources), `ProtoSchemaSummary`. |
| `domain/ports.rs` | `ProtoSchemaRepository`: `list`, `get`, `save` (upsert), `rename`, `delete`, `users`. |
| `proto/compile.rs` | `SourceMap::to_map` and `From<BTreeMap>`, so sources round-trip through `sources_json`. |
| `persistence/repositories/proto_schemas.rs` | `SqliteProtoSchemaRepository`. `users` finds the saved requests whose `grpc_json` has `$.schemaId` equal to the id (SQLite's `json_extract`, in the bundled build). |
| `domain/services/proto_schemas.rs` | `ProtoSchemas` takes the repository. `get` falls back to the library, so a saved schema loads in a new session and is then kept in memory. `list`, `save(id, name)`, `rename`, `delete`. Saving a reflected schema gives it a library id of its own (`proto_…`), because the reflection id is per target and reflecting again would replace it. Delete is refused while a saved request uses the schema, and the error names those requests. New tests with an in-memory library mock. |
| `commands/grpc_dto.rs` | `ProtoSchemaSummaryDto`; `ProtoSchemaDto.name` (null until saved). |
| `commands/grpc.rs`, `lib.rs` | `grpc_list_schemas`, `grpc_save_schema`, `grpc_rename_schema`, `grpc_delete_schema`, registered. 15 gRPC commands in all. |
| `tests/proto_schemas.rs` | 9 repository tests on the real migrations: round trip, reflected origin, upsert, order by name, NotFound for get/rename/delete, rename, delete, `users` through `grpc_json`, and the migration default `''` on an existing row. |

Until 16g-2 no request writes `grpc_json`, so `users` is always empty in the app; the repository test writes the rows directly.

#### `verify.bat` and `release.bat` for 16g-1 — 2026-09-28: green. **16g-1 is done.**

- 623 library tests, 14 gRPC and 9 schema-repository integration tests, 440 frontend tests. fmt, clippy and eslint clean.
- **Release build:** the static-link check passes (only Windows system DLLs are imported). The exe is **11,955,712 bytes (11.96 MB)**, up from 10.93 MB before Phase 16: **about +1.0 MB (+9%)** for protox, prost-reflect, prost and miette. The installers are 5.19 MB (NSIS) and 6.46 MB (MSI). The gRPC code was already reachable from commands after 16f, so this is close to the final cost; 16j re-measures.

### 16g-2 as written — saved gRPC requests (2026-09-28)

| File | What it does |
|---|---|
| `domain/models.rs` | `GrpcSchemaRef` (`None`, `Reflection`, `Library(id)`), `GrpcRequestDraft` (the tab's request, unresolved: URL as typed, TLS flag, method path, schema, metadata, auth, message text, settings), `SavedGrpcRequest`. |
| `domain/ports.rs` | `GrpcRequestRepository`: `list_by_collection`, `get`, `save`. Rename, move, delete and docs stay on `SavedRequestRepository`, which acts by id, as for WebSockets. |
| `persistence/repositories/request_kind.rs` | `RequestKind::Grpc = "grpc"` and `parse`. With three kinds, `refuse_other_kind` now asks the row which kind it is, so the refusal names it ("request req_1 is a gRPC request and cannot be saved as an HTTP request"). The two existing callers pass their connection. |
| `persistence/repositories/json.rs` | `StoredGrpc` in `grpc_json`: `tls`, `method_path`, `reflection`, `schema_id`, `message`, `settings`. Every field is defaulted; `tls` and `verify_tls` default to true. A stored receive size above the cap is brought down to it. 4 tests. |
| `persistence/repositories/grpc_requests.rs` | `SqliteGrpcRequestRepository`. URL in `url`, metadata in `headers_json`, **auth in `auth_json`, sealed by the same cipher path as HTTP**. `method` is `POST`. The update leaves `docs_md` alone. A `Library` schema must already be in `proto_schemas`, checked in the same transaction as the write. |
| `persistence/repositories/proto_schemas.rs` | `users` reads `$.schema_id`, matching the stored name (snake_case like every other stored blob). |
| `domain/services/collections.rs` | `CollectionContents.grpc_requests`; `save_grpc_request` (`req_` ids, name validation) and `load_grpc_request`. 3 tests. |
| `commands/grpc_dto.rs`, `commands/dto.rs` | `GrpcSchemaRefDto` (tagged by `kind`, `schemaId` for the library), `GrpcRequestDraftDto`, `SavedGrpcRequestDto` (with `secretState`), `SaveGrpcRequestInput`; `CollectionContentsDto.grpcRequests`. 1 test. |
| `commands/collections.rs`, `lib.rs` | `save_grpc_request`, `load_grpc_request`, registered. |
| `src/types/grpc.ts`, `src/types/collections.ts` | The saved-request half of the TS contract, and `grpcRequests` on `CollectionContents` (fixtures updated). `tsc --noEmit` clean. The rest of `types/grpc.ts` is 16h. |
| `tests/grpc_requests.rs` | 10 tests on the real migrations: round trip (library and reflection), **the token is not in `auth_json`**, an unsaved schema refused with nothing written, kind refusal both ways, each repository sees only its kind, **OpenAPI export of a mixed collection leaves the gRPC request out** (D8), rename/move/docs/delete by id with docs kept on re-save, collection delete cascades, a saved request is a user of its schema. |

**Why a session schema is refused:** an imported schema not saved to the library is gone after a restart, which would leave a saved request that cannot open. The UI (16i) saves the schema to the library first, or asks for a name for it, then saves the request.

**Left for 16i:** the sidebar does not draw gRPC requests yet (`collection-tree.ts` ignores `grpcRequests`), and `docs-title.ts` does not look them up, so a gRPC request's Docs tab would say "Untitled". Both are UI.

### 16h-1 as written — the frontend contract and pure logic (2026-09-28)

16h is split: **16h-1** is the types, the service and the pure `lib` files; **16h-2** is the `GrpcTab` in the request store, which uses all of them.

| File | What it does |
|---|---|
| `src/types/grpc.ts` | The whole TS contract, mirroring `grpc_dto.rs`: settings (with `DEFAULT_GRPC_SETTINGS` equal to Rust's default), target, request input, draft and saved request, schema reference, methods, services, schemas and summaries, status, events, outcome. **Differs from the section 5 sketch** in two places, which the Rust side already settled: events carry `atMs`, not `at`, and there is no `error` event — a call that fails after starting ends with an `ended` event whose `by` is `client`. |
| `src/services/grpc.ts` | The only `invoke` and `Channel` site for the gRPC commands: reflect, the two choosers, import, schema get/list/save/rename/delete, example message, invoke (one `Channel` per call, events batched per frame with everything but messages flushed at once, as WebSocket does), send, end stream, cancel, cancel all. Logs the target, method path and status code/name only. |
| `src/services/collections.ts` | `saveGrpcRequest`, `loadGrpcRequest`. |
| `src/lib/grpc-url.ts` | D6. `splitGrpcScheme` takes `grpc://`, `grpcs://`, `http://` or `https://` off and says what it means for the lock. `grpcTarget` turns the field and the lock into `{ authority, tls }`: a scheme in the text wins, **no port means 443 with TLS and 80 without**, one trailing `/` is dropped, a path is refused with a pointer to the method list, and IPv6 goes in brackets (a bare address is bracketed). |
| `src/lib/grpcurl-string-builder.ts` | The Code snippet. `-plaintext` follows the lock and `-insecure` follows verification (never both). Metadata, then auth as `-H`, built by the same rules as `http/auth.rs` (grpcurl has no `-u`, so Basic is the header it becomes; a query-string API key adds nothing). `-d`, `-connect-timeout`, `-max-time` for a deadline, `-max-msg-sz`, `-emit-defaults`, and `-proto` for each file of an imported schema. There is no proxy flag, because grpcurl reads `https_proxy`. |
| `src/lib/grpc-log.ts` | The stream log with the WebSocket caps: entries per call, direction, informative lines ("Call ended by the app, 4 DEADLINE_EXCEEDED: … (1.2 s)"), filter and search (metadata and trailers included). `applyPendingSends` keeps secrets out: a `sent` event carries Rust's re-encoded JSON, secrets included, so each one is paired in order with the message the store sent, and the display text is shown whenever a secret was substituted into it. |
| `src/lib/variables.ts` | `substituteGrpcDraft`: URL, metadata, auth and message, as text. |
| Tests | `grpc-url` 12, `grpcurl-string-builder` 10, `grpc-log` 12, `variables` +3, `services/grpc` 9, `services/collections` +2. |

**Checked here:** `tsc --noEmit`, eslint and Prettier are clean on your machine. vitest cannot run from here (its native Linux binaries are not installed, and the package registry is blocked), so the four `lib` test files were run through a small stand-in runner: 54 of 54 pass. The service tests run for the first time in `verify.bat`.

#### `verify.bat` and `release.bat` for 16g-2 and 16h-1 — 2026-09-28: green. **16g and 16h-1 are done.**

631 library tests; 14 gRPC, 10 saved-gRPC-request and 9 schema-repository integration tests; 488 frontend tests in 51 files. fmt, clippy, eslint and tsc clean. The release build passes the static-link check.

### 16h-2 as written — the gRPC tab in the store (2026-09-28)

| File | What it does |
|---|---|
| `src/lib/json-reformat.ts` | **Beautify for the gRPC message without `JSON.parse`.** It copies every token exactly as written and re-indents, so `9007199254740993` stays `9007199254740993` (section 3, point 5). Invalid JSON is refused with its line and column. 6 tests. |
| `src/lib/grpc-request.ts` | The tab's saveable shape (`GrpcRequestDraft`) and its dirty check; `schemaRefFor` (an unsaved reflected schema is recorded as "reflection", anything else by id); `findMethod`; `clientStreams` / `serverStreams`. 13 tests. |
| `src/store/request-store.ts` | `GrpcTab` in the tab union: URL and lock, method, metadata rows, auth with its secret state, message, settings, schema (reference, loaded catalog, status, error), call state (`idle` / `running` / `cancelling`), End Streaming, the response pane (metadata, last message, count, outcome), why Invoke did not start, the log with filter and search, the composer error. **Actions:** open blank or saved (a library schema is fetched on open; **a reflected one is not, so opening a tab sends nothing**, assumption 6), the field setters, `normalizeGrpcUrl` (D6), Beautify, method pick, reflect, import, `setGrpcSchema`, `fillExampleGrpcMessage`, Invoke, Send, End Streaming, Cancel, clear/filter/search, `markGrpcSaved`. Invoke refuses, with the reason on the tab, when there is no schema, no method or a bad target. A unary or server-streaming call carries its message; the client-streaming kinds open with none and use Send. An empty editor sends `{}`. Secrets are resolved only in what goes to Rust; the log gets the display text through `applyPendingSends`. Closing the tab cancels a running call. |
| `src/App.tsx`, `TabBar.tsx` | Only what the new tab kind needs to compile: a tab-strip entry, a close prompt for a running call ("Close and cancel"), `cancelAllGrpc()` at start-up beside `disconnectAllWebSockets()`, and a placeholder pane. Nothing opens a gRPC tab until 16i. |
| `src/store/grpc-tab.test.ts` | 20 tests: a unary call end to end; the outcome arriving before the `ended` event; `{}` for an empty message; a call that never starts; the three refusals; a bidirectional call with Send, End Streaming and Cancel; a refused message; no Send on a unary call; secrets resolved for Rust and absent from the log; scheme normalisation; Beautify keeping an int64; the example message; reflection with the tab's target and metadata, and its failure; a saved request opening clean with its library schema, a reflection request opening without a network call, dirty and clean again, untouched by the HTTP and WebSocket setters; closing a tab cancels its call. |

**Checked here:** tsc, eslint and Prettier are clean. With the stand-in runner (now able to mock a service module), the new store tests pass 20 of 20, the six gRPC `lib` test files pass 76 of 76, and the existing WebSocket tab tests still pass (the one the stand-in cannot run uses a matcher it lacks). `verify.bat` is the real gate.

**Found on the way:** the WebSocket composer's Beautify (`lib/beautify.ts`) used `JSON.parse`, so it rounded integers above 2^53 and turned `1e400` into `null`. **Decided 2026-09-28: use `reformatJson` there too.** `beautify("json", …)` now delegates to it, with the same two-space indent and the same `Not valid JSON:` prefix (the detail after it is now a line and column). New test in `beautify.test.ts` pins an int64, `1e400` and `1.50` passing through unchanged.

#### `verify.bat` for 16h-2 — 2026-09-28: green. **16h is done.**

631 library tests and all integration tests; 527 frontend tests in 54 files. fmt, clippy, eslint and tsc clean.

#### `verify.bat` for the Beautify change — 2026-09-28: green. 528 frontend tests.

### 16i-1 as written — the gRPC view (2026-09-28)

16i is split: **16i-1** is the tab itself, from the New menu to Save; **16i-2** is the sidebar (saved gRPC requests in the tree, New gRPC request on a collection or folder, rename, move, delete) and the Docs tab title.

| File | What it does |
|---|---|
| `src/components/MessageStreamLog.tsx` | **The shared stream log**, extracted from `WebSocketLog`: status badge, search, direction filter, Clear Messages, newest-first expandable rows, dropped-entries note. It knows no protocol; the caller passes filtered rows and how an entry looks (icon, summary, detail, time). Also `TextMessageDetail`, `CopyButton` and `PairsTable`, used by both logs. It sits in `components/` beside `KeyValueTable`, the project's place for shared components, rather than the `features/shared/` the section 5 sketch named. |
| `src/features/websocket/WebSocketLog.tsx` | Now a thin wrapper over the shared log. **`WebSocketView.test.tsx` is unchanged and passes 11 of 11.** |
| `src/features/grpc/*` | `GrpcView` (wires the store and the file choosers), `GrpcBuilder` (breadcrumb and Save; lock toggle, URL, method picker, Invoke/Cancel; Message, Metadata, Authorization, Service definition, Settings and Docs sub-tabs), `GrpcMethodPicker` (grouped by service, kind beside each method, a missing saved method kept and marked), `GrpcMessagePanel` (JSON editor, Beautify, Use Example Message, Send and End Streaming for client-streaming methods), `GrpcSchemaPanel` (server reflection, Import `.proto` files, import paths, the services and files of the loaded schema), `GrpcSettingsPanel`, `GrpcResponseView` (unary: Body / Metadata / Trailers with status, time and size; streaming: the shared log), `GrpcLog`. The Authorization tab is the existing `AuthPanel`; Docs is the existing docs panel. |
| `src/lib/grpc-status.ts` | Badge (`0 OK` in the ok tone, any other code as an error, "Running…", "Not invoked"), Invoke/Cancel, method-kind labels, the log's filter options. 5 tests. |
| `src/lib/number-input.ts` | `clampInt`, which the HTTP and WebSocket settings panels each had a private copy of; the gRPC panel would have been the third. All three import it now. 1 test. |
| `src/lib/pretty-print.ts` | **JSON pretty-printing now goes through `reformatJson` as well** (the same decision as Beautify), so the HTTP response viewer, examples and both logs show an int64 exactly as received. New test. |
| `src/store/request-store.ts` | `reflectGrpcSchemaIfStale` (assumption 6: on leaving the URL field and on opening the method list, only for a tab not on a library schema, and only when the server changed since the last reflection; Refresh forces it), `ensureGrpcSchemaSaved` (a schema imported this session goes into the library under the request's name before the request is saved), `receivedBytes` for the size. 4 new store tests. |
| `src/store/collections-store.ts` | `saveGrpcRequest`, `createGrpcRequest`, `loadGrpcRequest`. |
| `SaveRequestDialog.tsx`, `App.tsx`, `TabBar.tsx` | "gRPC request" in the New menu; the view in place of the placeholder; Save and Ctrl+S (the dialog for a new request, an overwrite for a saved one, the schema saved to the library first); the Docs shortcut. |
| `src/features/grpc/GrpcView.test.tsx` | 7 tests: a unary call end to end (with an int64 shown exactly), a non-OK status as the result, Invoke with no method, a bidirectional send / receive / End Streaming sequence, the method picker switching Send on and off, a pasted `grpcs://` moving the lock, importing from the Service definition tab. |

**Checked here:** tsc and eslint clean on all of `src/`. The stand-in runner can now render components (jsdom and Testing Library from your `node_modules`): the new view tests pass 7 of 7, the store tests 24 of 24, and the WebSocket view, Save dialog and request-store tests are unchanged and green. `verify.bat` is the real gate.

**Not done here:** the sidebar does not list gRPC requests yet (16i-2), so a saved one is reopened from its tab until then.

#### `verify.bat` for 16i-1 — 2026-09-28: green. 546 frontend tests in 57 files; 631 library tests and all integration tests. **16i-1 is done.**

### 16i-2 as written — gRPC requests in the sidebar (2026-09-28)

| File | What it does |
|---|---|
| `src/lib/collection-tree.ts` | `grpcRequests` on each folder node and `rootGrpcRequests` at the collection root, sorted by name like the other kinds; a request whose folder id does not resolve falls back to the root. 1 test. |
| `src/lib/docs-title.ts` | Finds a gRPC request's name, so its Docs tab is not "Docs: Untitled". 1 test. |
| `features/collections/tree-types.ts`, `CollectionTreeNode.tsx` | A `grpc` tree target and "new request" protocol. WebSocket and gRPC rows now share one `LeafRequestRow` (no chevron, since neither has saved responses; rename inline; context menu), instead of a second copy of the WebSocket row. gRPC rows carry the network icon. |
| `features/collections/CollectionsSidebar.tsx` | "New gRPC request" on a collection and on a folder (an empty request, saved at once and opened); a gRPC request opens from a fresh load by id; its menu is the WebSocket one (Rename, Docs, Move, Delete), all acting on the row by id; the delete prompt names it. |
| `features/sidebar/Sidebar.tsx`, `App.tsx` | `onOpenGrpc` down to the sidebar, to `openSavedGrpcRequest`. The loaded gRPC request is highlighted in the tree like the others. |
| `features/collections/CollectionsSidebar.test.tsx` | New, 3 tests: a saved gRPC request listed with its icon and opened from a fresh copy; created from the collection menu with an empty draft and opened; renamed through `rename_request`. |

**Checked here:** tsc and eslint clean on `src/`. With the stand-in runner: the new sidebar tests 3 of 3, the tree and docs-title tests 8 and 8, and the gRPC view and Save dialog tests unchanged and green.


#### `verify.bat` for 16i-2 — 2026-09-28: green. 551 frontend tests in 58 files; 631 library tests and all integration tests. **16i is done.**

### 16j as written — hardening and documentation (2026-09-28)

**Two gaps found while checking section 8 against the build, both fixed here:**

- **The grpcurl snippet had no button.** `lib/grpcurl-string-builder.ts` existed (16h), but nothing showed it. `RequestToolbar` gains an optional **grpcurl** button (as it has cURL for HTTP); `grpcurlCommand()` in the store builds it from the resolved call, with `-proto` for each file of an imported schema and nothing for a reflected one, and puts the reason on the tab when there is no method or the target does not parse. 2 store tests.
- **The OpenAPI export dialog counted only WebSocket requests** as left out. `nonHttpOmissionNotice(webSocketCount, grpcCount)` now names both ("1 WebSocket request and 1 gRPC request will not be exported…"); the WebSocket-only sentence is unchanged. 1 unit test, 1 dialog test.

**One privacy fix:** a refused message (`grpc_send`, or `grpc_invoke` failing to start) was logged with its error text, and a JSON mapping error can quote the value it could not read. Those two are now logged by error kind only; the UI still gets the whole error. 1 service test pins that a value in the error never reaches the log.

**Documentation:**

| File | Change |
|---|---|
| `CLAUDE.md` | §1 a gRPC sentence and a "gRPC schemas" row in the stack table (protox, prost-reflect, prost, miette); §3 `features/grpc/`, `components/` naming `MessageStreamLog`, `domain/grpc_wire.rs`, `http/curl_grpc.rs`, `proto/`; **a new §4 "gRPC" subsection** — the layers, one owner thread per call (Multi is not `Send`), the libcurl behaviour the design depends on (trailers in the header callback, unpause only when paused, cancel by removal, the empty `Accept:`, never framing a non-gRPC answer, the ~16 ms Windows tick), JSON text across IPC and `reformatJson`, the schema library, secrets and the log, the test server; §5 `proto_schemas` and the three request kinds. |
| `store/privacy-policy.md`, `docs/privacy-policy.html`, `desktop/notices.rs` (Privacy) | Dated 28 September 2026. gRPC requests and **gRPC schemas** listed among what is stored; gRPC calls **and server reflection** go only to the address entered; gRPC messages and metadata kept out of the log; importing `.proto` files reads the chosen files and what they import, only from their own folders and the chosen import folders. |
| `docs/terms.html`, `desktop/notices.rs` (Terms) | Dated 28 September 2026. protox and prost-reflect named among the third-party components. |
| `desktop/notices.rs` (About) | "HTTP, WebSocket, gRPC and server-sent events". All three dialogs stay under the 2,000-character limit the tests pin (Privacy 1,438, Terms 1,414). |
| `store/listing.md` | Short description, description (a paragraph and a feature bullet), two product features (18 of 20), the privacy paragraph, and the runFullTrust justification (gRPC calls, `.proto` files). |
| `docs/index.html` | The meta description, the intro, the engine sentence and a gRPC feature bullet. The site has no gRPC section of its own yet. |

**Checked here:** tsc and eslint clean. With the stand-in runner: store 26 of 26, view 7 of 7, service 10 of 10, export dialog 3 of 3, export notice 3 of 3. The notices text change is Rust string content only; its tests (no links, length) run in `verify.bat`.

**Still to do for 16j, on your machine:**

1. `verify.bat`.
2. `release.bat`: the import list should stay the 29 system DLLs, and the exe size is reported against 16g-1's 11.96 MB (10.93 MB before Phase 16).
3. The manual click-through in section 8.

**Store search term (resolved 2026-09-29):** the pre-existing search term naming another product was replaced by `grpc client`. No other product is named anywhere in the project; the only reference app is the alias Letterbox.

#### `verify.bat` and `release.bat` for 16j — 2026-09-28: green.

- 631 library tests; 14 gRPC, 10 saved-gRPC-request and 9 schema-repository integration tests; 556 frontend tests in 58 files. fmt, clippy, eslint and tsc clean.
- **Static-link check: PASS, 29 imported DLLs, all Windows system DLLs** — the same list as before Phase 16.
- **Exe: 12,029,952 bytes (12.03 MB)**, against 10.93 MB before Phase 16: **+1.10 MB (+10%)** for the whole phase. Of that, +1.03 MB came with the Rust gRPC code (16g-1, 11.96 MB) and +0.07 MB with the UI. Installers: NSIS 5.21 MB, MSI 6.50 MB.
- **Left for 16j:** the manual click-through in section 8, on your machine.

#### Manual click-through, step 1 — 2026-09-29: passed

Reflection and Invoke work over h2c (`grpcb.in:9000`, lock off) and TLS (`grpcb.in:9001`, lock on).

**Found:** the wrong lock for a port fails correctly but unreadably. TLS against the plain-text port gave `[35] SSL connect error (…InvalidContentType)` or `(Recv failure: Connection was reset)`, and plain text against the TLS port gave `[16] Error in the HTTP2 framing layer`. **Fixed** in `http/curl_grpc.rs` with `failure_text`: with TLS on, an SSL connect error now leads with "The TLS handshake failed. If the server uses plain text (h2c), turn TLS off."; with TLS off, CURLE_HTTP2 leads with "The server did not answer plain-text HTTP/2. If it uses TLS, turn TLS on." libcurl's text follows in brackets. Any other failure is unchanged. 3 unit tests. Step 5's "TLS on against an h2c server" now shows the first hint.

#### Manual click-through, step 2 — 2026-09-29: passed

The multi-file import with an import path, invoking SayHello on `grpcb.in:9000`, saving (the imported schema went into the library under the request's name first), and opening and invoking the saved request after the folder was renamed away all work.

**Found:** without the import path, the error named the missing file and the folders searched, but not where the import is written. **Fixed:** `proto_schemas.rs` now keeps the importing file with each queued name, and `at_import` leads the refusal with `file:line:` (for example `hello_service.proto:4: cannot find hello/messages.proto, …`), as a syntax error is led. The line comes from `compile::import_line`, which finds the `import` statement in either quote style and ignores a mention in a comment. A root file's own failure is unchanged. 2 unit tests for `import_line`; the existing import-path test now asserts the prefix.

#### `verify.bat` after the step 1 and step 2 fixes — 2026-09-29: green

636 library tests (the 3 `failure_text` and 2 `import_line` tests new), all integration tests, 556 frontend tests. fmt and clippy clean. In the app, the missing-import error now leads with the importing file and line.

#### Manual click-through, steps 3–7 — 2026-09-29: passed

- **Step 3:** Use Example Message and Invoke for all four method kinds on `grpcb.in:9000`; an `int64` above 2^53 came back unchanged.
- **Step 4:** Send and End Streaming on client-streaming and bidirectional calls; Cancel mid-stream ends the call.
- **Step 5:** an expiring deadline ends the call; TLS on against h2c shows the step 1 hint. A refused port (`:9999`, `localhost:1`) failed after 2.25–2.6 s with a 5 s connect timeout: that is Windows retrying a refused connection (libcurl `[7]`), not the timeout. A silent address (`10.255.255.1`) showed the full ~5 s timeout. Not a defect.
- **Step 6:** `x-token: {{token}}` (secret) in metadata and `{{token}}` in the message. The Sent log entry shows `{{token}}`; replies echo the value, which is the server's data. After save and restart the metadata row still reads `{{token}}` and Invoke works. The grpcurl snippet has `s3cret-abc` only with the `grpc-test` environment selected, and `{{token}}` as written with No environment.
- **Step 7:** the export dialog counts the WebSocket and gRPC requests left out; the exported file has no gRPC or WebSocket operation, host or method name.

**Noted during the click-through, decided 2026-09-29:**

- **Empty tags in the OpenAPI export — kept (your decision, 2026-09-29).** `openapi/from_collection.rs` makes a tag from every folder, so a folder holding only gRPC or WebSocket requests (`grpc`, `ws` in the test collection) appears as a tag no operation uses. Valid OpenAPI, but it leaks folder names. Predates Phase 16 (WebSocket phase). Fix: emit only tags some exported operation carries; that also drops a truly empty folder with documentation.
- **The grpcurl snippet has no `-import-path` — kept; comment corrected 2026-09-29.** The library stores sources, not folders (step 2's folder can move), so files imported from several folders need `-import-path` added by hand, or the `-proto` flags dropped when the server has reflection. The `protoFiles` comment in `lib/grpcurl-string-builder.ts` now says so instead of claiming the command runs from the import root.

### 16g — Persistence

**Migration `0011_grpc.sql`:**

- a `proto_schemas` table: id, name, origin (`import`/`reflection`), `descriptor_set BLOB`, `sources_json`, created/updated;
- `requests.grpc_json`, which holds the method path, the schema reference (library id or reflection), the draft message and `GrpcSettings`.

A gRPC row keeps its target in `url`, its metadata in `headers_json` and its auth in `auth_json`, so secrets are sealed by the existing path (CLAUDE.md §5). Its method is stored as `POST`, which is truthful. `RequestKind` gains `Grpc = "grpc"`.

**Repositories:**

- `proto_schemas.rs`;
- `grpc_requests.rs`, reading `WHERE kind = 'grpc'`;
- the collection-tree query lists the new kind.

A schema that is still referenced cannot be deleted. The UI names the requests that use it.

**Tests:** the real migrations in memory; save, load, rename, move and delete a gRPC request; a secret in its auth is sealed; **OpenAPI export of a mixed collection leaves the gRPC row out**; a newer-database refusal still fires.

### 16h — Frontend state and pure logic

- `types/grpc.ts`, `services/grpc.ts` (the only `invoke` and `Channel` site), and a `GrpcTab` in the request store.
- `lib/grpc-url.ts`: the D6 normaliser (host:port, `grpc://`/`grpcs://`, the default port rule, IPv6).
- `lib/grpc-log.ts`: events to log rows, reusing `event-batcher` and `capped-list`.
- `lib/grpcurl-string-builder.ts`: the Code snippet, display only. It shell-quotes like the cURL builder, and `-plaintext` follows the TLS toggle.
- Variable substitution in URL, metadata and message, through the existing `lib/variables.ts`. Substituted message text is still text (section 3, point 5).

**Vitest** for all four `lib` files and the service error mapping.

### 16i — UI

- **Builder row:** TLS lock, URL, method picker (grouped by service, with a badge for the method kind) and **Invoke** (Cancel while running).
- **Tabs:**
  - Message: Monaco JSON, Beautify and Use Example Message;
  - Authorization;
  - Metadata: the shared key-value table;
  - Service definition: reflection or schema library, with Import `.proto` and Refresh;
  - Settings;
  - Docs, the existing panel.
- **Streaming:** Send and End Streaming appear for client and bidi methods while the call is open.
- **Response:**
  - unary: body in read-only Monaco, Metadata, Trailers, and a status badge showing `code NAME`, the time and the size;
  - streaming: **MessageStreamLog**, extracted from `WebSocketLog` into a shared component with filter, search, expand and Clear Messages. CLAUDE.md §6 fails a second copy in review, so the WebSocket view moves to the shared component in the same step, with its tests unchanged.
- **Sidebar and New:** a gRPC item wherever New WebSocket is offered, and an icon for the kind.

**Component tests:** unary happy path, a non-OK status rendered as a result, a streaming send/end sequence, and the method picker switching Send on and off.

### 16j — Hardening and release

- `verify.bat` and `release.bat` green.
- The DLL import list is unchanged (29). Report the exe-size delta against 16b's measurement.
- The manual click-through (section 8).
- Documentation:
  - `CLAUDE.md` §1 (a gRPC line), §3 (layout), a new §4 "gRPC" subsection with the 16a findings, and the stack table;
  - the privacy policy (`store/privacy-policy.md`, `docs/privacy-policy.html` and `desktop/notices.rs`) says HTTP requests and WebSocket connections go only where you send them. Add gRPC calls;
  - the Terms' third-party list gains protox and prost-reflect;
  - the Store listing's feature list.

### 16k — After the phase (each its own decision)

- Saved examples for gRPC (D4): request, messages, trailers and status, only for requests in a collection, as in Letterbox.
- History entries for gRPC calls.
- Paste a `grpcurl` command into the URL bar.
- JSON-schema autocompletion in the message editor, generated from the descriptor.
- `grpc-status-details-bin` decoded as `google.rpc.Status`, which needs the vendored `google/rpc` protos.
- A server-name override for certificate validation (a Letterbox setting). libcurl has no direct equivalent, so it needs research.

---

## 7. Hard-rule check

| Rule | How it holds |
|---|---|
| 1. No system `curl`, no `Command` for HTTP | libcurl in-process. **No `protoc` either:** protox compiles in Rust. |
| 2. No dynamic linking | protox and prost-reflect are pure Rust. The DLL check in 16a and 16j. |
| 3. No `invoke()` outside `services/` | `services/grpc.ts` only. |
| 4. No SQL outside repositories | Two new repositories. The migration is new (0011); nothing shipped is edited (rule 5). |
| 6. No logging of bodies, auth or tokens | Messages and metadata are never logged. Log lines name the method path and the status only. |
| 7. TLS verification on by default | `GrpcSettings::verify_tls` defaults to true. The lock toggle is about using TLS, not verifying it. |
| 10. `unsafe` only in the FFI file | None expected. Gate 12 of 16a says otherwise before any is written. |
| §7 no speculative traits | One transport port with a mock seam. No trait over prost-reflect. |

---

## 8. Manual click-through (16j)

1. Reflection against a public server: `grpcb.in:9000` with the lock off (h2c), then `grpcb.in:9001` with the lock on (TLS). The test server from D7 only runs inside `cargo test`, so it is not usable here.
2. Import a multi-file `.proto` with an import-path folder. Delete the folder from disk: the saved request still works.
3. Use Example Message, then Invoke for every method kind. Try an `int64` above 2^53: it comes back unchanged.
4. Send and End Streaming on client and bidi calls, then Cancel mid-stream.
5. A deadline that expires. A wrong port. TLS on against an h2c server.
6. A secret variable in metadata survives a restart sealed, and appears in the grpcurl snippet (the **grpcurl** button beside Save) only after substitution. The log shows it as `{{name}}`.
7. OpenAPI export of a collection containing a gRPC request: the request is left out and the dialog says "1 gRPC request will not be exported".

---

## 9. Risks, and where each is retired

| Risk | Retired by |
|---|---|
| libcurl does not expose HTTP/2 trailers to the header callback | 16a gate 2. If it fails, D1 flips to tonic. |
| Pause and unpause deadlock or stall in full-duplex bidi | 16a gates 5–6 |
| Binary grows too much with protox + prost-reflect | Measured in 16b before anything is built on them |
| A server has only v1alpha reflection, or omits well-known types | 16e tests |
| `int64` corrupted in the UI | The text-only boundary rule, a Vitest test, and click-through step 3 |
| The tonic dev-dependency makes test builds slow on Windows | Measured in 16a. A separate test crate if it hurts. |
| Extracting `MessageStreamLog` regresses the WebSocket view | Its existing component tests run unchanged in 16i |

---

## 10. Next step

Phase 16 is done. The two notes under the click-through record are settled: empty tags stay, and the grpcurl snippet stays as it is with its comment corrected. Open: the 16k items.

# PLAN.md

Phased build plan for the ResponderHTTP desktop app. Each phase should be shippable and pass the Definition of Done in `CLAUDE.md` §10 before the next one starts. Phases are sequential; do not pull work forward without flagging it.

---

## Phase 0 — Project scaffold — **done**

Repository structure, build tooling, and module skeletons per `CLAUDE.md` §3. No feature logic.

- Frontend: Vite + React 18 + TS (strict), Tailwind + shadcn config, ESLint/Prettier, empty `features/*`, `services/`, `store/`, `lib/`, `hooks/`, `types/` directories.
- Backend: `src-tauri` with `curl` statically linked (`static-curl`, `rustls`, `http2`), `tauri-plugin-sql` registered (later replaced by `rusqlite`, see Phase 3), empty `commands/domain/http/persistence` module tree, base `AppError` type.
- No SQL migrations yet, no DTOs yet, no commands registered yet — those are decisions for Phase 1+.
- Desktop shell (`src-tauri/src/desktop/`):
  - System tray with Show and Quit. Show un-hides, restores and focuses the main window; Quit exits the app.
  - Closing the main window with X hides it to the tray instead of exiting.
  - Single instance via `tauri-plugin-single-instance`: a second launch shows and focuses the running instance's window instead of starting a new one.
  - Linux caveat: the tray needs `libayatana-appindicator` on the user's machine. Windows and macOS have no extra runtime dependency.
  - **Verified on Windows (2026-09-13):** close-to-tray, tray Show, tray Quit and second-launch focus all behave as specified.
- Static-link proof (`spikes/static-link-proof/`), done first because single-binary delivery is the one requirement that can't be patched late:
  - A throwaway binary with the app's exact `curl` line and release profile makes one hardcoded HTTPS request and prints the status code.
  - Each target passes when the binary's dynamic dependencies are only OS-provided libraries, and it prints a status code on a clean machine with no dev tools and no separately installed curl.
  - Windows x64: **passed** (2026-09-12). Imports 8 OS DLLs only — advapi32, api-ms-win-core-synch-l1-2-0, bcryptprimitives, crypt32, IPHLPAPI, KERNEL32, ntdll, ws2_32. No libcurl, no zlib, no OpenSSL, no VC++ runtime. Ran in Windows Sandbox (`proof.wsb`, no dev tools) and printed `status: 200`. Build was curl 8.21.0 / rustls-ffi 0.15.3 + aws-lc-rs / zlib 1.3.2 / HTTP/2 on.
  - Linux x64 and macOS arm64: **deferred, out of scope until the Windows app is done** (decided 2026-09-13). Windows is the only target being proven or built for now. When they come back, the known risks to check are: zlib may link the system `libz.so` dynamically; oldest glibc to support; CA roots location on each platform; and the tray's `libayatana-appindicator` requirement on Linux.
  - If a curl error appears after the version lines, linking is proven and the problem is configuration (e.g. CA roots), which is tracked separately.

**Findings from the Windows proof — these carry into the app, not just the spike:**

- **The MSVC C runtime must be linked statically.** Without `-C target-feature=+crt-static` the exe imports `VCRUNTIME140.dll` and the UCRT `api-ms-win-crt-*` set, and `VCRUNTIME140.dll` is not on a clean Windows install. The spike sets it via `RUSTFLAGS` in `check-windows.ps1`. Whether `tauri-build` already does this for the app is unverified — check by running `check-windows.ps1 -SkipBuild -ExePath <app.exe>` in Phase 7 (or earlier, as soon as a release build exists).
- **rustls has no trust store, so HTTPS fails by default.** libcurl built against rustls-ffi returns curl error 35 ("no server certificate verifier was configured") unless a CA source is set. `CURLSSLOPT_NATIVE_CA` (`SslOpt::new().native_ca(true)` in the `curl` crate) makes it use the OS certificate store, which keeps the binary self-contained. `CurlClient` must set this in Phase 1, and Linux/macOS need the same check — the OS store path differs per platform and a slim Linux container may have no roots at all.
- **Build-time only:** rustls-ffi pulls in `aws-lc-rs`, which needs `cmake` (and NASM on Windows) to build. This does not affect the shipped binary, but it constrains any build machine or CI runner.

**Done when:** `pnpm install`, `pnpm tauri dev` boots an empty window; `cargo check` and `tsc --noEmit` are clean; closing to tray, Show, Quit and second-launch focus all behave as described above; the static-link proof passes on Windows x64 (Linux and macOS deferred).

---

## Phase 1 — Vertical slice: send a GET request — **done (2026-09-13)**

Prove the React → `invoke()` → Tauri command → `HttpClient` trait → `CurlClient` → libcurl path end to end, on the simplest possible request.

- `domain/ports.rs`: `HttpClient` trait.
- `http/curl_client.rs`: `CurlClient`, `Easy2` + custom `Handler` collecting `Vec<u8>` body and headers, DNS/connect/TLS/TTFB/total timing via `Easy2::*_time()`.
- `domain/models.rs`: `HttpRequest` (method + URL only for now), `HttpResponse` (status, headers, body, timing).
- `CurlClient` sets `CURLSSLOPT_NATIVE_CA` — without it every HTTPS request fails at handshake (see Phase 0 findings).
- `CurlClient` also sets `CURLOPT_ACCEPT_ENCODING` to the empty string, so gzip/deflate responses arrive decompressed rather than as a binary blob.
- `CurlClient::with_timeout` exists as a test seam until timeouts become per-request settings in Phase 2.
- One command: `send_request`, thin adapter, no business logic.
- `src/types/`: mirrored DTOs (`camelCase`).
- `src/services/`: single `invoke()` wrapper.
- Minimal UI: URL bar, method select, Send button, raw response viewer (status + headers + body). No key-value tables, no body editor, no styling polish yet.
- Tests: Rust unit tests for the domain send-request service against a mock `HttpClient` (4) and for the curl mapping helpers (6); Vitest for the service wrapper (3) and the formatters (6). All passing.
- `CurlClient` integration tests against `httpmock` (6): status/headers/text body, redirect chain keeping only final headers, timeout as a transport failure, gzip decompression, binary body, and HEAD with no body. `httpmock` was chosen over `wiremock` because its API is synchronous, matching the blocking `HttpClient` trait.
- **Not covered by any test:** TLS. httpmock serves plain HTTP, so the rustls path and `CURLSSLOPT_NATIVE_CA` rest on the Phase 0 spike and manual checks. A TLS-failure test needs a self-signed local server — decide separately whether that is worth building.

**Verified end to end (2026-09-13):** `GET https://httpbin.org/get` returns 200 with the timing breakdown (DNS 47 ms · connect 123 ms · TLS 302 ms · TTFB 121 ms · 595 ms total), headers and JSON body rendered; `ftp://` is rejected as an invalid request; an unresolvable host surfaces as a transport failure.

**Note for running the app:** a debug build points at the Vite dev server (`devUrl`), so `pnpm tauri dev` (or `run-app.bat`) is required — launching `target/debug/responderhttp.exe` on its own shows "can't reach this page". Only a release build embeds the frontend.

**Done when:** typing a URL and clicking Send shows a real response from a real server, with a passing test on both sides of the boundary.

---

## Phase 2 — Full request building — **done (2026-09-13)**

- Headers/params/form-data: one reusable key-value table (`components/KeyValueTable.tsx`), used by all four. Rows carry their own ids (`lib/key-values.ts`), so React keys stay stable while editing; a blank row is always appended to type into.
- Body types: none, raw (Monaco, lazy + `Suspense`), form-urlencoded, multipart. Raw covers JSON/XML/text — the content type decides highlighting, so there is no separate JSON variant in the domain.
  - **Multipart was text parts only** until 2026-09-14, when file parts landed — see "Phase 6 extension — Multipart file parts" below. The file picker and the streaming reads both arrived with `tauri-plugin-dialog` and libcurl's form API.
- Monaco is bundled via the `monaco-editor` package rather than loaded from a CDN, which would break offline use and the no-external-dependency rule. It lands in its own 3.3 MB lazy chunk, loaded on first use.
- `lib/curl-string-builder.ts`: "Copy as cURL" — display-only, never executed, 7 tests including POSIX single-quote escaping.
- Response viewer: Pretty / Raw / Headers views. Language comes from `Content-Type` (`lib/content-type.ts`) and pretty-printing from `lib/pretty-print.ts`, which returns malformed input untouched rather than hiding what the server sent.
- Per-request settings in the domain: follow redirects, max redirects, timeout, TLS verification (off is an explicit opt-in, with a visible warning), proxy (rejected unless it carries a scheme — a bare host:port silently becomes HTTP in libcurl).
- Request cancellation: a `CancellationToken` polled by libcurl's progress callback; the service registers in-flight requests by id so `cancel_request` can reach one. A cancelled transfer reports `Cancelled`, not a transport failure.
- Also set on every request: `CURLOPT_ACCEPT_ENCODING` (gzip arrives decoded) and `Expect:` cleared (avoids the 100-continue round trip).
- Tests: 27 Rust (10 mapping, 8 service, 1 cancellation, 8 integration incl. headers/params/raw body, form-urlencoded, method preserved with a body, redirects off, mid-transfer cancel) and 25 TypeScript (curl builder, content-type, pretty-print, key-values, service wrapper).

**Verified end to end (2026-09-13):** `POST https://httpbin.org/post?q=a%20b%26c` with an `X-Api-Key` header and a raw JSON body — httpbin echoed `args.q` as `a b&c`, the header, `Content-Type: application/json` set automatically, and the parsed `json` object; response rendered pretty-printed in Monaco with the timing breakdown.

**Done when:** an arbitrary request (any method, headers, body, redirect/timeout/proxy settings) can be built, sent, cancelled mid-flight, and its response read comfortably.

---

## Phase 3 — Persistence: collections — **done (2026-09-13)**

- **SQLite via `rusqlite` (bundled), not `tauri-plugin-sql`** (decided 2026-09-13). The plugin exposes SQL to the frontend, which §11 rules 3 and 4 forbid; `CLAUDE.md` §5 was updated to match. `rusqlite` is synchronous like the `HttpClient` trait and statically links SQLite, so the single-binary guarantee holds. `@tauri-apps/plugin-sql` was dropped from package.json.
- Migration `0001_initial.sql`: `collections`, `folders`, `requests`, with headers/params/body/settings as JSON columns. Applied from `persistence/database.rs` in a transaction, tracked by `PRAGMA user_version`; `PRAGMA foreign_keys = ON` per connection, without which `ON DELETE CASCADE` is silently ignored.
- `domain/ports.rs`: `CollectionRepository`, `FolderRepository`, `SavedRequestRepository` — one per aggregate.
- `persistence/repositories/`: SQLite implementations. Saving uses `ON CONFLICT(id) DO UPDATE` so re-saving overwrites rather than duplicating; a no-op UPDATE/DELETE reports `NotFound` instead of silently succeeding.
- `domain/services/collections.rs`: use-cases with one name-validation rule for every entity, so a blank name cannot reach storage from any path.
- 13 commands (list/contents/create/rename/delete per entity, save/load/move for requests), all running on blocking tasks.
- Tests: 7 repository tests against in-memory SQLite running the real migrations (round trip through every JSON column, re-save updates, both cascade paths, move between folder and root, case-insensitive ordering, NotFound), plus 3 service tests and migration/id/timestamp unit tests.

- Collections sidebar UI: tree view (collections → folders → requests), Save on the current request, inline create, and context-menu rename/delete/move (drag-and-drop was ruled out, 2026-09-13).
- **Extended beyond the original scope** (2026-09-13, at request, no backend changes): "New request" from the collection and folder context menus, "Duplicate" on a request, and a multi-tab request builder with per-tab dirty tracking and confirm-on-close. Tabs are in-memory only and reset on launch — persisting them was considered and deferred; Phase 6 is where it would go if ever wanted.

**Done when:** a request built in Phase 1/2 can be saved into a collection, closed, and reopened with all fields intact. — **met**, confirmed by manual click-through 2026-09-13.

---

## Out-of-phase UI work (2026-09-13)

Done at request, outside the phase structure, all frontend-only:

- **"Nord Ice" theme** — the shadcn token set in `src/index.css` remapped to the supplied palette. `destructive` uses Nord's own aurora red (`#bf616a`) to fill the one slot the palette didn't name.
- **App preloader** — inline `<style>` in `index.html` (it must paint before React or `index.css` load), removed by `main.tsx` after the first render.
- **HTTP method colours** — `--method-*` tokens plus `lib/http-method-colors.ts`, applied in the collections tree, the method select, and the tab bar. GET/POST/PUT/DELETE are sampled from a screenshot; PATCH/HEAD/OPTIONS are picked to match the same family.
- **In-flight animation** — `features/response-viewer/SendingIndicator.tsx` replaced the plain "Sending…" text. Added `--frost-teal` / `--frost-deep` tokens (nord7/nord10) and two keyframes in `tailwind.config.ts`.

---

## Phase 4 — Auth strategies — **done, verified 2026-09-14**

- `domain/models.rs`: `Auth` enum (`None`, `Basic`, `Bearer`, `ApiKey`, `Custom`) plus `ApiKeyLocation` (header or query). `Auth` is a field on `HttpRequest` rather than an `Option`, so "no auth" is something the user picks and sees.
- `http/auth.rs`: `AuthStrategy` with one implementation per variant. A strategy returns the headers and query parameters it contributes (`AuthParts`) instead of mutating a curl handle, so all 13 of its unit tests run with no transport. Base64 for Basic is ~20 lines of std rather than a new crate.
- `http/curl_client.rs`: applies those parts. Query contributions join the user's own before the URL is built; a header typed into the Headers tab overrides the generated one — the same precedence Content-Type already had.
- Persistence: migration `0002_request_auth.sql` adds `auth_json`, defaulting to the serialised `Auth::None` so every request saved before this phase keeps loading. `StoredAuth` in `repositories/json.rs`.
- Auth tab (`features/request-builder/AuthPanel.tsx`), and `lib/curl-string-builder.ts` now emits `-u` for Basic and mirrors the same override rule, so the copied command matches what is actually sent.

**Verified 2026-09-14:** full `verify.bat` green — build, eslint, 88 Vitest tests, `cargo fmt --check`, `cargo clippy -- -D warnings`, 70 Rust tests. The real-server check in Done-when is still worth doing by hand.

**Secrets — open decision.** Auth values persist in `auth_json` in the app-data SQLite, the same way headers always have: a token typed into the Headers tab has been stored in plaintext since Phase 3, so this phase does not change the threat model. The OS-keychain option in `CLAUDE.md` §5 needs a new dependency and makes collections non-portable between machines, so it was left for an explicit decision rather than assumed. Export-time exclusion is not implemented because there is no export feature yet.

**Decided 2026-09-16:** a data key in the OS keychain, with secrets encrypted in SQLite. See Phase 9.

**Done when:** all four auth types apply correctly against a real server and round-trip through save/load without leaking into logs or plaintext exports.

---

## Phase 5 — Environments & variables — **done, verified 2026-09-14**

**Decided 2026-09-14, before building:** no global tier — environments only, so resolution is one step. The variable editor opens as a tab beside requests rather than replacing the main panel. Which environment is *active* for substitution is a separate control from which one is open for editing, so you can edit Staging while sending against Production.

Built:
- Migration `0003_environments.sql`: `environments`, and `environment_variables` keyed on `(environment_id, position)` so the editor's row order survives a reload. The primary key's leading column already indexes lookups by environment, so there is no second index.
- `domain/ports.rs`: `EnvironmentRepository`. Variables sit behind the environment's own trait rather than a second one — they are part of that aggregate.
- `domain/services/validation.rs`: `validated_name` moved out of `collections.rs`, which had it privately. Two callers now, one rule (CLAUDE.md §7, DRY).
- `persistence/repositories/environments.rs`: `set_variables` replaces the whole set in a transaction, checks the environment exists first (an unknown id would otherwise look like success whenever the set is empty), and drops blank names — those are the editor's trailing row, not an error. 5 repository tests.
- 6 commands, wired in `lib.rs` alongside the collections ones.
- UI: sidebar split into **Collections | Environments** panels (`features/sidebar/Sidebar.tsx`), `EnvironmentsPanel` for create/rename/delete and opening one, `EnvironmentEditor` reusing the one `KeyValueTable`, `EnvironmentSelector` in the tab strip for the active environment.
- The tab store is now a union of `RequestTab | EnvironmentTab`. `replaceActiveTab` patches only request tabs, which makes every field setter a no-op while an environment tab is in front instead of each one needing a guard.

- `lib/variables.ts`: the substitution engine, pure and tested (14 tests). An unknown name is left exactly as written rather than blanked — a request that visibly asks for `{{token}}` is easier to diagnose than one that silently sends `Bearer `. Substitution is a **single pass**, so a value containing `{{x}}` is not expanded again; that makes a self-referencing variable impossible to hang on. Duplicate names resolve to the **first** row, since that is the one nearest the top of the editor.
- Applied on the way out only: `send()` and Copy-as-cURL resolve, `currentInput()` does not. A **saved request keeps its placeholders**, so the same request can be sent against a different environment tomorrow — resolving before save would have baked one environment into the stored row.
- `setActiveEnvironment` now loads that environment's variables. Without it, activating an environment whose editor had never been opened would substitute nothing, because variables are otherwise fetched lazily.
- `activeEnvironmentId` resets on launch (like open tabs) rather than silently pointing at production after a restart.

**Verified 2026-09-14:** full `verify.bat` green on the second run. The first run found four real problems, all fixed: a `SendRequestInput` fixture missing `auth` in `collection-tree.test.ts`, three places where the tab union reached `request-store.test.ts` unnarrowed, an eslint unused-binding in `environments-store.ts`, and six `cargo fmt` hunks.

**Note on the tooling gap:** the Rust for the rename, Phase 4 and Phase 5 was written without a compiler (`device_bash` has been broken since the 2026-09-08 Windows update) and still came through clippy and 70 tests clean. The TypeScript checks done remotely are weaker than they look — they cannot see through `zustand`, so anything reached via a store is effectively unchecked until `verify.bat` runs. Treat `verify.bat` as the only real gate.

**Not done, deliberately:** secret-flagged variables. They depend on the Phase 4 secrets decision (settled 2026-09-16: envelope encryption, Phase 9) — every variable is currently stored in plaintext in the app-data SQLite, exactly like request headers and auth values.

**Done when:** a request using `{{base_url}}/{{token}}`-style placeholders resolves correctly per active environment, with a test covering missing-variable and override behavior.

---

## Phase 5 extension — Cookies — **done, verified 2026-09-14**

Scope added on request, before Phase 6. It is unrelated to environments; it sits
under Phase 5 only because that is where it was asked for.

### The constraint that shaped this

The `curl` crate exposes `CURLOPT_COOKIE`, `CURLOPT_COOKIEFILE`, `CURLOPT_COOKIEJAR`,
`CURLOPT_COOKIESESSION` and `CURLOPT_COOKIELIST` as **setters**, but there is **no
`CURLINFO_COOKIELIST` getter** — libcurl's in-memory jar cannot be read back from
Rust. Verified against docs.rs, twice. So "let libcurl own the jar" would force the
cookie manager to parse libcurl's Netscape file, and since `CurlClient` builds a
fresh `Easy2` per request on concurrent blocking tasks, every send would be writing
that one file at once.

### Decisions (2026-09-14)

- **We own the jar, in SQLite.** `Collector::header` already sees `Set-Cookie`, so
  capture costs no new plumbing. Matching becomes a pure, tested function — the same
  shape as `variables.ts` and `http/mapping.rs`.
- **Cookie manager is a dialog** opened from the request bar, not a third sidebar tab.
- **Per-request `sendCookies` toggle** in the existing Settings tab.
- **Session cookies (no Expires/Max-Age) are dropped on launch**, per RFC 6265.

### Known limitations, accepted up front

- **No public suffix list.** A server at `foo.co.uk` setting `Domain=co.uk` would be
  accepted. Guarded only by requiring the Domain to contain a dot and to be a proper
  suffix of the request host. Acceptable for a developer tool pointed at servers the
  user chose; it would not be acceptable in a browser.
- **`Expires` parsing without a date crate.** `Max-Age` is preferred (a plain
  integer). `Expires` is parsed as IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`) only;
  anything else falls back to treating the cookie as a session cookie, which errs
  toward expiring sooner rather than later.
- **Copy-as-cURL will not include jar cookies.** The jar lives in Rust and the builder
  is a pure frontend function. Fixing it means a `cookies_for_url` command; left as a
  follow-up rather than widening this round.

### Built as planned, with two changes

- **The jar is a decorator, not a change to `SendRequest`.** `http/cookie_client.rs`
  implements `HttpClient` and wraps `CurlClient`, so `curl_client.rs` still only
  executes requests and `SendRequest` still only validates and cancels (CLAUDE.md §7).
  Nothing upstream knows cookies exist — the Liskov rule in §7 doing real work — and
  `SendRequest`'s eight tests were untouched.
- **Times are unix seconds, not ISO strings.** The other tables store `created_at` as
  ISO text, but an expiry is compared on every request rather than displayed, and
  integers keep `domain/cookies.rs` free of any date handling at all.

### Verified

Full `verify.bat` green: build, eslint, 88 Vitest, `cargo fmt --check`,
`cargo clippy -- -D warnings`, and **108 Rust tests** (80 unit including all 28
cookie-domain and 6 decorator tests, 11 curl integration, 17 repository).

Three rounds were needed, and each failure is worth remembering:
1. `lib.rs` called `clear_session()` on a concrete `Arc<SqliteCookieRepository>` without
   `CookieRepository` in scope — everywhere else the repo coerces to `Arc<dyn …>` when
   passed to a service, so the startup purge was the only bare trait call.
2. `unix_now` sat below `mod tests` (`clippy::items_after_test_module`).
3. The scheme check in `request_target` tripped `clippy::question_mark`.

Numbers 1 and 2 were found locally first: `http/cookie_client.rs` depends only on
domain types, so it plus `domain/{models,cookies,ports,cancellation}.rs` were compiled
in a scratch crate with a stubbed `AppError` (thiserror is unavailable offline) — 34
tests and clippy. Worth keeping that harness; it turns the highest-risk half of this
feature into the best-tested Rust in the project.

**Toolchain gap:** local clippy is 0.1.95, the project's is 1.97.0. Number 3 only fires
on the newer one. Remote Rust checks are a subset of `verify.bat`, never a substitute.

### Steps, in dependency order

1. **Migration `0004_cookies.sql`.** `cookies(name, domain, path, value, expires_at,
   secure, http_only, host_only, created_at)` with `PRIMARY KEY (domain, path, name)` —
   the RFC 6265 identity triple, so re-setting a cookie upserts rather than duplicates.
   `expires_at IS NULL` means a session cookie. Bump `MIGRATIONS` to 4.
2. **`domain/models.rs`:** `Cookie` struct. Also add `send_cookies: bool` to
   `RequestSettings` (default true).
3. **`domain/cookies.rs` (new, pure):** `parse_set_cookie`, `default_path`,
   `domain_matches`, `path_matches`, `cookies_for_request` (filter + RFC 6265 §5.4.2
   ordering: longest path first, then oldest). No I/O, no libcurl — this is where the
   security-relevant logic lives and where the test table goes.
4. **`domain/ports.rs`:** `CookieRepository` — `matching(url)`, `store(cookie)`,
   `list()`, `delete(domain, path, name)`, `clear()`, `clear_session()`.
5. **`persistence/repositories/cookies.rs`:** SQLite implementation. Upsert keeps the
   original `created_at` (RFC 6265 §5.3 step 11). Expired rows filtered on read and
   purged at startup.
6. **`http/curl_client.rs`:** `Collector` accumulates `Set-Cookie` lines across *all*
   redirect hops into a separate `Vec`, and `HttpResponse` gains `set_cookies`. Needed
   because the collector deliberately clears headers on each new status line, so a
   cookie set on a 302 — the normal login flow — is currently thrown away. The DTO is
   unchanged; the frontend has no use for the raw lines.
7. **`domain/services/send_request.rs`:** the orchestration point. Before send, unless
   `settings.send_cookies` is false or the user set their own `Cookie` header, attach
   the matching cookies. After send, parse `set_cookies` and store. Cookie handling
   belongs here and not in `curl_client.rs`, which per CLAUDE.md §7 executes requests
   and does not persist anything.
8. **`commands/cookies.rs`:** `list_cookies`, `delete_cookie`, `clear_cookies`. Wire
   into `lib.rs` with a `Cookies` service; call `clear_session()` once at startup.
9. **Frontend:** `types/cookies.ts`, `services/cookies.ts`, `store/cookies-store.ts`,
   `features/cookies/CookieManagerDialog.tsx`, a Cookies button in `RequestBuilder`,
   and a `sendCookies` checkbox in `SettingsPanel`.

### Backward compatibility to get right

`send_cookies` lands in the `settings_json` column and in `RequestSettingsDto`, both of
which already have rows and payloads written without it. Both need an explicit
`#[serde(default = ...)]` returning **true** — `#[serde(default)]` on a bool gives
false, which would silently turn cookies off for every request saved before this phase.

### Tests

Pure Rust: the parse and match table — attributes present and absent, `Max-Age` beating
`Expires`, an unparseable date degrading to a session cookie, a `Domain` the host does
not match being rejected, a `Domain` without a dot being rejected, secure-only over
http, path prefix boundaries (`/foo` must not match `/foobar`), and ordering.
Repository: upsert on the identity triple preserves `created_at`, session purge,
expired purge. Service: header attached, user's own `Cookie` header wins, `sendCookies:
false` skips, and a cookie set on a redirect hop is captured.

### Outstanding: manual click-through (not done as of 2026-09-14)

Automated verification is complete, but **no part of the cookie feature has met a real
server**. Deliberately deferred to move on to Phase 6; it remains the gap between
"passes its tests" and "works". Still to do by hand:

- Log in somewhere that sets its cookie on a **302**, then send a second request to the
  same host and confirm it goes automatically. This redirect path is what the previous
  code silently discarded, and no test exercises a real multi-hop response.
- Confirm the manager lists it under the right domain and path, and that delete works.
- Untick "Send stored cookies" on one request and confirm it is omitted.
- Restart: a session cookie should be gone, one with an expiry should survive.
- Open a request saved **before** 2026-09-14 and confirm "Send stored cookies" is still
  ticked — that is the `#[serde(default = "enabled")]` guard, and if it were wrong it
  would fail silently rather than loudly.

The same applies to the Phase 4 auth types and the Phase 5 substitution engine: neither
has been exercised against a real server either.

**Done when:** a login request that receives `Set-Cookie` on a 302 leaves a cookie in
the jar, a following request to the same host sends it automatically, the manager can
show and delete it, a request with `sendCookies` off omits it, and session cookies are
gone after a restart.

---

## Out-of-phase UI work (2026-09-14)

From the first manual click-through of the Phase 3–5 UI:

- **Press feedback on every button.** `index.css` now carries one `@layer base` rule giving all buttons a transition, an `:active` scale, a `:focus-visible` ring and a disabled state, so a new button cannot be added without it — which is how cURL and Save ended up with no feedback at all. The buttons that had no hover colour either (cURL, Save, Send, Cancel, both ConfirmDialog buttons, the environment Save) got explicit `hover:`/`active:` backgrounds.
- **"New request" / "New folder" now expand their target first.** The inline name input renders among a node's children, so on a collapsed collection or folder it was invisible until you expanded by hand. `expandCollection`/`expandFolder` were added to the collections store — idempotent, unlike `toggle`, because creating inside a node must open it and never close it — and wired into all four context-menu entries. `toggleCollection`/`toggleFolder` now delegate to them when opening, so the lazy `refreshContents` call keeps one home.

---

## Out-of-phase UI work (2026-09-14, second pass)

- **Environments now load at app start.** `loadEnvironments()` lived only in
  `EnvironmentsPanel`'s mount effect, and that panel does not mount until the sidebar
  tab is clicked — so the environment selector in the tab strip was empty on launch and
  no environment could be made active, which made `{{variable}}` substitution
  unreachable until you happened to visit the sidebar. Moved to `App.tsx`; the panel no
  longer loads at all, since the store's create/rename/delete already refresh the list.
  Adding a second load to the selector was rejected: two mount effects racing on the
  same fetch.
- **Window minimums set to 900 × 600.** 500 × 350 was tried first and reverted on the
  same day: the sidebar is a fixed `w-64` and `RequestBuilder` gives its tab content a
  fixed `h-64`, so below roughly 900 × 600 the request bar overflows horizontally and
  the response viewer is squeezed to nothing. Making the app genuinely usable smaller
  means a flexible builder height, a wrapping or overflow-menu request bar, and a
  collapsible sidebar — not done.

---

## Phase 6 — History — **done, verified 2026-09-14**

**Decided before building (2026-09-14), four questions, all answered:**

1. **Store the full request**, not just a summary — otherwise re-run and
   save-to-collection cannot work. This widens an exposure collections already
   have (auth values in plaintext in the app-data SQLite, the open Phase 4
   decision); it does not create a new kind of one.
2. **Store the request as typed, `{{placeholders}} intact`.** A re-run then
   resolves against whichever environment is active *then*, matching the Phase 5
   rule that saved requests keep their placeholders. `resolved_url` is stored
   separately, purely so the list reads and searches sensibly.
3. **Cap at the most recent 500**, trimmed inside the insert transaction rather
   than swept periodically — nothing has to remember to run, and a token sent
   months ago does not sit on disk forever.
4. **A third sidebar tab**: Collections | Environments | History.

### The one design decision worth remembering

Substitution happens in the **frontend** (`lib/variables.ts`, applied in
`request-store.send()`), so Rust never sees the template — by the time
`send_request` runs, the placeholders are gone. Rather than thread a second
`template: SendRequestInput` parameter through `send_request` and change
`SendRequest`'s signature and its eight tests, **the frontend calls a separate
`record_history` command after the send settles**, success or failure.

The cost: a crash between the response landing and that call loses the entry.
Acceptable for a log whose purpose is to be re-run from, not an audit trail.

### Built

- Migration `0005_history.sql`: `history` with the same JSON columns a saved
  request uses (`headers_json`, `query_params_json`, `body_json`, `auth_json`,
  `settings_json`) plus `sent_at`, `resolved_url`, `status`, `error_kind`,
  `duration_ms`. `status IS NULL` means the request never produced a response;
  `error_kind IS NULL` means it did. Index on `sent_at DESC`. `MIGRATIONS` → 5.
- `domain/models.rs`: `HistoryEntry`, and `NewHistoryEntry` for one before
  storage assigns it an id and a timestamp — the same split collections,
  folders and environments already use.
- `domain/ports.rs`: `HistoryRepository` (`list(limit)`, `record(entry, keep)`,
  `delete`, `clear`). The repository owns trimming, so the cap cannot be
  bypassed by a caller that forgets it.
- `persistence/repositories/history.rs`: insert-then-trim in one transaction.
- `domain/services/history.rs`: `HISTORY_LIMIT = 500` lives here.
- `commands/history.rs`: `list_history`, `record_history`,
  `delete_history_entry`, `clear_history`.
- Frontend: `types/history.ts`, `services/history.ts`, `store/history-store.ts`,
  `features/history/HistoryPanel.tsx`, third sidebar tab.
- `lib/history-filter.ts`: search is a **pure function over the capped list**,
  not SQL — at 500 rows the difference is imperceptible and a pure function is
  testable. It matches method, resolved URL and status only: searching headers
  or the body would put credentials one keystroke from a results list
  (CLAUDE.md §11 rule 6). 6 tests, one of which asserts exactly that.
- Re-run and save-to-collection are **not separate code paths**. Clicking an
  entry opens its request in a new tab (`openHistoryEntry`) with
  `loadedRequest: null`, so the existing Send and Save buttons do both jobs and
  Save asks where to put it rather than overwriting anything. The tab arrives
  *clean*, so glancing at an entry and closing it again does not prompt.
- `buildTabFromSaved` was rebased onto a new `buildTabFromRequest`, which is
  what the history path uses — one place that turns a `SendRequestInput` into a
  tab instead of two.
- `durationMs` is libcurl's own total when there is a response, and wall-clock
  time when there is not (a failure has no timing).

### Verified

Full `verify.bat` green **on the first run** — build, eslint, **97 Vitest**
(+6 `history-filter`, +3 `openHistoryEntry`), `cargo fmt --check`,
`cargo clippy --all-targets -D warnings`, and **114 Rust tests** (80 unit,
11 curl integration, 23 repository — +6 for history).

First clean first run of the project so far. Worth noting what made it so: the
two files that could not be compiled remotely (`repositories/history.rs` needs
rusqlite, the `commands/dto.rs` additions need serde — crates.io is off the
sandbox egress allowlist) were written by copying the *shape* of
`saved_requests.rs` and the existing DTO conversions rather than by inventing
one, and the domain layer that could be compiled offline
(`models`, `ids`, `ports`, `services/history`) was, along with `rustfmt`.

### Outstanding

Manual click-through not done — the same gap already logged for cookies, Phase
4 auth and Phase 5 substitution. For history specifically, the things no test
covers:

- Send, restart the app, and confirm the entry is still listed. The cap and the
  ordering are tested; **survival across a restart is not**, because every test
  runs against an in-memory database.
- Re-run an entry saved while a *different* environment was active, and confirm
  it resolves against the one active now rather than replaying the old values.
  This is the whole point of storing the template, and it is only covered by a
  repository round-trip assertion.
- Confirm a failed send (unreachable host) lands in the list with its error kind
  rather than vanishing.
- Confirm no token or cookie is visible anywhere in the panel or in search
  results.

**Done when:** history survives app restart, is searchable, and a past request
can be replayed or promoted to a saved collection item.

---

## Phase 6 extension — Saved responses (examples) — **done, verified 2026-09-14**

"Save as Example": a named response kept under the request that
produced it, which makes a request node in the sidebar expandable. Asked for
during Phase 6, hence the placement — but it is **collections work**, not
history work, and nothing about it touches Phase 6's tables or commands.

### Decided before building (2026-09-14)

1. **An example stores the request snapshot as well as the response.** Without
   it, two examples under one request cannot say what differed between them,
   which is the whole reason to keep more than one.
2. **Text bodies only, for now.** `ResponseBody::from_bytes` discards the bytes
   of a non-UTF-8 response and keeps only `byte_length`, so a binary response
   never reaches a layer that could store it. Save Response is disabled on one,
   with the reason on the button rather than the action quietly disappearing.
   Fixing it means carrying bytes through curl_client → domain → DTO → viewer:
   its own slice, and the prerequisite for any "export response to file".
3. **1 MB cap, refused above, never truncated.** A stored example that silently
   differs from what the server sent is worse than no example.

### The snapshot is resolved, unlike everywhere else

`SavedRequest` and `HistoryEntry` both keep `{{placeholders}}` so they can be
re-run against whichever environment is active later. An example is the
opposite: it records one exchange that actually happened, so it stores the
request **as sent**, resolved. Resolving it again later would make it a record
of nothing.

That snapshot is captured at send time (`RequestTab.sentRequest`), not read off
the builder when Save is clicked — the fields may well have been edited in
between, and an example that claims inputs it did not have is a lie in a file.

### Built

- Migration `0006_examples.sql`: `examples`, FK to `requests(id)` **ON DELETE
  CASCADE**, request snapshot in the same JSON columns a saved request uses,
  plus `status`, `response_headers_json`, `response_body`. `MIGRATIONS` → 6.
- `domain/models.rs`: `Example`, `NewExample`, `ExampleSummary`.
- `domain/ports.rs`: `ExampleRepository`.
- **Summaries in the tree, full rows on open.** `collection_contents` returns
  `ExampleSummary` (id, requestId, name, status) only. Returning full examples
  would mean the sidebar pulling every body in a collection — potentially
  hundreds of megabytes — to draw a list of names. `load_example` fetches one.
- `list_summaries_by_collection` joins through `requests` rather than
  denormalising `collection_id` onto `examples`: a request can be moved between
  folders, and one copy of that fact is one chance to get it wrong.
- Ordered by `created_at`, not by name: examples read as a sequence of cases
  under a request, and renaming one should not reshuffle them.
- `create` checks the request exists inside the transaction, so a stale id
  reports `NotFound` rather than an opaque foreign-key failure.
- The 1 MB cap and the name validation live in the `Collections` service, which
  gained a fourth repository. `validated_name` is now used by five callers.
- Four commands on the existing collections adapter: `save_example`,
  `load_example`, `rename_example`, `delete_example`. `SaveExampleInput` groups
  the six fields rather than passing them as six loose command parameters.
- Frontend: `buildFolderTree` returns `examplesByRequestId` beside the tree
  rather than folding examples into `FolderNode` — they are children of a
  request, not part of the folder hierarchy. `ExampleViewer` opens as a third
  `Tab` union member. `SaveExampleDialog` asks only for a name; unlike saving a
  request, where it goes is already decided.
- `ExampleViewer` deliberately does **not** reuse `ResponseViewer`: that renders
  an `HttpResponse`, which carries a timing breakdown an example has no honest
  value for. Reusing it would have meant fabricating a `Timing` and putting
  invented numbers on screen.
- **`lib/status-colors.ts`** extracted while doing this. The status→colour map
  had been copied into `ResponseViewer` and then into `HistoryPanel`; the
  example viewer would have been the third copy (CLAUDE.md §7, DRY). Both
  existing copies now point at it.
- The delete prompt for a request was reworded — "this cannot be undone" reads
  very differently now that something hangs underneath.

### Verified so far

- Frontend type-check clean, with the standing zustand caveat.
- Domain layer compiled and clippy-clean in the offline scratch crate.
- `rustfmt --edition 2021` applied to all ten touched Rust files.
- **Not compiled remotely:** `repositories/examples.rs` (rusqlite) and the
  `commands/` additions (serde, tauri). 6 new repository tests and 3 new service
  tests are written but have not run.

### Outstanding

*(2026-09-16: `verify.bat` has since passed. See Phase 7, "The five-slice backlog cleared". The manual checks below are still outstanding.)*

`verify.bat` has not been run. Manual checks specific to this slice:

- Save a response, confirm the request node gains a chevron and the example
  appears under it with the right status colour.
- Save a second example under the same request and confirm creation order
  holds after renaming the first.
- Delete the request; confirm its examples go with it (the cascade).
- Save an example, restart, and confirm it is still there.
- Confirm Save Response is disabled with a readable reason on an unsaved
  request and on a binary response.

**Done when:** a response can be saved under its request, the request node
expands to show it, it survives a restart, and deleting the request removes it.

---

## Phase 6 extension — Send and download — **done, verified 2026-09-14**

A split Send button: Send on the left, a chevron opening
"Send and download", which writes the response body to a file the user picks.

### New dependency — approved 2026-09-14

`tauri-plugin-dialog = "2"`. Tauri 2 has no dialog in core and a native file
chooser needs platform APIs, so there was no way to do this without one. It is
first-party and links into the binary, so the single-file guarantee holds
(§11 rule 2). **Rust side only** — `@tauri-apps/plugin-dialog` was deliberately
not added: the chooser is exposed through our own `send_and_download` command,
so the call stays behind a typed service function (§11 rule 3) and there is one
new dependency rather than two. No `capabilities/` entry is needed, because the
plugin's JS commands are never called.

### The bytes never cross the IPC boundary

One command does send → dialog → write, rather than send-then-save as two
commands. A 200 MB download serialised as JSON through IPC would be miserable,
and holding it in app state between two commands would mean deciding when to
drop it. One command keeps the body in a single scope.

`DownloadResultDto` therefore carries **no body at all** — status, headers,
timing, byte count, and where it went. `savedTo: null` means the user dismissed
the dialog; the request still happened and its status and timing still show.

The dialog opens *after* the response arrives. That is what lets it suggest a
name from `Content-Disposition`, and it is why a failed request never asks
where to save anything.

### ResponseBody now keeps its bytes

```rust
Binary { bytes: Vec<u8> }   // was: byte_length: usize
```

`from_bytes` had been throwing away the bytes of every non-UTF-8 response, so
nothing downstream could write a downloaded file. `ResponseBodyDto` still sends
only `byteLength`, so **nothing on the frontend changed**. This is also the
prerequisite for saving a binary response as an example, and for any future
export-to-file — both currently refused for exactly this reason.

### Where the pieces live

- `domain/services/downloads.rs`: `Downloads::save` plus `suggested_file_name`,
  both pure enough to test without a dialog (9 tests).
- **No `FileWriter` port.** A trait with one implementation and no test seam is
  speculative (§7, YAGNI); these functions are already testable against a temp
  directory, and no second filesystem adapter is coming.
- **The chooser is not in the domain.** A native dialog is a platform concern,
  so it lives in the command layer — which is the adapter to the platform. The
  service only decides what to write and what to call it.
- `blocking_save_file` is used rather than the callback form, and is correct
  only because the call is already inside `spawn_blocking`: the docs are
  explicit that it deadlocks on the main thread.

### Filename suggestion, in priority order

`Content-Disposition` filename (a server that sends one has named the file
itself) → the URL's last path segment (right for the `/files/report.pdf` shape)
→ `response` plus an extension guessed from `Content-Type`.

RFC 5987 `filename*=` is deliberately not handled: it carries a charset and
percent-encoding, and getting it half-right would give a worse name than
falling through to the URL.

A name from a header or a URL is untrusted input heading for a filesystem path,
so separators and traversal are stripped — a server must not be able to steer
the save dialog out of the directory the user picked. There is a test for that.

### Fixed while here

`services/http-client.ts` had its own copy of `toApiError`, kept from before
`lib/api-error.ts` existed, and the copy had drifted: it was missing `notFound`
and `storage`. A storage failure would have been flattened into an internal
error with the wrong message — which `send_and_download` is the first command
able to produce. It now uses the shared mapping.

### Verified so far

- Frontend type-check clean, with the standing zustand caveat.
- `domain/services/downloads.rs` compiled and tested in the offline scratch
  crate: 9 tests, clippy clean. It caught one real error — `split("://")` is not
  a `DoubleEndedIterator`, so `next_back()` does not compile; replaced with
  `split_once`.
- `rustfmt --edition 2021` applied to all seven touched Rust files.
- **Not compiled remotely:** `commands/request.rs` (tauri, tauri-plugin-dialog).
  The dialog API was checked against docs.rs rather than guessed —
  `DialogExt::dialog()`, `Dialog::file()`, `set_file_name`,
  `blocking_save_file() -> Option<FilePath>`, `FilePath::into_path() ->
  Result<PathBuf, Error>` — but it has not been through a compiler.

### Outstanding

*(2026-09-16: `verify.bat` has since passed. See Phase 7, "The five-slice backlog cleared". The manual checks below are still outstanding.)*

`verify.bat` has not been run, and this is the first slice that adds a crate, so
the build is where it will fail if the API is wrong. Manual checks:

- Download a binary file (an image or a PDF) and confirm it opens correctly —
  the bytes surviving `from_bytes` is the whole change, and a corrupt file is
  the only symptom of getting it wrong.
- Confirm the suggested name comes from `Content-Disposition` when a server
  sends one, and from the URL when it does not.
- Dismiss the Save As dialog and confirm the status and timing still show, with
  no error.
- Cancel a download mid-flight and confirm it behaves like cancelling a send.
- Confirm a downloaded request still lands in History.

**Done when:** "Send and download" writes a byte-exact file to a chosen
location, suggests a sensible name, and a dismissed dialog is not an error.

---

## Phase 6 extension — Multipart file parts — **done, verified 2026-09-14**

Closes the "text parts only" limitation Phase 2 deferred. The file chooser was
the stated blocker; `tauri-plugin-dialog`, added for Send-and-download,
supplies it.

### Decided 2026-09-14: libcurl builds the multipart body

The hand-rolled encoder in `mapping.rs` is gone, replaced by `build_form()` +
`Easy2::httppost()`. The deciding fact came from reading the crate source
rather than the prose docs:

```rust
pub struct Form {            // no lifetime parameter
    strings: Vec<CString>,   // Part::add() copies borrowed data in
    buffers: Vec<Vec<u8>>,
}
```

`Part::add()` copies names and inline values into the `Form`, so the borrows
only have to survive the call — which is why `httppost(&mut self, form: Form)`
needs no lifetimes. A **file** part stores the path and `CURLFORM_FILE`, so
libcurl opens and reads the file itself during the transfer. **An upload never
sits in this process's memory**, which every other body type does
(`post_fields_copy` means peak memory is roughly twice the body).

Cost, accepted: the two byte-level encoder tests are gone, because asserting
our own bytes would now be asserting nothing. They are replaced by an httpmock
integration test that sends a real file from a temp directory and matches on
`body_contains`, so the *server* is what proves the file arrived — a stronger
test than the one it replaces.

Also gone: `multipart_boundary()`. libcurl picks the boundary and sets
Content-Type.

### A file part carries a path, not bytes

Which means a saved request can outlive the file it points at — renamed,
moved, or on a machine the collection was copied from. libcurl reports that as
a generic transport failure that never mentions which file, so
`validate_multipart` in `SendRequest` catches it first and returns
`InvalidRequest` naming the part and the path. Two tests cover it, including
that a blank-named row (the table's spare) is skipped rather than failing the
send, matching what `build_form` does.

### Old saved bodies still load

`body_json` changed shape (`fields` → `parts`, and parts gained a `kind`).
Migration 0001 is shipped and must not be edited (§11 rule 5), so `StoredBody`
handles both: `#[serde(alias = "fields")]` on the field, and an untagged
`StoredMultipartPart` that tries the tagged form first and falls back to a bare
name/value pair, which is what a pre-today row is. Without this, one old
multipart request would fail to deserialise and take its **whole load** down,
not just its body.

### One MIME table, two directions

`domain/mime.rs` now owns the extension ⇄ media-type mapping. `downloads.rs`
had the download half of it; the upload half would have been a second copy
disagreeing about what a .csv is (§7, DRY).

### Frontend

- `lib/multipart-rows.ts` + 11 tests. Rows hold **both** a text value and a
  file path, so toggling Text ⇄ File does not silently discard what was typed.
  A file row with no file chosen is dropped rather than sent, since it would
  only fail backend validation for a row the user has not finished.
- `MultipartTable.tsx` is multipart's own table. `KeyValueTable` stays the one
  shared table for headers, params and form-urlencoded — this is a different
  row shape, not a second copy, and the comment on `KeyValueTable` now says so
  rather than leaving the next reader to assume a rule was broken.
- `curl-string-builder.ts` emits curl's `@` syntax for file parts
  (`-F 'upload=@/tmp/a.png;type=image/png'`), so a copied command uploads the
  same file instead of pasting the path in as text. `type=` is omitted when
  there is no guess, letting curl fall back to its own.
- `{{variables}}` substitute into a file **path** as well as into names and
  text values — `{{fixtures}}/logo.png` is as reasonable as `{{base_url}}`.
  `contentType` is left alone: it is derived from the chosen file, not typed.
- Dirty-checking compares paths, so swapping the file marks the tab unsaved.
- `choose_file` command + `services/files.ts`. Only the path comes back;
  nothing reads the file in Rust either.

### Verified so far

- Frontend type-check clean, with the standing zustand caveat.
- Domain layer in the offline scratch crate: **52 tests**, clippy clean. It
  caught two real errors before the build — `Option::or_else` where `or` was
  wanted (`unnecessary_lazy_evaluations`), and earlier the `split("://")`
  `next_back` mistake.
- `rustfmt --edition 2021` applied to all fourteen touched Rust files.
- **Not compiled remotely:** `http/mapping.rs` and `http/curl_client.rs` (need
  the `curl` crate), `commands/files.rs` (tauri + dialog), and the persistence
  and DTO changes (rusqlite, serde). The `Form`/`Part` API was read from the
  crate source, not guessed, but has not been through a compiler.

### Outstanding

*(2026-09-16: `verify.bat` has since passed. See Phase 7, "The five-slice backlog cleared". The manual checks below are still outstanding.)*

`verify.bat` has not been run. Manual checks:

- Upload a real file to a real endpoint and confirm the server received the
  right bytes and the basename as the filename.
- Upload something large enough to notice (100 MB+) and watch memory — the
  whole point of this design is that it should stay flat.
- Point a saved request at a file, move the file, re-send, and confirm the
  error names the path rather than saying the transport failed.
- Open a multipart request **saved before today** and confirm its text parts
  are still there — that is the `serde(alias)` guard, and it fails silently if
  it is wrong.
- Copy as cURL with a file part and confirm the pasted command actually
  uploads.

**Done when:** a file part uploads byte-exactly without the process buffering
it, a moved file produces a message naming the path, and a multipart request
saved before this change still loads.

---

## Phase 6 extension — Header autocomplete — **done, verified 2026-09-14**

Name autocomplete on the Headers tab from the IANA registry, and value
autocomplete for the headers that have a closed set of standard values.

### The registry does not answer the question that was asked

There is **no request/response column** in the IANA HTTP Field Name Registry.
"Request headers from IANA" is not a filter the registry supports. So
`IANA_HEADER_NAMES` includes response-only names — Set-Cookie,
WWW-Authenticate, ETag. That is deliberate: splitting them would be our
invention rather than the registry's, and a request builder has no business
refusing to suggest something IANA registered. If that proves noisy in use,
curating a "common request headers" subset shown first is the fix, and it
should be labelled as our curation.

Entries the registry marks `obsoleted` or `deprecated` are excluded (46 of the
255 rows read), so Warning, Content-MD5 and Pragma are not offered. Typing one
still works — the field is free text and autocomplete only ever suggests.
`Accept-Charset` falls out this way too: RFC 9110 deprecates it, and IANA
records that.

### Completeness could not be verified, and does not need to be

`WebFetch` answers a prompt against the page through a small model rather than
returning the file, so 500 rows do not come back in one call. The registry was
read in seven alphabetical slices and reassembled: **255 rows, 255 unique
names**, 186 permanent / 23 provisional / 38 obsoleted / 8 deprecated.

The tool's own counts contradict themselves — one call said "500 data rows",
another returned per-letter counts summing to 224, and the slices yielded 255.
**Its negatives are not trustworthy**, the same lesson as the Wikipedia pass.
Rows may have been dropped silently and there is no way to prove otherwise
through this tool.

That is acceptable *here specifically*: a missing entry costs a suggestion, not
correctness, because the field stays free text. It would not have been
acceptable for anything the app enforces. To refresh the list properly, read
the CSV directly rather than through a summarising fetch.

### Values are ours, not IANA's

IANA registers field **names** only — no values column, no companion registry.
Every entry in `lib/http-header-values.ts` is hand-written from the RFC that
defines the field, and each group cites one. That is a real maintenance
burden, stated at the top of the file: a wrong suggestion there is a bug no
upstream refresh will fix.

27 headers have suggestions. Only fields with a small closed set of tokens
qualify — dates, URLs, credentials, digests and quoted ETags are free text,
and suggesting for them would be noise, so Host, Origin, Referer and
If-Modified-Since are deliberately absent.

A test asserts every value key also exists in `IANA_HEADER_NAMES`. Without it
the two lists drift and a value suggestion becomes unreachable except by
typing the name exactly.

### DRY fallout, fixed while here

`lib/media-types.ts` extracted. `RAW_CONTENT_TYPES` lived in `BodyEditor.tsx`
and the Accept / Content-Type value suggestions needed the same list — a
second copy would have let the body editor and the Headers tab disagree about
what this app offers (§7). `Access-Control-Request-Method` reuses
`HTTP_METHODS` from `types/http.ts` for the same reason.

No Rust counterpart, and that is the point: header names are a UI affordance,
not a rule the domain enforces, so unlike HTTP methods or content types this
list has nothing to stay in step with on the other side of the boundary.

### How it renders

Native `<datalist>`, no new dependency and no popup to manage. The name column
shares one list per table; the value column gets a list **per row**, since the
options depend on that row's header name, and renders none at all when the
header has no suggestions. Ids come from `useId()` because Params and Headers
are both mounted in the builder at once.

`nameSuggestions` and `valueSuggestionsFor` are optional props on
`KeyValueTable`, so query params and form fields are unchanged — their names
are whatever the API happens to call them.

### Verified so far

- Frontend type-check clean, with the standing zustand caveat.
- The name/value drift invariant checked directly as well as by the test.
- 12 new tests in `lib/http-header-values.test.ts`; not yet run.
- No Rust changed.

### Outstanding

*(2026-09-16: `verify.bat` has since passed. See Phase 7, "The five-slice backlog cleared". The manual checks below are still outstanding.)*

`verify.bat` has not been run. Manual checks:

- Type `acc` in a header name and confirm the list filters.
- Put `Accept-Encoding` in a row and confirm the value box offers gzip, while
  a row named `Host` offers nothing.
- Confirm the Params tab has no suggestions at all.
- Confirm a header name that is not in the list still types and sends freely.

**Done when:** a header name completes from the registry, a known header's
value completes from its RFC's token set, and neither one blocks a value that
is not on either list.

---

## Phase 7 — Packaging & release hardening — **release gate green 2026-09-15; clean-machine install still outstanding**

### The five-slice backlog cleared, 2026-09-14

`verify.bat` green: **121 TypeScript tests** across 16 files, **144 Rust tests**
(103 unit, 12 curl integration, 29 repository), eslint and clippy clean.

That one run verified five slices at once — examples, send-and-download,
multipart, header autocomplete and the transport settings — including a new
crate and a rewrite of the transport's body path. **Every API read from docs.rs
rather than compiled turned out correct**: `HttpVersion::V11`/`V2`,
`PostRedirections`, `SslVersion::Tlsv12`/`Tlsv13`, `unrestricted_auth`,
`http_09_allowed`, `Form`/`Part::file`, and the dialog plugin's
`blocking_save_file` and `blocking_pick_file`.

Two failures, both frontend, both instructive:

1. **Two inline `{ folders: [], requests: [] }` fallbacks** never gained
   `examples`. They sit behind `store.contentsById[...]`, and zustand is
   untyped in the remote sandbox, so the expression collapsed to `any` there
   and the union never formed — **the third time that gap has hidden
   something**. Replaced with one `EMPTY_COLLECTION_CONTENTS` constant so the
   next field cannot be forgotten in two places.
2. **A non-null assertion in a test** (`rows[0]!.id`), against the project's
   own rule.

Also corrected: a mock in `services/collections.test.ts` still returning the
two-field contents shape. It passed, because the mock is typed `any` — it was
asserting against something the backend no longer sends.

**Lesson worth keeping:** the remote TypeScript check is reliable for pure
modules and blind to anything reached through a store. Rust, by contrast, was
verified well by the offline scratch crate — it caught two compile errors
before this run and nothing it passed failed here.

### The question Phase 0 left open — **answered 2026-09-15**

`release.bat` ran for the first time. Two results.

**1. `tauri-build` does NOT pass `-C target-feature=+crt-static`.** Confirmed,
not suspected: the release exe links the C runtime dynamically.

**2. It does not matter, and here is why.** The 27 imports contain **no
`vcruntime140.dll` and no `msvcp140.dll`** — those ship in the VC++
Redistributable, not in Windows, and they were the failure the spike was
written to catch. What the exe links instead is the Universal CRT, which
Microsoft documents as "a Microsoft Windows operating system component...
included as part of the operating system in Windows 10 or later, and Windows
Server 2016 or later". WebView2 already puts a Windows 10 floor under this app,
so the UCRT is present wherever the app can run at all. Also absent: `libcurl`,
`libssl`/`libcrypto`, `sqlite3.dll`. **libcurl, rustls and SQLite are inside
the binary — the guarantee §11 rule 2 actually makes holds.**

The check failed only because its allowlist was written for a console spike
that imported almost nothing. Widened, each entry justified in place: the
UCRT APIset family (`api-ms-win-crt-*`) and four core UI/shell libraries
(`comctl32`, `dwmapi`, `gdi32`, `shlwapi`) that a WebView2 host cannot avoid.
Deliberately **not** a blanket `api-ms-win-*`, so only the two known families
pass. Simulated against the real 27-import list: all pass, while
`vcruntime140`, `msvcp140`, `libcurl`, `libssl`, `sqlite3` and
`WebView2Loader.dll` all still fail.

**Decided 2026-09-15: widen the allowlist rather than force `+crt-static`.**
Static linking would buy reach to Windows 8.1 and Server 2012 R2, which
WebView2 rules out anyway, at the cost of a from-clean rebuild and the risk
that libcurl's and SQLite's C code object to `/MT`. The check's job is now the
narrower, sharper one: catch the day a dependency appears that a user would
have to install.

**Outstanding:** the four UI/shell DLLs have not been eyeballed in a clean
Windows Sandbox, which the script's own rule demands before adding anything.
One `dir C:\Windows\System32\<name>` each closes it; there is a comment in
the file that says so and should be deleted when someone does.

### Second run: the whole gate green, 2026-09-15

`release.bat` clean end to end, and into the right file this time. verify
(build, lint, 128 frontend tests, fmt, clippy, 182+12+29 Rust tests), then a
release build in **2m 04s**, then the static-link check: **27 imports, PASS**.

The bundle step also produced both installers for the first time, which is the
Phase 7 config finally being exercised rather than merely written:

```
target\release\bundle\nsis\Responder_0.1.0_x64-setup.exe
target\release\bundle\msi\Responder_0.1.0_x64_en-US.msi
```

**What this does and does not prove.** It proves the binary imports nothing a
Windows 10+ machine lacks, and that the bundle config builds. It does not
prove the Done-when below, which is a *clean-machine install* — nothing has yet
installed and run this on a machine without Rust, Node, curl or OpenSSL, and
with the WebView2 caveat that means "with network at install time" too. The
NSIS graphics, the downgrade block and the bootstrapper are all still unseen.

### The bug that hid all of this

`release-log.txt` came back **21 bytes** — just its header. `verify.bat` had no
`setlocal`, so its `set LOG=...verify-log.txt` overwrote `release.bat`'s `LOG`,
and every line the release gate wrote afterwards — the bundle output and the
static-link verdict — was appended to `verify-log.txt` instead. The release
gate had been reporting nothing at all, and only reading the *other* log found
the answer.

Both scripts now `setlocal`. Two further fixes came with it: `release.bat` now
`cd /d "%~dp0"` before bundling (`verify.bat` ends inside `src-tauri`), and the
build's exit code is captured into a variable instead of being read as
`%errorlevel%` inside a parenthesised block, where it expands at parse time and
would have reported a stale value on failure.

### WebView2: the hole the spike could not see

The proof was a console binary. The real app renders in **WebView2**, a
Microsoft runtime on the *user's machine* — bundled with Windows 11 and pushed
to most Windows 10, but not guaranteed on LTSC or Server. CLAUDE.md §10 says
no runtime dependency on anything installed on the user's machine, and this is
one. libcurl, the TLS stack and the CRT being inside the binary never spoke to
it.

**Decided 2026-09-14: `downloadBootstrapper`** (the Tauri default). The
installer fetches the runtime when it is missing, which keeps the installer
small at the cost of needing network *at install time*. The honest consequence
is that the Done-when below is narrower than it reads: a clean machine with no
network may not install. The offline installer (~130 MB) and the fixed runtime
(~180 MB, and no longer one file) were both considered and rejected as too
heavy for the current stage.

### Bundle config — done 2026-09-14

- `targets: ["nsis", "msi"]` rather than `"all"`. Windows is the only target
  being built (Phase 0); `"all"` worked only because the other targets are
  skipped silently, which stated no intent. MSI is included because corporate
  deployment tooling expects it.
- `nsis.installMode: "currentUser"` — no UAC prompt, and it matches where the
  app actually keeps things: the SQLite database lives in the per-user app-data
  directory, so a per-machine install with per-user data would be a mismatch.
- `publisher`, `copyright`, `category`, `shortDescription`, `longDescription`
  filled in. Publisher and copyright were first inferred from the original
  identifier; **both were corrected to `Zoran Davidović` on 2026-09-18** when
  the app moved to an individual publisher (Phase 10).
- `allowDowngrades: false` (2026-09-14). Tauri defaults it to `true`; it sits
  under `bundle.windows`, so it covers both the NSIS and the MSI artifact.

  This is not housekeeping. `Database::migrate` reads `PRAGMA user_version` and
  does `MIGRATIONS.iter().enumerate().skip(applied)` — when `applied` exceeds
  the array's length, that yields an **empty iterator and returns Ok**. An
  older build opening a newer database therefore starts up silently against a
  schema it does not know, and the first write touching a table a later
  migration changed is where it surfaces. Blocking downgrades at the installer
  is currently the only thing preventing that.

  **Open, not done:** `migrate()` should refuse a database from the future
  rather than shrugging at it — the installer guard does not help someone who
  copies an app-data directory between machines, and the failure it prevents is
  silent data corruption rather than a clean error.

### Installer graphics — done 2026-09-14

Five assets generated from `src-tauri/icons/icon.png` and wired into
`bundle.windows.nsis`: `installerIcon`, `uninstallerIcon`, `headerImage`,
`uninstallerHeaderImage`, `sidebarImage`. All five key names verified against
the Tauri config schema first.

- **The R is lifted as a mask from the app icon**, not re-set in a font. A
  lookalike letterform drifts the moment either the icon or the installer art
  is retouched; this way there is one letterform.
- **Icons carry different artwork per size, not one image downsampled.** At 16
  and 20 px the gradient turns to mud and the soft corner eats the silhouette,
  so those use a flat plate and a slightly larger glyph. Pillow's ICO writer
  cannot do per-size art — it resizes a single source — so the directory is
  packed by hand: DIB entries up to 48 px, PNG above, which is the layout old
  icon loaders handle.
- **The uninstaller badge keeps the dark plate and only neutralises the glyph**
  to nord4. Muting both, as the first pass did, washed the mark out at 16 px,
  where it has least to give.
- **Headers are white**, because MUI draws that strip on `${MUI_BGCOLOR}`; a
  dark bitmap there reads as a sticker pasted onto the wizard. The sidebar is
  full-bleed dark, which is what that panel is for.
- One defect found by looking at the output at 4x and fixed: resizing an RGBA
  badge dragged undefined colour out of the transparent region into the edge
  pixels, haloing every rounded corner. Colour and alpha are now downsampled as
  separate full-bleed images.
- Verified: all three BMPs are 24-bit `BI_RGB` at exactly 150x57 and 164x314 —
  NSIS rejects compressed bitmaps — and the generator reproduces all five files
  byte-identically from a clean directory.

**Not changed, deliberately:** `languages` stays `["English"]` and
`displayLanguageSelector` stays `false`. The snippet these came from listed
Hebrew; adding a language nobody asked for would invent a requirement.

### Still outstanding

- **`csp: null`.** Content Security Policy is disabled. Low risk today: the app
  loads Monaco from its own bundle and renders response bodies as text in an
  editor rather than as HTML, so there is little injection surface. But it is a
  release-posture decision that has never been made deliberately, and turning
  it on may need work for Monaco's blob workers. Left alone rather than changed
  blind.
- ~~**Tray glyph.**~~ *Done 2026-09-17 — see "Tray icon and orphan cleanup".*
- **Code signing.** Unsigned installers get SmartScreen warnings on Windows.
  Not started; needs a certificate, which is a purchase, not a code change.
- Auto-update remains out of scope unless asked: a new dependency
  (`tauri-plugin-updater`) plus an update server to run.

**Done when:** a clean-machine install of the built binary (no Rust, Node,
curl or OpenSSL present) runs and sends a real HTTPS request. Note the
WebView2 caveat above — as configured, "clean machine" also means "with
network at install time".

---

## Phase 7 extension — More transport settings — **done, verified 2026-09-14**

Six settings from request Settings tab, chosen after auditing all
thirteen against what libcurl and our TLS backend can actually do.

### What could not be built, and why it matters

- **Cipher suite selection** and **use server cipher suite during handshake**:
  unavailable because we link **rustls**, which deliberately refuses to expose
  cipher suite selection — a design stance, not a missing binding, so
  `CURLOPT_SSL_CIPHER_LIST` has nothing to act on. This is the cost side of the
  Phase 0 trade that bought the single-binary guarantee. Switching to
  `static-ssl` would unlock both and put an OpenSSL build back in the supply
  chain; that is the actual decision, and it is not worth two settings.
- **Remove referer header on redirect**: libcurl has no such option. It offers
  `autoreferer`, which is the opposite — it *adds* a Referer on redirect. There
  is no way to strip one the user set, so it was left out rather than faked.

### Two were narrowed, and are labelled for what they do

**"enable strict HTTP parser"** restricts invalid headers generally;
libcurl exposes no such switch. All we can control is whether HTTP/0.9
responses are accepted, so the setting reads **"Accept HTTP/0.9 responses"**.

**"TLS/SSL protocols disabled during handshake"** becomes a
**minimum** of TLS 1.2 or 1.3, because rustls supports only those two — there
is no TLS 1.0 or 1.1 here to disable. `Auto` does not call the option at all,
so the common path is untouched.

Copying the other product's wording would have promised more than the
transport delivers.

### Built

| Setting | Mechanism |
|---|---|
| HTTP version | `Easy2::http_version`, previously hardcoded to `V2TLS` |
| Keep method on redirect | `post_redirections(PostRedirections::redirect_all)` |
| Keep Authorization across hosts | `unrestricted_auth` |
| Encode query parameters | our own `build_url`, not libcurl |
| Accept HTTP/0.9 | `http_09_allowed` |
| Minimum TLS version | `ssl_version(SslVersion::Tlsv12 / Tlsv13)` |

`Auto` for HTTP version maps to `V2TLS` — exactly what the client did before
the setting existed — so adding it changes nothing for a request that ignores
it. The same principle applies to `TlsMinimum::Auto`, which sets no option.

`encode_url` governs only the parameters the app appends. The URL the user
typed is passed through untouched either way: rewriting someone's URL because
a checkbox is ticked would be a surprise, and libcurl parses it regardless.

Keeping Authorization across hosts shows a warning when enabled — it is how a
credential reaches a server that was never meant to see it, which is why
libcurl makes it opt-in too.

### The backward-compatibility trap, again

Six keys added to `settings_json`, and **every saved request, history entry and
example in an existing database predates them**. The column is JSON, so there
is no migration to write — the serde defaults *are* the migration.

`encode_url` is the dangerous one: a bare `#[serde(default)]` on a bool yields
`false`, which would silently stop percent-encoding query parameters on every
request already stored. A wrong request, sent with no error anywhere. It uses
an explicit default fn returning true, the same guard `send_cookies` needed,
and there are now three tests in `json.rs` pinning it — including a verbatim
copy of the pre-2026-09-14 blob.

### Verified so far

- Frontend type-check clean, with the standing zustand caveat.
- Domain model compiled and clippy-clean offline; 52 tests still pass.
- `rustfmt` applied to all six touched Rust files.
- **Not compiled remotely:** `curl_client.rs`, `mapping.rs`, `json.rs`,
  `dto.rs`. `PostRedirections`, `SslVersion` and `unrestricted_auth` were read
  from docs.rs; `HttpVersion::V11` and `V2` are inferred from the crate already
  exporting `V2TLS`, which we call today — the likeliest place for a wrong
  identifier.

### Outstanding

*(2026-09-16: `verify.bat` has since passed. See Phase 7, "The five-slice backlog cleared". The manual checks below are still outstanding.)*

`verify.bat` still has not been run, and this is now the **fifth** uncompiled
slice on top of examples, send-and-download, multipart and header autocomplete.

Manual checks:

- Force HTTP/1.1 against a server that speaks h2 and confirm the version
  actually changes — the hardcoded `V2TLS` meant there was no workaround at all
  before this.
- POST to something that 302s and confirm the method survives only with the
  setting on.
- Open a request **saved before today** and confirm "Encode query parameters"
  is still ticked. Wrong, it fails silently.
- Set TLS 1.3 only against a TLS 1.2 server and confirm it fails cleanly rather
  than erroring on an option rustls does not support.

**Done when:** each of the six changes observable behaviour, and a request
saved before today still encodes its query parameters.

---

## Phase 8 — OpenAPI export (and, later, import) — **planned 2026-09-14; 8a green on verify.bat 2026-09-15, manual click-through outstanding**

Export a collection as an OpenAPI document, targeting **3.2.0** first, with
3.1 and 3.0 emitters and an importer to follow. The architecture below exists
because of those three follow-ups, not despite them.

### What the spec actually gives us

Read from `swagger.io/specification/v3.2/` and the OAI repo's `3.2.0.md`.
Confirmed 3.2 additions over 3.1: `$self` (OpenAPI Object), `name` (Server
Object), the `query` HTTP method and `additionalOperations` (Path Item),
`mediaTypes` in Components, `itemSchema` / `prefixEncoding` / `itemEncoding`
(Media Type, for sequential and positional-multipart content), `serializedValue`
(Example Object), plus `in: "querystring"` and `style: cookie` on Parameter.

**Of those, exactly one is useful to this exporter: `serializedValue`.** Our
saved responses are already-encoded strings; `value` wants an in-memory
representation, and `serializedValue` is the correct home for a body we have
not parsed. Everything else is either about streaming (we do not stream),
custom methods (we support seven, fixed), or document identity.

The practical consequence shapes the whole design: **targeting 3.2 is about
being current, not about capability.** The internal model should therefore be
3.1-shaped plus those few 3.2 extras, which makes the 3.1 emitter nearly free
and leaves 3.0 as the only genuinely lossy direction.

**Not verified:** the exact default `jsonSchemaDialect` URI for 3.2 — the
fetched text referenced Draft 2020-12 repeatedly without stating the string.
Confirm it against the spec before emitting it rather than copying 3.1's.

### The mismatch this feature is really about

ResponderHTTP stores **concrete requests**. OpenAPI describes **an API**. Nothing
in the export is a rename; every row below is an inference.

| ResponderHTTP | OpenAPI | Honest? |
|---|---|---|
| Collection | the document, `info.title` | exact |
| Folder | `tags[]` + `operation.tags` | exact |
| `SavedRequest.name` | `operation.summary`, `operationId` | exact |
| URL with `{{var}}` | `servers[].variables` / path templating | **exact — see below** |
| URL without variables | a literal path | exact but low-value |
| header rows | `parameters[in: header]` | value → schema is inference |
| query param rows | `parameters[in: query]` | same |
| raw/form/multipart body | `requestBody.content.<type>` | schema is inference |
| `Auth` variant | `components.securitySchemes` + `security` | approximate for Custom |
| **`Example` (saved response)** | `responses.<code>.content.<type>` | **exact, and the good stuff** |

**Two things fall out of that table.**

First: **path templating should be driven by `{{variables}}`, not by
heuristics.** Guessing that `/users/123` means `/users/{id}` invents an API.
But `{{base_url}}/users/{{userId}}` is the user *telling* us what varies — so
it maps exactly: leading variable becomes the Server, interior variables
become path parameters, everything else stays literal. It is deterministic, it
never invents, and it hands the user a lever they already know how to pull. A
collection full of hardcoded URLs exports a poor document, and that is the
correct outcome rather than a reason to start guessing.

Second: **the saved-responses feature is what makes this export worth having.**
Status code, content type from the stored headers, and a real body — that is a
populated `responses` object. Without examples every operation degrades to
`responses: { default: { description: "..." } }`, which is close to worthless.
Export quality is a direct function of how many examples a collection carries,
and the UI should say so rather than let someone export an empty-looking
document and conclude the feature is broken.

### Security: what must never leave the app

An exported OpenAPI file gets committed to git and posted in tickets. §11 rule
6's reasoning applies with more force here than it does to logs.

- **Never export credential values.** `Auth::Bearer{token}` becomes a
  `securitySchemes` entry describing *that the endpoint uses bearer auth* —
  the token itself is not in the document. Same for Basic, ApiKey and Custom.
- **Never resolve `{{variables}}` against the active environment on the way
  out.** A resolved `{{api_key}}` is a leaked key in a file. Variables export
  as Server Variables with a *placeholder* default, not the live value.
- **Never export cookies.** The jar has no OpenAPI representation worth having.
- **Saved-response bodies are the residual risk**: a login response carries a
  token, and it is a real body we would otherwise happily embed as an example.
  This needs an explicit, defaulted-off decision, not a silent inclusion.

### Architecture

One internal model, N codecs — the only shape that survives three more
versions and an importer.

```
src-tauri/src/openapi/            # new top-level module, beside http/ and persistence/
  model.rs        version-neutral document (3.1-shaped + the 3.2 extras we use)
  infer.rs        body/value -> JSON Schema, pure
  url.rs          {{var}} URL -> server + templated path + parameters, pure
  v3_2.rs         serde structs + From<model> / TryFrom<-> for import
  v3_1.rs         same, minus the 3.2-only fields
  v3_0.rs         same, plus the JSON Schema downgrade
  downgrade.rs    what each emitter had to drop, as data
domain/services/openapi.rs        the use-case: repositories -> model -> codec
commands/openapi.rs               adapter; reuses Downloads + the save dialog
```

The serde structs live in `openapi/`, not `domain/models.rs`, for the same
reason `Stored*` lives in `persistence/repositories/json.rs`: they are a wire
format, and the domain should not deform to match a file spec.

`url.rs` and `infer.rs` are pure and carry the majority of the risk, so they
carry the majority of the tests — the pattern `domain/cookies.rs` already sets.

### The downgrade report is a first-class output

Emitting 3.0 from a 3.1-shaped model is lossy in specific, enumerable ways:

- `type: ["string", "null"]` → `nullable: true`
- `examples` (array) → `example` (single, first one)
- numeric `exclusiveMinimum`/`exclusiveMaximum` → boolean form + `minimum`
- `const` → single-value `enum`
- `contentEncoding`/`contentMediaType` → `format: byte` / `binary`
- `webhooks`, `license.identifier`, `$ref` siblings, `jsonSchemaDialect`: dropped
- `paths` stops being optional

Emitting 3.1 from the model drops only `$self`, Server `name`,
`additionalOperations`, `serializedValue`, `itemSchema` and the `querystring`
parameter location.

**Every emitter returns `(document, Vec<Downgrade>)`.** The UI shows the list.
The alternative — quietly producing a 3.0 document that says something subtly
different from what the user built — is the failure mode this design exists to
prevent.

### Phasing

**8a — skeleton export, 3.2 JSON.** Model, `url.rs`, the document shell
(`info`/`servers`/`paths`/`operations`/`parameters`/`security`), examples into
`responses`, no schema inference (`requestBody` carries an example, not a
schema). Reuses `Downloads::save` and the existing save dialog. Shippable and
useful on its own.

**8b — schema inference.** `infer.rs`: JSON body → Schema Object, arrays
unified across items, `required` from presence. This is where the document
stops being a URL list and starts being a contract, and it is the easiest part
to get subtly wrong.

**8c — 3.1 and 3.0 emitters + the downgrade report.** Cheap for 3.1, real work
for 3.0's JSON Schema rewrite.

**8d — import.** `TryFrom` on each codec, then model → collection: operations
become saved requests, `tags` become folders, Server Variables become an
environment, `examples` become saved responses. **Import is the harder
direction** — the spec allows far more than this app models (`callbacks`,
`links`, `discriminator`, `oneOf` bodies, `additionalOperations`), so it needs
its own "what was ignored" report, symmetric with the downgrade one.
*(2026-09-17: planned in full in its own section, "Phase 8d — OpenAPI import".)*

**8e — YAML.** OpenAPI is conventionally YAML, and this is a dependency
decision, not a formatting one: `serde_yaml` is deprecated and unmaintained,
so it means adopting a fork (`serde_norway`, `serde_yml`) and owning that
choice. JSON is valid OpenAPI, so 8a–8d ship without it.
*(2026-09-16: planned in its own section, "Phase 8e — YAML export".
`serde_yml` has since been flagged unsound, RUSTSEC-2025-0068.)*

### Testing

- **Golden files per version.** A fixture collection exercising every mapping
  row, with a committed expected document for 3.0/3.1/3.2. Diffs are the review.
- **Round-trip** (needs 8d; *done 2026-09-17, see Phase 8d as built*): collection → 3.2 → collection, asserting equality
  of everything the format can carry, and asserting the *report* names
  everything it cannot.
- **Validate against the official JSON Schema.** The OAI project publishes a
  schema per version; vendoring those and validating our output in a test is
  the only check that proves conformance rather than proving we agree with
  ourselves. Costs a validator crate (`jsonschema`) — worth it, and the one
  new dependency I would argue hardest for. *(2026-09-17: `jsonschema` 0.56 fails on the 3.1/3.2
  schema-base files; `boon` is used instead — see Phase 8d.)*
- `url.rs` and `infer.rs` get table-driven unit tests; they are pure.

### Decisions taken 2026-09-14

1. **Path templating: variables-only.** `{{var}}` is the user stating what
   varies, so it maps exactly and nothing else does. A leading variable becomes
   the Server; interior ones become path parameters; `/users/123` stays
   literal. `openapi/url.rs` implements exactly this and its 15 tests are the
   specification of it — including `a_numeric_segment_stays_literal`, which
   exists so nobody later "improves" the exporter into guessing.
2. **Saved responses as examples: opt-in, default off.** A login response
   carries a token. The dialog says why rather than just offering a checkbox.
3. **Build 8a first**, before 3.1/3.0, so the mapping is proven once rather
   than debugged three times.

### Still open

- **Schema inference depth** in 8b: top-level only, or full recursion with
  array unification?
- The `jsonSchemaDialect` string for 3.2 — confirm, do not assume. 8a sidesteps
  it by emitting no schemas worth a dialect, so nothing depends on this yet.

### 8a as built — where it departs from the plan above

**No `model.rs`.** The plan called for a version-neutral model with N codecs.
With one emitter that is a layer with a single implementation and no test seam
— the speculative abstraction §7 says to push back on. `from_collection.rs`
builds a `v3_2::Document` directly. The split stays cheap to make later
because the mapping is already a pure function returning
`(Document, Vec<ExportNote>)`: 8c extracts the shared fields into `model.rs`
and gives each emitter a `From<Model>`, which is mechanical.

**`ExportNote` is the 8a ancestor of the downgrade report.** Same principle —
what the export could not represent faithfully is data, surfaced in the UI,
never swallowed. Its five variants are collisions (one operation per method
and path), unmappable URLs, omitted credentials, omitted credential headers,
and flattened folder nesting. 8c adds version-specific losses alongside them.

**Files:**

```
src-tauri/src/openapi/mod.rs, url.rs, v3_2.rs, from_collection.rs
src-tauri/src/domain/services/openapi.rs      OpenApiExport
src-tauri/src/commands/openapi.rs             export_collection_openapi + describe_note
src/types/openapi.ts, src/services/openapi.ts
src/features/collections/ExportOpenApiDialog.tsx   (collection context menu)
```

Supporting edits: `AppError::Internal` (a serialiser refusing a value we built
ourselves is neither the user's fault nor the disk's), `Downloads::save_text`
sharing one private `write` with `save`, and every repository in `lib.rs` now
taking `database.clone()` so the next service added does not have to move a
line to compile.

**The security guarantee is structural, not procedural.** `OpenApiExport` has
no `EnvironmentRepository` handle, so it cannot resolve `{{api_key}}` even by
accident; server variables get `<name>` as their placeholder default. Three
tests in `from_collection.rs` assert a credential never reaches the document by
rendering the whole thing and searching it for the secret. Adding an
environments handle to that service is the change to argue about in review.

**Not in 8a:** schema inference (parameters get a bare `type: string`, bodies
get an example and no schema), 3.1/3.0, YAML, import, and the golden-file and
official-JSON-Schema validation tests — those land with 8c, where there is more
than one output to compare.

**Verified 2026-09-15**, first green `verify.bat` covering both this and the
logging slice: build, eslint, 121 frontend tests, `cargo fmt --check`,
`cargo clippy --all-targets -D warnings`, and 148 + 12 + 29 Rust tests. Of
those, 33 are 8a's — 15 in `url.rs`, 14 in `from_collection.rs`, 4 in
`v3_2.rs`. The `v3_2.rs` four are the ones that could not be checked offline
(no `serde` in the sandbox) and they are what proves the `in` / `type` renames
and the omit-don't-null rule actually reach the wire.

Two failures preceded that green run, both in the logging slice rather than in
8a:

- **E0716 in `logging.rs`'s test helper.** `format_args!` produces a temporary
  that cannot outlive its statement, so it can never be parked on a `Record`
  and read back on a later line. The helper now passes the message alongside
  the record — which is also the shape `tauri-plugin-log`'s format callback
  uses, so the test exercises the real calling convention rather than an
  approximation of it.
- **A dead `eslint-disable-next-line no-console`** in `services/logger.ts`:
  `no-console` is not enabled in `.eslintrc.cjs` and `eslint:recommended` does
  not include it, so `--report-unused-disable-directives` failed on it.
  Removed. **Open**: turning `no-console` on and putting the directive back
  would make `logger.ts` the enforced exception rather than the conventional
  one — a stray `console.log(request.body)` in a component is exactly what
  §11 rule 6 forbids and nothing currently stops.

**Manual click-through for 8a, not yet done:** export a collection with no
saved responses and confirm every operation carries the `default` placeholder;
export one with `{{base_url}}` and confirm the Server variable's default is
`<base_url>` and not the environment's value; export a request with Bearer auth
and grep the file for the token; put two GETs on the same path and confirm the
collision note names the dropped one; dismiss the Save dialog and confirm the
notes still show.

---

## Phase 8c — OpenAPI 3.1 and 3.0 export — **green on verify.bat 2026-09-15**

Taken ahead of 8b, which is the better order: fewer schemas now means less to
downgrade later.

### What the spec actually required

Checked against the **official 3.1 JSON Schema** (`schemas/v3.1/schema.json` in
the OAI repo) rather than the prose, because the prose fetch kept truncating
the object tables. The result was smaller than the plan assumed:

**The entire difference between what this exporter emits and valid 3.1 is one
field.** `$defs/server` has no `name` (we always emitted `None`),
`$defs/parameter`'s `in` enum is exactly `query|header|path|cookie` (we emit
path/query/header), `$defs/components` has no `mediaTypes`, and there is no
`$self`. All of it already validated.

The exception is the Example Object:

```
"example": { "properties": { "summary", "description", "value", "externalValue" },
             "unevaluatedProperties": false }
```

`serializedValue` — the one 3.2 feature this exporter genuinely uses — is not
merely absent in 3.1, it is **rejected**: `unevaluatedProperties: false` means
emitting it invalidates the whole document rather than being ignored.

`openapi` is constrained to `^3\.1\.\d+(-.+)?$`, so any patch validates. We
emit **`3.1.0`**: the spec says major.minor designates the feature set and
patch releases only correct the prose, so 3.1.0 claims exactly what we support
while staying acceptable to validators that match the string exactly.

### Decision 2026-09-15: one struct set, version as a value

**Supersedes this phase's "model.rs plus one emitter per version" sketch.** A
second near-identical struct set for a one-field difference is the duplication
§7 forbids, and it would drift. `openapi/v3_2.rs` is renamed
`openapi/document.rs` — it is version-neutral now and the old name was a lie —
and `OpenApiVersion` lives there as an argument threaded through
`to_document`. It changes exactly two things: the `openapi` string, and which
of two mutually exclusive Example fields is populated.

**3.0 is where the real split earns itself**: `nullable`, a single `example`,
boolean `exclusiveMinimum`, no `webhooks`, `paths` mandatory again. Those are
structural, not cosmetic, and that is the point to revisit this.

### `value` means decoded, and that matters

3.2's `serializedValue` exists precisely because `value` is defined as the
*decoded* representation. So the 3.1 path parses a JSON body into a real JSON
value instead of embedding it as a string — otherwise the document would claim
the endpoint returns a quoted blob of JSON text. **This is correctness, not the
schema inference 8b is about.** Non-JSON bodies stay strings, which is not a
fallback: the decoded form of `text/plain` *is* a string. A body that claims
JSON and fails to parse also stays a string, because reproducing what the
server actually sent beats discarding it. `application/problem+json` and the
rest of the `+json` structured suffix count as JSON (RFC 6839).

New `ExportNote::ExampleBodyDecoded`, emitted when 3.1 is chosen **and** saved
responses were included — conditioned on content rather than on the version
alone, since a note that fires on every 3.1 export is noise.

### Also

- The version is now in the suggested file name (`work_api.openapi-3.1.json`),
  so exporting one collection at both versions does not offer to overwrite.
- The export dialog gained the version picker 8a deliberately left out,
  defaulting to 3.2.
- An unrecognised version string from the frontend is an `InvalidRequest`, not
  a silent default — the two sides disagreeing is a bug to surface.

**Verified offline:** 28 tests (21 in `from_collection`, 7 in `document`),
clippy clean, `tsc` clean. The serde layer again could not be compiled here —
no crates.io — so the four wire-shape tests in `document.rs` are what prove
3.1 never emits `serializedValue`.

**Left behind:** `src-tauri/src/openapi/v3_2.rs` was orphaned — no `mod`
declared it. *Deleted 2026-09-17.*

---

### 3.0, added the same day

Checked against the official 3.0 schema (`spec.openapis.org/oas/3.0/schema`,
fetched from the OAI repo — the `schemas/v3.0/schema.json` path in `main` is
404, it is `schema.yaml` at a pinned commit). **Also nearly free, and for a
reason worth stating plainly: our documents are trivially portable because we
emit almost nothing.**

| 3.0 requires | us |
|---|---|
| `^3\.0\.\d(-.+)?$` (one digit) | `3.0.0` |
| `openapi`, `info`, **`paths`** all required | `paths` is the one map serialised unconditionally |
| Example: `summary`/`description`/`value`/`externalValue`, `additionalProperties: false` | already using `value` for pre-3.2 |
| Info: no `summary` (3.1 added it) | never populated |
| Components: no `pathItems` | only `securitySchemes` |
| SecurityScheme type ∈ apiKey/http/oauth2/openIdConnect (no `mutualTLS`) | http, apiKey |
| Parameter `in` ∈ path/query/header/cookie | path, query, header |

So the whole of 3.0 was: a third enum variant, and `example_body` keying off
`OpenApiVersion::has_serialized_value()` instead of matching 3.2 against 3.1.

**The catch, and it is a real one.** 3.0's genuine divergence is the Schema
Object — Draft-4-flavoured: `nullable` rather than a `null` type, boolean
`exclusiveMinimum`, a single `example`, no `const`, no `contentEncoding`. None
of that bites today because the only Schema we emit is `{"type": "string"}`,
valid in all three versions. **Phase 8b is where 3.0 stops being free**, and
two tests exist purely to say so when that day comes:

- `document::tests::a_schema_is_still_only_a_type` — pins the serialised Schema
  Object to exactly `{"type":"string"}`. Anything richer fails it.
- `from_collection::tests::a_3_0_document_carries_no_info_summary` — the same
  trick for the Info Object field 3.0 does not have.

Neither is a behaviour test. Both are tripwires, and the comments say so, so
nobody deletes them as redundant.

`has_serialized_value()` matches exhaustively rather than using a wildcard: a
version added later must decide for itself rather than inherit an answer.

**Version strings: `3.0.0`, `3.1.0`, `3.2.0`** — the `.0` patch in each case,
not the newest. Major.minor designates the feature set; patch releases only
correct the prose. A newer patch would validate, it would just claim
conformance to prose we have not read.

**Verified offline:** 39 tests across the openapi module (24 in
`from_collection`, 9 in `document`, plus `url`), clippy clean, `tsc` clean.
The offline tsc caught one real mistake — a stale two-argument call to
`exportCollectionOpenApi` — which turned out to be a stale staged file rather
than committed code, but the check did its job.

**Version chooser stays in the export dialog**, not the context menu: one menu
entry that opens a dialog beats three menu entries, and the dialog already has
to exist for the saved-responses opt-in.

---

## Phase 8b — schema inference — **green on verify.bat 2026-09-15**

`openapi/infer.rs`. Request and response bodies now carry a real Schema Object
instead of nothing, and parameters keep their bare `type: string`.

### Why it is an accumulator and not a translator

**An array has exactly one `items` schema, however many differently-shaped
elements it holds.** That single fact sets the shape of the module: inference
cannot map one value to one schema, it has to fold observations together and
then describe what they had in common. `Observed` is that accumulator, and the
same machinery serves the other half of the job — merging every saved response
for one status code into one schema.

`Observed` carries flags (`null`, `boolean`, `integer`, `float`, `string`) plus
optional object and array detail, rather than "the" type. A position can
legitimately have held a string in one response and a number in another;
flattening that to a single guess is where an exporter starts lying.

### Decisions 2026-09-15

1. **Full recursion, no depth cap.** None is needed and none was added: serde_json
   refuses to parse past 128 levels of nesting, and `decode` already treats an
   unparseable body as text. The bound exists upstream, where rejecting a
   malformed document is still cheap.
2. **`required` is intersection, not union.** A key is required only if *every*
   object observed at that position carried it — across array elements and
   across separate saved responses alike. One response proves a key was present
   that once; two that disagree prove it optional.
3. **Conflicting types become `oneOf`, in every version.** 3.1 could write
   `type: ["integer","string"]` and 3.0 could not. `oneOf` is valid in all
   three and says the same thing, so there is no version-specific union logic
   to get wrong. This is the decision that kept 3.0 from needing its own
   emitter after all.
4. **`integer` widens to `number`** when a non-integral value appears. 1 and
   1.5 in one position are one numeric field, not two types in conflict.
5. **An empty array gets no `items`.** It proves the position is an array and
   nothing else; inventing an item schema from no items is a guess.
6. **No format guessing.** A string that looks like a timestamp is not
   necessarily one, and a wrong `format` is worse than an absent one — client
   generators turn it into a type.

### Where the versions still differ

Exactly one place, and `OpenApiVersion::has_null_type()` is it. 3.1 aligned the
Schema Object with JSON Schema 2020-12, where `null` is a type and `type` may
be a list; 3.0's is Draft-4-flavoured with the separate `nullable` keyword.

```
3.1  {"type":["string","null"]}        3.0  {"type":"string","nullable":true}
3.1  {"type":"null"}                   3.0  {"nullable":true}
```

The two 3.0 tripwires from the previous slice have now fired and been replaced:
`a_schema_is_still_only_a_type` is gone, and `Schema` has grown `nullable`,
`properties`, `required`, `items` and `one_of`. `a_3_0_document_carries_no_info_summary`
still stands.

### One parse, one body

`request_body_for` and `responses_for` now decode a body **once** and use the
value twice — for the schema and for the example — so a document can never
describe a shape its own example contradicts. `example_body` takes the decoded
value rather than decoding again.

### Verified offline

55 tests across the openapi module (14 in `infer`, 12 in `document`, 26 in
`from_collection`, plus `url`), clippy clean, rustfmt clean.

This slice needed a **throwaway serde_json stand-in** in the scratch harness —
`Value`, `Number::is_f64`, a `json!` macro and a hand mirror of `Schema`'s serde
attributes — because inference is entirely about `serde_json::Value` and
crates.io is unreachable from the remote sandbox. That means the algorithm and
its field ordering are verified there, while the serde attributes themselves are
still only verified by `cargo test` on the real machine. The hand mirror is
scratch and is not in the repository.

### The harness gap this slice exposed

`verify.bat` failed once, on two errors in the **test module of `document.rs`**:
a `Schema` literal still spelling `kind: Some("string".into())` after the struct
grew four fields and `kind` became a `SchemaType`. Nothing caught it because the
offline harness *stripped* that file's test module — it asserts on
`serde_json::to_string`, which the sandbox cannot provide.

Fixed properly rather than by hand: the harness now **compiles** that module
instead of dropping it, by keeping the code, deleting the `#[test]` attributes
so nothing runs, and pointing `to_string` at a Debug formatter. Struct literals
— exactly the class of thing that broke — are type-checked; the serde
assertions are honestly left to the real machine. Reverting the fix in the
harness reproduces both compiler errors, so the guard is known to work.

**The lesson, which is now twice-learned** (zustand being the first): the gap
to worry about is never the code the harness checks, it is the code the harness
silently skips. Anything excluded to make an offline check possible should be
excluded from *running*, not from *compiling*.

### Green, 2026-09-15

`verify.bat` clean across the whole backlog that had accumulated — the stdout
log target, 3.1, 3.0 and 8b: **182 Rust unit tests + 12 curl integration + 29
repository**, 128 frontend, eslint, clippy and `cargo fmt` all clean.

Worth recording, because it was the standing caveat on three slices: **every
`infer` test passed against real serde, including the exact string assertions**
(`{"type":["string","null"]}` for 3.1, `{"type":"string","nullable":true}` for
3.0, `{"oneOf":[...]}`, `{}` for an empty schema). The hand mirror in the
offline harness agreed with serde exactly, so the serde attributes on the grown
`Schema` — the part the sandbox could not check — are now verified rather than
assumed.

---

## Ctrl+S to save the focused request — **built 2026-09-15; checked 2026-09-16; `verify.bat` not yet run**

### As built

Option B was chosen: a window listener that fires while a request tab is active.
The code is `lib/shortcuts.ts` (`matchesSaveShortcut`, 7 tests) and one effect in
`App.tsx`. The handler lives in a ref, so the listener is not re-subscribed on
every render. The shortcut is ignored while a modal is open. It checks for
`[data-modal]`, which `components/Modal.tsx` sets. It does not check the dialogs'
open flags, because the export dialog's flag is not visible to App.

**Monaco risk closed (2026-09-16), checked in the source, not assumed.**
In monaco-editor 0.52.2, `StandaloneKeybindingService` calls `preventDefault` and
`stopPropagation` only when `_dispatch` finds a keybinding. No keybinding uses
`KeyCode.KeyS` anywhere in the ESM build, so Ctrl+S bubbles up to the window. No
change to `CodeEditor.tsx` was needed.

**Bug found and fixed while checking (2026-09-16).** `SaveExampleDialog` drew its
own backdrop instead of using `Modal`. Because it had no `data-modal`, pressing
Ctrl+S while naming an example saved the request behind the dialog. It now
renders inside `Modal`. That also makes it the last dialog to use the shared
shell (CLAUDE.md §6). Visible changes: clicking the backdrop now closes it like
every other dialog, and the input's own Escape handler was removed, because
`Modal` already handles Escape.

**Checked on the device shell (2026-09-16):** `tsc --noEmit` passes on the whole
project, eslint and prettier pass on the touched files, and
`vitest run src/lib/shortcuts.test.ts` passes 7 of 7. These checks ran against
the project's real `node_modules`. That removes the zustand blind spot
described in Phase 7, which only applied to the remote sandbox. Vitest needed
the Linux esbuild and rollup binaries, installed outside the project folder. A
full `vitest run` over the mounted folder takes more than 3 minutes, so only the
new file was run. `verify.bat` remains the gate.

**Manual checks:** Ctrl+S with the caret in the URL bar, in a header row, and in
the body editor, on a saved tab (the dirty dot clears) and on a new tab (the
Save dialog opens). Ctrl+S with an environment tab in front does nothing. Ctrl+S
while any dialog is open does nothing, including Save Response. Holding Ctrl+S
saves once.

### Original plan (2026-09-15)

Not part of a numbered phase; same bucket as the splitter and collapse work.

### The one decision worth making first

Asked for as: fires when focus is *inside* the request panels or buttons.
That is one of two readings, and they behave differently.

**A — listen on RequestBuilder's root.** React's `onKeyDown` bubbles from
whatever descendant has focus, so this is naturally scoped: no global
listener, no activeElement whitelist, and no chance of stealing Ctrl+S from an
environment tab or an example viewer. The cost is the classic bad-shortcut
experience: press it with focus on the tab strip, in the sidebar, or nowhere
at all, and nothing happens with no explanation.

**B — window listener, guarded by `activeTab.kind === "request"`** (recommended).
The real precondition is not "where is the caret" but "is the thing on screen
a request". This is what every editor the user already has open does, and the
target is never ambiguous because there is exactly one active tab. The cost is
that Ctrl+S while typing in the sidebar's inline rename field would also save
the request — harmless, since rename commits on Enter, but it is the honest
downside.

Either way the shortcut must be inert while a modal is open
(`saveDialogOpen`, `closingTabId`, `cookiesOpen`, `savingExample`, the export
dialog), which App already holds as booleans.

### What "save" already means

`App.handleSaveClick` is the whole behaviour and needs no change:

- tab has a `loadedRequest` → overwrite in place, silently
- tab has never been saved → open SaveRequestDialog for a name and a
  collection

So Ctrl+S on a new tab opens the dialog, which is what every editor does with
Save on an untitled file. Nothing new to design.

### Feedback is already there

A silent save triggered by a keystroke is the case where "did that work?" bites,
and the answer already exists: `TabBar` renders a dot for `tab.isDirty`, and
`markSaved` clears it. Pressing Ctrl+S makes the dot disappear. **No toast, and
no toast system** — that would be a new pattern for one message.

### Where the logic goes

`lib/shortcuts.ts`, pure and tested, because a key matcher is exactly the kind
of thing that is subtly wrong in ways nobody notices:

```ts
export function matchesSaveShortcut(event: ShortcutEvent): boolean
```

Takes a plain `{ key, ctrlKey, metaKey, altKey, shiftKey, repeat }` rather than
a `KeyboardEvent`, so it tests without a DOM. It must be false for Ctrl+Shift+S,
Ctrl+Alt+S and a held repeat, and true for both Ctrl+S and Cmd+S — the app
builds for macOS too, and `metaKey` is two characters.

### The risk worth naming before starting: Monaco

The body editor is Monaco, which installs its own keydown handling and
`preventDefault`s anything it has bound. Monaco's standalone editor has no save
command, so **Ctrl+S should pass through to the DOM — but that is an
expectation, not a verified fact**, and it is the single thing most likely to
make this not work.

If it does swallow the key, the fix is not a workaround in App: it is
`editor.addCommand(KeyMod.CtrlCmd | KeyCode.KeyS, ...)` in an `onMount` on
`components/CodeEditor.tsx`, which today has no `onMount` and no `onSave` prop.
That is a prop threaded through LazyCodeEditor and BodyEditor. **Check this
first**, with focus in the body editor, before writing anything else — it
decides whether this is a thirty-line change or a hundred-line one.

### Also required

- `preventDefault()` unconditionally. A webview may otherwise offer "Save page
  as".
- Ignore `event.repeat`, or holding the keys fires a save per repeat.
- No change while a request is in flight: the template being saved is
  independent of the response.

### Not doing

A shortcut registry, a keyboard-shortcuts help panel, or any other binding.
One shortcut does not justify an abstraction (§7, YAGNI); the second one is
when a registry earns itself.

### Tests

`lib/shortcuts.test.ts` on the matcher: Ctrl+S, Cmd+S, the three modifier
combinations that must not match, a held repeat, and a bare "s". The wiring
itself is a one-line call and is covered by the click-through.

---

## UI polish — 2026-09-15

Three fixes from a click-through, none of them in a numbered phase.

**Tab strip contrast.** The strip painted `bg-muted/30` over `bg-background`,
which resolves to about one point of lightness away from the pane it sat on,
and the active tab was `bg-background` — *darker* than the strip — leaving
`shadow-sm` (invisible on a dark surface) and white-vs-grey label text as the
only marks of selection. Strip is now `bg-card`, a real step down; the active
tab keeps the content colour and so reads as raised; and it gains the same
`border-primary` underline the Params/Auth/Headers row already used, so the two
levels of tabs agree. Hover was a second bug found only by rendering it:
`bg-accent` is lighter than the active fill, so hovering an inactive tab made
it look *more* selected than the selected one. Hover is now a fraction of the
active fill, keeping the ladder strip < hover < active.

**Scrollbars.** The webview was rendering the platform's light scrollbar —
white gutter, arrow buttons — inside a dark app. `color-scheme: dark` on
`:root` is the part that matters (it also stops a native `<select>` dropping a
white popup over Settings), plus `::-webkit-scrollbar` rules putting the thumb
on `--border` with a transparent track. Deliberately **not** paired with
`scrollbar-width`/`scrollbar-color`: since Chromium 121, setting either
standard property makes the engine ignore every `::-webkit-scrollbar` rule, so
using both would silently discard the styling. Monaco draws its own scrollbars
and is unaffected — matching those is a `defineTheme` call, deferred.

**Draggable divider** between the request panel and the response viewer. The
panel's `h-64` became a dragged height.

- `lib/split-pane.ts` — `clampPaneHeight`, pure, 7 tests. Takes *available*
  space (panel top to the bottom of the shared area), not a container height:
  the panel is not the only thing above the response, so a container height
  alone cannot say what the panel may take. When the window is too short to
  honour both minimums the response wins — a cramped panel is annoying, a
  response you cannot see is the thing the window was opened for.
- `features/request-builder/PanelResizeHandle.tsx` — pointer capture so a fast
  drag does not escape the 6px strip, double-click or Home to reset, arrow
  keys to nudge, `role="separator"`. Emits a *desired* height and does no
  clamping of its own.
- `store/layout-store.ts` — **decided 2026-09-15: session-only.** The height
  resets on restart. Persisting it means a `ui_state` table, a migration, a
  repository and a command for one integer; worth doing once there is other
  layout state to keep beside it (window size, sidebar width), not before. It
  lives in a store rather than in `RequestBuilder` because switching to an
  environment or example tab unmounts the builder, and one global position is
  what every editor with a split view does.
- Re-clamps on window resize. Without that, shrinking the window strands a
  now-illegal height and pushes the response off the bottom permanently.

The one assumption to revisit: available space is measured as
`window.innerHeight - panelTop`, which is exact because the app is a single
`h-screen` column with nothing below the response viewer. Add a status bar
down there and it has to become a measured container.

**Verified 2026-09-15**, `verify.bat` green: 128 frontend tests across 17 files
(up 7, all of them `split-pane`), 148 + 12 + 29 Rust, eslint and clippy clean.
Nothing in this slice had a test before the divider — the two colour fixes are
CSS and were checked by rendering them in Chromium at the real token values,
which is what caught the hover inversion.

**Not done:** `no-console` as an enforced lint rule (offered, declined for
now), and the Monaco theme.

---

## Phase 9 — Secrets encrypted at rest (envelope encryption) — **done, verified 2026-09-16**

This settles the Phase 4 secrets decision. **Option chosen 2026-09-16: one
data key in the OS keychain, secrets encrypted in SQLite.** This is the usual
envelope pattern, and browsers protect saved data the same way.

Options considered and rejected:

- **One Credential Manager entry per secret** (the first draft of this
  phase). Windows caps each value at 2,560 bytes (`CRED_MAX_CREDENTIAL_BLOB_SIZE`,
  documented by Microsoft), so long tokens would need splitting. Deleting a
  collection leaves entries behind, because SQLite cascades cannot reach the
  keychain. Windows also does not guarantee that writes from different threads
  arrive in order. All three problems come from storing each secret in a
  second store.
- **DPAPI only** (`CryptProtectData`). Simpler, but Windows-only, and
  CLAUDE.md still lists macOS and Linux as targets.
- **Stronghold.** It makes the user type a password, and its normal use goes
  through JavaScript, which conflicts with §11 rule 3.

### What this protects, and what it does not

It protects the app-data database whenever the file is copied, backed up,
synced, attached to a ticket or lifted off a disk. It does **not** protect
against malware running as the same Windows user. No option considered here
does, and Credential Manager has the same limit.

Decrypted values still cross IPC, because the UI edits them and
`lib/variables.ts` substitutes them in the frontend. The protection is for
data at rest, not data in memory.

### What we accept

- **Secrets do not travel with the database.** A database copied to another
  machine, or opened after the keychain entry is gone, loads everything
  except the secrets. Each affected secret field loads empty and is marked
  "needs re-entering". A missing secret never causes a load to fail.
- **Two new direct dependencies, and possibly less new code than that.**
  - `aws-lc-rs` **is already compiled into the binary.** `Cargo.lock` has
    1.18.1 as a dependency of `rustls` and `rustls-webpki`, pulled in by the
    rustls-ffi build that Phase 0 proved. Depending on it directly should not
    add any new crypto code.
  - `keyring-core` 1.x and `windows-native-keyring-store` 1.x are new. The
    store calls Credential Manager, which lives in `advapi32`, and that DLL is
    already on the static-link allowlist. `check-windows.ps1` must confirm
    both claims after the first build.
  - The `keyring` 4.x umbrella crate is **not** used. Its own docs point apps
    at `keyring-core` plus explicit stores. Setting the store explicitly also
    means a build can never fall back to the mock store without anyone
    noticing, which keyring 3.x does when a feature flag is missing.
  - The APIs were read from docs.rs on 2026-09-16 and not compiled:
    - `keyring_core::set_default_store`
    - `Entry::new(service, user)`, plus `set_secret(&[u8])`, `get_secret() -> Vec<u8>` and `delete_credential()`
    - `Error::NoEntry`
    - `windows_native_keyring_store::Store::new()`
    - `aws_lc_rs::aead::RandomizedNonceKey::{new, seal_in_place_append_tag, open_in_place}`, with `AES_256_GCM`

### Decisions carried over from the earlier draft

1. **What counts as a secret:** the Auth fields (Basic password, Bearer
   token, API key value, Custom value) and environment variables marked with a
   new **"secret"** flag. Header values and cookie-jar values are not secrets
   and stay as plain text. The UI should steer credentials toward the Auth tab
   or toward a secret variable.
2. **History and saved responses (examples) keep no secrets.** A secret
   field holding a **literal** value is blanked before it is stored. A field
   that is entirely a `{{placeholder}}` is kept, because it contains no
   secret, and a re-run then works with no re-typing.
   - The example snapshot is stored after substitution. It must be built by
     substituting **non-secret variables only**, so a secret variable stays
     as `{{name}}` in the URL, headers and body.
   - Both rules are enforced in the Rust services (`History::record` and the
     example save), not only in the frontend. The rule has to hold whichever
     code calls it.
3. ~~Splitting values larger than 2,560 bytes~~: **no longer needed.** The
   keychain now holds one small key, and secret values have no size limit.

### The key

- **32 random bytes** from `aws_lc_rs::rand`. They are stored as one
  Credential Manager entry: service `io.github.zoran-php.responderhttp`, user
  `data-key`. See "Keychain entry" below for the exact Windows name.
- The stored secret is `key_id (16 random bytes) ‖ key (32 bytes)`. Every
  encrypted value records the `key_id` it was encrypted under, so a
  replacement key is never mistaken for the old one.
- **Read once at startup** and held in memory by the cipher service for the
  whole session. The keychain is written to only when a key is created, so
  the thread-ordering caveat no longer applies.
- **Startup cases:**
  - **Key present:** use it.
  - **Key missing** (`NoEntry`): create a new key and store it. Any value
    encrypted under the old `key_id` now reports "needs re-entering". It is
    never reported as corrupt, and it is never decrypted with the wrong key.
  - **Keychain unavailable** (access denied, service failing): the app still
    starts. Secrets load as unavailable, and saving a secret fails with a
    typed `AppError::SecretStore`. **Never fall back to writing plain text.**
- The key is never logged, never serialised, and never crosses IPC (§11
  rule 6).
- **Test builds never touch the real keychain.** The key source is a port,
  and unit tests use an in-memory one.
- **No rotation yet.** The format version byte and `key_id` make it possible
  later without a format change (§7, YAGNI).

### Keychain entry — decided 2026-09-16

> **Renamed 2026-09-18.** The identifier became
> `io.github.zoran-php.responderhttp` (Phase 10), so the target name is now
> `data-key@io.github.zoran-php.responderhttp`. Every name quoted from here on,
> including in the 2026-09-16 verification records, has been updated to the
> current one; what was observed on the day carried the original identifier.

1. **Roaming: keep the store's default (Enterprise).** The key follows a
   roaming Windows profile. The database is in `app_data_dir()`
   (`lib.rs`), which on Windows is the roaming AppData folder, so the key and
   the database travel together. On a PC without roaming profiles this
   behaves the same as Local. The setting is applied once, when the key is
   created, and never changed afterwards, because the store's docs warn that
   changing it just before a read can make the read fail.
2. **Configure the store to reject the delimiter in service names.** Without
   this, two different service/user pairs can map to the same Windows entry.
   - **Consequence:** the default delimiter is `.`, and the service name
     `io.github.zoran-php.responderhttp` contains dots. With this option on, the
     store would reject our own service name, so the delimiter has to change
     too.
   - **Proposed delimiter: `@`.** It appears in neither name, and it gives
     the Windows target name `data-key@io.github.zoran-php.responderhttp` (the store
     builds prefix + user + delimiter + service + suffix, with empty prefix
     and suffix). **Confirmed by the user 2026-09-16.**
   - Both options are set through `Store::new_with_configuration`. The exact
     configuration keys still need reading from the crate before this is
     built.
3. **Dev and release builds share the key.** `pnpm tauri dev` and the
   installed app already share one database, because both use
   `app_data_dir()` under the same identifier. Separate keys would only make
   each build's secrets unreadable in the other. Automated tests still never
   touch the real keychain.
4. **Uninstall removes the key only when "Delete the application data" is
   ticked,** so the key and the database are always removed together.
   - **NSIS:** add `bundle.windows.nsis.installerHooks` pointing at a `.nsh`
     file that defines `NSIS_HOOK_POSTUNINSTALL`:

     ```nsis
     !macro NSIS_HOOK_POSTUNINSTALL
       ${If} $DeleteAppDataCheckboxState = 1
       ${AndIf} $UpdateMode <> 1
         nsExec::Exec 'cmdkey /delete:"data-key@io.github.zoran-php.responderhttp"'
       ${EndIf}
     !macroend
     ```

   - **Why this is sound**, checked against Tauri's `installer.nsi` template
     on 2026-09-16:
     - The checkbox's app-data deletion removes `$APPDATA\${BUNDLEID}`,
       which is where our database is.
     - It uses the same two conditions as above: the box is ticked, and the
       uninstall is not part of an update. An update must never remove the
       key.
     - `$DeleteAppDataCheckboxState` is set in `un.ConfirmLeave`, before the
       uninstall section runs, so the hook can read it.
   - **Why `cmdkey`:**
     - It is part of Windows (System32), so nothing is added to the
       installer.
     - By the time this hook runs the app's own exe has been deleted, so the
       app cannot be asked to remove its key.
     - The install is per-user (`installMode: "currentUser"`), so the
       uninstaller runs as the user who owns the Credential Manager entry.
   - **Caveats:**
     - The hook's target name must match what the store actually writes.
       Check it once in Credential Manager after the first real run, and keep
       the name in one place in the code, with a comment pointing at the
       hook.
     - `$DeleteAppDataCheckboxState` is a variable inside Tauri's template,
       not a documented API, so a Tauri upgrade could rename it. Add it to
       the manual checks for every Tauri upgrade.
     - `nsExec::Exec` ignores failures. If deleting the key fails, the only
       result is a leftover entry, which is the harmless direction.
   - **MSI:** it has no "delete app data" option, so an MSI uninstall leaves
     both the database and the key. That still keeps them together.

### Stored format

`sealed = version (1 byte, = 1) ‖ key_id (16) ‖ nonce (12) ‖ ciphertext ‖ tag (16)`

- **AES-256-GCM through `RandomizedNonceKey`.** The library generates a
  fresh nonce for every seal, so our code never picks one.
- **Associated data binds each value to where it is stored:**
  `responderhttp/v1/<table>/<row id>/<field>`, for example
  `responderhttp/v1/requests/<uuid>/auth.token`. A sealed value copied into a
  different row or field fails to decrypt instead of quietly producing
  another row's secret.
  - Environment variables use `environment_id` plus `position`. That is safe
    only because `set_variables` rewrites the whole set, and so re-encrypts,
    on every save. **If that ever becomes a partial update, the associated
    data has to change as well.**
  - Duplicating a request gives it a new id. Its secrets are re-encrypted,
    because the frontend sends plain text and the service seals it again.
- **Decryption failures are sorted into:**
  - **unknown `key_id`:** "needs re-entering"
  - **authentication failure under the current key:** "unreadable". Logged
    with the table, row id and field only, never the value.
  - **unknown version byte:** a typed error, which is the same stance as
    "a database from the future" in Phase 7.

### Where the code goes

```
domain/ports.rs          DataKeySource   (load_or_create)      — keychain behind a trait
                         SecretCipher    (seal / open)
domain/models.rs         SecretValue = Plain(String) | Sealed(Vec<u8>) | Unavailable
secrets/                 new infrastructure module, beside http/ and persistence/
  envelope.rs            AesGcmCipher: the format above, pure given key bytes
  keychain.rs            KeyringDataKeySource (keyring-core + windows store)
  memory.rs              in-memory DataKeySource for tests
domain/services/…        seal before a repository write, open after a read
```

- **Repositories never see a key** (§7 SRP, §11 rule 4). They serialise
  whichever `SecretValue` variant they are given. A sealed value is stored as
  base64 inside the JSON columns, and as a BLOB in the new variables column.
- **Only `Plain` crosses IPC.** A value that could not be decrypted is sent
  as an empty string with a `needsReentry` flag. The DTO never carries
  ciphertext.
- **base64:** `http/auth.rs` has a hand-written encoder but no decoder. Either
  add a decoder next to it, or depend on `base64` 0.22, which is already in
  the lock file through other crates. **Decide this when building.**

### Upgrading existing databases

- **Migration `0007_secrets.sql`:** adds
  `environment_variables.secret INTEGER NOT NULL DEFAULT 0` and
  `environment_variables.value_sealed BLOB`. Shipped migrations stay
  untouched (§11 rule 5).
- **One-time step in Rust, run after `migrate()`.** SQL cannot encrypt, so a
  `.sql` file cannot do this step. It:
  1. encrypts every plain-text Auth secret in `requests.auth_json`
  2. blanks literal Auth secrets in `history` and `examples`
  3. blanks those rows' `auth_json` of literals, as in decision 2
  - Each row gets its own transaction. Rows that are already sealed or blank
    are skipped, so a crash midway is fixed by the next start.
  - Plain-text values already in history URLs or bodies are **not**
    rewritten. The step cannot know which strings in them are secrets.
- **Leftover plain text on disk.** Deleted and overwritten plain text can
  survive in SQLite's free pages and WAL.
  - Set `PRAGMA secure_delete = ON` on every connection, so future deletes
    zero their data.
  - Run `VACUUM` once after the step has converted anything.
  - **Without these, the "Done when" check below would still find the
    token.**

### Frontend

- **Secret flag:** a new column in `EnvironmentEditor`.
  - **`KeyValueTable` cannot simply grow a column.** It is the one shared
    table (§6), and headers, params and form fields have no use for this
    column. Decide when building whether it becomes an optional column there
    or a separate row shape, as `MultipartTable` is.
- **Masking:** secret values are masked by default and have a reveal toggle.
  The same applies to the Auth tab's secret fields.
- **"Needs re-entering":** shown on any field that loaded that way. The
  request itself still loads and sends.
- **Example snapshots:** `substituteRequestInput` gains a mode that leaves
  secret variables as placeholders.

### Tests (planned)

- **Envelope code, with a fixed test key:**
  - round trip
  - sealing the same text twice gives different ciphertext
  - wrong associated data fails
  - a flipped ciphertext byte fails
  - an unknown `key_id` reports "needs re-entering"
  - an unknown version byte is a typed error
- **Services, against the in-memory key source:**
  - seal on save and open on load
  - a missing key loads the value as empty with the flag set, and does not
    fail
  - history and examples blank literal secrets and keep `{{placeholders}}`
- **Upgrade step:**
  - plain-text rows are sealed or blanked
  - a second run changes nothing
  - a half-converted database finishes converting
- **The "Done when" check as a test:**
  - write a known token through the real repository into a **file-backed**
    temporary database
  - run the upgrade step and `VACUUM`
  - read the raw file bytes and assert the token is not in them
  - This is the only test here that proves the goal rather than the
    mechanism.
- **Real keychain:** one Credential Manager round trip, marked `#[ignore]`
  and run by hand. Neither the offline test harness nor CI has a keychain.

### Open questions (confirm when building)

- That `keyring-core` 1.x and `windows-native-keyring-store` 1.x build
  together, and the exact `new_with_configuration` keys for the delimiter,
  the service-name rule and persistence.

- Whether depending on `aws-lc-rs` directly really adds no new code.
  `cargo tree -d` and the size of the release exe will show it.
- The base64 choice, and the `KeyValueTable` choice, both listed above.

**Done when:**
- Uninstalling through NSIS with "Delete the application data" ticked
  removes the Credential Manager entry. Unticked, or during an update, it
  stays.
- A token saved through the Auth tab or a secret variable cannot be found
  anywhere in the raw app-data database file, including after an upgrade from
  a database that held it in plain text.
- Credential Manager holds exactly one ResponderHTTP entry.
- Deleting that entry makes secret fields load as "needs re-entering"
  without breaking anything else.
- No new DLL appears in `check-windows.ps1`.

---

### As built (2026-09-16)

**Files**

```
src-tauri/src/domain/secrets.rs              which fields are secrets, scopes, strip rules, SecretState (pure)
src-tauri/src/domain/ports.rs                SecretCipher, DataKeyStore, OpenedSecret; variables are EnvironmentVariable
src-tauri/src/domain/models.rs               EnvironmentVariable; SavedRequest.secret_state
src-tauri/src/domain/error.rs                AppError::SecretStore (ApiError kind "secretStore")
src-tauri/src/secrets/mod.rs                 open_cipher: only "no entry" creates a key
src-tauri/src/secrets/data_key.rs            key_id ‖ key, generated from aws-lc's RNG, zeroized, Debug prints nothing
src-tauri/src/secrets/envelope.rs            EnvelopeCipher (AES-256-GCM, RandomizedNonceKey), UnavailableCipher
src-tauri/src/secrets/keychain.rs            KeychainDataKeyStore (Windows); other platforms report the store unavailable
src-tauri/src/secrets/memory.rs              in-memory key store for tests
src-tauri/src/persistence/migrations/0007_secrets.sql
src-tauri/src/persistence/repositories/json.rs            StoredSecret, SecretWrite::{Seal, Strip}, SecretRead
src-tauri/src/persistence/repositories/secret_upgrade.rs  startup upgrade + VACUUM
src-tauri/windows/hooks.nsh                  uninstall hook; tauri.conf.json > nsis.installerHooks
src/components/SecretInput.tsx               masked input with a show/hide toggle
src/lib/variable-rows.ts                     environment editor rows (secret flag, state)
```

**Where this departs from the design above**

- **Encryption happens in the storage mapping, not in the domain services.**
  `SqliteSavedRequestRepository` and `SqliteEnvironmentRepository` hold an
  `Arc<dyn SecretCipher>`, and `json::encode_auth` takes a `SecretWrite`
  mode. That mapping is the one place every write passes through, so a new
  caller cannot skip it. Doing it in the services would have meant a second
  `Auth` type carrying ciphertext through the domain. Repositories still
  never see a key, only the port. History and examples use
  `SecretWrite::Strip`, and their services strip first as well, so the entry
  returned to the UI matches what was stored.
- **An unknown format version is a field-level "needs re-entering", not a
  typed error.** A typed error would fail the whole row, which contradicts
  "a missing secret never fails a load".
- **Three states, not four:** `Ok`, `NeedsReentry` (wrong key, tampered,
  truncated or unknown version) and `Unavailable` (no usable key this
  session). An unreadable value is as lost as one sealed under an old key, so
  the UI tells the user the same thing for both.
- **Saving over a secret is refused while the key is unavailable, even when
  the new value is blank.** Otherwise loading an unreadable secret (which
  arrives empty) and pressing Save would erase it.
- **The upgrade also seals a request whose secret is only a
  `{{placeholder}}`.** It is harmless and keeps the rule uniform: every
  non-empty secret in `requests` is sealed.
- **On non-Windows builds**, `KeychainDataKeyStore::new()` returns
  `SecretStore`. The app runs, secrets load as unavailable and cannot be
  saved, and nothing is written in plain text.
- `base64` 0.22 was already compiled for Tauri's build-time codegen. In the
  shipped binary it is a small pure-Rust addition. `aws-lc-rs` and
  `zeroize` were already linked through rustls (`cargo tree -i`).
- The crate's doc comment says `new_with_configuration` defaults the prefix
  to `keyring:`, but its code defaults to empty. Prefix and suffix are set
  explicitly so the target name does not depend on which is right.
- **Frontend:** the example snapshot is built with
  `substituteRequestInput(..., { leaveSecrets: true })`. A secret that wins a
  duplicate name keeps its placeholder even when a later plain row has the
  same name. The environment editor reuses `KeyValueTable`, which is now
  generic over its row type and has an optional secret column. There is no
  second table. `ApiError` gained `secretStore`, and the response viewer's
  exhaustive switch caught it.

**Verified in the cloud sandbox (2026-09-16).** crates.io was reachable this
time, so these ran against the real crates, not a stub harness:

- `cargo test`: **225 unit** (was 182), **12 curl integration**, **42
  repository** (was 29). All pass.
- `cargo clippy --all-targets -- -D warnings` is clean on Rust **1.95**.
  The project uses 1.97, which the sandbox could not download.
- `cargo fmt` applied. `Cargo.lock` changed only by adding `keyring-core`
  and `windows-native-keyring-store`; nothing was upgraded.
- The Windows-only adapter (`keychain.rs`) was type-checked and
  clippy-checked against `keyring-core` 1.0.0 and
  `windows-native-keyring-store` 1.1.0 in a scratch crate with its
  `cfg(windows)` removed. The Windows target itself could not be installed.
- **The goal is tested, not only the mechanism.**
  `after_the_upgrade_the_token_is_nowhere_in_the_database_file` writes a
  known token into a file-backed database, deletes a row holding it, runs the
  upgrade, then searches every file in the directory. With `secure_delete`
  and `VACUUM` disabled it **fails** ("token found in …responderhttp.sqlite3"),
  so it catches exactly what it is meant to catch.
- Frontend: `tsc --noEmit`, eslint, `vite build`, and **160 Vitest tests in
  20 files**, including new ones for `variable-rows`, secret placeholders in
  `variables`, tab secret state and the new error kind.

**`verify.bat` green on Windows, first run (2026-09-16).**

- Build, eslint and **160 Vitest tests** pass.
- `cargo fmt --check` passes.
- `cargo clippy --all-targets -D warnings` is clean on the project's
  toolchain. This closes the 1.95-versus-1.97 gap noted above.
- `cargo test` passes: **225 unit, 12 curl integration, 42 repository**.
  The Windows build compiled `keyring-core` 1.0.0 and
  `windows-native-keyring-store` 1.1.0 with no changes, which confirms the
  scratch-crate type check.
- **Not covered:** no test touches the real Credential Manager. The
  Windows-only `KeychainDataKeyStore` has only been compiled; the manual
  checks below are where it first runs.

**`release.bat` green (2026-09-16), version 0.1.1.**

- The release build passes, and both bundles were produced:
  `Responder_0.1.1_x64-setup.exe` and `Responder_0.1.1_x64_en-US.msi`.
  makensis accepted `windows/hooks.nsh` without a warning, which shows the
  hook compiles. Whether it runs correctly is still an uninstall check below.
- **Static-link check passes with 27 imports, the same count as before
  Phase 9.** No new DLL appeared: Credential Manager is reached through
  `advapi32`, which was already imported.

**Confirmed by hand (2026-09-16), first `pnpm tauri dev` run:**

- **Credential Manager** now holds exactly one ResponderHTTP entry under
  Generic Credentials:
  - Target `data-key@io.github.zoran-php.responderhttp`
  - User name `data-key`
  - Persistence **Enterprise**

  Name, divider and persistence are exactly as decided.
- **Log sequence:**
  - keyring-core looked the entry up, found nothing, and wrote it.
  - `secrets: no data key found, created one`
  - `UpgradeReport { sealed: 0, stripped: 0, waiting_for_key: 0, unreadable: 0, vacuumed: false }`
- The `NoEntry` path that creates the key has now run against the real
  store.
- **keyring-core logs at DEBUG** ("create entry wrapping Cred { target_name
  … }"). Those lines carry the target name, user and persistence, never the
  secret bytes, so they do not break §11 rule 6. They do go to the rolling
  file log, because the app logs at Debug.
- **Not yet shown:**
  - **The upgrade converting real data.** It found nothing to convert,
    so this database held no plain-text auth secrets. The upgrade path is
    covered by tests, not yet by a real old database.
  - ~~**The existing-key path.**~~ Confirmed on the second launch: a
    read, no write, then `secrets: data key loaded`.

**Secret variables confirmed at rest (2026-09-16).** DBeaver showed
`environment_variables` holding two rows marked secret (`name`,
`lastname`):

- `value` is empty and `secret` is 1.
- `value_sealed` holds a BLOB.
- Both BLOBs start with the same bytes, then diverge. That matches the
  format: the version byte and `key_id` come first and are the same for
  every value sealed under one key; the random nonce and the ciphertext
  follow.
- The plain text appears nowhere in the row.

The editor shows both variables masked, each with a show/hide button and
a closed lock. The empty trailing row shows an open lock. No "could not be
decrypted" notice appeared, so both values were opened with the stored key.
Still to see for this check: `{{name}}` resolving with that environment
active. The screenshot had "No environment" selected.

**Secret variables resolve on send (2026-09-16).** With environment `ll`
active, a GET to postman-echo used `{{name}}` in the URL, and
`{{name}}`/`{{lastname}}` in headers and in the raw body. The echo shows
`zoran`/`davidovic` in `args`, `x-name` and `x-lastname`.

- postman-echo does not echo a GET body. Its `content-length: 51` matches
  the resolved body with CRLF line endings, not the template, so the body
  was very likely substituted too.
- The same response shows the cookie jar attaching `__cf_bm`, `_cfuvid`
  and `sails.sid` from earlier responses. That is the first real-server
  evidence for the Phase 5 cookies feature, though not the 302 path.

**Two leaks found from that screenshot, fixed the same day.** Secret
variable values still reached disk in two places the design had missed.
Both came from using the fully resolved request where only the snapshot
(secret variables left as placeholders) was safe:

1. **`history.resolved_url`.** `request-store` recorded `resolved.url`.
   `?name={{name}}` was stored as `?name=zoran`, in plain text, next to a
   template that was correctly placeholder-only.
2. **The log file.** `services/http-client.ts` logged
   `${method} ${input.url}` with the resolved URL. `logging.rs` redacts
   only credential-shaped parameters (`key`, `token`, …), so `name=zoran`
   went through.

**Fix:**

- `send` and `sendAndDownload` both build the `leaveSecrets` snapshot and
  record `resolvedUrl: snapshot.url`.
- `sendRequest` and `sendAndDownload` take a **required** `logUrl`, and
  the store passes `snapshot.url`. It is required so that a new caller
  cannot silently log a resolved URL.
- Two tests in `services/http-client.test.ts` assert that the log line
  carries `{{secret}}` and never the value, on success and on failure.
- **`verify.bat` green after the fix (2026-09-16):** build, eslint,
  **162 Vitest** (`http-client.test.ts` now has 8), `cargo fmt --check`,
  clippy, and **225 + 12 + 42** Rust tests.

**Not covered by a test:** the store passing `snapshot.url` rather than
`resolved.url`. `request-store.test.ts` does not mock the HTTP service.

**Existing data is not cleaned.** History rows and log lines written before
this fix may hold secret values. Clear History to remove the rows:
`secure_delete` zeroes them. Old log files are in the app's log directory
and are not rotated away by this change.

**Lost key and uninstall confirmed (2026-09-16).**

- **Lost key.** The Credential Manager entry was deleted and the app
  restarted. `root` still loaded, with its URL, method and Bearer scheme
  intact, an empty Token field, and the "could not be decrypted on this
  computer" alert on the Auth tab. In the `lockoncam` environment, `name`
  and `lastname` loaded empty, still marked secret, each with
  "Could not be decrypted on this computer. Enter it again." Nothing failed
  to load, and no plain text was used in place of a secret: the
  `NeedsReentry` path behaves as designed.
- **Uninstall.** Uninstalling through NSIS with "Delete the application
  data" ticked removed `data-key@io.github.zoran-php.responderhttp` from
  Credential Manager. The hook works.

**Phase 9 is done.** Three smaller checks from the list below were not
reported and are worth doing when convenient; none blocks the phase:

- `findstr` over `responderhttp.sqlite3` for a saved secret. The file-level
  test covers this in CI.
- Uninstalling with the box **unticked** keeps the entry.
- An update through the installer keeps the entry.

**The original checklist, for reference**

- `release.bat`: `check-windows.ps1` must still pass with no new DLL.
  Credential Manager is in `advapi32`, which is already allowed.
- **Manual checks:**
  - ~~First launch creates exactly one entry.~~ Done 2026-09-16.
  - ~~Second launch loads the existing key.~~ Done 2026-09-16 (0.1.1). The
    log shows keyring-core reading the entry, with no write, followed by
    `secrets: data key loaded`.
  - Open a database from before Phase 9. The log shows the upgrade counts,
    and saved Bearer, Basic, API Key and Custom secrets still send correctly.
  - Save a request with a Bearer token, then search `responderhttp.sqlite3` for
    the token (for example with `findstr`). Nothing is found.
  - Mark an environment variable secret, save, restart. It is masked, shows
    with the eye button, and still resolves `{{name}}`.
  - Delete the Credential Manager entry and restart. The request loads with
    an empty token and the "could not be decrypted" notice. Typing a new
    token clears the notice.
  - Save a response as an example from a request that uses a secret
    variable in the URL. The example shows `{{name}}`, not the value.
  - A history entry for a request with a literal token shows the token
    blank. One using `{{token}}` re-runs.
  - Uninstall with "Delete the application data" ticked, and the entry is
    gone. Unticked, it stays. An update through the installer never removes
    it.

## Phase 8e — YAML export — **done, verified 2026-09-16**

The export dialog gets a picker for the output format: **JSON** or **YAML**,
with **JSON selected by default**. The same document goes into both formats;
only the text written to disk differs.

### The dependency decision (the real work in this slice)

The Phase 8 note assumed a fork of `serde_yaml`. Checked again on
2026-09-16, on crates.io and in RustSec:

| Crate | State | Verdict |
|---|---|---|
| `serde_yaml` | `0.9.34+deprecated`, archived | no |
| `serde_yml` | **RUSTSEC-2025-0068**: unsound (a segfault reachable through the serializer) and archived | no |
| `serde_yaml_ng` | last release 0.10.0, May 2024; the advisory notes it builds on the unmaintained `unsafe-libyaml` | no |
| `serde_norway` | last release 0.9.42, Dec 2024; C libyaml translated to unsafe Rust (`unsafe-libyaml-norway`) | fallback |
| **`serde-saphyr`** | 1.3.0, released 2026-09-16; about 5M recent downloads; **forbids `unsafe`**; pure Rust; MSRV 1.89; MIT OR Apache-2.0 | **proposed** |

**Proposed: `serde-saphyr` with only its serializer.**

```toml
serde-saphyr = { version = "1", default-features = false, features = ["serialize"] }
```

- **Serializer only.** It pulls in `base64` (already linked), `num-traits`,
  `zmij` and `nohash-hasher`. It leaves out the parser (`granit-parser`)
  and the error-report crates. The parser arrives when 8d import needs it.
- **No C code** and no new system library, so the static-link check should
  show the same 27 imports. `release.bat` must confirm that.
- **Risk: the crate is young.** Its first release was Sep 2025, and 1.x
  arrived in Aug 2026. `Cargo.lock` pins the version; take upgrades
  deliberately, not through a blanket `cargo update`. The fallback,
  `serde_norway`, has the same API shape as `serde_yaml`, so switching means
  changing one function.
- Needs approval, like every new crate (§11, §10). Asking for YAML is not
  the same as approving a specific crate.

### YAML traps the output has to avoid

YAML treats many unquoted words and numbers as something other than
strings. In an OpenAPI document that corrupts values without any visible
error:

- `responses` keys `"200"`, `"404"` become integers. The spec requires
  strings.
- `openapi: 3.0.0` stays a string, but a value like `"3.0"` would become a
  float.
- An example value `"007"`, `"1e3"` or `"0x1F"` becomes a number.
- `"yes"`, `"no"`, `"on"`, `"off"`, `"y"`, `"n"`, `"null"`, `"~"` or `""`
  become booleans or null. The YAML 1.1 words still matter, because many
  OpenAPI tools use YAML 1.1 parsers.
- A multi-line example body must round-trip exactly, including a trailing
  newline and leading spaces.
- A string that starts with `{`, `[`, `*`, `&`, `!`, `%`, `@` or `#`
  (a JSON `serializedValue` starts with `{`) needs quoting.

The serializer's documentation says it quotes the YAML 1.1 booleans and
emits long or multi-line strings as block scalars. **That is not trusted
without a test.** The round-trip test below covers every case in this list.

### Shape

**Rust**

- `openapi/format.rs` (new, pure):
  - `ExportFormat { Json, Yaml }`
  - `from_wire("json" | "yaml")`, returning `InvalidRequest` for anything
    else, the same rule as `OpenApiVersion`
  - `extension()`: `json` or `yaml`
  - `render(&Document, ExportFormat) -> Result<String, AppError>`
  - This is the only file that knows the formats exist. JSON stays
    `serde_json::to_string_pretty`, unchanged.
- **File extension:** `.yaml`, not `.yml`. That is the spelling the
  OpenAPI spec itself recommends for YAML documents.
- `from_collection::document_file_name(name, version, format)` produces,
  for example, `work_api.openapi-3.2.yaml`. The four existing file-name
  tests gain a format argument, plus one YAML case.
- `domain/services/openapi.rs`:
  - `export(…, format)`
  - `ExportedDocument.json` is renamed `text`, because it no longer is
    always JSON.
  - Still no environment repository.
- `commands/openapi.rs`:
  - takes `format: String`
  - the Save As filter follows the format (`"OpenAPI document (YAML)"`
    with `["yaml", "yml"]`, or the JSON one), so the dialog does not offer
    the wrong extension
  - passes `exported.text` to `save_text`
- **Nothing changes in** `document.rs`'s structs, `infer.rs`,
  `from_collection.rs`'s mapping, or the export notes. Serde attributes
  apply to both serializers, so `camelCase`, the renames and "omit, don't
  null" carry over. The tests check this rather than assume it.
- **Key order:**
  - struct fields keep declaration order, so `openapi` stays first, which
    readers expect
  - maps are `BTreeMap`, so they come out sorted, as the JSON does today

**Frontend**

- `types/openapi.ts`:
  - `ExportFormat = "json" | "yaml"`
  - `EXPORT_FORMATS` (JSON first)
  - `EXPORT_FORMAT_LABELS`
  - This mirrors how versions are already declared.
- `services/openapi.ts`: `exportCollectionOpenApi(collectionId, version,
  format, includeExamples)`, sending `format` in the invoke payload.
- `ExportOpenApiDialog.tsx`:
  - `useState<ExportFormat>("json")`
  - a **Format** control next to the version picker, disabled while busy
  - The choice resets to JSON on every open, like the other two options.
    Remembering it is not asked for.

### Tests

**Rust, `openapi/format.rs`:**

- `from_wire` accepts exactly `json` and `yaml`.
- Extensions are `json` and `yaml`.
- JSON output is byte-for-byte what `export` wrote before this change.
  That guards against a refactor changing the default without anyone
  noticing.
- **Round trip:** render one fixture `Document` as YAML, parse it back
  into `serde_json::Value`, and assert it **equals** the JSON rendering
  parsed the same way.
  - The fixture carries every trap in the list above: status-code keys, a
    version-like string, `"007"`, the YAML 1.1 boolean words, `"null"`,
    `"~"`, `""`, a multi-line body with a trailing newline, a
    `serializedValue` that starts with `{`, non-ASCII text, an empty map
    and an empty array, and nested `oneOf`/`nullable`.
  - Parsing needs the deserializer, so it is a **dev-dependency**:
    `serde-saphyr` with `features = ["deserialize"]` under
    `[dev-dependencies]`. With Cargo's feature resolver 2, dev-dependency
    features do not leak into the release build. `cargo tree -e normal`
    should confirm `granit-parser` is absent from the shipped graph.
  - Parsing back with the same library it was written with has a blind
    spot: an error both halves share would still pass. The manual check
    against an external validator closes that.
- **`openapi` is the first key.** The YAML text starts with `openapi:`.

**Frontend:** `tsc` covers the new parameter. The format list and its
labels live together in one module, so there is nothing new to unit-test
there.

**Manual:**

- Export the same collection as JSON and as YAML, at 3.2 and at 3.0.
- Paste both YAML files into Swagger Editor, or run
  `npx @redocly/cli lint`, and confirm they parse and validate. **This is
  the only independent check of the YAML itself.**
- The Save As dialog suggests `.yaml` for YAML and `.json` for JSON.
- Dismissing the Save As dialog still shows the export notes.
- JSON is selected every time the dialog opens.
- `release.bat`: the static-link check still shows 27 imports.

### Done when

- The dialog offers JSON (the default) and YAML.
- A YAML export parses back to exactly the document the JSON export
  describes, and passes an external OpenAPI validator.
- The JSON output is unchanged.
- No new DLL appears, and the parser is not in the release build.

### Decided 2026-09-16

1. **Crate:** `serde-saphyr`, serializer only. The user approved this new
   dependency.
2. **Picker:** a dropdown, matching the version picker above it.

---

### As built (2026-09-16)

**Files**

```
src-tauri/src/openapi/format.rs          ExportFormat, render(), yaml_options(); 8 tests
src-tauri/src/openapi/from_collection.rs document_file_name(name, version, format)
src-tauri/src/domain/services/openapi.rs export(…, format, …); ExportedDocument.text
src-tauri/src/commands/openapi.rs        `format` parameter; Save As filter per format
src-tauri/tests/repositories.rs          2 export tests through the real service
src/types/openapi.ts                     ExportFormat, EXPORT_FORMATS, DEFAULT_EXPORT_FORMAT, labels
src/services/openapi.ts                  `format` argument
src/services/openapi.test.ts             3 tests (new file)
src/features/collections/ExportOpenApiDialog.tsx   Format dropdown, JSON by default
```

**Where it departs from the plan**

- **No block scalars.** `SerializerOptions` is built with
  `prefer_block_scalars: false`, so a multi-line string, such as a saved
  JSON body, is written as one double-quoted string with `\n` escapes, never
  as `|` or `>`. Block scalars are where YAML's chomping, indentation and
  line folding can change a string when it is read back, and example bodies
  are exactly the strings with leading spaces and trailing newlines. This is
  the "safe way" chosen: less pretty, never ambiguous. A test fails if a
  block scalar ever appears.
- **`yaml_12: false`**, so the YAML 1.1 words (`yes`, `no`, `on`, `off`,
  `y`, `n`) are quoted too.
- **Every option is set explicitly** through the crate's `ser_options!`
  macro, which is required because the struct is `#[non_exhaustive]`, so a
  change to the library's defaults cannot change the output.
- **1.3.0 is the locked version.** It shipped the day this slice was
  built, and its changelog includes "Preserve string scalars across YAML
  1.1 readers", which is the property this slice depends on.
- **The Save As dialog also offers `.yml`** for YAML, but suggests `.yaml`.
- **One more service test than planned:** `services/openapi.test.ts` checks
  that `format` reaches the command, that a refused format comes back as a
  typed error, and that JSON is the default and is listed first.

**Verified in the cloud sandbox:**

- `cargo test`: **234 unit** (+9), **12 curl**, **44 repository** (+2).
  `cargo clippy --all-targets -D warnings` and `cargo fmt --check` are
  clean.
- **Round trip.** A fixture with 68 trap strings is placed in every
  position a string can take: map keys, values, tags, parameter names,
  examples, and serialized and decoded bodies. It includes:
  - status codes, version strings, `007`, `0x1F`, `1e3` and `1_000`
  - YAML 1.1 and 1.2 booleans, `null`, `~` and `""`
  - `.inf` and `.nan`
  - YAML indicator characters, `---` and `...`
  - quotes and backslashes
  - tabs, CR/LF, leading and trailing blanks and newlines
  - U+FEFF, U+0000, U+0085, U+2028 and U+FFFE
  - emoji
  - `u64::MAX`, `i64::MIN` and `1e300`

  Read back as YAML, it equals the JSON rendering exactly.
- **Independent parsers, not only the one that wrote it.** The same trap
  YAML was parsed by:
  - **PyYAML** (YAML 1.1)
  - **ruamel.yaml** (YAML 1.2)
  - **js-yaml 4.3.2** (used by Swagger UI and Editor)
  - **yaml 2.9.1**, which Redocly uses, in both 1.2 and 1.1 mode

  All five matched the JSON exactly, so the blind spot noted in the plan
  (a mistake shared by writer and reader) is closed.
- **Through the real service.** A realistic collection was exported at
  3.0, 3.1 and 3.2 in both formats. Each YAML file equals its JSON twin,
  the notes match, and the file names differ only in the extension. It
  includes:
  - a folder
  - `{{base_url}}` and `{{userId}}`
  - `limit=007`
  - Bearer `{{token}}`
  - a JSON body containing `"no"` and `"3.0"`
  - a saved response with a `"200"` key and `"on"`, `"null"` and `"~"`
    values
- **Redocly CLI 2.53.2** (`lint --extends=minimal`) reports **all six
  files valid**. The one warning, `tag-description`, is the same in JSON
  and YAML and was already there before this change.
- **swagger-parser** accepts 3.0 in both formats and does not support 3.2
  at all. It rejects 3.1 in **both** formats with the same error: a server
  variable's `description`. That comes from its bundled 2021-04-15 3.1
  schema, which misspells the property as `descriptions`, so it is not a
  fault in either format. The current official schema could not be fetched
  (`spec.openapis.org` is blocked by the sandbox's proxy).
- **Release build graph.** `cargo tree -e normal --target
  x86_64-pc-windows-msvc -p serde-saphyr` shows only `base64`,
  `nohash-hasher`, `num-traits`, `serde_core` and `zmij`. **No
  `granit-parser`:** the parser is test-only.
  - `Cargo.lock` only gained entries. The parser-side ones
    (`granit-parser`, `annotate-snippets`, `encoding_rs_io`, …) are in the
    lock file for the tests, not in the release build.
  - Nothing already there was upgraded.
- **Frontend:** `tsc --noEmit`, eslint and prettier are clean; **165
  Vitest** pass (+3).

**Verified on Windows (2026-09-16).**

- **`verify.bat` green:** build, eslint, 165 Vitest, fmt, clippy, and
  234 + 12 + 44 Rust tests.
- **`release.bat` green:** both 0.1.1 bundles were built, and the
  static-link check **passes with 27 imports**, unchanged.
- **The user's real export** of `lockoncam` at 3.2, in both formats:
  - Parsed by js-yaml, yaml (in 1.2 and 1.1 mode) and PyYAML, the YAML
    **equals the JSON exactly**.
  - Redocly reports both files valid.
  - The Windows body's `\r\n` line endings survive as escapes in a quoted
    `serializedValue`. That is exactly the case the no-block-scalars
    decision exists for: a `|` block would have lost the `\r`.
  - The `{{name}}`, `{{lastname}}` and `{{token}}` placeholders appear only
    as placeholders, and no credential is in either file.
- **Found while checking, not part of this slice:** the request's URL is
  `…/get?name={{name}}`, but the exported operation has no `name` query
  parameter. `openapi/url.rs` drops the query string by design
  (`a_query_string_is_not_part_of_the_path`), and only rows in the Params
  tab become parameters. A query written into the URL is therefore
  silently missing from the document. That is a candidate for an
  `ExportNote`, or for parsing the query into parameters.

**The earlier checklist, for reference**

- **Manual checks:**
  - The export dialog shows Format with JSON selected, every time it opens.
  - Export as YAML: Save As suggests `….openapi-3.2.yaml` and filters on
    YAML; the file opens in Swagger Editor.
  - Export as JSON: the file is unchanged from before.
  - Dismissing Save As still shows the export notes.

## Phase 8e extension — URL query as parameters — **done, verified 2026-09-17**

Found while checking the first real YAML export. `…/get?name={{name}}`
exported no `name` parameter, because `openapi/url.rs` dropped the query
string and only Params-tab rows became parameters. **Decided 2026-09-16:
parse the URL's query into parameters**, rather than only adding a note.

### Built

- **`openapi/url.rs`:** `MappedUrl.query_parameters: Vec<QueryPair>`.
  - The query runs from `?` to `#`; the fragment is ignored.
  - Pairs are split on `&` only. `;` is a legacy separator that servers
    disagree about, and splitting on it would break values containing one.
  - The first `=` separates name from value, and a bare `?flag` has an
    empty value.
  - Names and values are percent-decoded, with `+` read as a space, as
    browsers do for a query string. A malformed escape (`%zz`, a trailing
    `%`), or bytes that are not UTF-8, keep the component exactly as typed.
  - Pairs without a name (`?=x`, `?&`) are skipped.
  - `{{variables}}` stay as typed: in a query they are values, not
    templating, so they never become path parameters.
  - Repeated names are all reported; collapsing them is the caller's job.
  - 11 new tests.
- **`openapi/from_collection.rs`:**
  - **Order:** the URL's pairs come first, then the Params tab, which is
    the order libcurl sends them. A name in both is one parameter,
    described by whichever came first.
  - **Uniqueness (`Parameters`).** Parameters are now unique by name and
    location, which the spec requires. This also fixes an older gap: two
    Params rows, or two header rows, with the same name used to produce a
    duplicate and therefore an invalid document. Header names compare
    case-insensitively; everything else compares exactly. A path parameter
    and a query parameter may share a name.
  - **Credentials.**
    - A query parameter whose name marks it as a credential (`token`,
      `api_key`, `signature`, …) is described **without its value**, and
      the new `ExportNote::CredentialParameterValueOmitted` says so, once
      per parameter.
    - This now covers the Params tab too, which used to export such a
      value.
    - A value that is only a `{{placeholder}}` is kept, because it names a
      variable rather than holding a secret.
    - The Auth tab's API-key-in-query name is suppressed in the URL as
      well as in the Params tab.
  - 9 new tests.
- **One credential-name list.** `SENSITIVE_PARAMS` moved out of
  `logging.rs` to `domain::secrets::CREDENTIAL_PARAM_NAMES`, with
  `is_credential_param_name`. The log redactor and the exporter can no
  longer disagree about what counts as sensitive. The redactor's own tests
  are unchanged and pass.
- **`commands/openapi.rs`:** the sentence for the new note.

### Verified in the cloud sandbox

- `cargo test` passes: **254 unit** (+20), 12 curl, 44 repository.
  `cargo clippy --all-targets -D warnings` and `cargo fmt --check` are
  clean.
- **Mutation check.** With the URL pairs removed from the merge, 6 of the
  new export tests fail, so they test the feature rather than pass
  regardless.
- **Redocly** reports all six fresh exports valid, at 3.0, 3.1 and 3.2 in
  JSON and YAML. The fixture URL `…?active=yes` now produces an `active`
  query parameter with `example: "yes"`, which stays quoted in YAML.
- **No frontend change.**

### Still to do

- `verify.bat` on Windows.
- Re-export `lockoncam`. The `/get` operation should now list `name` in
  query with example `{{name}}`.

---

## Params tab synced with the URL bar — **done, verified 2026-09-17**

Asked for: typing a key/value in Params shows in the URL bar, and the other
way round.

### Decided (2026-09-17)

- **The URL is the only source of truth.** The Params table is a view of
  its query string, and a tab's `queryParams` is always sent and saved
  empty. Keeping two lists in step, and then appending one to the other at
  send time, would send every parameter twice.
- **The table shows the query as typed, not decoded.**
  - `a%20b` stays `a%20b`, so there is no decode/encode round trip to get
    wrong around `+`, a stray `%`, or an escape meant literally.
  - Editing the table escapes only what would change the query's structure:
    `&` and `#`, plus `=` in a name.
  - A row with an empty value is written bare (`?flag`), so `a=` becomes
    `a` once the table is edited. The URL is never rewritten from the
    table while the user is typing in the URL bar.
- **Spaces and similar characters are fixed at send time.**
  - Measured against libcurl 8.21: it **refuses** a URL containing a space
    ("Malformed input to a URL function"), and sends `ö` or `"` raw, which
    servers answer with 400. So a value typed with a space in Params would
    have broken the request.
  - The existing `encode_url` setting (default on) is now **"Encode URL
    automatically"**. It percent-encodes only what a URL cannot carry, in
    the path, query and fragment. Valid text and existing `%XX` escapes are
    left byte-for-byte, a stray `%` becomes `%25`, and the scheme and host
    are untouched.
  - With the setting off, the URL goes out as typed.
  - This replaces the old rule that "the typed URL is sent as written
    either way".
- **Requests saved before this change.** Their Params rows are folded into
  the URL when opened, after the URL's own query, **encoded exactly as the
  backend used to encode them**, so they send the same bytes. They open
  clean (not marked dirty), and the next Save stores the new shape.
  - `build_url` still appends `query_params`, for such rows and for an API
    key the Auth tab sends in the query.
  - **Known differences:**
    - An old row whose value was a `{{variable}}` is now substituted into
      the URL raw. A variable value containing `&` would
      therefore split into two parameters.
    - An old row holding a literal `%41` with encoding on is folded as
      `%2541`, so it stays literal. With encoding off it stays `%41`, as it
      was sent before.

### Built

- `src/lib/query-sync.ts`:
  - `splitUrl`, `queryPairs`, `withQueryPairs`
  - `foldLegacyParams`
  - `encodeUrlForSend`, which mirrors the Rust function so Copy as cURL
    shows the bytes that are actually sent
  - 22 tests, including a table → URL → table round trip
- `src/lib/key-values.ts`:
  - `rowsKeepingIds`, which keeps row ids stable while the URL is typed
  - `updateRow`/`removeRow` were already generic
- `src/store/request-store.ts`:
  - `setUrl` re-reads the table
  - `setParamRows` rewrites the URL's query, keeping the base and fragment
  - `buildTabFromRequest` folds legacy rows
  - `currentInput().queryParams` is always `[]`
  - 6 new tests
- `src/lib/curl-string-builder.ts`: encodes the URL when the setting is on.
  2 new tests.
- `src/features/request-builder/SettingsPanel.tsx`: the new label and hint.
- `src-tauri/src/http/mapping.rs`: `encode_url_for_send`, applied in
  `build_url` when `encode` is on. 6 new tests.
- `src-tauri/tests/curl_client.rs`: 2 new tests.
  - `?q=a b&city=Köln&n=1%2B1` reaches httpmock as `a b`, `Köln` and
    `1+1`.
  - With encoding off, the same URL is refused by libcurl as a transport
    error.
- `src-tauri/src/domain/models.rs`: the doc comment for `encode_url`.
- **No storage change and no migration.** `query_params_json` stays; new
  saves write `[]` there.

### Verified in the cloud sandbox

- `cargo test` passes: **260 unit**, **14 curl**, **44 repository**.
  clippy and fmt are clean.
- `tsc`, eslint and prettier are clean on the touched files, and **195
  Vitest** pass.

### Verified on Windows (2026-09-17)

- **`verify.bat` green:** build, eslint, **195 Vitest**, fmt, clippy, and
  **260 unit, 14 curl and 44 repository** Rust tests. That includes both
  new libcurl tests: a URL with a space and `Köln` reaches the server, and
  the same URL with encoding off is refused.
- **Manual test by the user:** query parameters work in both directions.
  The same `verify.bat` run also covers the URL-query export slice above.

### The original manual checklist, for reference

- **Manual checks:**
  - Type `?q=cats&page=2` in the URL bar: the Params tab shows two rows.
  - Edit a value in Params: the URL bar updates. Delete both rows: the `?`
    goes.
  - A Params value with a space sends successfully (postman-echo `args`
    shows the space).
  - Open a request saved before today that has Params rows. Its URL now
    carries them, the tab is not marked dirty, and it sends the same
    request.
  - Copy as cURL on a URL with a space pastes and runs in a terminal.
  - Untick "Encode URL automatically" and send a URL with a space: a clear
    transport error, not a crash.

---

## Phase 8d — OpenAPI import — **done, verified 2026-09-17** (dialog height follow-up below)

The user picks a `.json`, `.yaml` or `.yml` file. The app works out which of
the two it is, checks it against the official OpenAPI schema, and refuses it if
it fails. If it passes, the app shows a preview and then imports it as a new
collection. Nothing is written unless everything can be written.

Everything below was checked in the cloud sandbox on 2026-09-17 with a
throwaway prototype. The real-world files used were the GitHub REST
description (3.1, 13 MB JSON and 9.9 MB YAML, 1,239 operations) and the Stripe
spec (3.0, 9.3 MB YAML).

### Step 1 — work out the format (`openapi/import/detect.rs`)

The file extension is a hint for the file picker only. The content decides the
format.

1. **Size cap: 50 MB.** The file is refused before it is parsed. The GitHub
   spec, one of the largest public ones, is 13 MB.
2. **UTF-8 only.** A UTF-8 byte-order mark is removed. UTF-16, UTF-32 and
   invalid UTF-8 are refused with the message "not a UTF-8 text file".
3. **If the first non-whitespace character is `{` or `[`, the file is read as
   strict JSON** with `serde_json`. A small custom visitor refuses duplicate
   keys, because `serde_json` quietly keeps the last one and a spec with two
   `/users` keys is ambiguous. There is no fallback to YAML: a file that looks
   like JSON but is broken reports the JSON error at its line and column,
   which is the error the user needs.
4. **Anything else is read as YAML** with the serde-saphyr deserializer. The
   options are strict:
   - one document only, so `---` separating several documents is refused
   - duplicate keys are refused (the library default)
   - `strict_booleans: true`, so `on`, `yes` and `off` stay strings. This
     follows YAML 1.2, which is what OpenAPI tools use. Otherwise
     `enum: [on, off]` would silently turn into booleans.
   - `reject_unsupported_tags: true`
   - the `include` and `properties` features stay off, so a file cannot pull
     in other files or environment values
   - **the node budget is raised and the alias limits stay at their
     defaults.** The default budget of 250,000 nodes rejected both the GitHub
     and the Stripe YAML files (tested). The alias limits are what stop
     "billion laughs" alias bombs, and one was confirmed rejected. The new
     budget is tied to the 50 MB cap.
   - Numeric map keys such as `200:` come out as the string `"200"`
     (tested), so response codes work without special handling.
5. **The root must be a mapping.** This single rule is how "other markup
   languages are not supported" is enforced:
   - TOML (`[package]`) parses as a YAML sequence, so it is refused.
   - XML parses as one YAML string, so it is refused.
   - Anything else either fails to parse or is not a mapping.

   All three cases show the same message: "This file is not JSON or YAML."
6. **Version gate, checked before schema validation** so the message is
   specific:
   - `swagger: "2.0"`: "Swagger 2.0 is not supported. Convert it to OpenAPI 3
     first."
   - `openapi` is `3.0.x`, `3.1.x` or `3.2.x`: accepted.
   - `openapi` is a number (an unquoted `3.1` in YAML): "the `openapi` field
     must be a string. Quote it."
   - any other version: "OpenAPI X is not supported (3.0, 3.1 and 3.2 are)."
   - no `openapi` key at all: "This is valid JSON/YAML but not an OpenAPI
     document."

Output: `(serde_json::Value, SourceFormat, OpenApiVersion)`. Validation and
mapping both work on `serde_json::Value`, so after this step JSON and YAML take
the same path.

### Step 2 — schema validation (`openapi/import/validate.rs`)

**The official OAI schemas are vendored.** They live in
`src-tauri/schemas/openapi/`, are loaded with `include_str!`, and ship with
their Apache-2.0 LICENSE. Nothing is fetched at run time.

| Version | Root schema | Also registered |
|---|---|---|
| 3.0 | `oas/3.0/schema/2024-10-18` (draft-04) | none |
| 3.1 | `oas/3.1/schema-base/2025-09-15` | `schema/2025-09-15`, `dialect/2024-11-10`, `meta/2024-11-10` |
| 3.2 | `oas/3.2/schema-base/2025-09-17` | `schema/2025-09-17`, `dialect/2025-09-17`, `meta/2025-09-17` |

For 3.1 and 3.2 the `schema-base` variant is used, because it also validates
every Schema Object against the OpenAPI dialect. The plain `schema` variant
does not check them. If a document declares a different `jsonSchemaDialect`,
schema-base would reject it unfairly. In that case the app validates with the
plain `schema` and adds an import note saying Schema Objects were not checked.

**Validator: `boon` 0.6.1 (MIT OR Apache-2.0). This needs your approval.** The
prototype ran both candidate crates against the same files.

- **`boon`** validated all nine official schema files out of the box.
  - It accepted our own 3.2 export.
  - It refused a document with no `info.title`, a 3.0 parameter with
    `in: body`, a 3.1 Schema Object with `type: strnig`, and a 3.1 document
    containing the 3.2-only `serializedValue`.
  - Its errors are nested and carry JSON pointers.
  - Timings on the GitHub spec: 12 ms to compile the schema, 207 ms to
    validate. Stripe: 48 ms.
  - It adds 4 crates that are not already in `Cargo.lock`: `boon`,
    `appendlist`, `fluent-uri` and `borrow-or-share`. `regex`, `idna`,
    `ahash`, `url`, `base64` and `once_cell` are already linked.
  - Its default loader only reads `file://`. We replace it with a loader
    that refuses everything, so only the vendored schemas can ever be
    resolved.
- **`jsonschema` 0.56** failed on the 3.1 and 3.2 `schema-base` files: it
  reported "Pointer '/$defs/dialect' does not exist". The same thing
  happened with an in-memory retriever and with a prepared registry. Its
  default features also pull in reqwest to fetch schemas over HTTP. It also
  has about 74 transitive crates, against boon's handful.
- The only risk with boon is maintenance: its last release was January 2025.
  JSON Schema draft 2020-12 is frozen, and boon passes the official
  JSON-Schema-Test-Suite, so a quiet release history is acceptable. A test
  pins the behaviour we depend on.

**Refusal.** Any error means the file is not imported.

- The dialog shows up to 20 errors. Each has the instance path, such as
  `/paths/~1users/get/responses`, decoded for display as
  `paths › /users › get › responses`, plus boon's message for it.
- When there are more than 20, the dialog shows the total.
- The full list can be copied to the clipboard.
- Nothing is logged except the counts. The file content is never logged,
  because a spec can contain example tokens (rule 6).

**Checks the schema cannot do.** These run after validation passes and also
refuse the import.

- Every internal `$ref` must resolve.
- An external `$ref` (another file or a URL) is refused. The message names
  the ref and suggests bundling the spec into one file first, for example
  with `redocly bundle`. The app never fetches anything over the network and
  never reads other files.

### Step 3 — resolve references (`openapi/import/refs.rs`)

- Only internal refs (`#/...`) are followed. JSON-pointer escaping (`~0`,
  `~1`, percent-encoding) is handled.
- Refs are resolved lazily, only for objects the mapper reads: Path Item,
  Parameter, Request Body, Response, Example, Security Scheme and Media
  Type. Schema Objects are not expanded unless a request body has to be
  generated from one (see decision D3).
- Recursion is guarded by the set of refs already being visited plus a depth
  limit of 32. A recursive schema is cut off at the point where it repeats,
  and no note is added because that is normal.

### Step 4 — map the document to a collection (`openapi/import/to_collection.rs`, pure)

The mapper reads `serde_json::Value` into a lenient **read model**
(`openapi/import/model.rs`). Every field defaults, and unknown fields are
ignored. The export structs in `document.rs` are not reused, because they are
shaped for writing. The mapper produces an `ImportPlan` plus a
`Vec<ImportNote>`. The note list is the "what was ignored" report that 8d was
always meant to have.

| OpenAPI | ResponderHTTP |
|---|---|
| `info.title` | collection name (a new collection every time, never merged; a duplicate name is allowed, as it already is) |
| operation | saved request |
| name | `summary`, then `operationId`, then `METHOD /path` |
| `get` … `options` | the 7 methods we support |
| `trace`, 3.2 `query`, `additionalOperations` | skipped, one note each |
| `servers[0]` | `{{baseUrl}}` plus an environment variable (D2) |
| other `servers` entries | listed in a note |
| path-level or operation-level `servers` | that request uses the literal URL instead of `{{baseUrl}}`, with a note |
| `/users/{id}` | `/users/{{id}}` |
| query parameter | written into the URL, which is the single source of truth since the Params sync; see the rule below |
| header parameter | header row (the same rule) |
| cookie parameter | note: the cookie jar handles cookies |
| 3.2 `querystring` location | note |
| `requestBody` | the first media type the app can represent, in this order: JSON / `+json`, form-urlencoded, multipart (text parts only), XML, `text/*`. Anything else goes to a note. A `Content-Type` header is added. |
| body content | `example`, then the first entry of `examples`, then (per D3) a sample generated from the schema |
| security (operation-level, falling back to top-level) | the first requirement is used: `http basic` becomes Basic with `{{username}}`/`{{password}}`; `http bearer` becomes Bearer with `{{token}}`; `apiKey` in a header or query becomes ApiKey with the key's name and the value `{{apiKey}}`; `apiKey` in a cookie, `oauth2`, `openIdConnect` and `mutualTLS` become None with a note; an empty requirement (`{}`) means no auth |
| response `examples` / `example` | saved examples, if the dialog option is ticked: status from the response code (`default` and `2XX` become the first code in that range, with a note), body from the example, `Content-Type` from the media type |
| `callbacks`, `links`, `webhooks`, `discriminator`, extra auth requirements | one summary note each, with a count, not one note per occurrence |
| `deprecated: true` | the request name gets a " (deprecated)" suffix |

**Query and header parameter rule.**

- Required parameters are always included. The value is `example`, then
  `schema.default`, then the first `enum` value, then `{{name}}`.
- Optional parameters are included only when they have an example or a
  default. The rest are left out, because a table row cannot be disabled
  and an empty `?limit=` changes what the server does. They are counted in
  one note per request, naming them.
- Credential-named parameters, using the `CREDENTIAL_PARAM_NAMES` list the
  exporter uses, always get `{{name}}` and never the example value.

**Import never creates a literal secret.** Every credential slot is a
`{{placeholder}}`. The environment variables behind those placeholders are
created **empty and flagged secret** (Phase 9). The user fills them in; the
spec never does.

### Step 5 — folders and grouping (decision D1)

**Option A — by tag**.

- One folder per tag, in the order of the document's top-level `tags` list,
  then the order in which tags first appear.
- An operation with no tag goes to the collection root.
- An operation with several tags goes into the first tag's folder. The
  alternative, a copy in every tag's folder, makes duplicates that drift
  apart when one of them is edited, so it is not the default.
- In **3.2**, Tag `parent` is followed (it is confirmed in the 3.2 schema,
  together with `kind`), so tags nest into nested folders, which the app
  already supports. A cycle in `parent`, or an unknown parent, is flattened
  to the root with a note.
- Good for: specs whose authors tagged them. The GitHub spec has 49 tags,
  and every one of its 1,239 operations has exactly one.
- Bad for: untagged specs, which import as one flat list.

**Option B — by path.**

- Folders follow the URL segments after the server: `/repos/{owner}/{repo}/issues`
  becomes `repos › {owner} › {repo} › issues`.
- Levels with a single child are collapsed, so the result is `repos › issues`
  rather than a deep chain of one-item folders.
- Parameter segments are dropped from folder names.
- Good for: untagged specs and REST-shaped APIs.
- Bad for: very flat APIs. The GitHub spec yields 37 top-level folders
  here, against 49 tags.

**Option C — flat.** Every request goes to the collection root. This is the
simplest option and fine for small specs. It is unusable for 1,000
operations.

**Option D — a picker in the import dialog** offering A, B and C, with a
preview of the resulting top-level folders. It defaults to A when the spec has
tags and to B when it has none.

The tree is sorted in document order. The `folders` and `requests` tables have
no position column, so the order comes from `created_at`. The importer
therefore writes rows in document order inside the transaction. **No
migration is needed for any of the options.**

### Step 6 — write it all at once

**New port: `ImportRepository::import(&ImportPlan) -> Result<ImportedIds>`.**
It uses one SQLite transaction for everything: the collection, the folders,
the requests, the examples and the environment. Any failure rolls all of it
back.

- The row-insert code in `collections.rs`, `folders.rs`,
  `saved_requests.rs`, `examples.rs` and `environments.rs` is moved into
  functions that take a `&Transaction`. The existing repositories call those
  same functions, so there is one insert path, not two.
- Auth still goes through `SecretWrite::Seal`. The values are placeholders,
  so nothing is actually sealed, but the code path stays the only one.
- A test forces a failure halfway through and asserts that the database is
  unchanged.

### Step 7 — commands and UI

**Rust side: two commands, and the path never goes through the webview.**

1. `pick_openapi_import()` opens the native file picker (filter:
   `*.json;*.yaml;*.yml`, plus "All files").
   - It reads, detects, validates and maps the file with default options.
   - It keeps the parsed document in `PendingImport` state under a random
     token.
   - It returns a `ImportPreviewDto` with the title, version, format,
     operation count, top-level folders for each grouping, environment
     variables, and notes. If the file was refused, it returns an
     `ImportRefusedDto` with the reason and the error list instead.
2. `import_openapi(token, options)` takes the options (grouping, examples
   on/off, environment on/off). It maps the pending document again with
   those options, writes it, clears the token, and returns the new
   collection id.

**Frontend side.**

- `types/openapi-import.ts`
- `services/openapi-import.ts`
- `features/collections/ImportOpenApiDialog.tsx`, built on the existing
  `Modal`. It shows either the refusal (errors plus Copy) or the preview
  (options plus the report).
- An "Import OpenAPI…" entry next to the existing "New collection" control
  in the sidebar.
- After a successful import, the collections store reloads and the new
  collection is selected. If an environment was created, the environments
  store reloads too; the new environment is not activated automatically.

**Where the code goes.**

```
src-tauri/schemas/openapi/            vendored OAI schemas + LICENSE
src-tauri/src/openapi/import/
    mod.rs  detect.rs  validate.rs  refs.rs  model.rs
    to_collection.rs  grouping.rs  notes.rs
src-tauri/src/domain/services/openapi_import.rs
src-tauri/src/domain/ports.rs          + ImportRepository
src-tauri/src/persistence/repositories/import.rs  (+ shared insert fns)
src-tauri/src/commands/openapi_import.rs
src/types/openapi-import.ts
src/services/openapi-import.ts (+ test)
src/features/collections/ImportOpenApiDialog.tsx
```

### Tests

- **`detect`** (table-driven):
  - JSON, YAML, JSON with a BOM, UTF-16 (refused), empty, root array,
    TOML, XML, multi-document YAML, duplicate keys in JSON and in YAML
  - `on`/`yes` staying strings, alias bomb refused, numeric response keys
  - `swagger: 2.0`, unquoted `openapi: 3.1`, `openapi: 4.0`, file over the
    size cap
- **`validate`**:
  - our own 3.0, 3.1 and 3.2 exports pass, which is the conformance check
    the Phase 8 testing notes asked for
  - one fixture per refusal above
  - a foreign `jsonSchemaDialect` falls back to the plain schema, with a note
  - the loader refuses `file://` and `https://`
- **`refs`**: pointer escaping, a cycle, a depth limit hit, a missing
  target, an external ref refused.
- **`to_collection` and `grouping`**: a unit test for each mapping-table
  row and each grouping option, including 3.2 parent nesting and a parent
  cycle.
- **Repository**: a full import round-trips through the database; the
  rollback-on-failure test; imported secret variables are empty and
  flagged secret.
- **Round trip**: collection → export (3.0, 3.1, 3.2 × JSON, YAML) →
  import → export. The two exports must be equal, apart from the notes the
  first export already reported.
- **Large specs**: `#[ignore]` smoke tests driven by
  `RESPONDERHTTP_BIG_SPEC=<path>`, for the GitHub and Stripe files. They check
  that import finishes and stays under a time budget. They are not
  committed, because the files are too large.
- **Frontend**: a service test and a dialog test (refusal view, preview
  view, grouping switch).

### Slices

1. **8d-1: detection, validation and the refusal UI.** The preview shows only
   counts. This is shippable on its own as "check this spec".
2. **8d-2: mapping, grouping and the transactional import.** Requests and
   folders only.
3. **8d-3: environment, auth placeholders and saved examples.**
4. **8d-4: the round-trip tests, large-spec smoke tests and polish.**

### Dependencies (need approval before 8d-1)

- `boon = "0.6"`, a new crate, for the reasons given in Step 2.
- `serde-saphyr` gets the `deserialize` feature in the **release** build.
  Until now only the serializer ships. This is the same crate at the same
  locked version (1.3.0), and `include`/`properties` stay off.

### Decided 2026-09-17

- **D1: option D.** The import dialog has a grouping picker: by tag, by path,
  or flat. It shows a preview of the folders, and defaults to by tag when the
  spec has tags and by path when it has none.
- **D2: `{{baseUrl}}` plus a new environment.** The environment holds
  `baseUrl` and the auth variables, which start empty and flagged secret. A
  checkbox in the dialog skips creating the environment.
- **D3: generate a sample body from the schema** when the spec gives no
  example. It uses `default`, `enum` and `example` values, falling back to
  placeholders by type. Depth is capped and recursive schemas are cut off.
- **D4: both dependency changes approved.** `boon` 0.6 is added, and the
  serde-saphyr deserializer ships in the release build.
- The other defaults below stand.

### Decisions as asked (for reference)

- **D1 — grouping:** A, B, C or D (Step 5).
- **D2 — base URL:** `{{baseUrl}}` plus a new environment (`<title>`
  holding `baseUrl` and the empty secret auth variables), **or** literal
  URLs with no environment.
- **D3 — request bodies with no example:** generate a sample from the
  schema, **or** leave the body empty and keep only the Content-Type.
- **D4 — dependencies:** boon, and the serde-saphyr deserializer in the
  release build.
- Default: saved examples are imported, ticked by default in the dialog. A
  spec's examples are documentation, not captured traffic, so the reason the
  export defaults to off does not apply.
- Default: an operation with several tags goes only in the first tag's
  folder.
- Default: external `$ref` is refused, not resolved from neighbouring files.
- Default: 50 MB size cap, and at most 20 errors shown.
- Default: optional parameters with no example are left out and listed in
  a note.

### As built (2026-09-17) — cloud-verified, waiting on `verify.bat` and a manual test

All four slices were built in one pass.

**Verified in the cloud sandbox:**

- `cargo fmt --check` and `cargo clippy --all-targets -D warnings` are clean.
- `cargo test`: 369 unit, 14 curl and 48 repository tests pass; the large-spec test is ignored by default.
- `tsc --noEmit` and eslint are clean.
- vitest: 217 tests pass. This includes the first component test in the repo, `ImportOpenApiDialog.test.tsx`.

**Large-spec smoke test** (debug build, `RESPONDERHTTP_BIG_SPEC=…`, times are check + map + write):

| Spec | Result | Time |
|---|---|---|
| GitHub JSON (13 MB) | 1,239 requests, 49 folders, 1,061 examples | 3.0 s |
| GitHub YAML (9.9 MB) | same | 10.4 s |
| Stripe YAML (9.3 MB) | 541 requests | 6.1 s |
| Swagger Petstore 3.0 | 19 requests | 0.14 s |

Release builds will be several times faster. The reports stayed short: the GitHub spec produced 4 notes.

**Round trip.** `export_then_import_then_export_changes_nothing` covers 3.0, 3.1 and 3.2, each as JSON and YAML. In all six cases the second export is equal to the first and has the same notes. Every export also passes `check`, so the Phase 8 wish of validating our own output against the official schema is now a test.

#### Where it departs from the plan above

- **Tree order.** The Step 5 note on tree order was wrong. `folders` and `requests` are listed by name (`ORDER BY name COLLATE NOCASE`), not by `created_at`, so the order rows are written in makes no difference. Nothing depends on it.
- **Top-level lists.** A file whose first character is `[` is refused as "not JSON or YAML" without being parsed. A top-level list can never be OpenAPI, and without this a TOML `[table]` line would have produced a JSON syntax error.
- **Path parameters** become plain environment variables. Their value is the parameter's example or default, and is empty otherwise.
- **Required parameters without a value** get `{{name}}` and a plain, empty environment variable.
- **Credential-named parameters** are always included, whether required or not, as `{{name}}` with a secret, empty variable. Headers named `Cookie` and `Proxy-Authorization` count as credentials too.
- **API-key variables** are named after the security scheme, for example `{{apiKey}}` or `{{api_key}}`. Bearer uses `{{token}}` and Basic uses `{{username}}`/`{{password}}`; only the password is secret.
- **Servers.** A server whose URL is a single variable, which is what our export writes for `{{base_url}}`, keeps that variable's name, and a `<name>` placeholder default becomes an empty value. That is what makes the round trip exact. Any other server becomes `{{baseUrl}}`, with its variables filled in from their defaults.
- **Response status codes.** A `default` response's example is saved with status 500, and a range such as `2XX` becomes 200. Both are reported.
- **Ignored header parameters.** Header parameters named `Accept`, `Content-Type` and `Authorization` are ignored, as the spec requires. `Content-Type` is not needed anyway, because a raw body already carries it.
  - **Follow-up for the export:** it still writes an `Accept` header row as a header parameter. Those rows are lost on a round trip. Decide whether the export should drop them with a note.
- **Format checks.** 3.0 is validated with draft-04 semantics, where boon checks `format` (`email`, `uri-reference`, …) by default. 3.1 and 3.2 treat `format` as an annotation, as their dialects say.
- **Schema error lists.** These are the leaves of the error tree, without repeats. For `oneOf`/`anyOf` failures that includes each alternative's complaint, which is noisy but always points at the right place.
- **Repositories.** The insert code was moved into shared functions that take a `Connection`: `insert_collection`, `insert_folder`, `upsert_request`, `insert_example`, `insert_environment`, `prepare_variables` and `insert_variables`. The existing repositories call the same functions.
  - One behaviour changed: a saved request's auth is now sealed inside the transaction rather than just before it.
- **UI.**
  - `Modal` gained a `size` prop (`default` / `wide`).
  - The import dialog opens the file chooser immediately. It re-previews on every option change.
  - After a failed write it keeps the document open, so Import can be pressed again. It releases the document on Cancel, on Escape, or when another file is chosen.
- **Credential store.** An import whose plan has an auth placeholder or a secret variable needs a usable credential store, the same as saving such a request by hand. Without one, the whole import fails with the secret-store error and nothing is written.

#### Files

- **Rust, import:**
  - `src-tauri/src/openapi/import/{mod,detect,validate,refs,sample,grouping,to_collection,to_collection_tests}.rs`
  - `src-tauri/src/domain/import_plan.rs`
  - `ImportRepository` in `domain/ports.rs`
  - `src-tauri/src/domain/services/openapi_import.rs`
  - `src-tauri/src/persistence/repositories/import.rs`
  - `src-tauri/src/commands/openapi_import.rs`
- **Rust, wiring:** `lib.rs` (`AppState.openapi_import` and 4 commands), plus the three `mod.rs` files.
- **Rust, shared insert refactor:** `collections.rs`, `folders.rs`, `saved_requests.rs`, `examples.rs` and `environments.rs` under `persistence/repositories/`.
- **Vendored schemas:** `src-tauri/schemas/openapi/`, with 9 schema files, `LICENSE` and `README.md`.
- **Cargo:**
  - `Cargo.toml` adds `boon = "0.6"`.
  - `serde-saphyr` gains `deserialize` in the release build, and its dev-dependency entry is gone.
  - `Cargo.lock` adds `boon`, `appendlist`, `fluent-uri` and `borrow-or-share`.
- **Tests:** `src-tauri/tests/repositories.rs` gains 5 tests: plan written, rollback, bad plan, round trip, and the large spec (ignored).
- **Frontend:**
  - `src/types/openapi-import.ts`
  - `src/services/openapi-import.ts` and its test
  - `src/lib/openapi-import.ts` and its test
  - `src/features/collections/ImportOpenApiDialog.tsx` and its test
  - `CollectionsSidebar.tsx` (import button)
  - `components/Modal.tsx`
- **Manual test files:** `tools/openapi-import-samples/`, whose README lists the expected result for each file.

#### Manual checklist

1. Run `verify.bat`, then `release.bat`. Check that the imported-DLL count is still 27, since `boon` is pure Rust.
2. Import each file in `tools/openapi-import-samples/` and compare with its README.
3. Import a real spec you use, and send one request after filling in the environment.
4. Export a collection as YAML, import the file, and export again. Nothing should change.

---

## Phase 8d follow-up — import dialog too tall — **done, verified 2026-09-17** (A + B1)

### Status of 8d

Checked on Windows on 2026-09-17:

- `verify.bat` is green: 217 vitest tests, 369 unit, 14 curl and 48 repository tests.
- `release.bat` is green, and the binary still imports 27 DLLs.
- The GitHub JSON and YAML specs and the Stripe spec were imported by hand without problems.

### The problem, from the screenshots

With the GitHub spec the preview shows about 160 variable chips. That produced two separate problems:

1. **A layout bug that affects every dialog.** `Modal` has no height limit and no scroll area. A dialog taller than the window is centred and clipped at the top and the bottom. In the smaller window, the title, the file line and the **Import / Cancel buttons could not be reached at all**. That makes this a bug, not polish. The Cookie Manager dialog only avoids it because it caps its own list at `max-h-96`.
2. **Too much content.** 129 of the ~160 variables are GitHub path parameters, and only 9 of those have an example value. The other ~20 are required query or header parameters with no value. So the environment would open in the editor with about 150 empty rows, and the dialog lists every one of them before the notes.

### Part A — fix `Modal` (needed whatever is chosen for Part B)

- **Height.** The dialog shell gets `max-h-[calc(100vh-2rem)]` and becomes a flex column.
- **Header and footer stay put.** The title stays at the top. A new optional `footer` prop renders the buttons in a row pinned to the bottom.
- **One scroll area.** Only the body between header and footer scrolls (`min-h-0 overflow-y-auto`), so buttons are always on screen.
- **Dialogs to move to `footer`:**
  - Import and Export move now. They are the tall ones.
  - Save request, Save example, Move and Confirm keep their buttons in the body. They are short, and a `Modal` without `footer` behaves exactly as today, so no other dialog has to change.
- **Remove nested scroll boxes.** The import dialog's notes list (`max-h-48`) and the refusal details (`max-h-64`) lose their own caps. The body scrolls once, which avoids a scroll box inside a scroll box.
- **Tests.**
  - `Modal`: the footer renders outside the scroll area.
  - The existing dialog tests must stay green.

### Part B — show the environment in less space (options)

**B1 — summary line plus "Show all".** Collapsed by default:

> Environment "GitHub v3 REST API" (not activated): 162 variables — `baseUrl`, 2 secret (`key`, `token`), 159 others. **Show all**

Expanding it shows the chips in a box about 4 rows high that scrolls. This is a frontend-only change.

**B2 — group by where the variable came from.**

- **Always shown:** Base URL and Credentials (secret, still to fill in). These are the ones the user has to act on.
- **Collapsed, count only:** "Request parameters (159, 9 with example values)".
- **Cost:** the preview DTO gains an `origin` on each variable (`baseUrl` / `credential` / `parameter`), which is a small Rust change. The domain `PreviewVariable` gets the same field, set where `to_collection` adds the variable.

**B3 — leave parameters out of the environment unless asked.**

- **New option:** a checkbox "Add path and required parameters to the environment".
- **Default:** on only when a spec needs few of them, for example 20 or fewer. The GitHub spec would open with it off.
- **When off:** those requests still say `{{owner}}`, and the user adds the variables they need.
- **What it fixes:** the environment editor would no longer open with ~150 empty rows, not just the dialog.
- **Cost:** a new field in `ImportOptions` and the DTO, one branch in `to_collection`, and a test.

**B4 — tabs or collapsible sections for the whole preview.**

- **Layout:** Options / Environment / Notes, as native `<details>` sections. There are no shadcn primitives in `components/ui/` yet, so no new dependency.
- **Downside:** the most in one pass, and it hides the notes, which are what the user should read.

**Recommendation: A + B2 + B3.**

- A makes every dialog usable at any window size.
- B2 puts the few variables that need action first.
- B3 fixes the same clutter at its source, in the environment editor.
- B1 is the cheapest fallback if B2/B3 feel like too much.

### Decided 2026-09-17

- **A — fix `Modal`: yes.**
- **B1 — summary line plus "Show all": chosen** for the variable list. This is a frontend-only change.
- **B2 (group by source) and B4 (collapsible sections): not now.**
- **B3 — rejected.** Parameters keep going into the environment as they do today, including empty ones. No new option is added and nothing changes in Rust.
- **What B1 shows collapsed:** the variable count, `baseUrl`, and the secret variables by name. These can be told apart with the data the preview already sends (`secret: true`, and `baseUrl` is the first variable). "Show all" opens the full chip list in a box about 4 rows high that scrolls (changed from 8 on 2026-09-17).

### As built (2026-09-17)

**`Modal`**

- **Layout.** The backdrop has `p-4` and the dialog has `max-h-full` with a flex column. The title and the new `footer` row stay put, and only `[data-modal-body]` scrolls.
- **Other dialogs.** A dialog without `footer` looks as it did before, because the padding is unchanged.
- **Test.** `Modal.test.tsx` is new. It checks the height cap, that the footer sits outside the scrolling body, that there is no footer row when none is passed, and Escape / outside-click.

**Import and Export dialogs**

- **Footer.** The buttons are now passed as `footer`. The error message sits in the footer too, on the left (`mr-auto`), so it is visible however far the report is scrolled.
- **One scroll area.** The notes list and the refusal details lost their own `max-h` and scroll, so the body is the only scroll area.

**B1**

- **Summary helper.** `environmentSummary()` is new, in `lib/openapi-import.ts`.
  - It produces "161 variables - baseUrl, 2 secret (a, b), 158 others".
  - It spells out at most 5 secret names, then "and N more".
  - It uses " - " rather than an em dash, as the rest of the dialog's text does.
- **Chips.**
  - Environments with 6 or fewer variables (`ALWAYS_LISTED_VARIABLES`) still show their chips directly.
  - Larger ones show only the summary, with a "Show all" / "Hide" toggle.
  - The expanded list is `max-h-24 overflow-y-auto`. Each chip row is 20 px plus a 4 px gap, so exactly 4 rows are visible.

**Checked in the cloud**

- `tsc`, eslint and prettier are clean. vitest: 228 tests pass (+11).
- No Rust changes.
- **Rendering checked.** A throwaway Vite page (removed afterwards) rendered the dialog with a mocked `__TAURI_INTERNALS__` and 161 variables. Playwright checked it in a 1200×830 and a 900×560 window:
  - The dialog stays inside the window.
  - The buttons stay visible.
  - The expanded list shows 4 rows.

**Verified on Windows (2026-09-17)**

- `verify.bat` is green: 228 vitest tests in 26 files, 369 unit, 14 curl and 48 repository tests.
- The dialog was checked by hand: the buttons stay reachable in a small window, and Show all works.

### Also noticed (not part of this change)

- The sidebar shows "GitHub v3 REST API" twice because the file was imported twice. **This is expected behaviour, confirmed 2026-09-17:** every import creates a new collection, and a repeated title gets no warning and no suffix.

---

## Sidebar header buttons with labels — **done, verified 2026-09-17**

- **What changed.** The two icon-only buttons at the top of the Collections sidebar now show their label next to the icon: "Import Collection" (`FileInput`) and "New Collection" (`Plus`). Together they fill the header row, half each (`grid-cols-2`).
- **Sizing.** At the sidebar's minimum width of 256 px, each half is 122 px wide.
  - At 12 px, "Import Collection" plus its icon and padding does not fit. So the labels use 11 px, the same size as the COLLECTIONS / ENVIRONMENTS / HISTORY tabs.
  - The icons are 14 px, button padding is `px-1.5` and row padding is `px-1`.
  - Measured with Arial metrics, which are close to Segoe UI: the "Import Collection" label is 83 px wide and has 92 px available.
- **If a font runs wider.** The label ends in "…" instead of wrapping, and the button's title shows the full text.
- **Empty state.** The text now reads "No collections yet. Create or import one above." It used to say "Click + to create one".
- **File:** `src/features/collections/CollectionsSidebar.tsx` only.
- **Verified on Windows:** `verify.bat` is green (228 vitest tests, 369/14/48 Rust tests), and `release.bat` is green with 27 imported DLLs and the PASS static-link check.

---

## Tray icon and orphan cleanup — **done, verified 2026-09-17** (`verify.bat` green; tray look still to eyeball)

- **Orphan deleted.** `src-tauri/src/openapi/v3_2.rs` is gone. No `mod` declared it and nothing else referred to it.
- **Tray art.**
  - `tools/build-installer-art.py` now also writes `src-tauri/icons/tray-{16,20,24,32,40,48}.png`. These use the small-size badge: a flat plate for sizes up to 32 px, and the gradient above that.
  - When the script was run again, the existing installer art came out byte-for-byte unchanged (checked by hash).
- **Tray code.** `desktop/tray.rs` no longer reuses `default_window_icon()`.
  - It embeds the six PNGs with `tauri::include_image!`, as raw pixels (about 25 KB), with no new crate or feature.
  - It picks the smallest size that is at least 16 px × the primary monitor's scale: 100 % → 16, 125 % → 20, 150 % → 24, 175/200 % → 32, 250 % → 40, 300 % → 48. If the monitor cannot be read, it assumes 100 %.
- **Not handled:** a DPI change while the app is running. The shell resizes the icon it already has.
- **Rebuilds.** `include_image!` does not make Cargo watch the PNGs. After regenerating them, touch `tray.rs` or run a clean build.
- **Tests.** 4 new unit tests: the size for each scale step, out-of-range scales, every embedded image being the size its name says, and the chosen icon matching the chosen size.
- **Verified in the cloud:** fmt and clippy are clean; `cargo test` passes 373 unit, 14 curl and 48 repository tests.
- **`verify.bat` (Windows, 2026-09-17):** green — 228 vitest; 373 unit (including the 4 tray tests), 14 curl and 48 repository tests.
- **Manual check still open:** look at the tray on Windows at 100 % and at your usual scale.

---

## Phase 10 — Microsoft Store — **planned 2026-09-17, decided, waiting on the Partner Center account**

Researched 2026-09-17. Sources:

- Tauri: "Distribute > Microsoft Store"
- Microsoft Learn: MSI/EXE and MSIX package requirements, certification process, code signing options, free individual registration, winapp CLI Tauri guide
- Windows Developer Blog, 2026-05-07: company registration is free

### The account

- **Cost.** Registration is **free** for individuals (since September 2025) and for companies (since May 2026). The $19 and $99 fees are gone.
- **Individuals.** Verification needs a government ID plus a selfie. Registration is at storedeveloper.microsoft.com.
- **Companies.** Verification needs a D-U-N-S number or business documents, plus a work email on the company domain.
- **Publisher name.** The name that appears in the Store comes from this account. `tauri.conf.json` had a publisher *inferred* from the original identifier; it is now `Zoran Davidović` (renamed 2026-09-18), and it must agree with the account. Tauri also requires that `publisher` is not the same as `productName`.
- **App name.** It must be reserved in Partner Center. "ResponderHTTP" may already be taken; checking is free and takes a minute.

### Two ways in

**Route A — MSIX package.** Microsoft's recommended way for new apps.

- **Signing:** the Store **re-signs the package for free**. No certificate, hardware token or yearly fee.
- **Updates:** the Store installs updates. That fits, because this app has no updater and auto-update is out of scope (Phase 7).
- **Packaging:** Tauri does not build MSIX itself. Microsoft's **winapp CLI** (`winget install microsoft.winappcli`) has a Tauri guide:
  - `winapp init` creates `Package.appxmanifest` and an `Assets/` folder.
  - `winapp pack` packs the release exe.
  - No Windows SDK or .NET is needed, but it needs **Windows 11**.
  - Because our exe is a single file with the frontend embedded, the package is just that exe plus the manifest and icons.
- **Alternative packager:** the third-party `@choochmeque/tauri-windows-bundle` (npm, MIT). It is smaller and less established (25 stars).
- **Package identity:** Name, Publisher and PublisherDisplayName are copied from Partner Center → *View app identity details*.
- **Version rule:** four parts, the last one `0`, and **the first part cannot be 0**. **Our `0.1.2` cannot be submitted as is.** The first Store release needs `1.x` (for example `1.0.0` → `1.0.0.0`).
- **Things that behave differently in a package** (each is a test item, not a blocker):
  - **App data is virtualised.** The database and logs end up under `%LOCALAPPDATA%\Packages\<package>\LocalCache\…`. Uninstalling the Store app deletes them.
    - A machine that also has the NSIS build may see copy-on-write behaviour on the existing `%APPDATA%` folder. This needs testing.
  - **Credential Manager works for packaged desktop apps.** The uninstall hook (`windows/hooks.nsh`) does not run for MSIX, so the data key stays behind after uninstall.
    - The data key is useless without the database, but this breaks the decision that key and database are always removed together.
    - Options: document it, or accept that a reinstall finds the old key. A reinstall has no database, so nothing is exposed.
  - ~~**WebView2 cannot be bootstrapped from an MSIX.**~~ Settled 2026-09-19 — see "WebView2 in the Store package" below.
  - **`runFullTrust`** (a desktop app) is a restricted capability. It is the normal case for packaged Win32 apps and is declared in the manifest.
  - **The tray, single instance and the file dialogs** need a smoke test under the package.
- **Store icons:** Square44x44Logo, Square150x150Logo, Wide310x150Logo and StoreLogo, each with scale and target-size variants. `tools/build-installer-art.py` can draw them from the same badge, just as it now does for the tray.

**Route B — the existing NSIS `.exe` (or MSI).**

- **Signing is required.** The installer and every PE file must be signed with a certificate that chains to the Microsoft Trusted Root Program.
  - **Azure Artifact Signing** costs about $9.99 a month. Individuals can use it **only in the USA and Canada**; organisations in the USA, Canada, the EU and the UK.
  - **OV certificate:** about $150–300 a year, on a hardware token or HSM.
- **Hosting.** The installer is hosted by us at a **versioned HTTPS URL** that never changes after submission (for example a GitHub Release asset). Each update means a new URL and a new submission.
- **Installer settings:**
  - It must be offline: `webviewInstallMode` becomes `offlineInstaller`, which adds about 130 MB to the installer. Tauri recommends putting that in a separate `tauri.microsoftstore.conf.json`.
  - It must install silently (`/S` for NSIS).
- **Updates are ours to deliver.** The Store does not update Win32 installers. That would bring in `tauri-plugin-updater` and an update server, which Phase 7 left out on purpose.
- **Advantage:** `hooks.nsh`, the data location and the downgrade block all stay exactly as they are now.

**Recommendation: Route A (MSIX via winapp CLI).** It is free, the Store handles updates, and there is no certificate to buy. Route B only makes sense if the signed installer will also be distributed outside the Store.

### Steps for Route A

1. **Account and name** (you): register, verify, reserve the name in Partner Center, and copy the identity values.
2. **Version:** bump to `1.0.0` in `package.json`, `Cargo.toml` and `tauri.conf.json`. Set `bundle.publisher` to match the Partner Center publisher display name.
3. **Store icons:** extend `tools/build-installer-art.py` to write the MSIX asset set.
4. **Packaging:**
   - `winapp init` (you, on Windows 11). Commit `Package.appxmanifest` with the Partner Center identity, `runFullTrust`, the minimum OS version and the assets.
   - Add a `pack-store.bat` next to `release.bat`: build, then `winapp pack`.
5. **Local test:**
   - Sign with a dev certificate (`winapp cert generate` / `cert install`) and install with `Add-AppxPackage`.
   - Smoke-test:
     - sending a request
     - saving a request with a secret, then restarting
     - the tray and single instance
     - import / export file dialogs
     - where the database lives
     - uninstall
   - Run the Windows App Certification Kit.
6. **Store listing** (you, with drafts from me):
   - description, screenshots, category (Developer tools)
   - the WebView2 line in *Additional system requirements* (see above)
   - age rating questionnaire, price (free?)
   - **privacy policy URL** — needed because the app stores credentials and talks to the network; a short page is enough
7. **Submit** the unsigned `.msix`. The Store signs it, and certification takes up to 3 business days.
8. **Follow-ups:** a line in CLAUDE.md §9/§10 for the Store build. The Phase 7 clean-machine test is still worth doing for the NSIS build.

### Decided 2026-09-17

- **Route A: MSIX, packaged with the winapp CLI.**
- **Individual account.**
  - Verification is a government ID plus a selfie.
  - The Store will show **your own name** as the publisher. So `bundle.publisher` becomes the exact PublisherDisplayName that Partner Center assigns, and the manifest's `Publisher` gets the `CN=…` value Partner Center gives.
  - ~~The company name stays in `identifier` and in the Credential Manager target name. Changing either would orphan every existing user's data key, so they stay.~~ Superseded 2026-09-18 — see "Identity rename" below. The app is not published anywhere, so there are no existing users to orphan.
  - The NSIS installer's publisher and copyright lines follow the new name, so the two builds agree.
- **First Store version: `1.0.0`**, which becomes `1.0.0.0` in the manifest.
  - Bumped in `package.json`, `Cargo.toml` and `tauri.conf.json` together.
  - The NSIS build moves to 1.0.0 as well. With `allowDowngrades: false`, no installed 0.1.x is blocked by this, because it is an upgrade.
- **Free.** No payout or tax setup is needed.

### Store package built — 2026-09-19

Partner Center identity arrived, so the package is assembled. Store ID
`9NV8JJQKDBRK`.

| Manifest field | Value |
|---|---|
| `Identity/Name` | `ZoranDavidovi.ResponderHTTP` |
| `Identity/Publisher` | `CN=921A9FC1-B849-48FC-BD33-F30886A7A063` |
| `Properties/PublisherDisplayName` | `Zoran Davidović` |
| `Identity/Version` | `1.0.0.0` |

The PFN `ZoranDavidovi.ResponderHTTP_4ka2c3wj7wsgy` and the package SID are
derived by Windows from those; nothing in the repo needs them, but they are
the values to expect when the app reports its own identity.

**`Package.appxmanifest`** (repo root, where winapp looks by default). A
packaged Win32 app: `EntryPoint="Windows.FullTrustApplication"` plus the
`runFullTrust` restricted capability. `Executable="ResponderHTTP.exe"`.
`TargetDeviceFamily Windows.Desktop`, MinVersion `10.0.17763.0`,
MaxVersionTested `10.0.26100.0`.

Only the two mandatory logos and the Store logo are referenced. The wide tile,
the large square tile and a splash screen are optional and deliberately left
out until a package is known to build — a manifest naming an asset the
generator did not produce fails at pack time, and that is a slow failure to
diagnose.

**`pack-store.bat`.** Same shape as verify.bat and release.bat: writes
`pack-store-log.txt` so a run can be read from off-machine. Plain run makes an
**unsigned** package for the Store, which re-signs it; `pack-store.bat test`
signs with a local development certificate so the package can be installed and
smoke-tested first. Steps: check winapp is on PATH, fail on version drift
between `tauri.conf.json` and the manifest, generate the asset set if it is
missing, `pnpm tauri build --no-bundle`, stage, pack.

- **The staging folder is `msix-stage\`, not `dist\`.** Microsoft's Tauri
  guide stages into `dist`, which in this repo is the Vite frontend build
  (`frontendDist`). Packing that would ship the whole web build beside an exe
  that already embeds it. Worth knowing before following that guide verbatim.
- **`--no-bundle`** skips the NSIS and MSI installers, which the package does
  not use and which cost minutes.
- **The exe is renamed on the way in**, `responderhttp.exe` →
  `ResponderHTTP.exe`, so Task Manager and the install folder show the product
  name rather than the Cargo crate name. The manifest's `Executable` matches.
- **Assets** come from `winapp manifest update-assets app-icon.png` — the same
  1024×1024 source the Tauri icons use. The tool draws every scale and
  target-size variant, which is a long list to keep by hand. Existing icons
  are untouched.

**Listing drafts** are in `store/listing.md`. The privacy policy lives on the
GitHub Pages site built from `docs/`: `index.html` (an overview built from the
README), `privacy-policy.html` and `terms.html`, plus `.nojekyll` so the
committed HTML is served verbatim instead of being run through Jekyll. That is
where the public privacy-policy URL Partner Center asks for comes from.
Licensing is `LICENSE` at the repo root — free to use, no distribution of
modified versions — and `terms.html` is its plain-language twin. **GPLv3 was
considered and rejected on 2026-09-23:** it guarantees the right to modify and
redistribute, which is the one thing this licence withholds, and §10 forbids a
licensor from adding that restriction back. Copyleft is a coherent choice for
this app, just not this one; if it is ever revisited, the licence, `terms.html`,
the README and the Store text all have to move together. The listing and
the policy were both refreshed on 2026-09-23 to cover what
shipped after they were first written — Markdown documentation (Phase 12),
WebSocket requests (Phase 13) and server-sent events (Phase 14). The policy
gained those in its stored-data list, widened "network requests" to include
WebSocket connections, and added WebSocket messages and stream events to the
list of what never reaches the log. The listing gained the features, two more
screenshots to take, and `websocket client` as a search term in place of
`api testing`, which only repeated `api client`. Whenever a phase adds a
visible feature, these two are part of it.

**First run, 2026-09-19.** The package builds. `winapp` produced
`ZoranDavidovi.ResponderHTTP_1.0.0.0_x64.msix`, 5.98 MB, from a 10.6 MB exe
plus the 44 generated assets. The manifest was accepted as written — winapp did
not rewrite it — and `winapp manifest update-assets` drew the whole set from
`app-icon.png` on its own.

Two failures on the way, both worth remembering:

1. **`LNK1285: corrupt PDB file`** on the first attempt, linking
   `responderhttp_lib.dll`. A stale debug-symbol file, nothing to do with the
   package. Deleting the PDB and rebuilding fixed it. Worth noting that the
   crate that failed is one nothing uses: `crate-type` is
   `["staticlib", "cdylib", "rlib"]` from the Tauri mobile template, and the
   desktop binary and the integration tests only need `rlib`.
2. **pack-store.bat ran both branches at once.** cmd parses a parenthesised
   block in one pass, so the unescaped `)` in
   `echo === winapp pack (signed, for local testing) ===` closed the `if`
   early. The signed branch and the unsigned branch both ran, four
   redirections collided on the log file, `winapp cert generate` never ran at
   all, and the script then reported a signed package when what was on disk
   was the unsigned one — under a different filename than the one it printed.
   Rewritten with `goto` labels and no bracket in any echo. The filename is now
   read back from disk rather than assumed, because winapp names the file after
   the manifest's `Identity/Name`, not after the product.

**Package verified, 2026-09-19.** The rewritten script ran clean — the
unsigned branch only, correct header, correct filename read back from disk.
The `.msix` was opened and inspected rather than trusted:

| | |
|---|---|
| Entries | 54: `ResponderHTTP.exe`, 44 assets, manifests, PRI resources |
| Size | 10.5 MB uncompressed, 5.98 MB packed |
| `AppxSignature.p7x` | absent — unsigned, which is what Partner Center wants |
| Strays | none: no `dist/`, no JS or CSS, no `.pdb` |
| Identity as packaged | `ZoranDavidovi.ResponderHTTP`, `CN=921A9FC1-…`, `Zoran Davidović`, `1.0.0.0`, `x64` |

The staging-folder decision is visible in that result: the frontend is embedded
in the exe and appears nowhere else in the package. winapp adds a
`build:Metadata` element and its namespace to the manifest it packages; the
file on disk is untouched.

**Installed and running under the package, 2026-09-19.** The signed path works
too: `pack-store.bat test` generates `devcert.pfx` from the manifest's
Publisher, signs, and the package installs and launches. So the whole chain —
manifest, assets, staging, pack, sign, install — is proven end to end on a real
machine.

Sequence, for next time: `pack-store.bat test` in cmd, then
`winapp cert install .\devcert.pfx` in an **elevated** shell, then
`Add-AppxPackage .\ZoranDavidovi.ResponderHTTP_1.0.0.0_x64.msix` in
**PowerShell** — `Add-AppxPackage` is a cmdlet and does not exist in cmd, and
double-clicking the .msix in Explorer does the same job. The development
certificate is self-signed and trusted machine-wide once installed; remove it
from certmgr.msc when testing is done.

**Still open before submitting:**

1. **The privacy policy needs a public URL.** Partner Center requires one and
   the text is only a file in the repo. GitHub Pages on `zoran-php/responderhttp`
   is the free route.
2. **Screenshots** — at least one, 1366×768 or larger.
3. ~~**A first `pack-store.bat test` run.**~~ Done — see above. Still to do:
   a *signed* run, which has never completed, so the package has never been
   installed.
4. **Smoke test under the package.** It starts — the rest still needs eyes:
   the tray and Show/Quit, single instance, the import/export file dialogs,
   where the database ends up
   (`%LOCALAPPDATA%\Packages\ZoranDavidovi.ResponderHTTP_4ka2c3wj7wsgy\LocalCache`),
   and that the data key still reaches Credential Manager. The app-data folder
   is the one most likely to surprise: it is virtualised under the package, not
   the roaming `%APPDATA%` path the NSIS build uses.
5. **Windows App Certification Kit** before uploading.

### WebView2 in the Store package — decided 2026-09-19

Researched 2026-09-19. The question was whether the runtime can be downloaded
during installation, as the NSIS build does with
`webviewInstallMode: downloadBootstrapper`.

**It cannot, and it does not need to be.**

- **MSIX has no install-time actions at all.** Nothing of ours runs during the
  install, so `webviewInstallMode` — which only drives the NSIS and WiX
  bundlers — has no effect on the package. `winapp pack` wraps the built
  `.exe`; the setting stays as it is for the installers that do use it.
- **The one declarative route does not apply to the Store.**
  `win32dependencies:ExternalDependency` with `Microsoft.WebView2` exists, but
  Microsoft's own page says it applies "only to installs that use the
  Microsoft App Installer app" and is ignored for every other mechanism, the
  Store included. Reports of it failing in exactly this way are easy to find.
- **The Store would not want a download anyway.** Tauri's Microsoft Store page
  says a Win32 installer listed in the Store must use `offlineInstaller`,
  because the Store requires silent, offline installation.

**Decision: rely on the Evergreen Runtime already being present.** It is part
of Windows 11, and Microsoft states that "the vast majority of Windows 10
devices have the WebView2 Runtime installed already" — it went out through
Edge and Windows Update, and Windows 10 itself left support in October 2025.
Nothing is added to the package.

**Rejected: bundling the fixed-version runtime.** About 180 MB, it ends the
single-small-binary property, it makes us responsible for shipping Chromium
security patches, and there is an unresolved report (WebView2Feedback #4175)
of WACK rejecting packages that include it for "use of prohibited APIs by the
WebView2 runtime" — a certification failure that would only show up late.

**What covers the remainder**

1. **A startup check** (`src-tauri/src/desktop/webview.rs`, built 2026-09-19).
   Tauri creates the window declared in `tauri.conf.json` inside its own
   `setup`, *before* the setup hook this app registers, and a failure there is
   a `panic!`, not a returned error. The release binary is GUI-subsystem with
   `panic = "abort"`, so a missing runtime was a silent crash. `run()` now
   calls `tauri::webview_version()` first; on failure it shows a native dialog
   naming the runtime and Microsoft's download page, and returns the new
   `StartupError::NoWebview` so the process still exits non-zero. On success
   the version is logged once logging exists, so a rendering bug report says
   which WebView2 build drew the window.
   - The dialog uses `rfd` directly, because there is no app handle before the
     builder runs and `tauri-plugin-dialog`'s builder needs one. It is the
     same crate that plugin uses underneath, declared with the same version,
     features and target filter — `Cargo.lock` gained one line (`"rfd"` under
     the `responderhttp` package) and no new package: 595 before, 595 after.
2. **A line in the Store listing.** Partner Center's *System requirements*
   section is hardware-only; the *Additional system requirements* free-text
   field on the listing is where this goes: "Requires the Microsoft Edge
   WebView2 Runtime, included in Windows 11 and installed on most Windows 10
   PCs."

Sources: Microsoft Learn "Distribute your app and the WebView2 Runtime";
`win32dependencies:ExternalDependency`; Tauri "Microsoft Store" and "Windows
Installer"; "System requirements for MSIX app"; WebView2Feedback #4175.

### Product rename — decided 2026-09-19

**Responder → ResponderHTTP**, everywhere. A new repository came with it:
`https://github.com/zoran-php/responderhttp.git`.

Every mention above and below has been rewritten to the new name, so this
entry is the only record that the old one existed.

- **What users see:** `productName`, the window title, the tray tooltip, the
  missing-webview dialog, `index.html`'s `<title>`, `bundle.longDescription`
  and the wording the OpenAPI export writes into a document ("Exported from
  the ResponderHTTP collection …").
- **Identity:** `identifier` → `io.github.zoran-php.responderhttp`, with
  `KEYCHAIN_SERVICE`, `KEYCHAIN_TARGET` and `windows/hooks.nsh` following it.
  The three keychain tests that tie those together still pass.
- **Internals:** the Cargo package `responder` → `responderhttp`, the library
  `responder_lib` → `responderhttp_lib` (so the release binary is
  `responderhttp.exe`, which `release.bat` now looks for), the database file
  `responderhttp.sqlite3`, the log file `responderhttp.log`, `package.json`'s
  and `package-lock.json`'s `name`, and the test env vars
  `RESPONDERHTTP_BIG_SPEC` / `RESPONDERHTTP_DUMP_EXPORTS`.
- **The secret scope prefix** in `domain/secrets.rs` went from `responder/v1`
  to `responderhttp/v1`. That string is the AEAD's associated data, so it
  would have made every existing sealed value undecryptable — which costs
  nothing here, because the identifier change already gives the app a new
  app-data folder and a new data key. Changing it later would not have been
  free, so it was changed now.
- **Left alone: the icons and the generated installer art.** Asked for.
  `tools/build-installer-art.py` now draws "ResponderHTTP" on the NSIS header
  and sidebar bitmaps, but the committed `.bmp`s were not regenerated, so the
  installer still shows the old wordmark until someone runs the script.
- **Not done here:** the claude.ai project instructions hold their own copy of
  CLAUDE.md and have to be updated by hand.

**Second fresh start for local data.** Same reasoning as the identity rename
below: the app-data folder and the Credential Manager entry are both named
after the identifier, so a build after this change finds an empty database and
creates a new key. Nothing is shipped, so there is nobody to migrate.

### Identity rename — decided 2026-09-18

The company name had to go, because the Store account is an individual one
and the publisher shown there will be a person.

- **Identifier: now `io.github.zoran-php.responderhttp`.**
  A GitHub-based reverse-DNS identifier, from
  `https://github.com/zoran-php/responderhttp.git`. It is owned by an account that
  actually exists, it costs nothing, and it does not have to change if the
  project later gets its own domain. Hyphens are allowed in a Tauri identifier
  and in the Windows folder name derived from it.
- **`bundle.publisher`: now "Zoran Davidović".**
  `bundle.copyright` follows: `© 2026 Zoran Davidović`. Tauri requires
  `publisher` ≠ `productName`, and "ResponderHTTP" is the product name, so this is
  fine. When Partner Center assigns a PublisherDisplayName, `bundle.publisher`
  must be changed to match it exactly.
  - `package.json`'s `author.name` was the ASCII spelling, `Zoran Davidovic`;
    changed to `Zoran Davidović` the same day so every name in the repo is
    spelled the same.
- **Existing local data: start fresh.** No migration code. The identifier is
  the name of the app-data directory and of the Credential Manager entry, so a
  build after this change finds an empty database and creates a new data key.
  Nothing is published, so the only affected installs are the developer's own;
  the app-data folder and the Credential Manager entry left behind under the
  previous identifier can be deleted by hand.
- **What changed:** `src-tauri/tauri.conf.json` (identifier, publisher,
  copyright), `src-tauri/src/secrets/keychain.rs` (`KEYCHAIN_SERVICE`,
  `KEYCHAIN_TARGET`), `src-tauri/windows/hooks.nsh` (the `cmdkey /delete:`
  target) and this file. The three keychain tests that tie the constant to
  `hooks.nsh` and to `tauri.conf.json` keep them in step, and they still pass.
- **Version bumped to `1.0.0`** in the same pass, in `package.json`,
  `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and
  `src-tauri/tauri.conf.json`, as decided on 2026-09-17.

### Next

1. ~~**You:** register at storedeveloper.microsoft.com, reserve the name, and send me the three identity values from *Product management → View app identity details*: Package/Identity/Name, Package/Identity/Publisher and PublisherDisplayName.~~ Done 2026-09-19 — see "Store package built" above.
2. **Me, meanwhile:**
   - ~~the version bump to 1.0.0~~ done 2026-09-18, with the identity rename
   - ~~the missing-webview startup check~~ done 2026-09-19
   - ~~the rename to ResponderHTTP~~ done 2026-09-19
   - ~~the Store icon set~~ done 2026-09-19 — delegated to
     `winapp manifest update-assets`, run by pack-store.bat
   - ~~a pack-store.bat~~ done 2026-09-19
   - ~~draft listing text and a privacy policy~~ done 2026-09-19, in `store/`
   - the installer art rerun for the new wordmark (NSIS only, not the Store) in `tools/build-installer-art.py`
   - a `pack-store.bat`
   - draft listing text and a privacy policy
3. **When the identity values arrive:**
   - put them in the manifest and in `bundle.publisher`
   - you run `winapp init` / `winapp pack` on Windows 11
   - local install and smoke test, then submit.

---

## Phase 11 — Close-to-tray notice — **built 2026-09-19**

Closing the window hides the app to the tray (Phase 7). Nothing said so, which
reads as "the X did nothing". The first closes now raise a Windows toast with
two buttons: **Got it** and **Don't show this again**.

### Decided 2026-09-19

- **A real toast, with buttons**, over an in-app dialog. Chosen by the user
  from three options; the dialog would have been cheaper and fully testable,
  the toast is what a tray app is expected to do on Windows.
- **`tauri-winrt-notification` 0.8.1 directly, not `tauri-plugin-notification`.**
  The notice needs two buttons and a callback saying which was pressed. The
  plugin's JS API describes actions but its documentation states no Windows
  support for them; this crate is from the same tauri-apps org, is what that
  plugin wraps underneath, and has `add_button` and `on_activated` outright.
  - **Cost: 6 new packages** — the crate plus `windows` 0.62.2,
    `windows-collections`, `windows-future`, `windows-numerics`,
    `windows-threading`. That is a *second* `windows` version in the tree
    alongside the 0.61.3 Tauri already pulls. All of it is behind
    `cfg(windows)`, so nothing is compiled on other platforms.
- **AppUserModelID: try the package id, fall back.** A toast is attributed to
  a registered AUMID. The Store build has one from its package identity —
  `ZoranDavidovi.ResponderHTTP_4ka2c3wj7wsgy!App`, PackageFamilyName plus the
  manifest's Application Id. The NSIS build is unpackaged and has none, so it
  falls back to the id the crate documents for that case, at the cost of the
  toast claiming to come from PowerShell. `show` tries the package id and
  falls back when Windows rejects it, rather than detecting which build it is:
  one code path, and it cannot get the detection wrong.
  - **Follow-up, not done:** giving the NSIS build a proper AUMID needs a
    Start Menu shortcut carrying `System.AppUserModel.ID`. Until then the
    attribution is wrong in that build and in `tauri dev`.
- **The preference is stored, in SQLite** — migration `0008_app_settings.sql`,
  a key/value table. Key/value rather than a column per setting so the next
  shell preference does not mean another migration, the same reasoning as the
  JSON columns on requests. Never a home for secrets.
- **Fail towards telling the user.** If the preference cannot be read the
  notice shows; if it cannot be written the notice comes back next time. The
  two ways to be wrong are not equal — a notice someone dismissed is an
  annoyance, while swallowing it leaves them believing they quit an app that
  is still running.

### Shape

| File | Role |
|---|---|
| `persistence/migrations/0008_app_settings.sql` | the table |
| `domain/ports.rs` — `AppSettingsRepository` | get/set by key |
| `persistence/repositories/app_settings.rs` | SQLite, upsert on conflict |
| `domain/services/tray_notice.rs` | the decision, and the key name |
| `desktop/toast.rs` | draws the toast; strings and action ids as constants |
| `desktop/window.rs` | hides, then asks TrayNotice whether to notify |

`desktop/` holds no rule of its own: it asks the service (CLAUDE.md section 3).
The window hides *first* and the toast follows, because the toast describes a
window that has already gone.

`notify_hidden_to_tray` uses `try_state`, not `state`. Tauri creates the
config window inside its own setup, before the hook that calls `manage` — the
same ordering that makes the webview check a pre-flight — so a close in that
gap would panic on `state`, and a panic in an event handler takes the app with
it.

### Verified on Windows, 2026-09-20

The `cfg(windows)` body of `desktop/toast.rs` could not be compiled where it
was written — the cloud container is Linux and `cargo check` skips it — so it
was written against the vendored source of tauri-winrt-notification 0.8.1
rather than from memory, with everything testable pushed onto the
platform-independent side.

`verify.bat` settled it. **`cargo clippy --all-targets -- -D warnings`
returned 0 on Windows**, so the Windows body compiles against the real API,
and every `desktop::toast` test passes. The run also exercises
`desktop::webview::tests::windows_is_told_where_to_get_the_runtime`, the
`cfg(windows)` test the cloud never sees — which is why Windows counts **389**
unit tests where the cloud counts 388. Repository tests: 52 passing, 1
ignored, including the four new `app_settings` ones against the real
migrations. Frontend: 228.

Verified in the cloud: `cargo fmt --check`, `clippy --all-targets -D warnings`,
`cargo test` (383 + 14 + 52, up 8 unit and 4 repository tests), `tsc --noEmit`,
eslint, vitest 228.

### Fixed 2026-09-20 — the buttons showed the window

Reported after the first Windows run: pressing either toast button brought the
app out of the tray and focused it, which defeats the point of the notice.

**The toast was not doing it — we were.** Microsoft's guidance for packaged
desktop apps: "If you add the COM activator to your existing packaged app,
then Foreground/Background and Legacy notification activations will activate
your COM activator instead of your **command line**." There is no COM
activator here, so the activation *is* the command line: Windows relaunches
the exe with the button's id appended, `tauri-plugin-single-instance` catches
the second launch, and its callback called `show_main_window`. From the
plugin's side a toast press and a user double-clicking the app looked
identical.

`activationType="background"` on the actions would stop the relaunch, but
tauri-winrt-notification 0.8.1 writes `<action content= arguments= />` and
exposes no way to set it — and without a COM activator a background activation
has nowhere to land either.

**The fix is in our code.** `toast::action_from_args` reads a button id out of
the process arguments, and the single-instance callback returns early when it
finds one instead of showing the window. Button ids are namespaced
(`tray-notice:dismiss`, `tray-notice:never-again`) so they cannot be confused
with a real argument.

This also repaired something that was quietly broken: if the activation
arrives as a relaunch, the in-process `on_activated` callback never fires, so
"Don't show this again" was very likely not saving at all in the packaged
build. Both paths are now handled — the callback for unpackaged builds, the
command line for packaged ones — and recording the preference twice is
harmless.

Clicking the toast *body* still shows the window. No `launch` argument is set
on the toast element, so that activation carries no id, falls through, and
behaves like any other second launch. That seems right: the buttons are for
staying out of the way, the body is for "let me see it".

### To check by hand on Windows

1. Close the window with X on a fresh profile — the toast should appear.
2. Press **Got it** — the window must stay hidden. Close again; it should
   appear again.
3. Press **Don't show this again** — the window must stay hidden. Close again:
   silence, and `app_settings` should hold
   `tray_close_notice_dismissed = true`.
4. Restart the app and close — still silent.
5. Click the toast *body* rather than a button — the window should come back.
5. In the Store build, the toast should be attributed to ResponderHTTP. In the
   NSIS build and in `tauri dev`, expect PowerShell until the shortcut
   follow-up above is done.

---

## The MSI could not be built — fixed 2026-09-20

`release.bat` failed at the last bundling step. NSIS produced
`ResponderHTTP_1.0.0_x64-setup.exe` fine; the WiX step died with nothing but

```
Running light to produce ...ResponderHTTP_1.0.0_x64_en-US.msi
failed to bundle project: `failed to run ...WixTools314\light.exe`
```

**Cause: the diacritic in the publisher name, and a codepage.** The generated
`target/release/wix/x64/` workspace gives it away:

- `locale.wxl` sets `TauriCodepage` to **1252**, and `main.wxs` line 29 passes
  it as the package's `SummaryCodepage`.
- The only non-ASCII character anywhere in `main.wxs` is **`ć` (U+0107),
  seven times** — in `Manufacturer` and in every
  `Software\Zoran Davidović\ResponderHTTP` registry key.
- `ć` does not exist in Windows-1252. Encoding it to that codepage yields
  nothing at all.

That explains the shape of the failure exactly: `candle` succeeded and wrote
`main.wixobj`, because a `.wxs` is UTF-8 XML and any character is fine there.
`light` is where strings are written into the MSI database under the package
codepage, and there `ć` cannot be represented. NSIS succeeding alongside it is
the corroboration — NSIS is Unicode and never had the problem.

**Fix: ASCII in the Win32 installer metadata only.** `bundle.publisher` and
`bundle.copyright` in `tauri.conf.json` are now `Zoran Davidovic` and
`© 2026 Zoran Davidovic` — `©` is fine, it is 0xA9 in 1252. Every
`bundle` string is now checked to survive a cp1252 round trip.

**`Package.appxmanifest` keeps `Zoran Davidović`,** with the diacritic. It
has to: it must match Partner Center exactly, and MSIX has no codepage limit —
it is UTF-8 XML throughout, which is why `pack-store.bat` never had trouble.
The Store never reads `tauri.conf.json`, so the two do not need to agree.

So the rule is: **the real spelling everywhere except the two
`tauri.conf.json` bundle fields that a codepage-bound Win32 installer reads.**

Alternatives not taken: overriding the WiX locale to codepage 1250 (which does
have `ć`) changes the installer's language, and UTF-8 summary codepages are
not safe across Windows Installer versions; dropping the `msi` target would
lose the artifact corporate deployment tooling expects (Phase 7).

**Honest note:** this was predicted and waved through. When the publisher was
first set on 2026-09-18 the non-ASCII spelling was flagged as "a mild risk" in
the installers and chosen anyway. It was not mild.

---

## The static-link check failed on `combase.dll` — fixed 2026-09-20

With the MSI fixed, `release.bat` got as far as the last gate — the
static-link check against the real binary — and failed there:

```
Imported DLLs (29):
  ... api-ms-win-core-winrt-l1-1-0.dll ... combase.dll ...
FAIL: DLLs a clean Windows install does not guarantee:
  combase.dll
exit=1
```

**The count went 27 → 29, and both new entries are the Phase 11 toast.**
`tauri-winrt-notification` calls WinRT, and WinRT activation *is* COM
activation, so the binary now imports `api-ms-win-core-winrt-l1-1-0.dll` and
`combase.dll`. The first matched the existing `^api-ms-win-core-` pattern and
passed silently; the second had nothing to match. Nothing else in the list
changed — it was diffed entry by entry against the 27 recorded on 2026-09-15.

**This is a stale allowlist, not a new dependency.** `combase.dll` is the COM
base runtime and has lived in System32 since Windows 8; `ole32.dll` and
`oleaut32.dll`, both already on the allowlist, are largely forwarders onto it,
so the binary was reaching this code all along and only now names it directly.
It is absent on Windows 7, which cannot run this app anyway: WebView2 puts a
Windows 10 floor under it and `Package.appxmanifest` sets MinVersion
10.0.17763.0. No user installs anything.

The allowlist in `spikes/static-link-proof/check-windows.ps1` now carries
`^combase\.dll$` with that reasoning next to it. The check keeps its real job
intact — `vcruntime140.dll`, `msvcp140.dll`, `libcurl`, `libssl`, `sqlite3.dll`
still fail it — and the `api-ms-win-*` patterns stay deliberately narrow
(`-core-` and `-crt-` only) rather than being widened to a blanket match,
so the next family Microsoft adds still has to be looked at by a human.

**Baseline for future runs: 29 imports.** Any other number is a change to
explain, not a number to update.

**Verified on Windows the same day:** `release.bat` is green end to end —
`verify.bat` inside it clean (228 vitest, 389 + 14 + 52 Rust tests), both
bundles built (`ResponderHTTP_1.0.0_x64-setup.exe` and
`ResponderHTTP_1.0.0_x64_en-US.msi`), and the static-link check reporting
`PASS: only Windows system DLLs are imported.` at 29 imports.

---

## Phase 12 — Item-level Markdown documentation

Docs for a collection, a folder or a request: a Markdown tab opened from the
sidebar's right-click menu, edited in Monaco, previewed rendered, stored with
the item and carried through OpenAPI export and import.

Docs are the one thing a collection cannot currently record. The endpoint is
saved, its headers are saved, an example response is saved — why it exists,
what the payload means, which error the third call returns, all of that lives
in someone's head or in a wiki nobody opens. This phase gives it a home
beside the request.

### What the codebase already gives us

Read before planning, so the estimate is against the real code and not a guess:

- **Tabs are already a discriminated union.** `Tab = RequestTab |
  EnvironmentTab | ExampleTab` in `store/request-store.ts`. `DocsTab` is a
  fourth arm, not a new mechanism. `EnvironmentTab` and `ExampleTab` hold only
  an id and read the name from their store, which is how a sidebar rename
  reaches the tab label for free — docs follow that for the label and carry
  their own text for the body.
- **The context menu exists**, with a `menuItems(target)` in
  `CollectionsSidebar.tsx` already branching on collection / folder / example /
  request. Three of those four arms gain one entry.
- **Monaco already speaks Markdown.** `CodeEditor` takes
  `{value, language, readOnly, onChange}` and `LazyCodeEditor` wraps it in
  Suspense. Edit mode is `language="markdown"` and nothing else.
- **The split view's arithmetic is written.** `lib/split-pane.ts` serves both
  axes and is tested; `ResizeHandle` is the component. Split view reuses them
  rather than growing a third divider implementation.
- **`Tag` already carries `description`** (`openapi/document.rs`), and
  `from_collection.rs` sets it to `None` on every folder today. Folder docs
  drop straight in.
- **Ctrl+S is taken and Ctrl+Shift+D is free.** `lib/shortcuts.ts`'s
  `isSaveShortcut` deliberately rejects Ctrl+Shift+S, so the modifier
  discipline is already there to copy.

And two things it does not give us:

- **`Operation` has no `description` field at all.** It must be added to the
  struct, with `skip_serializing_if = "Option::is_none"` like its neighbours.
- **Import throws operation descriptions away.** `to_collection.rs` reads
  `description` only for responses; a request's name comes from summary →
  operationId → route and the description is dropped on the floor. So this
  phase also stops discarding the documentation in every spec ever imported,
  which is arguably the larger win.

### Decisions taken 2026-09-21

**Renderer: `marked` + `dompurify`.** Two packages, both zero-dependency,
about 45 KB together. The alternative considered was `react-markdown` +
`remark-gfm`, which renders to React elements and so cannot inject HTML at
all; it was turned down for the ~40-package `unified`/`micromark` tree behind
it. The cost of the choice is explicit and is written down here so it is not
forgotten: **we own the `innerHTML` call site, and sanitising is not optional
there.** One function in `lib/`, one test that a `<script>` and a
`javascript:` href do not survive it, and no second place in the app that
renders Markdown.

**Saving: debounced autosave plus explicit Ctrl+S.** Autosave ~800 ms after
typing stops and on tab blur and close; Ctrl+S forces it immediately because
users who reach for it expect it to mean something. The dirty dot shows only
while a save is pending or has failed — with autosave there is nothing to
prompt about on close, so a Docs tab never raises the close dialog that a
dirty request tab does. Losing prose is worse than losing a form field, and
that asymmetry is the whole reason for the divergence.

**Docs travel through OpenAPI, in this phase.** Collection docs ↔
`info.description`, folder docs ↔ tag `description`, request docs ↔ operation
`description`. It is the only way the spec's third acceptance criterion —
export, re-import, docs intact — can pass, since OpenAPI is this app's only
export format.

### Data model

Migration `0009_item_docs.sql`, three columns, one per owning table:

```sql
ALTER TABLE collections ADD COLUMN docs_md TEXT NOT NULL DEFAULT '';
ALTER TABLE folders      ADD COLUMN docs_md TEXT NOT NULL DEFAULT '';
ALTER TABLE requests     ADD COLUMN docs_md TEXT NOT NULL DEFAULT '';
```

`MIGRATIONS` in `database.rs` becomes `[&str; 9]`.

A single polymorphic `docs (item_type, item_id, markdown)` table was
considered and rejected: SQLite cannot enforce a foreign key whose target
table varies, so deleting a collection would leave its docs behind as orphan
rows, and every other table in this schema gets `ON DELETE CASCADE` for free.
A column on the owning table inherits the cascade and needs no new index.

**Verified, and worth a regression test:** the `requests` upsert in
`persistence/repositories/saved_requests.rs` names its `DO UPDATE SET` columns
one by one and does not touch `docs_md`, so pressing Save in the request
builder cannot wipe a request's documentation. That is true by accident of how
the statement is written, which is exactly the kind of thing that stops being
true during a later edit — hence the test.

Docs are stored **in plain text and exported verbatim**. They are prose the
user writes, not a credential, so they do not go through the Phase 9 sealing
path. The consequence is worth stating plainly: a token pasted into a Docs tab
will appear in the exported OpenAPI file. `domain/secrets.rs` stays the one
place that decides what is secret, and `docs_md` is not on that list.

A size cap, mirroring the existing request-body cap (`MAX_BODY_BYTES` and its
`a_body_past_the_cap_is_refused_before_it_reaches_storage` test): a named
constant, checked in the domain service before storage, with the same
at-the-cap and past-the-cap pair of tests.

### Ports, services, commands

Two methods on each of the three existing traits in `domain/ports.rs`, rather
than one new `DocsRepository`:

```rust
fn docs(&self, id: &str) -> Result<String, AppError>;
fn set_docs(&self, id: &str, markdown: &str) -> Result<(), AppError>;
```

This is what §7's interface segregation asks for — a repository trait per
aggregate — and it costs three mock updates in the existing test suites.

`domain/services/docs.rs` holds the rule that belongs to nobody else: trim
nothing (leading whitespace is meaningful in Markdown), enforce the cap, and
dispatch on target kind to the right repository. `commands/docs.rs` is the
usual thin pair, `item_docs` and `set_item_docs`, over a
`DocsTargetDto { kind, id }` in `commands/dto.rs`, mirrored in
`src/types/docs.ts` and wrapped in `src/services/docs.ts`.

### Frontend

- `DocsTab { kind: "docs"; id; target: { kind: "collection" | "folder" |
  "request"; id: string }; markdown: string; savedMarkdown: string; view:
  "edit" | "preview" | "split" }`. The target id gives the label its name from
  the collections store, so a rename in the sidebar retitles the tab; the two
  text fields make `isDirty` a string comparison, as `savedSnapshot` already
  does for requests.
- `openDocs(target)` focuses an existing tab for the same target before
  opening a new one — the spec's single-instance rule, and the same shape as
  the existing `openEnvironment` / `openExample`.
- `ContextMenuItem` gains an optional `icon`, since the spec asks for a
  document icon and the type currently has no room for one. `FileText` from
  lucide-react, which is already the app's only icon set.
- `features/docs/DocsEditor.tsx` — toolbar (mode switcher, then bold, italic,
  code, link, list, heading), `LazyCodeEditor` on the left, preview on the
  right. The formatting buttons are a pure function in
  `lib/markdown-format.ts` (`applyFormat(text, selection, action) → {text,
  selection}`) so the wrapping and unwrapping rules are unit-testable without
  a DOM, and the component only moves the cursor.
- `lib/markdown.ts` — `renderMarkdown(md): string`, `marked` then DOMPurify,
  the single sanitising chokepoint.
- Ctrl+Shift+D opens docs for the selected item; a matcher in
  `lib/shortcuts.ts` beside `isSaveShortcut`, with the same
  modifier-exactness tests.

### Rendering safety

An imported OpenAPI document's descriptions are **third-party text that will
be rendered into a webview holding Tauri IPC**. That makes the preview pane a
genuine attack surface rather than a theoretical one, and it decides three
things:

1. **Sanitise, in one place.** DOMPurify with a restrictive allowlist; no
   `<script>`, no event handlers, no `javascript:` or `data:` hrefs.
2. **Links must not navigate the app.** A plain `<a href="https://…">` click
   inside the app replaces the whole UI with the remote page, with no way
   back. Preview intercepts clicks on anchors and prevents the default. What
   happens next is the one open question below.
3. **Remote images do not load by default.** `![](https://tracker/x.png)` in
   an imported description is a beacon that fires the moment the pane renders
   — and the Store listing promises no telemetry and no analytics. Remote
   image sources are stripped, with a per-tab "load images" toggle if the user
   wants them.

Syntax highlighting inside preview code blocks uses **Monaco's
`editor.colorize`**, which is already bundled. No `highlight.js`, no second
grammar set, no third dependency.

### OpenAPI mapping

| Item | OpenAPI field | Note |
|---|---|---|
| Collection | `info.description` | Currently holds a generated "Exported from the ResponderHTTP collection …" sentence. Docs replace it when present; the generated line stays only when there are no docs. |
| Folder | `tags[].description` | Already `Option<String>`, always `None` today. |
| Request | `operation.description` | Field does not exist yet; add it. `summary` keeps carrying the request name. |

Edge cases that need naming rather than discovering later:

- **Flat export has nowhere to put folder docs.** When the grouping is not
  tag-based, a folder's documentation has no OpenAPI home. It becomes an
  `ExportNote` — the user is told what was left out, the way credential
  omission is reported today — rather than being dropped silently.
- **Nested folders are nested tags.** Docs follow the tag that the existing
  parent-nesting already produces; no new mapping.
- **Import by path grouping invents folders** that never had docs. They import
  with empty docs, which is correct and needs no special case.
- **Round-trip is not lossless in one direction:** re-importing an exported
  collection recreates docs from the descriptions, but a folder whose docs
  were dropped by a flat export cannot come back. The export note is the only
  honest answer.

### Order of work

1. Migration `0009`, `MIGRATIONS` count, and a schema test that the three
   columns exist.
2. Repository methods and their tests, including the regression test that
   `save` leaves `docs_md` alone.
3. `domain/services/docs.rs` with the cap, against mock repositories.
4. Commands, DTO, TS types, TS service wrapper.
5. `lib/markdown.ts` and `lib/markdown-format.ts` with their tests — pure,
   highest coverage, written before the component that uses them.
6. `DocsTab` in the store, `openDocs`, dirty handling, autosave.
7. `DocsEditor`, toolbar, split view, context menu entry, shortcut.
8. `Operation.description`, export mapping, export note for flat grouping.
9. Import mapping for all three levels, plus the ignored-features tally.
10. Component tests, then the full `verify.bat` / `release.bat` pass.

### Tests

- Rust: repository round-trip per level; cascade deletes docs; `save` does not
  clobber; cap at and past the limit; export writes all three descriptions;
  flat export raises the note; import reads all three back; export → import →
  export is unchanged (the existing `export_then_import_then_export_changes_nothing`
  test extended rather than duplicated).
- TypeScript: `renderMarkdown` renders each construct the spec names, and
  strips `<script>`, an `onerror` attribute, a `javascript:` href and a remote
  image; `applyFormat` wraps, unwraps and handles an empty selection; the
  store focuses an existing Docs tab rather than opening a second one, and
  marks dirty and clean correctly.
- Component: right-click → Docs opens a tab titled `Docs: <name>`; typing
  `# Overview` and switching to Preview shows an `h1` — the spec's two
  scenarios, as written.

### Out of scope

Docs on **examples** (the tree has them, the spec does not ask for them),
image paste or attachment, docs search across a collection, and any export
format other than OpenAPI.

### Open question

**Where an external link in the preview goes.** Intercepting the click is not
optional, but opening the URL in the user's real browser needs a way to ask
the OS, and this app has no opener today — `tauri-plugin-opener` or the `open`
crate would be a new dependency, which is a decision for Zoran and not one to
take quietly. Until it is settled the preview renders links as styled,
non-navigating text with a copy-link affordance, which is safe and complete on
its own.

---

### Phase 12 shipped — 2026-09-21

Everything in the plan above is implemented and green in the cloud
(`cargo fmt --check`, `clippy --all-targets -D warnings`, 410 + 14 + 59 Rust
tests, `tsc --noEmit`, eslint, 295 vitest — up from 228). Still to run on
Windows: `verify.bat`, then `release.bat`.

**Two corrections to the plan, found by reading the code while implementing:**

1. **"Flat export has nowhere to put folder docs" was wrong.** Grouping is an
   *import* option; the exporter always emits one tag per folder, so folder
   documentation always has a home and the `ExportNote` the plan proposed
   cannot fire. It was not written.
2. **The plan missed the preview's stylesheet.** `renderMarkdown` returns an
   HTML string, and Tailwind's preflight strips headings, list markers and
   table borders back to plain text — so the preview would have rendered as
   undifferentiated prose. Styles live in `src/index.css` under
   `@layer components`, written out rather than pulling in
   `@tailwindcss/typography`: that plugin is a whole prose design system where
   two dozen declarations were needed, and it would have been a third
   dependency for the feature. Every colour is a theme token.

**Two bugs the tests caught, worth recording because both were silent:**

- `applyFormat`'s unwrap computed the wrong end offset, and its toggle
  mistook `**bold**` for italic and stripped one asterisk from each end.
  Both now have the longer-run guard and a test.
- `openDocs` fetching while the user typed: the guard was "is it loaded",
  which the keystroke did not change, so the arriving text overwrote the
  draft. The rule is now that an unloaded tab is read-only — the editor
  enforces it and the store enforces it again — so the stored text wins and
  nothing is lost, because there was nowhere to type. Without this, a
  keystroke landing first would have left a one-character draft that autosave
  then wrote over documentation the user had never seen.

**What is in the tree**

Backend: migration `0009_item_docs.sql`; `persistence/repositories/docs.rs`
(the two statements, with `DocsTable` as the closed enum that keeps the table
name out of reach of any caller); `docs`/`set_docs` on the three existing
repository traits; `domain/services/docs.rs` with `MAX_DOCS_BYTES` at 256 KB;
`commands/docs.rs`; `Operation.description` added to the OpenAPI document;
docs mapped into `info`, tags and operations on export and read back on
import.

Frontend: `lib/markdown.ts` (the one sanitising chokepoint),
`lib/markdown-format.ts`, `lib/docs-title.ts`, `services/docs.ts`,
`types/docs.ts`, `features/docs/` (editor, toolbar, preview), `DocsTab` in the
tabs store with debounced autosave, a Docs entry in three context menus, and
Ctrl/Cmd+Shift+D.

**The open question is still open.** An external link in the preview renders
as styled, non-navigating text and copies its URL on click. Opening it in the
user's real browser still needs an opener dependency, which is Zoran's call.

**New dependencies:** `marked` and `dompurify`, both zero-dependency. A
`pnpm install` is needed before the first build on a machine that has not run
one since — `verify.bat` does it first, so running that covers it.

---

**Verified on Windows 2026-09-21.** `verify.bat` green end to end: 411 + 14 +
59 Rust tests (the 411th is the Windows-only webview test), 295 vitest,
`clippy --all-targets -D warnings` and `cargo fmt --check` both clean, and the
production `vite build` succeeds.

**Lockfile churn, caused by this phase and worth knowing about.** The two new
packages were added with `pnpm add` in the cloud container, which runs
**pnpm 10.28.0**, while this machine runs **12.3.4**. The two resolve a
lockfile differently, so committing the cloud's `pnpm-lock.yaml` moved two
unrelated devDependencies *backwards* within their caret ranges —
`autoprefixer` 10.6.1 to 10.6.0 and `prettier` 3.9.8 to 3.9.6. Both are
dev-only and neither changes a byte of the shipped binary, but it is an
unintended diff.

Fixed on 2026-09-21 with `pnpm up autoprefixer prettier`, which re-resolved
them under pnpm 12; the lockfile now pins `autoprefixer@10.6.1` and
`prettier@3.9.8` again.

**A reading error worth not repeating:** the run after that fix reported
`pnpm install` → "Already up to date", and that was taken as evidence the fix
had *not* been applied. It is the opposite — "Already up to date" is what
`install` says once the lockfile and `package.json` agree, which is precisely
the state `pnpm up` leaves behind. The install line says nothing about which
versions are pinned; only the lockfile does.

**The lesson for later phases: add dependencies on the machine whose pnpm
owns the lockfile.** A cloud `pnpm add` is fine for checking that something
builds, but the lockfile it writes should not be the one that ships while the
two pnpm majors disagree.

**`release.bat` green the same day.** Both bundles built, and the
static-link check reports **29 imported DLLs — unchanged from before this
phase**. That is the number worth recording: `marked` and `dompurify` are pure
JavaScript bundled into the frontend, so neither reaches the linker, and the
single-binary guarantee (CLAUDE.md section 11 rule 2) is untouched by Phase 12.

**Bundle cost of the renderer choice**, now that it is measurable rather than
estimated: the main chunk went 298.80 kB to 388.32 kB raw, 88.91 kB to
117.89 kB gzipped — about **29 kB gzipped** for `marked` plus `dompurify`.
The stylesheet went 23.05 kB to 25.68 kB for the preview's rules. For
comparison, the Monaco chunk this app already ships is 861 kB gzipped.

---

## The installer art still said "Responder" — fixed 2026-09-21

The NSIS header and sidebar bitmaps were regenerated for the rename. What
came out of that showed the rename had left a second, quieter problem behind.

**The bitmaps were stale, and the script was not.** `tools/build-installer-art.py`
was updated on 2026-09-18 to say `ResponderHTTP`; the `.bmp` files it writes
were last generated on 2026-09-14 and never rebuilt, so every installer built
since the rename has carried the old wordmark. Generated files that are also
committed need the generator *run*, not merely edited — the edit is invisible
until someone does.

**Regenerating it revealed the real bug: `fit_text` only ever fitted height.**

```python
def fit_text(text, font_path, target_px):
    """Largest size whose cap height fits target_px."""
```

At 150 px wide, "ResponderHTTP" set to the same cap height as "Responder" is
about fifteen pixels too long, and the header's lockup is right-aligned — so
the overflow pushed the badge clean off the left edge of the canvas. The first
regenerated header had the new name and no icon. The sidebar, centred, did not
clip but sat within three pixels of each edge.

`fit_text` now takes an optional `max_width` and honours both dimensions,
which is what a wordmark in a fixed-size bitmap has always needed. The header
gives the text exactly the room left after its margins, the badge and the gap,
and clamps the badge's position so it cannot leave the canvas whatever the
name. The sidebar reserves a 14 px margin either side.

**And the name now lives in one place.** `NAME = "ResponderHTTP"` at the top,
where it was previously spelled out at each of the four sites that draw it —
which is precisely why the rename could half-apply.

Measured, before and after:

| | before | after |
|---|---|---|
| Sidebar wordmark | 91 px wide, margins 37 / 36 | 134 px wide, margins 15 / 15 |
| Header leftmost badge pixel | x = 23 | x = 10 (0 would mean clipped) |

Both files are still `BM 24-bit BI_RGB` at exactly 150×57 and 164×314, the
same byte count as the ones they replace — NSIS is particular about that and
the format did not change.

Only the three bitmaps that carry text were regenerated. `installer.ico`,
`uninstaller.ico` and the six `tray-*.png` files are the badge alone, with no
wordmark in them, so they had nothing stale to fix and rerunning the whole
script would have churned eight binary files for nothing.

**Verified inside the built installer, not just on disk.** A green
`release.bat` proves only that `makensis` ran — it would have succeeded with
the stale bitmaps too. So the setup.exe was opened as an archive
(`7z x ResponderHTTP_1.0.0_x64-setup.exe '$PLUGINSDIR/*.bmp'`, which reads
NSIS) and the two bitmaps it carries were compared against the two in the
repository:

```
fdf8b2874e3ec82affa752b6589e8357  $PLUGINSDIR/modern-wizard.bmp   <- sidebar.bmp
0d419c66f274b35837b65c0dae01ce3d  $PLUGINSDIR/modern-header.bmp   <- header.bmp
```

Both match byte for byte. (The archive listing shows the header as 31 454
bytes, which is its *compressed* size; extracted it is the expected 25 818.)
Worth remembering as the way to check any installer asset without installing
anything.

---

## Phase 13 — WebSocket requests — **done, verified 2026-09-22 (`verify.bat` and `release.bat` green)**

The full plan, decisions and per-step build notes are in `PLAN-WEBSOCKET.md`. This is the summary.

**What was built.** WebSocket requests (`ws://`, `wss://`) sit beside HTTP requests in any collection or folder.
- The tab has Connect/Disconnect, a Text/JSON/Hex composer, and Docs, Message, Params, Headers and Settings sub-tabs. The Cookies link opens the existing manager.
- The status badge follows the connection state. The activity log runs newest first with millisecond timestamps and expandable rows (pretty-printed JSON, hex dumps, handshake headers), plus search, a direction filter and Clear Messages.
- WebSocket requests save, reopen, rename, move, delete and carry docs exactly as HTTP requests do.
- The OpenAPI export dialog says how many WebSocket requests it will leave out. OpenAPI cannot describe them.
- There is no WebSocket import or export yet (D2); that comes with AsyncAPI.

**Engine.**
- libcurl's own WebSocket API, not a second library (D1), proven by a spike on Windows first (13a). It adds nothing to the binary.
- The API has no Rust bindings, so `http/curl_ws_ffi.rs` declares them by hand. It is the codebase's first and only `unsafe`, which `CLAUDE.md` §4 and §11 rule 10 now fence in.

**Sub-phases, all verified on `verify.bat`:**
- 13a: the spike.
- 13b: domain and transport, with 18 integration tests against a local server.
- 13c: commands and IPC streaming.
- 13d: persistence. Migration `0010` adds `kind` and `ws_json` to the shared `requests` table.
- 13e: frontend state and pure libraries.
- 13f: the UI.
- 13g: the export dialog count.
- 13h: hardening.

**What was found along the way, worth remembering:**
- **libcurl sends a ping's pong only lazily, and never answers a server's close.** Both are handled by the connection's owner thread (13a).
- **The first smoke test lost messages.** Send, Send, Disconnect inside one 25 ms poll closed the connection before the queued messages went out, though both sends had reported success. The owner loop now drains the queue before acting on Disconnect, with a regression test (13c).
- **Messages that arrive between Disconnect and the server's close are now logged** rather than dropped (decided 2026-09-22).
- **A database from a newer version now gets a dialog at startup** (`desktop/startup_error.rs`), where it used to exit silently. This closes the Phase 7 item (13d, decided 2026-09-22).
- **Collapsed log rows were rebuilding their preview from the whole message on every render.** With the log full of 1 MiB messages, one render spent 1.6 s doing it. Previews now read a bounded slice, and rows are memoised: 1.8 ms (13h).
- **Decisions revised after the UI existed:** there is one Clear Messages that clears everything, with no "…" menu and no Clear Response. And a single count in the export dialog replaced one note per request.

**Release gate, 2026-09-22.**
- `verify.bat` green: 481 Rust unit tests, 14 curl, 69 repository, 19 websocket integration, and 377 vitest in 42 files.
- `release.bat` green: NSIS and MSI both built.
- **The static-link check still reports 29 imported DLLs, all Windows system DLLs.** WebSocket support added none, since it lives inside the statically linked libcurl.

**Still open:** an in-app scroll and typing check under a fast message stream. The pure-code measurement in 13h already removed the cost it was guarding against.

---

## Phase 13 extension — WebSocket message formats — **done, verified 2026-09-22**

The composer now offers XML, HTML and Binary, and Binary can be typed as Base64 or Hexadecimal. JSON, XML and HTML get a Beautify button. Text that does not parse is left as typed, with the reason shown.

- Binary still crosses IPC as hex.
- No migration: the draft lives in the `ws_json` blob, and a draft saved as the old "Hex" format opens as Binary in Hexadecimal.
- An expanded binary row in the log opens in the composer's encoding and can be switched per row.

Details are in `PLAN-WEBSOCKET.md`, 13i.

---

## Phase 14 — Server-sent events — **done, verified 2026-09-23 (`verify.bat` and `release.bat` green)**

The full plan, decisions and per-step build notes are in `PLAN-SSE.md`. This is the summary.

**What was built.** An HTTP response that says `Content-Type: text/event-stream` is now shown as it arrives instead of after it ends. Nothing has to be turned on and Send stays one button (D1): it is the response that differs, not the request.
- The response pane switches to an Events view — one row per block, numbered, with its event name, `id` and a millisecond timestamp — plus a live Raw view and the headers. Comments (`: keep-alive`) are shown as comments rather than as empty events.
- The list runs **newest first**, as the WebSocket log does, and follows the newest event unless the user has scrolled down to read the older ones. The status pill appears as soon as the headers land, with a Streaming badge while the stream is open, and the line carries a running event count and byte total.
- The size badge shows headers plus body, and hovering it breaks that into Response (Headers, Body) and Request (Headers, Body). The ordinary response bar, which had no size display at all, now shows the same badge. The request half is libcurl's to report and only exists once the transfer ends, so it reads "—" while a stream is open.
- The byte total is measured in Rust, on each block's raw bytes, and counts what the server sent rather than what the viewer still holds. Measuring it in the UI would have used `String.length`, which counts UTF-16 units and undercounts multi-byte characters.
- The request still finishes normally, so history, saving an example and Copy as cURL are unchanged.

**Engine.**
- `domain/sse.rs` parses; it is pure, has no clock and no I/O, and keeps each block's raw lines so nothing the server sent is lost.
- `HttpClient::send_streaming` has a default that calls `send`, so no existing client, mock or decorator had to change.
- **The total timeout moved out of `CURLOPT_TIMEOUT` into libcurl's progress callback** and stops applying once a response proves to be a stream — otherwise a stream would be cut off at 30 seconds. A request that never answers still times out.
- One `ipc::Channel` per request carries the events, batched to one store update per animation frame and capped at 1 000 events / 32 MiB. Both guards are the WebSocket log's, now shared in `lib/event-batcher.ts` and `lib/capped-list.ts` rather than copied.

**Sub-phases:**
- 14a: the parser, 17 tests.
- 14b: transport, with 8 integration tests against a local server that writes a stream piece by piece.
- 14c: the command, the DTOs and the streaming service.
- 14d: the store and the Events view.
- 14e: the long-stream measurement and these docs.

**What was found along the way, worth remembering:**
- **A sink that stops listening has to be checked before the parser runs, not after.** libcurl finishes the chunk it is on before the progress callback can abort, so the collector reported one block more than the test expected until the dropped flag was checked first (14b).
- **`httpmock` cannot test this.** It answers in one go, which is precisely the behaviour under test, so the integration tests use a few lines of `std::net` instead — the same choice `tests/support/ws_server.rs` made.
- **Nothing new was added to the binary.** SSE is plain HTTP; the parser is ours.

**Release gate, 2026-09-23.**
- `verify.bat` green: 504 Rust unit tests, 14 curl, 69 repository, 9 SSE, 19 WebSocket, and 440 vitest in 47 files, with `cargo fmt`, clippy, `tsc` and eslint clean.
- `release.bat` green: NSIS and MSI both built.
- **The static-link check still reports 29 imported DLLs, all Windows system DLLs.** SSE added none, as expected: it is plain HTTP and the parser is ours.

**Deliberately not in this phase:** automatic reconnect with `Last-Event-ID`, saving a stream as an example, recording every event in history, a dedicated SSE request type in the sidebar, and streaming for `send_and_download`.

---

## Open decisions to confirm before the relevant phase starts

- ~~Phase 5: variable resolution order if more than one tier (environment vs. global) is wanted.~~ Settled 2026-09-14: environments only, no global tier.
- Phase 7: whether auto-update is in scope at all. Still out of scope unless someone asks.
- Phase 9: macOS and Linux credential stores, when those platforms come back.

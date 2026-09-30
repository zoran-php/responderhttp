# PLAN-LINUX.md — Phase 17: Fedora (Linux) build, packaging and Flathub

Written 2026-09-29, from a Fedora 44 WSL2 session. **D2–D5 decided 2026-09-29; D1 and D6 stand as recommended. 17a done except the in-app checks; 17c done and verified; `oo7` 0.6 approved 2026-09-29.**

Companion to `PLAN.md`, `PLAN-WEBSOCKET.md`, `PLAN-SSE.md` and `PLAN-GRPC.md`. The same rules apply: `CLAUDE.md` wins, and nothing here is done until its gate is green. On Linux that gate is a new `verify.sh` (17a), the counterpart of `verify.bat`, which stays the gate for Windows.

---

## 1. What you asked for

> Since we are running inside Fedora 44 in WSL, modify (if needed) the Tauri app to run on Fedora and package the app on Fedora. Later publish the app on the Fedora store.

Windows stays the primary platform, and nothing in this phase may change what the Windows build does. Every Linux change is behind `cfg(target_os = "linux")`, in a platform config file, or in a new script.

---

## 2. Where "the Fedora store" is

Fedora's store is **GNOME Software** (KDE Discover on the KDE spin). It installs from three kinds of source:

| Source | Who builds it | Takes this app? |
|---|---|---|
| Fedora repositories (RPM) and Fedora Flatpaks | Fedora packagers, from Fedora RPMs | **No.** Fedora ships free and open-source software only. |
| Copr (community RPM repositories) | You, on Fedora's build service | **No.** The public Copr only builds FOSS, under the same licensing rules as Fedora. |
| **Flathub** | Flathub's build service, from our source and manifest | **Yes**, with conditions (below). Fedora Workstation offers Flathub in GNOME Software once "third-party repositories" are enabled, which the first-boot setup asks about. |

**Why the first two are out:** `LICENSE` forbids distributing modified versions and repackagings (section 2). That is a reasonable licence, but it is not a free-software licence, and Fedora and Copr accept nothing else. Changing the licence would be the only way in, and it was decided against on 2026-09-23 (PLAN.md Phase 10).

**Flathub's conditions** (docs.flathub.org, "Requirements", read 2026-09-29):

- "All content hosted on Flathub must allow legal redistribution." `LICENSE` 1(b) allows unmodified redistribution without a fee, and the submission comes from you, the licensor, so Flathub distributing your own build is your permission to give. The MetaInfo file must declare the licence as it is (`LicenseRef-proprietary=https://zoran-php.github.io/responderhttp/terms.html`), matching `LICENSE`.
- "All source available submissions must be built entirely from source code", with **no network access during the build**. Every Cargo crate and every npm package has to be listed in the manifest in advance (17e).
- **App ID:** "A dash `-` is only allowed in the last component." Our identifier is `io.github.zoran-php.responderhttp`, whose third component has one. The Flathub ID has to be **`io.github.zoran_php.responderhttp`**. Flathub maps the underscore back to `github.com/zoran-php/responderhttp` to check ownership. See D1.
- "Static permissions must be kept to an absolute minimum", and a portal must be used wherever one covers the need. This shapes secrets, file access and the tray (D3–D5).

**Also worth having:** an `.rpm` attached to the GitHub release, for people who do not use Flatpak. Tauri builds one itself (17d), and it costs nothing extra.

---

## 3. What does not carry over from Windows

### F1. "One file, no system libraries" cannot be literally true on Linux

On Windows the app uses WebView2, which Windows provides. On Linux, Tauri draws with **WebKitGTK 4.1** and **GTK 3**, and those, with glib, libsoup 3 and glibc, are shared libraries of the system (for the RPM) or of the GNOME runtime (for the Flatpak). No Linux desktop app ships them statically.

What *does* still hold, and what `CLAUDE.md` §11 rule 2 actually requires: **libcurl, rustls and SQLite stay inside the binary**. A new `check-linux.sh` (17d) plays the part of `check-windows.ps1`. It reads the binary's `NEEDED` entries and fails on `libcurl`, `libssl`, `libcrypto`, `libsqlite3` or anything else not on a reviewed allowlist.

`CLAUDE.md` §1 ("never assume any system library is present") gains a Linux sentence saying which system libraries are allowed, and why.

**Open detail:** zlib. On Linux `libz-sys` may link the system `libz.so` rather than building its own (the risk PLAN.md Phase 0 wrote down). libz is on every Fedora install and in the GNOME runtime, so it is harmless either way, but the allowlist should record which it is.

### F2. The credential store is Windows-only

`secrets/keychain.rs` has a Windows implementation and, on every other platform, one that reports the store as unavailable. That is safe (nothing is written in plain text), but it means **no secret can be saved on Linux**: the Auth tab's tokens and every secret variable refuse to save. This has to be built before Linux is usable (17c, D3).

Linux has two ways to hold the data key:

- **Secret Service** over D-Bus (`org.freedesktop.secrets`): GNOME Keyring or KWallet. This is what the RPM build would use, and what a Flatpak can use with the static permission `--talk-name=org.freedesktop.secrets`.
- **The Secret portal** (`org.freedesktop.portal.Secret`): the sandbox-friendly way. The portal hands the app a secret, and the app keeps its own encrypted keyring file inside its sandbox. Flathub prefers the portal where it works.

A library that does both, choosing by whether it is sandboxed, keeps it to one code path. `oo7` (GNOME's pure-Rust Secret Service client) is the candidate to check first. It would sit behind the existing `DataKeyStore` port, as the Windows store does. The exact crate is a new dependency and needs your approval (D3), after its API and dependency tree are read.

### F3. Fedora's default desktop has no tray

GNOME, which Fedora Workstation uses, shows no tray icons unless an extension is installed. Fedora does not enable one by default. KDE has a tray. WSLg has none either.

Today, closing the window **hides it to the tray** (PLAN.md Phase 0 and Phase 11). On stock GNOME the window would simply vanish, with nothing on screen to bring it back. Launching the app again would show it, through single-instance, but nobody would guess that. See D4.

### F4. The close-to-tray toast is Windows-only

`desktop/toast.rs` does nothing off Windows, so the notice that explains the tray would never appear. Whether that matters depends on D4.

### F5. File access inside a Flatpak

A Flatpak sees only its own files, plus what the user picks in a file chooser that goes through the **document portal**. Three features read files by path, possibly later and from somewhere else:

- **multipart file parts** keep a path, which libcurl opens at send time (PLAN.md Phase 6);
- **Send and download** and **OpenAPI import and export** read or write one chosen file (fine through the portal);
- **.proto import** reads the chosen files *and the files they import*, from the chosen import folders (PLAN-GRPC.md 16f). A folder picked through the portal grants that folder.

A path from the portal looks like `/run/user/1000/doc/<id>/name`. It keeps working across restarts while the permission stands, but it is not the path the user thinks they chose, and a saved multipart request stores it. See D5.

### F6. The identifier and where data lives

`tauri.conf.json`'s identifier names the app-data folder and the Credential Manager entry on Windows. If Linux needs the Flathub spelling (D1), the change goes in **`src-tauri/tauri.linux.conf.json`**, which Tauri merges over the main config on Linux builds only. Windows keeps `io.github.zoran-php.responderhttp`, and no Windows user's data or key moves.

Where Linux keeps data:

| Build | Database and logs |
|---|---|
| RPM, AppImage, `tauri dev` | `~/.local/share/io.github.zoran-php.responderhttp/` |
| Flatpak | `~/.var/app/io.github.zoran_php.responderhttp/data/io.github.zoran_php.responderhttp/` |

(As revised in D1: only the Flatpak uses the underscore spelling.)

### F7. Smaller things, each one line in a sub-phase

- **The missing-WebView check** (`desktop/webview.rs`) is phrased for WebView2. On Linux WebKitGTK is linked, so a missing library stops the program before `main` runs. The check can stay; its message needs a Linux wording.
- **The About, Privacy and Terms texts** (`desktop/notices.rs`, `store/privacy-policy.md`, `docs/`) say "Windows Credential Manager" and "your Windows user profile". They need a platform-neutral wording, or a Linux sentence.
- **Single instance** uses D-Bus on Linux. In a Flatpak that needs the app to own a bus name under its ID. To confirm in 17e.
- **Monospace fonts:** `index.css` falls back from `Consolas` to `monospace`, which is fine. The UI font needs one look on Fedora.
- **The WebSocket FFI** already has its Unix `poll` branch (`curl_ws_ffi.rs`). The 13a spike's cloud run proved the same calls on Linux, but not this file. `cargo test` here is the first run.
- **The static-link proof** was never run on Linux. The Phase 0 risk list (zlib, glibc floor, CA roots, the appindicator library) is what 17a and 17d check.
- **CA roots:** `CURLSSLOPT_NATIVE_CA` reads the system store. On Fedora that is `/etc/pki/tls`, and the GNOME runtime has its own. An HTTPS request in each build proves it.
- **The tray library** on Linux is `libayatana-appindicator` or `libappindicator-gtk3`. Tauri loads it at run time, so it is not in `NEEDED`, and a missing one only loses the tray.

---

## 4. Decisions — D2–D5 taken 2026-09-29, D1 and D6 as recommended

| # | Question | Recommendation |
|---|---|---|
| **D1** | The Linux identifier | ~~`io.github.zoran_php.responderhttp` on Linux only, through `tauri.linux.conf.json`.~~ **Revised 2026-09-30: split by package.** The Tauri CLI's bundler refuses an underscore in the identifier (17d), so the RPM, the AppImage and Windows keep `io.github.zoran-php.responderhttp`, and only the Flatpak, which Flathub requires to use the underscore spelling, is compiled with `TAURI_CONFIG='{"identifier":"io.github.zoran_php.responderhttp"}' cargo build` (17e). The data folder, the keyring item and the single-instance bus name all follow the identifier the binary was built with. |
| **D2** | What to ship | ~~Flathub plus an `.rpm`~~ **Decided: Flathub, plus an `.rpm` and an AppImage on the GitHub release.** No `.deb`. The AppImage is for other distributions; it bundles GTK and WebKitGTK, so it is the one Linux artefact whose contents 17d must inspect beyond `NEEDED`. |
| **D3** | Where the data key lives on Linux | **Decided as recommended.** One library that uses the Secret portal inside a Flatpak and Secret Service outside it (candidate `oo7`, to be checked in 17c). No plain-text fallback, ever: without a store, secrets refuse to save, as on Windows today. |
| **D4** | Closing the window on Linux | **Decided as recommended.** Hide to the tray only when a tray host is present; otherwise closing quits, first asking when a request, stream, WebSocket or gRPC call is live. On stock GNOME that gives normal app behaviour, and on KDE it matches Windows. |
| **D5** | File access in the Flatpak | **Decided as recommended.** Portals only, no `--filesystem=home`. A saved multipart path from the portal keeps working while its permission stands. The request tells the user if it cannot open the file, as it already does for a moved file. |
| **D6** | Where Linux is tested | **Build and run the tests in this WSL session. Before submitting to Flathub, click through on a real Fedora 44 Workstation** (a VM is enough), because WSLg has no tray, no keyring and not all the portals. |

### Assumptions (confirm or correct)

1. **x86_64 only.** Flathub also builds aarch64 by default. An `only-arches` line keeps to x86_64 until aarch64 has been tried.
2. **No auto-update.** Flathub and the RPM repository handle updates, as the Store does on Windows.
3. **One version number on every platform.** Linux starts at the current 1.1.0.
4. **The Microsoft Store listing text is not touched,** but the README and the website gain a Linux section.

---

## 5. Sub-phases

### 17a — Build and test as-is on Fedora

- Install what is missing: `cmake`, and whatever the build asks for (recorded here with the reason).
- `pnpm install`, `pnpm run build`, `cargo build`, then `pnpm tauri dev` shown through WSLg.
- **`verify.sh`**: a line-for-line port of `verify.bat` (install, build, lint, vitest, `cargo fmt --check`, clippy, `cargo test`), writing `verify-log.txt` in the same `exit=N` format.
- Fix only what fails. Every fix is behind `cfg`, or proven neutral on Windows by `verify.bat`.

**Done when** `verify.sh` is green here and an HTTPS request, a WebSocket echo and an SSE stream work in the running app.

#### 17a first run — 2026-09-29, before any decision

On Fedora 44 in WSL2: Rust 1.98.1 (Fedora's package, not rustup), Node 26, pnpm 12.6. WebKitGTK 4.1, GTK 3, libappindicator and OpenSSL headers were already installed, and nothing else had to be installed for a debug build. **`cmake` and `nasm` were not needed:** aws-lc-sys and curl-sys both built without them on Linux.

- **Frontend:** `pnpm install --frozen-lockfile` and `pnpm run build` clean. eslint clean. **vitest 556 of 556**, the same count as Windows.
- **One compile error, a real portability bug:** `curl_grpc.rs::failure_text` (the 16j hint) compared `error.code()` with an `i32` constant. `CURLcode` is `i32` on MSVC and `u32` on Linux, so it only compiled on Windows, and so did its three tests. Now a `u8` constant, with both sides compared as `i64`. Windows behaviour is unchanged.
- **One clippy error, from the newer toolchain rather than from Linux:** clippy 1.98 adds `chunks_exact_to_as_chunks`. `commands/dto.rs::decode_hex` now uses `as_chunks::<2>()`, which is stable since 1.88, so Windows' 1.97 accepts it too. Worth a decision: pin the toolchain (`rust-toolchain.toml`, which needs rustup here) or keep the code clean on both versions, as done here.
- `cargo fmt --check` clean.
- **cargo test:** 635 library tests (Windows counts 636; the difference is the Windows-only webview test), 14 curl, 14 gRPC, 10 saved-gRPC-request, 9 schema-repository, 69 repository (1 ignored), 9 SSE, 19 WebSocket (1 ignored). **The WebSocket suite is the first run of `curl_ws_ffi.rs`'s Unix `poll` branch.**
- **One test failed, and it was the test server, not the app.** `server_streaming_messages_arrive_as_they_are_sent` failed every time on Linux, with gaps of `[58, 101, 100, 101] ms` against a 70 ms floor. Only the first gap was short: message 0 arrived ~42 ms late. That is Nagle on the test server's socket (HEADERS, then a small DATA frame held until the client ACKs) meeting Linux's ~40 ms delayed ACK. libcurl sets `TCP_NODELAY` on the client socket, and real gRPC servers set it on theirs, so the app is unaffected. **Fixed in `tests/support/grpc_server.rs`** with `set_nodelay(true)` on each accepted socket, after which the test passed three runs in three.
- **The gRPC round trip is 127 µs here** (median, p95 143 µs), against ~16 ms on Windows. On Linux `multi.wait` wakes when data arrives, which confirms that the 16 ms floor in CLAUDE.md §4 is Windows-only.
- **`verify.sh` written and green end to end**, the same steps and log shape as `verify.bat`. `verify-log.txt` is already ignored by `*log.txt*`.
- **`pnpm tauri dev` runs through WSLg.** The log says `webview: 2.54.0` (WebKitGTK). The database and logs are in `~/.local/share/io.github.zoran-php.responderhttp/`, beside WebKit's own `CacheStorage`, `hsts-storage.sqlite` and `WebKitCache`. As expected (F2), `secrets: no usable data key this session`, and saving a secret variable was refused with `secretStore`. Two `Gtk-CRITICAL gtk_widget_get_scale_factor` lines appear at start-up, probably from the tray's scale lookup (`desktop/tray.rs`) before the window is realised. Harmless, but 17b looks at it. The MESA/dzn lines are WSLg's GPU layer.
- **Still to do for 17a:** an HTTPS request, a WebSocket echo and an SSE stream in the running app, which confirms the CA store on Fedora.
- **`NEEDED` of the debug binary:** the GTK/WebKit/glib family, `libsoup-3.0`, `libjavascriptcoregtk-4.1`, **`libz.so.1`** (the system zlib: the Phase 0 risk, confirmed), **`libdbus-1.so.3`** (D-Bus, for single instance), `libgcc_s`, `libm`, `libc`, `ld-linux`. **No `libcurl`, `libssl`, `libcrypto` or `libsqlite3`.** Rule 2 holds.

### 17b — Platform behaviour

- `tauri.linux.conf.json`: the identifier (D1), `bundle.linux.rpm` settings and the Linux targets (D2).
- Close behaviour (D4): detect a StatusNotifier host, and hide or quit.
- A Linux wording for the missing-WebView message, and platform-neutral notices.

**Split on 2026-09-30, at your request:** the texts and the GTK warnings first (17b-1), closing the window later (17b-2).

#### 17b-1 as built — 2026-09-30, `verify.sh` green

**The GTK warnings came from the tray, not our code.** Traced with `G_DEBUG=fatal-criticals` under gdb: `gtk_widget_get_scale_factor ← gtk_status_icon_update_image ← gtk_status_icon_set_from_file ← libappindicator3 fallback_timer_expire`. When no StatusNotifier host owns its name on the session bus (WSLg, stock Fedora GNOME), libappindicator falls back to GTK's old `GtkStatusIcon`, which has no tray to draw in either. The icon never appears, and GTK complains twice.

- **`desktop/tray.rs`:** `build_tray` now asks the session bus first whether `org.kde.StatusNotifierWatcher` or `org.freedesktop.StatusNotifierWatcher` has an owner. The first is what KDE and GNOME's AppIndicator extension own; the second is the newer spelling. With no host it logs `tray: this desktop has no tray host; running without a tray icon` and makes no icon. A bus that cannot be asked is logged and counts as no host. Windows always has a tray, so it is unchanged. **The same check is what 17b-2 builds on.** Until then, closing the window still hides it, as before: with no tray host that already lost the window, and 17b-2 fixes it.
- **`Cargo.toml`:** `zbus 5` (`blocking-api`) named as a Linux dependency. It was already linked at this version (lockfile: one line, zbus under `responderhttp`'s own dependencies).
- **Checked:** with `G_DEBUG=fatal-criticals` the app now runs to the timeout without a critical, and logs the tray line. An `#[ignore]`d unit test claims the StatusNotifier name on a private bus: no host, then a host while the name is owned, then no host once it is released. It **passed**.
- **`tools/test-linux-keyring.sh` renamed `tools/test-linux-session.sh`**, since it now runs both session-bus tests (the keyring and the tray).
- **Flatpak note for 17e:** the sandbox's D-Bus proxy only shows names the app may talk to, so the manifest needs `--talk-name=org.kde.StatusNotifierWatcher` for the tray to be found. That is the usual permission for a tray app on Flathub.

**The texts:**

- **`desktop/notices.rs`:** a `Platform` enum (`Windows`, `Linux`; `Platform::CURRENT` for the build, and macOS reads Linux until it has its own). `privacy_text(platform)` and `terms_text(platform)` replace the two constants. They share every paragraph and differ only where the facts do:
  - Privacy, storage: "…in your home folder. Uninstalling the app leaves it there; delete the app's data folder to remove it." Neither the RPM nor a Flatpak removes home-folder data on uninstall, so the Windows sentence "Uninstalling the app removes it" would be false.
  - Privacy, credentials: "…a key held in your desktop's keyring (GNOME Keyring or KWallet, reached through the secret portal when the app runs as a Flatpak), which only your user account can read."
  - Terms: "Pass the package on unchanged." and "WebKitGTK, GTK and the other system libraries that draw the window remain under their own licences." in place of the installer and WebView2 sentences.
  - **The Windows texts are byte-identical to before**, checked against the committed constants with a temporary dump. They condense the published policy and terms, so they must not drift.
  - About: "so nothing else needs to be installed" became "so curl does not need to be installed" on both platforms. It was not strictly true on either (WebView2, WebKitGTK), and it is the claim the sentence is about.
  - Every platform's text is built and tested on every build: no links, under 2,000 characters, the core promises, every placeholder filled, and **each platform names only its own system** (no "Windows", "WebView2" or "Credential Manager" in the Linux texts, and no "GNOME", "KWallet", "Flatpak" or "WebKitGTK" in the Windows ones). Linux Privacy is 1,563 characters and Terms 1,443.
- **`desktop/menu.rs`:** calls the two functions with `Platform::CURRENT`.
- **`desktop/webview.rs`:** a Linux arm. "ResponderHTTP draws its window with WebKitGTK, and it could not be started…", then update the Flatpak and its runtime, or `sudo dnf reinstall webkit2gtk4.1`. The module header explains that a *missing* WebKitGTK stops the loader before `main`, so the dialog covers one that loads but cannot start. One Linux-only test.
- **`verify.sh`: green.** vitest 556, **642 library tests** (637 + 3 notices + the tray name test + the webview test; the tray test is ignored), and every integration suite. **`verify.bat` on Windows, 2026-09-30: green, 641 library tests as predicted** (the tray-name and webview tests are Linux-only), every integration suite unchanged. The notices tests ran there too: `the_build_reads_its_own_platform` confirms Windows picks its own wording, and `each_platform_names_only_its_own_system` checked the Linux texts from a Windows build.
- **Still to change with 17f:** `store/privacy-policy.md`, `docs/privacy-policy.html` and `docs/terms.html`, the published full versions, still describe Windows only. The dialogs now say more for Linux than the published policy does; the two must agree before a Linux release.

#### 17b-2 as built — 2026-09-30, `verify.sh` green

**Closing the window on Linux (D4).** With a tray, closing hides the window, as on Windows. With no tray host, closing quits, but asks first when quitting would lose something.

**Why the frontend decides.** What quitting loses is unsaved tabs (tabs live only in memory) and running work, and only the tab store knows either. So Rust does not close the window itself: it asks, and the frontend answers. The rule is the one a single tab already uses when it is closed (`App.tsx`), applied to every tab at once, plus a request that is still sending or streaming.

- **Rust**
  - `desktop/tray.rs`: `build_tray` returns whether a tray was made (17b-1's check). `lib.rs` keeps the answer as `window::CloseToTray`.
  - `desktop/window.rs`: the close handler hides to the tray when `CloseToTray` is true (always on Windows), or when setup has not run yet. Otherwise it emits `QUIT_REQUESTED_EVENT` (`"quit-requested"`) and leaves the window open. If the emit fails, nobody can answer, and it quits: a close button that does nothing is worse than one that quits.
  - `commands/app.rs` (new): `quit_app`, which logs `quitting from the window's close button` and calls `AppHandle::exit`, the same exit as the tray and the File menu. `EXIT_CODE_SUCCESS` now lives once, in `window.rs`, where the tray and the menu each had their own copy.
- **Frontend**
  - `lib/quit-guard.ts` (new, pure): `quitSummary` counts unsaved tabs (never a Docs tab, which autosaves and is flushed on quit), sending requests, open WebSockets and running gRPC calls. `quitWarning` turns that into one sentence, or `null`: "Quitting ResponderHTTP loses the unsaved changes in 2 tabs, stops 1 running request, disconnects 3 WebSocket connections and cancels 1 gRPC call."
  - `services/app-lifecycle.ts` (new): `onQuitRequested` (the one `listen` for that event) and `quitApp` (the `quit_app` command).
  - `store/request-store.ts`: `quitSummary()`, built from each tab's kind, `isDirty` and running state, and `saveAllDocs()`, which writes every pending Docs autosave before the app ends.
  - `App.tsx`: listens for the event and reads the store when it arrives, not when the listener is made. With nothing to lose it quits at once; otherwise it shows the existing `ConfirmDialog` ("Quit ResponderHTTP", Cancel / Quit). `quitNow` flushes Docs, then calls `quitApp`.
- **Checked in the real app** (WSLg, which has no tray host, so it is the no-tray path), in a private D-Bus session. The close request was a `WM_DELETE_WINDOW` sent by a throwaway X11 tool, which is what a window manager's close button sends. WSLg's window manager publishes no window list, so `wmctrl` could not find the window.
  - All tabs clean: exit code 0, 107 ms after the close request, with `quitting from the window's close button` in the log.
  - A URL typed into the blank tab, then close: the app stayed open and showed "Quitting ResponderHTTP loses the unsaved changes in 1 tab." (screenshot checked). Typing had to go through XTest; `xdotool type --window` sends synthetic events, which GTK ignores.
  - Cancel: the dialog closes, and the app and the unsaved URL stay (screenshot checked). Close again, then Quit: exit code 0.
- **Tests:** `quit-guard.test.ts` (6), `app-lifecycle.test.ts` (3), and three store tests: an open connection counted until it closes, `saveAllDocs` writing every pending Docs tab at once, and a Docs tab never counted as unsaved.
- **`verify.sh`: green.** vitest **568** (556 + 12), 642 library tests (unchanged: the close handler needs a real window, so the end-to-end check above is its test), and every integration suite.
- **`verify.bat` on Windows, 2026-09-30: green, as predicted:** vitest 568 in 60 files, 641 library tests, and every integration suite. On Windows `CloseToTray` is always true, so closing hides to the tray exactly as before.
- **Not covered:** a webview that has hung. The close request then gets no answer and the window stays open, so the user has to end the process. No fallback was added: nothing so far calls for one, and a timer that quits under a slow but working page would be worse.
- **For 17f:** `CLAUDE.md` §3 describes `desktop/window.rs` as "show/focus main window, close-to-tray". It should also say "or quit, asking first, when there is no tray", and list `commands/app.rs`.

### 17c — The Linux credential store (D3)

**Candidate checked 2026-09-29: `oo7` 0.6.0** (MIT, `github.com/linux-credentials/oo7`, about 1.5 M recent downloads; 0.7 is in beta, so 0.6 is the line to pin).

- **What it does:** on the host it talks to Secret Service (GNOME Keyring, KWallet) over D-Bus. Inside a Flatpak it keeps an encrypted keyring file in the sandbox, whose key comes from the Secret portal. One API for both: `Keyring::new()`, `create_item(label, attributes, secret, replace)`, `search_items(attributes)`, `item.secret()`. It is async-only, which is fine: Tauri already runs tokio.
- **Features:** `default-features = false, features = ["tokio", "native_crypto"]`. `openssl_crypto` is ruled out by §11 rule 2.
- **Measured in a scratch crate against our `Cargo.lock`:** it reuses **zbus 5.19, zvariant 5.15 and tokio 1.53, the exact versions already linked** (zbus comes with `tauri-plugin-single-instance`), so it brings no second D-Bus stack and no `libdbus` of its own. It adds about 22 crates, all pure Rust: `ashpd` (portals) and RustCrypto (`aes`, `cbc`, `cipher`, `hkdf`, `hmac`, `md-5`, `pbkdf2`, `num-bigint`/`num-bigint-dig` for the Secret Service's session key exchange, `rand`, `zeroize_derive` and similar). Linux-only, behind `[target.'cfg(target_os = "linux")'.dependencies]`, so the Windows binary does not change.
- ~~Needs approval before it is added.~~ **Approved 2026-09-29.**

#### 17c as built — 2026-09-29, `verify.sh` green

- **`tauri.linux.conf.json`** (new) sets the identifier to `io.github.zoran_php.responderhttp` on Linux (D1) and the bundle targets to `rpm` and `appimage` (D2). Tauri merges it over `tauri.conf.json` on Linux only. **Tauri accepts the underscore**, though its schema's description lists only letters, digits, hyphens and periods: nothing in tauri-build, tauri-utils or tauri-codegen enforces it, and the running app used `~/.local/share/io.github.zoran_php.responderhttp/`. The bundler (17d) is the last place that could object.
- **`Cargo.toml`**: `oo7 0.6` under `[target.'cfg(target_os = "linux")'.dependencies]`, features `tokio` and `native_crypto`. The Windows dependency comment was corrected to say Linux now has a store.
- **`secrets/keychain.rs`**: a Linux `KeychainDataKeyStore`. `new()` opens `oo7::Keyring` (the portal file backend in a sandbox, Secret Service otherwise). `load` unlocks the collection, which may show the desktop's own prompt, then searches for `LINUX_ATTRIBUTES` (`service` = the Linux identifier, `username` = `data-key`, the libsecret convention, so `secret-tool lookup service io.github.zoran_php.responderhttp username data-key` finds it). None found is `Ok(None)`, the only case that creates a key. More than one is an error, like `Ambiguous` on Windows. `store` replaces the item. The fallback for other platforms is now `cfg(not(any(windows, target_os = "linux")))`.
  - **The sync-to-async bridge:** `DataKeyStore` is synchronous and oo7 is async. Each call runs on a scoped helper thread through `tauri::async_runtime::block_on`, which gives oo7's zbus connection the tokio runtime it needs and keeps its background tasks running. It works whether or not the caller is already inside a runtime, where a direct `block_on` would panic.
  - **A risk checked rather than assumed:** enabling zbus's `tokio` feature applies to the whole build, and `tauri-plugin-single-instance` uses zbus's *blocking* API. zbus 5.19's `utils::block_on` then runs on its own multi-threaded tokio runtime, whose workers keep the connection's tasks alive, so the plugin is unaffected. Confirmed with the real binary: a second launch exited at once, and the first owned `io.github.zoran_php.responderhttp.SingleInstance`.
- **Tests:** two new unit tests (the Linux service matches `tauri.linux.conf.json`, and it is a valid Flathub ID). **`tests/linux_keyring.rs`**, one `#[ignore]`d test: the first store finds the keyring empty and creates a key, a second store (the next start) reads it back, and a secret sealed under the first opens under the second. **`tools/test-linux-session.sh`** runs it inside `dbus-run-session` with a temporary `XDG_DATA_HOME` and a throwaway GNOME Keyring, so neither the developer's keyring nor app data is touched. **Passed.** Installed in WSL for it: `gnome-keyring`, `dbus-daemon`.
- **The real app, in the same kind of private session:** `secrets: no data key found, created one`.
- **`verify.sh`: green.** vitest 556, **637 library tests** (635 + 2), 14 curl, 14 gRPC, 10, 9, 69 repository, 9 SSE, 19 WebSocket; `linux_keyring` 1 ignored. fmt and clippy clean.
- **`verify.bat` on Windows, 2026-09-30: green.** vitest 556, **638 library tests** (636 + the two identifier tests, which read config files and so run on Windows too), 14 curl, 14 gRPC (the bidirectional test still inside its 25 ms bound with the `TCP_NODELAY` server), 10, 9, 69 repository, 9 SSE, 19 WebSocket; `linux_keyring` compiles to no tests there. fmt and clippy 1.97 clean, so `as_chunks` and the `i64` comparison hold on both toolchains. Every shared-code change in 17a and 17c is now verified on both platforms.
- **Not yet seen:** the Secret portal path inside a Flatpak (17e), and the unlock prompt of a locked keyring on a real GNOME desktop (D6).
- **Superseded in 17d:** the identifier override in `tauri.linux.conf.json` and the two tests tied to it are gone (D1 as revised). The keyring item is now named after the identifier the app runs under, passed in from `lib.rs`.

- A spike first: the chosen library against GNOME Keyring in WSL (`dnf install gnome-keyring`, unlocked in the session) and against the portal in a Flatpak.
- `secrets/keychain.rs` gains a `cfg(target_os = "linux")` implementation of `DataKeyStore`, with the same rule that only "no entry" creates a key.
- Tests: the in-memory tests stay as they are. One `#[ignore]`d test against the real store, as on Windows.

### 17d — RPM and the release gate

- `release.sh`: `verify.sh`, then `pnpm tauri build` (the targets come from `tauri.linux.conf.json`), then `check-linux.sh` on the binary.
- The AppImage carries its own GTK and WebKitGTK, so its check is different: list what it bundles, and run it in a clean container with none of the `-devel` packages installed.
- `check-linux.sh`: the `NEEDED` allowlist from F1, each entry with its reason, as in `check-windows.ps1`.
- Install the `.rpm` in a clean Fedora 44 container (`podman`), confirm that `dnf` pulls in WebKitGTK and nothing unexpected, and send a request.

#### 17d as built (RPM) — 2026-09-30, `release.sh` green

The AppImage is the next step, and is not covered here.

- **The identifier (D1 revised).** `pnpm tauri build` stopped before bundling: the Tauri CLI validates the identifier and refuses the underscore, though `cargo build` and tauri-build accept it (17c). The override came out of `tauri.linux.conf.json`, so the RPM, the AppImage and Windows share `io.github.zoran-php.responderhttp`. The Flatpak will be compiled with `TAURI_CONFIG='{"identifier":"io.github.zoran_php.responderhttp"}' cargo build`, which Flathub's own build runs anyway. **Checked with a build in a separate target folder:** that binary used `~/.local/share/io.github.zoran_php.responderhttp/` and owned `io.github.zoran_php.responderhttp.SingleInstance`.
- **`secrets/keychain.rs`:** `KeychainDataKeyStore::new(app_identifier)` takes the identifier from `app.config().identifier`, so each build keeps its key beside its own database. `linux_attributes(identifier)` replaces `LINUX_ATTRIBUTES`. Windows passes the same identifier as before, so the Credential Manager target and the uninstall hook are unchanged. Tests: the two that tied the Linux service to `tauri.linux.conf.json` became one (`the_linux_item_is_named_after_the_running_identifier`), so there is **one library test fewer**. `tests/linux_keyring.rs` passes `KEYCHAIN_SERVICE`, and still passes in `tools/test-linux-session.sh`.
- **`tauri.linux.conf.json`:** targets `rpm` (the AppImage is added back in its own step), licence `LicenseRef-Proprietary`, homepage, a long description, and the LICENSE installed as `/usr/share/licenses/responder-http/LICENSE`.
- **The RPM:** `ResponderHTTP-1.1.0-1.x86_64.rpm`, 7.6 MB. Package `responder-http`. It contains `/usr/bin/responderhttp` (15.6 MB), `ResponderHTTP.desktop` (passes `desktop-file-validate`), the icons and the licence. **Requires only `libwebkit2gtk-4.1.so.0`, `libgtk-3.so.0` and `libappindicator3.so.1`.** rpmlint's remaining findings are accepted: the spelling check on "gRPC" and "keyring", the proprietary licence tag, the missing changelog and build-host tags and man page (Tauri's bundler cannot set them), and the file name, which differs from the package name.
- **`spikes/static-link-proof/check-linux.sh`** (new): the `NEEDED` allowlist from F1, each entry with its reason. libcurl, OpenSSL, SQLite, nghttp2, rustls, ssh, brotli and zstd fail by name, whatever the allowlist says. **The release binary names 15 libraries, all allowed:** libc, libm, libgcc_s, WebKitGTK 4.1, JavaScriptCore, libsoup 3, GTK 3, GDK, gdk-pixbuf, cairo, gio, gobject, glib, libdbus and zlib. A negative test (a binary linked against libcurl and libssl) failed, naming both.
- **`release.sh`** (new): the Linux twin of `release.bat`. It runs `verify.sh` and stops on any failing step, then `pnpm tauri build`, the link check, and each RPM's requirements, all into `release-log.txt`. **Green:** vitest 568, **641 library tests**, and every integration suite.
- **`tools/test-rpm-install.sh`** (new): installs the RPM into a clean `registry.fedoraproject.org/fedora:44` container through `dnf`, as a user would, then fails on any `ldd` "not found" and checks the desktop file and the licence. **Passed:** `dnf` resolved 343 packages, all from Fedora (a bare container has no desktop; a Workstation already has nearly all of them), and the loader found every library.
- **`--gui`** starts the installed app from inside that container on the WSLg display. **It drew, and a GET to `https://fedoraproject.org/` returned 200 in 688 ms** with TLS verified: `Loaded 121 CA root certificates from the system`, from the container's own `/etc/pki`. The container has no session bus, and the app handled that as designed: `secrets: no usable data key this session` (secrets refuse to save, no plain-text fallback), and `this desktop has no tray host; running without a tray icon`.
- **Expected from `verify.bat`:** vitest 568, **640 library tests** (641 − 2 + 1). Nothing else shared changed.
- **Not yet seen:** the RPM on a real Fedora Workstation, with a keyring and GNOME Shell (D6).

### 17e — Flatpak

- `flatpak/io.github.zoran_php.responderhttp.yml`: runtime `org.gnome.Platform` (which carries WebKitGTK 4.1), SDK extensions for Rust and Node, `--share=network`, the Wayland and X11 sockets, and no filesystem access (D5).
- Offline sources: `cargo-sources.json` from `flatpak-cargo-generator`, and the npm packages from `flatpak-node-generator`. **Risk:** that tool's support for pnpm's lockfile has to be checked. If it does not read `pnpm-lock.yaml`, the fallback is to build the frontend from an npm lockfile generated for the Flatpak.
- `io.github.zoran_php.responderhttp.metainfo.xml` (licence, description, screenshots, releases, OARS rating) and a `.desktop` file, both validated with `appstreamcli` and `desktop-file-validate`.
- Icons from `app-icon.png`, drawn by the existing art tooling.
- `flatpak-builder` locally, run it, and repeat the 17a checks inside the sandbox.

### 17f — Docs and submission

- `CLAUDE.md` (§1, §3 new files, §4 Linux engine notes, §9 the Linux commands, §10 and §11 which system libraries are allowed), the README, `store/privacy-policy.md` and `docs/`, and a `store/flathub.md` like `store/listing.md`.
- Screenshots taken on real GNOME (D6).
- **You:** a pull request to `flathub/flathub` with the manifest. Once merged, Flathub creates the app repository, and you verify the app ID against your GitHub account so it shows as verified.

**Sequencing:** 17a → 17b → 17c → 17d → 17e → 17f. 17c's spike can run beside 17b.

---

## 6. Hard-rule check

| Rule | How it holds |
|---|---|
| 1. No system `curl` | Unchanged. libcurl stays in-process. |
| 2. No dynamic libcurl or OpenSSL | `check-linux.sh` fails on both. WebKitGTK and GTK are allowed and documented (F1). |
| 3–5 | Untouched: no frontend `invoke` change, no SQL change, no migration. |
| 7. TLS verification on by default | Unchanged. |
| 10. `unsafe` only in `curl_ws_ffi.rs` | Its Unix branch already exists. The secret store is expected to need no `unsafe`, and 17c checks that. |
| §7 no speculative traits | The Linux store implements the existing `DataKeyStore` port. No new trait. |

# Flathub — ResponderHTTP

Notes for putting ResponderHTTP on Flathub, the store GNOME Software and KDE
Discover install from on Fedora and most other distributions. Nothing here is
read by the build. PLAN-LINUX.md §2 has why Flathub is the only store route
(Fedora's repositories and Copr take free software only), and 17e has the
technical findings behind everything below.

---

## Flathub's policy on AI-generated material

Flathub's requirements (docs.flathub.org, "Requirements", Generative AI policy,
as of 28 September 2026) decide how this submission can be made:

- **Flathub manifests must not contain AI-generated or AI-assisted content.**
  Disclosing it does not make it acceptable.
- **AI-generated code, documentation, packaging or other material in the app or
  its Flathub packaging must be disclosed**, naming the parts and their rough
  extent. Reviewers decide case by case and may reject on it.
- **AI tools must not open or automate the submission pull request**, nor write
  its commit messages, description, review comments or replies.
- AI used only for research, discussion or debugging needs no disclosure, as
  long as no generated material ends up in the app or its packaging.

What follows from that for this repository:

- `flatpak/io.github.zoran_php.responderhttp.yml` was written with an AI
  assistant. It builds the **self-hosted bundle** (below) and must not be
  submitted to Flathub, in whole or in part. The Flathub manifest has to be
  written by hand.
- `flatpak/cargo-sources.json` and `node-sources.json` are produced by
  Flathub's own generators (`flatpak-builder-tools`) from the lockfiles, not by
  an AI. Regenerate them yourself with `tools/flatpak-sources.sh`, or run the
  generators directly, for the Flathub repository.
- The application itself, and `flatpak/*.desktop` and `*.metainfo.xml`, which
  ship inside it, were largely written with an AI assistant. That is what the
  disclosure has to cover.
- The pull request, its description, commits and every reply to reviewers are
  yours to write.

## Until then: the self-hosted Flatpak

`tools/build-flatpak.sh` (run by `release.sh`) writes
`src-tauri/target/flatpak/bundle/ResponderHTTP-<version>.flatpak`. Attach it to
the GitHub release. It installs with

```
flatpak install --user ResponderHTTP-1.1.0.flatpak
```

and fetches the GNOME runtime from Flathub by itself. A bundle does not update
itself: a new version is a new file, as with the AppImage.

## What the Flathub submission has to satisfy

These are Flathub's rules and facts established while building the bundle;
they are reference, not text to paste.

- **App ID:** `io.github.zoran_php.responderhttp`. Flathub allows a dash only
  in the last component, and maps `zoran_php` back to
  `github.com/zoran-php` for verification. The Tauri CLI refuses the
  underscore, so the build calls `cargo` directly with
  `TAURI_CONFIG='{"identifier":"io.github.zoran_php.responderhttp"}'` and
  `--features tauri/custom-protocol`, after building the frontend.
- **Offline build:** no network during the build. Every crate and npm package
  must be a listed source; that is what the two generated JSON files are.
- **pnpm 12 offline** needs three things the generators do not provide: its
  native binary (`@pnpm/exe.linux-x64`, placed as `pnpm-native` next to the
  launcher), `--trust-lockfile` (its supply-chain check needs registry
  metadata), and the frontend tools run by path, because the generated store
  records no `bin` entries.
- **The tray library** (`libayatana-appindicator`) is not in the GNOME runtime
  and must be built as a module; Flathub's `shared-modules` repository has it,
  used as a git submodule.
- **Permissions used by the bundle, and why:** `--share=network` (it is an API
  client), `--share=ipc`, `--socket=wayland`, `--socket=fallback-x11`,
  `--device=dri` (WebKitGTK rendering), `--talk-name=org.kde.StatusNotifierWatcher`
  (the tray). No filesystem access: files are chosen through the portal.
- **Source:** Flathub needs a `git` source at a tag and commit of the public
  repository. A `dir` source is refused by the `--sandbox` build Flathub runs.
- **Licence:** the MetaInfo declares
  `LicenseRef-proprietary=https://zoran-php.github.io/responderhttp/terms.html`.
  `LICENSE` allows unmodified redistribution, and a submission by the author
  grants Flathub the right to distribute it.
- **Architectures:** x86_64 only until aarch64 has been tried; a
  `flathub.json` with `"only-arches": ["x86_64"]` says so
  (`flatpak/flathub.json`).
- **Screenshots** must be reachable over HTTPS when Flathub builds; the one in
  the MetaInfo is `docs/screenshots/linux-http.png` on GitHub Pages. Real
  GNOME screenshots are better (D6).
- **Local check before submitting:** Flathub's own builder and linter,
  `org.flatpak.Builder` (`flathub-build` and `flatpak-builder-lint`), as
  docs.flathub.org's submission guide describes.

## The process

From docs.flathub.org, "Submission":

1. Fork `flathub/flathub` with all branches, and branch from `new-pr`.
2. Add the manifest and its required files, commit and push.
3. Open a pull request against the `new-pr` base branch, titled
   "Add io.github.zoran_php.responderhttp", following the template, with the
   AI disclosure.
4. Answer the reviewers; a test build is started by commenting `bot, build`.
5. Once merged, Flathub creates `flathub/io.github.zoran_php.responderhttp` and
   invites you to it. Log in on flathub.org, open the Developer Portal, and
   verify the app with the GitHub account that owns `zoran-php/responderhttp`.

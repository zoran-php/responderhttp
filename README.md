# ResponderHTTP

An API client for Windows that ships as **one executable with nothing to install
alongside it**. Send HTTP requests, open WebSocket connections, watch a
server-sent event stream arrive live — and keep every collection, environment
and cookie on your own machine.

curl is not a dependency. libcurl is compiled into the binary, so there is no
curl to install, no runtime to add, and no system SSL library to keep patched.

- **No accounts, no telemetry, no analytics, no crash reporting.** The app has
  no server to talk to. See [`docs/privacy-policy.md`](docs/privacy-policy.md).
- **One file.** A release build imports only Windows system DLLs — that is
  checked on every release.
- **Your data stays put.** A single SQLite database in your user profile.

---

## Contents

- [ResponderHTTP](#responderhttp)
  - [Contents](#contents)
  - [Install](#install)
  - [Your first request](#your-first-request)
  - [The window](#the-window)
  - [Building a request](#building-a-request)
  - [Sending](#sending)
  - [Reading the response](#reading-the-response)
  - [Server-sent events](#server-sent-events)
  - [WebSocket](#websocket)
  - [Collections and folders](#collections-and-folders)
  - [Environments and variables](#environments-and-variables)
  - [History](#history)
  - [Documentation](#documentation)
  - [Cookies](#cookies)
  - [OpenAPI](#openapi)
  - [Per-request settings](#per-request-settings)
  - [Keyboard shortcuts](#keyboard-shortcuts)
  - [The tray](#the-tray)
  - [Where your data lives](#where-your-data-lives)
  - [Troubleshooting](#troubleshooting)
  - [Building from source](#building-from-source)
  - [Privacy and contact](#privacy-and-contact)

---

## Install

**Windows 10 or 11, 64-bit.** A release build produces two installers:

| File                                | What it is                        |
| ----------------------------------- | --------------------------------- |
| `ResponderHTTP_1.0.0_x64-setup.exe` | NSIS installer — the usual choice |
| `ResponderHTTP_1.0.0_x64_en-US.msi` | MSI, for deployment tooling       |

Both install the same single executable. A Microsoft Store listing is prepared
but not published yet.

**WebView2.** The app draws its interface with the Microsoft Edge WebView2
Runtime, which is part of Windows 11 and already present on most Windows 10
machines. If it is missing, Windows Update or Microsoft's standalone
installer adds it.

macOS and Linux are not supported today. The engine is portable, but secret
storage is written against Windows Credential Manager.

---

## Your first request

1. Open the app. A blank request tab is already waiting.
2. Type `https://httpbin.org/get` in the URL bar.
3. Press **Send**.

The response pane fills in: a status pill, how long it took, the size, and the
body with syntax highlighting. The timing line underneath breaks the total into
DNS, connect, TLS and time-to-first-byte, which is usually enough to tell a slow
server from a slow network.

To keep the request, press **Ctrl+S** and choose a collection.

---

## The window

**Left — the sidebar**, with three panels:

- **Collections** — your saved requests, in folders.
- **Environments** — sets of `{{variables}}`.
- **History** — everything you have sent.

The sidebar can be resized by dragging its edge, or collapsed entirely.

**Top — tabs.** Every request you open gets one. A dot on a tab means unsaved
changes. The **+** button offers a new HTTP request or a new WebSocket request.

**Middle — the request builder**: URL bar, method, and the Params, Headers,
Body, Auth, Settings and Docs sub-tabs.

**Bottom — the response.** Drag the divider to resize it, or collapse it with
the chevron.

---

## Building a request

**Method and URL.** Every common method — GET, POST, PUT, PATCH, DELETE, HEAD,
OPTIONS. The URL bar accepts a full URL including its query string.

**Params.** A table view of the URL's query string, kept in step both ways: edit
a row and the URL updates, edit the URL and the rows follow.

**Headers.** Name and value, with autocomplete for the common header names and
their usual values.

**Body.**

- **None**
- **Raw** — JSON, XML, HTML or plain text, with the content type you pick
- **Form URL-encoded** — a key-value table
- **Multipart** — text fields and file parts. A file part stores the _path_:
  the file is read from disk as the request is sent, so a large upload never
  sits in memory. Move the file and the request will tell you it cannot find it.

**Auth.** Basic, Bearer token, API key (in a header or the query string), or a
custom header. Anything the app treats as a secret is encrypted before it is
saved — see [Environments and variables](#environments-and-variables).

---

## Sending

**Send** runs the request and fills the response pane. While it is running the
button becomes **Cancel**, which stops it immediately.

**Send and download** — the chevron beside Send — writes the response body
straight to a file you choose instead of rendering it. The bytes never cross into the
interface, so a multi-gigabyte download costs nothing in memory. The Save As
dialog opens after the response arrives, which is what lets it suggest a
filename from `Content-Disposition`.

**Copy as cURL** turns the request as configured into a command line you can
paste into a terminal or a bug report.

---

## Reading the response

The bar across the top shows the status code, the total time, the size, and the
timing breakdown. Three views:

- **Pretty** — JSON and XML formatted, with syntax highlighting.
- **Raw** — exactly what arrived.
- **Headers** — the response headers as a table.

**Size.** Hover the size for a breakdown: the response split into headers and
body, and the request split the same way. A gzipped response counts decoded —
the size of what you are looking at.

**Save response** stores the response next to the request that produced it, as
an _example_. Examples reopen in their own tab and are kept with the request in
its collection.

A binary response is not rendered; the app reports its size and offers the
download instead.

---

## Server-sent events

A response that arrives as `Content-Type: text/event-stream` is shown **event by
event while it is still arriving**. There is nothing to switch on and Send stays
one button — it is the response that differs, not the request.

- **Events** — one row per event, numbered, with its event name, `id` and a
  millisecond timestamp. Newest at the top. The list follows the newest event
  unless you scroll down to read older ones.
- **Raw** — the stream exactly as it arrived, growing as it goes.
- **Headers** — as usual.

Comments (`: keep-alive`) are shown as comments rather than as empty events. A
**Streaming** badge and a running event count and byte total sit in the status
line while the stream is open.

Two things worth knowing:

- **The request timeout stops applying once a response turns out to be a
  stream.** A timeout is there to stop you waiting forever for a response, not
  to cut one off that is working. A request that never answers still times out.
- The viewer keeps the most recent **1 000 events, up to 32 MiB**, and says how
  many earlier ones it dropped. The stream itself is unaffected.

---

## WebSocket

WebSocket requests live in the same collections as HTTP ones. Create one from
the **+** menu or by right-clicking a collection.

**Connect** performs the handshake; the badge follows the connection state.
Query parameters and handshake headers work as they do for HTTP, and the cookie
jar is offered to the handshake unless you turn that off.

**The composer** sends Text, JSON, XML, HTML or Binary. Binary is typed as
**Base64** or **hexadecimal**, whichever you prefer, and the choice is
remembered. JSON, XML and HTML get a **Beautify** button; text that does not
parse is left exactly as you typed it, with the reason shown.

**The message log** runs newest first with millisecond timestamps. Rows expand
to show the whole message — pretty-printed JSON, a hex dump, the handshake
headers. There is a direction filter, a search box, and **Clear Messages**,
which empties the log of both the current connection and any before it.

**Settings** for a WebSocket request: verify TLS, proxy, send cookies, handshake
timeout (30 s by default), largest message to accept (1 MiB by default; a bigger
one closes the connection with code 1009), and automatic reconnect, which is off
by default and applies only to an unexpected drop — never after you disconnect
and never after a refused handshake.

---

## Collections and folders

Save a request with **Ctrl+S**. Collections hold folders, folders hold requests,
and the tree is yours to arrange.

Right-click anything for its menu:

| On a collection       | On a folder           | On a request |
| --------------------- | --------------------- | ------------ |
| New HTTP request      | New HTTP request      | Rename       |
| New WebSocket request | New WebSocket request | Docs         |
| New folder            | New folder            | Duplicate    |
| Rename                | Rename                | Move         |
| Docs                  | Docs                  | Delete       |
| Export as OpenAPI     | Delete                |              |
| Delete                |                       |              |

**Move** puts a request in another folder of the same collection. **Duplicate**
copies it in place. Deleting a collection or folder deletes what is inside it,
and the app asks first.

---

## Environments and variables

An environment is a named set of variables. Write `{{base_url}}` anywhere in a
request — the URL, a header, the body, an auth field — and the active
environment's value is substituted when you send.

Pick the active environment from the selector at the top right. Requests are
saved with the `{{placeholders}}` intact, so the same request works against
staging and production by switching environment.

**Secret variables.** Mark a variable secret and its value is encrypted before
it reaches the database, with a key held in Windows Credential Manager that only
your Windows account can read. Secrets never appear in history, in saved
examples, or in the log file. If the credential store cannot be reached, the app
still runs: secret fields load empty and refuse to save, rather than quietly
writing anything in plain text.

---

## History

Every request you send is recorded with its URL, status, duration and the
request itself. Click an entry to reopen it in a tab, ready to re-send. Secret
values are stored as their placeholders, never resolved.

---

## Documentation

Any collection, folder or request can carry Markdown notes: right-click →
**Docs**, or **Ctrl+Shift+D** for the request currently open in the builder
(once it has been saved — documentation hangs off a stored item). The editor has
Edit, Preview and Split views and saves as you type.

Documentation is included when you export a collection as OpenAPI, and imported
descriptions arrive as documentation.

---

## Cookies

Cookies that servers set are kept in a jar and sent back to matching hosts, per
RFC 6265. Session cookies do not survive a restart, and expired ones are purged
when the app starts.

The **Cookies** manager lists everything in the jar and lets you delete a single
cookie or clear them all. Any one request can be sent without the jar, from its
Settings tab.

---

## OpenAPI

**Import** a 3.0, 3.1 or 3.2 document, in JSON or YAML, as a new collection.
Nothing is written until you confirm: the dialog first shows how many requests
and saved responses it found, lets you choose how they are grouped into folders
(with a suggestion), and offers to create an environment from the document's
servers. Descriptions come in as documentation. Large real-world specifications
are supported.

**Export** a collection back out as OpenAPI, choosing the version and JSON or
YAML. Request and response shapes are inferred from the examples you saved.

WebSocket requests cannot be exported: the OpenAPI specification has no way to
describe them. The export dialog says how many it is leaving out.

---

## Per-request settings

Every request carries its own settings, saved with it.

| Setting                 | Default   | What it does                                                                                         |
| ----------------------- | --------- | ---------------------------------------------------------------------------------------------------- |
| Follow redirects        | On        | Follow 3xx responses                                                                                 |
| Max redirects           | 10        | How many hops before giving up                                                                       |
| Timeout                 | 30 000 ms | Whole request; lifted once a response turns out to be an event stream                                |
| Verify TLS              | **On**    | Turning it off is a deliberate, visible, per-request choice                                          |
| Minimum TLS             | Auto      | Or pin 1.2 / 1.3                                                                                     |
| HTTP version            | Auto      | Auto tries HTTP/2 over TLS and falls back to 1.1; or force 1.1 or 2                                  |
| Proxy                   | None      | Per request, not a global setting                                                                    |
| Send cookies            | On        | Use the jar for this request                                                                         |
| Keep method on redirect | Off       | Browsers and curl turn POST into GET on 301/302/303; this keeps it                                   |
| Keep auth across hosts  | Off       | On is how credentials leak somewhere unintended                                                      |
| Encode URL              | On        | Percent-encode the parameters the app appends; what you typed is passed through untouched either way |
| Allow HTTP/0.9          | Off       | Accept a bodies-only response                                                                        |

---

## Keyboard shortcuts

| Keys           | Action                                            |
| -------------- | ------------------------------------------------- |
| `Ctrl+S`       | Save the current request                          |
| `Ctrl+Shift+D` | Open documentation for the request in the builder |

---

## The tray

Closing the window hides the app to the notification area rather than quitting
it, so requests in flight and open WebSocket connections survive. The first few
closes say so in a toast with a **Don't show this again** button. The tray icon's
menu has Show and Quit.

---

## Where your data lives

One SQLite database plus a log folder, under your user profile:

```
%APPDATA%\io.github.zoran-php.responderhttp\
    responderhttp.sqlite3      collections, environments, history, cookies, settings
    logs\                      rotating log files
```

Installed from the Microsoft Store the same files live under
`%LOCALAPPDATA%\Packages\<package>\LocalCache\`, which Windows removes when the
app is uninstalled.

**Back up** by copying `responderhttp.sqlite3` while the app is closed. Note that
secrets in it are encrypted with a key in _your_ Windows Credential Manager — the
database alone will not carry them to another machine or account.

**Start over** by closing the app and deleting the database; it is recreated
empty on the next start.

**The log** records what happened, never what was in it: request bodies, auth
headers, tokens, cookies, WebSocket messages and stream events are all kept out,
and credentials that would otherwise ride along inside an error message are
redacted. It never leaves your computer.

---

## Troubleshooting

**"The database was created by a newer version."** A dialog says so at startup
and the app stops rather than touching the file. Install the newer version, or
move the database aside to start fresh.

**A request fails with a TLS error.** The server's certificate did not verify.
Check the clock, the certificate chain and any corporate proxy first. Turning
off Verify TLS for that one request will tell you whether that is the cause —
it is a per-request switch, and it should go back on.

**Everything times out behind a corporate network.** Set the proxy in the
request's Settings tab.

**A stream stops after 30 seconds.** It should not: the timeout is lifted once a
response identifies itself as `text/event-stream`. If it is being cut off, the
response is probably not sending that content type.

**Secrets ask to be re-entered.** The data key in Credential Manager could not
be read — usually a different Windows account, or a restored profile. The stored
values cannot be recovered; type them again.

**The window does not appear.** The app may be in the notification area. Click
its tray icon, or use Show from the icon's menu.

---

## Building from source

Requires Rust (stable), Node with pnpm, and on Windows the MSVC build tools.

```bash
pnpm install
pnpm tauri dev      # run it
pnpm tauri build    # single-file release plus installers
```

`verify.bat` runs the whole gate — typecheck, lint, frontend tests, `cargo fmt`,
clippy and the Rust test suites. `release.bat` adds the release build and checks
the binary imports nothing but Windows system DLLs.

Architecture, conventions and the rules the code is held to are in
[`CLAUDE.md`](CLAUDE.md); the phased build log is in [`PLAN.md`](PLAN.md).

---

## Privacy and contact

The full policy is [`docs/privacy-policy.md`](docs/privacy-policy.md). The
short version: nothing is collected, nothing is sent anywhere except the
addresses you type, and everything the app keeps stays on your computer.

Questions and bug reports: **zorandavidovic@outlook.com**

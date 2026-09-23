# Microsoft Store listing — ResponderHTTP

Drafts for the Partner Center listing fields. Store ID `9NV8JJQKDBRK`.
Copy each block into the matching field; nothing here is read by the build.

---

## Product name

```
ResponderHTTP
```

## Short description (max 1000 characters)

```
A desktop API client that ships as one file. Send HTTP requests, open
WebSocket connections, watch a server-sent event stream arrive live, and keep
your collections, environments and history on your own machine. curl is built
in — nothing to install alongside it.
```

## Description (max 10 000 characters)

```
ResponderHTTP is an API client for Windows. Type a URL, choose a method, add
headers, parameters, a body and authentication, and send. You get the status
code, the response headers, the body with syntax highlighting, and a timing
breakdown showing where the time actually went — DNS, connect, TLS, first
byte, total.

WebSocket requests sit beside HTTP ones in the same collections: connect,
send text, JSON, XML, HTML or binary, and read the conversation in a
timestamped log. And when a response arrives as an event stream, it is shown
event by event while it is still arriving rather than after it ends.

It ships as a single executable. The HTTP and WebSocket engine is libcurl,
compiled into the app, so there is no curl to install, no runtime to add and
no system SSL library to keep up to date. Everything the app stores —
collections, environments, history, cookies — lives in a local database on
your PC.

WHAT IT DOES

• Requests — every common method, query parameters, headers, form and raw
  bodies, and per-request control over redirects, timeouts, TLS verification
  and proxies.
• Authentication — Basic, Bearer, API key and custom schemes.
• Responses — pretty-printed JSON and XML with a raw toggle, response headers,
  cookies, a size breakdown, and the full timing breakdown.
• Server-sent events — a text/event-stream response is shown event by event
  as it arrives, newest first, with a live raw view. Nothing to turn on.
• WebSocket — ws:// and wss:// requests saved alongside HTTP ones, a composer
  for text, JSON, XML, HTML and binary (Base64 or hex) messages, and a
  searchable message log with millisecond timestamps.
• Collections — organise saved requests into folders, keep example responses
  next to the request that produced them.
• Environments — {{variables}} substituted into any part of a request, with
  secret values encrypted at rest.
• History — every request you sent, ready to re-send.
• Documentation — Markdown notes on any collection, folder or request, edited
  and previewed in the app.
• OpenAPI — import a 3.0, 3.1 or 3.2 document as a collection, in JSON or
  YAML, and export a collection back out. Large real-world specifications are
  supported.
• Copy as cURL — turn any request into a command you can paste elsewhere.

PRIVACY

No accounts, no telemetry, no analytics, no crash reporting. Requests and
WebSocket connections go straight from your computer to the server you named. Passwords, tokens and
API keys are encrypted with a key held in Windows Credential Manager and are
never written in plain text.
```

## Search terms (max 7, 30 characters each)

```
api client
http client
rest client
curl
openapi
postman alternative
websocket client
```

## Category

```
Developer tools
```

## Subcategory

```
Utilities
```

Not Networking: in a developer-tools listing that shelf is for traffic-level
tooling — proxies, packet capture, protocol analysers — where Fiddler belongs
because it sits in the middle of someone else's traffic. This app does not
intercept anything; you write a request and it sends it. Changeable later
through a new submission, unlike a Games category.

## System requirements

Partner Center's *System requirements* table, row by row. The section is
optional and leaving it blank publishes no hardware requirements at all — which
is nearly the right answer for a developer tool. The one thing to understand
before ticking anything: a box under **Minimum hardware** makes the Store show
a warning to customers whose device lacks that feature, and **those customers
cannot rate or review the app**. It does not stop them installing it. So a tick
there costs reviews and buys nothing for an app that only needs a PC.

| Feature | Minimum | Recommended | Why |
|---|---|---|---|
| Touch screen | — | — | Works with touch, needs none |
| Keyboard | — | ✓ | It is a text-entry tool; the on-screen keyboard still works, so this is a recommendation, not a requirement |
| Mouse | — | ✓ | Right-click menus and draggable dividers want a pointer |
| Camera | — | — | |
| NFC HCE | — | — | |
| NFC Proximity | — | — | |
| Bluetooth LE | — | — | |
| Telephony | — | — | |
| Microphone | — | — | |
| Xbox controller or gamepad | — | — | |
| Windows Mixed Reality motion controllers | — | — | |
| Windows Mixed Reality immersive headset | — | — | |
| Memory | 4 GB | 8 GB | A judgement, not a measurement — see below |
| DirectX | Not specified | Not specified | The interface is HTML in WebView2; there is no feature level to claim |
| Video memory | Not specified | Not specified | |
| Processor | *(blank)* | *(blank)* | Naming a threshold nobody has tested is worse than saying nothing |
| Graphics | *(blank)* | *(blank)* | |

**Memory, DirectX, Video memory, Processor and Graphics are never verified by
the Store** — no warning is ever shown for them, on any device. They are display
text on the listing and nothing more. The 4 GB / 8 GB figures have not been
measured; they are what a WebView2 app of this size needs to be pleasant. Either
measure them or leave Memory unspecified as well.

Architecture is not in this table. x64-only comes from the package itself.

## Additional system requirements

Partner Center's _System requirements_ section covers hardware only, so the
WebView2 note belongs in the listing's **Additional system requirements**
free-text field (PLAN.md Phase 10, "WebView2 in the Store package"):

```
Requires the Microsoft Edge WebView2 Runtime, which is included in Windows 11
and already installed on most Windows 10 PCs.
```

## Restricted capabilities (Submission options page)

Uploading the package raises: *"The following restricted capabilities require
approval before you can use them in your app: runFullTrust."* That is a
**warning, not a rejection**, and it is unavoidable — `runFullTrust` is what
lets a packaged Win32 app install and run at all, and the manifest pairs it
with `EntryPoint="Windows.FullTrustApplication"`. Removing it would break the
package. It is the only restricted capability declared.

Microsoft no longer takes these by support ticket: the details go on the
**Submission options** page of the submission, and certification testers read
them, which can add a little review time. Paste:

```
ResponderHTTP is a packaged Win32 desktop application rather than a UWP app:
its executable is declared with EntryPoint="Windows.FullTrustApplication", and
runFullTrust is the capability that allows such a package to be installed and
launched. It is the only restricted capability this package declares.

Full trust is what the app's ordinary work needs:

- It sends HTTP requests and opens WebSocket connections using libcurl, which
  is compiled into the executable — a native networking stack rather than the
  WinRT HTTP APIs.
- It keeps saved requests, collections, environments, history and cookies in a
  SQLite database in its own app-data folder.
- It reads and writes one data key in Windows Credential Manager, so that
  passwords, bearer tokens and API keys the user saves are encrypted rather
  than stored in plain text.
- It opens and saves files the user chooses in the system dialog: file parts
  for multipart uploads, OpenAPI documents to import, and response bodies
  saved to disk.
- It hosts its interface in the Microsoft Edge WebView2 Runtime and places an
  icon in the notification area.

The app has no accounts, no telemetry and no analytics, collects nothing, and
contacts no service operated by the developer. Requests go only to the
addresses the user types. Privacy policy:
https://zoran-php.github.io/responderhttp/privacy-policy.html
```

## Privacy policy URL

The `docs/` folder is the GitHub Pages site: three hand-written pages —
`index.html` (overview), `privacy-policy.html` and `terms.html` — plus a
`.nojekyll` marker, so what is committed is served verbatim. Paste:

```
https://zoran-php.github.io/responderhttp/privacy-policy.html
```

The extensionless `/privacy-policy` resolves to the same page; the `.html` form
is the one that cannot surprise anyone. Check it in a browser before pasting —
a reviewer will.

## Price

```
Free
```

---

## Screenshots

At least one is required; up to ten, 1366 × 768 or larger, PNG. Worth taking:

1. A request with headers and a JSON response, timing breakdown visible.
2. The collections sidebar with a large imported collection.
3. The OpenAPI import dialog mid-preview.
4. An environment with a secret variable.
5. A WebSocket session mid-conversation, with the message log filled.
6. An event stream arriving, the Streaming badge and event count visible.

## Age rating

The questionnaire is short for a developer tool: no user-generated content
shared between users, no ads, no in-app purchases, no data collection.

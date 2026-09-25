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

## Product features (up to 20, one per field)

Short lines shown as a bulleted list near the top of the listing — the part a
reader actually scans. One per field; *Add more* adds the next.

```
HTTP requests with every common method, query parameters, headers, and raw, form or multipart bodies
Basic, Bearer, API key and custom authentication
Pretty-printed JSON and XML with a raw toggle, response headers and cookies
Timing breakdown for every request: DNS, connect, TLS, time to first byte, total
Server-sent events shown live, event by event, as the stream arrives
WebSocket requests with a Text, JSON, XML, HTML and binary composer
A searchable WebSocket message log with millisecond timestamps
Collections and folders, with example responses saved beside the request
Environments and {{variables}}, resolved when you send
Secrets encrypted with a key held in Windows Credential Manager
Request history, ready to re-send
OpenAPI 3.0, 3.1 and 3.2 import and export, in JSON or YAML
Markdown documentation on any collection, folder or request
Copy as cURL
One executable with libcurl built in — no curl, runtime or SSL library to install
No accounts, no telemetry, no analytics; everything stored on your PC
```

## Store logos

Generated from `app-icon.png` by the snippet recorded below, so the listing art
cannot drift from the app icon. Files are in `store/art/`.

| Slot | File | Notes |
|---|---|---|
| 9:16 Poster art | `poster-9x16-1440x2160.png` | Partner Center calls it "required for display on Xbox", but its own hint also says it is **the main logo for Windows 10/11 customers** — so upload it |
| 1:1 Box art | `box-1x1-2160x2160.png` | Recommended; used in various Store layouts |
| Store display images (300×300, 150×150) | — | **Leave empty.** The Store falls back to the logos in the package, which `winapp manifest update-assets` already generated at every scale |
| Store display image 71×71 | `logo-71x71.png` | Generated for the slot that asks for it by name; the package fallback covers it otherwise |

### Promotional art

Optional, and only ever shown if Microsoft features the app — but they cost
nothing now that the generator exists.

| Slot | File | Notes |
|---|---|---|
| Super hero art | `super-hero-3840x2160.png` | **No wordmark, subject kept left.** A hero placement has the Store's own title and buttons laid over it, so the right two thirds are deliberately quiet |
| Titled hero art | `titled-hero-1920x1080.png` | The titled variant is the one that carries the name, so the wordmark is the point |
| Branded key art | `branded-key-art-584x800.png` | Portrait: icon above, name below |
| Featured promotional square art | `promotional-square-1080x1080.png` | **No title** — Microsoft's rule for this slot is that it must not include the product's title, so it is the icon alone, centred and large |

All five, and the two above, come from `tools/build-store-art.py` — one 1024 px
icon in, every size out, so nothing drifts. Text is set in Lato; the site uses
Inter, which is not installed on the build machine, and Lato is the nearest
humanist sans that reads the same at these sizes.

**Confirmed by the guidance:** featured promotional square art must not include
the product's title. That rule is the reason the super hero art carries no
wordmark either — a *titled* hero exists precisely because the plain one has no
title — though only the square rule has been read first-hand. Branded key art
keeps its wordmark: "branded" is the slot asking for it. Worth a last check of
the asset guidance before submitting.

The gradient runs from the manifest's `BackgroundColor` (#3B4252) to the site's
page colour (#181D24), so tile, listing and web pages agree.

## Trailers

Skip. A trailer only appears at the top of the listing if a 16:9 hero image is
uploaded with it, which is two more assets for a developer tool nobody watches a
video about.

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

## Copyright and trademark info

One line, matching the website footer and the `LICENSE` header:

```
© 2026 Zoran Davidović. All rights reserved.
```

A trademark claim is optional and unregistered marks use ™, never ®. If you
want one: `© 2026 Zoran Davidović. All rights reserved. ResponderHTTP™ is a
trademark of Zoran Davidović.`

## Additional license terms

Only needed because this app ships its own `LICENSE`, which says more than
Microsoft's Standard Application License Terms do. Leaving it blank is valid —
the standard terms would then govern the Store copy on their own — but then the
Store would be silent while the app carries a licence file, and the two should
not disagree.

```
ResponderHTTP is licensed, not sold, and is provided free of charge. In addition to the Standard Application License Terms:

- You may use ResponderHTTP for any purpose, including commercial use and use within an organisation, on any number of devices, at no cost.
- You may not distribute a modified version of the application or any work derived from it, and you may not present it as your own work or distribute it under another name.
- The application is provided "as is", without warranty of any kind. You are responsible for the requests you send with it and for having permission to send them.
- The application includes third-party open-source components, each of which remains subject to its own licence.

Full terms: https://zoran-php.github.io/responderhttp/terms.html
```

Deliberately **not** included: the permission in `LICENSE` to pass the
unmodified installer on to someone else. That is true of the copy from GitHub;
a Store copy is delivered by the Store to the account that acquired it, so
granting it here would describe something that cannot happen.

## Price

```
Free
```

---

## Languages

`Package.appxmanifest` declares `<Resource Language="en-us" />`, which is why
Partner Center lists exactly one language supported in packages: **English
(United States)**. That single listing is shown in every market, so there is no
market this leaves uncovered.

**Leave *Additional Store listing languages* empty.** A listing in another
language is a promise the app does not keep — the interface is English only —
and every extra language is a translation to maintain on each submission.
Localising the app is the thing that would change this, and then the manifest
gains the language first.

The "Incomplete" status beside English (United States) is not an error: it means
the listing fields for that language have not been filled in yet. Click through
to it, paste the blocks above, add at least one screenshot, and it turns
complete.

## Screenshots

**Where:** the same per-language Store listing page as everything else —
*Submission > Store listings > English (United States)*, section **Screenshots**.

**Rules:** `.png` only, at least 1366 × 768, up to 3840 × 2160, 50 MB each. One
is required, ten is the maximum for desktop, four or more is what Microsoft
recommends. Order is drag-to-reorder after upload. Each may carry a caption of
**200 characters or less**.

Two constraints worth designing around: the Store may lay its own text over the
**bottom third**, so keep the point of the picture in the top two-thirds; and no
logos, marketing copy or added text — just the app.

Capture with the window at 1366 × 768 or larger, on the dark theme it ships
with. Six worth taking, with captions ready to paste:

1. A request with headers and a JSON response, timing breakdown visible.
   > Send a request and read the response: status, headers, body with syntax highlighting, and a timing breakdown showing DNS, connect, TLS and time to first byte.
2. An event stream arriving, the Streaming badge and event count visible.
   > A server-sent event stream shown event by event as it arrives, newest first, with a live raw view. Nothing to turn on — any text/event-stream response does this.
3. A WebSocket session mid-conversation, with the message log filled.
   > WebSocket requests sit beside HTTP ones. Send text, JSON, XML, HTML or binary, and read the conversation in a timestamped, searchable log.
4. The collections sidebar with a large imported collection.
   > Organise saved requests into collections and folders, with example responses kept beside the request that produced them.
5. The OpenAPI import dialog mid-preview.
   > Import an OpenAPI 3.0, 3.1 or 3.2 document, in JSON or YAML, as a collection — with a preview of what will be created before anything is written.
6. An environment with a secret variable.
   > Environments substitute {{variables}} into any part of a request. Values marked secret are encrypted with a key held in Windows Credential Manager.

## Age rating

The questionnaire is short for a developer tool: no user-generated content
shared between users, no ads, no in-app purchases, no data collection.

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

## Additional system requirements

Partner Center's _System requirements_ section covers hardware only, so the
WebView2 note belongs in the listing's **Additional system requirements**
free-text field (PLAN.md Phase 10, "WebView2 in the Store package"):

```
Requires the Microsoft Edge WebView2 Runtime, which is included in Windows 11
and already installed on most Windows 10 PCs.
```

## Privacy policy URL

`store/privacy-policy.md` in this repository is the text. Partner Center wants
a public URL, so it needs hosting — GitHub Pages on
`zoran-php/responderhttp` is the free option, which would give roughly:

```
https://zoran-php.github.io/responderhttp/privacy-policy
```

**Not done yet — this is the one submission blocker that needs a decision.**

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

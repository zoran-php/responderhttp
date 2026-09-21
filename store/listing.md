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
A desktop API client that ships as one file. Send HTTP requests, read the
response, and keep your collections, environments and history on your own
machine. curl is built in — nothing to install alongside it.
```

## Description (max 10 000 characters)

```
ResponderHTTP is an API client for Windows. Type a URL, choose a method, add
headers, parameters, a body and authentication, and send. You get the status
code, the response headers, the body with syntax highlighting, and a timing
breakdown showing where the time actually went — DNS, connect, TLS, first
byte, total.

It ships as a single executable. The HTTP engine is libcurl, compiled into the
app, so there is no curl to install, no runtime to add and no system SSL
library to keep up to date. Everything the app stores — collections,
environments, history, cookies — lives in a local database on your PC.

WHAT IT DOES

• Requests — every common method, query parameters, headers, form and raw
  bodies, and per-request control over redirects, timeouts, TLS verification
  and proxies.
• Authentication — Basic, Bearer, API key and custom schemes.
• Responses — pretty-printed JSON and XML with a raw toggle, response headers,
  cookies, and the full timing breakdown.
• Collections — organise saved requests into folders, keep example responses
  next to the request that produced them.
• Environments — {{variables}} substituted into any part of a request, with
  secret values encrypted at rest.
• History — every request you sent, ready to re-send.
• OpenAPI — import a 3.0, 3.1 or 3.2 document as a collection, in JSON or
  YAML, and export a collection back out. Large real-world specifications are
  supported.
• Copy as cURL — turn any request into a command you can paste elsewhere.

PRIVACY

No accounts, no telemetry, no analytics, no crash reporting. Requests go
straight from your computer to the server you named. Passwords, tokens and
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
api testing
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

## Age rating

The questionnaire is short for a developer tool: no user-generated content
shared between users, no ads, no in-app purchases, no data collection.

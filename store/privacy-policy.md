# ResponderHTTP — Privacy Policy

_Last updated: 23 September 2026_

ResponderHTTP is a desktop API client published by Zoran Davidović. This page
describes what the app does with your data. It is short because the app does
very little with it.

## The developer collects nothing

ResponderHTTP has no analytics, no telemetry, no crash reporting and no
accounts. It does not phone home, and it contains no update checker. Nothing
about you or your use of the app is sent to the developer, and there is no
server to send it to.

## Everything the app stores stays on your computer

The app keeps its data in a SQLite database in your Windows user profile,
inside the app's own storage. That database holds:

- the HTTP and WebSocket requests and collections you save,
- the Markdown documentation you write for a collection, folder or request,
- environments and their variables,
- your request history,
- cookies received from servers you sent requests to,
- application settings.

Uninstalling the app removes this data.

## Credentials

Values the app treats as secrets — passwords, bearer tokens, API keys, and
environment variables marked secret — are never written to the database in
plain text. They are encrypted with a key held in Windows Credential Manager,
which only your Windows account can read. The key never leaves your computer.

## Network requests

ResponderHTTP sends HTTP requests, and opens WebSocket connections, **only to
the addresses you enter**. It is a tool for making those requests, so the
destination, the headers and the body are entirely under your control. They go
directly from your computer to the server you named; they do not pass through
any service operated by the developer.

## Logs

The app writes a local log file to help diagnose problems. Request bodies,
authentication headers, tokens and cookies are deliberately kept out of it, as
are the WebSocket messages you send and receive and the events of a streamed
response; a redaction step strips credentials that would otherwise ride along
inside an error message. The log stays on your computer and is never uploaded.

## Files you open or save

Importing an OpenAPI document, or saving a response to disk, reads and writes
only the file you chose in the system dialog.

## Children

The app is a developer tool and is not directed at children.

## Changes to this policy

If this policy changes, the updated version will be published at this address
and the date above will change.

## Contact

Questions about this policy: **zorandavidovic@outlook.com**

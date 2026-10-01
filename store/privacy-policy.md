# ResponderHTTP — Privacy Policy

_Last updated: 30 September 2026_

ResponderHTTP is a desktop API client published by Zoran Davidović. This page
describes what the app does with your data. It is short because the app does
very little with it.

## The developer collects nothing

ResponderHTTP has no analytics, no telemetry, no crash reporting and no
accounts. It does not phone home, and it contains no update checker. Nothing
about you or your use of the app is sent to the developer, and there is no
server to send it to.

## Everything the app stores stays on your computer

The app keeps its data in a SQLite database inside the app's own storage: in
your Windows user profile on Windows, and in your home folder on Linux. That
database holds:

- the HTTP, WebSocket and gRPC requests and collections you save,
- the gRPC schemas you import or save to the schema library: the text of the
  .proto files and the compiled schema,
- the Markdown documentation you write for a collection, folder or request,
- environments and their variables,
- your request history,
- cookies received from servers you sent requests to,
- application settings.

On Windows, uninstalling the app removes this data. On Linux, uninstalling
leaves it in your home folder; delete the app's data folder to remove it.

## Credentials

Values the app treats as secrets — passwords, bearer tokens, API keys, and
environment variables marked secret — are never written to the database in
plain text. They are encrypted with a key held in the operating system's
credential store, which only your user account can read: Windows Credential
Manager on Windows, and your desktop's keyring on Linux (GNOME Keyring or
KWallet, reached through the secret portal when the app runs as a Flatpak). The
key never leaves your computer.

## Network requests

ResponderHTTP sends HTTP requests and gRPC calls, and opens WebSocket
connections, **only to the addresses you enter**. Loading a gRPC schema by
server reflection asks the server you entered, and no one else. It is a tool for making those requests, so the
destination, the headers and the body are entirely under your control. They go
directly from your computer to the server you named; they do not pass through
any service operated by the developer.

## Logs

The app writes a local log file to help diagnose problems. Request bodies,
authentication headers, tokens and cookies are deliberately kept out of it, as
are the WebSocket and gRPC messages you send and receive, gRPC metadata, and
the events of a streamed response; a redaction step strips credentials that would otherwise ride along
inside an error message. The log stays on your computer and is never uploaded.

## Files you open or save

Importing an OpenAPI document, or saving a response to disk, reads and writes
only the file you chose in the system dialog. Importing .proto files reads the
files you chose and the files they import, looked up only in their own folders
and the import folders you chose. When the app runs as a Flatpak, it has no
access to your files of its own: the system dialog grants it the files you
choose, and nothing else.

## Children

The app is a developer tool and is not directed at children.

## Changes to this policy

If this policy changes, the updated version will be published at this address
and the date above will change.

## Contact

Questions about this policy: **zorandavidovic@outlook.com**

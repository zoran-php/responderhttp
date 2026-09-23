-- 0010_web_sockets.sql
--
-- WebSocket requests (PLAN.md Phase 13d). Committed and shipped: never edit
-- this file, add a new numbered one (CLAUDE.md section 11, rule 5).
--
-- WebSocket requests live in the same `requests` table as HTTP ones, so a
-- collection or folder can hold both, and rename, move, delete, docs and the
-- ON DELETE CASCADE from folders and collections work on them unchanged. A
-- second table would mean keeping all of that in step twice.
--
-- `kind` tells the two apart. Every existing row is HTTP, which is what the
-- default says. Only two values are ever written, both from Rust constants
-- (persistence/repositories/request_kind.rs); a CHECK constraint cannot be
-- added to an existing table by ALTER TABLE in SQLite, so the closed enum on
-- the Rust side is the guard.
--
-- A WebSocket row keeps its URL in `url` and its handshake headers in
-- `headers_json`, reusing the columns and codecs HTTP already has. Its
-- method is stored as GET, which is truthful: the handshake is a GET. What
-- only a WebSocket has — the draft message, its format, and the connection
-- settings — goes in `ws_json`. Empty for HTTP rows.
--
-- The HTTP repository reads `WHERE kind = 'http'`, so a WebSocket row can
-- never be decoded as an HTTP request, sent, or exported to OpenAPI.

ALTER TABLE requests ADD COLUMN kind TEXT NOT NULL DEFAULT 'http';
ALTER TABLE requests ADD COLUMN ws_json TEXT NOT NULL DEFAULT '';

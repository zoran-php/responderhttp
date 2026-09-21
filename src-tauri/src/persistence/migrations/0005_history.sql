-- 0005_history.sql
--
-- Sent-request history (PLAN.md Phase 6). Committed and shipped: never edit
-- this file, add a new numbered one (CLAUDE.md section 11, rule 5).
--
-- The request is stored as the user typed it, {{placeholders}} intact, in the
-- same JSON columns a saved request uses — so a re-run picks up the current
-- environment rather than replaying whichever one was active that day.
-- `resolved_url` is what actually went over the wire, kept only so the list
-- is readable and searchable.
--
-- `status` is NULL when the request never reached a response (transport
-- failure, cancellation); `error_kind` is NULL when it did.

CREATE TABLE history (
    id                TEXT PRIMARY KEY,
    sent_at           TEXT NOT NULL,
    resolved_url      TEXT NOT NULL,
    status            INTEGER,
    error_kind        TEXT,
    duration_ms       INTEGER NOT NULL,
    method            TEXT NOT NULL,
    url               TEXT NOT NULL,
    headers_json      TEXT NOT NULL,
    query_params_json TEXT NOT NULL,
    body_json         TEXT NOT NULL,
    auth_json         TEXT NOT NULL,
    settings_json     TEXT NOT NULL
);

CREATE INDEX idx_history_sent_at ON history (sent_at DESC);

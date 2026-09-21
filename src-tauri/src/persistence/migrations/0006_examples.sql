-- 0006_examples.sql
--
-- Saved responses: a named response kept
-- under the request that produced it, which is what makes a request node in
-- the sidebar expandable. Committed and shipped: never edit this file, add a
-- new numbered one (CLAUDE.md section 11, rule 5).
--
-- The request snapshot is stored beside the response, in the same JSON
-- columns a saved request uses, so several examples under one request can
-- each say which inputs produced them. It is the request *as it was sent* —
-- resolved, unlike a saved request or a history entry — because an example
-- documents one exchange that actually happened.
--
-- ON DELETE CASCADE: deleting a request takes its examples with it, the same
-- way deleting a collection takes its folders and requests.
--
-- `response_body` is TEXT. A binary response never reaches this layer (only
-- its length survives ResponseBody::from_bytes), so there is nothing to
-- store and the UI refuses the save rather than writing an empty row.

CREATE TABLE examples (
    id                    TEXT PRIMARY KEY,
    request_id            TEXT NOT NULL REFERENCES requests (id) ON DELETE CASCADE,
    name                  TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    method                TEXT NOT NULL,
    url                   TEXT NOT NULL,
    headers_json          TEXT NOT NULL,
    query_params_json     TEXT NOT NULL,
    body_json             TEXT NOT NULL,
    auth_json             TEXT NOT NULL,
    settings_json         TEXT NOT NULL,
    status                INTEGER NOT NULL,
    response_headers_json TEXT NOT NULL,
    response_body         TEXT NOT NULL
);

CREATE INDEX idx_examples_request ON examples (request_id);

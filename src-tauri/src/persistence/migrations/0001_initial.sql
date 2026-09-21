-- 0001_initial.sql
--
-- Committed and shipped: never edit this file, add a new numbered one
-- (CLAUDE.md section 11, rule 5).

CREATE TABLE collections (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE folders (
    id               TEXT PRIMARY KEY,
    collection_id    TEXT NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    parent_folder_id TEXT REFERENCES folders (id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    created_at       TEXT NOT NULL
);

CREATE INDEX idx_folders_collection ON folders (collection_id);

-- Headers, params, body and settings live as JSON so adding a request
-- feature does not need a migration (CLAUDE.md section 5).
CREATE TABLE requests (
    id                TEXT PRIMARY KEY,
    collection_id     TEXT NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    folder_id         TEXT REFERENCES folders (id) ON DELETE CASCADE,
    name              TEXT NOT NULL,
    method            TEXT NOT NULL,
    url               TEXT NOT NULL,
    headers_json      TEXT NOT NULL,
    query_params_json TEXT NOT NULL,
    body_json         TEXT NOT NULL,
    settings_json     TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE INDEX idx_requests_collection ON requests (collection_id);
CREATE INDEX idx_requests_folder ON requests (folder_id);

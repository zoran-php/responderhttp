-- 0002_request_auth.sql
--
-- Per-request authentication (PLAN.md Phase 4). Committed and shipped:
-- never edit this file, add a new numbered one (CLAUDE.md section 11, rule 5).
--
-- The column is NOT NULL, so the default has to be a valid StoredAuth
-- document rather than an empty string — that is what lets every request
-- saved before this migration keep loading unchanged.

ALTER TABLE requests ADD COLUMN auth_json TEXT NOT NULL DEFAULT '{"kind":"None"}';

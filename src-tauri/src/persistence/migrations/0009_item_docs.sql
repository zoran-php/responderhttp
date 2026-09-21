-- 0009_item_docs.sql
--
-- Markdown documentation for a collection, a folder or a request
-- (PLAN.md Phase 12). Committed and shipped: never edit this file, add a new
-- numbered one (CLAUDE.md section 11, rule 5).
--
-- A column on each owning table rather than one polymorphic
-- `docs (item_type, item_id, markdown)` table. SQLite cannot enforce a foreign
-- key whose target table varies, so a polymorphic table would leave orphan
-- rows behind every deleted collection; a column inherits the ON DELETE
-- CASCADE the three tables already have, and needs no index of its own.
--
-- Empty string rather than NULL, so reading never has to decide what a
-- missing row means: every item has documentation, and most of it is empty.
--
-- Plain text by design. Documentation is prose the user writes and it is
-- exported verbatim into OpenAPI descriptions, so it does not go through the
-- Phase 9 sealing path. domain/secrets.rs stays the one place that decides
-- what is a secret, and docs_md is not on that list.

ALTER TABLE collections ADD COLUMN docs_md TEXT NOT NULL DEFAULT '';
ALTER TABLE folders ADD COLUMN docs_md TEXT NOT NULL DEFAULT '';
ALTER TABLE requests ADD COLUMN docs_md TEXT NOT NULL DEFAULT '';

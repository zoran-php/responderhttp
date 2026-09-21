-- 0007_secrets.sql
--
-- Secret environment variables (PLAN.md Phase 9). Committed and shipped once
-- released: never edit this file, add a new numbered one (CLAUDE.md section
-- 11, rule 5).
--
-- A secret variable keeps `value` empty and its sealed bytes in
-- `value_sealed`. Every row written before this migration is a plain
-- variable, which is exactly what the defaults say.
--
-- Auth secrets need no column: they live inside `auth_json`, and the startup
-- upgrade (persistence/repositories/secret_upgrade.rs) seals the ones already
-- there. SQL cannot encrypt, so that step cannot be a migration.

ALTER TABLE environment_variables ADD COLUMN secret INTEGER NOT NULL DEFAULT 0;
ALTER TABLE environment_variables ADD COLUMN value_sealed BLOB;

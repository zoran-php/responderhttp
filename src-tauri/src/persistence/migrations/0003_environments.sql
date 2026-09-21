-- 0003_environments.sql
--
-- Environments and their {{variable}} bindings (PLAN.md Phase 5). Committed
-- and shipped: never edit this file, add a new numbered one (CLAUDE.md
-- section 11, rule 5).

CREATE TABLE environments (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- `position` keeps the editor's row order, which is otherwise lost on reload.
-- The primary key's leading column is environment_id, so lookups by
-- environment already have an index and a second one would be dead weight.
CREATE TABLE environment_variables (
    environment_id TEXT NOT NULL REFERENCES environments (id) ON DELETE CASCADE,
    position       INTEGER NOT NULL,
    name           TEXT NOT NULL,
    value          TEXT NOT NULL,
    PRIMARY KEY (environment_id, position)
);

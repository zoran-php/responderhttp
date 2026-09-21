-- 0004_cookies.sql
--
-- The cookie jar (PLAN.md, Phase 5 extension). Committed and shipped: never
-- edit this file, add a new numbered one (CLAUDE.md section 11, rule 5).
--
-- Times here are unix seconds, not the ISO strings the other tables use for
-- created_at. An expiry is compared on every request rather than displayed,
-- and integer comparison keeps the matching rules in domain/cookies.rs pure
-- with no date handling at all.
--
-- The primary key is RFC 6265's identity triple, so re-setting a cookie
-- replaces it instead of accumulating duplicates.

CREATE TABLE cookies (
    domain     TEXT NOT NULL,
    path       TEXT NOT NULL,
    name       TEXT NOT NULL,
    value      TEXT NOT NULL,
    expires_at INTEGER,
    secure     INTEGER NOT NULL,
    http_only  INTEGER NOT NULL,
    host_only  INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (domain, path, name)
);

-- App-level settings: one row per key, values as TEXT.
--
-- Deliberately a key/value table rather than a column per setting: these are
-- shell preferences, not domain data, and a new one must not mean a new
-- migration every time (CLAUDE.md section 5, same reasoning as the JSON
-- columns on requests).
--
-- Never a home for secrets. Everything here is plain text; anything sensitive
-- goes through the envelope in secrets/ like every other secret.
CREATE TABLE app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

// http_client/src-tauri/src/persistence/database.rs
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::domain::error::AppError;

/// Numbered, append-only. `PRAGMA user_version` records how many have run,
/// so adding a migration means appending to this list and nothing else.
const MIGRATIONS: [&str; 10] = [
    include_str!("migrations/0001_initial.sql"),
    include_str!("migrations/0002_request_auth.sql"),
    include_str!("migrations/0003_environments.sql"),
    include_str!("migrations/0004_cookies.sql"),
    include_str!("migrations/0005_history.sql"),
    include_str!("migrations/0006_examples.sql"),
    include_str!("migrations/0007_secrets.sql"),
    include_str!("migrations/0008_app_settings.sql"),
    include_str!("migrations/0009_item_docs.sql"),
    include_str!("migrations/0010_web_sockets.sql"),
];

/// Shared handle to the SQLite file. rusqlite's Connection is not Sync, so a
/// mutex serialises access — fine at this scale, where writes are a user
/// clicking Save rather than a server under load.
#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let connection = Connection::open(path).map_err(to_storage_error)?;
        Self::prepare(connection)
    }

    /// Used by tests: same migrations, no file on disk.
    pub fn open_in_memory() -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(to_storage_error)?;
        Self::prepare(connection)
    }

    fn prepare(connection: Connection) -> Result<Self, AppError> {
        // Without this, ON DELETE CASCADE is silently ignored: SQLite has
        // foreign keys off by default, per connection.
        //
        // secure_delete makes SQLite overwrite deleted content with zeros
        // instead of leaving it in free pages. Without it, a secret that was
        // blanked or re-sealed would still be readable in the raw file
        // (PLAN.md Phase 9). It is per connection, like foreign_keys.
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON;")
            .map_err(to_storage_error)?;
        let database = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        database.migrate()?;
        Ok(database)
    }

    fn migrate(&self) -> Result<(), AppError> {
        let mut guard = self.lock();
        let applied: u32 = guard
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(to_storage_error)?;
        refuse_newer_schema(applied)?;

        for (index, sql) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
            let version = index as u32 + 1;
            let transaction = guard.transaction().map_err(to_storage_error)?;
            transaction.execute_batch(sql).map_err(to_storage_error)?;
            // Parameters are not allowed in a PRAGMA, and `version` is a loop
            // counter rather than user input, so formatting it is safe here.
            transaction
                .pragma_update(None, "user_version", version)
                .map_err(to_storage_error)?;
            transaction.commit().map_err(to_storage_error)?;
        }
        Ok(())
    }

    /// A poisoned lock means another thread panicked mid-statement. Recovering
    /// it beats refusing every later query.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A database whose schema is newer than this build's migrations was written
/// by a newer version of the app. Opening it anyway used to succeed — the
/// loop above simply had nothing to run — and the older build would then
/// read and write tables it does not understand. Since migration 0010 that
/// means WebSocket rows listed as broken HTTP requests; in general it means
/// silent damage. Refusing is the only safe answer.
///
/// The installer already blocks downgrades (`allowDowngrades: false`), so
/// this is for the case it cannot see: an app-data folder copied from a
/// machine running a newer build.
fn refuse_newer_schema(applied: u32) -> Result<(), AppError> {
    let known = MIGRATIONS.len() as u32;
    if applied <= known {
        return Ok(());
    }
    Err(AppError::Storage(format!(
        "this database was written by a newer version of ResponderHTTP \
         (schema {applied}, this version knows {known}); update the app to open it"
    )))
}

pub fn to_storage_error(error: rusqlite::Error) -> AppError {
    AppError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_once_and_record_their_version() {
        let database = Database::open_in_memory().expect("should open");

        let version: u32 = database
            .lock()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version should be readable");

        assert_eq!(version, MIGRATIONS.len() as u32);
    }

    #[test]
    fn migrations_create_every_table() {
        let database = Database::open_in_memory().expect("should open");
        let guard = database.lock();
        let mut statement = guard
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("should prepare");
        let tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .expect("should query")
            .collect::<Result<_, _>>()
            .expect("should collect");

        assert!(tables.contains(&"collections".to_string()));
        assert!(tables.contains(&"folders".to_string()));
        assert!(tables.contains(&"requests".to_string()));
    }

    /// Phase 12 added the column by ALTER TABLE on three shipped tables. A
    /// migration that ran on one of them and not the others would leave the
    /// docs repository reading a column that is not there, so all three are
    /// asserted rather than one standing in for the rest.
    #[test]
    fn every_documentable_item_has_a_docs_column() {
        let database = Database::open_in_memory().expect("should open");
        let guard = database.lock();

        for table in ["collections", "folders", "requests"] {
            let count: i64 = guard
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = 'docs_md'",
                    [table],
                    |row| row.get(0),
                )
                .expect("should read the table's columns");

            assert_eq!(count, 1, "{table} should have a docs_md column");
        }
    }

    /// Migration 0010 on a database that already holds requests: every row
    /// that existed before it is an HTTP request, with no WebSocket blob.
    #[test]
    fn rows_written_before_web_sockets_existed_are_http_requests() {
        let connection = Connection::open_in_memory().expect("should open");
        for sql in &MIGRATIONS[..9] {
            connection
                .execute_batch(sql)
                .expect("old migrations should run");
        }
        connection
            .pragma_update(None, "user_version", 9)
            .expect("should set the version");
        connection
            .execute_batch(
                "INSERT INTO collections (id, name, created_at) VALUES ('col_1', 'C', 't');
                 INSERT INTO requests (
                     id, collection_id, folder_id, name, method, url, headers_json,
                     query_params_json, body_json, settings_json, created_at, updated_at
                 ) VALUES (
                     'req_1', 'col_1', NULL, 'Old', 'GET', 'https://x.test/', '[]',
                     '[]', '{\"kind\":\"None\"}', '{}', 't', 't'
                 );",
            )
            .expect("an old-shape row should insert");

        let database = Database::prepare(connection).expect("0010 should apply");
        let (kind, ws_json): (String, String) = database
            .lock()
            .query_row(
                "SELECT kind, ws_json FROM requests WHERE id = 'req_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("the old row should still be there");

        assert_eq!(kind, "http");
        assert_eq!(ws_json, "");
    }

    #[test]
    fn a_database_from_a_newer_version_is_refused() {
        let connection = Connection::open_in_memory().expect("should open");
        connection
            .pragma_update(None, "user_version", MIGRATIONS.len() as u32 + 1)
            .expect("should set the version");

        let error = match Database::prepare(connection) {
            Ok(_) => panic!("a newer schema must not open"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("newer version"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn a_database_at_exactly_this_version_opens() {
        assert!(refuse_newer_schema(MIGRATIONS.len() as u32).is_ok());
    }

    #[test]
    fn deleted_content_is_zeroed() {
        let database = Database::open_in_memory().expect("should open");

        let secure_delete: i64 = database
            .lock()
            .query_row("PRAGMA secure_delete", [], |row| row.get(0))
            .expect("should read the pragma");

        assert_eq!(secure_delete, 1);
    }
}

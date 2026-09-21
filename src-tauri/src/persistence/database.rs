// http_client/src-tauri/src/persistence/database.rs
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::domain::error::AppError;

/// Numbered, append-only. `PRAGMA user_version` records how many have run,
/// so adding a migration means appending to this list and nothing else.
const MIGRATIONS: [&str; 8] = [
    include_str!("migrations/0001_initial.sql"),
    include_str!("migrations/0002_request_auth.sql"),
    include_str!("migrations/0003_environments.sql"),
    include_str!("migrations/0004_cookies.sql"),
    include_str!("migrations/0005_history.sql"),
    include_str!("migrations/0006_examples.sql"),
    include_str!("migrations/0007_secrets.sql"),
    include_str!("migrations/0008_app_settings.sql"),
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

// http_client/src-tauri/src/persistence/repositories/app_settings.rs
use rusqlite::params;

use crate::domain::error::AppError;
use crate::domain::ports::AppSettingsRepository;
use crate::persistence::database::{to_storage_error, Database};

pub struct SqliteAppSettingsRepository {
    database: Database,
}

impl SqliteAppSettingsRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl AppSettingsRepository for SqliteAppSettingsRepository {
    fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare("SELECT value FROM app_settings WHERE key = ?1")
            .map_err(to_storage_error)?;
        let mut rows = statement.query(params![key]).map_err(to_storage_error)?;
        match rows.next().map_err(to_storage_error)? {
            Some(row) => Ok(Some(row.get(0).map_err(to_storage_error)?)),
            None => Ok(None),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        guard
            .execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(to_storage_error)?;
        Ok(())
    }
}

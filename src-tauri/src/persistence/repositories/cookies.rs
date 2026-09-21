// http_client/src-tauri/src/persistence/repositories/cookies.rs
use rusqlite::params;

use crate::domain::error::AppError;
use crate::domain::models::Cookie;
use crate::domain::ports::CookieRepository;
use crate::persistence::database::{to_storage_error, Database};

const SELECT_COLUMNS: &str =
    "domain, path, name, value, expires_at, secure, http_only, host_only, created_at";

pub struct SqliteCookieRepository {
    database: Database,
}

impl SqliteCookieRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl CookieRepository for SqliteCookieRepository {
    fn list(&self) -> Result<Vec<Cookie>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM cookies
                 ORDER BY domain COLLATE NOCASE, path, name COLLATE NOCASE"
            ))
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Cookie {
                    domain: row.get(0)?,
                    path: row.get(1)?,
                    name: row.get(2)?,
                    value: row.get(3)?,
                    expires_at: row.get(4)?,
                    secure: row.get(5)?,
                    http_only: row.get(6)?,
                    host_only: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)
    }

    fn upsert(&self, cookie: &Cookie) -> Result<(), AppError> {
        let guard = self.database.lock();
        guard
            .execute(
                // created_at is deliberately absent from the UPDATE: RFC 6265
                // section 5.3 step 11 keeps the original creation time when a
                // cookie is replaced, and that time decides send order.
                "INSERT INTO cookies (
                     domain, path, name, value, expires_at, secure, http_only,
                     host_only, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(domain, path, name) DO UPDATE SET
                     value      = excluded.value,
                     expires_at = excluded.expires_at,
                     secure     = excluded.secure,
                     http_only  = excluded.http_only,
                     host_only  = excluded.host_only",
                params![
                    cookie.domain,
                    cookie.path,
                    cookie.name,
                    cookie.value,
                    cookie.expires_at,
                    cookie.secure,
                    cookie.http_only,
                    cookie.host_only,
                    cookie.created_at
                ],
            )
            .map_err(to_storage_error)?;
        Ok(())
    }

    fn delete(&self, domain: &str, path: &str, name: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "DELETE FROM cookies WHERE domain = ?1 AND path = ?2 AND name = ?3",
                params![domain, path, name],
            )
            .map_err(to_storage_error)?;
        if changed == 0 {
            return Err(AppError::NotFound(format!(
                "cookie {name} for {domain}{path} does not exist"
            )));
        }
        Ok(())
    }

    fn clear(&self) -> Result<(), AppError> {
        let guard = self.database.lock();
        guard
            .execute("DELETE FROM cookies", [])
            .map_err(to_storage_error)?;
        Ok(())
    }

    fn clear_session(&self) -> Result<(), AppError> {
        let guard = self.database.lock();
        guard
            .execute("DELETE FROM cookies WHERE expires_at IS NULL", [])
            .map_err(to_storage_error)?;
        Ok(())
    }

    fn purge_expired(&self, now: u64) -> Result<(), AppError> {
        let guard = self.database.lock();
        guard
            .execute(
                "DELETE FROM cookies WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                params![now],
            )
            .map_err(to_storage_error)?;
        Ok(())
    }
}

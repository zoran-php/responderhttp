// http_client/src-tauri/src/persistence/repositories/saved_requests.rs
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Row};

use crate::domain::error::AppError;
use crate::domain::models::{HttpMethod, HttpRequest, SavedRequest};
use crate::domain::ports::{SavedRequestRepository, SecretCipher};
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::collections::missing_if_zero;
use crate::persistence::repositories::json::{self, SecretRead, SecretWrite};

/// The table name as it appears in a sealed secret's scope.
pub const SECRET_TABLE: &str = "requests";

const SELECT_COLUMNS: &str = "id, collection_id, folder_id, name, method, url,
     headers_json, query_params_json, body_json, settings_json, auth_json";

/// Auth secrets are sealed here, in the storage mapping, because it is the
/// one place every write passes through (PLAN.md Phase 9). The repository
/// holds a cipher, never a key.
pub struct SqliteSavedRequestRepository {
    database: Database,
    cipher: Arc<dyn SecretCipher>,
}

impl SqliteSavedRequestRepository {
    pub fn new(database: Database, cipher: Arc<dyn SecretCipher>) -> Self {
        Self { database, cipher }
    }

    fn read_row(&self, row: &Row<'_>) -> rusqlite::Result<Result<SavedRequest, AppError>> {
        read_row(row, self.cipher.as_ref())
    }
}

impl SavedRequestRepository for SqliteSavedRequestRepository {
    fn list_by_collection(&self, collection_id: &str) -> Result<Vec<SavedRequest>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM requests
                 WHERE collection_id = ?1
                 ORDER BY name COLLATE NOCASE"
            ))
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map(params![collection_id], |row| self.read_row(row))
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)?
            .into_iter()
            .collect()
    }

    fn get(&self, id: &str) -> Result<SavedRequest, AppError> {
        let guard = self.database.lock();
        let row = guard
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM requests WHERE id = ?1"),
                params![id],
                |row| self.read_row(row),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("request {id} does not exist"))
                }
                other => to_storage_error(other),
            })?;
        row
    }

    fn save(&self, saved: &SavedRequest) -> Result<(), AppError> {
        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;
        upsert_request(&transaction, saved, self.cipher.as_ref(), &now_iso8601())?;
        transaction.commit().map_err(to_storage_error)
    }

    fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "UPDATE requests SET name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, name.trim(), now_iso8601()],
            )
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "request")
    }

    fn move_to(&self, id: &str, folder_id: Option<&str>) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "UPDATE requests SET folder_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, folder_id, now_iso8601()],
            )
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "request")
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute("DELETE FROM requests WHERE id = ?1", params![id])
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "request")
    }
}

/// The one write for a saved request, shared with the import repository.
/// Auth secrets are sealed here, so no caller can store a request without
/// going through the cipher.
pub fn upsert_request(
    connection: &Connection,
    saved: &SavedRequest,
    cipher: &dyn SecretCipher,
    now: &str,
) -> Result<(), AppError> {
    let request = &saved.request;
    let headers = json::encode_pairs(&request.headers)?;
    let params = json::encode_pairs(&request.query_params)?;
    let body = json::encode_body(&request.body)?;
    let settings = json::encode_settings(&request.settings)?;
    let auth = json::encode_auth(
        &request.auth,
        SecretWrite::Seal {
            cipher,
            table: SECRET_TABLE,
            row_id: &saved.id,
        },
    )?;
    connection
        .execute(
            "INSERT INTO requests (
                 id, collection_id, folder_id, name, method, url,
                 headers_json, query_params_json, body_json, settings_json,
                 auth_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
             ON CONFLICT(id) DO UPDATE SET
                 collection_id     = excluded.collection_id,
                 folder_id         = excluded.folder_id,
                 name              = excluded.name,
                 method            = excluded.method,
                 url               = excluded.url,
                 headers_json      = excluded.headers_json,
                 query_params_json = excluded.query_params_json,
                 body_json         = excluded.body_json,
                 settings_json     = excluded.settings_json,
                 auth_json         = excluded.auth_json,
                 updated_at        = excluded.updated_at",
            params![
                saved.id,
                saved.collection_id,
                saved.folder_id,
                saved.name,
                request.method.as_str(),
                request.url,
                headers,
                params,
                body,
                settings,
                auth,
                now
            ],
        )
        .map_err(to_storage_error)?;
    Ok(())
}

/// Returns a Result inside the row mapper's Result: rusqlite reports column
/// errors, while a JSON column that will not parse is a domain-level problem.
fn read_row(
    row: &Row<'_>,
    cipher: &dyn SecretCipher,
) -> rusqlite::Result<Result<SavedRequest, AppError>> {
    let id: String = row.get(0)?;
    let collection_id: String = row.get(1)?;
    let folder_id: Option<String> = row.get(2)?;
    let name: String = row.get(3)?;
    let method: String = row.get(4)?;
    let url: String = row.get(5)?;
    let headers: String = row.get(6)?;
    let query_params: String = row.get(7)?;
    let body: String = row.get(8)?;
    let settings: String = row.get(9)?;
    let auth: String = row.get(10)?;

    Ok(build(
        id,
        collection_id,
        folder_id,
        name,
        method,
        url,
        headers,
        query_params,
        body,
        settings,
        auth,
        cipher,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build(
    id: String,
    collection_id: String,
    folder_id: Option<String>,
    name: String,
    method: String,
    url: String,
    headers: String,
    query_params: String,
    body: String,
    settings: String,
    auth: String,
    cipher: &dyn SecretCipher,
) -> Result<SavedRequest, AppError> {
    let method = HttpMethod::parse(&method)
        .ok_or_else(|| AppError::Storage(format!("stored request has unknown method {method}")))?;
    let (auth, secret_state) = json::decode_auth(
        &auth,
        SecretRead::Open {
            cipher,
            table: SECRET_TABLE,
            row_id: &id,
        },
    )?;
    Ok(SavedRequest {
        id,
        collection_id,
        folder_id,
        name,
        request: HttpRequest {
            method,
            url,
            headers: json::decode_pairs(&headers)?,
            query_params: json::decode_pairs(&query_params)?,
            body: json::decode_body(&body)?,
            auth,
            settings: json::decode_settings(&settings)?,
        },
        secret_state,
    })
}

/// Seconds-resolution UTC, formatted by hand rather than pulling in `chrono`
/// for one timestamp. Stored for ordering and debugging only.
pub fn now_iso8601() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let (year, month, day, hour, minute, second) = civil_from_unix(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days-to-calendar conversion (Howard Hinnant's civil_from_days), valid for
/// any date this app will ever store.
fn civil_from_unix(seconds: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let time_of_day = seconds % 86_400;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };

    (
        year,
        month,
        day,
        (time_of_day / 3600) as u32,
        ((time_of_day % 3600) / 60) as u32,
        (time_of_day % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_known_instant() {
        // 1 700 000 000 == 2023-11-14T22:13:20Z
        let (year, month, day, hour, minute, second) = civil_from_unix(1_700_000_000);

        assert_eq!(
            (year, month, day, hour, minute, second),
            (2023, 11, 14, 22, 13, 20)
        );
    }
}

// http_client/src-tauri/src/persistence/repositories/history.rs
use rusqlite::{params, Row};

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::models::{HistoryEntry, HttpMethod, HttpRequest, NewHistoryEntry};
use crate::domain::ports::HistoryRepository;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::json;
use crate::persistence::repositories::saved_requests::now_iso8601;

const SELECT_COLUMNS: &str = "id, sent_at, resolved_url, status, error_kind, duration_ms,
     method, url, headers_json, query_params_json, body_json, auth_json, settings_json";

pub struct SqliteHistoryRepository {
    database: Database,
}

impl SqliteHistoryRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl HistoryRepository for SqliteHistoryRepository {
    fn list(&self, limit: u32) -> Result<Vec<HistoryEntry>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM history ORDER BY sent_at DESC, id DESC LIMIT ?1"
            ))
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map(params![limit], read_row)
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)?
            .into_iter()
            .collect()
    }

    fn record(&self, entry: &NewHistoryEntry, keep: u32) -> Result<HistoryEntry, AppError> {
        let request = &entry.request;
        let headers = json::encode_pairs(&request.headers)?;
        let query_params = json::encode_pairs(&request.query_params)?;
        let body = json::encode_body(&request.body)?;
        let auth = json::encode_auth(&request.auth, json::SecretWrite::Strip)?;
        let settings = json::encode_settings(&request.settings)?;
        let id = new_id("his");
        let sent_at = now_iso8601();

        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;
        transaction
            .execute(
                "INSERT INTO history (
                     id, sent_at, resolved_url, status, error_kind, duration_ms,
                     method, url, headers_json, query_params_json, body_json,
                     auth_json, settings_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    id,
                    sent_at,
                    entry.resolved_url,
                    entry.status,
                    entry.error_kind,
                    entry.duration_ms,
                    request.method.as_str(),
                    request.url,
                    headers,
                    query_params,
                    body,
                    auth,
                    settings
                ],
            )
            .map_err(to_storage_error)?;

        // Trimmed on every insert rather than swept periodically: it keeps the
        // table bounded without anything having to remember to run, and old
        // credentials age out on their own.
        transaction
            .execute(
                "DELETE FROM history WHERE id NOT IN (
                     SELECT id FROM history ORDER BY sent_at DESC, id DESC LIMIT ?1
                 )",
                params![keep],
            )
            .map_err(to_storage_error)?;
        transaction.commit().map_err(to_storage_error)?;

        Ok(HistoryEntry {
            id,
            sent_at,
            resolved_url: entry.resolved_url.clone(),
            status: entry.status,
            error_kind: entry.error_kind.clone(),
            duration_ms: entry.duration_ms,
            request: entry.request.clone(),
        })
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute("DELETE FROM history WHERE id = ?1", params![id])
            .map_err(to_storage_error)?;
        if changed == 0 {
            return Err(AppError::NotFound(format!(
                "history entry {id} does not exist"
            )));
        }
        Ok(())
    }

    fn clear(&self) -> Result<(), AppError> {
        let guard = self.database.lock();
        guard
            .execute("DELETE FROM history", [])
            .map_err(to_storage_error)?;
        Ok(())
    }
}

/// Two layers of Result on purpose, as in the saved-request repository: a bad
/// column type is a rusqlite error, while a JSON column that will not parse is
/// a domain-level problem.
fn read_row(row: &Row<'_>) -> rusqlite::Result<Result<HistoryEntry, AppError>> {
    let id: String = row.get(0)?;
    let sent_at: String = row.get(1)?;
    let resolved_url: String = row.get(2)?;
    let status: Option<u16> = row.get(3)?;
    let error_kind: Option<String> = row.get(4)?;
    let duration_ms: u64 = row.get(5)?;
    let method: String = row.get(6)?;
    let url: String = row.get(7)?;
    let headers: String = row.get(8)?;
    let query_params: String = row.get(9)?;
    let body: String = row.get(10)?;
    let auth: String = row.get(11)?;
    let settings: String = row.get(12)?;

    Ok(build(
        id,
        sent_at,
        resolved_url,
        status,
        error_kind,
        duration_ms,
        method,
        url,
        headers,
        query_params,
        body,
        auth,
        settings,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build(
    id: String,
    sent_at: String,
    resolved_url: String,
    status: Option<u16>,
    error_kind: Option<String>,
    duration_ms: u64,
    method: String,
    url: String,
    headers: String,
    query_params: String,
    body: String,
    auth: String,
    settings: String,
) -> Result<HistoryEntry, AppError> {
    let method = HttpMethod::parse(&method)
        .ok_or_else(|| AppError::Storage(format!("stored history has unknown method {method}")))?;
    Ok(HistoryEntry {
        id,
        sent_at,
        resolved_url,
        status,
        error_kind,
        duration_ms,
        request: HttpRequest {
            method,
            url,
            headers: json::decode_pairs(&headers)?,
            query_params: json::decode_pairs(&query_params)?,
            body: json::decode_body(&body)?,
            auth: json::decode_auth(&auth, json::SecretRead::PlainOnly)?.0,
            settings: json::decode_settings(&settings)?,
        },
    })
}

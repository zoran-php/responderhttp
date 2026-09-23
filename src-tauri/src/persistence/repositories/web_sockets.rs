// http_client/src-tauri/src/persistence/repositories/web_sockets.rs
//
// Saved WebSocket requests (PLAN.md Phase 13d). They are rows in `requests`
// with `kind = 'websocket'` (migration 0010), so they sit in any collection
// or folder beside HTTP requests and inherit its cascades. Renaming, moving,
// deleting and documenting one go through SqliteSavedRequestRepository,
// which acts by id; this repository only lists, loads and saves the
// WebSocket's own shape.
//
// No cipher: a WebSocket request has no Auth tab, so nothing here is a
// secret by the rules in domain/secrets.rs. Handshake headers are stored in
// plain text, exactly as HTTP request headers are (PLAN.md Phase 9).
use rusqlite::{params, Connection, Row};

use crate::domain::error::AppError;
use crate::domain::models::{
    HttpMethod, RequestBody, RequestSettings, SavedWebSocket, WebSocketRequest,
};
use crate::domain::ports::WebSocketRepository;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::json;
use crate::persistence::repositories::request_kind::{refuse_other_kind, RequestKind};
use crate::persistence::repositories::saved_requests::now_iso8601;

const SELECT_COLUMNS: &str = "id, collection_id, folder_id, name, url, headers_json, ws_json";

pub struct SqliteWebSocketRepository {
    database: Database,
}

impl SqliteWebSocketRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl WebSocketRepository for SqliteWebSocketRepository {
    fn list_by_collection(&self, collection_id: &str) -> Result<Vec<SavedWebSocket>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM requests
                 WHERE collection_id = ?1 AND kind = ?2
                 ORDER BY name COLLATE NOCASE"
            ))
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map(
                params![collection_id, RequestKind::WebSocket.as_str()],
                read_row,
            )
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)?
            .into_iter()
            .collect()
    }

    fn get(&self, id: &str) -> Result<SavedWebSocket, AppError> {
        let guard = self.database.lock();
        guard
            .query_row(
                // An HTTP id is NotFound here, the mirror of the HTTP side.
                &format!("SELECT {SELECT_COLUMNS} FROM requests WHERE id = ?1 AND kind = ?2"),
                params![id, RequestKind::WebSocket.as_str()],
                read_row,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("WebSocket request {id} does not exist"))
                }
                other => to_storage_error(other),
            })?
    }

    fn save(&self, saved: &SavedWebSocket) -> Result<(), AppError> {
        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;
        upsert_web_socket(&transaction, saved, &now_iso8601())?;
        transaction.commit().map_err(to_storage_error)
    }
}

/// The HTTP-only columns are NOT NULL, so a WebSocket row fills them with the
/// values an empty HTTP request would have. They are never read back for this
/// kind; they exist so the row is well-formed. `method` is GET because the
/// handshake is one. `auth_json` and `docs_md` are left to their column
/// defaults on insert and untouched on update, which is also what keeps Save
/// from wiping a WebSocket's documentation.
fn upsert_web_socket(
    connection: &Connection,
    saved: &SavedWebSocket,
    now: &str,
) -> Result<(), AppError> {
    let request = &saved.request;
    let headers = json::encode_pairs(&request.headers)?;
    let ws = json::encode_web_socket(&request.settings, &saved.draft)?;
    let no_params = json::encode_pairs(&[])?;
    let no_body = json::encode_body(&RequestBody::None)?;
    let http_settings = json::encode_settings(&RequestSettings::default())?;

    let changed = connection
        .execute(
            "INSERT INTO requests (
                 id, collection_id, folder_id, name, method, url,
                 headers_json, query_params_json, body_json, settings_json,
                 ws_json, created_at, updated_at, kind
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 collection_id = excluded.collection_id,
                 folder_id     = excluded.folder_id,
                 name          = excluded.name,
                 url           = excluded.url,
                 headers_json  = excluded.headers_json,
                 ws_json       = excluded.ws_json,
                 updated_at    = excluded.updated_at
             WHERE requests.kind = excluded.kind",
            params![
                saved.id,
                saved.collection_id,
                saved.folder_id,
                saved.name,
                HttpMethod::Get.as_str(),
                request.url,
                headers,
                no_params,
                no_body,
                http_settings,
                ws,
                now,
                RequestKind::WebSocket.as_str()
            ],
        )
        .map_err(to_storage_error)?;
    refuse_other_kind(changed, &saved.id, RequestKind::WebSocket)
}

/// Returns a Result inside the row mapper's Result, as saved_requests.rs
/// does: rusqlite reports column errors, while a JSON column that will not
/// parse is a domain-level problem.
fn read_row(row: &Row<'_>) -> rusqlite::Result<Result<SavedWebSocket, AppError>> {
    let id: String = row.get(0)?;
    let collection_id: String = row.get(1)?;
    let folder_id: Option<String> = row.get(2)?;
    let name: String = row.get(3)?;
    let url: String = row.get(4)?;
    let headers: String = row.get(5)?;
    let ws: String = row.get(6)?;

    Ok(build(
        id,
        collection_id,
        folder_id,
        name,
        url,
        &headers,
        &ws,
    ))
}

fn build(
    id: String,
    collection_id: String,
    folder_id: Option<String>,
    name: String,
    url: String,
    headers: &str,
    ws: &str,
) -> Result<SavedWebSocket, AppError> {
    let (settings, draft) = json::decode_web_socket(ws)?;
    Ok(SavedWebSocket {
        id,
        collection_id,
        folder_id,
        name,
        request: WebSocketRequest {
            url,
            headers: json::decode_pairs(headers)?,
            settings,
        },
        draft,
    })
}

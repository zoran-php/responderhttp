// http_client/src-tauri/src/persistence/repositories/grpc_requests.rs
//
// Saved gRPC requests (PLAN-GRPC.md 16g-2). Rows in `requests` with
// `kind = 'grpc'`, so they sit in any collection or folder and inherit its
// cascades. Renaming, moving, deleting and documenting one go through
// SqliteSavedRequestRepository, which acts by id; this repository lists,
// loads and saves the gRPC shape.
//
// The target is in `url`, metadata in `headers_json` and auth in `auth_json`,
// so auth secrets are sealed by the same path as HTTP's (CLAUDE.md §5). The
// rest is in `grpc_json` (json::StoredGrpc).
use std::sync::Arc;

use rusqlite::{params, Connection, Row};

use crate::domain::error::AppError;
use crate::domain::models::{
    GrpcRequestDraft, GrpcSchemaRef, HttpMethod, RequestBody, RequestSettings, SavedGrpcRequest,
};
use crate::domain::ports::{GrpcRequestRepository, SecretCipher};
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::json::{self, SecretRead, SecretWrite};
use crate::persistence::repositories::request_kind::{refuse_other_kind, RequestKind};
use crate::persistence::repositories::saved_requests::{now_iso8601, SECRET_TABLE};

const SELECT_COLUMNS: &str =
    "id, collection_id, folder_id, name, url, headers_json, auth_json, grpc_json";

pub struct SqliteGrpcRequestRepository {
    database: Database,
    cipher: Arc<dyn SecretCipher>,
}

impl SqliteGrpcRequestRepository {
    pub fn new(database: Database, cipher: Arc<dyn SecretCipher>) -> Self {
        Self { database, cipher }
    }
}

impl GrpcRequestRepository for SqliteGrpcRequestRepository {
    fn list_by_collection(&self, collection_id: &str) -> Result<Vec<SavedGrpcRequest>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM requests
                 WHERE collection_id = ?1 AND kind = ?2
                 ORDER BY name COLLATE NOCASE"
            ))
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map(params![collection_id, RequestKind::Grpc.as_str()], |row| {
                read_row(row, self.cipher.as_ref())
            })
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)?
            .into_iter()
            .collect()
    }

    fn get(&self, id: &str) -> Result<SavedGrpcRequest, AppError> {
        let guard = self.database.lock();
        guard
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM requests WHERE id = ?1 AND kind = ?2"),
                params![id, RequestKind::Grpc.as_str()],
                |row| read_row(row, self.cipher.as_ref()),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("gRPC request {id} does not exist"))
                }
                other => to_storage_error(other),
            })?
    }

    fn save(&self, saved: &SavedGrpcRequest) -> Result<(), AppError> {
        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;
        if let GrpcSchemaRef::Library(schema_id) = &saved.request.schema {
            require_schema_in_library(&transaction, schema_id)?;
        }
        upsert_grpc_request(&transaction, saved, self.cipher.as_ref(), &now_iso8601())?;
        transaction.commit().map_err(to_storage_error)
    }
}

/// Checked in the same transaction as the write, so a schema cannot be
/// deleted between the check and the save. A schema loaded only for this
/// session would be gone on the next start, leaving a request that cannot
/// be opened, so it has to be saved to the library first.
fn require_schema_in_library(connection: &Connection, schema_id: &str) -> Result<(), AppError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM proto_schemas WHERE id = ?1",
            params![schema_id],
            |row| row.get(0),
        )
        .map_err(to_storage_error)?;
    if count == 0 {
        return Err(AppError::InvalidRequest(format!(
            "schema {schema_id} is not in the schema library; save it there before saving the request"
        )));
    }
    Ok(())
}

/// The HTTP-only columns are NOT NULL, so a gRPC row fills them with what an
/// empty HTTP request would have; they are never read back for this kind.
/// `method` is POST, which is what a gRPC call is on the wire. `docs_md` is
/// left to its default on insert and untouched on update, so Save keeps a
/// request's documentation.
fn upsert_grpc_request(
    connection: &Connection,
    saved: &SavedGrpcRequest,
    cipher: &dyn SecretCipher,
    now: &str,
) -> Result<(), AppError> {
    let request = &saved.request;
    let metadata = json::encode_pairs(&request.metadata)?;
    let auth = json::encode_auth(
        &request.auth,
        SecretWrite::Seal {
            cipher,
            table: SECRET_TABLE,
            row_id: &saved.id,
        },
    )?;
    let grpc = json::encode_grpc(request)?;
    let no_params = json::encode_pairs(&[])?;
    let no_body = json::encode_body(&RequestBody::None)?;
    let http_settings = json::encode_settings(&RequestSettings::default())?;

    let changed = connection
        .execute(
            "INSERT INTO requests (
                 id, collection_id, folder_id, name, method, url,
                 headers_json, query_params_json, body_json, settings_json,
                 auth_json, grpc_json, created_at, updated_at, kind
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                 collection_id = excluded.collection_id,
                 folder_id     = excluded.folder_id,
                 name          = excluded.name,
                 url           = excluded.url,
                 headers_json  = excluded.headers_json,
                 auth_json     = excluded.auth_json,
                 grpc_json     = excluded.grpc_json,
                 updated_at    = excluded.updated_at
             WHERE requests.kind = excluded.kind",
            params![
                saved.id,
                saved.collection_id,
                saved.folder_id,
                saved.name,
                HttpMethod::Post.as_str(),
                request.url,
                metadata,
                no_params,
                no_body,
                http_settings,
                auth,
                grpc,
                now,
                RequestKind::Grpc.as_str()
            ],
        )
        .map_err(to_storage_error)?;
    refuse_other_kind(connection, changed, &saved.id, RequestKind::Grpc)
}

/// The columns of one row, before the JSON in them is read.
struct StoredRow {
    id: String,
    collection_id: String,
    folder_id: Option<String>,
    name: String,
    url: String,
    metadata: String,
    auth: String,
    grpc: String,
}

/// A Result inside the row mapper's Result, as saved_requests.rs does:
/// rusqlite reports column errors, while a JSON column that will not parse
/// is a domain-level problem.
fn read_row(
    row: &Row<'_>,
    cipher: &dyn SecretCipher,
) -> rusqlite::Result<Result<SavedGrpcRequest, AppError>> {
    let stored = StoredRow {
        id: row.get(0)?,
        collection_id: row.get(1)?,
        folder_id: row.get(2)?,
        name: row.get(3)?,
        url: row.get(4)?,
        metadata: row.get(5)?,
        auth: row.get(6)?,
        grpc: row.get(7)?,
    };
    Ok(build(stored, cipher))
}

fn build(stored: StoredRow, cipher: &dyn SecretCipher) -> Result<SavedGrpcRequest, AppError> {
    let (auth, secret_state) = json::decode_auth(
        &stored.auth,
        SecretRead::Open {
            cipher,
            table: SECRET_TABLE,
            row_id: &stored.id,
        },
    )?;
    let grpc = json::decode_grpc(&stored.grpc)?;
    Ok(SavedGrpcRequest {
        id: stored.id,
        collection_id: stored.collection_id,
        folder_id: stored.folder_id,
        name: stored.name,
        request: GrpcRequestDraft {
            url: stored.url,
            tls: grpc.tls,
            method_path: grpc.method_path,
            schema: grpc.schema,
            metadata: json::decode_pairs(&stored.metadata)?,
            auth,
            message: grpc.message,
            settings: grpc.settings,
        },
        secret_state,
    })
}

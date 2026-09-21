// http_client/src-tauri/src/persistence/repositories/examples.rs
use rusqlite::{params, Connection, Row};

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::models::{Example, ExampleSummary, HttpMethod, HttpRequest, NewExample};
use crate::domain::ports::ExampleRepository;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::collections::missing_if_zero;
use crate::persistence::repositories::json;
use crate::persistence::repositories::saved_requests::now_iso8601;

const SELECT_COLUMNS: &str = "id, request_id, name, created_at, method, url,
     headers_json, query_params_json, body_json, auth_json, settings_json,
     status, response_headers_json, response_body";

pub struct SqliteExampleRepository {
    database: Database,
}

impl SqliteExampleRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl ExampleRepository for SqliteExampleRepository {
    fn list_summaries_by_collection(
        &self,
        collection_id: &str,
    ) -> Result<Vec<ExampleSummary>, AppError> {
        let guard = self.database.lock();
        // Joined through requests rather than denormalising collection_id
        // onto examples: a request can be moved between folders, and one
        // copy of that fact is one chance to get it wrong.
        //
        // Creation order, not alphabetical: examples read as a sequence of
        // cases under a request, and renaming one should not reshuffle them.
        let mut statement = guard
            .prepare(
                "SELECT e.id, e.request_id, e.name, e.status
                 FROM examples e
                 JOIN requests r ON r.id = e.request_id
                 WHERE r.collection_id = ?1
                 ORDER BY e.created_at, e.id",
            )
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map(params![collection_id], |row| {
                Ok(ExampleSummary {
                    id: row.get(0)?,
                    request_id: row.get(1)?,
                    name: row.get(2)?,
                    status: row.get(3)?,
                })
            })
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)
    }

    fn get(&self, id: &str) -> Result<Example, AppError> {
        let guard = self.database.lock();
        guard
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM examples WHERE id = ?1"),
                params![id],
                read_row,
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("example {id} does not exist"))
                }
                other => to_storage_error(other),
            })?
    }

    fn create(&self, example: &NewExample) -> Result<Example, AppError> {
        let id = new_id("exa");
        let created_at = now_iso8601();

        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;
        // Checked rather than left to the foreign key, so a stale request id
        // reports NotFound instead of an opaque constraint failure.
        let exists: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM requests WHERE id = ?1",
                params![example.request_id],
                |row| row.get(0),
            )
            .map_err(to_storage_error)?;
        if exists == 0 {
            return Err(AppError::NotFound(format!(
                "request {} does not exist",
                example.request_id
            )));
        }
        insert_example(&transaction, &id, &created_at, example)?;
        transaction.commit().map_err(to_storage_error)?;

        let request = &example.request;
        Ok(Example {
            id,
            request_id: example.request_id.clone(),
            name: example.name.clone(),
            created_at,
            request: request.clone(),
            status: example.status,
            response_headers: example.response_headers.clone(),
            response_body: example.response_body.clone(),
        })
    }

    fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "UPDATE examples SET name = ?2 WHERE id = ?1",
                params![id, name],
            )
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "example")
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute("DELETE FROM examples WHERE id = ?1", params![id])
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "example")
    }
}

/// Two layers of Result, as in the other repositories: a bad column type is a
/// rusqlite error, a JSON column that will not parse is a domain problem.
/// The one INSERT for an example, shared with the import repository. The
/// request snapshot never keeps a literal secret (PLAN.md Phase 9).
pub fn insert_example(
    connection: &Connection,
    id: &str,
    created_at: &str,
    example: &NewExample,
) -> Result<(), AppError> {
    let request = &example.request;
    let headers = json::encode_pairs(&request.headers)?;
    let query_params = json::encode_pairs(&request.query_params)?;
    let body = json::encode_body(&request.body)?;
    let auth = json::encode_auth(&request.auth, json::SecretWrite::Strip)?;
    let settings = json::encode_settings(&request.settings)?;
    let response_headers = json::encode_pairs(&example.response_headers)?;
    connection
        .execute(
            "INSERT INTO examples (
                 id, request_id, name, created_at, method, url,
                 headers_json, query_params_json, body_json, auth_json,
                 settings_json, status, response_headers_json, response_body
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id,
                example.request_id,
                example.name,
                created_at,
                request.method.as_str(),
                request.url,
                headers,
                query_params,
                body,
                auth,
                settings,
                example.status,
                response_headers,
                example.response_body
            ],
        )
        .map_err(to_storage_error)?;
    Ok(())
}

fn read_row(row: &Row<'_>) -> rusqlite::Result<Result<Example, AppError>> {
    let id: String = row.get(0)?;
    let request_id: String = row.get(1)?;
    let name: String = row.get(2)?;
    let created_at: String = row.get(3)?;
    let method: String = row.get(4)?;
    let url: String = row.get(5)?;
    let headers: String = row.get(6)?;
    let query_params: String = row.get(7)?;
    let body: String = row.get(8)?;
    let auth: String = row.get(9)?;
    let settings: String = row.get(10)?;
    let status: u16 = row.get(11)?;
    let response_headers: String = row.get(12)?;
    let response_body: String = row.get(13)?;

    Ok(build(Columns {
        id,
        request_id,
        name,
        created_at,
        method,
        url,
        headers,
        query_params,
        body,
        auth,
        settings,
        status,
        response_headers,
        response_body,
    }))
}

/// Fourteen columns is past the point where positional arguments are
/// readable, so they arrive named.
struct Columns {
    id: String,
    request_id: String,
    name: String,
    created_at: String,
    method: String,
    url: String,
    headers: String,
    query_params: String,
    body: String,
    auth: String,
    settings: String,
    status: u16,
    response_headers: String,
    response_body: String,
}

fn build(columns: Columns) -> Result<Example, AppError> {
    let method = HttpMethod::parse(&columns.method).ok_or_else(|| {
        AppError::Storage(format!(
            "stored example has unknown method {}",
            columns.method
        ))
    })?;
    Ok(Example {
        id: columns.id,
        request_id: columns.request_id,
        name: columns.name,
        created_at: columns.created_at,
        request: HttpRequest {
            method,
            url: columns.url,
            headers: json::decode_pairs(&columns.headers)?,
            query_params: json::decode_pairs(&columns.query_params)?,
            body: json::decode_body(&columns.body)?,
            auth: json::decode_auth(&columns.auth, json::SecretRead::PlainOnly)?.0,
            settings: json::decode_settings(&columns.settings)?,
        },
        status: columns.status,
        response_headers: json::decode_pairs(&columns.response_headers)?,
        response_body: columns.response_body,
    })
}

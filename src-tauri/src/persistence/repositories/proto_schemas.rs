// http_client/src-tauri/src/persistence/repositories/proto_schemas.rs
//
// The schema library in SQLite (migration 0011, PLAN-GRPC.md 16g).
use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension};

use crate::domain::error::AppError;
use crate::domain::models::{ProtoSchemaSummary, SchemaOrigin, StoredProtoSchema};
use crate::domain::ports::ProtoSchemaRepository;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::collections::missing_if_zero;
use crate::persistence::repositories::saved_requests::now_iso8601;

const WHAT: &str = "schema";

/// The two values of `proto_schemas.origin`, written only from here.
fn origin_to_str(origin: SchemaOrigin) -> &'static str {
    match origin {
        SchemaOrigin::Import => "import",
        SchemaOrigin::Reflection => "reflection",
    }
}

fn origin_from_str(value: &str) -> Result<SchemaOrigin, AppError> {
    match value {
        "import" => Ok(SchemaOrigin::Import),
        "reflection" => Ok(SchemaOrigin::Reflection),
        other => Err(AppError::Storage(format!(
            "unknown schema origin {other:?}"
        ))),
    }
}

pub struct SqliteProtoSchemaRepository {
    database: Database,
}

impl SqliteProtoSchemaRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl ProtoSchemaRepository for SqliteProtoSchemaRepository {
    fn list(&self) -> Result<Vec<ProtoSchemaSummary>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(
                "SELECT id, name, origin, updated_at FROM proto_schemas
                 ORDER BY name COLLATE NOCASE, id",
            )
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(to_storage_error)?;
        let mut summaries = Vec::new();
        for row in rows {
            let (id, name, origin, updated_at) = row.map_err(to_storage_error)?;
            summaries.push(ProtoSchemaSummary {
                id,
                name,
                origin: origin_from_str(&origin)?,
                updated_at,
            });
        }
        Ok(summaries)
    }

    fn get(&self, id: &str) -> Result<StoredProtoSchema, AppError> {
        let guard = self.database.lock();
        let row = guard
            .query_row(
                "SELECT id, name, origin, descriptor_set, sources_json, updated_at
                 FROM proto_schemas WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(to_storage_error)?;
        let (id, name, origin, encoded, sources_json, updated_at) =
            row.ok_or_else(|| AppError::NotFound(format!("{WHAT} {id} does not exist")))?;
        let sources: BTreeMap<String, String> =
            serde_json::from_str(&sources_json).map_err(|error| {
                AppError::Storage(format!("schema {id}: unreadable sources: {error}"))
            })?;
        Ok(StoredProtoSchema {
            id,
            name,
            origin: origin_from_str(&origin)?,
            encoded,
            sources,
            updated_at,
        })
    }

    fn save(&self, schema: &StoredProtoSchema) -> Result<(), AppError> {
        let sources_json = serde_json::to_string(&schema.sources)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let now = now_iso8601();
        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;
        transaction
            .execute(
                "INSERT INTO proto_schemas
                     (id, name, origin, descriptor_set, sources_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                     name = excluded.name,
                     origin = excluded.origin,
                     descriptor_set = excluded.descriptor_set,
                     sources_json = excluded.sources_json,
                     updated_at = excluded.updated_at",
                params![
                    schema.id,
                    schema.name,
                    origin_to_str(schema.origin),
                    schema.encoded,
                    sources_json,
                    now
                ],
            )
            .map_err(to_storage_error)?;
        transaction.commit().map_err(to_storage_error)
    }

    fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "UPDATE proto_schemas SET name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, name, now_iso8601()],
            )
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, WHAT)
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute("DELETE FROM proto_schemas WHERE id = ?1", params![id])
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, WHAT)
    }

    /// Only gRPC rows have a non-empty `grpc_json`, so no kind filter is
    /// needed. SQLite's JSON functions are part of the bundled build.
    fn users(&self, id: &str) -> Result<Vec<String>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(
                "SELECT name FROM requests
                 WHERE grpc_json <> '' AND json_extract(grpc_json, '$.schema_id') = ?1
                 ORDER BY name COLLATE NOCASE",
            )
            .map_err(to_storage_error)?;
        let names = statement
            .query_map(params![id], |row| row.get::<_, String>(0))
            .map_err(to_storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)?;
        Ok(names)
    }
}

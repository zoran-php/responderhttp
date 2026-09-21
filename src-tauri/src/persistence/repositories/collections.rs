// http_client/src-tauri/src/persistence/repositories/collections.rs
use rusqlite::{params, Connection};

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::models::Collection;
use crate::domain::ports::CollectionRepository;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::docs::{read_docs, write_docs, DocsTable};
use crate::persistence::repositories::saved_requests::now_iso8601;

pub struct SqliteCollectionRepository {
    database: Database,
}

impl SqliteCollectionRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl CollectionRepository for SqliteCollectionRepository {
    fn list(&self) -> Result<Vec<Collection>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare("SELECT id, name FROM collections ORDER BY name COLLATE NOCASE")
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Collection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)
    }

    fn create(&self, name: &str) -> Result<Collection, AppError> {
        let collection = Collection {
            id: new_id("col"),
            name: name.trim().to_string(),
        };
        let guard = self.database.lock();
        insert_collection(&guard, &collection, &now_iso8601())?;
        Ok(collection)
    }

    fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "UPDATE collections SET name = ?2 WHERE id = ?1",
                params![id, name.trim()],
            )
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "collection")
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute("DELETE FROM collections WHERE id = ?1", params![id])
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "collection")
    }

    fn docs(&self, id: &str) -> Result<String, AppError> {
        let guard = self.database.lock();
        read_docs(&guard, DocsTable::Collections, id)
    }

    fn set_docs(&self, id: &str, markdown: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        write_docs(&guard, DocsTable::Collections, id, markdown)
    }
}

/// The one INSERT for a collection, shared with the import repository so an
/// imported collection is written exactly like a created one.
pub fn insert_collection(
    connection: &Connection,
    collection: &Collection,
    created_at: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT INTO collections (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![collection.id, collection.name, created_at],
        )
        .map_err(to_storage_error)?;
    Ok(())
}

/// A no-op UPDATE or DELETE means the id was wrong, which the caller should
/// hear about rather than assume success.
pub fn missing_if_zero(changed: usize, id: &str, what: &str) -> Result<(), AppError> {
    if changed == 0 {
        return Err(AppError::NotFound(format!("{what} {id} does not exist")));
    }
    Ok(())
}

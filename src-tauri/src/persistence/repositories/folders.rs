// http_client/src-tauri/src/persistence/repositories/folders.rs
use rusqlite::{params, Connection};

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::models::Folder;
use crate::domain::ports::FolderRepository;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::collections::missing_if_zero;
use crate::persistence::repositories::docs::{read_docs, write_docs, DocsTable};
use crate::persistence::repositories::saved_requests::now_iso8601;

pub struct SqliteFolderRepository {
    database: Database,
}

impl SqliteFolderRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl FolderRepository for SqliteFolderRepository {
    fn list_by_collection(&self, collection_id: &str) -> Result<Vec<Folder>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(
                "SELECT id, collection_id, parent_folder_id, name
                 FROM folders
                 WHERE collection_id = ?1
                 ORDER BY name COLLATE NOCASE",
            )
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map(params![collection_id], |row| {
                Ok(Folder {
                    id: row.get(0)?,
                    collection_id: row.get(1)?,
                    parent_folder_id: row.get(2)?,
                    name: row.get(3)?,
                })
            })
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)
    }

    fn create(
        &self,
        collection_id: &str,
        parent_folder_id: Option<&str>,
        name: &str,
    ) -> Result<Folder, AppError> {
        let folder = Folder {
            id: new_id("fld"),
            collection_id: collection_id.to_string(),
            parent_folder_id: parent_folder_id.map(str::to_string),
            name: name.trim().to_string(),
        };
        let guard = self.database.lock();
        insert_folder(&guard, &folder, &now_iso8601())?;
        Ok(folder)
    }

    fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "UPDATE folders SET name = ?2 WHERE id = ?1",
                params![id, name.trim()],
            )
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "folder")
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute("DELETE FROM folders WHERE id = ?1", params![id])
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "folder")
    }

    fn docs(&self, id: &str) -> Result<String, AppError> {
        let guard = self.database.lock();
        read_docs(&guard, DocsTable::Folders, id)
    }

    fn set_docs(&self, id: &str, markdown: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        write_docs(&guard, DocsTable::Folders, id, markdown)
    }
}

/// The one INSERT for a folder, shared with the import repository.
pub fn insert_folder(
    connection: &Connection,
    folder: &Folder,
    created_at: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT INTO folders (id, collection_id, parent_folder_id, name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                folder.id,
                folder.collection_id,
                folder.parent_folder_id,
                folder.name,
                created_at
            ],
        )
        .map_err(to_storage_error)?;
    Ok(())
}

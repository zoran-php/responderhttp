// http_client/src-tauri/src/persistence/repositories/docs.rs
//
// The two statements behind every item's Markdown documentation
// (PLAN.md Phase 12).
//
// Collections, folders and requests each carry a `docs_md` column, and
// reading or writing it is the same statement three times over with one word
// changed. Three copies would drift, so the statement lives here once and the
// three repositories call it with their own table (CLAUDE.md section 7, DRY).
//
// **On the table name in the SQL.** Section 5 says every query is
// parameterised and that string-concatenated SQL is a bug. A table name
// cannot be bound — SQLite allows parameters for values, not identifiers — so
// it is formatted in. What makes that safe here is `DocsTable`: a closed enum
// whose three variants map to three `&'static str` literals written in this
// file. No caller can hand in a name, and no user input reaches the format.
// The id and the Markdown are bound as parameters, as always.
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::AppError;
use crate::persistence::database::to_storage_error;

/// Which of the three documentable items a statement is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocsTable {
    Collections,
    Folders,
    Requests,
}

impl DocsTable {
    /// The table's name. A literal per variant, never a caller's string.
    fn table(self) -> &'static str {
        match self {
            Self::Collections => "collections",
            Self::Folders => "folders",
            Self::Requests => "requests",
        }
    }

    /// What to call the item in a not-found message.
    fn item(self) -> &'static str {
        match self {
            Self::Collections => "collection",
            Self::Folders => "folder",
            Self::Requests => "request",
        }
    }
}

/// Reads an item's documentation. Empty when it has none — the column is
/// NOT NULL DEFAULT '', so "no documentation" and "empty documentation" are
/// deliberately the same state and no caller has to tell them apart.
pub fn read_docs(connection: &Connection, table: DocsTable, id: &str) -> Result<String, AppError> {
    connection
        .query_row(
            &format!("SELECT docs_md FROM {} WHERE id = ?1", table.table()),
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_storage_error)?
        .ok_or_else(|| AppError::NotFound(format!("{} {id} does not exist", table.item())))
}

/// Replaces an item's documentation. A missing id is reported rather than
/// silently doing nothing, the way every other update in this layer behaves.
pub fn write_docs(
    connection: &Connection,
    table: DocsTable,
    id: &str,
    markdown: &str,
) -> Result<(), AppError> {
    let changed = connection
        .execute(
            &format!("UPDATE {} SET docs_md = ?2 WHERE id = ?1", table.table()),
            params![id, markdown],
        )
        .map_err(to_storage_error)?;

    super::collections::missing_if_zero(changed, id, table.item())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names are what reaches the SQL, so a typo here is a runtime error
    /// in a statement no compiler checks.
    #[test]
    fn each_variant_names_its_table_and_its_item() {
        assert_eq!(DocsTable::Collections.table(), "collections");
        assert_eq!(DocsTable::Folders.table(), "folders");
        assert_eq!(DocsTable::Requests.table(), "requests");

        assert_eq!(DocsTable::Collections.item(), "collection");
        assert_eq!(DocsTable::Folders.item(), "folder");
        assert_eq!(DocsTable::Requests.item(), "request");
    }
}

// http_client/src-tauri/src/persistence/repositories/environments.rs
use std::sync::Arc;

use rusqlite::{params, Connection};

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::models::{Environment, EnvironmentVariable};
use crate::domain::ports::{EnvironmentRepository, SecretCipher};
use crate::domain::secrets::{secret_scope, SecretState};
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::collections::missing_if_zero;
use crate::persistence::repositories::json;
use crate::persistence::repositories::saved_requests::now_iso8601;

const SECRET_TABLE: &str = "environment_variables";
const SECRET_FIELD: &str = "value";

pub struct SqliteEnvironmentRepository {
    database: Database,
    cipher: Arc<dyn SecretCipher>,
}

impl SqliteEnvironmentRepository {
    pub fn new(database: Database, cipher: Arc<dyn SecretCipher>) -> Self {
        Self { database, cipher }
    }
}

/// A variable's scope is its environment and its position. That is only safe
/// because `set_variables` rewrites — and so re-seals — every row on every
/// save; if it ever updates rows in place, the scope must change with it.
fn variable_scope(environment_id: &str, position: i64) -> String {
    secret_scope(
        SECRET_TABLE,
        &format!("{environment_id}/{position}"),
        SECRET_FIELD,
    )
}

impl EnvironmentRepository for SqliteEnvironmentRepository {
    fn list(&self) -> Result<Vec<Environment>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare("SELECT id, name FROM environments ORDER BY name COLLATE NOCASE")
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Environment {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })
            .map_err(to_storage_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)
    }

    fn create(&self, name: &str) -> Result<Environment, AppError> {
        let environment = Environment {
            id: new_id("env"),
            name: name.trim().to_string(),
        };
        let guard = self.database.lock();
        insert_environment(&guard, &environment, &now_iso8601())?;
        Ok(environment)
    }

    fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute(
                "UPDATE environments SET name = ?2 WHERE id = ?1",
                params![id, name.trim()],
            )
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "environment")
    }

    fn delete(&self, id: &str) -> Result<(), AppError> {
        let guard = self.database.lock();
        let changed = guard
            .execute("DELETE FROM environments WHERE id = ?1", params![id])
            .map_err(to_storage_error)?;
        missing_if_zero(changed, id, "environment")
    }

    fn variables(&self, environment_id: &str) -> Result<Vec<EnvironmentVariable>, AppError> {
        let guard = self.database.lock();
        let mut statement = guard
            .prepare(
                "SELECT position, name, value, secret, value_sealed FROM environment_variables
                 WHERE environment_id = ?1
                 ORDER BY position",
            )
            .map_err(to_storage_error)?;
        let rows = statement
            .query_map(params![environment_id], |row| {
                let position: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                let value: String = row.get(2)?;
                let secret: bool = row.get(3)?;
                let sealed: Option<Vec<u8>> = row.get(4)?;
                Ok((position, name, value, secret, sealed))
            })
            .map_err(to_storage_error)?;
        let rows = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_storage_error)?;

        Ok(rows
            .into_iter()
            .map(|(position, name, value, secret, sealed)| {
                if !secret {
                    return EnvironmentVariable::plain(name, value);
                }
                let (value, state) = match sealed {
                    // An empty secret is stored without a sealed value.
                    None => (String::new(), SecretState::Ok),
                    Some(bytes) => json::open_value(
                        self.cipher.as_ref(),
                        &variable_scope(environment_id, position),
                        &bytes,
                    ),
                };
                EnvironmentVariable {
                    name,
                    value,
                    secret: true,
                    state,
                }
            })
            .collect())
    }

    fn set_variables(
        &self,
        environment_id: &str,
        variables: &[EnvironmentVariable],
    ) -> Result<(), AppError> {
        // Sealed before the lock is taken: sealing is the one step here that
        // can fail for a reason that has nothing to do with the database,
        // and nothing should be deleted until every row is ready to write.
        let prepared = prepare_variables(self.cipher.as_ref(), environment_id, variables)?;

        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;

        // An unknown id would otherwise look like success whenever the set
        // being written is empty, since the DELETE below matches nothing.
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM environments WHERE id = ?1",
                params![environment_id],
                |row| row.get(0),
            )
            .map_err(to_storage_error)?;
        if existing == 0 {
            return Err(AppError::NotFound(format!(
                "environment {environment_id} does not exist"
            )));
        }

        // Replaced wholesale: the editor always sends the complete set, and
        // diffing rows would be more code for no visible difference here.
        transaction
            .execute(
                "DELETE FROM environment_variables WHERE environment_id = ?1",
                params![environment_id],
            )
            .map_err(to_storage_error)?;

        insert_variables(&transaction, environment_id, &prepared)?;
        transaction.commit().map_err(to_storage_error)
    }
}

/// The one INSERT for an environment, shared with the import repository.
pub fn insert_environment(
    connection: &Connection,
    environment: &Environment,
    created_at: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT INTO environments (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![environment.id, environment.name, created_at],
        )
        .map_err(to_storage_error)?;
    Ok(())
}

/// A variable ready to write: secret values already sealed for their row.
pub struct PreparedVariable {
    position: i64,
    name: String,
    value: String,
    secret: bool,
    sealed: Option<Vec<u8>>,
}

/// Seals every secret value for the position it will be written at. Blank
/// names are the editor's trailing empty row, not an error, and are dropped
/// before positions are assigned.
pub fn prepare_variables(
    cipher: &dyn SecretCipher,
    environment_id: &str,
    variables: &[EnvironmentVariable],
) -> Result<Vec<PreparedVariable>, AppError> {
    variables
        .iter()
        .filter(|variable| !variable.name.trim().is_empty())
        .enumerate()
        .map(|(index, variable)| {
            let position = index as i64;
            let (value, sealed) = if variable.secret {
                let sealed = json::seal_value(
                    cipher,
                    &variable_scope(environment_id, position),
                    &variable.value,
                )?;
                (String::new(), sealed)
            } else {
                (variable.value.clone(), None)
            };
            Ok(PreparedVariable {
                position,
                name: variable.name.trim().to_string(),
                value,
                secret: variable.secret,
                sealed,
            })
        })
        .collect()
}

/// Writes prepared rows. The caller owns the transaction and any DELETE that
/// has to come first.
pub fn insert_variables(
    connection: &Connection,
    environment_id: &str,
    prepared: &[PreparedVariable],
) -> Result<(), AppError> {
    for variable in prepared {
        connection
            .execute(
                "INSERT INTO environment_variables
                     (environment_id, position, name, value, secret, value_sealed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    environment_id,
                    variable.position,
                    variable.name,
                    variable.value,
                    variable.secret,
                    variable.sealed
                ],
            )
            .map_err(to_storage_error)?;
    }
    Ok(())
}

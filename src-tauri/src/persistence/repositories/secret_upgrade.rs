// http_client/src-tauri/src/persistence/repositories/secret_upgrade.rs
//
// The one-time move of pre-Phase-9 secrets out of plain text, run at every
// startup after the migrations. SQL cannot encrypt, so this cannot be a
// migration file; instead it is written to be safe to repeat: rows already
// sealed or blank are skipped, each row is its own transaction, and a crash
// part-way leaves the rest for the next start.
//
// - requests: a plain auth secret is sealed. Skipped entirely when there is no
//   usable key this session — blanking it instead would lose it.
// - history and examples: a literal auth secret is blanked (they keep none).
//   Plain text that might be a secret elsewhere in those rows — a token
//   pasted into a URL or a body — is left alone: nothing here can tell which
//   strings are secrets.
//
// Anything changed is followed by VACUUM, so the old plain text does not
// survive in free pages. secure_delete (database.rs) covers everything
// deleted from now on; VACUUM covers what was deleted before it was on.
use rusqlite::params;

use crate::domain::error::AppError;
use crate::domain::ports::SecretCipher;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::json::{self, SecretRead, SecretWrite, StoredAuthSecret};
use crate::persistence::repositories::saved_requests::SECRET_TABLE as REQUESTS_TABLE;

/// Counts only — never values — so it can go to the log as it is.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UpgradeReport {
    pub sealed: usize,
    pub stripped: usize,
    /// Plain secrets left as they are because no key was usable.
    pub waiting_for_key: usize,
    /// Rows whose auth could not be parsed. Left untouched; loading them
    /// reports the same problem to the user.
    pub unreadable: usize,
    pub vacuumed: bool,
}

pub fn upgrade_plaintext_secrets(
    database: &Database,
    cipher: &dyn SecretCipher,
) -> Result<UpgradeReport, AppError> {
    let mut report = UpgradeReport::default();
    let key_usable = cipher.ensure_available().is_ok();

    for (id, raw) in auth_rows(database, "requests")? {
        match json::inspect_auth(&raw) {
            Ok(StoredAuthSecret::Nothing) => {}
            Ok(StoredAuthSecret::Plain | StoredAuthSecret::LiteralSecret) if !key_usable => {
                report.waiting_for_key += 1;
            }
            Ok(StoredAuthSecret::Plain | StoredAuthSecret::LiteralSecret) => {
                let (auth, _) = json::decode_auth(&raw, SecretRead::PlainOnly)?;
                let sealed = json::encode_auth(
                    &auth,
                    SecretWrite::Seal {
                        cipher,
                        table: REQUESTS_TABLE,
                        row_id: &id,
                    },
                )?;
                replace_auth(database, "requests", &id, &raw, &sealed)?;
                report.sealed += 1;
            }
            Err(_) => report.unreadable += 1,
        }
    }

    for table in ["history", "examples"] {
        for (id, raw) in auth_rows(database, table)? {
            match json::inspect_auth(&raw) {
                Ok(StoredAuthSecret::LiteralSecret) => {
                    let (auth, _) = json::decode_auth(&raw, SecretRead::PlainOnly)?;
                    let stripped = json::encode_auth(&auth, SecretWrite::Strip)?;
                    replace_auth(database, table, &id, &raw, &stripped)?;
                    report.stripped += 1;
                }
                Ok(_) => {}
                Err(_) => report.unreadable += 1,
            }
        }
    }

    if report.sealed + report.stripped > 0 {
        database
            .lock()
            .execute_batch("VACUUM;")
            .map_err(to_storage_error)?;
        report.vacuumed = true;
    }
    Ok(report)
}

/// `table` is one of three literals in this file, never user input, which is
/// the only reason it may be formatted into the statement.
fn auth_rows(database: &Database, table: &str) -> Result<Vec<(String, String)>, AppError> {
    let guard = database.lock();
    let mut statement = guard
        .prepare(&format!("SELECT id, auth_json FROM {table}"))
        .map_err(to_storage_error)?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(to_storage_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(to_storage_error)
}

/// Guarded on the old value, so a row the user re-saved between the read and
/// this write is left with what they saved.
fn replace_auth(
    database: &Database,
    table: &str,
    id: &str,
    old: &str,
    new: &str,
) -> Result<(), AppError> {
    let mut guard = database.lock();
    let transaction = guard.transaction().map_err(to_storage_error)?;
    transaction
        .execute(
            &format!("UPDATE {table} SET auth_json = ?1 WHERE id = ?2 AND auth_json = ?3"),
            params![new, id, old],
        )
        .map_err(to_storage_error)?;
    transaction.commit().map_err(to_storage_error)
}

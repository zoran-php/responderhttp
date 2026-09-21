// http_client/src-tauri/src/commands/history.rs
use tauri::State;

use crate::commands::dto::{HistoryEntryDto, RecordHistoryInput};
use crate::commands::error::ApiError;
use crate::domain::models::NewHistoryEntry;
use crate::AppState;

macro_rules! blocking {
    ($state:expr, $service:ident, $body:expr) => {{
        let $service = $state.history.clone();
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|_| ApiError::internal("storage task failed to complete"))?
    }};
}

#[tauri::command]
pub async fn list_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntryDto>, ApiError> {
    let entries = blocking!(state, service, service.list())?;
    Ok(entries.into_iter().map(HistoryEntryDto::from).collect())
}

/// Called after a send settles, success or failure. Returns the stored entry
/// so the panel can prepend it without re-reading the whole list.
#[tauri::command]
pub async fn record_history(
    state: State<'_, AppState>,
    entry: RecordHistoryInput,
) -> Result<HistoryEntryDto, ApiError> {
    let new_entry = NewHistoryEntry::try_from(entry)?;
    let stored = blocking!(state, service, service.record(&new_entry))?;
    Ok(stored.into())
}

#[tauri::command]
pub async fn delete_history_entry(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    blocking!(state, service, service.delete(&id))?;
    Ok(())
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), ApiError> {
    blocking!(state, service, service.clear())?;
    Ok(())
}

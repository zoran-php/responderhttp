// http_client/src-tauri/src/commands/docs.rs
//
// Adapters for item documentation (PLAN.md Phase 12): parse the target kind,
// call the service, map the result. The size cap, the dispatch and the
// storage all live below this file.
use tauri::State;

use crate::commands::dto::DocsTargetKind;
use crate::commands::error::ApiError;
use crate::AppState;

/// SQLite calls block, so they run on a blocking task like every other
/// storage command here.
macro_rules! blocking {
    ($state:expr, $service:ident, $body:expr) => {{
        let $service = $state.docs.clone();
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|_| ApiError::internal("storage task failed to complete"))?
    }};
}

#[tauri::command]
pub async fn item_docs(
    state: State<'_, AppState>,
    kind: DocsTargetKind,
    id: String,
) -> Result<String, ApiError> {
    let markdown = blocking!(state, service, service.get(kind.into(), &id))?;
    Ok(markdown)
}

#[tauri::command]
pub async fn set_item_docs(
    state: State<'_, AppState>,
    kind: DocsTargetKind,
    id: String,
    markdown: String,
) -> Result<(), ApiError> {
    blocking!(state, service, service.set(kind.into(), &id, &markdown))?;
    Ok(())
}

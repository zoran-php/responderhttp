// http_client/src-tauri/src/commands/cookies.rs
use tauri::State;

use crate::commands::dto::CookieDto;
use crate::commands::error::ApiError;
use crate::AppState;

macro_rules! blocking {
    ($state:expr, $service:ident, $body:expr) => {{
        let $service = $state.cookies.clone();
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|_| ApiError::internal("storage task failed to complete"))?
    }};
}

#[tauri::command]
pub async fn list_cookies(state: State<'_, AppState>) -> Result<Vec<CookieDto>, ApiError> {
    let cookies = blocking!(state, service, service.list())?;
    Ok(cookies.into_iter().map(CookieDto::from).collect())
}

#[tauri::command]
pub async fn delete_cookie(
    state: State<'_, AppState>,
    domain: String,
    path: String,
    name: String,
) -> Result<(), ApiError> {
    blocking!(state, service, service.delete(&domain, &path, &name))?;
    Ok(())
}

#[tauri::command]
pub async fn clear_cookies(state: State<'_, AppState>) -> Result<(), ApiError> {
    blocking!(state, service, service.clear())?;
    Ok(())
}

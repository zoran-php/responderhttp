// http_client/src-tauri/src/commands/environments.rs
use tauri::State;

use crate::commands::dto::{EnvironmentDto, EnvironmentVariableDto, EnvironmentVariableInput};
use crate::commands::error::ApiError;
use crate::domain::models::EnvironmentVariable;
use crate::AppState;

/// Adapters only: parse, delegate, map. SQLite calls block, so they run on a
/// blocking task exactly as the collections commands do.
macro_rules! blocking {
    ($state:expr, $service:ident, $body:expr) => {{
        let $service = $state.environments.clone();
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|_| ApiError::internal("storage task failed to complete"))?
    }};
}

#[tauri::command]
pub async fn list_environments(
    state: State<'_, AppState>,
) -> Result<Vec<EnvironmentDto>, ApiError> {
    let environments = blocking!(state, service, service.list())?;
    Ok(environments.into_iter().map(EnvironmentDto::from).collect())
}

#[tauri::command]
pub async fn create_environment(
    state: State<'_, AppState>,
    name: String,
) -> Result<EnvironmentDto, ApiError> {
    let environment = blocking!(state, service, service.create(&name))?;
    Ok(environment.into())
}

#[tauri::command]
pub async fn rename_environment(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), ApiError> {
    blocking!(state, service, service.rename(&id, &name))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_environment(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    blocking!(state, service, service.delete(&id))?;
    Ok(())
}

#[tauri::command]
pub async fn environment_variables(
    state: State<'_, AppState>,
    environment_id: String,
) -> Result<Vec<EnvironmentVariableDto>, ApiError> {
    let variables = blocking!(state, service, service.variables(&environment_id))?;
    Ok(variables
        .into_iter()
        .map(EnvironmentVariableDto::from)
        .collect())
}

#[tauri::command]
pub async fn set_environment_variables(
    state: State<'_, AppState>,
    environment_id: String,
    variables: Vec<EnvironmentVariableInput>,
) -> Result<(), ApiError> {
    let variables: Vec<EnvironmentVariable> = variables
        .into_iter()
        .map(EnvironmentVariable::from)
        .collect();
    blocking!(
        state,
        service,
        service.set_variables(&environment_id, &variables)
    )?;
    Ok(())
}

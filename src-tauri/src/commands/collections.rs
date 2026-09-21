// http_client/src-tauri/src/commands/collections.rs
use tauri::State;

use crate::commands::dto::{
    CollectionContentsDto, CollectionDto, ExampleDto, SaveExampleInput, SavedRequestDto,
    SendRequestInput,
};
use crate::commands::error::ApiError;
use crate::domain::models::{HttpRequest, KeyValue};
use crate::AppState;

/// Every command here is an adapter: parse, delegate, map. SQLite calls are
/// blocking, so they run on a blocking task like the HTTP ones do.
macro_rules! blocking {
    ($state:expr, $service:ident, $body:expr) => {{
        let $service = $state.collections.clone();
        tauri::async_runtime::spawn_blocking(move || $body)
            .await
            .map_err(|_| ApiError::internal("storage task failed to complete"))?
    }};
}

#[tauri::command]
pub async fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionDto>, ApiError> {
    let collections = blocking!(state, service, service.list())?;
    Ok(collections.into_iter().map(CollectionDto::from).collect())
}

#[tauri::command]
pub async fn collection_contents(
    state: State<'_, AppState>,
    collection_id: String,
) -> Result<CollectionContentsDto, ApiError> {
    let contents = blocking!(state, service, service.contents(&collection_id))?;
    Ok(contents.into())
}

#[tauri::command]
pub async fn create_collection(
    state: State<'_, AppState>,
    name: String,
) -> Result<CollectionDto, ApiError> {
    let collection = blocking!(state, service, service.create_collection(&name))?;
    Ok(collection.into())
}

#[tauri::command]
pub async fn rename_collection(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), ApiError> {
    blocking!(state, service, service.rename_collection(&id, &name))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_collection(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    blocking!(state, service, service.delete_collection(&id))?;
    Ok(())
}

#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    collection_id: String,
    parent_folder_id: Option<String>,
    name: String,
) -> Result<crate::commands::dto::FolderDto, ApiError> {
    let folder = blocking!(
        state,
        service,
        service.create_folder(&collection_id, parent_folder_id.as_deref(), &name)
    )?;
    Ok(folder.into())
}

#[tauri::command]
pub async fn rename_folder(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), ApiError> {
    blocking!(state, service, service.rename_folder(&id, &name))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    blocking!(state, service, service.delete_folder(&id))?;
    Ok(())
}

#[tauri::command]
pub async fn save_request(
    state: State<'_, AppState>,
    id: Option<String>,
    collection_id: String,
    folder_id: Option<String>,
    name: String,
    request: SendRequestInput,
) -> Result<SavedRequestDto, ApiError> {
    let domain_request = HttpRequest::try_from(request)?;
    let saved = blocking!(
        state,
        service,
        service.save_request(
            id,
            &collection_id,
            folder_id.as_deref(),
            &name,
            domain_request
        )
    )?;
    Ok(saved.into())
}

#[tauri::command]
pub async fn load_request(
    state: State<'_, AppState>,
    id: String,
) -> Result<SavedRequestDto, ApiError> {
    let saved = blocking!(state, service, service.load_request(&id))?;
    Ok(saved.into())
}

#[tauri::command]
pub async fn rename_request(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), ApiError> {
    blocking!(state, service, service.rename_request(&id, &name))?;
    Ok(())
}

#[tauri::command]
pub async fn move_request(
    state: State<'_, AppState>,
    id: String,
    folder_id: Option<String>,
) -> Result<(), ApiError> {
    blocking!(
        state,
        service,
        service.move_request(&id, folder_id.as_deref())
    )?;
    Ok(())
}

#[tauri::command]
pub async fn delete_request(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    blocking!(state, service, service.delete_request(&id))?;
    Ok(())
}

#[tauri::command]
pub async fn save_example(
    state: State<'_, AppState>,
    example: SaveExampleInput,
) -> Result<ExampleDto, ApiError> {
    let request = HttpRequest::try_from(example.request)?;
    let headers: Vec<KeyValue> = example
        .response_headers
        .into_iter()
        .map(KeyValue::from)
        .collect();
    let saved = blocking!(
        state,
        service,
        service.save_example(
            &example.request_id,
            &example.name,
            request,
            example.status,
            headers,
            example.response_body
        )
    )?;
    Ok(saved.into())
}

#[tauri::command]
pub async fn load_example(state: State<'_, AppState>, id: String) -> Result<ExampleDto, ApiError> {
    let example = blocking!(state, service, service.load_example(&id))?;
    Ok(example.into())
}

#[tauri::command]
pub async fn rename_example(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), ApiError> {
    blocking!(state, service, service.rename_example(&id, &name))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_example(state: State<'_, AppState>, id: String) -> Result<(), ApiError> {
    blocking!(state, service, service.delete_example(&id))?;
    Ok(())
}

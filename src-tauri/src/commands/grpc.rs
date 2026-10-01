// http_client/src-tauri/src/commands/grpc.rs
//
// gRPC adapters (PLAN-GRPC.md 16f). Each command parses, delegates to
// GrpcCalls or ProtoSchemas, and maps the result. services/grpc.ts is the
// only caller.
//
// Everything that touches the network or the disk runs on a blocking task,
// as `send_request` and `connect_web_socket` do. For `grpc_invoke` that task
// is also the call's owner thread for its whole life: libcurl's multi handle
// cannot move between threads (16d), so the call is opened and run there.
//
// Events go back over the `Channel` passed to `grpc_invoke`, one per call,
// in order. A failed send on it means the webview is gone, and the call is
// cancelled rather than run unseen.
use std::path::PathBuf;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::error::ApiError;
use crate::commands::grpc_dto::{
    GrpcCallOutcomeDto, GrpcEventDto, GrpcRequestInput, ProtoSchemaDto, ProtoSchemaSummaryDto,
};
use crate::domain::models::GrpcCallRequest;
use crate::domain::services::grpc::GrpcEventSink;
use crate::AppState;

const PROTO_FILTER_NAME: &str = "Protocol Buffers";
const PROTO_EXTENSIONS: [&str; 1] = ["proto"];

fn joined(error: tauri::Error) -> ApiError {
    ApiError::internal(format!("background task failed to complete: {error}"))
}

/// Asks the server for its schema by reflection.
#[tauri::command]
pub async fn grpc_reflect(
    state: State<'_, AppState>,
    request: GrpcRequestInput,
) -> Result<ProtoSchemaDto, ApiError> {
    let schemas = state.proto_schemas.clone();
    let request = GrpcCallRequest::from(request);
    let schema = tauri::async_runtime::spawn_blocking(move || schemas.reflect(&request))
        .await
        .map_err(joined)??;
    Ok(ProtoSchemaDto::from(schema.as_ref()))
}

/// The native chooser for the root `.proto` files of an import. An empty
/// list means the dialog was dismissed.
#[tauri::command]
pub async fn grpc_choose_proto_files(app: AppHandle) -> Result<Vec<String>, ApiError> {
    // blocking_pick_* deadlocks on the main thread, hence the blocking task.
    tauri::async_runtime::spawn_blocking(move || {
        let Some(chosen) = app
            .dialog()
            .file()
            .add_filter(PROTO_FILTER_NAME, &PROTO_EXTENSIONS)
            .blocking_pick_files()
        else {
            return Ok(Vec::new());
        };
        chosen
            .into_iter()
            .map(|file| {
                file.into_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|error| ApiError::internal(format!("unusable file path: {error}")))
            })
            .collect()
    })
    .await
    .map_err(joined)?
}

/// The native chooser for one import folder. None when dismissed.
#[tauri::command]
pub async fn grpc_choose_import_folder(app: AppHandle) -> Result<Option<String>, ApiError> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(chosen) = app.dialog().file().blocking_pick_folder() else {
            return Ok(None);
        };
        chosen
            .into_path()
            .map(|path| Some(path.to_string_lossy().into_owned()))
            .map_err(|error| ApiError::internal(format!("unusable folder path: {error}")))
    })
    .await
    .map_err(joined)?
}

/// Reads the chosen files and everything they import, within the chosen
/// folders, and compiles them.
#[tauri::command]
pub async fn grpc_import_proto(
    state: State<'_, AppState>,
    roots: Vec<String>,
    import_paths: Vec<String>,
) -> Result<ProtoSchemaDto, ApiError> {
    let schemas = state.proto_schemas.clone();
    let roots: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
    let import_paths: Vec<PathBuf> = import_paths.into_iter().map(PathBuf::from).collect();
    let schema =
        tauri::async_runtime::spawn_blocking(move || schemas.import_files(&roots, &import_paths))
            .await
            .map_err(joined)??;
    Ok(ProtoSchemaDto::from(schema.as_ref()))
}

/// A schema already loaded this session, for a tab that reopens it.
#[tauri::command]
pub fn grpc_schema(
    state: State<'_, AppState>,
    schema_id: String,
) -> Result<ProtoSchemaDto, ApiError> {
    let schema = state.proto_schemas.get(&schema_id)?;
    Ok(ProtoSchemaDto::from(schema.as_ref()))
}

/// The schema library, by name.
#[tauri::command]
pub fn grpc_list_schemas(
    state: State<'_, AppState>,
) -> Result<Vec<ProtoSchemaSummaryDto>, ApiError> {
    Ok(state
        .proto_schemas
        .list()?
        .into_iter()
        .map(ProtoSchemaSummaryDto::from)
        .collect())
}

/// Saves a loaded schema to the library. A reflected schema gets a library
/// id of its own, which the returned schema carries.
#[tauri::command]
pub fn grpc_save_schema(
    state: State<'_, AppState>,
    schema_id: String,
    name: String,
) -> Result<ProtoSchemaDto, ApiError> {
    let schema = state.proto_schemas.save(&schema_id, &name)?;
    Ok(ProtoSchemaDto::from(schema.as_ref()))
}

#[tauri::command]
pub fn grpc_rename_schema(
    state: State<'_, AppState>,
    schema_id: String,
    name: String,
) -> Result<(), ApiError> {
    Ok(state.proto_schemas.rename(&schema_id, &name)?)
}

/// Refused, naming the requests, while a saved request uses the schema.
#[tauri::command]
pub fn grpc_delete_schema(state: State<'_, AppState>, schema_id: String) -> Result<(), ApiError> {
    Ok(state.proto_schemas.delete(&schema_id)?)
}

/// "Use Example Message": JSON text for the method's input type.
#[tauri::command]
pub fn grpc_example_message(
    state: State<'_, AppState>,
    schema_id: String,
    method_path: String,
) -> Result<String, ApiError> {
    Ok(state.proto_schemas.example(&schema_id, &method_path)?)
}

/// Runs the call and resolves when it ends. Every event on the way arrives
/// on `on_event`; the last is `ended`, whose facts are also the result.
///
/// `message` is the one message of a unary or server-streaming call; the
/// client-streaming kinds take theirs through `grpc_send`.
#[tauri::command]
pub async fn grpc_invoke(
    state: State<'_, AppState>,
    call_id: String,
    schema_id: String,
    request: GrpcRequestInput,
    message: String,
    on_event: Channel<GrpcEventDto>,
) -> Result<GrpcCallOutcomeDto, ApiError> {
    let method = state
        .proto_schemas
        .method(&schema_id, &request.method_path)?;
    let calls = state.grpc_calls.clone();
    let request = GrpcCallRequest::from(request);
    let sink: GrpcEventSink =
        Box::new(move |event| on_event.send(GrpcEventDto::from(event)).is_ok());
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        calls.invoke(call_id, request, &method, &message, sink)
    })
    .await
    .map_err(joined)??;
    Ok(outcome.into())
}

/// Queues one message of a client-streaming call. Bad JSON comes back here;
/// the `sent` event follows on the channel once the message is handed over.
#[tauri::command]
pub fn grpc_send(
    state: State<'_, AppState>,
    call_id: String,
    message: String,
) -> Result<(), ApiError> {
    state
        .grpc_calls
        .send(&call_id, &message)
        .map_err(ApiError::from)
}

/// End Streaming. `streamEnded` follows on the channel.
#[tauri::command]
pub fn grpc_end_stream(state: State<'_, AppState>, call_id: String) -> Result<(), ApiError> {
    state
        .grpc_calls
        .end_stream(&call_id)
        .map_err(ApiError::from)
}

/// Returns at once; `ended` with CANCELLED follows on the channel.
#[tauri::command]
pub fn grpc_cancel(state: State<'_, AppState>, call_id: String) {
    state.grpc_calls.cancel(&call_id);
}

/// Cancels every call. The frontend calls it once as it starts, as it does
/// `disconnect_all_web_sockets`, so a reloaded webview leaves nothing
/// running unseen.
#[tauri::command]
pub fn grpc_cancel_all(state: State<'_, AppState>) {
    state.grpc_calls.cancel_all();
}

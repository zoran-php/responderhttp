// http_client/src-tauri/src/commands/websocket.rs
//
// WebSocket adapters (PLAN.md Phase 13c). Events go back to the webview over
// the `Channel` the frontend passes to `connect_web_socket`, one per
// connection: the Tauri 2 channel carries an index with every message and
// @tauri-apps/api re-orders by it, so they arrive in the order they were
// sent. services/websocket.ts is the only caller.
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::dto::{WebSocketRequestDto, WsEventDto, WsPayloadDto};
use crate::commands::error::ApiError;
use crate::domain::models::{WebSocketRequest, WsPayload};
use crate::domain::services::websocket::EventSink;
use crate::AppState;

/// Resolves once the handshake is done, or fails with why it was not. Every
/// later event arrives on `on_event`. Blocking (libcurl is), so it runs on a
/// blocking task, as `send_request` does.
///
/// A failed `send` on the channel means the webview side is gone; the sink
/// reports that, and the connection closes rather than running unseen.
#[tauri::command]
pub async fn connect_web_socket(
    state: State<'_, AppState>,
    connection_id: String,
    request: WebSocketRequestDto,
    on_event: Channel<WsEventDto>,
) -> Result<(), ApiError> {
    let sessions = state.websockets.clone();
    let request = WebSocketRequest::from(request);
    let sink: EventSink = Box::new(move |event| on_event.send(WsEventDto::from(event)).is_ok());

    tauri::async_runtime::spawn_blocking(move || sessions.connect(connection_id, request, sink))
        .await
        .map_err(|_| ApiError::internal("connect task failed to complete"))??;
    Ok(())
}

/// Queues the message and returns; its `sent` event follows on the channel
/// once it is actually on the wire.
#[tauri::command]
pub fn send_web_socket_message(
    state: State<'_, AppState>,
    connection_id: String,
    message: WsPayloadDto,
) -> Result<(), ApiError> {
    let payload = WsPayload::try_from(message)?;
    state
        .websockets
        .send(&connection_id, payload)
        .map_err(ApiError::from)
}

/// Returns at once; `closed` follows on the channel. Also cancels a
/// handshake still in progress, which then fails with `cancelled`.
#[tauri::command]
pub fn disconnect_web_socket(state: State<'_, AppState>, connection_id: String) {
    state.websockets.disconnect(&connection_id);
}

/// Closes every connection. The frontend calls it once as it starts, so a
/// reloaded webview never leaves connections running unseen.
#[tauri::command]
pub fn disconnect_all_web_sockets(state: State<'_, AppState>) {
    state.websockets.disconnect_all();
}

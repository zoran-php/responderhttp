// http_client/src-tauri/src/commands/request.rs
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::dto::{
    DownloadResultDto, HttpResponseDto, HttpStreamEventDto, KeyValueDto, SendRequestInput,
};
use crate::commands::error::ApiError;
use crate::domain::error::AppError;
use crate::domain::models::HttpRequest;
use crate::domain::services::downloads::suggested_file_name;
use crate::AppState;

/// Adapter only: parse input, hand it to the domain service, map the result.
///
/// The service call is blocking (libcurl is), so it runs on a blocking task
/// rather than on the thread driving the webview.
///
/// `on_event` carries what arrives before the response is complete: the
/// status and headers, and, for a `text/event-stream` response, every block
/// as it is parsed (PLAN-SSE.md). The promise still resolves with the whole
/// response, so history, saved responses and the rest are unchanged. A
/// failed `send` on the channel means the webview is gone, which ends the
/// transfer rather than letting it run unseen.
#[tauri::command]
pub async fn send_request(
    state: State<'_, AppState>,
    request_id: String,
    request: SendRequestInput,
    on_event: Channel<HttpStreamEventDto>,
) -> Result<HttpResponseDto, ApiError> {
    let service = state.send_request.clone();
    let domain_request = HttpRequest::try_from(request)?;

    let response = tauri::async_runtime::spawn_blocking(move || {
        service.execute_streaming(request_id, domain_request, &mut |update| {
            on_event.send(HttpStreamEventDto::from(update)).is_ok()
        })
    })
    .await
    .map_err(|_| ApiError::internal("request task failed to complete"))??;

    Ok(response.into())
}

/// Returns immediately: cancellation only sets a flag, and the in-flight
/// request surfaces it by failing with Cancelled.
#[tauri::command]
pub fn cancel_request(state: State<'_, AppState>, request_id: String) {
    state.send_request.cancel(&request_id);
}

/// Sends, then writes the response body to a file the user picks.
///
/// One command rather than send-then-save, so the bytes live in a single
/// scope and never cross the Tauri boundary — a large download would be
/// miserable as JSON, and holding it in app state between two commands would
/// mean deciding when to drop it.
///
/// The dialog opens *after* the response arrives, which is what lets it
/// suggest a name from `Content-Disposition`. It is also the reason a failed
/// request never asks where to save anything.
#[tauri::command]
pub async fn send_and_download(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    request: SendRequestInput,
) -> Result<DownloadResultDto, ApiError> {
    let service = state.send_request.clone();
    let downloads = state.downloads.clone();
    let domain_request = HttpRequest::try_from(request)?;
    let url = domain_request.url.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let response = service.execute(request_id, domain_request)?;

        // blocking_save_file must not run on the main thread — this closure
        // is already off it, which is the only reason the blocking variant is
        // the right one here.
        let suggested = suggested_file_name(&url, &response.headers);
        let chosen = app
            .dialog()
            .file()
            .set_file_name(suggested)
            .blocking_save_file();

        let saved_to = match chosen {
            Some(file_path) => {
                let path = file_path.into_path().map_err(|error| {
                    AppError::Storage(format!("unusable save location: {error}"))
                })?;
                downloads.save(&path, &response.body)?;
                Some(path.display().to_string())
            }
            // Dismissed. Not an error: the request happened, and its status
            // and timing are still worth showing.
            None => None,
        };

        Ok::<_, AppError>(DownloadResultDto {
            status: response.status,
            headers: response
                .headers
                .into_iter()
                .map(KeyValueDto::from)
                .collect(),
            timing: response.timing.into(),
            byte_length: response.body.byte_length(),
            saved_to,
        })
    })
    .await
    .map_err(|_| ApiError::internal("download task failed to complete"))??;

    Ok(result)
}

// http_client/src-tauri/src/commands/files.rs
//
// The native file chooser. An adapter by definition: a dialog is a platform
// concern, and this exists so no React component has to reach for
// @tauri-apps/plugin-dialog (CLAUDE.md section 11, rule 3) — one dependency
// rather than two, and the call stays behind a typed service function.
//
// Only the *path* comes back. Nothing reads the file here: libcurl opens it
// during the transfer, so an upload never travels through IPC.
use std::path::Path;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::commands::error::ApiError;
use crate::domain::mime::content_type_for_extension;

/// What the UI needs to draw a chosen file: where it is, what to call it, and
/// what it will be labelled as on the wire.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChosenFileDto {
    pub path: String,
    pub file_name: String,
    /// None when the extension is not one we recognise — the server then
    /// decides, which beats asserting application/octet-stream.
    pub content_type: Option<String>,
}

/// Returns None when the dialog is dismissed, which is not an error.
#[tauri::command]
pub async fn choose_file(app: AppHandle) -> Result<Option<ChosenFileDto>, ApiError> {
    // blocking_pick_file deadlocks on the main thread — the docs are explicit
    // about it — so this runs on a blocking task, as send_and_download does.
    tauri::async_runtime::spawn_blocking(move || {
        let Some(chosen) = app.dialog().file().blocking_pick_file() else {
            return Ok(None);
        };
        let path = chosen
            .into_path()
            .map_err(|error| ApiError::internal(format!("unusable file path: {error}")))?;
        Ok(Some(describe(&path)))
    })
    .await
    .map_err(|_| ApiError::internal("file chooser task failed to complete"))?
}

fn describe(path: &Path) -> ChosenFileDto {
    ChosenFileDto {
        path: path.to_string_lossy().into_owned(),
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        content_type: path
            .extension()
            .and_then(|extension| content_type_for_extension(&extension.to_string_lossy()))
            .map(str::to_string),
    }
}

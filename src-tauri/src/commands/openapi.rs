// http_client/src-tauri/src/commands/openapi.rs
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::dto::OpenApiExportResultDto;
use crate::commands::error::ApiError;
use crate::domain::error::AppError;
use crate::openapi::document::OpenApiVersion;
use crate::openapi::format::ExportFormat;
use crate::openapi::from_collection::ExportNote;
use crate::AppState;

/// Builds the document, then writes it where the user points. One command
/// rather than export-then-save for the same reason as `send_and_download`:
/// the document exists in a single scope and never crosses the IPC boundary.
///
/// `blocking_save_file` deadlocks on the main thread, so the whole thing runs
/// inside `spawn_blocking` — which the SQLite reads need anyway.
#[tauri::command]
pub async fn export_collection_openapi(
    app: AppHandle,
    state: State<'_, AppState>,
    collection_id: String,
    version: String,
    format: String,
    include_examples: bool,
) -> Result<OpenApiExportResultDto, ApiError> {
    let service = state.openapi.clone();
    let downloads = state.downloads.clone();
    // An unknown version means the frontend and this command disagree about
    // what is on offer. That is a bug to surface, not a default to guess at.
    let version = OpenApiVersion::from_wire(&version).ok_or_else(|| {
        AppError::InvalidRequest(format!("unsupported OpenAPI version {version}"))
    })?;
    let format = ExportFormat::from_wire(&format)
        .ok_or_else(|| AppError::InvalidRequest(format!("unsupported export format {format}")))?;
    let (filter_name, filter_extensions) = format.dialog_filter();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let exported = service.export(&collection_id, version, format, include_examples)?;

        let chosen = app
            .dialog()
            .file()
            .set_file_name(exported.file_name)
            .add_filter(filter_name, filter_extensions)
            .blocking_save_file();

        let saved_to = match chosen {
            Some(file_path) => {
                let path = file_path.into_path().map_err(|error| {
                    AppError::Storage(format!("unusable save location: {error}"))
                })?;
                downloads.save_text(&path, &exported.text)?;
                Some(path.display().to_string())
            }
            // Dismissed. Not an error, and the notes are still worth showing —
            // they are the reason to change the collection and export again.
            None => None,
        };

        Ok::<_, AppError>(OpenApiExportResultDto {
            saved_to,
            notes: exported.notes.iter().map(describe_note).collect(),
        })
    })
    .await
    .map_err(|_| ApiError::internal("export task failed to complete"))??;

    Ok(result)
}

/// Notes reach the UI as finished sentences rather than as a union the
/// frontend would have to re-describe. The wording lives on this side because
/// the variants do — a new variant should not compile until it has something
/// to say (CLAUDE.md section 2: commands map, they do not decide).
fn describe_note(note: &ExportNote) -> String {
    match note {
        ExportNote::DuplicateOperation {
            method,
            path,
            kept,
            dropped,
        } => format!(
            "{method} {path} is described by \"{kept}\"; \"{dropped}\" was left out - \
             OpenAPI has one operation per method and path."
        ),
        ExportNote::UnmappableUrl { request, url } => {
            format!("\"{request}\" was left out: \"{url}\" is not a URL this app could send.")
        }
        ExportNote::CredentialOmitted { request, scheme } => format!(
            "\"{request}\" is documented as using {scheme}. The credential itself was not exported."
        ),
        ExportNote::CredentialHeaderOmitted { request, header } => format!(
            "The {header} header on \"{request}\" was not exported - it carries credentials or \
             session state."
        ),
        ExportNote::CredentialParameterValueOmitted { request, parameter } => format!(
            "The value of query parameter \"{parameter}\" on \"{request}\" was not exported - \
             its name marks it as a credential."
        ),
        ExportNote::FolderNestingFlattened { folder } => format!(
            "Folder \"{folder}\" is nested. OpenAPI tags do not nest, so it became a tag of its own."
        ),
        ExportNote::ExampleBodyDecoded =>
            "Saved response bodies were written as `value` rather than 3.2's `serializedValue`, \
             which this version does not have. JSON bodies were decoded; everything else \
             stayed text."
                .to_string(),
    }
}

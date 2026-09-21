// http_client/src-tauri/src/commands/openapi_import.rs
//
// OpenAPI import (PLAN.md Phase 8d). The file is chosen and read on this
// side; the webview only ever holds a token for it. Notes and refusals cross
// as finished sentences, like the export's notes, so the wording lives next
// to the variants it describes.
//
// The DTOs are here rather than in dto.rs because nothing else uses them and
// every one of them is shaped by this dialog alone.
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::error::ApiError;
use crate::domain::error::AppError;
use crate::domain::services::openapi_import::{ImportPreview, ImportResult, LoadOutcome};
use crate::openapi::import::grouping::Grouping;
use crate::openapi::import::to_collection::{IgnoredFeature, ImportNote, ImportOptions, Tally};
use crate::openapi::import::validate::MAX_SHOWN_VIOLATIONS;
use crate::openapi::import::Refusal;
use crate::AppState;

const DIALOG_FILTER_NAME: &str = "OpenAPI (JSON or YAML)";
const DIALOG_EXTENSIONS: [&str; 3] = ["json", "yaml", "yml"];
/// Enough for the largest real specs (GitHub has 49 tags) without sending a
/// path-derived list of thousands across IPC.
const MAX_PREVIEW_FOLDERS: usize = 100;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenApiImportLoadDto {
    Cancelled,
    Refused(OpenApiImportRefusalDto),
    Ready(OpenApiImportPreviewDto),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiImportRefusalDto {
    pub file_name: String,
    pub title: String,
    pub reason: String,
    /// Schema violations or references, one line each.
    pub details: Vec<String>,
    /// How many details there were before the list was capped.
    pub total_details: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingPreviewDto {
    pub grouping: &'static str,
    pub folder_count: usize,
    /// Before `top_level` was capped.
    pub top_level_count: usize,
    pub top_level: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewVariableDto {
    pub name: String,
    pub secret: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentPreviewDto {
    pub name: String,
    pub variables: Vec<PreviewVariableDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiImportPreviewDto {
    pub token: String,
    pub file_name: String,
    pub title: String,
    pub version: &'static str,
    pub format: &'static str,
    pub request_count: usize,
    pub example_count: usize,
    pub default_grouping: &'static str,
    pub groupings: Vec<GroupingPreviewDto>,
    pub environment: Option<EnvironmentPreviewDto>,
    pub notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiImportOptionsDto {
    pub grouping: String,
    pub include_examples: bool,
    pub create_environment: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiImportResultDto {
    pub collection_id: String,
    pub environment_id: Option<String>,
    pub collection_name: String,
    pub request_count: usize,
    pub notes: Vec<String>,
}

impl TryFrom<OpenApiImportOptionsDto> for ImportOptions {
    type Error = AppError;

    fn try_from(dto: OpenApiImportOptionsDto) -> Result<Self, AppError> {
        let grouping = Grouping::from_wire(&dto.grouping).ok_or_else(|| {
            AppError::InvalidRequest(format!("unknown grouping {}", dto.grouping))
        })?;
        Ok(ImportOptions {
            grouping,
            include_examples: dto.include_examples,
            create_environment: dto.create_environment,
        })
    }
}

/// Opens the file chooser, then reads and checks the chosen file. Dismissing
/// the chooser is `cancelled`, not an error.
#[tauri::command]
pub async fn pick_openapi_import(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OpenApiImportLoadDto, ApiError> {
    let service = state.openapi_import.clone();
    // blocking_pick_file deadlocks on the main thread, and a 10 MB spec takes
    // a moment to validate, so both happen on a blocking task.
    tauri::async_runtime::spawn_blocking(move || {
        let Some(chosen) = app
            .dialog()
            .file()
            .add_filter(DIALOG_FILTER_NAME, &DIALOG_EXTENSIONS)
            .add_filter("All files", &["*"])
            .blocking_pick_file()
        else {
            return Ok(OpenApiImportLoadDto::Cancelled);
        };
        let path = chosen
            .into_path()
            .map_err(|error| AppError::InvalidRequest(format!("unusable file path: {error}")))?;
        Ok(match service.load(&path)? {
            LoadOutcome::Ready(preview) => OpenApiImportLoadDto::Ready(preview_dto(preview)),
            LoadOutcome::Refused { file_name, refusal } => {
                OpenApiImportLoadDto::Refused(refusal_dto(file_name, &refusal))
            }
        })
    })
    .await
    .map_err(|_| ApiError::internal("import task failed to complete"))?
}

#[tauri::command]
pub async fn preview_openapi_import(
    state: State<'_, AppState>,
    token: String,
    options: OpenApiImportOptionsDto,
) -> Result<OpenApiImportPreviewDto, ApiError> {
    let service = state.openapi_import.clone();
    let options = ImportOptions::try_from(options)?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(preview_dto(service.preview(&token, &options)?))
    })
    .await
    .map_err(|_| ApiError::internal("import preview failed to complete"))?
}

#[tauri::command]
pub async fn import_openapi(
    state: State<'_, AppState>,
    token: String,
    options: OpenApiImportOptionsDto,
) -> Result<OpenApiImportResultDto, ApiError> {
    let service = state.openapi_import.clone();
    let options = ImportOptions::try_from(options)?;
    tauri::async_runtime::spawn_blocking(move || Ok(result_dto(service.import(&token, &options)?)))
        .await
        .map_err(|_| ApiError::internal("import task failed to complete"))?
}

#[tauri::command]
pub fn discard_openapi_import(state: State<'_, AppState>, token: String) {
    state.openapi_import.discard(&token);
}

fn preview_dto(preview: ImportPreview) -> OpenApiImportPreviewDto {
    OpenApiImportPreviewDto {
        token: preview.token,
        file_name: preview.file_name,
        title: preview.title,
        version: preview.version.label(),
        format: preview.format.label(),
        request_count: preview.request_count,
        example_count: preview.example_count,
        default_grouping: preview.default_grouping.wire(),
        groupings: preview
            .groupings
            .into_iter()
            .map(|grouping| GroupingPreviewDto {
                grouping: grouping.grouping.wire(),
                folder_count: grouping.folder_count,
                top_level_count: grouping.top_level.len(),
                top_level: grouping
                    .top_level
                    .into_iter()
                    .take(MAX_PREVIEW_FOLDERS)
                    .collect(),
            })
            .collect(),
        environment: preview
            .environment
            .map(|environment| EnvironmentPreviewDto {
                name: environment.name,
                variables: environment
                    .variables
                    .into_iter()
                    .map(|variable| PreviewVariableDto {
                        name: variable.name,
                        secret: variable.secret,
                    })
                    .collect(),
            }),
        notes: preview.notes.iter().map(describe_note).collect(),
    }
}

fn result_dto(result: ImportResult) -> OpenApiImportResultDto {
    OpenApiImportResultDto {
        collection_id: result.ids.collection_id,
        environment_id: result.ids.environment_id,
        collection_name: result.collection_name,
        request_count: result.request_count,
        notes: result.notes.iter().map(describe_note).collect(),
    }
}

fn refusal_dto(file_name: String, refusal: &Refusal) -> OpenApiImportRefusalDto {
    let (title, reason, details, total_details) = describe_refusal(refusal);
    OpenApiImportRefusalDto {
        file_name,
        title: title.to_string(),
        reason,
        total_details: total_details.max(details.len()),
        details,
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn describe_refusal(refusal: &Refusal) -> (&'static str, String, Vec<String>, usize) {
    let simple = |title: &'static str, reason: String| (title, reason, Vec::new(), 0);
    match refusal {
        Refusal::Unreadable { message } => simple(
            "The file could not be read",
            format!("The operating system said: {message}"),
        ),
        Refusal::TooLarge { bytes, limit } => simple(
            "The file is too large",
            format!(
                "It is {}; the import accepts files up to {}.",
                megabytes(*bytes),
                megabytes(*limit)
            ),
        ),
        Refusal::NotUtf8 => simple(
            "Not a UTF-8 text file",
            "OpenAPI files are read as UTF-8. Save the file as UTF-8 and try again.".into(),
        ),
        Refusal::Empty => simple("The file is empty", "There is nothing to import.".into()),
        Refusal::InvalidJson { message } => simple(
            "Not valid JSON",
            format!("The file starts like JSON but does not parse: {message}."),
        ),
        Refusal::InvalidYaml { message } => simple(
            "Not valid JSON or YAML",
            format!("The file is not JSON, and reading it as YAML failed: {message}."),
        ),
        Refusal::NotJsonOrYaml => simple(
            "This file is not JSON or YAML",
            "Only OpenAPI documents written in JSON or YAML can be imported.".into(),
        ),
        Refusal::NotOpenApi => simple(
            "Not an OpenAPI document",
            "The file is valid JSON/YAML, but it has no `openapi` field.".into(),
        ),
        Refusal::Swagger2 => simple(
            "Swagger 2.0 is not supported",
            "Convert the document to OpenAPI 3 first, then import the result.".into(),
        ),
        Refusal::VersionNotString => simple(
            "The OpenAPI version is not text",
            "The `openapi` field must be a string. In YAML, quote it: openapi: \"3.1.0\".".into(),
        ),
        Refusal::UnsupportedVersion { version } => simple(
            "Unsupported OpenAPI version",
            format!("OpenAPI {version} is not supported; 3.0, 3.1 and 3.2 are."),
        ),
        Refusal::SchemaViolations {
            version,
            shown,
            total,
        } => (
            "The document is not valid OpenAPI",
            format!(
                "It does not match the official OpenAPI {} schema, so nothing was imported. \
                 {} shown below{}.",
                version.label(),
                plural(*total, "problem", "problems"),
                if *total > MAX_SHOWN_VIOLATIONS {
                    format!(" (the first {MAX_SHOWN_VIOLATIONS})")
                } else {
                    String::new()
                }
            ),
            shown
                .iter()
                .map(|violation| {
                    format!(
                        "{}: {}",
                        readable_pointer(&violation.location),
                        violation.message
                    )
                })
                .collect(),
            *total,
        ),
        Refusal::ExternalReferences { refs } => (
            "The document refers to other files",
            "The import reads one file and never fetches anything. Bundle the spec into a \
             single file first (for example with `redocly bundle`), then import that."
                .into(),
            refs.clone(),
            refs.len(),
        ),
        Refusal::BrokenReferences { refs } => (
            "The document has references that go nowhere",
            "These `$ref`s do not point at anything in the file, or point at each other in a loop."
                .into(),
            refs.clone(),
            refs.len(),
        ),
    }
}

/// `/paths/~1users~1{id}/get` → `paths › /users/{id} › get`.
fn readable_pointer(pointer: &str) -> String {
    if pointer.is_empty() {
        return "(document root)".into();
    }
    pointer
        .trim_start_matches('/')
        .split('/')
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>()
        .join(" › ")
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// "on "List pets"" or "on 12 requests, including "A", "B" and "C"".
fn on_requests(tally: &Tally) -> String {
    let quoted: Vec<String> = tally
        .sample
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect();
    if tally.count == 1 {
        return format!("on {}", quoted.first().cloned().unwrap_or_default());
    }
    let listed = match quoted.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    };
    format!("on {} requests, including {listed}", tally.count)
}

fn describe_note(note: &ImportNote) -> String {
    match note {
        ImportNote::SchemaObjectsUnchecked => {
            "The document declares its own JSON Schema dialect, so its schemas were not checked - \
             only the document's structure was."
                .into()
        }
        ImportNote::UnsupportedMethod { method, path } => {
            format!("{method} {path} was left out: this app cannot send {method} requests.")
        }
        ImportNote::Ignored { feature, count } => match feature {
            IgnoredFeature::Webhooks => format!(
                "{} left out - they describe requests the API sends, not ones you send.",
                plural(*count, "webhook was", "webhooks were")
            ),
            IgnoredFeature::Callbacks => format!(
                "Callbacks on {} were left out.",
                plural(*count, "operation", "operations")
            ),
            IgnoredFeature::Links => format!(
                "Links on {} were left out.",
                plural(*count, "response", "responses")
            ),
            IgnoredFeature::ExternalExamples => format!(
                "{} point at an external file or URL and were not read.",
                plural(*count, "example", "examples")
            ),
        },
        ImportNote::NoServer => {
            "The document names no server. Set the base URL in the environment (or in each \
             request) before sending."
                .into()
        }
        ImportNote::RelativeServer { url } => format!(
            "The server URL \"{url}\" is relative to wherever the document is hosted. Replace it \
             with a full URL before sending."
        ),
        ImportNote::OtherServersIgnored { urls } => format!(
            "Only the first server was used. Also listed: {}.",
            urls.join(", ")
        ),
        ImportNote::OwnServer(tally) => format!(
            "A server of their own is set {}; those URLs were written out in full.",
            on_requests(tally)
        ),
        ImportNote::CookieParameters(tally) => format!(
            "Cookie parameters {} were left out - the cookie jar sends cookies.",
            on_requests(tally)
        ),
        ImportNote::QuerystringParameters(tally) => format!(
            "Whole-query-string parameters {} were left out.",
            on_requests(tally)
        ),
        ImportNote::OptionalParametersLeftOut(tally) => format!(
            "Optional parameters with no example or default were left out {}, so they are not \
             sent empty.",
            on_requests(tally)
        ),
        ImportNote::UnsupportedBody {
            media_type,
            requests,
        } => format!(
            "{media_type} request bodies cannot be represented here; the body was left empty {}.",
            on_requests(requests)
        ),
        ImportNote::FilePartsLeftOut(tally) => format!(
            "File fields in multipart bodies were left out {} - choose the files in the Body tab.",
            on_requests(tally)
        ),
        ImportNote::UnsupportedSecurity {
            scheme,
            kind,
            requests,
        } => format!(
            "\"{scheme}\" ({kind}) is not an auth type this app has; auth was left as None {}.",
            on_requests(requests)
        ),
        ImportNote::UnknownSecurityScheme { scheme, requests } => format!(
            "\"{scheme}\" is required {} but not defined in the document; auth was left as None.",
            on_requests(requests)
        ),
        ImportNote::CombinedSecurity(tally) => format!(
            "Some requirements combine several auth schemes; only the first was set {}.",
            on_requests(tally)
        ),
        ImportNote::TagNotNested { tag } => format!(
            "Tag \"{tag}\" names a parent that does not exist or loops back to it; its folder \
             was put at the top level."
        ),
        ImportNote::ExampleStatusGuessed(tally) => format!(
            "{} documented for a status range or `default`, so a status was chosen for them.",
            plural(tally.count, "saved example is", "saved examples are")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::document::OpenApiVersion;
    use crate::openapi::import::SchemaViolation;

    #[test]
    fn pointers_are_shown_as_paths() {
        assert_eq!(readable_pointer(""), "(document root)");
        assert_eq!(
            readable_pointer("/paths/~1users~1{id}/get/parameters/0"),
            "paths › /users/{id} › get › parameters › 0"
        );
        assert_eq!(readable_pointer("/a~0b"), "a~b");
    }

    #[test]
    fn tallies_read_naturally() {
        let one = Tally {
            count: 1,
            sample: vec!["List pets".into()],
        };
        let many = Tally {
            count: 12,
            sample: vec!["A".into(), "B".into(), "C".into()],
        };

        assert_eq!(on_requests(&one), "on \"List pets\"");
        assert_eq!(
            on_requests(&many),
            "on 12 requests, including \"A\", \"B\" and \"C\""
        );
    }

    #[test]
    fn schema_refusals_carry_their_violations_and_total() {
        let refusal = Refusal::SchemaViolations {
            version: OpenApiVersion::V3_1,
            shown: vec![SchemaViolation {
                location: "/info".into(),
                message: "missing properties 'title'".into(),
            }],
            total: 25,
        };

        let dto = refusal_dto("spec.yaml".into(), &refusal);

        assert_eq!(dto.title, "The document is not valid OpenAPI");
        assert!(dto.reason.contains("OpenAPI 3.1"), "{}", dto.reason);
        assert!(dto.reason.contains("25 problems"), "{}", dto.reason);
        assert_eq!(dto.details, vec!["info: missing properties 'title'"]);
        assert_eq!(dto.total_details, 25);
    }

    #[test]
    fn options_parse_the_grouping_strictly() {
        let good = OpenApiImportOptionsDto {
            grouping: "paths".into(),
            include_examples: true,
            create_environment: false,
        };
        let bad = OpenApiImportOptionsDto {
            grouping: "folders".into(),
            include_examples: true,
            create_environment: false,
        };

        assert_eq!(
            ImportOptions::try_from(good).unwrap().grouping,
            Grouping::Paths
        );
        assert!(ImportOptions::try_from(bad).is_err());
    }

    #[test]
    fn every_refusal_and_note_has_words() {
        for refusal in [
            Refusal::Empty,
            Refusal::NotUtf8,
            Refusal::Swagger2,
            Refusal::TooLarge {
                bytes: 60 * 1024 * 1024,
                limit: 50 * 1024 * 1024,
            },
        ] {
            let (title, reason, _, _) = describe_refusal(&refusal);
            assert!(!title.is_empty() && !reason.is_empty());
        }
        let too_large = describe_refusal(&Refusal::TooLarge {
            bytes: 60 * 1024 * 1024,
            limit: 50 * 1024 * 1024,
        });
        assert_eq!(
            too_large.1,
            "It is 60.0 MB; the import accepts files up to 50.0 MB."
        );
        assert_eq!(
            describe_note(&ImportNote::Ignored {
                feature: IgnoredFeature::Webhooks,
                count: 1
            }),
            "1 webhook was left out - they describe requests the API sends, not ones you send."
        );
    }
}

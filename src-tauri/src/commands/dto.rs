// http_client/src-tauri/src/commands/dto.rs
//
// The wire format between Rust and TypeScript. Mirrored by src/types/http.ts;
// camelCase on both sides. Durations cross as whole milliseconds — the UI has
// no use for nanosecond precision.
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::commands::error::ApiError;
use crate::domain::models::{
    ApiKeyLocation, Auth, Collection, Cookie, Environment, EnvironmentVariable, Example,
    ExampleSummary, Folder, HistoryEntry, HttpMethod, HttpRequest, HttpResponse,
    HttpVersionPreference, KeyValue, MultipartPart, NewHistoryEntry, RequestBody, RequestSettings,
    ResponseBody, SavedRequest, Timing, TlsMinimum,
};
use crate::domain::secrets::SecretState;
use crate::domain::services::collections::CollectionContents;
use crate::domain::services::docs::DocsTarget;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueDto {
    pub name: String,
    pub value: String,
}

impl From<KeyValueDto> for KeyValue {
    fn from(dto: KeyValueDto) -> Self {
        Self {
            name: dto.name,
            value: dto.value,
        }
    }
}

impl From<KeyValue> for KeyValueDto {
    fn from(pair: KeyValue) -> Self {
        Self {
            name: pair.name,
            value: pair.value,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieDto {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires_at: Option<u64>,
    pub secure: bool,
    pub http_only: bool,
    pub host_only: bool,
}

impl From<Cookie> for CookieDto {
    fn from(cookie: Cookie) -> Self {
        Self {
            name: cookie.name,
            value: cookie.value,
            domain: cookie.domain,
            path: cookie.path,
            expires_at: cookie.expires_at,
            secure: cookie.secure,
            http_only: cookie.http_only,
            host_only: cookie.host_only,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentDto {
    pub id: String,
    pub name: String,
}

impl From<Environment> for EnvironmentDto {
    fn from(environment: Environment) -> Self {
        Self {
            id: environment.id,
            name: environment.name,
        }
    }
}

/// Whether a stored secret could be read back. Mirrored by SecretState in
/// src/types/http.ts.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SecretStateDto {
    Ok,
    NeedsReentry,
    Unavailable,
}

impl From<SecretState> for SecretStateDto {
    fn from(state: SecretState) -> Self {
        match state {
            SecretState::Ok => Self::Ok,
            SecretState::NeedsReentry => Self::NeedsReentry,
            SecretState::Unavailable => Self::Unavailable,
        }
    }
}

/// A variable as the editor receives it. A secret's value is sent in plain
/// text: the editor shows it (masked) and the frontend substitutes it. The
/// protection is for the database file, not for this process's memory.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariableDto {
    pub name: String,
    pub value: String,
    pub secret: bool,
    pub secret_state: SecretStateDto,
}

impl From<EnvironmentVariable> for EnvironmentVariableDto {
    fn from(variable: EnvironmentVariable) -> Self {
        Self {
            name: variable.name,
            value: variable.value,
            secret: variable.secret,
            secret_state: variable.state.into(),
        }
    }
}

/// A variable as the editor sends it. No state: whether a value can be read
/// is decided by storage, never claimed by the caller.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariableInput {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub secret: bool,
}

impl From<EnvironmentVariableInput> for EnvironmentVariable {
    fn from(input: EnvironmentVariableInput) -> Self {
        Self {
            name: input.name,
            value: input.value,
            secret: input.secret,
            state: SecretState::Ok,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RequestBodyDto {
    None,
    #[serde(rename_all = "camelCase")]
    Raw {
        content_type: String,
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    FormUrlEncoded {
        fields: Vec<KeyValueDto>,
    },
    #[serde(rename_all = "camelCase")]
    Multipart {
        parts: Vec<MultipartPartDto>,
    },
}

/// A file part carries a path, not bytes: libcurl reads the file during the
/// transfer, so an upload never travels through IPC or sits in memory here.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MultipartPartDto {
    #[serde(rename_all = "camelCase")]
    Text { name: String, value: String },
    #[serde(rename_all = "camelCase")]
    File {
        name: String,
        path: String,
        content_type: Option<String>,
    },
}

impl From<MultipartPartDto> for MultipartPart {
    fn from(dto: MultipartPartDto) -> Self {
        match dto {
            MultipartPartDto::Text { name, value } => Self::Text { name, value },
            MultipartPartDto::File {
                name,
                path,
                content_type,
            } => Self::File {
                name,
                path: std::path::PathBuf::from(path),
                content_type,
            },
        }
    }
}

impl From<MultipartPart> for MultipartPartDto {
    fn from(part: MultipartPart) -> Self {
        match part {
            MultipartPart::Text { name, value } => Self::Text { name, value },
            MultipartPart::File {
                name,
                path,
                content_type,
            } => Self::File {
                name,
                path: path.to_string_lossy().into_owned(),
                content_type,
            },
        }
    }
}

impl From<RequestBodyDto> for RequestBody {
    fn from(dto: RequestBodyDto) -> Self {
        match dto {
            RequestBodyDto::None => Self::None,
            RequestBodyDto::Raw { content_type, text } => Self::Raw { content_type, text },
            RequestBodyDto::FormUrlEncoded { fields } => Self::FormUrlEncoded(into_pairs(fields)),
            RequestBodyDto::Multipart { parts } => {
                Self::Multipart(parts.into_iter().map(MultipartPart::from).collect())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ApiKeyLocationDto {
    #[default]
    Header,
    Query,
}

impl From<ApiKeyLocationDto> for ApiKeyLocation {
    fn from(dto: ApiKeyLocationDto) -> Self {
        match dto {
            ApiKeyLocationDto::Header => Self::Header,
            ApiKeyLocationDto::Query => Self::Query,
        }
    }
}

impl From<ApiKeyLocation> for ApiKeyLocationDto {
    fn from(location: ApiKeyLocation) -> Self {
        match location {
            ApiKeyLocation::Header => Self::Header,
            ApiKeyLocation::Query => Self::Query,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthDto {
    #[default]
    None,
    #[serde(rename_all = "camelCase")]
    Basic { username: String, password: String },
    #[serde(rename_all = "camelCase")]
    Bearer { token: String },
    #[serde(rename_all = "camelCase")]
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyLocationDto,
    },
    #[serde(rename_all = "camelCase")]
    Custom {
        header_name: String,
        header_value: String,
    },
}

impl From<AuthDto> for Auth {
    fn from(dto: AuthDto) -> Self {
        match dto {
            AuthDto::None => Self::None,
            AuthDto::Basic { username, password } => Self::Basic { username, password },
            AuthDto::Bearer { token } => Self::Bearer { token },
            AuthDto::ApiKey {
                key,
                value,
                location,
            } => Self::ApiKey {
                key,
                value,
                location: location.into(),
            },
            AuthDto::Custom {
                header_name,
                header_value,
            } => Self::Custom {
                header_name,
                header_value,
            },
        }
    }
}

impl From<Auth> for AuthDto {
    fn from(auth: Auth) -> Self {
        match auth {
            Auth::None => Self::None,
            Auth::Basic { username, password } => Self::Basic { username, password },
            Auth::Bearer { token } => Self::Bearer { token },
            Auth::ApiKey {
                key,
                value,
                location,
            } => Self::ApiKey {
                key,
                value,
                location: location.into(),
            },
            Auth::Custom {
                header_name,
                header_value,
            } => Self::Custom {
                header_name,
                header_value,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSettingsDto {
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub timeout_ms: u64,
    pub verify_tls: bool,
    pub proxy: Option<String>,
    /// Spelled-out default for the same reason as StoredSettings: `false`
    /// would quietly disable cookies for anything sent by an older payload.
    #[serde(default = "cookies_enabled")]
    pub send_cookies: bool,

    // Defaulted so a payload built before these existed still deserialises —
    // history entries and saved examples both replay stored settings through
    // this type. encode_url defaults true for the same reason send_cookies
    // does: false is the harmful direction.
    #[serde(default)]
    pub http_version: HttpVersionDto,
    #[serde(default)]
    pub keep_method_on_redirect: bool,
    #[serde(default)]
    pub keep_auth_on_redirect: bool,
    #[serde(default = "cookies_enabled")]
    pub encode_url: bool,
    #[serde(default)]
    pub allow_http_09: bool,
    #[serde(default)]
    pub tls_minimum: TlsMinimumDto,
}

fn cookies_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HttpVersionDto {
    #[default]
    Auto,
    Http11,
    Http2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TlsMinimumDto {
    #[default]
    Auto,
    Tls12,
    Tls13,
}

impl From<RequestSettingsDto> for RequestSettings {
    fn from(dto: RequestSettingsDto) -> Self {
        Self {
            follow_redirects: dto.follow_redirects,
            max_redirects: dto.max_redirects,
            timeout: Duration::from_millis(dto.timeout_ms),
            verify_tls: dto.verify_tls,
            proxy: dto.proxy,
            send_cookies: dto.send_cookies,
            http_version: match dto.http_version {
                HttpVersionDto::Auto => HttpVersionPreference::Auto,
                HttpVersionDto::Http11 => HttpVersionPreference::Http11,
                HttpVersionDto::Http2 => HttpVersionPreference::Http2,
            },
            keep_method_on_redirect: dto.keep_method_on_redirect,
            keep_auth_on_redirect: dto.keep_auth_on_redirect,
            encode_url: dto.encode_url,
            allow_http_09: dto.allow_http_09,
            tls_minimum: match dto.tls_minimum {
                TlsMinimumDto::Auto => TlsMinimum::Auto,
                TlsMinimumDto::Tls12 => TlsMinimum::Tls12,
                TlsMinimumDto::Tls13 => TlsMinimum::Tls13,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequestInput {
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValueDto>,
    pub query_params: Vec<KeyValueDto>,
    pub body: RequestBodyDto,
    /// Defaulted so a request saved before Phase 4 still deserialises.
    #[serde(default)]
    pub auth: AuthDto,
    pub settings: RequestSettingsDto,
}

impl TryFrom<SendRequestInput> for HttpRequest {
    type Error = ApiError;

    fn try_from(input: SendRequestInput) -> Result<Self, Self::Error> {
        let method = HttpMethod::parse(&input.method).ok_or_else(|| ApiError::InvalidRequest {
            message: format!("unsupported HTTP method: {}", input.method),
        })?;
        Ok(Self {
            method,
            url: input.url,
            headers: into_pairs(input.headers),
            query_params: into_pairs(input.query_params),
            body: input.body.into(),
            auth: input.auth.into(),
            settings: input.settings.into(),
        })
    }
}

fn into_pairs(dtos: Vec<KeyValueDto>) -> Vec<KeyValue> {
    dtos.into_iter().map(KeyValue::from).collect()
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResponseBodyDto {
    Text {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Binary {
        byte_length: usize,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingDto {
    pub dns_ms: u64,
    pub connect_ms: u64,
    pub tls_ms: u64,
    pub time_to_first_byte_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponseDto {
    pub status: u16,
    pub headers: Vec<KeyValueDto>,
    pub body: ResponseBodyDto,
    pub timing: TimingDto,
}

impl From<HttpResponse> for HttpResponseDto {
    fn from(response: HttpResponse) -> Self {
        Self {
            status: response.status,
            headers: response
                .headers
                .into_iter()
                .map(KeyValueDto::from)
                .collect(),
            body: response.body.into(),
            timing: response.timing.into(),
        }
    }
}

impl From<ResponseBody> for ResponseBodyDto {
    fn from(body: ResponseBody) -> Self {
        match body {
            ResponseBody::Text(text) => Self::Text { text },
            // Only the length crosses the boundary — the bytes stay in Rust.
            ResponseBody::Binary { ref bytes } => Self::Binary {
                byte_length: bytes.len(),
            },
        }
    }
}

impl From<Timing> for TimingDto {
    fn from(timing: Timing) -> Self {
        Self {
            dns_ms: millis(timing.dns),
            connect_ms: millis(timing.connect),
            tls_ms: millis(timing.tls),
            time_to_first_byte_ms: millis(timing.time_to_first_byte),
            total_ms: millis(timing.total),
        }
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl From<RequestBody> for RequestBodyDto {
    fn from(body: RequestBody) -> Self {
        match body {
            RequestBody::None => Self::None,
            RequestBody::Raw { content_type, text } => Self::Raw { content_type, text },
            RequestBody::FormUrlEncoded(fields) => Self::FormUrlEncoded {
                fields: from_pairs(fields),
            },
            RequestBody::Multipart(parts) => Self::Multipart {
                parts: parts.into_iter().map(MultipartPartDto::from).collect(),
            },
        }
    }
}

impl From<RequestSettings> for RequestSettingsDto {
    fn from(settings: RequestSettings) -> Self {
        Self {
            follow_redirects: settings.follow_redirects,
            max_redirects: settings.max_redirects,
            timeout_ms: millis(settings.timeout),
            verify_tls: settings.verify_tls,
            proxy: settings.proxy,
            send_cookies: settings.send_cookies,
            http_version: match settings.http_version {
                HttpVersionPreference::Auto => HttpVersionDto::Auto,
                HttpVersionPreference::Http11 => HttpVersionDto::Http11,
                HttpVersionPreference::Http2 => HttpVersionDto::Http2,
            },
            keep_method_on_redirect: settings.keep_method_on_redirect,
            keep_auth_on_redirect: settings.keep_auth_on_redirect,
            encode_url: settings.encode_url,
            allow_http_09: settings.allow_http_09,
            tls_minimum: match settings.tls_minimum {
                TlsMinimum::Auto => TlsMinimumDto::Auto,
                TlsMinimum::Tls12 => TlsMinimumDto::Tls12,
                TlsMinimum::Tls13 => TlsMinimumDto::Tls13,
            },
        }
    }
}

impl From<HttpRequest> for SendRequestInput {
    fn from(request: HttpRequest) -> Self {
        Self {
            method: request.method.as_str().to_string(),
            url: request.url,
            headers: from_pairs(request.headers),
            query_params: from_pairs(request.query_params),
            body: request.body.into(),
            auth: request.auth.into(),
            settings: request.settings.into(),
        }
    }
}

fn from_pairs(pairs: Vec<KeyValue>) -> Vec<KeyValueDto> {
    pairs.into_iter().map(KeyValueDto::from).collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
}

impl From<Collection> for CollectionDto {
    fn from(collection: Collection) -> Self {
        Self {
            id: collection.id,
            name: collection.name,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: String,
    pub collection_id: String,
    pub parent_folder_id: Option<String>,
    pub name: String,
}

impl From<Folder> for FolderDto {
    fn from(folder: Folder) -> Self {
        Self {
            id: folder.id,
            collection_id: folder.collection_id,
            parent_folder_id: folder.parent_folder_id,
            name: folder.name,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedRequestDto {
    pub id: String,
    pub collection_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub request: SendRequestInput,
    /// Whether `request.auth`'s secret loaded. When it did not, the field is
    /// empty and the UI asks for it again.
    pub secret_state: SecretStateDto,
}

impl From<SavedRequest> for SavedRequestDto {
    fn from(saved: SavedRequest) -> Self {
        Self {
            id: saved.id,
            collection_id: saved.collection_id,
            folder_id: saved.folder_id,
            name: saved.name,
            request: saved.request.into(),
            secret_state: saved.secret_state.into(),
        }
    }
}

/// One collection plus its contents — what the sidebar renders in one go.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionContentsDto {
    pub folders: Vec<FolderDto>,
    pub requests: Vec<SavedRequestDto>,
    /// Summaries only — see CollectionContents.
    pub examples: Vec<ExampleSummaryDto>,
}

impl From<CollectionContents> for CollectionContentsDto {
    fn from(contents: CollectionContents) -> Self {
        Self {
            folders: contents.folders.into_iter().map(FolderDto::from).collect(),
            requests: contents
                .requests
                .into_iter()
                .map(SavedRequestDto::from)
                .collect(),
            examples: contents
                .examples
                .into_iter()
                .map(ExampleSummaryDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleSummaryDto {
    pub id: String,
    pub request_id: String,
    pub name: String,
    pub status: u16,
}

impl From<ExampleSummary> for ExampleSummaryDto {
    fn from(summary: ExampleSummary) -> Self {
        Self {
            id: summary.id,
            request_id: summary.request_id,
            name: summary.name,
            status: summary.status,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleDto {
    pub id: String,
    pub request_id: String,
    pub name: String,
    pub created_at: String,
    pub request: SendRequestInput,
    pub status: u16,
    pub response_headers: Vec<KeyValueDto>,
    pub response_body: String,
}

impl From<Example> for ExampleDto {
    fn from(example: Example) -> Self {
        Self {
            id: example.id,
            request_id: example.request_id,
            name: example.name,
            created_at: example.created_at,
            request: example.request.into(),
            status: example.status,
            response_headers: from_pairs(example.response_headers),
            response_body: example.response_body,
        }
    }
}

/// Grouped rather than six loose command parameters, which would be six
/// chances to pass them in the wrong order.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveExampleInput {
    pub request_id: String,
    pub name: String,
    /// The request as sent, already resolved — see the Example model.
    pub request: SendRequestInput,
    pub status: u16,
    pub response_headers: Vec<KeyValueDto>,
    pub response_body: String,
}

/// What the frontend reports after a send completes. Substitution happens in
/// the frontend (src/lib/variables.ts), so Rust never sees the template —
/// `request` is the as-typed version, sent back here explicitly.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordHistoryInput {
    pub resolved_url: String,
    pub status: Option<u16>,
    pub error_kind: Option<String>,
    pub duration_ms: u64,
    pub request: SendRequestInput,
}

impl TryFrom<RecordHistoryInput> for NewHistoryEntry {
    type Error = ApiError;

    fn try_from(input: RecordHistoryInput) -> Result<Self, Self::Error> {
        Ok(Self {
            resolved_url: input.resolved_url,
            status: input.status,
            error_kind: input.error_kind,
            duration_ms: input.duration_ms,
            request: HttpRequest::try_from(input.request)?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryDto {
    pub id: String,
    pub sent_at: String,
    pub resolved_url: String,
    pub status: Option<u16>,
    pub error_kind: Option<String>,
    pub duration_ms: u64,
    pub request: SendRequestInput,
}

impl From<HistoryEntry> for HistoryEntryDto {
    fn from(entry: HistoryEntry) -> Self {
        Self {
            id: entry.id,
            sent_at: entry.sent_at,
            resolved_url: entry.resolved_url,
            status: entry.status,
            error_kind: entry.error_kind,
            duration_ms: entry.duration_ms,
            request: entry.request.into(),
        }
    }
}

/// The outcome of "Send and download". Deliberately carries **no body**: the
/// bytes were written to disk in Rust, and shipping them through IPC as well
/// would defeat the point of downloading rather than rendering.
///
/// `saved_to` is None when the user dismissed the Save As dialog. The request
/// still happened, and its status and timing are still worth showing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResultDto {
    pub status: u16,
    pub headers: Vec<KeyValueDto>,
    pub timing: TimingDto,
    pub byte_length: usize,
    pub saved_to: Option<String>,
}

/// The outcome of an OpenAPI export. `saved_to` is None when the user
/// dismissed the Save As dialog; `notes` is still worth showing either way,
/// because it is the list of things the user would change before exporting
/// again. Sentences, not a union — see commands/openapi.rs.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiExportResultDto {
    pub saved_to: Option<String>,
    pub notes: Vec<String>,
}

/// Which item a Docs tab is for (PLAN.md Phase 12).
///
/// A tagged enum rather than a bare string, so an unknown kind is a
/// deserialisation failure at the boundary rather than a not-found several
/// layers in. serde's lowercase rename matches what TypeScript writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DocsTargetKind {
    Collection,
    Folder,
    Request,
}

impl From<DocsTargetKind> for DocsTarget {
    fn from(kind: DocsTargetKind) -> Self {
        match kind {
            DocsTargetKind::Collection => Self::Collection,
            DocsTargetKind::Folder => Self::Folder,
            DocsTargetKind::Request => Self::Request,
        }
    }
}

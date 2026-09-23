// http_client/src-tauri/src/commands/dto.rs
//
// The wire format between Rust and TypeScript. Mirrored by src/types/http.ts;
// camelCase on both sides. Durations cross as whole milliseconds — the UI has
// no use for nanosecond precision.
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::commands::error::ApiError;
use crate::domain::models::{
    ApiKeyLocation, Auth, ClosedBy, Collection, Cookie, Environment, EnvironmentVariable, Example,
    ExampleSummary, Folder, HistoryEntry, HttpMethod, HttpRequest, HttpResponse,
    HttpVersionPreference, KeyValue, MultipartPart, NewHistoryEntry, RequestBody, RequestSettings,
    ResponseBody, SavedRequest, SavedWebSocket, Timing, TlsMinimum, TransferSizes,
    WebSocketRequest, WebSocketSettings, WsBinaryEncoding, WsDraft, WsEvent, WsMessageFormat,
    WsPayload,
};
use crate::domain::ports::HttpStreamUpdate;
use crate::domain::secrets::SecretState;
use crate::domain::services::collections::CollectionContents;
use crate::domain::services::docs::DocsTarget;
use crate::domain::sse::{SseBlock, SseBlockKind};

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

/// Mirrors TransferSizes. Bytes, not milliseconds: the UI picks the unit.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSizesDto {
    pub request_headers: u64,
    pub request_body: u64,
    pub response_headers: u64,
    pub response_body: u64,
}

impl From<TransferSizes> for TransferSizesDto {
    fn from(sizes: TransferSizes) -> Self {
        Self {
            request_headers: sizes.request_headers,
            request_body: sizes.request_body,
            response_headers: sizes.response_headers,
            response_body: sizes.response_body,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponseDto {
    pub status: u16,
    pub headers: Vec<KeyValueDto>,
    pub body: ResponseBodyDto,
    pub timing: TimingDto,
    pub sizes: TransferSizesDto,
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
            sizes: response.sizes.into(),
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

/// Mirrors WsMessageFormat. Mirrored in src/types/websocket.ts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WsMessageFormatDto {
    Text,
    Json,
    Xml,
    Html,
    Binary,
}

impl From<WsMessageFormatDto> for WsMessageFormat {
    fn from(dto: WsMessageFormatDto) -> Self {
        match dto {
            WsMessageFormatDto::Text => Self::Text,
            WsMessageFormatDto::Json => Self::Json,
            WsMessageFormatDto::Xml => Self::Xml,
            WsMessageFormatDto::Html => Self::Html,
            WsMessageFormatDto::Binary => Self::Binary,
        }
    }
}

impl From<WsMessageFormat> for WsMessageFormatDto {
    fn from(format: WsMessageFormat) -> Self {
        match format {
            WsMessageFormat::Text => Self::Text,
            WsMessageFormat::Json => Self::Json,
            WsMessageFormat::Xml => Self::Xml,
            WsMessageFormat::Html => Self::Html,
            WsMessageFormat::Binary => Self::Binary,
        }
    }
}

/// Mirrors WsBinaryEncoding. Mirrored in src/types/websocket.ts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum WsBinaryEncodingDto {
    #[default]
    Base64,
    Hex,
}

impl From<WsBinaryEncodingDto> for WsBinaryEncoding {
    fn from(dto: WsBinaryEncodingDto) -> Self {
        match dto {
            WsBinaryEncodingDto::Base64 => Self::Base64,
            WsBinaryEncodingDto::Hex => Self::Hex,
        }
    }
}

impl From<WsBinaryEncoding> for WsBinaryEncodingDto {
    fn from(encoding: WsBinaryEncoding) -> Self {
        match encoding {
            WsBinaryEncoding::Base64 => Self::Base64,
            WsBinaryEncoding::Hex => Self::Hex,
        }
    }
}

/// Durations cross as whole milliseconds and sizes as a plain number of
/// bytes, as everywhere else in this file. A JS number holds any size this
/// app would set exactly.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketSettingsDto {
    pub verify_tls: bool,
    pub proxy: Option<String>,
    pub send_cookies: bool,
    pub connect_timeout_ms: u64,
    pub max_message_bytes: u64,
    pub auto_reconnect: bool,
}

impl From<WebSocketSettingsDto> for WebSocketSettings {
    fn from(dto: WebSocketSettingsDto) -> Self {
        Self {
            verify_tls: dto.verify_tls,
            proxy: dto.proxy,
            send_cookies: dto.send_cookies,
            connect_timeout: Duration::from_millis(dto.connect_timeout_ms),
            max_message_bytes: usize::try_from(dto.max_message_bytes).unwrap_or(usize::MAX),
            auto_reconnect: dto.auto_reconnect,
        }
    }
}

impl From<WebSocketSettings> for WebSocketSettingsDto {
    fn from(settings: WebSocketSettings) -> Self {
        Self {
            verify_tls: settings.verify_tls,
            proxy: settings.proxy,
            send_cookies: settings.send_cookies,
            connect_timeout_ms: millis(settings.connect_timeout),
            max_message_bytes: u64::try_from(settings.max_message_bytes).unwrap_or(u64::MAX),
            auto_reconnect: settings.auto_reconnect,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketRequestDto {
    pub url: String,
    pub headers: Vec<KeyValueDto>,
    pub settings: WebSocketSettingsDto,
}

impl From<WebSocketRequestDto> for WebSocketRequest {
    fn from(dto: WebSocketRequestDto) -> Self {
        Self {
            url: dto.url,
            headers: into_pairs(dto.headers),
            settings: dto.settings.into(),
        }
    }
}

impl From<WebSocketRequest> for WebSocketRequestDto {
    fn from(request: WebSocketRequest) -> Self {
        Self {
            url: request.url,
            headers: from_pairs(request.headers),
            settings: request.settings.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsDraftDto {
    pub format: WsMessageFormatDto,
    #[serde(default)]
    pub binary_encoding: WsBinaryEncodingDto,
    pub text: String,
}

impl From<WsDraftDto> for WsDraft {
    fn from(dto: WsDraftDto) -> Self {
        Self {
            format: dto.format.into(),
            binary_encoding: dto.binary_encoding.into(),
            text: dto.text,
        }
    }
}

impl From<WsDraft> for WsDraftDto {
    fn from(draft: WsDraft) -> Self {
        Self {
            format: draft.format.into(),
            binary_encoding: draft.binary_encoding.into(),
            text: draft.text,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedWebSocketDto {
    pub id: String,
    pub collection_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub request: WebSocketRequestDto,
    pub draft: WsDraftDto,
}

impl From<SavedWebSocket> for SavedWebSocketDto {
    fn from(saved: SavedWebSocket) -> Self {
        Self {
            id: saved.id,
            collection_id: saved.collection_id,
            folder_id: saved.folder_id,
            name: saved.name,
            request: saved.request.into(),
            draft: saved.draft.into(),
        }
    }
}

/// Grouped, as SaveExampleInput is, rather than six loose command parameters.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebSocketInput {
    /// None saves a new WebSocket request; an existing id overwrites it.
    pub id: Option<String>,
    pub collection_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub request: WebSocketRequestDto,
    pub draft: WsDraftDto,
}

/// Mirrors SseBlock: one block of a `text/event-stream` response
/// (PLAN-SSE.md). Mirrored in src/types/http.ts.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SseBlockDto {
    #[serde(rename_all = "camelCase")]
    Event {
        name: String,
        data: String,
        id: Option<String>,
        /// Milliseconds. Shown, not acted on: this app does not reconnect.
        retry: Option<u64>,
        raw: String,
    },
    #[serde(rename_all = "camelCase")]
    Comment { text: String, raw: String },
}

impl From<SseBlock> for SseBlockDto {
    fn from(block: SseBlock) -> Self {
        let raw = block.raw;
        match block.kind {
            SseBlockKind::Event {
                name,
                data,
                id,
                retry,
            } => Self::Event {
                name,
                data,
                id,
                retry,
                raw,
            },
            SseBlockKind::Comment { text } => Self::Comment { text, raw },
        }
    }
}

/// Mirrors HttpStreamUpdate: what a request reports before it finishes.
/// Tagged on `type`, as WsEventDto is, because the block inside uses `kind`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HttpStreamEventDto {
    #[serde(rename_all = "camelCase")]
    Headers {
        status: u16,
        headers: Vec<KeyValueDto>,
        /// The header block's size, so the running total has both halves.
        bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    Block {
        at_ms: u64,
        /// The block's own bytes on the wire. Counted here because `raw` is
        /// UTF-8 and JavaScript's `length` counts UTF-16 units, which would
        /// undercount every multi-byte character the stream carries.
        bytes: usize,
        block: SseBlockDto,
    },
}

impl From<HttpStreamUpdate> for HttpStreamEventDto {
    fn from(update: HttpStreamUpdate) -> Self {
        match update {
            HttpStreamUpdate::Headers {
                status,
                headers,
                bytes,
            } => Self::Headers {
                status,
                headers: from_pairs(headers),
                bytes,
            },
            HttpStreamUpdate::Block { at_ms, block } => Self::Block {
                at_ms,
                bytes: block.raw.len(),
                block: block.into(),
            },
        }
    }
}

/// Mirrors WsPayload, both ways: what the composer sends and what the log
/// shows. Binary crosses as hex because the UI shows it as hex anyway, and
/// a JSON array of numbers would be almost twice the size.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WsPayloadDto {
    Text { text: String },
    Binary { hex: String },
}

impl TryFrom<WsPayloadDto> for WsPayload {
    type Error = ApiError;

    fn try_from(dto: WsPayloadDto) -> Result<Self, Self::Error> {
        match dto {
            WsPayloadDto::Text { text } => Ok(Self::Text(text)),
            WsPayloadDto::Binary { hex } => decode_hex(&hex)
                .map(Self::Binary)
                .map_err(|message| ApiError::InvalidRequest { message }),
        }
    }
}

impl From<WsPayload> for WsPayloadDto {
    fn from(payload: WsPayload) -> Self {
        match payload {
            WsPayload::Text(text) => Self::Text { text },
            WsPayload::Binary(bytes) => Self::Binary {
                hex: encode_hex(&bytes),
            },
        }
    }
}

/// Lowercase, no separators: the one spelling both sides agree on.
fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    hex
}

/// Strict: pairs of hex digits and nothing else. The composer's leniency
/// (spaces, a `0x` prefix) is the frontend's to strip before it sends, so
/// that rule lives in one place.
fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.len().is_multiple_of(2) {
        return Err("hex payload has an odd number of digits".into());
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| match (hex_digit(pair[0]), hex_digit(pair[1])) {
            (Some(high), Some(low)) => Ok(high << 4 | low),
            _ => Err("hex payload contains a character that is not a hex digit".into()),
        })
        .collect()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Mirrors ClosedBy.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WsClosedByDto {
    User,
    Server,
    Error,
}

impl From<ClosedBy> for WsClosedByDto {
    fn from(by: ClosedBy) -> Self {
        match by {
            ClosedBy::User => Self::User,
            ClosedBy::Server => Self::Server,
            ClosedBy::Error => Self::Error,
        }
    }
}

/// Mirrors WsEvent: one message on a connection's IPC channel. Tagged on
/// `type` rather than `kind`, because the payload inside already uses
/// `kind`. `byteLength` is the size on the wire, which for text is not the
/// JS string length.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsEventDto {
    #[serde(rename_all = "camelCase")]
    Connected {
        at_ms: u64,
        url: String,
        status: u16,
        headers: Vec<KeyValueDto>,
    },
    #[serde(rename_all = "camelCase")]
    Sent {
        at_ms: u64,
        byte_length: u64,
        payload: WsPayloadDto,
    },
    #[serde(rename_all = "camelCase")]
    Received {
        at_ms: u64,
        byte_length: u64,
        payload: WsPayloadDto,
    },
    #[serde(rename_all = "camelCase")]
    Closed {
        at_ms: u64,
        code: Option<u16>,
        reason: String,
        by: WsClosedByDto,
    },
    #[serde(rename_all = "camelCase")]
    Error { at_ms: u64, message: String },
    #[serde(rename_all = "camelCase")]
    Reconnecting {
        at_ms: u64,
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
    },
}

fn byte_length(payload: &WsPayload) -> u64 {
    u64::try_from(payload.byte_length()).unwrap_or(u64::MAX)
}

impl From<WsEvent> for WsEventDto {
    fn from(event: WsEvent) -> Self {
        match event {
            WsEvent::Connected {
                at_ms,
                url,
                status,
                headers,
            } => Self::Connected {
                at_ms,
                url,
                status,
                headers: from_pairs(headers),
            },
            WsEvent::Sent { at_ms, payload } => Self::Sent {
                at_ms,
                byte_length: byte_length(&payload),
                payload: payload.into(),
            },
            WsEvent::Received { at_ms, payload } => Self::Received {
                at_ms,
                byte_length: byte_length(&payload),
                payload: payload.into(),
            },
            WsEvent::Closed {
                at_ms,
                code,
                reason,
                by,
            } => Self::Closed {
                at_ms,
                code,
                reason,
                by: by.into(),
            },
            WsEvent::Error { at_ms, message } => Self::Error { at_ms, message },
            WsEvent::Reconnecting {
                at_ms,
                attempt,
                max_attempts,
                delay,
            } => Self::Reconnecting {
                at_ms,
                attempt,
                max_attempts,
                delay_ms: millis(delay),
            },
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
    pub web_sockets: Vec<SavedWebSocketDto>,
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
            web_sockets: contents
                .web_sockets
                .into_iter()
                .map(SavedWebSocketDto::from)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// src/types/websocket.ts mirrors these names exactly. A rename here
    /// that is not made there fails silently at runtime, so the keys the
    /// frontend reads are pinned.
    #[test]
    fn a_saved_web_socket_crosses_the_boundary_in_camel_case() {
        let dto = SavedWebSocketDto::from(SavedWebSocket {
            id: "req_1".into(),
            collection_id: "col_1".into(),
            folder_id: None,
            name: "Echo".into(),
            request: WebSocketRequest {
                url: "wss://echo.websocket.org".into(),
                headers: vec![KeyValue::new("X-Trace", "1")],
                settings: WebSocketSettings::default(),
            },
            draft: WsDraft {
                format: WsMessageFormat::Binary,
                binary_encoding: WsBinaryEncoding::Hex,
                text: "de ad".into(),
            },
        });

        let json = serde_json::to_value(&dto).expect("should serialise");

        assert_eq!(json["collectionId"], "col_1");
        assert!(json["folderId"].is_null());
        assert_eq!(json["request"]["url"], "wss://echo.websocket.org");
        assert_eq!(json["request"]["headers"][0]["name"], "X-Trace");
        let settings = &json["request"]["settings"];
        assert_eq!(settings["verifyTls"], true);
        assert_eq!(settings["sendCookies"], true);
        assert_eq!(settings["connectTimeoutMs"], 30_000);
        assert_eq!(settings["maxMessageBytes"], 1_048_576);
        assert_eq!(settings["autoReconnect"], false);
        assert_eq!(json["draft"]["format"], "binary");
        assert_eq!(json["draft"]["binaryEncoding"], "hex");
    }

    #[test]
    fn a_save_from_the_frontend_deserialises_into_the_domain_shape() {
        let input: SaveWebSocketInput = serde_json::from_str(
            r#"{
                "id": null,
                "collectionId": "col_1",
                "folderId": "fld_1",
                "name": "Echo",
                "request": {
                    "url": "wss://echo.websocket.org",
                    "headers": [],
                    "settings": {
                        "verifyTls": true,
                        "proxy": null,
                        "sendCookies": false,
                        "connectTimeoutMs": 5000,
                        "maxMessageBytes": 4096,
                        "autoReconnect": true
                    }
                },
                "draft": { "format": "binary", "binaryEncoding": "base64", "text": "3q0=" }
            }"#,
        )
        .expect("the frontend's payload should deserialise");

        let request = WebSocketRequest::from(input.request);
        let draft = WsDraft::from(input.draft);

        assert_eq!(input.folder_id.as_deref(), Some("fld_1"));
        assert!(!request.settings.send_cookies);
        assert_eq!(
            request.settings.connect_timeout,
            Duration::from_millis(5000)
        );
        assert_eq!(request.settings.max_message_bytes, 4096);
        assert!(request.settings.auto_reconnect);
        assert_eq!(draft.format, WsMessageFormat::Binary);
        assert_eq!(draft.binary_encoding, WsBinaryEncoding::Base64);
    }

    /// src/types/http.ts switches on these keys while a stream is running.
    #[test]
    fn a_streaming_update_crosses_the_boundary_in_camel_case() {
        let json = |update: HttpStreamUpdate| {
            serde_json::to_value(HttpStreamEventDto::from(update)).expect("should serialise")
        };

        let headers = json(HttpStreamUpdate::Headers {
            status: 200,
            headers: vec![KeyValue::new("content-type", "text/event-stream")],
            bytes: 120,
        });
        assert_eq!(headers["type"], "headers");
        assert_eq!(headers["status"], 200);
        assert_eq!(headers["bytes"], 120);
        assert_eq!(headers["headers"][0]["value"], "text/event-stream");

        let event = json(HttpStreamUpdate::Block {
            at_ms: 7,
            block: SseBlock {
                kind: SseBlockKind::Event {
                    name: "end".into(),
                    data: "Stream ended".into(),
                    id: Some("42".into()),
                    retry: Some(3000),
                },
                raw: "event: end\ndata: Stream ended\n".into(),
            },
        });
        assert_eq!(event["type"], "block");
        assert_eq!(event["atMs"], 7);
        assert_eq!(event["bytes"], 30);
        assert_eq!(event["block"]["kind"], "event");
        assert_eq!(event["block"]["name"], "end");
        assert_eq!(event["block"]["data"], "Stream ended");
        assert_eq!(event["block"]["id"], "42");
        assert_eq!(event["block"]["retry"], 3000);
        assert_eq!(event["block"]["raw"], "event: end\ndata: Stream ended\n");

        let comment = json(HttpStreamUpdate::Block {
            at_ms: 8,
            block: SseBlock {
                kind: SseBlockKind::Comment {
                    text: "keep-alive".into(),
                },
                raw: ": keep-alive\n".into(),
            },
        });
        assert_eq!(comment["block"]["kind"], "comment");
        assert_eq!(comment["block"]["text"], "keep-alive");
        assert!(comment["block"]["name"].is_null());
    }

    /// The spellings src/types/websocket.ts sends, and the fallback for a
    /// draft that predates the binary encoding.
    #[test]
    fn every_message_format_and_encoding_has_its_frontend_spelling() {
        for (wire, format) in [
            ("text", WsMessageFormat::Text),
            ("json", WsMessageFormat::Json),
            ("xml", WsMessageFormat::Xml),
            ("html", WsMessageFormat::Html),
            ("binary", WsMessageFormat::Binary),
        ] {
            let dto: WsMessageFormatDto =
                serde_json::from_value(serde_json::json!(wire)).expect("should parse");
            assert_eq!(WsMessageFormat::from(dto), format);
        }
        let draft: WsDraftDto = serde_json::from_str(r#"{ "format": "xml", "text": "<a/>" }"#)
            .expect("a draft without binaryEncoding should parse");
        assert_eq!(
            WsDraft::from(draft).binary_encoding,
            WsBinaryEncoding::Base64
        );
    }

    /// src/types/websocket.ts switches on `type` and reads these keys. Every
    /// variant is pinned, because a rename that misses the frontend drops
    /// events silently instead of failing.
    #[test]
    fn every_websocket_event_crosses_the_boundary_in_camel_case() {
        let json = |event: WsEvent| {
            serde_json::to_value(WsEventDto::from(event)).expect("should serialise")
        };

        let connected = json(WsEvent::Connected {
            at_ms: 1,
            url: "wss://a.test/".into(),
            status: 101,
            headers: vec![KeyValue::new("Upgrade", "websocket")],
        });
        assert_eq!(connected["type"], "connected");
        assert_eq!(connected["atMs"], 1);
        assert_eq!(connected["status"], 101);
        assert_eq!(connected["headers"][0]["name"], "Upgrade");

        let sent = json(WsEvent::Sent {
            at_ms: 2,
            payload: WsPayload::Text("é".into()),
        });
        assert_eq!(sent["type"], "sent");
        assert_eq!(sent["byteLength"], 2);
        assert_eq!(sent["payload"]["kind"], "text");
        assert_eq!(sent["payload"]["text"], "é");

        let received = json(WsEvent::Received {
            at_ms: 3,
            payload: WsPayload::Binary(vec![0xde, 0xad, 0x0f]),
        });
        assert_eq!(received["type"], "received");
        assert_eq!(received["byteLength"], 3);
        assert_eq!(received["payload"]["kind"], "binary");
        assert_eq!(received["payload"]["hex"], "dead0f");

        let closed = json(WsEvent::Closed {
            at_ms: 4,
            code: Some(1000),
            reason: "bye".into(),
            by: ClosedBy::Server,
        });
        assert_eq!(closed["type"], "closed");
        assert_eq!(closed["code"], 1000);
        assert_eq!(closed["reason"], "bye");
        assert_eq!(closed["by"], "server");
        let no_code = json(WsEvent::Closed {
            at_ms: 4,
            code: None,
            reason: String::new(),
            by: ClosedBy::Error,
        });
        assert!(no_code["code"].is_null());
        assert_eq!(no_code["by"], "error");

        let error = json(WsEvent::Error {
            at_ms: 5,
            message: "boom".into(),
        });
        assert_eq!(error["type"], "error");
        assert_eq!(error["message"], "boom");

        let reconnecting = json(WsEvent::Reconnecting {
            at_ms: 6,
            attempt: 2,
            max_attempts: 10,
            delay: Duration::from_secs(2),
        });
        assert_eq!(reconnecting["type"], "reconnecting");
        assert_eq!(reconnecting["maxAttempts"], 10);
        assert_eq!(reconnecting["delayMs"], 2000);
    }

    #[test]
    fn an_outgoing_message_from_the_composer_deserialises() {
        let text: WsPayloadDto =
            serde_json::from_str(r#"{ "kind": "text", "text": "hi" }"#).expect("text");
        let binary: WsPayloadDto =
            serde_json::from_str(r#"{ "kind": "binary", "hex": "DEad" }"#).expect("binary");

        assert_eq!(
            WsPayload::try_from(text).ok(),
            Some(WsPayload::Text("hi".into()))
        );
        assert_eq!(
            WsPayload::try_from(binary).ok(),
            Some(WsPayload::Binary(vec![0xde, 0xad]))
        );
    }

    #[test]
    fn hex_round_trips_every_byte() {
        let bytes: Vec<u8> = (0..=255).collect();

        let hex = encode_hex(&bytes);

        assert_eq!(&hex[..6], "000102");
        assert_eq!(&hex[hex.len() - 4..], "feff");
        assert_eq!(decode_hex(&hex), Ok(bytes));
        assert_eq!(decode_hex(""), Ok(Vec::new()));
    }

    /// Anything but bare digit pairs is the frontend's to clean up, so the
    /// Rust side refuses it rather than guessing.
    #[test]
    fn malformed_hex_is_an_invalid_request() {
        for bad in ["abc", "zz", "de ad", "0xde", "é0"] {
            let result = WsPayload::try_from(WsPayloadDto::Binary { hex: bad.into() });
            assert!(
                matches!(result, Err(ApiError::InvalidRequest { .. })),
                "{bad:?} should be refused"
            );
        }
    }
}

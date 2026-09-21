// http_client/src-tauri/src/domain/models.rs
//
// Domain models. No serde here: DTOs crossing the Tauri boundary live in
// commands/dto.rs, so the wire format can change without dragging the
// domain along.
use std::path::PathBuf;
use std::time::Duration;

use crate::domain::secrets::SecretState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }

    /// The single source of truth for which methods exist — the TypeScript
    /// union in src/types/http.ts mirrors this list.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }
}

/// One name/value pair. Request headers, query parameters and form fields
/// are all this shape, so they share a type rather than three identical ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    pub name: String,
    pub value: String,
}

impl KeyValue {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestBody {
    None,
    /// JSON, XML and plain text are all this: the content type decides how
    /// the editor highlights it, not a separate domain variant.
    Raw {
        content_type: String,
        text: String,
    },
    FormUrlEncoded(Vec<KeyValue>),
    Multipart(Vec<MultipartPart>),
}

/// One part of a multipart body. A file part carries the **path**, not the
/// bytes: libcurl opens and reads the file itself during the transfer, so
/// nothing here ever holds an upload in memory. The consequence is that a
/// saved request stores a path — move the file and the request stops working,
/// which is why the path is validated before a send rather than failing as a
/// transport error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartPart {
    Text {
        name: String,
        value: String,
    },
    File {
        name: String,
        path: PathBuf,
        /// None lets the server decide. Filled in from the extension when the
        /// file is chosen, so the user sees what will be sent.
        content_type: Option<String>,
    },
}

impl MultipartPart {
    pub fn name(&self) -> &str {
        match self {
            Self::Text { name, .. } | Self::File { name, .. } => name,
        }
    }
}

impl RequestBody {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::None => true,
            Self::Raw { text, .. } => text.is_empty(),
            Self::FormUrlEncoded(fields) => fields.is_empty(),
            Self::Multipart(parts) => parts.is_empty(),
        }
    }
}

/// Where an API key travels. A key in the query string ends up in server
/// logs and proxy history, so the header is the default the UI offers first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

/// How a request authenticates. `None` is a variant rather than
/// `Option<Auth>` so "no auth" is something the user picks and sees, not an
/// absence. Adding a scheme means a variant here plus an `AuthStrategy` in
/// http/auth.rs — never a new branch inside the curl client.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Auth {
    #[default]
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyLocation,
    },
    Custom {
        header_name: String,
        header_value: String,
    },
}

/// Which HTTP version to negotiate. `Auto` is not "let libcurl decide" — it
/// is the behaviour this app has always had: attempt HTTP/2 over TLS and fall
/// back to 1.1. Keeping it as the default means adding this setting changes
/// nothing for a request that does not touch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpVersionPreference {
    #[default]
    Auto,
    /// Forces 1.1, which is the workaround when a server mishandles h2.
    Http11,
    /// Forces h2 and fails rather than falling back.
    Http2,
}

/// Lowest TLS version to accept. rustls supports 1.2 and 1.3 and nothing
/// older, so there is
/// no TLS 1.0 or 1.1 here to switch off — the floor is already 1.2.
/// `Auto` leaves libcurl's own default alone rather than asserting one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsMinimum {
    #[default]
    Auto,
    Tls12,
    Tls13,
}

/// Per-request transport settings. Modelled here rather than hardcoded in
/// the client so every one of them is visible and overridable in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestSettings {
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub timeout: Duration,
    /// Off is an explicit, per-request user opt-in and never the default
    /// (CLAUDE.md section 11, rule 7).
    pub verify_tls: bool,
    pub proxy: Option<String>,
    /// Off lets one request run without the jar — proving an endpoint
    /// rejects an unauthenticated call, without clearing every cookie.
    pub send_cookies: bool,
    pub http_version: HttpVersionPreference,
    /// Keep the original method across a redirect. libcurl's default (and the
    /// browsers') turns POST into GET on 301/302/303; some APIs expect the
    /// method to survive.
    pub keep_method_on_redirect: bool,
    /// Keep the Authorization header when a redirect crosses to another host.
    /// Off by default because on is how credentials leak to a host that was
    /// never meant to see them.
    pub keep_auth_on_redirect: bool,
    /// "Encode URL automatically": percent-encode what a URL cannot carry
    /// (spaces, non-ASCII, quotes, …) when sending, leaving text that is
    /// already valid — including existing `%XX` escapes — exactly as typed.
    /// Off sends the URL as written. See `http::mapping::encode_url_for_send`.
    pub encode_url: bool,
    /// Accept a bodies-only HTTP/0.9 response. Off matches libcurl's own
    /// default; on exists for talking to something ancient or hand-rolled.
    pub allow_http_09: bool,
    pub tls_minimum: TlsMinimum,
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_REDIRECTS: u32 = 10;

impl Default for RequestSettings {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            timeout: DEFAULT_TIMEOUT,
            verify_tls: true,
            proxy: None,
            send_cookies: true,
            http_version: HttpVersionPreference::Auto,
            keep_method_on_redirect: false,
            keep_auth_on_redirect: false,
            encode_url: true,
            allow_http_09: false,
            tls_minimum: TlsMinimum::Auto,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub query_params: Vec<KeyValue>,
    pub body: RequestBody,
    pub auth: Auth,
    pub settings: RequestSettings,
}

/// Bodies may be binary, so the transport collects bytes and the decision
/// about text happens once, here, rather than in the viewer.
///
/// `Binary` keeps the bytes rather than only their length: they are needed to
/// write a downloaded file to disk. They do **not** cross the Tauri boundary —
/// `ResponseBodyDto` still sends only `byteLength`, so a 200 MB download is
/// never serialised through IPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseBody {
    Text(String),
    Binary { bytes: Vec<u8> },
}

impl ResponseBody {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        match String::from_utf8(bytes) {
            Ok(text) => Self::Text(text),
            Err(err) => Self::Binary {
                bytes: err.into_bytes(),
            },
        }
    }

    /// What a download writes. Text is re-encoded rather than kept as bytes
    /// alongside, so there is one representation and no chance of the two
    /// disagreeing.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Binary { bytes } => bytes,
        }
    }

    pub fn byte_length(&self) -> usize {
        self.as_bytes().len()
    }
}

/// Phase breakdown as libcurl measures it, already turned into per-phase
/// durations rather than the cumulative offsets curl reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timing {
    pub dns: Duration,
    pub connect: Duration,
    pub tls: Duration,
    pub time_to_first_byte: Duration,
    pub total: Duration,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<KeyValue>,
    pub body: ResponseBody,
    pub timing: Timing,
    /// Raw Set-Cookie lines from every redirect hop, not just the last one.
    /// `headers` deliberately keeps only the final response, but a login
    /// flow sets its cookie on the 302, so the jar needs all of them.
    pub set_cookies: Vec<String>,
}

/// One stored cookie. Identity is (domain, path, name) per RFC 6265, and
/// `expires_at` of None means a session cookie, which dies with the app.
/// Times are unix seconds so the matching rules stay free of date handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires_at: Option<u64>,
    pub secure: bool,
    pub http_only: bool,
    /// True when Set-Cookie carried no Domain: the cookie then belongs to
    /// exactly that host and must not reach its subdomains.
    pub host_only: bool,
    pub created_at: u64,
}

/// One `{{variable}}` binding. A secret one is encrypted at rest and masked
/// in the editor; it resolves exactly like any other (PLAN.md Phase 9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
    pub secret: bool,
    /// Only meaningful on the way out of storage. A value that could not be
    /// read arrives empty, with this saying why.
    pub state: SecretState,
}

impl EnvironmentVariable {
    pub fn plain(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            secret: false,
            state: SecretState::Ok,
        }
    }

    pub fn secret(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            secret: true,
            ..Self::plain(name, value)
        }
    }
}

/// A named set of `{{variable}}` bindings. Which one is active is a UI
/// concern, so it is not modelled here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub id: String,
    pub name: String,
}

/// A named group of saved requests. The top level of the sidebar tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub id: String,
    pub name: String,
}

/// One level of grouping inside a collection. `parent_folder_id` allows
/// nesting; the sidebar renders whatever depth exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub collection_id: String,
    pub parent_folder_id: Option<String>,
    pub name: String,
}

/// A request the user saved, with everything needed to reproduce it.
#[derive(Debug, Clone)]
pub struct SavedRequest {
    pub id: String,
    pub collection_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub request: HttpRequest,
    /// Whether the auth secret could be read back. Always `Ok` on the way
    /// in; a request whose secret was lost still loads, with the field empty.
    pub secret_state: SecretState,
}

/// One request that was actually sent, kept so it can be inspected, re-run,
/// or promoted into a collection. `request` is the template the user typed,
/// `{{placeholders}}` intact — a re-run therefore resolves against whichever
/// environment is active then, not the one that happened to be active that
/// day. `resolved_url` is what went over the wire, kept only so the list
/// reads and searches sensibly.
///
/// `status` is None when the request never produced a response (transport
/// failure, cancellation); `error_kind` is None when it did.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: String,
    pub sent_at: String,
    pub resolved_url: String,
    pub status: Option<u16>,
    pub error_kind: Option<String>,
    pub duration_ms: u64,
    pub request: HttpRequest,
}

/// A history entry before storage assigns it an id and a timestamp, matching
/// how collections, folders and environments are created.
#[derive(Debug, Clone)]
pub struct NewHistoryEntry {
    pub resolved_url: String,
    pub status: Option<u16>,
    pub error_kind: Option<String>,
    pub duration_ms: u64,
    pub request: HttpRequest,
}

/// A named response saved under a request.
/// It carries the request snapshot that produced it, so several examples
/// under one request (a success, a 404, a validation error) each say which
/// inputs got there.
///
/// The snapshot is the request **as sent**: resolved, unlike `SavedRequest`
/// and `HistoryEntry`, which keep `{{placeholders}}` so they can be re-run
/// against a different environment. An example is a record of one exchange
/// that happened, so resolving it later would make it a record of nothing.
#[derive(Debug, Clone)]
pub struct Example {
    pub id: String,
    pub request_id: String,
    pub name: String,
    pub created_at: String,
    pub request: HttpRequest,
    pub status: u16,
    pub response_headers: Vec<KeyValue>,
    /// Text only. A binary response never gets this far — `ResponseBody`
    /// keeps only its length — so there is nothing to store for one.
    pub response_body: String,
}

/// An example before storage assigns it an id and a timestamp.
#[derive(Debug, Clone)]
pub struct NewExample {
    pub request_id: String,
    pub name: String,
    pub request: HttpRequest,
    pub status: u16,
    pub response_headers: Vec<KeyValue>,
    pub response_body: String,
}

/// Enough to draw an example in the sidebar tree. The tree shows every
/// example in a collection at once and a body may be a megabyte, so the
/// bodies stay in storage until one is actually opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleSummary {
    pub id: String,
    pub request_id: String,
    pub name: String,
    pub status: u16,
}

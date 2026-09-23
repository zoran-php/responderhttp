// http_client/src-tauri/src/persistence/repositories/json.rs
//
// The stored shape of a request's variable parts. Kept separate from the
// command DTOs: the wire format and the storage format are free to change
// independently, and the domain stays serde-free.
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::domain::error::AppError;
use std::path::PathBuf;

use crate::domain::models::{
    ApiKeyLocation, Auth, HttpVersionPreference, KeyValue, MultipartPart, RequestBody,
    RequestSettings, TlsMinimum, WebSocketSettings, WsBinaryEncoding, WsDraft, WsMessageFormat,
    DEFAULT_WS_CONNECT_TIMEOUT, DEFAULT_WS_MAX_MESSAGE_BYTES,
};
use crate::domain::ports::{OpenedSecret, SecretCipher};
use crate::domain::secrets::{
    auth_secret, auth_without_literal_secret, has_literal_auth_secret, secret_scope,
    with_auth_secret, SecretState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredKeyValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum StoredBody {
    None,
    Raw {
        content_type: String,
        text: String,
    },
    FormUrlEncoded {
        fields: Vec<StoredKeyValue>,
    },
    Multipart {
        /// Was `fields` when multipart was text-only. Migration 0001 is
        /// shipped and must not be edited (CLAUDE.md section 11, rule 5), and
        /// a request saved before file parts existed would otherwise fail to
        /// deserialise — taking the whole load down, not just its body.
        #[serde(alias = "fields")]
        parts: Vec<StoredMultipartPart>,
    },
}

/// Reads both shapes. Untagged tries the variants in order, so the tagged
/// form is first: a new row carries `kind`, an old one is a bare
/// name/value pair and matches only `Legacy`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StoredMultipartPart {
    Tagged(TaggedMultipartPart),
    Legacy(StoredKeyValue),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TaggedMultipartPart {
    Text {
        name: String,
        value: String,
    },
    File {
        name: String,
        /// The path, never the bytes. A collection copied to another machine
        /// does not carry its uploads.
        path: String,
        content_type: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum StoredApiKeyLocation {
    Header,
    Query,
}

/// A secret field as stored. Rows written before Phase 9 hold a bare string,
/// which still reads as `Plain`; the startup upgrade seals those. Untagged
/// tries `Sealed` first, so an object with a `sealed` key is never mistaken
/// for anything else.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum StoredSecret {
    /// Base64 of the envelope (secrets/envelope.rs).
    Sealed { sealed: String },
    /// Empty, a `{{placeholder}}` in history or an example, or a row from
    /// before Phase 9.
    Plain(String),
}

impl StoredSecret {
    fn plain_text(&self) -> Option<&str> {
        match self {
            Self::Plain(text) => Some(text),
            Self::Sealed { .. } => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum StoredAuth {
    None,
    Basic {
        username: String,
        password: StoredSecret,
    },
    Bearer {
        token: StoredSecret,
    },
    ApiKey {
        key: String,
        value: StoredSecret,
        location: StoredApiKeyLocation,
    },
    Custom {
        header_name: String,
        header_value: StoredSecret,
    },
}

impl StoredAuth {
    fn secret(&self) -> Option<&StoredSecret> {
        match self {
            Self::None => None,
            Self::Basic { password, .. } => Some(password),
            Self::Bearer { token } => Some(token),
            Self::ApiKey { value, .. } => Some(value),
            Self::Custom { header_value, .. } => Some(header_value),
        }
    }
}

/// How a row's auth secret is written. Every repository that stores an auth
/// goes through one of these, so there is no write path that skips both.
pub enum SecretWrite<'a> {
    /// Saved requests: sealed, and bound to this row.
    Seal {
        cipher: &'a dyn SecretCipher,
        table: &'static str,
        row_id: &'a str,
    },
    /// History and examples keep no secrets. A literal value is blanked; a
    /// whole-value `{{placeholder}}` is kept. The services strip first; this
    /// is the backstop that makes the rule hold for any caller.
    Strip,
}

/// How a row's auth secret is read back.
pub enum SecretRead<'a> {
    Open {
        cipher: &'a dyn SecretCipher,
        table: &'static str,
        row_id: &'a str,
    },
    /// History and examples: nothing sealed is expected there.
    PlainOnly,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredSettings {
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub timeout_ms: u64,
    pub verify_tls: bool,
    pub proxy: Option<String>,
    /// Every request saved before cookies existed has no such key. A bare
    /// `#[serde(default)]` would yield false and silently turn cookies off
    /// for all of them, so the default is spelled out.
    #[serde(default = "enabled")]
    pub send_cookies: bool,

    // The transport settings added 2026-09-14. Each is defaulted for the same
    // reason as send_cookies above: the key is absent from every row written
    // before today. `encode_url` is the one that would actually hurt — a bare
    // `#[serde(default)]` gives false, which would silently stop encoding
    // query parameters on every saved request in the database.
    #[serde(default)]
    pub http_version: StoredHttpVersion,
    #[serde(default)]
    pub keep_method_on_redirect: bool,
    #[serde(default)]
    pub keep_auth_on_redirect: bool,
    #[serde(default = "enabled")]
    pub encode_url: bool,
    #[serde(default)]
    pub allow_http_09: bool,
    #[serde(default)]
    pub tls_minimum: StoredTlsMinimum,
}

fn enabled() -> bool {
    true
}

/// Mirrors HttpVersionPreference. Defaulted because every settings blob
/// written before today has no such key.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum StoredHttpVersion {
    #[default]
    Auto,
    Http11,
    Http2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum StoredTlsMinimum {
    #[default]
    Auto,
    Tls12,
    Tls13,
}

pub fn encode_pairs(pairs: &[KeyValue]) -> Result<String, AppError> {
    let stored: Vec<StoredKeyValue> = pairs
        .iter()
        .map(|pair| StoredKeyValue {
            name: pair.name.clone(),
            value: pair.value.clone(),
        })
        .collect();
    to_json(&stored)
}

pub fn decode_pairs(raw: &str) -> Result<Vec<KeyValue>, AppError> {
    let stored: Vec<StoredKeyValue> = from_json(raw)?;
    Ok(stored
        .into_iter()
        .map(|pair| KeyValue::new(pair.name, pair.value))
        .collect())
}

pub fn encode_body(body: &RequestBody) -> Result<String, AppError> {
    let stored = match body {
        RequestBody::None => StoredBody::None,
        RequestBody::Raw { content_type, text } => StoredBody::Raw {
            content_type: content_type.clone(),
            text: text.clone(),
        },
        RequestBody::FormUrlEncoded(fields) => StoredBody::FormUrlEncoded {
            fields: to_stored(fields),
        },
        RequestBody::Multipart(parts) => StoredBody::Multipart {
            parts: parts
                .iter()
                .map(|part| {
                    StoredMultipartPart::Tagged(match part {
                        MultipartPart::Text { name, value } => TaggedMultipartPart::Text {
                            name: name.clone(),
                            value: value.clone(),
                        },
                        MultipartPart::File {
                            name,
                            path,
                            content_type,
                        } => TaggedMultipartPart::File {
                            name: name.clone(),
                            path: path.to_string_lossy().into_owned(),
                            content_type: content_type.clone(),
                        },
                    })
                })
                .collect(),
        },
    };
    to_json(&stored)
}

pub fn decode_body(raw: &str) -> Result<RequestBody, AppError> {
    Ok(match from_json::<StoredBody>(raw)? {
        StoredBody::None => RequestBody::None,
        StoredBody::Raw { content_type, text } => RequestBody::Raw { content_type, text },
        StoredBody::FormUrlEncoded { fields } => RequestBody::FormUrlEncoded(from_stored(fields)),
        StoredBody::Multipart { parts } => RequestBody::Multipart(
            parts
                .into_iter()
                .map(|part| match part {
                    StoredMultipartPart::Tagged(TaggedMultipartPart::Text { name, value }) => {
                        MultipartPart::Text { name, value }
                    }
                    StoredMultipartPart::Tagged(TaggedMultipartPart::File {
                        name,
                        path,
                        content_type,
                    }) => MultipartPart::File {
                        name,
                        path: PathBuf::from(path),
                        content_type,
                    },
                    // Written before file parts existed: every part was text.
                    StoredMultipartPart::Legacy(pair) => MultipartPart::Text {
                        name: pair.name,
                        value: pair.value,
                    },
                })
                .collect(),
        ),
    })
}

pub fn encode_auth(auth: &Auth, mode: SecretWrite<'_>) -> Result<String, AppError> {
    let auth = match mode {
        SecretWrite::Strip => auth_without_literal_secret(auth),
        SecretWrite::Seal { .. } => auth.clone(),
    };
    let secret = match (auth_secret(&auth), &mode) {
        (None, _) => StoredSecret::Plain(String::new()),
        (Some((_, value)), SecretWrite::Strip) => StoredSecret::Plain(value.to_string()),
        (
            Some((field, value)),
            SecretWrite::Seal {
                cipher,
                table,
                row_id,
            },
        ) => {
            if value.is_empty() {
                // Writing a blank over a sealed value is only safe when the
                // key is usable: otherwise a secret that merely could not be
                // read this session would be erased.
                cipher.ensure_available()?;
                StoredSecret::Plain(String::new())
            } else {
                let sealed = cipher.seal(&secret_scope(table, row_id, field), value)?;
                StoredSecret::Sealed {
                    sealed: BASE64.encode(sealed),
                }
            }
        }
    };
    to_json(&stored_auth(&auth, secret))
}

fn stored_auth(auth: &Auth, secret: StoredSecret) -> StoredAuth {
    match auth {
        Auth::None => StoredAuth::None,
        Auth::Basic { username, .. } => StoredAuth::Basic {
            username: username.clone(),
            password: secret,
        },
        Auth::Bearer { .. } => StoredAuth::Bearer { token: secret },
        Auth::ApiKey { key, location, .. } => StoredAuth::ApiKey {
            key: key.clone(),
            value: secret,
            location: match location {
                ApiKeyLocation::Header => StoredApiKeyLocation::Header,
                ApiKeyLocation::Query => StoredApiKeyLocation::Query,
            },
        },
        Auth::Custom { header_name, .. } => StoredAuth::Custom {
            header_name: header_name.clone(),
            header_value: secret,
        },
    }
}

/// The auth with its secret opened, and whether that worked. A secret that
/// could not be opened comes back empty, so the row still loads.
pub fn decode_auth(raw: &str, mode: SecretRead<'_>) -> Result<(Auth, SecretState), AppError> {
    let stored = from_json::<StoredAuth>(raw)?;
    let (secret, state) = match stored.secret() {
        None => (String::new(), SecretState::Ok),
        Some(StoredSecret::Plain(text)) => (text.clone(), SecretState::Ok),
        Some(StoredSecret::Sealed { sealed }) => open_sealed(sealed, &stored, &mode),
    };
    let shape = match stored {
        StoredAuth::None => Auth::None,
        StoredAuth::Basic { username, .. } => Auth::Basic {
            username,
            password: String::new(),
        },
        StoredAuth::Bearer { .. } => Auth::Bearer {
            token: String::new(),
        },
        StoredAuth::ApiKey { key, location, .. } => Auth::ApiKey {
            key,
            value: String::new(),
            location: match location {
                StoredApiKeyLocation::Header => ApiKeyLocation::Header,
                StoredApiKeyLocation::Query => ApiKeyLocation::Query,
            },
        },
        StoredAuth::Custom { header_name, .. } => Auth::Custom {
            header_name,
            header_value: String::new(),
        },
    };
    Ok((with_auth_secret(&shape, secret), state))
}

fn open_sealed(sealed: &str, stored: &StoredAuth, mode: &SecretRead<'_>) -> (String, SecretState) {
    let SecretRead::Open {
        cipher,
        table,
        row_id,
    } = mode
    else {
        return (String::new(), SecretState::NeedsReentry);
    };
    // The field name comes from the variant, exactly as encode_auth chose it.
    let field = match stored {
        StoredAuth::None => return (String::new(), SecretState::Ok),
        StoredAuth::Basic { .. } => "auth.password",
        StoredAuth::Bearer { .. } => "auth.token",
        StoredAuth::ApiKey { .. } => "auth.value",
        StoredAuth::Custom { .. } => "auth.header_value",
    };
    let Ok(bytes) = BASE64.decode(sealed) else {
        return (String::new(), SecretState::NeedsReentry);
    };
    match cipher.open(&secret_scope(table, row_id, field), &bytes) {
        OpenedSecret::Plain(text) => (text, SecretState::Ok),
        OpenedSecret::Lost(state) => (String::new(), state),
    }
}

/// What the startup upgrade needs to know about a stored auth, without
/// opening anything.
#[derive(Debug, PartialEq, Eq)]
pub enum StoredAuthSecret {
    /// No secret field, an empty one, or one already sealed.
    Nothing,
    /// A non-empty value in plain text: a saved request written before
    /// Phase 9 needs it sealed.
    Plain,
    /// Plain text that history or an example must not keep.
    LiteralSecret,
}

pub fn inspect_auth(raw: &str) -> Result<StoredAuthSecret, AppError> {
    let stored = from_json::<StoredAuth>(raw)?;
    let Some(text) = stored.secret().and_then(StoredSecret::plain_text) else {
        return Ok(StoredAuthSecret::Nothing);
    };
    if text.is_empty() {
        return Ok(StoredAuthSecret::Nothing);
    }
    let (auth, _) = decode_auth(raw, SecretRead::PlainOnly)?;
    Ok(if has_literal_auth_secret(&auth) {
        StoredAuthSecret::LiteralSecret
    } else {
        StoredAuthSecret::Plain
    })
}

/// Base64 for a sealed environment variable, which lives in its own BLOB
/// column rather than inside JSON.
pub fn seal_value(
    cipher: &dyn SecretCipher,
    scope: &str,
    value: &str,
) -> Result<Option<Vec<u8>>, AppError> {
    if value.is_empty() {
        cipher.ensure_available()?;
        return Ok(None);
    }
    cipher.seal(scope, value).map(Some)
}

pub fn open_value(cipher: &dyn SecretCipher, scope: &str, sealed: &[u8]) -> (String, SecretState) {
    match cipher.open(scope, sealed) {
        OpenedSecret::Plain(text) => (text, SecretState::Ok),
        OpenedSecret::Lost(state) => (String::new(), state),
    }
}

pub fn encode_settings(settings: &RequestSettings) -> Result<String, AppError> {
    to_json(&StoredSettings {
        follow_redirects: settings.follow_redirects,
        max_redirects: settings.max_redirects,
        timeout_ms: u64::try_from(settings.timeout.as_millis()).unwrap_or(u64::MAX),
        verify_tls: settings.verify_tls,
        proxy: settings.proxy.clone(),
        send_cookies: settings.send_cookies,
        http_version: match settings.http_version {
            HttpVersionPreference::Auto => StoredHttpVersion::Auto,
            HttpVersionPreference::Http11 => StoredHttpVersion::Http11,
            HttpVersionPreference::Http2 => StoredHttpVersion::Http2,
        },
        keep_method_on_redirect: settings.keep_method_on_redirect,
        keep_auth_on_redirect: settings.keep_auth_on_redirect,
        encode_url: settings.encode_url,
        allow_http_09: settings.allow_http_09,
        tls_minimum: match settings.tls_minimum {
            TlsMinimum::Auto => StoredTlsMinimum::Auto,
            TlsMinimum::Tls12 => StoredTlsMinimum::Tls12,
            TlsMinimum::Tls13 => StoredTlsMinimum::Tls13,
        },
    })
}

pub fn decode_settings(raw: &str) -> Result<RequestSettings, AppError> {
    let stored: StoredSettings = from_json(raw)?;
    Ok(RequestSettings {
        follow_redirects: stored.follow_redirects,
        max_redirects: stored.max_redirects,
        timeout: std::time::Duration::from_millis(stored.timeout_ms),
        verify_tls: stored.verify_tls,
        proxy: stored.proxy,
        send_cookies: stored.send_cookies,
        http_version: match stored.http_version {
            StoredHttpVersion::Auto => HttpVersionPreference::Auto,
            StoredHttpVersion::Http11 => HttpVersionPreference::Http11,
            StoredHttpVersion::Http2 => HttpVersionPreference::Http2,
        },
        keep_method_on_redirect: stored.keep_method_on_redirect,
        keep_auth_on_redirect: stored.keep_auth_on_redirect,
        encode_url: stored.encode_url,
        allow_http_09: stored.allow_http_09,
        tls_minimum: match stored.tls_minimum {
            StoredTlsMinimum::Auto => TlsMinimum::Auto,
            StoredTlsMinimum::Tls12 => TlsMinimum::Tls12,
            StoredTlsMinimum::Tls13 => TlsMinimum::Tls13,
        },
    })
}

/// What only a WebSocket request has, in `requests.ws_json` (migration 0010).
/// The URL and the handshake headers are in the columns HTTP already uses.
///
/// Every field is defaulted, so a blob written before a field existed still
/// loads: the column is JSON, so the defaults are the migration. The same
/// rule as `StoredSettings`, and the same trap — a bare `#[serde(default)]` on
/// a bool is false, which for `verify_tls` would silently turn certificate
/// checking off. Each bool whose default is true says so explicitly.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoredWebSocket {
    #[serde(default)]
    pub draft_format: StoredWsMessageFormat,
    /// Absent before binary messages had a Base64 option. A draft stored as
    /// `Hex` then reads as binary typed in hex, whatever this says.
    #[serde(default)]
    pub draft_binary_encoding: StoredWsBinaryEncoding,
    #[serde(default)]
    pub draft_text: String,
    #[serde(default)]
    pub settings: StoredWebSocketSettings,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum StoredWsMessageFormat {
    #[default]
    Text,
    Json,
    /// Written before XML, HTML and Base64 existed: binary typed in hex.
    /// Still read, never written.
    Hex,
    Xml,
    Html,
    Binary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum StoredWsBinaryEncoding {
    #[default]
    Base64,
    Hex,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredWebSocketSettings {
    #[serde(default = "enabled")]
    pub verify_tls: bool,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default = "enabled")]
    pub send_cookies: bool,
    #[serde(default = "default_ws_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_ws_max_message_bytes")]
    pub max_message_bytes: u64,
    #[serde(default)]
    pub auto_reconnect: bool,
}

/// A whole missing `settings` object gets the same values a new request
/// does, from the one place those are decided (`WebSocketSettings::default`).
impl Default for StoredWebSocketSettings {
    fn default() -> Self {
        stored_ws_settings(&WebSocketSettings::default())
    }
}

fn default_ws_connect_timeout_ms() -> u64 {
    millis_u64(DEFAULT_WS_CONNECT_TIMEOUT)
}

fn default_ws_max_message_bytes() -> u64 {
    u64::try_from(DEFAULT_WS_MAX_MESSAGE_BYTES).unwrap_or(u64::MAX)
}

fn millis_u64(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn stored_ws_settings(settings: &WebSocketSettings) -> StoredWebSocketSettings {
    StoredWebSocketSettings {
        verify_tls: settings.verify_tls,
        proxy: settings.proxy.clone(),
        send_cookies: settings.send_cookies,
        connect_timeout_ms: millis_u64(settings.connect_timeout),
        max_message_bytes: u64::try_from(settings.max_message_bytes).unwrap_or(u64::MAX),
        auto_reconnect: settings.auto_reconnect,
    }
}

pub fn encode_web_socket(
    settings: &WebSocketSettings,
    draft: &WsDraft,
) -> Result<String, AppError> {
    to_json(&StoredWebSocket {
        draft_format: match draft.format {
            WsMessageFormat::Text => StoredWsMessageFormat::Text,
            WsMessageFormat::Json => StoredWsMessageFormat::Json,
            WsMessageFormat::Xml => StoredWsMessageFormat::Xml,
            WsMessageFormat::Html => StoredWsMessageFormat::Html,
            WsMessageFormat::Binary => StoredWsMessageFormat::Binary,
        },
        draft_binary_encoding: match draft.binary_encoding {
            WsBinaryEncoding::Base64 => StoredWsBinaryEncoding::Base64,
            WsBinaryEncoding::Hex => StoredWsBinaryEncoding::Hex,
        },
        draft_text: draft.text.clone(),
        settings: stored_ws_settings(settings),
    })
}

/// An empty column is what an HTTP row holds; read as all defaults rather
/// than refused, so a row whose kind and blob disagree still opens.
pub fn decode_web_socket(raw: &str) -> Result<(WebSocketSettings, WsDraft), AppError> {
    let stored: StoredWebSocket = if raw.trim().is_empty() {
        from_json("{}")?
    } else {
        from_json(raw)?
    };
    let settings = stored.settings;
    let stored_encoding = match stored.draft_binary_encoding {
        StoredWsBinaryEncoding::Base64 => WsBinaryEncoding::Base64,
        StoredWsBinaryEncoding::Hex => WsBinaryEncoding::Hex,
    };
    let (format, binary_encoding) = match stored.draft_format {
        StoredWsMessageFormat::Text => (WsMessageFormat::Text, stored_encoding),
        StoredWsMessageFormat::Json => (WsMessageFormat::Json, stored_encoding),
        StoredWsMessageFormat::Xml => (WsMessageFormat::Xml, stored_encoding),
        StoredWsMessageFormat::Html => (WsMessageFormat::Html, stored_encoding),
        StoredWsMessageFormat::Binary => (WsMessageFormat::Binary, stored_encoding),
        StoredWsMessageFormat::Hex => (WsMessageFormat::Binary, WsBinaryEncoding::Hex),
    };
    Ok((
        WebSocketSettings {
            verify_tls: settings.verify_tls,
            proxy: settings.proxy,
            send_cookies: settings.send_cookies,
            connect_timeout: std::time::Duration::from_millis(settings.connect_timeout_ms),
            max_message_bytes: usize::try_from(settings.max_message_bytes).unwrap_or(usize::MAX),
            auto_reconnect: settings.auto_reconnect,
        },
        WsDraft {
            format,
            binary_encoding,
            text: stored.draft_text,
        },
    ))
}

fn to_stored(pairs: &[KeyValue]) -> Vec<StoredKeyValue> {
    pairs
        .iter()
        .map(|pair| StoredKeyValue {
            name: pair.name.clone(),
            value: pair.value.clone(),
        })
        .collect()
}

fn from_stored(pairs: Vec<StoredKeyValue>) -> Vec<KeyValue> {
    pairs
        .into_iter()
        .map(|pair| KeyValue::new(pair.name, pair.value))
        .collect()
}

fn to_json<T: Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|error| AppError::Storage(error.to_string()))
}

/// A row that will not parse means the file was edited by hand or written by
/// a newer version; surfacing it beats silently loading a half-empty request.
fn from_json<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, AppError> {
    serde_json::from_str(raw)
        .map_err(|error| AppError::Storage(format!("stored request could not be read: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exactly what `settings_json` looked like before 2026-09-14. Every
    /// saved request, history entry and example in an existing database has
    /// this shape, and none of them can be migrated — the column is JSON, so
    /// the defaults are the migration.
    const LEGACY_SETTINGS: &str = r#"{
        "follow_redirects": true,
        "max_redirects": 10,
        "timeout_ms": 30000,
        "verify_tls": true,
        "proxy": null,
        "send_cookies": true
    }"#;

    /// The one that would actually do damage. `#[serde(default)]` on a bool
    /// yields false, which would silently stop percent-encoding query
    /// parameters on every request already in the database — a wrong request
    /// sent with no error anywhere.
    #[test]
    fn a_settings_blob_written_before_these_keys_existed_still_encodes_its_urls() {
        let settings = decode_settings(LEGACY_SETTINGS).expect("legacy settings should decode");

        assert!(settings.encode_url);
    }

    #[test]
    fn the_rest_of_the_new_keys_fall_back_to_the_same_defaults_a_new_request_gets() {
        let settings = decode_settings(LEGACY_SETTINGS).expect("legacy settings should decode");
        let fresh = RequestSettings::default();

        assert_eq!(settings.http_version, fresh.http_version);
        assert_eq!(
            settings.keep_method_on_redirect,
            fresh.keep_method_on_redirect
        );
        assert_eq!(settings.keep_auth_on_redirect, fresh.keep_auth_on_redirect);
        assert_eq!(settings.allow_http_09, fresh.allow_http_09);
        assert_eq!(settings.tls_minimum, fresh.tls_minimum);
        // Still guarded, and still the reason the default is spelled out.
        assert!(settings.send_cookies);
    }

    #[test]
    fn a_settings_blob_round_trips_with_every_field_set_away_from_its_default() {
        let original = RequestSettings {
            follow_redirects: false,
            max_redirects: 3,
            timeout: std::time::Duration::from_millis(1500),
            verify_tls: false,
            proxy: Some("http://127.0.0.1:8080".into()),
            send_cookies: false,
            http_version: HttpVersionPreference::Http2,
            keep_method_on_redirect: true,
            keep_auth_on_redirect: true,
            encode_url: false,
            allow_http_09: true,
            tls_minimum: TlsMinimum::Tls12,
        };

        let encoded = encode_settings(&original).expect("should encode");
        let decoded = decode_settings(&encoded).expect("should decode");

        assert_eq!(decoded, original);
    }

    /// A WebSocket blob with every field away from its default survives the
    /// trip, draft text in a script other than Latin included.
    #[test]
    fn a_web_socket_blob_round_trips_with_every_field_set() {
        let settings = WebSocketSettings {
            verify_tls: false,
            proxy: Some("http://127.0.0.1:8080".into()),
            send_cookies: false,
            connect_timeout: std::time::Duration::from_millis(2500),
            max_message_bytes: 4096,
            auto_reconnect: true,
        };
        let draft = WsDraft {
            format: WsMessageFormat::Binary,
            binary_encoding: WsBinaryEncoding::Hex,
            text: "de ad be ef — Ђорђе".into(),
        };

        let encoded = encode_web_socket(&settings, &draft).expect("should encode");
        let (decoded_settings, decoded_draft) = decode_web_socket(&encoded).expect("should decode");

        assert_eq!(decoded_settings, settings);
        assert_eq!(decoded_draft, draft);
    }

    /// The trap StoredSettings already fell into once: a bool that is
    /// missing from an older blob must come back as its real default. For
    /// `verify_tls` the wrong answer is certificate checking switched off.
    #[test]
    fn a_web_socket_blob_missing_its_settings_keys_gets_the_defaults_a_new_request_gets() {
        let (settings, draft) =
            decode_web_socket(r#"{"draft_text":"hi","settings":{}}"#).expect("should decode");

        assert_eq!(settings, WebSocketSettings::default());
        assert!(settings.verify_tls);
        assert!(settings.send_cookies);
        assert_eq!(draft.format, WsMessageFormat::Text);
        assert_eq!(draft.binary_encoding, WsBinaryEncoding::Base64);
        assert_eq!(draft.text, "hi");
    }

    /// A draft saved when the only binary option was "Hex" opens as binary
    /// typed in hex, not as something else or not at all.
    #[test]
    fn a_draft_saved_as_hex_opens_as_binary_in_hex() {
        let (_, draft) = decode_web_socket(r#"{"draft_format":"Hex","draft_text":"de ad"}"#)
            .expect("a hex draft should decode");

        assert_eq!(draft.format, WsMessageFormat::Binary);
        assert_eq!(draft.binary_encoding, WsBinaryEncoding::Hex);
        assert_eq!(draft.text, "de ad");
    }

    #[test]
    fn every_format_and_encoding_round_trips() {
        let formats = [
            WsMessageFormat::Text,
            WsMessageFormat::Json,
            WsMessageFormat::Xml,
            WsMessageFormat::Html,
            WsMessageFormat::Binary,
        ];
        for format in formats {
            for binary_encoding in [WsBinaryEncoding::Base64, WsBinaryEncoding::Hex] {
                let draft = WsDraft {
                    format,
                    binary_encoding,
                    text: "x".into(),
                };
                let encoded = encode_web_socket(&WebSocketSettings::default(), &draft)
                    .expect("should encode");

                // The legacy spelling is read, never written.
                assert!(!encoded.contains(r#""draft_format":"Hex""#));
                assert_eq!(decode_web_socket(&encoded).expect("should decode").1, draft);
            }
        }
    }

    #[test]
    fn a_web_socket_blob_with_no_settings_object_at_all_still_loads() {
        let (settings, draft) =
            decode_web_socket(r#"{"draft_format":"Json"}"#).expect("an older blob should decode");

        assert_eq!(settings, WebSocketSettings::default());
        assert_eq!(draft.format, WsMessageFormat::Json);
        assert_eq!(draft.text, "");
    }

    #[test]
    fn an_empty_web_socket_column_reads_as_all_defaults() {
        let (settings, draft) = decode_web_socket("").expect("empty should decode");

        assert_eq!(settings, WebSocketSettings::default());
        assert_eq!(draft, WsDraft::default());
    }

    use crate::secrets::data_key::DataKey;
    use crate::secrets::envelope::{EnvelopeCipher, UnavailableCipher};

    fn cipher() -> EnvelopeCipher {
        EnvelopeCipher::new(&DataKey::generate().expect("should generate")).expect("should build")
    }

    fn seal_for<'a>(cipher: &'a dyn SecretCipher, row_id: &'a str) -> SecretWrite<'a> {
        SecretWrite::Seal {
            cipher,
            table: "requests",
            row_id,
        }
    }

    fn open_for<'a>(cipher: &'a dyn SecretCipher, row_id: &'a str) -> SecretRead<'a> {
        SecretRead::Open {
            cipher,
            table: "requests",
            row_id,
        }
    }

    #[test]
    fn a_sealed_auth_never_holds_its_secret_in_plain_text() {
        let cipher = cipher();
        let auth = Auth::Bearer {
            token: "live-token-123".into(),
        };

        let raw = encode_auth(&auth, seal_for(&cipher, "req_1")).expect("should encode");

        assert!(!raw.contains("live-token-123"));
        assert!(raw.contains("\"sealed\""));
        assert_eq!(
            decode_auth(&raw, open_for(&cipher, "req_1")).expect("should decode"),
            (auth, SecretState::Ok)
        );
    }

    #[test]
    fn a_sealed_auth_moved_to_another_row_does_not_open() {
        let cipher = cipher();
        let auth = Auth::Basic {
            username: "ann".into(),
            password: "pw".into(),
        };
        let raw = encode_auth(&auth, seal_for(&cipher, "req_1")).expect("should encode");

        let (loaded, state) = decode_auth(&raw, open_for(&cipher, "req_2")).expect("should decode");

        assert_eq!(state, SecretState::NeedsReentry);
        assert_eq!(
            loaded,
            Auth::Basic {
                username: "ann".into(),
                password: String::new(),
            }
        );
    }

    #[test]
    fn a_pre_phase_9_plain_secret_still_loads() {
        let raw = r#"{"kind":"Bearer","token":"old-plain"}"#;

        let (auth, state) = decode_auth(raw, SecretRead::PlainOnly).expect("should decode");

        assert_eq!(
            auth,
            Auth::Bearer {
                token: "old-plain".into()
            }
        );
        assert_eq!(state, SecretState::Ok);
        assert_eq!(
            inspect_auth(raw).expect("inspect"),
            StoredAuthSecret::LiteralSecret
        );
    }

    #[test]
    fn inspection_tells_the_upgrade_what_to_do() {
        let cipher = cipher();
        let sealed = encode_auth(
            &Auth::Bearer { token: "t".into() },
            seal_for(&cipher, "req_1"),
        )
        .expect("should encode");

        assert_eq!(
            inspect_auth(&sealed).expect("inspect"),
            StoredAuthSecret::Nothing
        );
        assert_eq!(
            inspect_auth(r#"{"kind":"None"}"#).expect("inspect"),
            StoredAuthSecret::Nothing
        );
        assert_eq!(
            inspect_auth(r#"{"kind":"Bearer","token":""}"#).expect("inspect"),
            StoredAuthSecret::Nothing
        );
        assert_eq!(
            inspect_auth(r#"{"kind":"Bearer","token":"{{token}}"}"#).expect("inspect"),
            StoredAuthSecret::Plain
        );
    }

    #[test]
    fn stripping_blanks_a_literal_and_keeps_a_placeholder() {
        let literal = encode_auth(
            &Auth::Bearer {
                token: "live".into(),
            },
            SecretWrite::Strip,
        )
        .expect("should encode");
        let placeholder = encode_auth(
            &Auth::Bearer {
                token: "{{token}}".into(),
            },
            SecretWrite::Strip,
        )
        .expect("should encode");

        assert!(!literal.contains("live"));
        assert!(placeholder.contains("{{token}}"));
    }

    #[test]
    fn an_unavailable_store_refuses_even_a_blank_secret() {
        let cipher = UnavailableCipher::new("locked".into());

        let result = encode_auth(
            &Auth::Bearer {
                token: String::new(),
            },
            seal_for(&cipher, "req_1"),
        );

        assert!(matches!(result, Err(AppError::SecretStore(_))));
    }

    #[test]
    fn an_auth_without_a_secret_needs_no_key() {
        let cipher = UnavailableCipher::new("locked".into());

        let raw = encode_auth(&Auth::None, seal_for(&cipher, "req_1")).expect("should encode");

        assert_eq!(raw, r#"{"kind":"None"}"#);
    }

    #[test]
    fn an_unreachable_store_reports_the_secret_unavailable_not_lost() {
        let key = cipher();
        let raw = encode_auth(&Auth::Bearer { token: "t".into() }, seal_for(&key, "req_1"))
            .expect("should encode");
        let locked = UnavailableCipher::new("locked".into());

        let (_, state) = decode_auth(&raw, open_for(&locked, "req_1")).expect("should decode");

        assert_eq!(state, SecretState::Unavailable);
    }
}

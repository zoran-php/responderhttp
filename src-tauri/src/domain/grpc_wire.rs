// http_client/src-tauri/src/domain/grpc_wire.rs
//
// The gRPC-over-HTTP/2 rules that do not depend on a transport (PLAN-GRPC.md
// 16c): message framing, how a call's status is read, which metadata may be
// sent, and the request headers every call carries. Pure: no libcurl, no
// socket, no clock, the same shape as ws_frames.rs and sse.rs.
//
// The reference is the gRPC project's "gRPC over HTTP2" protocol document.
// Where it leaves a choice open, the choice is written next to the code.
use std::time::Duration;

use crate::domain::models::{ApiKeyLocation, Auth, KeyValue};

/// One flag byte, then the message length as a big-endian u32.
pub const PREFIX_LEN: usize = 5;

/// gRPC's own default for a received message, and this app's (PLAN-GRPC.md
/// assumption 4). Settings can raise it up to `MAX_RECEIVE_BYTES_CAP`.
pub const DEFAULT_MAX_RECEIVE_BYTES: usize = 4 * 1024 * 1024;

/// The highest the setting goes. There is no "unlimited": a declared length
/// is allocated before the message arrives, so a hostile or broken server
/// could otherwise ask for 4 GiB with five bytes.
pub const MAX_RECEIVE_BYTES_CAP: usize = 256 * 1024 * 1024;

const FLAG_UNCOMPRESSED: u8 = 0;
const FLAG_COMPRESSED: u8 = 1;

pub const CONTENT_TYPE: &str = "application/grpc";
const USER_AGENT: &str = concat!("ResponderHTTP/", env!("CARGO_PKG_VERSION"));

/// A `grpc-timeout` value has at most eight digits.
const MAX_TIMEOUT_DIGITS_VALUE: u128 = 99_999_999;

/// Headers a user may not set as metadata: HTTP/2 forbids the connection-
/// specific ones, and the rest belong to the protocol. `grpc-*` is refused
/// as a whole, by prefix.
const RESERVED_METADATA: [&str; 8] = [
    "content-type",
    "te",
    "host",
    "connection",
    "keep-alive",
    "proxy-connection",
    "transfer-encoding",
    "upgrade",
];
const RESERVED_PREFIX: &str = "grpc-";
const BINARY_SUFFIX: &str = "-bin";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error(
        "the server announced a {declared}-byte message, over the {max}-byte limit in Settings"
    )]
    MessageTooLarge { declared: usize, max: usize },
    #[error("the server sent a compressed message, which this client did not ask for")]
    Compressed,
    #[error("the server sent a message flag of {0}, which gRPC does not define")]
    UnknownFlag(u8),
    #[error("the response ended in the middle of a message ({0} bytes of it arrived)")]
    Truncated(usize),
    #[error("a message of {0} bytes is too large to send in one gRPC frame")]
    TooLargeToSend(usize),
    #[error("{0}")]
    InvalidMetadata(String),
    #[error("{0}")]
    UnsupportedAuth(String),
}

impl WireError {
    /// The status a receive-side failure ends the call with. The client ends
    /// it, not the server, which the UI shows (`by: "client"`). Send-side and
    /// input errors have none: they stop the call before it starts.
    pub fn client_status(&self) -> Option<GrpcStatus> {
        let code = match self {
            Self::MessageTooLarge { .. } => Code::ResourceExhausted,
            Self::Compressed | Self::UnknownFlag(_) | Self::Truncated(_) => Code::Internal,
            Self::TooLargeToSend(_) | Self::InvalidMetadata(_) | Self::UnsupportedAuth(_) => {
                return None
            }
        };
        Some(GrpcStatus {
            code,
            message: self.to_string(),
        })
    }
}

// --- Framing -----------------------------------------------------------------

/// Frames one message for sending, uncompressed.
pub fn frame(message: &[u8]) -> Result<Vec<u8>, WireError> {
    let length =
        u32::try_from(message.len()).map_err(|_| WireError::TooLargeToSend(message.len()))?;
    let mut out = Vec::with_capacity(PREFIX_LEN + message.len());
    out.push(FLAG_UNCOMPRESSED);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(message);
    Ok(out)
}

/// Turns response body chunks, split wherever the transport split them,
/// into whole messages.
///
/// The declared length is checked against the limit **before** anything is
/// buffered for it, so an oversized message costs five bytes, not its size.
#[derive(Debug)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    max_message_bytes: usize,
}

impl FrameDecoder {
    pub fn new(max_message_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_message_bytes: max_message_bytes.min(MAX_RECEIVE_BYTES_CAP),
        }
    }

    /// Every message completed by `chunk`. After an error the decoder is
    /// spent: the call ends with the error's status.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, WireError> {
        self.buffer.extend_from_slice(chunk);
        let mut messages = Vec::new();
        let mut start = 0;
        while self.buffer.len() - start >= PREFIX_LEN {
            let prefix = &self.buffer[start..start + PREFIX_LEN];
            match prefix[0] {
                FLAG_UNCOMPRESSED => {}
                FLAG_COMPRESSED => return Err(WireError::Compressed),
                other => return Err(WireError::UnknownFlag(other)),
            }
            let declared =
                u32::from_be_bytes([prefix[1], prefix[2], prefix[3], prefix[4]]) as usize;
            if declared > self.max_message_bytes {
                return Err(WireError::MessageTooLarge {
                    declared,
                    max: self.max_message_bytes,
                });
            }
            let end = start + PREFIX_LEN + declared;
            if self.buffer.len() < end {
                break;
            }
            messages.push(self.buffer[start + PREFIX_LEN..end].to_vec());
            start = end;
        }
        self.buffer.drain(..start);
        Ok(messages)
    }

    /// Called when the response body ends. Leftover bytes are a message the
    /// server started and never finished.
    pub fn finish(&self) -> Result<(), WireError> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(WireError::Truncated(self.buffer.len()))
        }
    }
}

// --- Status ------------------------------------------------------------------

/// The seventeen gRPC status codes. This is the only table of them in the
/// app: the frontend gets the name with the number (CLAUDE.md section 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Ok,
    Cancelled,
    Unknown,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
}

const CODES: [(Code, u32, &str); 17] = [
    (Code::Ok, 0, "OK"),
    (Code::Cancelled, 1, "CANCELLED"),
    (Code::Unknown, 2, "UNKNOWN"),
    (Code::InvalidArgument, 3, "INVALID_ARGUMENT"),
    (Code::DeadlineExceeded, 4, "DEADLINE_EXCEEDED"),
    (Code::NotFound, 5, "NOT_FOUND"),
    (Code::AlreadyExists, 6, "ALREADY_EXISTS"),
    (Code::PermissionDenied, 7, "PERMISSION_DENIED"),
    (Code::ResourceExhausted, 8, "RESOURCE_EXHAUSTED"),
    (Code::FailedPrecondition, 9, "FAILED_PRECONDITION"),
    (Code::Aborted, 10, "ABORTED"),
    (Code::OutOfRange, 11, "OUT_OF_RANGE"),
    (Code::Unimplemented, 12, "UNIMPLEMENTED"),
    (Code::Internal, 13, "INTERNAL"),
    (Code::Unavailable, 14, "UNAVAILABLE"),
    (Code::DataLoss, 15, "DATA_LOSS"),
    (Code::Unauthenticated, 16, "UNAUTHENTICATED"),
];

impl Code {
    /// A number outside the table is UNKNOWN, as the protocol says.
    pub fn from_number(number: u32) -> Self {
        CODES
            .iter()
            .find(|(_, n, _)| *n == number)
            .map_or(Self::Unknown, |(code, _, _)| *code)
    }

    pub fn number(self) -> u32 {
        CODES
            .iter()
            .find(|(code, _, _)| *code == self)
            .map_or(2, |(_, n, _)| *n)
    }

    pub fn name(self) -> &'static str {
        CODES
            .iter()
            .find(|(code, _, _)| *code == self)
            .map_or("UNKNOWN", |(_, _, name)| name)
    }
}

/// How a call ended, as the user sees it. A non-OK status is a result, not
/// an application error (PLAN-GRPC.md section 3, point 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcStatus {
    pub code: Code,
    pub message: String,
}

impl GrpcStatus {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn find<'a>(block: &'a [KeyValue], name: &str) -> Option<&'a str> {
    block
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(name))
        .map(|entry| entry.value.as_str())
}

/// Reads a finished call's status.
///
/// - `grpc-status` normally comes in the trailers. A **trailers-only**
///   response (an error with no body) carries it in the headers instead, so
///   the headers are the fallback (PLAN-GRPC.md 16a, gate G03).
/// - Without a `grpc-status`, a non-200 HTTP status is mapped as the
///   protocol's table says. A 200 without one is UNKNOWN.
/// - A 200 that is not `application/grpc` at all is UNKNOWN too, named as
///   such: a proxy's HTML error page should not read as a gRPC failure.
pub fn status_of(http_status: u32, headers: &[KeyValue], trailers: &[KeyValue]) -> GrpcStatus {
    for block in [trailers, headers] {
        if let Some(raw) = find(block, "grpc-status") {
            let code = raw
                .trim()
                .parse::<u32>()
                .map_or(Code::Unknown, Code::from_number);
            let message = find(block, "grpc-message")
                .map(percent_decode)
                .unwrap_or_default();
            return GrpcStatus { code, message };
        }
    }
    if http_status != 200 {
        return GrpcStatus::new(
            code_for_http_status(http_status),
            format!("the server answered HTTP {http_status} without a gRPC status"),
        );
    }
    match find(headers, "content-type") {
        Some(content_type) if !content_type.starts_with(CONTENT_TYPE) => GrpcStatus::new(
            Code::Unknown,
            format!("the server answered with {content_type}, not a gRPC response"),
        ),
        _ => GrpcStatus::new(
            Code::Unknown,
            "the server ended the call without a gRPC status",
        ),
    }
}

/// Whether the response body is gRPC messages at all, and so whether it may
/// go through a `FrameDecoder`. A proxy's error page, or a plain 404 from a
/// server that is not gRPC, is text: framing it reads its first byte as a
/// flag and fails with a nonsense error instead of the status the headers
/// already give (found by tests/grpc.rs, 16d). Such a body is skipped, and
/// `status_of` explains the call from the HTTP status.
pub fn is_grpc_response(http_status: u32, headers: &[KeyValue]) -> bool {
    http_status == 200
        && find(headers, "content-type").is_some_and(|value| value.starts_with(CONTENT_TYPE))
}

/// The protocol's mapping for an HTTP status that arrives without a gRPC one.
pub fn code_for_http_status(http_status: u32) -> Code {
    match http_status {
        400 => Code::Internal,
        401 => Code::Unauthenticated,
        403 => Code::PermissionDenied,
        404 => Code::Unimplemented,
        429 | 502 | 503 | 504 => Code::Unavailable,
        _ => Code::Unknown,
    }
}

/// `grpc-message` is percent-encoded UTF-8. A malformed escape is kept as
/// written rather than failing: the message is for reading, and half of it is
/// better than none.
pub fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push((high << 4) | low);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// --- Metadata ----------------------------------------------------------------

/// Checks and normalises the user's metadata.
///
/// - Rows with an empty name are an untouched table row and are dropped.
/// - Names are lowercased (HTTP/2 requires it) and may only use
///   `0-9 a-z _ . -`.
/// - Reserved names are refused, naming the key, so the user learns why
///   rather than finding it silently missing.
/// - A text value must be printable ASCII. A `-bin` value is sent as the
///   user typed it, which must be base64: that is what the protocol puts on
///   the wire, and what grpcurl expects too.
pub fn check_metadata(entries: &[KeyValue]) -> Result<Vec<KeyValue>, WireError> {
    let mut checked = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = entry.name.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if !name.bytes().all(|b| {
            b.is_ascii_digit() || b.is_ascii_lowercase() || matches!(b, b'_' | b'.' | b'-')
        }) {
            return Err(WireError::InvalidMetadata(format!(
                "metadata key {name:?} may only contain letters, digits, '_', '.' and '-'"
            )));
        }
        if name.starts_with(RESERVED_PREFIX) || RESERVED_METADATA.contains(&name.as_str()) {
            return Err(WireError::InvalidMetadata(format!(
                "metadata key {name:?} is reserved by gRPC or HTTP/2 and cannot be set"
            )));
        }
        let value = entry.value.trim();
        if name.ends_with(BINARY_SUFFIX) {
            if !is_base64(value) {
                return Err(WireError::InvalidMetadata(format!(
                    "{name} is binary metadata, so its value must be base64"
                )));
            }
        } else if !value.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
            return Err(WireError::InvalidMetadata(format!(
                "the value of {name} must be printable ASCII; use a key ending in -bin for anything else"
            )));
        }
        checked.push(KeyValue::new(name, value));
    }
    Ok(checked)
}

/// Standard alphabet; padding optional, as the protocol requires receivers
/// to accept both.
fn is_base64(value: &str) -> bool {
    let unpadded = value.trim_end_matches('=');
    value.len() - unpadded.len() <= 2
        && unpadded.len() % 4 != 1
        && unpadded
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

/// gRPC has no query string, so an API key can only travel as metadata.
/// Refused rather than quietly moved, so a saved HTTP-style setting does not
/// change meaning without the user seeing it.
pub fn check_auth(auth: &Auth) -> Result<(), WireError> {
    match auth {
        Auth::ApiKey {
            location: ApiKeyLocation::Query,
            ..
        } => Err(WireError::UnsupportedAuth(
            "gRPC has no query string: set the API key's location to Header".to_string(),
        )),
        _ => Ok(()),
    }
}

// --- Request headers ---------------------------------------------------------

/// The headers every call sends, then the (already checked) metadata. One
/// function, so no transport can forget `te: trailers`, without which some
/// servers refuse the call.
///
/// A `user-agent` in the metadata replaces the app's own.
pub fn request_headers(metadata: &[KeyValue], deadline: Option<Duration>) -> Vec<KeyValue> {
    let mut headers = vec![
        KeyValue::new("content-type", CONTENT_TYPE),
        KeyValue::new("te", "trailers"),
    ];
    if find(metadata, "user-agent").is_none() {
        headers.push(KeyValue::new("user-agent", USER_AGENT));
    }
    if let Some(deadline) = deadline {
        headers.push(KeyValue::new("grpc-timeout", encode_timeout(deadline)));
    }
    headers.extend(metadata.iter().cloned());
    headers
}

/// `grpc-timeout` is at most eight digits and a unit. The finest unit that
/// fits is used, rounding up, so the server is never told less time than the
/// user set.
pub fn encode_timeout(deadline: Duration) -> String {
    const UNITS: [(char, u128); 6] = [
        ('n', 1),
        ('u', 1_000),
        ('m', 1_000_000),
        ('S', 1_000_000_000),
        ('M', 60_000_000_000),
        ('H', 3_600_000_000_000),
    ];
    let nanos = deadline.as_nanos();
    for (unit, size) in UNITS {
        let value = nanos.div_ceil(size);
        if value <= MAX_TIMEOUT_DIGITS_VALUE {
            return format!("{value}{unit}");
        }
    }
    format!("{MAX_TIMEOUT_DIGITS_VALUE}H")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(name: &str, value: &str) -> KeyValue {
        KeyValue::new(name, value)
    }

    // --- framing

    #[test]
    fn a_framed_message_is_flag_length_then_bytes() {
        assert_eq!(frame(b"hi").expect("fits"), vec![0, 0, 0, 0, 2, b'h', b'i']);
        assert_eq!(frame(b"").expect("fits"), vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn messages_split_at_every_offset_come_out_whole() {
        let wire = [frame(b"hello"), frame(b""), frame(b"world!")]
            .into_iter()
            .map(|framed| framed.expect("fits"))
            .collect::<Vec<_>>()
            .concat();
        for split in 0..=wire.len() {
            let mut decoder = FrameDecoder::new(DEFAULT_MAX_RECEIVE_BYTES);
            let mut out = decoder.push(&wire[..split]).expect("valid");
            out.extend(decoder.push(&wire[split..]).expect("valid"));
            assert_eq!(
                out,
                vec![b"hello".to_vec(), Vec::new(), b"world!".to_vec()],
                "split {split}"
            );
            assert_eq!(decoder.finish(), Ok(()));
        }
    }

    #[test]
    fn an_oversized_message_is_refused_from_its_prefix_alone() {
        let mut decoder = FrameDecoder::new(10);

        let error = decoder.push(&[0, 0, 0, 0, 11]).expect_err("11 > 10");

        assert_eq!(
            error,
            WireError::MessageTooLarge {
                declared: 11,
                max: 10
            }
        );
        assert_eq!(
            error.client_status().map(|s| s.code),
            Some(Code::ResourceExhausted)
        );
    }

    #[test]
    fn the_limit_cannot_be_set_above_the_cap() {
        let mut decoder = FrameDecoder::new(usize::MAX);
        let declared = (MAX_RECEIVE_BYTES_CAP + 1) as u32;

        let error = decoder
            .push(&[&[0][..], declared.to_be_bytes().as_slice()].concat())
            .expect_err("over the cap");

        assert!(
            matches!(error, WireError::MessageTooLarge { max, .. } if max == MAX_RECEIVE_BYTES_CAP)
        );
    }

    #[test]
    fn compressed_and_undefined_flags_are_internal_errors() {
        assert_eq!(
            FrameDecoder::new(100).push(&[1, 0, 0, 0, 0]),
            Err(WireError::Compressed)
        );
        assert_eq!(
            FrameDecoder::new(100).push(&[7, 0, 0, 0, 0]),
            Err(WireError::UnknownFlag(7))
        );
        assert_eq!(
            WireError::Compressed.client_status().map(|s| s.code),
            Some(Code::Internal)
        );
    }

    #[test]
    fn a_body_that_stops_mid_message_is_truncated() {
        let mut decoder = FrameDecoder::new(100);
        decoder.push(&[0, 0, 0, 0, 4, b'a']).expect("valid so far");

        assert_eq!(decoder.finish(), Err(WireError::Truncated(6)));
    }

    // --- status

    #[test]
    fn every_code_round_trips_through_its_number_and_has_a_name() {
        for number in 0..17 {
            let code = Code::from_number(number);
            assert_eq!(code.number(), number);
            assert!(!code.name().is_empty());
        }
        assert_eq!(Code::from_number(99), Code::Unknown);
        assert_eq!(Code::Unauthenticated.name(), "UNAUTHENTICATED");
    }

    #[test]
    fn the_status_comes_from_the_trailers() {
        let status = status_of(
            200,
            &[kv("content-type", "application/grpc")],
            &[
                kv("grpc-status", "5"),
                kv("grpc-message", "no%20such%20item"),
            ],
        );

        assert_eq!(status, GrpcStatus::new(Code::NotFound, "no such item"));
    }

    #[test]
    fn a_trailers_only_response_carries_it_in_the_headers() {
        let status = status_of(
            200,
            &[
                kv("content-type", "application/grpc"),
                kv("Grpc-Status", "16"),
                kv("grpc-message", "token expired"),
            ],
            &[],
        );

        assert_eq!(
            status,
            GrpcStatus::new(Code::Unauthenticated, "token expired")
        );
    }

    #[test]
    fn trailers_win_over_headers() {
        let status = status_of(200, &[kv("grpc-status", "13")], &[kv("grpc-status", "0")]);

        assert_eq!(status.code, Code::Ok);
    }

    #[test]
    fn a_non_200_without_a_grpc_status_is_mapped() {
        for (http, code) in [
            (400, Code::Internal),
            (401, Code::Unauthenticated),
            (403, Code::PermissionDenied),
            (404, Code::Unimplemented),
            (429, Code::Unavailable),
            (502, Code::Unavailable),
            (503, Code::Unavailable),
            (504, Code::Unavailable),
            (500, Code::Unknown),
            (302, Code::Unknown),
        ] {
            let status = status_of(http, &[kv("content-type", "text/html")], &[]);
            assert_eq!(status.code, code, "HTTP {http}");
            assert!(
                status.message.contains(&http.to_string()),
                "{}",
                status.message
            );
        }
    }

    #[test]
    fn only_a_200_with_a_grpc_content_type_carries_messages() {
        let grpc = [kv("content-type", "application/grpc")];
        let grpc_proto = [kv("Content-Type", "application/grpc+proto")];
        let html = [kv("content-type", "text/html")];

        assert!(is_grpc_response(200, &grpc));
        assert!(is_grpc_response(200, &grpc_proto));
        assert!(!is_grpc_response(404, &grpc));
        assert!(!is_grpc_response(200, &html));
        assert!(!is_grpc_response(200, &[]));
    }

    #[test]
    fn a_200_that_is_not_grpc_says_so() {
        let status = status_of(200, &[kv("content-type", "text/html; charset=utf-8")], &[]);

        assert_eq!(status.code, Code::Unknown);
        assert!(status.message.contains("text/html"), "{}", status.message);
    }

    #[test]
    fn a_missing_status_is_unknown() {
        let status = status_of(200, &[kv("content-type", "application/grpc+proto")], &[]);

        assert_eq!(status.code, Code::Unknown);
    }

    #[test]
    fn an_unparseable_status_is_unknown() {
        assert_eq!(
            status_of(200, &[], &[kv("grpc-status", "OK")]).code,
            Code::Unknown
        );
    }

    #[test]
    fn percent_decoding_handles_utf8_and_leaves_malformed_escapes() {
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("100%25"), "100%");
        assert_eq!(percent_decode("50% off"), "50% off");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
        assert_eq!(percent_decode("tail%4"), "tail%4");
        assert_eq!(percent_decode("tail%"), "tail%");
        assert_eq!(percent_decode("lower%c3%a9"), "loweré");
    }

    // --- metadata

    #[test]
    fn metadata_is_lowercased_and_empty_rows_are_dropped() {
        let checked = check_metadata(&[
            kv("X-Request-Id", " abc "),
            kv("", "ignored"),
            kv("  ", "x"),
        ])
        .expect("valid");

        assert_eq!(checked, vec![kv("x-request-id", "abc")]);
    }

    #[test]
    fn reserved_keys_are_refused_by_name() {
        for name in [
            "grpc-timeout",
            "grpc-status",
            "content-type",
            "te",
            "connection",
            "Host",
        ] {
            let error = check_metadata(&[kv(name, "x")]).expect_err(name);
            assert!(
                error.to_string().contains(&name.to_ascii_lowercase()),
                "{error}"
            );
        }
    }

    #[test]
    fn keys_with_characters_outside_the_protocols_set_are_refused() {
        for name in ["with space", "colon:", "ümlaut", ":path"] {
            assert!(check_metadata(&[kv(name, "x")]).is_err(), "{name}");
        }
    }

    #[test]
    fn text_values_must_be_printable_ascii() {
        assert!(check_metadata(&[kv("note", "héllo")]).is_err());
        assert!(check_metadata(&[kv("note", "tab\there")]).is_err());
        assert!(check_metadata(&[kv("note", "plain text, fine!")]).is_ok());
    }

    #[test]
    fn binary_values_must_be_base64_padded_or_not() {
        assert!(check_metadata(&[kv("trace-bin", "AAEC")]).is_ok());
        assert!(check_metadata(&[kv("trace-bin", "AAE=")]).is_ok());
        assert!(check_metadata(&[kv("trace-bin", "AAE")]).is_ok());
        assert!(check_metadata(&[kv("trace-bin", "not base64!")]).is_err());
        assert!(check_metadata(&[kv("trace-bin", "A")]).is_err());
        assert!(check_metadata(&[kv("trace-bin", "AA===")]).is_err());
    }

    #[test]
    fn an_api_key_in_the_query_string_is_refused() {
        let query = Auth::ApiKey {
            key: "k".into(),
            value: "v".into(),
            location: ApiKeyLocation::Query,
        };
        let header = Auth::ApiKey {
            key: "k".into(),
            value: "v".into(),
            location: ApiKeyLocation::Header,
        };

        assert!(check_auth(&query).is_err());
        assert!(check_auth(&header).is_ok());
        assert!(check_auth(&Auth::Bearer { token: "t".into() }).is_ok());
    }

    // --- request headers

    #[test]
    fn every_call_carries_the_protocol_headers_then_the_metadata() {
        let headers = request_headers(&[kv("x-id", "1")], None);

        assert_eq!(headers[0], kv("content-type", "application/grpc"));
        assert_eq!(headers[1], kv("te", "trailers"));
        assert!(headers[2].value.starts_with("ResponderHTTP/"));
        assert_eq!(headers.last(), Some(&kv("x-id", "1")));
        assert!(find(&headers, "grpc-timeout").is_none());
    }

    #[test]
    fn a_user_agent_in_the_metadata_replaces_the_apps() {
        let headers = request_headers(&[kv("user-agent", "mine/1")], None);

        let agents: Vec<&str> = headers
            .iter()
            .filter(|h| h.name == "user-agent")
            .map(|h| h.value.as_str())
            .collect();
        assert_eq!(agents, vec!["mine/1"]);
    }

    #[test]
    fn a_deadline_is_sent_as_grpc_timeout() {
        let headers = request_headers(&[], Some(Duration::from_secs(5)));

        assert_eq!(find(&headers, "grpc-timeout"), Some("5000000u"));
    }

    #[test]
    fn timeouts_use_the_finest_unit_that_fits_in_eight_digits_rounding_up() {
        assert_eq!(encode_timeout(Duration::from_nanos(1)), "1n");
        assert_eq!(
            encode_timeout(Duration::from_nanos(99_999_999)),
            "99999999n"
        );
        assert_eq!(encode_timeout(Duration::from_nanos(100_000_001)), "100001u");
        assert_eq!(encode_timeout(Duration::from_millis(250)), "250000u");
        assert_eq!(encode_timeout(Duration::from_secs(100_000)), "100000S");
        assert_eq!(
            encode_timeout(Duration::from_secs(3_600 * 200_000_000)),
            "99999999H"
        );
        assert_eq!(encode_timeout(Duration::ZERO), "0n");
    }
}

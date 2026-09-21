// http_client/src-tauri/src/http/mapping.rs
//
// Domain <-> libcurl translation. Kept out of curl_client.rs so the client
// stays about performing requests, and these conversions stay unit-testable.
use std::time::Duration;

use curl::easy::{Easy2, Form};

use crate::domain::error::AppError;
use crate::domain::models::{KeyValue, MultipartPart, RequestBody, Timing};

/// libcurl reports cumulative offsets from the start of the request; the UI
/// wants per-phase durations. `appconnect` is zero on plain HTTP, so the TLS
/// phase collapses to zero there rather than going negative.
pub fn timing_from_offsets(
    namelookup: Duration,
    connect: Duration,
    appconnect: Duration,
    starttransfer: Duration,
    total: Duration,
) -> Timing {
    let handshake_end = if appconnect.is_zero() {
        connect
    } else {
        appconnect
    };
    Timing {
        dns: namelookup,
        connect: connect.saturating_sub(namelookup),
        tls: appconnect.saturating_sub(connect),
        time_to_first_byte: starttransfer.saturating_sub(handshake_end),
        total,
    }
}

/// Header lines arrive raw, including the status line and the blank line that
/// ends each header block. Only `Name: value` lines become headers; a
/// redirect chain replaces the collected set, so the last block wins.
pub fn parse_header_line(line: &[u8]) -> Option<KeyValue> {
    let line = String::from_utf8_lossy(line);
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() || line.starts_with("HTTP/") {
        return None;
    }
    let (name, value) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(KeyValue::new(name, value.trim()))
}

/// Status codes above u16 range are not reachable over HTTP, but
/// `response_code()` is a u32, so clamp rather than panic on a cast.
pub fn status_from_curl<H>(easy: &Easy2<H>) -> Result<u16, curl::Error> {
    let code = easy.response_code()?;
    Ok(u16::try_from(code).unwrap_or(0))
}

/// The URL libcurl is given.
///
/// With `encode` on (the default, "Encode URL automatically"), characters a
/// URL cannot carry are percent-encoded first — see `encode_url_for_send` —
/// and appended parameters are fully encoded. With it off, the typed URL and
/// the parameters go out exactly as written.
///
/// Since the Params tab became a view of the URL (2026-09-17), `params` only
/// holds what a request saved before that still carries, plus an API key the
/// Auth tab sends in the query.
pub fn build_url(base: &str, params: &[KeyValue], encode: bool) -> String {
    let base = if encode {
        encode_url_for_send(base)
    } else {
        base.to_string()
    };
    let base = base.as_str();
    if params.is_empty() {
        return base.to_string();
    }
    let query = if encode {
        encode_form(params)
    } else {
        raw_form(params)
    };
    let separator = if base.contains('?') { '&' } else { '?' };
    // A trailing separator in the base means the caller already opened the
    // query string, so adding another would produce `?&`.
    if base.ends_with('?') || base.ends_with('&') {
        format!("{base}{query}")
    } else {
        format!("{base}{separator}{query}")
    }
}

/// Percent-encodes what libcurl would reject or send raw in the path, query
/// and fragment, and nothing else — what Postman calls "encode URL
/// automatically". Measured against libcurl 8.21 on 2026-09-17: a space is
/// refused outright ("Malformed input to a URL function"), while `ö` or `"`
/// go out as raw bytes that servers answer with 400.
///
/// - Text that is already a valid URL stays byte-for-byte the same:
///   `%XX` escapes, `&`, `=`, `+`, `/`, `?`, `:`, `@` and the other
///   characters RFC 3986 allows are left as they are. `+` keeps whatever
///   meaning the server gives it.
/// - A `%` that does not start a two-hex-digit escape becomes `%25`.
/// - The scheme and authority are left alone: the host is libcurl's to parse
///   (IDN, IPv6 brackets), and a user name or password there is not ours to
///   rewrite.
pub fn encode_url_for_send(url: &str) -> String {
    let Some(scheme_end) = url.find("://").map(|index| index + 3) else {
        return url.to_string();
    };
    let authority_end = url[scheme_end..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |index| scheme_end + index);
    let (head, rest) = url.split_at(authority_end);

    let bytes = rest.as_bytes();
    let mut out = String::with_capacity(url.len() + 8);
    out.push_str(head);
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            let escape = bytes.get(index + 1..index + 3);
            if escape.is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit)) {
                out.push('%');
            } else {
                out.push_str("%25");
            }
        } else if is_url_safe(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
        index += 1;
    }
    out
}

/// RFC 3986 `unreserved`, `sub-delims`, and the delimiters a path, query or
/// fragment may contain. `#` is kept so a fragment stays a fragment.
fn is_url_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'/'
                | b'?'
                | b'#'
        )
}

/// `application/x-www-form-urlencoded`: used for both query strings and form
/// bodies, which are the same encoding.
pub fn encode_form(fields: &[KeyValue]) -> String {
    fields
        .iter()
        .map(|field| {
            format!(
                "{}={}",
                percent_encode(&field.name),
                percent_encode(&field.value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Joins parameters without touching them. For a caller that has already
/// encoded its own values, or is deliberately sending something this
/// encoder would mangle.
fn raw_form(fields: &[KeyValue]) -> String {
    fields
        .iter()
        .map(|field| format!("{}={}", field.name, field.value))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encodes everything outside the unreserved set of RFC 3986. Spaces
/// become %20 rather than `+`: valid in both a query string and a form body,
/// while `+` is only correct in the latter.
fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub struct EncodedBody {
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Turns a domain body into bytes plus the content type it implies. A raw
/// body keeps whatever content type the user chose; the encoded forms
/// dictate their own.
///
/// Multipart returns None here and is **not** a missing case: libcurl builds
/// and streams that one itself (`build_form` below), so there is no buffer to
/// produce and no boundary for us to pick. The caller matches on the variant
/// rather than relying on this None, so adding a body type cannot silently
/// fall through to "no body".
pub fn encode_body(body: &RequestBody) -> Option<EncodedBody> {
    match body {
        RequestBody::None | RequestBody::Multipart(_) => None,
        RequestBody::Raw { content_type, text } => Some(EncodedBody {
            content_type: Some(content_type.clone()),
            bytes: text.as_bytes().to_vec(),
        }),
        RequestBody::FormUrlEncoded(fields) => Some(EncodedBody {
            content_type: Some("application/x-www-form-urlencoded".into()),
            bytes: encode_form(fields).into_bytes(),
        }),
    }
}

/// Hands the multipart body to libcurl rather than encoding it ourselves.
///
/// `Form` owns copies of every name and inline value once `add()` has run
/// (its fields are `Vec<CString>` and `Vec<Vec<u8>>`), so the borrows here
/// only have to live until this function returns. A **file** part stores the
/// path and `CURLFORM_FILE`, which means libcurl opens and reads the file
/// during the transfer — an upload never sits in this process's memory, which
/// is the whole reason multipart stopped being hand-rolled.
///
/// libcurl also picks the boundary and sets Content-Type, so neither appears
/// anywhere in this file any more.
pub fn build_form(parts: &[MultipartPart]) -> Result<Form, AppError> {
    let mut form = Form::new();
    for part in parts {
        let name = part.name().trim();
        // A blank name is the table's trailing row, not an error.
        if name.is_empty() {
            continue;
        }
        let mut entry = form.part(name);
        match part {
            MultipartPart::Text { value, .. } => {
                entry.contents(value.as_bytes());
            }
            MultipartPart::File {
                path, content_type, ..
            } => {
                entry.file(path);
                // Without this libcurl sends the full path as the filename on
                // some platforms; the server should see the basename.
                if let Some(file_name) = path.file_name() {
                    entry.filename(file_name);
                }
                if let Some(content_type) = content_type {
                    entry.content_type(content_type);
                }
            }
        }
        entry
            .add()
            .map_err(|error| AppError::InvalidRequest(format!("part \"{name}\": {error}")))?;
    }
    Ok(form)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn kv(name: &str, value: &str) -> KeyValue {
        KeyValue::new(name, value)
    }

    #[test]
    fn splits_cumulative_offsets_into_phases() {
        let timing = timing_from_offsets(ms(10), ms(30), ms(60), ms(100), ms(140));

        assert_eq!(timing.dns, ms(10));
        assert_eq!(timing.connect, ms(20));
        assert_eq!(timing.tls, ms(30));
        assert_eq!(timing.time_to_first_byte, ms(40));
        assert_eq!(timing.total, ms(140));
    }

    #[test]
    fn reports_no_tls_phase_for_plain_http() {
        let timing = timing_from_offsets(ms(10), ms(30), Duration::ZERO, ms(50), ms(70));

        assert_eq!(timing.tls, Duration::ZERO);
        assert_eq!(timing.time_to_first_byte, ms(20));
    }

    #[test]
    fn never_underflows_when_offsets_are_out_of_order() {
        let timing = timing_from_offsets(ms(30), ms(10), Duration::ZERO, ms(5), ms(40));

        assert_eq!(timing.connect, Duration::ZERO);
        assert_eq!(timing.time_to_first_byte, Duration::ZERO);
    }

    #[test]
    fn parses_a_header_line() {
        let header = parse_header_line(b"Content-Type: application/json\r\n")
            .expect("should parse a normal header");

        assert_eq!(header.name, "Content-Type");
        assert_eq!(header.value, "application/json");
    }

    #[test]
    fn skips_status_and_blank_lines() {
        assert!(parse_header_line(b"HTTP/2 200 \r\n").is_none());
        assert!(parse_header_line(b"\r\n").is_none());
        assert!(parse_header_line(b"garbage\r\n").is_none());
    }

    #[test]
    fn keeps_colons_inside_a_header_value() {
        let header =
            parse_header_line(b"Location: https://example.com:8443/x\r\n").expect("should parse");

        assert_eq!(header.value, "https://example.com:8443/x");
    }

    #[test]
    fn leaves_a_url_alone_when_there_are_no_params() {
        assert_eq!(
            build_url("https://example.com/x", &[], true),
            "https://example.com/x"
        );
    }

    #[test]
    fn appends_params_with_the_right_separator() {
        assert_eq!(
            build_url("https://example.com/x", &[kv("a", "1"), kv("b", "2")], true),
            "https://example.com/x?a=1&b=2"
        );
        assert_eq!(
            build_url("https://example.com/x?existing=1", &[kv("a", "2")], true),
            "https://example.com/x?existing=1&a=2"
        );
        assert_eq!(
            build_url("https://example.com/x?", &[kv("a", "1")], true),
            "https://example.com/x?a=1"
        );
    }

    #[test]
    fn percent_encodes_reserved_characters_and_unicode() {
        assert_eq!(
            encode_form(&[kv("q", "a b&c=d"), kv("emoji", "é")]),
            "q=a%20b%26c%3Dd&emoji=%C3%A9"
        );
    }

    /// The point of the toggle: a value that is already encoded, or that the
    /// user wants sent verbatim, survives instead of being encoded twice.
    #[test]
    fn a_valid_url_is_sent_unchanged() {
        for url in [
            "https://example.com/users/42?q=a%20b&tags=x,y&sum=1+2#top",
            "https://user:p%40ss@example.com:8443/a;b/c?x=&y=*!$'()@:/?",
            "http://[::1]:8080/",
            "https://example.com",
        ] {
            assert_eq!(encode_url_for_send(url), url);
        }
    }

    #[test]
    fn characters_a_url_cannot_carry_are_encoded_when_sent() {
        assert_eq!(
            encode_url_for_send("https://example.com/a b/ö?q=a b&c=\"x\"&d=<{1}>|^`\\"),
            "https://example.com/a%20b/%C3%B6?q=a%20b&c=%22x%22&d=%3C%7B1%7D%3E%7C%5E%60%5C"
        );
    }

    #[test]
    fn a_stray_percent_is_encoded_and_a_real_escape_is_not() {
        assert_eq!(
            encode_url_for_send("https://example.com/?a=100%&b=%41&c=%4&d=%zz&e=%"),
            "https://example.com/?a=100%25&b=%41&c=%254&d=%25zz&e=%25"
        );
    }

    #[test]
    fn the_authority_is_left_to_libcurl() {
        assert_eq!(
            encode_url_for_send("https://bücher.example/ä"),
            "https://bücher.example/%C3%A4"
        );
    }

    #[test]
    fn a_url_without_a_scheme_is_not_touched() {
        assert_eq!(encode_url_for_send("{{base}}/a b"), "{{base}}/a b");
    }

    #[test]
    fn encoding_is_applied_to_the_typed_url_only_when_asked() {
        assert_eq!(
            build_url("https://example.com/x?q=a b", &[], true),
            "https://example.com/x?q=a%20b"
        );
        assert_eq!(
            build_url("https://example.com/x?q=a b", &[], false),
            "https://example.com/x?q=a b"
        );
    }

    #[test]
    fn skips_encoding_when_the_caller_asks_it_to() {
        assert_eq!(
            build_url("https://example.com/x", &[kv("q", "a b&c")], false),
            "https://example.com/x?q=a b&c"
        );
        assert_eq!(
            build_url("https://example.com/x", &[kv("q", "a b&c")], true),
            "https://example.com/x?q=a%20b%26c"
        );
    }

    #[test]
    fn leaves_unreserved_characters_untouched() {
        assert_eq!(
            encode_form(&[kv("k-e.y_~", "v-a.l_~9")]),
            "k-e.y_~=v-a.l_~9"
        );
    }

    #[test]
    fn encodes_a_raw_body_with_its_declared_content_type() {
        let encoded = encode_body(&RequestBody::Raw {
            content_type: "application/json".into(),
            text: "{\"a\":1}".into(),
        })
        .expect("raw body should encode");

        assert_eq!(encoded.content_type.as_deref(), Some("application/json"));
        assert_eq!(encoded.bytes, b"{\"a\":1}");
    }

    #[test]
    fn reports_an_empty_body_as_nothing_to_send() {
        assert!(encode_body(&RequestBody::None).is_none());
    }

    /// Not "no body": libcurl builds this one, and the caller matches on the
    /// variant rather than on this None. Asserted so the distinction is
    /// written down somewhere a future reader will run.
    #[test]
    fn multipart_produces_no_buffer_because_libcurl_builds_it() {
        assert!(encode_body(&RequestBody::Multipart(Vec::new())).is_none());
    }

    #[test]
    fn a_form_skips_the_trailing_blank_row_rather_than_rejecting_it() {
        let parts = vec![
            MultipartPart::Text {
                name: "field".into(),
                value: "value".into(),
            },
            MultipartPart::Text {
                name: "  ".into(),
                value: String::new(),
            },
        ];

        build_form(&parts).expect("a blank name is the table's spare row, not an error");
    }

    /// A file part is accepted here even when the path is missing — libcurl
    /// only opens it at transfer time. Catching it earlier is the job of
    /// validate_multipart in the send-request service, which is where the
    /// user-facing message comes from.
    #[test]
    fn a_file_part_builds_without_touching_the_filesystem() {
        let parts = vec![MultipartPart::File {
            name: "upload".into(),
            path: std::path::PathBuf::from("/definitely/not/here.png"),
            content_type: Some("image/png".into()),
        }];

        build_form(&parts).expect("should build");
    }
}

// http_client/src-tauri/src/logging.rs
//
// One JSON log line for tauri-plugin-log's file target. Pulled out of lib.rs's
// plugin setup so the mapping logic gets real `cargo test` coverage while the
// OS-facing plugin wiring does not — the same split permissions.rs and
// downloads.rs use in the Lockoncam desktop app this is ported from.
//
// The addition that app did not need: **redaction**. Lockoncam logs a consent
// workflow; this app logs an HTTP client, where the interesting strings are
// exactly the dangerous ones. CLAUDE.md section 11 rule 6 forbids logging
// request bodies, auth headers, tokens and cookies, and `redact` below is the
// backstop for it.
//
// Redaction runs here rather than at each call site, and rather than in
// TypeScript, because every line converges on this function: a `log::info!`
// in Rust and a `logInfo()` from the frontend both arrive as a `log::Record`
// on the way to the format closure. One implementation, no drift, no way to
// route around it.
use serde_json::json;

use crate::domain::secrets::is_credential_param_name;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const REDACTED: &str = "[redacted]";

/// Auth scheme prefixes whose following word is the credential.
const SCHEMES: [&str; 3] = ["bearer ", "basic ", "digest "];

pub fn format_log_entry(message: &std::fmt::Arguments, record: &log::Record) -> String {
    json!({
        "level": record.level().to_string(),
        "target": record.target(),
        "message": redact(&message.to_string()),
        "version": APP_VERSION,
        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%f").to_string(),
    })
    .to_string()
}

/// Strips credentials that tend to ride along inside an otherwise innocuous
/// message — a URL with a key in the query string, an auth header echoed into
/// an error, a Cookie line.
///
/// **This is a safety net, not a licence.** It cannot know that an opaque
/// string is a token, so the rule stands: do not put bodies, headers, tokens
/// or cookies into a log message in the first place.
pub fn redact(message: &str) -> String {
    let mut out = redact_query_params(message);
    out = redact_schemes(&out);
    redact_cookie_headers(&out)
}

/// Rewrites `name=value` pairs whose name is a known credential parameter.
/// Scans the whole message rather than parsing a URL: the interesting strings
/// arrive embedded in prose ("GET https://… failed"), not on their own.
fn redact_query_params(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(equals) = rest.find('=') {
        let (before, after) = rest.split_at(equals);
        // The parameter name is whatever trails the last separator.
        let name_start = before
            .rfind(['?', '&', ' ', '"', '\'', ';', ','])
            .map_or(0, |index| index + 1);
        let name = &before[name_start..];

        out.push_str(before);
        out.push('=');
        let value = &after[1..];
        // Which names count is shared with the OpenAPI exporter
        // (domain::secrets). `ApiKeyLocation::Query` is a supported auth mode,
        // so a logged URL genuinely can carry a key.
        if !name.is_empty() && is_credential_param_name(name) {
            let end = value
                .find(['&', ' ', '"', '\'', ';', ',', '\n'])
                .unwrap_or(value.len());
            out.push_str(REDACTED);
            rest = &value[end..];
        } else {
            rest = value;
        }
    }
    out.push_str(rest);
    out
}

fn redact_schemes(message: &str) -> String {
    let lowered = message.to_ascii_lowercase();
    let mut out = String::with_capacity(message.len());
    let mut cursor = 0;
    while cursor < message.len() {
        let next = SCHEMES
            .iter()
            .filter_map(|scheme| {
                lowered[cursor..]
                    .find(scheme)
                    .map(|at| (cursor + at, scheme))
            })
            .min_by_key(|(at, _)| *at);
        let Some((at, scheme)) = next else {
            break;
        };
        let value_start = at + scheme.len();
        out.push_str(&message[cursor..value_start]);
        let value = &message[value_start..];
        let end = value
            .find([' ', '"', '\'', ',', '\n'])
            .unwrap_or(value.len());
        if end > 0 {
            out.push_str(REDACTED);
            cursor = value_start + end;
        } else {
            cursor = value_start;
        }
    }
    out.push_str(&message[cursor.min(message.len())..]);
    out
}

/// A Cookie or Set-Cookie header carries nothing loggable, so the whole value
/// goes rather than trying to keep the names.
fn redact_cookie_headers(message: &str) -> String {
    let lowered = message.to_ascii_lowercase();
    let mut out = String::with_capacity(message.len());
    let mut cursor = 0;
    for needle in ["set-cookie:", "cookie:"] {
        // Re-scan from the start each pass so the two needles cannot interleave
        // badly; set-cookie is checked first because it contains "cookie:".
        let mut search = cursor;
        while let Some(at) = lowered[search..].find(needle) {
            let absolute = search + at;
            if absolute < cursor {
                search = absolute + needle.len();
                continue;
            }
            out.push_str(&message[cursor..absolute + needle.len()]);
            let value = &message[absolute + needle.len()..];
            let end = value.find('\n').unwrap_or(value.len());
            out.push_str(REDACTED);
            cursor = absolute + needle.len() + end;
            search = cursor;
        }
    }
    out.push_str(&message[cursor.min(message.len())..]);
    out
}

#[cfg(test)]
mod tests {
    use super::{format_log_entry, redact};
    use log::{Level, Record};
    use serde_json::Value;

    /// The message is passed alongside the record rather than through
    /// `Record::args`, which is both what tauri-plugin-log's format callback
    /// does and the only shape that borrow-checks: `format_args!` produces a
    /// temporary that cannot outlive the statement it appears in, so it can
    /// never be parked on a `Record` and used on a later line.
    fn entry(level: Level, target: &str, message: &str) -> Value {
        let record = Record::builder().level(level).target(target).build();
        let line = format_log_entry(&format_args!("{message}"), &record);
        serde_json::from_str(&line).expect("valid JSON")
    }

    #[test]
    fn includes_the_crate_version_from_cargo_toml() {
        assert_eq!(
            entry(Level::Info, "responderhttp_lib::http", "sending")["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn carries_level_target_and_message() {
        let value = entry(Level::Warn, "responderhttp_lib::http", "transport failed");

        assert_eq!(value["level"], "WARN");
        assert_eq!(value["target"], "responderhttp_lib::http");
        assert_eq!(value["message"], "transport failed");
    }

    #[test]
    fn stamps_a_non_empty_timestamp() {
        let value = entry(Level::Debug, "responderhttp_lib", "tick");

        assert!(value["timestamp"].as_str().is_some_and(|s| !s.is_empty()));
    }

    /// The one that matters: a log line is formatted through the same closure
    /// whether it came from Rust or from the frontend bridge, so redaction has
    /// to happen here or not at all.
    #[test]
    fn redacts_on_the_way_into_the_entry_not_only_in_the_helper() {
        let value = entry(
            Level::Error,
            "responderhttp_lib::http",
            "GET https://api.example.com/x?api_key=sk_live_abc123 failed",
        );

        let message = value["message"].as_str().expect("a message");
        assert!(!message.contains("sk_live_abc123"), "{message}");
        assert!(message.contains("api_key=[redacted]"), "{message}");
    }

    #[test]
    fn keeps_a_url_readable_while_dropping_only_the_credential() {
        let out = redact("GET https://api.example.com/users?page=2&token=abc&sort=name");

        assert_eq!(
            out,
            "GET https://api.example.com/users?page=2&token=[redacted]&sort=name"
        );
    }

    /// ApiKeyLocation::Query is a supported auth mode, so this is the realistic
    /// leak, not a hypothetical one.
    #[test]
    fn covers_every_spelling_of_the_key_parameter() {
        for name in ["key", "api_key", "apiKey", "API-KEY", "access_token"] {
            let out = redact(&format!("https://x.test/?{name}=secret-value"));
            assert!(!out.contains("secret-value"), "{name} leaked: {out}");
        }
    }

    #[test]
    fn redacts_an_auth_scheme_credential_whatever_its_casing() {
        assert_eq!(
            redact("Authorization: Bearer eyJhbGciOi.J9 rest"),
            "Authorization: Bearer [redacted] rest"
        );
        assert!(!redact("authorization: basic dXNlcjpwYXNz").contains("dXNlcjpwYXNz"));
    }

    #[test]
    fn drops_a_cookie_header_value_entirely() {
        let out = redact("Cookie: session=abc; theme=dark");

        assert!(!out.contains("abc"), "{out}");
        assert!(out.starts_with("Cookie:"), "{out}");
    }

    #[test]
    fn set_cookie_is_matched_before_the_cookie_substring_inside_it() {
        let out = redact("Set-Cookie: session=abc; HttpOnly");

        assert!(!out.contains("abc"), "{out}");
        assert!(out.to_ascii_lowercase().starts_with("set-cookie:"), "{out}");
    }

    #[test]
    fn leaves_an_ordinary_message_untouched() {
        let message = "collection col_1 saved with 3 requests";

        assert_eq!(redact(message), message);
    }

    #[test]
    fn leaves_a_url_with_no_credentials_untouched() {
        let message = "GET https://api.example.com/users?page=2&sort=name";

        assert_eq!(redact(message), message);
    }
}

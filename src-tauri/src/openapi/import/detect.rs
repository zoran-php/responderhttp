// http_client/src-tauri/src/openapi/import/detect.rs
//
// Bytes in, a JSON value out — or a refusal that says exactly why.
//
// The content decides the syntax, never the file name: a first
// non-whitespace `{` means JSON, anything else is tried as YAML. There
// is no fallback from one to the other. A file that looks like JSON and is
// broken reports the JSON error at its line and column, which is the error
// the user needs; retrying it as YAML would report something unrelated.
//
// "Other markup languages are not supported" is enforced by one rule rather
// than a list of sniffers: the top level must be a mapping. TOML's
// `[package]` parses as a YAML sequence, XML as a single YAML string, and
// either way the file is refused as not JSON or YAML.
use std::fmt;

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::openapi::document::OpenApiVersion;
use crate::openapi::import::{Detected, Refusal, SourceFormat};

/// The GitHub REST description, one of the largest public specs, is 13 MB.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// YAML parser limits. The node and event budgets are raised well past the
/// library defaults, which rejected both the GitHub and the Stripe specs; the
/// alias limits — the actual defence against "billion laughs" files — are
/// left exactly as the library ships them.
const YAML_MAX_NODES: usize = 10_000_000;
const YAML_MAX_EVENTS: usize = 20_000_000;
/// serde_json's own default recursion limit, so both syntaxes agree.
const MAX_DEPTH: usize = 128;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

pub fn detect(bytes: &[u8]) -> Result<Detected, Refusal> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Refusal::TooLarge {
            bytes: bytes.len() as u64,
            limit: MAX_FILE_BYTES,
        });
    }
    let text = decode_text(bytes)?;
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return Err(Refusal::Empty);
    }

    // An OpenAPI document is an object, so a top-level list can never be one —
    // and a TOML `[table]` line would otherwise earn a JSON syntax error.
    if trimmed.starts_with('[') {
        return Err(Refusal::NotJsonOrYaml);
    }
    let (document, format) = if trimmed.starts_with('{') {
        (parse_json(text)?, SourceFormat::Json)
    } else {
        (parse_yaml(text)?, SourceFormat::Yaml)
    };

    let Value::Object(root) = &document else {
        return Err(Refusal::NotJsonOrYaml);
    };
    let version = version_of(root)?;
    Ok(Detected {
        document,
        format,
        version,
    })
}

/// UTF-8 only. A byte-order mark is dropped; UTF-16 and UTF-32 files start
/// with their own marks (or with NUL bytes) and fail the UTF-8 check anyway,
/// which is the right outcome — they are refused, not guessed at.
fn decode_text(bytes: &[u8]) -> Result<&str, Refusal> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| Refusal::NotUtf8)?;
    if text.contains('\0') {
        return Err(Refusal::NotUtf8);
    }
    Ok(text)
}

fn parse_json(text: &str) -> Result<Value, Refusal> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let value = StrictValue { depth: 0 }
        .deserialize(&mut deserializer)
        .and_then(|value| deserializer.end().map(|()| value))
        .map_err(|error| Refusal::InvalidJson {
            message: error.to_string(),
        })?;
    Ok(value)
}

fn parse_yaml(text: &str) -> Result<Value, Refusal> {
    let mut options = serde_saphyr::options! {
        strict_booleans: true,
        reject_unsupported_tags: true,
        with_snippet: false,
    };
    options.budget = serde_saphyr::budget! {
        max_nodes: YAML_MAX_NODES,
        max_events: YAML_MAX_EVENTS,
        max_depth: MAX_DEPTH,
    };
    serde_saphyr::from_str_with_options::<Value>(text, options).map_err(|error| {
        Refusal::InvalidYaml {
            message: first_line(&error.to_string()),
        }
    })
}

/// The library's messages can run to several lines of context; the dialog
/// wants the sentence that says what is wrong and where.
fn first_line(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("not valid YAML")
        .trim_start_matches("error: ")
        .to_string()
}

/// Checked before schema validation so a Swagger 2.0 file or a 4.0 file gets
/// a sentence about versions instead of a page of schema errors.
fn version_of(root: &Map<String, Value>) -> Result<OpenApiVersion, Refusal> {
    let Some(field) = root.get("openapi") else {
        return Err(if root.contains_key("swagger") {
            Refusal::Swagger2
        } else {
            Refusal::NotOpenApi
        });
    };
    let Value::String(version) = field else {
        // An unquoted `openapi: 3.1` in YAML is a number, and the most
        // common way to end up here.
        return Err(Refusal::VersionNotString);
    };
    let mut parts = version.trim().split('.');
    let major_minor = (parts.next(), parts.next());
    match major_minor {
        (Some("3"), Some("0")) => Ok(OpenApiVersion::V3_0),
        (Some("3"), Some("1")) => Ok(OpenApiVersion::V3_1),
        // The schema checks the full pattern (`3.2.x`) afterwards.
        (Some("3"), Some("2")) => Ok(OpenApiVersion::V3_2),
        _ => Err(Refusal::UnsupportedVersion {
            version: version.clone(),
        }),
    }
}

/// `serde_json::Value`'s own deserializer keeps the last of two equal keys
/// without a word. A spec with two `/users` entries is ambiguous, so this one
/// refuses it — the YAML parser already does the same by default.
struct StrictValue {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for StrictValue {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictVisitor { depth: self.depth })
    }
}

struct StrictVisitor {
    depth: usize,
}

impl StrictVisitor {
    fn child<E: serde::de::Error>(&self) -> Result<StrictValue, E> {
        if self.depth >= MAX_DEPTH {
            return Err(E::custom("the document is nested too deeply"));
        }
        Ok(StrictValue {
            depth: self.depth + 1,
        })
    }
}

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("a number is out of range"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self.child()?)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                return Err(A::Error::custom(format!("duplicate key \"{key}\"")));
            }
            let value = map.next_value_seed(self.child()?)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(input: &str) -> Refusal {
        detect(input.as_bytes()).expect_err("input should be refused")
    }

    fn accepted(input: &str) -> Detected {
        detect(input.as_bytes()).expect("input should be accepted")
    }

    const MINIMAL_JSON: &str = r#"{"openapi":"3.1.0","info":{"title":"t","version":"1"}}"#;
    const MINIMAL_YAML: &str = "openapi: 3.1.0\ninfo:\n  title: t\n  version: '1'\n";

    #[test]
    fn a_brace_means_json() {
        let detected = accepted(MINIMAL_JSON);

        assert_eq!(detected.format, SourceFormat::Json);
        assert_eq!(detected.version, OpenApiVersion::V3_1);
        assert_eq!(detected.document["info"]["title"], "t");
    }

    #[test]
    fn anything_else_is_tried_as_yaml() {
        let detected = accepted(MINIMAL_YAML);

        assert_eq!(detected.format, SourceFormat::Yaml);
        assert_eq!(detected.document["info"]["version"], "1");
    }

    #[test]
    fn the_extension_plays_no_part_leading_whitespace_and_a_bom_are_fine() {
        let with_bom = format!("\u{FEFF}\n\n  {MINIMAL_JSON}");

        assert_eq!(accepted(&with_bom).format, SourceFormat::Json);
    }

    #[test]
    fn broken_json_reports_the_json_error_and_is_not_retried_as_yaml() {
        let Refusal::InvalidJson { message } = refusal("{\"openapi\": \"3.1.0\",\n  \"info\": }")
        else {
            panic!("expected a JSON error");
        };

        assert!(message.contains("line 2"), "{message}");
    }

    #[test]
    fn duplicate_keys_are_refused_in_json() {
        let Refusal::InvalidJson { message } =
            refusal(r#"{"openapi":"3.1.0","paths":{"/a":{},"/a":{}}}"#)
        else {
            panic!("expected a JSON error");
        };

        assert!(message.contains("duplicate key \"/a\""), "{message}");
    }

    #[test]
    fn duplicate_keys_are_refused_in_yaml() {
        assert!(matches!(
            refusal("openapi: 3.1.0\npaths:\n  /a: {}\n  /a: {}\n"),
            Refusal::InvalidYaml { .. }
        ));
    }

    #[test]
    fn several_yaml_documents_are_refused() {
        assert!(matches!(
            refusal(&format!("{MINIMAL_YAML}---\n{MINIMAL_YAML}")),
            Refusal::InvalidYaml { .. }
        ));
    }

    #[test]
    fn yaml_1_1_booleans_stay_strings() {
        let detected = accepted(&format!(
            "{MINIMAL_YAML}x-flags: [on, off, yes, no, true]\n"
        ));

        assert_eq!(
            detected.document["x-flags"],
            serde_json::json!(["on", "off", "yes", "no", true])
        );
    }

    #[test]
    fn numeric_response_codes_become_string_keys() {
        let detected = accepted(&format!(
            "{MINIMAL_YAML}paths:\n  /a:\n    get:\n      responses:\n        200:\n          description: ok\n"
        ));

        assert_eq!(
            detected.document["paths"]["/a"]["get"]["responses"]["200"]["description"],
            "ok"
        );
    }

    #[test]
    fn an_alias_bomb_is_refused() {
        let mut bomb = String::from("openapi: 3.1.0\na: &a [x,x,x,x,x,x,x,x,x]\n");
        let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
        for pair in names.windows(2) {
            let (previous, name) = (pair[0], pair[1]);
            let row = vec![format!("*{previous}"); 9].join(",");
            bomb.push_str(&format!("{name}: &{name} [{row}]\n"));
        }

        assert!(matches!(refusal(&bomb), Refusal::InvalidYaml { .. }));
    }

    #[test]
    fn unknown_yaml_tags_are_refused() {
        assert!(matches!(
            refusal(&format!("{MINIMAL_YAML}x-thing: !custom value\n")),
            Refusal::InvalidYaml { .. }
        ));
    }

    #[test]
    fn other_markup_is_not_json_or_yaml() {
        for input in [
            "<?xml version=\"1.0\"?><openapi>3.1.0</openapi>",
            "[package]\nname = \"x\"\n",
            "just some words",
            "- a\n- b\n",
            "[1, 2]",
            "42",
        ] {
            assert_eq!(refusal(input), Refusal::NotJsonOrYaml, "{input}");
        }
    }

    #[test]
    fn toml_with_a_table_body_fails_to_parse_as_yaml() {
        assert!(matches!(
            refusal("title = \"x\"\n[owner]\nname = \"y\"\n"),
            Refusal::InvalidYaml { .. } | Refusal::NotJsonOrYaml
        ));
    }

    #[test]
    fn empty_and_whitespace_files_are_refused() {
        assert_eq!(refusal(""), Refusal::Empty);
        assert_eq!(refusal(" \n\t"), Refusal::Empty);
    }

    #[test]
    fn utf16_is_refused() {
        let utf16: Vec<u8> = "\u{FEFF}{\"openapi\":\"3.1.0\"}"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();

        assert_eq!(detect(&utf16).unwrap_err(), Refusal::NotUtf8);
    }

    #[test]
    fn invalid_utf8_is_refused() {
        assert_eq!(detect(b"openapi: \xFF\xFE").unwrap_err(), Refusal::NotUtf8);
    }

    #[test]
    fn a_file_over_the_cap_is_refused_before_parsing() {
        let big = vec![b' '; MAX_FILE_BYTES as usize + 1];

        assert!(matches!(
            detect(&big).unwrap_err(),
            Refusal::TooLarge {
                limit: MAX_FILE_BYTES,
                ..
            }
        ));
    }

    #[test]
    fn swagger_2_gets_its_own_message() {
        assert_eq!(
            refusal("swagger: '2.0'\ninfo: {title: t, version: '1'}\n"),
            Refusal::Swagger2
        );
    }

    #[test]
    fn a_mapping_without_openapi_is_not_an_openapi_document() {
        assert_eq!(refusal("name: x\nversion: 1\n"), Refusal::NotOpenApi);
    }

    #[test]
    fn an_unquoted_version_is_a_number_and_is_refused() {
        assert_eq!(
            refusal("openapi: 3.1\ninfo: {title: t, version: '1'}\n"),
            Refusal::VersionNotString
        );
    }

    #[test]
    fn versions_are_matched_on_major_and_minor() {
        for (input, expected) in [
            ("3.0.3", OpenApiVersion::V3_0),
            ("3.1.1", OpenApiVersion::V3_1),
            ("3.2.0", OpenApiVersion::V3_2),
        ] {
            let text = format!(r#"{{"openapi":"{input}"}}"#);
            assert_eq!(accepted(&text).version, expected, "{input}");
        }
        for input in ["4.0.0", "3.3.0", "2.0", "three"] {
            let text = format!(r#"{{"openapi":"{input}"}}"#);
            assert_eq!(
                refusal(&text),
                Refusal::UnsupportedVersion {
                    version: input.to_string()
                },
                "{input}"
            );
        }
    }

    #[test]
    fn json_nesting_is_capped() {
        let deep = format!(
            "{{\"openapi\":\"3.1.0\",\"x\":{}{}}}",
            "[".repeat(200),
            "]".repeat(200)
        );

        assert!(matches!(refusal(&deep), Refusal::InvalidJson { .. }));
    }
}

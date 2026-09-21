// http_client/src-tauri/src/openapi/format.rs
//
// Which text format an exported document is written in (PLAN.md Phase 8e).
// The only file that knows there is more than one: the document structs, the
// mapping and inference are format-agnostic, and serde's attributes apply to
// both serializers alike.
//
// YAML is written the conservative way, because YAML reinterprets unquoted
// text and an OpenAPI document is full of strings that look like something
// else — status codes ("200"), versions ("3.0"), example values ("007",
// "yes", "null"). The serializer quotes those; the round-trip test below is
// what proves it rather than assumes it.
use serde_saphyr::{ser_options, SerializerOptions};

use crate::domain::error::AppError;
use crate::openapi::document::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Yaml,
}

impl ExportFormat {
    /// How the frontend names it. Anything else is the two sides disagreeing,
    /// which is a bug to surface rather than a default to guess — the same
    /// rule as `OpenApiVersion::from_wire`.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "json" => Some(Self::Json),
            "yaml" => Some(Self::Yaml),
            _ => None,
        }
    }

    /// `.yaml` rather than `.yml`: the OpenAPI specification recommends
    /// `openapi.json` and `openapi.yaml` for a root document.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }

    /// What the Save As dialog filters on. `.yml` is accepted for YAML
    /// because it is what many people type, and the content is the same.
    pub fn dialog_filter(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Json => ("OpenAPI document (JSON)", &["json"]),
            Self::Yaml => ("OpenAPI document (YAML)", &["yaml", "yml"]),
        }
    }
}

/// The document as text, pretty-printed in both formats: the file's purpose
/// is to be read and diffed, not to be small.
pub fn render(document: &Document, format: ExportFormat) -> Result<String, AppError> {
    match format {
        ExportFormat::Json => serde_json::to_string_pretty(document).map_err(serialise_error),
        ExportFormat::Yaml => {
            serde_saphyr::to_string_with_options(document, yaml_options()).map_err(serialise_error)
        }
    }
}

/// Every setting that affects how a value reads back is spelled out, so a
/// change in the library's defaults cannot change the output unnoticed. Built
/// through the crate's own macro: the struct is `#[non_exhaustive]`.
///
/// - `prefer_block_scalars: false`. Block scalars (`|`, `>`) are the one
///   place where YAML's whitespace rules — chomping, indentation indicators,
///   folding long lines into spaces — can alter a string on the way back in,
///   and a saved response body is exactly the kind of string that has leading
///   spaces and trailing newlines. With this off, a multi-line string is
///   written double-quoted with `\n` escapes: less pretty, never ambiguous.
/// - `yaml_12: false`. Quote the YAML 1.1 words (`yes`, `no`, `on`, `off`,
///   `y`, `n`) too. Plenty of OpenAPI tooling still parses YAML 1.1, where
///   those unquoted are booleans.
/// - `quote_all: false`. Plain scalars where they are unambiguous keep the
///   file readable; the serializer quotes whatever needs it.
/// - `empty_as_braces: true`. `{}` and `[]` rather than an empty value,
///   which a reader would take as null.
fn yaml_options() -> SerializerOptions {
    ser_options! {
        indent_step: 2,
        prefer_block_scalars: false,
        yaml_12: false,
        quote_all: false,
        empty_as_braces: true,
        tagged_enums: false,
    }
}

fn serialise_error(error: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("could not serialise document: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::openapi::document::{
        Components, Example, ExampleBody, Info, MediaType, Operation, Parameter, PathItem,
        RequestBody, Response, Schema, SchemaType, SecurityScheme, Server, ServerVariable, Tag,
    };

    #[test]
    fn accepts_exactly_the_two_formats_the_dialog_offers() {
        assert_eq!(ExportFormat::from_wire("json"), Some(ExportFormat::Json));
        assert_eq!(ExportFormat::from_wire("yaml"), Some(ExportFormat::Yaml));
        assert_eq!(ExportFormat::from_wire("yml"), None);
        assert_eq!(ExportFormat::from_wire("JSON"), None);
        assert_eq!(ExportFormat::from_wire(""), None);
    }

    #[test]
    fn each_format_has_its_extension_and_filter() {
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Yaml.extension(), "yaml");
        assert_eq!(ExportFormat::Json.dialog_filter().1, &["json"]);
        assert_eq!(ExportFormat::Yaml.dialog_filter().1, &["yaml", "yml"]);
    }

    /// Strings that YAML would turn into something else if written bare.
    /// Every one of them is a plausible value in an exported document.
    const TRAPS: &[&str] = &[
        "200",
        "404",
        "3.0",
        "3.0.0",
        "007",
        "1e3",
        "0x1F",
        "0o17",
        "+1",
        "-0",
        ".5",
        "1_000",
        "12:30:00",
        "2026-09-16",
        "yes",
        "no",
        "on",
        "off",
        "y",
        "n",
        "Y",
        "NO",
        "true",
        "False",
        "null",
        "Null",
        "~",
        "",
        " ",
        ".inf",
        "-.Inf",
        ".nan",
        "{\"id\":1}",
        "[1, 2]",
        "*alias",
        "&anchor",
        "!tag",
        "%directive",
        "@at",
        "`tick",
        "#comment",
        "a #b",
        "key: value",
        "- item",
        "? question",
        "| pipe",
        "> fold",
        "---",
        "...",
        "'single'",
        "\"double\"",
        "back\\slash",
        "tab\there",
        "line\nbreak",
        "  leading spaces",
        "trailing spaces  ",
        "trailing newline\n",
        "\n\nleading newlines",
        "two trailing\n\n",
        "  indented\n    body\n",
        "crlf\r\nline",
        "ünïcödé — 日本語 🚀",
        "\u{feff}bom",
        "nul\u{0}byte",
        "bell\u{7}",
        "\u{85}next-line",
        "\u{2028}line-separator",
        "\u{fffe}",
    ];

    fn trap_schema() -> Schema {
        Schema {
            kind: Some(SchemaType::Many(vec!["string".into(), "null".into()])),
            nullable: None,
            properties: TRAPS
                .iter()
                .map(|trap| {
                    (
                        (*trap).to_string(),
                        Schema {
                            kind: Some(SchemaType::One("string".into())),
                            ..Schema::default()
                        },
                    )
                })
                .collect(),
            required: TRAPS.iter().map(|trap| (*trap).to_string()).collect(),
            items: Some(Box::new(Schema::default())),
            one_of: vec![
                Schema {
                    kind: Some(SchemaType::One("integer".into())),
                    nullable: Some(true),
                    ..Schema::default()
                },
                Schema::default(),
            ],
        }
    }

    /// One document carrying every trap in every position a string can take:
    /// map keys, plain values, example bodies (serialized and decoded), and
    /// the fields the spec itself constrains (`openapi`, response codes).
    fn trap_document() -> Document {
        let decoded = serde_json::json!({
            "strings": TRAPS,
            "numbers": [0, -1, 1.5, 1e300, u64::MAX, i64::MIN],
            "bools": [true, false],
            "null": null,
            "empty_object": {},
            "empty_array": [],
            "nested": { "200": { "yes": ["no", null, { "~": "" }] } },
        });

        let mut examples = BTreeMap::new();
        for (index, trap) in TRAPS.iter().enumerate() {
            examples.insert(
                format!("serialized {index}"),
                Example::new(
                    Some((*trap).to_string()),
                    Some(ExampleBody::Serialized((*trap).to_string())),
                ),
            );
        }
        examples.insert(
            "serialized json body".into(),
            Example::new(
                None,
                Some(ExampleBody::Serialized(
                    "{\n  \"id\": 1,\n  \"name\": \"yes\"\n}\n".into(),
                )),
            ),
        );
        examples.insert(
            "decoded".into(),
            Example::new(None, Some(ExampleBody::Decoded(decoded))),
        );
        examples.insert("empty".into(), Example::new(None, None));

        let mut content = BTreeMap::new();
        content.insert(
            "application/json".to_string(),
            MediaType {
                schema: Some(trap_schema()),
                examples,
            },
        );

        let mut responses = BTreeMap::new();
        for code in ["200", "204", "404", "default", "2XX"] {
            responses.insert(
                code.to_string(),
                Response {
                    description: format!("{code} response"),
                    content: content.clone(),
                },
            );
        }

        let parameters = TRAPS
            .iter()
            .map(|trap| Parameter {
                name: (*trap).to_string(),
                location: "query".into(),
                required: Some(false),
                schema: None,
                example: Some((*trap).to_string()),
            })
            .collect();

        let mut paths = BTreeMap::new();
        paths.insert(
            "/users/{userId}".to_string(),
            PathItem {
                get: Some(Operation {
                    tags: TRAPS.iter().map(|trap| (*trap).to_string()).collect(),
                    summary: Some("yes".into()),
                    operation_id: Some("007".into()),
                    parameters,
                    request_body: Some(RequestBody {
                        description: Some("null".into()),
                        content: content.clone(),
                    }),
                    responses: responses.clone(),
                    security: Some(vec![BTreeMap::from([("on".to_string(), Vec::new())])]),
                }),
                ..PathItem::default()
            },
        );
        paths.insert("/".to_string(), PathItem::default());
        paths.insert("/200".to_string(), PathItem::default());

        Document {
            openapi: "3.0.0".into(),
            info: Info {
                title: "no".into(),
                version: "1.0".into(),
                summary: Some("~".into()),
                description: Some("  indented\n    body\n".into()),
            },
            servers: vec![Server {
                url: "{base_url}".into(),
                description: None,
                name: Some("off".into()),
                variables: BTreeMap::from([(
                    "base_url".to_string(),
                    ServerVariable {
                        default: "<base_url>".into(),
                        description: Some("#comment".into()),
                    },
                )]),
            }],
            paths,
            tags: TRAPS
                .iter()
                .map(|trap| Tag {
                    name: (*trap).to_string(),
                    description: Some((*trap).to_string()),
                })
                .collect(),
            components: Some(Components {
                security_schemes: BTreeMap::from([(
                    "true".to_string(),
                    SecurityScheme {
                        kind: "http".into(),
                        description: None,
                        scheme: Some("bearer".into()),
                        name: None,
                        location: None,
                    },
                )]),
            }),
        }
    }

    fn as_json_value(document: &Document) -> serde_json::Value {
        let json = render(document, ExportFormat::Json).expect("JSON should render");
        serde_json::from_str(&json).expect("JSON should parse back")
    }

    /// The test the whole slice rests on: YAML output, read back, is exactly
    /// the document the JSON output describes. Parsed with the same library
    /// that wrote it, so a mistake shared by both halves would slip through —
    /// the manual check against an external validator is what covers that.
    #[test]
    fn yaml_reads_back_as_exactly_the_json_document() {
        let document = trap_document();

        let yaml = render(&document, ExportFormat::Yaml).expect("YAML should render");
        let from_yaml: serde_json::Value =
            serde_saphyr::from_str(&yaml).expect("YAML should parse back");

        assert_eq!(from_yaml, as_json_value(&document), "\n{yaml}");
    }

    /// The same, for a document shaped like a real export rather than a
    /// torture test: it would be a poor trade to pass the traps and fail on
    /// something ordinary.
    #[test]
    fn an_ordinary_document_reads_back_unchanged() {
        let document = Document {
            openapi: "3.2.0".into(),
            info: Info {
                title: "Work API".into(),
                version: "1.0.0".into(),
                summary: None,
                description: None,
            },
            servers: Vec::new(),
            paths: BTreeMap::new(),
            tags: Vec::new(),
            components: None,
        };

        let yaml = render(&document, ExportFormat::Yaml).expect("YAML should render");
        let from_yaml: serde_json::Value =
            serde_saphyr::from_str(&yaml).expect("YAML should parse back");

        assert_eq!(from_yaml, as_json_value(&document), "\n{yaml}");
        // `paths` is always written, and empty must not read as null.
        assert!(yaml.contains("paths: {}"), "{yaml}");
    }

    /// Readers expect the version first, and some tools sniff for it.
    #[test]
    fn yaml_opens_with_the_openapi_field() {
        let yaml = render(&trap_document(), ExportFormat::Yaml).expect("YAML should render");

        assert!(yaml.starts_with("openapi: "), "{yaml}");
    }

    /// A string that looks like a number must still be a string, in the
    /// three places the spec cares most about.
    #[test]
    fn version_and_status_codes_stay_strings() {
        let yaml = render(&trap_document(), ExportFormat::Yaml).expect("YAML should render");
        let value: serde_json::Value =
            serde_saphyr::from_str(&yaml).expect("YAML should parse back");

        assert_eq!(value["openapi"], serde_json::json!("3.0.0"));
        assert_eq!(value["info"]["version"], serde_json::json!("1.0"));
        let responses = &value["paths"]["/users/{userId}"]["get"]["responses"];
        assert!(responses.get("200").is_some(), "{responses}");
        assert!(responses.get("404").is_some(), "{responses}");
    }

    /// Adding YAML must not change a single byte of what JSON export has
    /// always written.
    #[test]
    fn json_output_is_what_the_exporter_always_wrote() {
        let document = trap_document();

        assert_eq!(
            render(&document, ExportFormat::Json).expect("JSON should render"),
            serde_json::to_string_pretty(&document).expect("JSON should render")
        );
    }

    /// No block scalars: they are where whitespace can change on the way back.
    #[test]
    fn multi_line_strings_are_written_quoted_not_as_block_scalars() {
        let yaml = render(&trap_document(), ExportFormat::Yaml).expect("YAML should render");

        for line in yaml.lines() {
            let trimmed = line.trim_end();
            assert!(
                !(trimmed.ends_with(": |")
                    || trimmed.ends_with(": >")
                    || trimmed.ends_with(": |-")
                    || trimmed.ends_with(": >-")
                    || trimmed.ends_with(": |+")
                    || trimmed.ends_with(": >+")),
                "block scalar in: {line}"
            );
        }
    }
}

// http_client/src-tauri/src/openapi/document.rs
//
// The OpenAPI document as serde structs. A wire format, so it lives here
// rather than in domain/models.rs — the same reason `Stored*` lives in
// persistence/repositories/json.rs. The domain does not deform to match a file
// spec.
//
// **One struct set serves 3.0, 3.1 and 3.2** (decided 2026-09-15, replacing
// PLAN.md's model-plus-one-emitter-per-version sketch). Checked against the
// official JSON Schema for each, the whole difference between what this
// exporter emits and a valid document is the Example Object's body field and
// the version string — everything else we produce already validates against
// all three. A near-identical struct set per version is the duplication §7
// forbids, and it would drift.
//
// **3.0 is nearly free only because no schemas are inferred yet.** Its real
// divergence is the Schema Object: Draft-4-flavoured, so `nullable` instead of
// a `null` type, boolean `exclusiveMinimum`, a single `example`, no `const`,
// no `contentEncoding`. We emit `{"type": "string"}` and nothing else, which
// is valid in all three. Phase 8b is where that stops being true, and
// `a_schema_is_still_only_a_type` below is the tripwire that will say so.
//
// Only the subset this exporter emits is modelled. Import (Phase 8d) will need
// more — and `Deserialize`, which is deliberately not derived yet — and the
// missing pieces are named in PLAN.md rather than stubbed here.
//
// Every optional field is `skip_serializing_if`: OpenAPI treats an absent
// field and a null one differently, and tooling downstream is not always
// forgiving.
use std::collections::BTreeMap;

use serde::Serialize;

/// Which OpenAPI version a document is being emitted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenApiVersion {
    V3_0,
    V3_1,
    V3_2,
}

impl OpenApiVersion {
    /// What goes in the document's `openapi` field.
    ///
    /// The `.0` patch in each case, not the newest one: the spec says the
    /// major.minor pair designates the feature set and patch releases only
    /// correct the prose, so `3.1.0` claims exactly what we support while
    /// staying acceptable to older validators that match the string exactly.
    /// The official schemas accept `^3\.1\.\d+(-.+)?$` and `^3\.0\.\d(-.+)?$`
    /// respectively, so a newer patch would validate — it would just claim
    /// conformance to prose we have not read.
    pub fn openapi_field(self) -> &'static str {
        match self {
            Self::V3_0 => "3.0.0",
            Self::V3_1 => "3.1.0",
            Self::V3_2 => "3.2.0",
        }
    }

    /// How the frontend names it. Parsed here rather than in the command so
    /// the accepted set lives next to the versions themselves.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "3.0" => Some(Self::V3_0),
            "3.1" => Some(Self::V3_1),
            "3.2" => Some(Self::V3_2),
            _ => None,
        }
    }

    /// For file names and messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::V3_0 => "3.0",
            Self::V3_1 => "3.1",
            Self::V3_2 => "3.2",
        }
    }

    /// Whether the Example Object has 3.2's `serializedValue`.
    ///
    /// It does not, before 3.2 — and both earlier schemas close the object
    /// (`additionalProperties: false` in 3.0, `unevaluatedProperties: false`
    /// in 3.1), so emitting it there invalidates the document rather than
    /// being quietly ignored.
    ///
    /// Matched exhaustively on purpose: a version added later must make this
    /// decision rather than inherit whichever answer a wildcard gave it.
    pub fn has_serialized_value(self) -> bool {
        match self {
            Self::V3_0 | Self::V3_1 => false,
            Self::V3_2 => true,
        }
    }

    /// Whether a Schema Object can say `"null"` in its `type`.
    ///
    /// 3.1 aligned the Schema Object with JSON Schema 2020-12, where `null` is
    /// a type like any other and `type` may be a list. 3.0's is
    /// Draft-4-flavoured: one type, and the separate `nullable` keyword.
    /// This is the only place inference has to know which version it is
    /// writing for.
    pub fn has_null_type(self) -> bool {
        match self {
            Self::V3_0 => false,
            Self::V3_1 | Self::V3_2 => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub openapi: String,
    pub info: Info,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
    /// BTreeMap, not HashMap: a document that reorders itself between exports
    /// produces a meaningless diff in review.
    ///
    /// Always serialised, empty or not. `paths` became optional in 3.1, but a
    /// document that omits it reads as "this describes something other than an
    /// API" rather than "this collection exported nothing".
    pub paths: BTreeMap<String, PathItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 3.2 only. The 3.1 and 3.0 emitters drop this and say so.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, ServerVariable>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerVariable {
    /// REQUIRED by the spec. Deliberately a placeholder, never the value from
    /// the active environment — see the export service.
    pub default: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    pub responses: BTreeMap<String, Response>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<BTreeMap<String, Vec<String>>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub name: String,
    /// `in` is a Rust keyword, so the field carries a name the language allows
    /// and serde puts the spec's name on the wire.
    #[serde(rename = "in")]
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    /// The value the user had typed, as documentation. Never a credential —
    /// auth-bearing parameters are filtered out before this point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content: BTreeMap<String, MediaType>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub examples: BTreeMap<String, Example>,
}

/// The body of an example, in whichever field the target version has.
///
/// 3.2 added `serializedValue` for exactly our situation: a saved response
/// body is an already-encoded string, while `value` is defined as the decoded
/// representation. 3.1 has no such field — its schema sets
/// `unevaluatedProperties: false`, so emitting `serializedValue` there does
/// not merely get ignored, it makes the document invalid — and the body has
/// to go in `value` decoded as best we can.
#[derive(Debug, Clone, PartialEq)]
pub enum ExampleBody {
    /// 3.2. The bytes as they went over the wire.
    Serialized(String),
    /// 3.1 and below. Decoded: a parsed document for JSON, the string itself
    /// for anything whose decoded form *is* a string.
    Decoded(serde_json::Value),
}

/// `serialized_value` and `value` are mutually exclusive and only `new` sets
/// them, so the pair cannot both be populated by accident — the same shape
/// the 3.1 schema enforces between `value` and `externalValue`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// 3.2 only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serialized_value: Option<String>,
    /// 3.1 and below.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

impl Example {
    /// `body` is optional because a saved response can have an empty one, and
    /// an example that says only "200 OK" is still worth emitting.
    pub fn new(summary: Option<String>, body: Option<ExampleBody>) -> Self {
        match body {
            Some(ExampleBody::Serialized(text)) => Self {
                summary,
                serialized_value: Some(text),
                value: None,
            },
            Some(ExampleBody::Decoded(value)) => Self {
                summary,
                serialized_value: None,
                value: Some(value),
            },
            None => Self {
                summary,
                serialized_value: None,
                value: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    /// REQUIRED by the spec, even when there is nothing useful to say.
    pub description: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub content: BTreeMap<String, MediaType>,
}

/// 3.1 allows `type` to be a list (`["string", "null"]`); 3.0 allows only a
/// single value. Untagged, so a `One` writes a bare string and a `Many` writes
/// a bare array — the spec has no wrapper around either.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SchemaType {
    One(String),
    Many(Vec<String>),
}

/// The subset of the Schema Object this exporter can honestly fill in from
/// observed bodies. Everything absent is absent because we do not know it, not
/// because it is unsupported: no `format` (a string that looks like a
/// timestamp is not necessarily one), no `minimum`, no `enum`.
///
/// `nullable` is 3.0-only and `type: [..., "null"]` is 3.1-and-later; which
/// one gets set is decided in infer.rs, never here.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<SchemaType>,
    /// 3.0 only. 3.1 and later put `"null"` in `type` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, Schema>,
    /// Sorted, because a document that reorders itself between exports
    /// produces a meaningless diff.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    /// Boxed: a Schema contains Schemas, and the compiler wants a size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    /// Where one position held genuinely different kinds of value across
    /// samples. Valid in 3.0, 3.1 and 3.2 alike, which is why it is preferred
    /// to 3.1's type lists for expressing a conflict.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub one_of: Vec<Schema>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Components {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub security_schemes: BTreeMap<String, SecurityScheme>,
}

/// Describes *that* an operation authenticates and how, never the credential.
/// An exported document gets committed to git.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityScheme {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `in` and `type` are Rust keywords, so both fields are renamed. A typo in
    /// either attribute produces a document that validates as nothing, and no
    /// other test in this crate would notice.
    #[test]
    fn keyword_fields_are_renamed_on_the_wire() {
        let parameter = Parameter {
            name: "userId".into(),
            location: "path".into(),
            required: Some(true),
            schema: Some(Schema {
                kind: Some(SchemaType::One("string".into())),
                ..Schema::default()
            }),
            example: None,
        };

        let json = serde_json::to_string(&parameter).expect("parameter should serialise");

        assert!(json.contains(r#""in":"path""#), "{json}");
        assert!(json.contains(r#""type":"string""#), "{json}");
        assert!(!json.contains("location"), "{json}");
        assert!(!json.contains("kind"), "{json}");
    }

    #[test]
    fn a_security_scheme_renames_both_of_its_keywords() {
        let scheme = SecurityScheme {
            kind: "apiKey".into(),
            description: None,
            scheme: None,
            name: Some("X-Api-Key".into()),
            location: Some("header".into()),
        };

        let json = serde_json::to_string(&scheme).expect("scheme should serialise");

        assert_eq!(
            json, r#"{"type":"apiKey","name":"X-Api-Key","in":"header"}"#,
            "absent fields must be omitted, not emitted as null"
        );
    }

    /// An absent field and a null one mean different things to a validator.
    #[test]
    fn empty_optionals_are_omitted_rather_than_written_as_null() {
        let document = Document {
            openapi: OpenApiVersion::V3_2.openapi_field().into(),
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

        let json = serde_json::to_string(&document).expect("document should serialise");

        assert_eq!(
            json,
            r#"{"openapi":"3.2.0","info":{"title":"Work API","version":"1.0.0"},"paths":{}}"#
        );
    }

    #[test]
    fn camel_case_reaches_the_multi_word_fields() {
        let example = Example::new(None, Some(ExampleBody::Serialized("{}".into())));

        let json = serde_json::to_string(&example).expect("example should serialise");

        assert_eq!(json, r#"{"serializedValue":"{}"}"#);
    }

    /// The 3.1 schema sets `unevaluatedProperties: false` on the Example
    /// Object, so a stray `serializedValue` does not get ignored there — it
    /// makes the whole document invalid. This is the test that catches it.
    #[test]
    fn a_decoded_body_writes_value_and_never_serialized_value() {
        let example = Example::new(
            Some("200 OK".into()),
            Some(ExampleBody::Decoded(serde_json::json!({ "id": 7 }))),
        );

        let json = serde_json::to_string(&example).expect("example should serialise");

        assert_eq!(json, r#"{"summary":"200 OK","value":{"id":7}}"#);
        assert!(!json.contains("serializedValue"), "{json}");
    }

    /// One field or the other, never both — the same exclusivity the 3.1
    /// schema enforces between `value` and `externalValue`.
    #[test]
    fn the_two_body_fields_are_mutually_exclusive() {
        let serialized = Example::new(None, Some(ExampleBody::Serialized("x".into())));
        let decoded = Example::new(None, Some(ExampleBody::Decoded(serde_json::Value::Null)));

        assert!(serialized.value.is_none());
        assert!(decoded.serialized_value.is_none());
    }

    #[test]
    fn each_version_names_itself_the_way_the_spec_requires() {
        assert_eq!(OpenApiVersion::V3_0.openapi_field(), "3.0.0");
        assert_eq!(OpenApiVersion::V3_1.openapi_field(), "3.1.0");
        assert_eq!(OpenApiVersion::V3_2.openapi_field(), "3.2.0");
        assert_eq!(OpenApiVersion::from_wire("3.0"), Some(OpenApiVersion::V3_0));
        assert_eq!(OpenApiVersion::from_wire("3.1"), Some(OpenApiVersion::V3_1));
        assert_eq!(OpenApiVersion::from_wire("3.2"), Some(OpenApiVersion::V3_2));
        // Anything else is the frontend and the backend disagreeing, which is
        // a bug to surface rather than a default to guess at.
        assert_eq!(OpenApiVersion::from_wire("2.0"), None);
        assert_eq!(OpenApiVersion::from_wire("3"), None);
        assert_eq!(OpenApiVersion::from_wire(""), None);
    }

    #[test]
    fn only_3_2_carries_a_serialized_value() {
        assert!(!OpenApiVersion::V3_0.has_serialized_value());
        assert!(!OpenApiVersion::V3_1.has_serialized_value());
        assert!(OpenApiVersion::V3_2.has_serialized_value());
    }

    #[test]
    fn a_single_type_writes_a_bare_string_and_a_list_writes_an_array() {
        let one = Schema {
            kind: Some(SchemaType::One("string".into())),
            ..Schema::default()
        };
        let many = Schema {
            kind: Some(SchemaType::Many(vec!["string".into(), "null".into()])),
            ..Schema::default()
        };

        assert_eq!(
            serde_json::to_string(&one).expect("should serialise"),
            r#"{"type":"string"}"#
        );
        assert_eq!(
            serde_json::to_string(&many).expect("should serialise"),
            r#"{"type":["string","null"]}"#
        );
    }

    /// An empty `properties` or `required` is not the same as an absent one to
    /// a strict reader, and a Schema with nothing known should serialise to
    /// `{}` rather than to a pile of empty containers.
    #[test]
    fn an_empty_schema_writes_nothing_at_all() {
        let json = serde_json::to_string(&Schema::default()).expect("should serialise");

        assert_eq!(json, "{}");
    }

    #[test]
    fn nested_schemas_keep_their_spec_spelling() {
        let schema = Schema {
            kind: Some(SchemaType::One("array".into())),
            items: Some(Box::new(Schema {
                kind: Some(SchemaType::One("object".into())),
                properties: BTreeMap::from([(
                    "id".to_string(),
                    Schema {
                        kind: Some(SchemaType::One("integer".into())),
                        ..Schema::default()
                    },
                )]),
                required: vec!["id".to_string()],
                ..Schema::default()
            })),
            ..Schema::default()
        };

        let json = serde_json::to_string(&schema).expect("should serialise");

        assert_eq!(
            json,
            r#"{"type":"array","items":{"type":"object","properties":{"id":{"type":"integer"}},"required":["id"]}}"#
        );
    }

    /// `one_of` is two words in Rust and one on the wire.
    #[test]
    fn a_conflict_writes_one_of_in_camel_case() {
        let schema = Schema {
            one_of: vec![
                Schema {
                    kind: Some(SchemaType::One("integer".into())),
                    ..Schema::default()
                },
                Schema {
                    kind: Some(SchemaType::One("string".into())),
                    ..Schema::default()
                },
            ],
            ..Schema::default()
        };

        let json = serde_json::to_string(&schema).expect("should serialise");

        assert_eq!(json, r#"{"oneOf":[{"type":"integer"},{"type":"string"}]}"#);
    }
}

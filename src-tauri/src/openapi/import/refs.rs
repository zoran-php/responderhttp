// http_client/src-tauri/src/openapi/import/refs.rs
//
// `$ref` handling. Only references inside the file are followed: the import
// never reads another file and never goes to the network, so a reference to
// either refuses the whole import with a message that names it.
//
// The whole-document check skips positions that hold data rather than
// description — `example`, `default`, `const`, `enum`, an Example's `value`
// and `x-` extensions — because a key called `$ref` inside an example body is
// just part of the example.
use std::collections::BTreeSet;

use serde_json::Value;

use crate::openapi::import::Refusal;

const REF: &str = "$ref";
/// How many `$ref` hops one lookup may take before it counts as a loop.
pub const MAX_REF_HOPS: usize = 32;
/// How many references a refusal lists.
const MAX_LISTED_REFS: usize = 20;

/// Subtrees that are values, not OpenAPI objects.
const DATA_KEYS: [&str; 6] = ["example", "default", "const", "enum", "value", "dataValue"];

/// Objects whose keys are names the author chose, not keywords: a schema
/// property called `default`, a response keyed `default`, a component called
/// `example` are all ordinary entries there.
const NAME_MAPS: [&str; 21] = [
    "properties",
    "patternProperties",
    "dependentSchemas",
    "$defs",
    "definitions",
    "schemas",
    "responses",
    "parameters",
    "examples",
    "requestBodies",
    "headers",
    "securitySchemes",
    "links",
    "callbacks",
    "pathItems",
    "mediaTypes",
    "content",
    "encoding",
    "variables",
    "paths",
    "webhooks",
];

/// Every reference the document makes must be internal and must resolve.
pub fn check_references(root: &Value) -> Result<(), Refusal> {
    let mut external = BTreeSet::new();
    let mut broken = BTreeSet::new();
    visit(root, root, None, &mut external, &mut broken);

    if !external.is_empty() {
        return Err(Refusal::ExternalReferences {
            refs: external.into_iter().take(MAX_LISTED_REFS).collect(),
        });
    }
    if !broken.is_empty() {
        return Err(Refusal::BrokenReferences {
            refs: broken.into_iter().take(MAX_LISTED_REFS).collect(),
        });
    }
    Ok(())
}

fn visit(
    root: &Value,
    value: &Value,
    parent_key: Option<&str>,
    external: &mut BTreeSet<String>,
    broken: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(reference)) = object.get(REF) {
                classify(root, reference, external, broken);
            }
            let keys_are_names = parent_key.is_some_and(|parent| NAME_MAPS.contains(&parent));
            for (key, child) in object {
                if !keys_are_names && (DATA_KEYS.contains(&key.as_str()) || key.starts_with("x-")) {
                    continue;
                }
                // A Schema Object's `examples` is a list of values; the
                // `examples` map elsewhere holds Example Objects, which can
                // themselves be references and are walked.
                if key == "examples" && child.is_array() {
                    continue;
                }
                visit(root, child, Some(key), external, broken);
            }
        }
        Value::Array(items) => {
            for item in items {
                visit(root, item, None, external, broken);
            }
        }
        _ => {}
    }
}

fn classify(
    root: &Value,
    reference: &str,
    external: &mut BTreeSet<String>,
    broken: &mut BTreeSet<String>,
) {
    match reference.strip_prefix('#') {
        None => {
            external.insert(reference.to_string());
        }
        // `#name` is a JSON Schema anchor, resolved by the schema's own
        // rules; the importer never needs to follow one.
        Some(fragment) if !fragment.is_empty() && !fragment.starts_with('/') => {}
        Some(_) => {
            if follow(root, reference).is_none() {
                broken.insert(reference.to_string());
            }
        }
    }
}

/// The value a reference points at, after following any chain of
/// references. None if it does not resolve or loops.
fn follow<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let mut current = reference.to_string();
    for _ in 0..MAX_REF_HOPS {
        let target = lookup(root, &current)?;
        match target.get(REF).and_then(Value::as_str) {
            Some(next) => current = next.to_string(),
            None => return Some(target),
        }
    }
    None
}

/// `#/a/b~1c` → `root["a"]["b/c"]`. `#` alone is the document.
fn lookup<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let fragment = reference.strip_prefix('#')?;
    let decoded = percent_decode(fragment)?;
    if decoded.is_empty() {
        return Some(root);
    }
    if !decoded.starts_with('/') {
        return None;
    }
    root.pointer(&decoded)
}

/// URI fragments may percent-encode; JSON pointers are then unescaped by
/// `Value::pointer` (`~1`, `~0`).
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Follows references for the mapper. The document has already passed
/// `check_references`, so a failure here is not expected; it still returns
/// None rather than panicking, and the caller skips what it cannot read.
#[derive(Clone, Copy)]
pub struct Refs<'a> {
    root: &'a Value,
}

impl<'a> Refs<'a> {
    pub fn new(root: &'a Value) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &'a Value {
        self.root
    }

    /// The object itself, or what its `$ref` chain points at.
    pub fn resolve(&self, value: &'a Value) -> Option<&'a Value> {
        match value.get(REF).and_then(Value::as_str) {
            Some(reference) => follow(self.root, reference),
            None => Some(value),
        }
    }

    /// The reference a value makes, if it is one. Used to guard recursion in
    /// schemas.
    pub fn reference_of(value: &Value) -> Option<&str> {
        value.get(REF).and_then(Value::as_str)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn internal_references_resolve_through_chains() {
        let root = json!({
            "components": {
                "parameters": {
                    "a": {"$ref": "#/components/parameters/b"},
                    "b": {"name": "id", "in": "query"}
                }
            },
            "x": {"$ref": "#/components/parameters/a"}
        });
        let refs = Refs::new(&root);

        assert_eq!(check_references(&root), Ok(()));
        assert_eq!(refs.resolve(&root["x"]).unwrap()["name"], "id");
    }

    #[test]
    fn pointer_escapes_and_percent_encoding_are_honoured() {
        let root = json!({
            "paths": {"/a/{id}": {"get": {"summary": "s"}}},
            "components": {"schemas": {"a b~c": {"type": "string"}}},
            "r1": {"$ref": "#/paths/~1a~1%7Bid%7D/get"},
            "r2": {"$ref": "#/components/schemas/a%20b~0c"}
        });
        let refs = Refs::new(&root);

        assert_eq!(check_references(&root), Ok(()));
        assert_eq!(refs.resolve(&root["r1"]).unwrap()["summary"], "s");
        assert_eq!(refs.resolve(&root["r2"]).unwrap()["type"], "string");
    }

    #[test]
    fn a_missing_target_is_broken() {
        let root = json!({"x": {"$ref": "#/components/schemas/Nope"}});

        assert_eq!(
            check_references(&root),
            Err(Refusal::BrokenReferences {
                refs: vec!["#/components/schemas/Nope".into()]
            })
        );
    }

    #[test]
    fn a_reference_loop_is_broken() {
        let root = json!({
            "a": {"$ref": "#/b"},
            "b": {"$ref": "#/a"}
        });

        assert!(matches!(
            check_references(&root),
            Err(Refusal::BrokenReferences { .. })
        ));
        assert!(Refs::new(&root).resolve(&root["a"]).is_none());
    }

    #[test]
    fn a_recursive_schema_is_fine() {
        let root = json!({
            "components": {"schemas": {"Node": {
                "type": "object",
                "properties": {"children": {"type": "array", "items": {"$ref": "#/components/schemas/Node"}}}
            }}}
        });

        assert_eq!(check_references(&root), Ok(()));
    }

    #[test]
    fn files_and_urls_are_external_and_listed() {
        let root = json!({
            "a": {"$ref": "./common.yaml#/components/schemas/Error"},
            "b": {"$ref": "https://example.com/schemas/user.json"},
            "c": {"$ref": "#/a"}
        });

        assert_eq!(
            check_references(&root),
            Err(Refusal::ExternalReferences {
                refs: vec![
                    "./common.yaml#/components/schemas/Error".into(),
                    "https://example.com/schemas/user.json".into()
                ]
            })
        );
    }

    #[test]
    fn data_positions_are_not_references() {
        let root = json!({
            "example": {"$ref": "./not-a-ref.json"},
            "schema": {
                "default": {"$ref": "nope"},
                "examples": [{"$ref": "nope"}],
                "enum": [{"$ref": "nope"}],
                "x-vendor": {"$ref": "nope"}
            },
            "examples": {"one": {"value": {"$ref": "nope"}}}
        });

        assert_eq!(check_references(&root), Ok(()));
    }

    #[test]
    fn example_objects_in_an_examples_map_are_still_checked() {
        let root = json!({"examples": {"one": {"$ref": "#/components/examples/missing"}}});

        assert!(matches!(
            check_references(&root),
            Err(Refusal::BrokenReferences { .. })
        ));
    }

    #[test]
    fn a_default_response_is_an_object_not_a_value() {
        let root = json!({"responses": {"default": {"$ref": "#/components/responses/missing"}}});

        assert!(matches!(
            check_references(&root),
            Err(Refusal::BrokenReferences { .. })
        ));
    }

    #[test]
    fn a_property_named_like_a_data_keyword_is_still_a_schema() {
        let root = json!({"schema": {"properties": {
            "value": {"$ref": "#/missing/value"},
            "default": {"$ref": "#/missing/default"}
        }}});

        let Err(Refusal::BrokenReferences { refs }) = check_references(&root) else {
            panic!("expected broken references");
        };
        assert_eq!(refs, vec!["#/missing/default", "#/missing/value"]);
    }

    #[test]
    fn anchors_are_left_to_json_schema() {
        let root = json!({"schema": {"$ref": "#node"}});

        assert_eq!(check_references(&root), Ok(()));
    }

    #[test]
    fn the_list_is_capped() {
        let refs: serde_json::Map<String, Value> = (0..30)
            .map(|i| {
                (
                    format!("k{i}"),
                    json!({"$ref": format!("#/missing/{i:02}")}),
                )
            })
            .collect();

        let Err(Refusal::BrokenReferences { refs }) = check_references(&Value::Object(refs)) else {
            panic!("expected broken references");
        };
        assert_eq!(refs.len(), MAX_LISTED_REFS);
    }
}

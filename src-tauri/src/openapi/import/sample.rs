// http_client/src-tauri/src/openapi/import/sample.rs
//
// A sample value built from a Schema Object, for a request body the document
// describes but gives no example for (PLAN.md Phase 8d, decision D3).
//
// The author's own values win at every level: `example`, then the first of
// `examples`, then `default`, `const` and the first `enum` entry. Only when a
// schema offers none of those does a placeholder of the right type appear.
//
// Bounded three ways, because real schemas are large and often recursive: a
// `$ref` already being expanded is not expanded again, nesting stops at
// MAX_DEPTH, and the whole sample stops growing at MAX_NODES.
use serde_json::{Map, Value};

use crate::openapi::import::refs::Refs;

const MAX_DEPTH: usize = 8;
const MAX_NODES: usize = 1_000;

/// A sample for `schema`, or None when there is nothing sensible to show —
/// a schema that is only a loop back to itself, for instance.
pub fn sample(refs: Refs<'_>, schema: &Value) -> Option<Value> {
    let mut builder = Builder {
        refs,
        stack: Vec::new(),
        nodes: 0,
    };
    builder.build(schema, 0)
}

/// The value a schema itself suggests, without building anything.
pub fn declared_value(schema: &Value) -> Option<&Value> {
    schema
        .get("example")
        .or_else(|| schema.get("examples").and_then(|list| list.get(0)))
        .or_else(|| schema.get("default"))
        .or_else(|| schema.get("const"))
        .or_else(|| schema.get("enum").and_then(|list| list.get(0)))
}

struct Builder<'a> {
    refs: Refs<'a>,
    /// References currently being expanded, innermost last.
    stack: Vec<&'a str>,
    nodes: usize,
}

impl<'a> Builder<'a> {
    fn build(&mut self, schema: &'a Value, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH || self.nodes >= MAX_NODES {
            return None;
        }
        if let Some(reference) = Refs::reference_of(schema) {
            if self.stack.contains(&reference) {
                return None;
            }
            let target = self.refs.resolve(schema)?;
            self.stack.push(reference);
            let built = self.build(target, depth);
            self.stack.pop();
            return built;
        }
        let Value::Object(object) = schema else {
            // `true` and `{}` accept anything; there is nothing to suggest.
            return None;
        };
        self.nodes += 1;

        if let Some(value) = declared_value(schema) {
            return Some(value.clone());
        }
        if let Some(parts) = object.get("allOf").and_then(Value::as_array) {
            return self.all_of(object, parts, depth);
        }
        for alternatives in ["oneOf", "anyOf"] {
            if let Some(first) = object
                .get(alternatives)
                .and_then(Value::as_array)
                .and_then(|list| list.first())
            {
                return self.build(first, depth + 1);
            }
        }

        match schema_type(object) {
            Some("object") => Some(self.object(object, depth)),
            Some("array") => Some(self.array(object, depth)),
            Some("string") => Some(Value::String(string_placeholder(object))),
            Some("integer") | Some("number") => Some(Value::from(0)),
            Some("boolean") => Some(Value::Bool(true)),
            Some("null") => Some(Value::Null),
            _ if object.contains_key("properties") => Some(self.object(object, depth)),
            _ if object.contains_key("items") => Some(self.array(object, depth)),
            _ => None,
        }
    }

    fn object(&mut self, object: &'a Map<String, Value>, depth: usize) -> Value {
        let mut out = Map::new();
        if let Some(Value::Object(properties)) = object.get("properties") {
            for (name, property) in properties {
                if is_read_only(self.refs, property) {
                    continue;
                }
                if let Some(value) = self.build(property, depth + 1) {
                    out.insert(name.clone(), value);
                }
            }
        }
        Value::Object(out)
    }

    fn array(&mut self, object: &'a Map<String, Value>, depth: usize) -> Value {
        let item = object
            .get("items")
            .and_then(|items| self.build(items, depth + 1));
        Value::Array(item.into_iter().collect())
    }

    /// The parts merged into one object; a part that is not an object (a
    /// string with a format, say) stands for the whole.
    fn all_of(
        &mut self,
        object: &'a Map<String, Value>,
        parts: &'a [Value],
        depth: usize,
    ) -> Option<Value> {
        let mut merged = Map::new();
        for part in parts {
            match self.build(part, depth + 1) {
                Some(Value::Object(fields)) => merged.extend(fields),
                Some(other) => return Some(other),
                None => {}
            }
        }
        if let Value::Object(own) = self.object(object, depth) {
            merged.extend(own);
        }
        Some(Value::Object(merged))
    }
}

/// `type` is a string in 3.0 and may be a list in 3.1+; `["string", "null"]`
/// is a string that may be null, so the first non-null entry wins.
fn schema_type(object: &Map<String, Value>) -> Option<&str> {
    match object.get("type")? {
        Value::String(kind) => Some(kind),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .find(|kind| *kind != "null")
            .or(Some("null")),
        _ => None,
    }
}

/// A server fills these in; a client sending them is at best ignored.
fn is_read_only(refs: Refs<'_>, property: &Value) -> bool {
    refs.resolve(property)
        .and_then(|resolved| resolved.get("readOnly"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn string_placeholder(object: &Map<String, Value>) -> String {
    let format = object.get("format").and_then(Value::as_str).unwrap_or("");
    match format {
        "date-time" => "2024-01-01T00:00:00Z",
        "date" => "2024-01-01",
        "time" => "00:00:00Z",
        "email" => "user@example.com",
        "uuid" => "00000000-0000-0000-0000-000000000000",
        "uri" | "url" => "https://example.com",
        "hostname" => "example.com",
        "ipv4" => "192.0.2.1",
        "ipv6" => "2001:db8::1",
        "byte" => "",
        _ => "string",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_of(root: &Value, schema: &Value) -> Option<Value> {
        sample(Refs::new(root), schema)
    }

    #[test]
    fn the_authors_values_win_at_every_level() {
        let root = json!({});
        let schema = json!({
            "type": "object",
            "properties": {
                "a": {"type": "string", "example": "from example"},
                "b": {"type": "integer", "default": 7},
                "c": {"type": "string", "enum": ["first", "second"]},
                "d": {"type": "string", "examples": ["listed"]},
                "e": {"const": "fixed"}
            }
        });

        assert_eq!(
            sample_of(&root, &schema),
            Some(json!({"a": "from example", "b": 7, "c": "first", "d": "listed", "e": "fixed"}))
        );
    }

    #[test]
    fn a_whole_object_example_is_used_as_is() {
        let schema = json!({"type": "object", "example": {"id": 1}, "properties": {"x": {"type": "string"}}});

        assert_eq!(sample_of(&json!({}), &schema), Some(json!({"id": 1})));
    }

    #[test]
    fn placeholders_follow_type_and_format() {
        let schema = json!({
            "type": "object",
            "properties": {
                "when": {"type": "string", "format": "date-time"},
                "mail": {"type": "string", "format": "email"},
                "name": {"type": "string"},
                "count": {"type": "integer"},
                "ratio": {"type": "number"},
                "on": {"type": "boolean"},
                "tags": {"type": "array", "items": {"type": "string"}},
                "maybe": {"type": ["string", "null"]},
                "nothing": {"type": "null"}
            }
        });

        assert_eq!(
            sample_of(&json!({}), &schema),
            Some(json!({
                "when": "2024-01-01T00:00:00Z",
                "mail": "user@example.com",
                "name": "string",
                "count": 0,
                "ratio": 0,
                "on": true,
                "tags": ["string"],
                "maybe": "string",
                "nothing": null
            }))
        );
    }

    #[test]
    fn references_are_followed_and_read_only_fields_left_out() {
        let root = json!({"components": {"schemas": {
            "Id": {"type": "integer", "readOnly": true},
            "Pet": {"type": "object", "properties": {
                "id": {"$ref": "#/components/schemas/Id"},
                "name": {"type": "string", "example": "Rex"}
            }}
        }}});

        assert_eq!(
            sample_of(&root, &json!({"$ref": "#/components/schemas/Pet"})),
            Some(json!({"name": "Rex"}))
        );
    }

    #[test]
    fn recursion_stops_where_the_schema_repeats() {
        let root = json!({"components": {"schemas": {"Node": {
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "children": {"type": "array", "items": {"$ref": "#/components/schemas/Node"}}
            }
        }}}});

        assert_eq!(
            sample_of(&root, &json!({"$ref": "#/components/schemas/Node"})),
            Some(json!({"name": "string", "children": []}))
        );
    }

    #[test]
    fn all_of_merges_and_one_of_takes_the_first() {
        let root = json!({"components": {"schemas": {
            "Base": {"type": "object", "properties": {"id": {"type": "integer"}}}
        }}});
        let schema = json!({
            "allOf": [
                {"$ref": "#/components/schemas/Base"},
                {"type": "object", "properties": {"kind": {"oneOf": [
                    {"type": "string", "enum": ["cat"]},
                    {"type": "integer"}
                ]}}}
            ]
        });

        assert_eq!(
            sample_of(&root, &schema),
            Some(json!({"id": 0, "kind": "cat"}))
        );
    }

    #[test]
    fn depth_is_capped() {
        let mut schema = json!({"type": "string"});
        for _ in 0..20 {
            schema = json!({"type": "object", "properties": {"n": schema}});
        }

        let built = sample_of(&json!({}), &schema).expect("a sample");
        let depth = std::iter::successors(Some(&built), |value| value.get("n")).count();
        assert!(depth <= MAX_DEPTH + 1, "{depth}");
    }

    #[test]
    fn size_is_capped() {
        let wide: Map<String, Value> = (0..5_000)
            .map(|i| (format!("p{i}"), json!({"type": "string"})))
            .collect();
        let schema = json!({"type": "object", "properties": wide});

        let built = sample_of(&json!({}), &schema).expect("a sample");
        assert!(built.as_object().unwrap().len() < MAX_NODES);
    }

    #[test]
    fn an_empty_schema_suggests_nothing() {
        assert_eq!(sample_of(&json!({}), &json!({})), None);
        assert_eq!(sample_of(&json!({}), &json!(true)), None);
    }
}

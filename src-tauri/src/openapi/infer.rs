// http_client/src-tauri/src/openapi/infer.rs
//
// JSON body -> Schema Object. Pure: it is handed already-parsed values and
// returns a Schema, so it can be tested exhaustively without a filesystem, a
// database or a network (CLAUDE.md §8).
//
// The shape of this module is set by one fact: **an array has exactly one
// `items` schema**, however many differently-shaped elements it holds. So
// inference cannot be a straight translation of one value — it has to
// accumulate observations and then describe what they had in common. The same
// accumulator then serves the other half of the job, merging every saved
// response for one status code into a single schema.
//
// Rules, decided 2026-09-15:
//
// - **Full recursion.** No depth cap, and none is needed: `serde_json` refuses
//   to parse beyond 128 levels of nesting, and `from_collection::decode`
//   already falls back to treating an unparseable body as text. The bound
//   exists, upstream, where a malformed document is still cheap to reject.
// - **`required` is intersection, not union.** A key is required only if every
//   object observed at that position carried it. One saved response proves a
//   key was present that once; two responses that disagree prove it optional.
// - **Conflicting types become `oneOf`**, in every version. 3.1 could write
//   `type: ["integer", "string"]` and 3.0 could not, but `oneOf` is valid in
//   all three and says the same thing, so there is no version-specific union
//   logic to get wrong. Nullability is the one place the versions still differ.
// - **No format guessing.** A string that looks like a timestamp is not
//   necessarily one, and a wrong `format` is worse than an absent one — client
//   generators turn it into a type.
use std::collections::{BTreeMap, BTreeSet};

use crate::openapi::document::{OpenApiVersion, Schema, SchemaType};

/// Everything seen at one position across every sample.
///
/// Flags rather than an enum of "the" type: a position can legitimately have
/// held a string in one response and a number in another, and flattening that
/// to a single guess is how an exporter starts lying.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observed {
    null: bool,
    boolean: bool,
    integer: bool,
    /// A non-integral number. Widens `integer` to `number` when both appear.
    float: bool,
    string: bool,
    object: Option<ObjectShape>,
    /// Every array seen here contributes its elements to one item observation,
    /// which is what "array unification" means.
    array: Option<Box<Observed>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct ObjectShape {
    /// How many objects were observed here. The denominator for `required`.
    samples: usize,
    /// Field name -> (how many of those objects had it, what was in it).
    fields: BTreeMap<String, (usize, Observed)>,
}

impl Observed {
    /// Folds one more value in. Call once per sample; the accumulator is the
    /// merge.
    pub fn observe(&mut self, value: &serde_json::Value) {
        match value {
            serde_json::Value::Null => self.null = true,
            serde_json::Value::Bool(_) => self.boolean = true,
            serde_json::Value::Number(number) => {
                if number.is_f64() {
                    self.float = true;
                } else {
                    self.integer = true;
                }
            }
            serde_json::Value::String(_) => self.string = true,
            serde_json::Value::Array(items) => {
                let observed = self.array.get_or_insert_with(Box::default);
                for item in items {
                    observed.observe(item);
                }
            }
            serde_json::Value::Object(map) => {
                let shape = self.object.get_or_insert_with(ObjectShape::default);
                shape.samples += 1;
                for (name, field) in map {
                    let entry = shape.fields.entry(name.clone()).or_default();
                    entry.0 += 1;
                    entry.1.observe(field);
                }
            }
        }
    }

    /// True when nothing was ever seen — an empty array's items, or a body
    /// that turned out to be nothing at all. Produces no schema rather than a
    /// schema that claims nothing is allowed.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn to_schema(&self, version: OpenApiVersion) -> Option<Schema> {
        if self.is_empty() {
            return None;
        }

        let variants = self.variants(version);
        let nullable = self.null;

        match variants.len() {
            // Only nulls were ever seen. 3.1 has a `null` type; 3.0 has to
            // settle for `nullable` and no type at all.
            0 => Some(match version.has_null_type() {
                true => Schema {
                    kind: Some(SchemaType::One("null".into())),
                    ..Schema::default()
                },
                false => Schema {
                    nullable: Some(true),
                    ..Schema::default()
                },
            }),
            1 => {
                let mut schema = variants.into_iter().next().unwrap_or_default();
                if nullable {
                    make_nullable(&mut schema, version);
                }
                Some(schema)
            }
            // A genuine conflict: this position held more than one kind of
            // value across the samples.
            _ => {
                let mut one_of = variants;
                let mut schema = Schema::default();
                if nullable {
                    if version.has_null_type() {
                        one_of.push(Schema {
                            kind: Some(SchemaType::One("null".into())),
                            ..Schema::default()
                        });
                    } else {
                        schema.nullable = Some(true);
                    }
                }
                schema.one_of = one_of;
                Some(schema)
            }
        }
    }

    /// One schema per non-null kind observed here.
    fn variants(&self, version: OpenApiVersion) -> Vec<Schema> {
        let mut variants = Vec::new();
        if self.boolean {
            variants.push(primitive("boolean"));
        }
        // `integer` widens to `number` the moment a non-integral value shows
        // up: 1 and 1.5 in the same position are one numeric field, not a
        // conflict between two types.
        if self.integer || self.float {
            variants.push(primitive(if self.float { "number" } else { "integer" }));
        }
        if self.string {
            variants.push(primitive("string"));
        }
        if let Some(shape) = &self.object {
            variants.push(shape.to_schema(version));
        }
        if let Some(items) = &self.array {
            variants.push(Schema {
                kind: Some(SchemaType::One("array".into())),
                // An empty array tells us it is an array and nothing more.
                items: items.to_schema(version).map(Box::new),
                ..Schema::default()
            });
        }
        variants
    }
}

impl ObjectShape {
    fn to_schema(&self, version: OpenApiVersion) -> Schema {
        let mut properties = BTreeMap::new();
        let mut required = BTreeSet::new();
        for (name, (present, observed)) in &self.fields {
            if let Some(schema) = observed.to_schema(version) {
                properties.insert(name.clone(), schema);
            }
            // Intersection: present in every object observed here, not in
            // merely one of them.
            if *present == self.samples {
                required.insert(name.clone());
            }
        }
        Schema {
            kind: Some(SchemaType::One("object".into())),
            properties,
            required: required.into_iter().collect(),
            ..Schema::default()
        }
    }
}

fn primitive(kind: &str) -> Schema {
    Schema {
        kind: Some(SchemaType::One(kind.to_string())),
        ..Schema::default()
    }
}

/// 3.1 says "or null" by adding it to the type list. 3.0 has no `null` type
/// and uses the `nullable` keyword, which is why this is the one part of
/// inference that has to know which version it is writing for.
fn make_nullable(schema: &mut Schema, version: OpenApiVersion) {
    if !version.has_null_type() {
        schema.nullable = Some(true);
        return;
    }
    schema.kind = match schema.kind.take() {
        Some(SchemaType::One(kind)) => Some(SchemaType::Many(vec![kind, "null".into()])),
        Some(SchemaType::Many(mut kinds)) => {
            kinds.push("null".into());
            Some(SchemaType::Many(kinds))
        }
        None => Some(SchemaType::One("null".into())),
    };
}

/// Convenience for the common case: one body, one schema.
pub fn infer(value: &serde_json::Value, version: OpenApiVersion) -> Option<Schema> {
    let mut observed = Observed::default();
    observed.observe(value);
    observed.to_schema(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema_of(values: &[serde_json::Value], version: OpenApiVersion) -> Schema {
        let mut observed = Observed::default();
        for value in values {
            observed.observe(value);
        }
        observed.to_schema(version).expect("something was observed")
    }

    fn json_of(values: &[serde_json::Value], version: OpenApiVersion) -> String {
        serde_json::to_string(&schema_of(values, version)).expect("should serialise")
    }

    #[test]
    fn primitives_get_their_json_schema_type() {
        let v = OpenApiVersion::V3_1;

        assert_eq!(json_of(&[json!("x")], v), r#"{"type":"string"}"#);
        assert_eq!(json_of(&[json!(true)], v), r#"{"type":"boolean"}"#);
        assert_eq!(json_of(&[json!(7)], v), r#"{"type":"integer"}"#);
        assert_eq!(json_of(&[json!(7.5)], v), r#"{"type":"number"}"#);
    }

    /// 1 and 1.5 in the same position are one numeric field, not two types in
    /// conflict.
    #[test]
    fn an_integer_widens_to_number_rather_than_conflicting_with_one() {
        let json = json_of(&[json!(1), json!(1.5)], OpenApiVersion::V3_1);

        assert_eq!(json, r#"{"type":"number"}"#);
    }

    #[test]
    fn an_object_recurses_all_the_way_down() {
        let json = json_of(
            &[json!({ "user": { "id": 42, "roles": ["admin"] } })],
            OpenApiVersion::V3_1,
        );

        assert_eq!(
            json,
            r#"{"type":"object","properties":{"user":{"type":"object","properties":{"id":{"type":"integer"},"roles":{"type":"array","items":{"type":"string"}}},"required":["id","roles"]}},"required":["user"]}"#
        );
    }

    /// The heart of array unification: one `items` schema for elements that
    /// did not agree. `lastSeen` was on one of two, so it is a property but
    /// not required.
    #[test]
    fn array_items_are_unified_and_required_is_the_intersection() {
        let schema = schema_of(
            &[json!([
                { "id": 1, "name": "Front door" },
                { "id": 2, "name": "Garage", "lastSeen": "2026-09-15" },
            ])],
            OpenApiVersion::V3_1,
        );

        let items = schema.items.as_ref().expect("an items schema");
        assert_eq!(items.required, vec!["id".to_string(), "name".to_string()]);
        assert!(items.properties.contains_key("lastSeen"));
    }

    /// Two saved responses for one status code are two samples of the same
    /// shape, so a key missing from either is optional.
    #[test]
    fn required_is_intersected_across_separate_samples() {
        let schema = schema_of(
            &[json!({ "id": 1, "note": "hi" }), json!({ "id": 2 })],
            OpenApiVersion::V3_1,
        );

        assert_eq!(schema.required, vec!["id".to_string()]);
        assert!(schema.properties.contains_key("note"));
    }

    /// Every array seen at one position contributes to the same item
    /// observation, even across samples.
    #[test]
    fn arrays_from_different_samples_unify_into_one_items_schema() {
        let schema = schema_of(&[json!([1, 2]), json!([3])], OpenApiVersion::V3_1);

        let items = schema.items.as_ref().expect("an items schema");
        assert_eq!(items.kind, Some(SchemaType::One("integer".into())));
    }

    /// An empty array proves it is an array and nothing more. Inventing an
    /// item schema from no items would be a guess.
    #[test]
    fn an_empty_array_gets_no_items_schema() {
        let schema = schema_of(&[json!([])], OpenApiVersion::V3_1);

        assert_eq!(schema.kind, Some(SchemaType::One("array".into())));
        assert!(schema.items.is_none());
    }

    #[test]
    fn a_conflict_becomes_one_of_in_every_version() {
        for version in [
            OpenApiVersion::V3_0,
            OpenApiVersion::V3_1,
            OpenApiVersion::V3_2,
        ] {
            let json = json_of(&[json!(1), json!("one")], version);

            assert_eq!(
                json, r#"{"oneOf":[{"type":"integer"},{"type":"string"}]}"#,
                "{version:?}"
            );
        }
    }

    /// The one place the versions genuinely differ: 3.1 aligned the Schema
    /// Object with JSON Schema 2020-12, where `null` is a type; 3.0's is
    /// Draft-4-flavoured and has the `nullable` keyword instead.
    #[test]
    fn nullability_is_spelled_the_way_each_version_spells_it() {
        let sample = [json!("x"), json!(null)];

        assert_eq!(
            json_of(&sample, OpenApiVersion::V3_1),
            r#"{"type":["string","null"]}"#
        );
        assert_eq!(
            json_of(&sample, OpenApiVersion::V3_0),
            r#"{"type":"string","nullable":true}"#
        );
    }

    #[test]
    fn a_field_that_was_only_ever_null_says_so_as_best_it_can() {
        let sample = [json!(null)];

        assert_eq!(json_of(&sample, OpenApiVersion::V3_1), r#"{"type":"null"}"#);
        assert_eq!(
            json_of(&sample, OpenApiVersion::V3_0),
            r#"{"nullable":true}"#
        );
    }

    #[test]
    fn a_nullable_conflict_keeps_both_the_union_and_the_null() {
        let sample = [json!(1), json!("one"), json!(null)];

        assert_eq!(
            json_of(&sample, OpenApiVersion::V3_1),
            r#"{"oneOf":[{"type":"integer"},{"type":"string"},{"type":"null"}]}"#
        );
        assert_eq!(
            json_of(&sample, OpenApiVersion::V3_0),
            r#"{"nullable":true,"oneOf":[{"type":"integer"},{"type":"string"}]}"#
        );
    }

    /// No depth cap by decision (2026-09-15); the bound lives upstream, where
    /// serde_json refuses to parse past 128 levels of nesting. This only has
    /// to prove the walk itself does not stop early.
    #[test]
    fn nesting_is_followed_as_deep_as_it_goes() {
        let mut value = json!("bottom");
        for _ in 0..64 {
            value = json!({ "next": value });
        }

        let mut schema = schema_of(&[value], OpenApiVersion::V3_1);
        for _ in 0..64 {
            schema = schema
                .properties
                .remove("next")
                .expect("every level should have been described");
        }

        assert_eq!(schema.kind, Some(SchemaType::One("string".into())));
    }

    #[test]
    fn nothing_observed_produces_no_schema() {
        assert!(Observed::default()
            .to_schema(OpenApiVersion::V3_1)
            .is_none());
    }

    #[test]
    fn the_convenience_wrapper_matches_the_accumulator() {
        let value = json!({ "a": 1 });

        assert_eq!(
            infer(&value, OpenApiVersion::V3_1),
            Some(schema_of(
                std::slice::from_ref(&value),
                OpenApiVersion::V3_1
            ))
        );
    }
}

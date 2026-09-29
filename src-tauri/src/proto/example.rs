// http_client/src-tauri/src/proto/example.rs
//
// "Use Example Message": a filled-in message of the method's input type, to
// edit rather than type from nothing.
//
// Built as a real `DynamicMessage` and serialized with every field shown,
// so the example is valid for its type by construction, and the well-known
// types come out in their JSON forms (a Timestamp as RFC 3339, a Duration as
// "0s") without special cases here.
use std::collections::HashMap;

use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MessageDescriptor, Value};

use super::codec::to_json;
use super::ProtoError;

/// How deep nested messages are filled in. A message that contains itself
/// (a tree node, a linked item) would otherwise never end; past this depth a
/// message field is left unset.
const MAX_DEPTH: usize = 3;

/// Types left unset, because no single example is right for them:
/// `Any` needs a type chosen by the user, and `Value` is "any JSON".
const LEFT_UNSET: [&str; 2] = ["google.protobuf.Any", "google.protobuf.Value"];

pub fn example_json(message_type: &MessageDescriptor) -> Result<String, ProtoError> {
    to_json(&example_message(message_type, 0), true)
}

fn example_message(message_type: &MessageDescriptor, depth: usize) -> DynamicMessage {
    let mut message = DynamicMessage::new(message_type.clone());
    for field in message_type.fields() {
        if !first_of_its_oneof(&field) {
            continue;
        }
        if let Some(value) = field_value(&field, depth) {
            message.set_field(&field, value);
        }
    }
    message
}

/// Only one member of a oneof can be set; the example takes the first.
fn first_of_its_oneof(field: &FieldDescriptor) -> bool {
    match field.containing_oneof() {
        None => true,
        Some(oneof) => oneof.fields().next().map(|first| first.number()) == Some(field.number()),
    }
}

/// A repeated field gets one element and a map one entry: enough to show
/// the shape without writing the user a list to delete.
fn field_value(field: &FieldDescriptor, depth: usize) -> Option<Value> {
    if field.is_map() {
        let Kind::Message(entry) = field.kind() else {
            return None;
        };
        let key_field = entry.map_entry_key_field();
        let value_field = entry.map_entry_value_field();
        let key = single_value(&key_field.kind(), key_field.name(), depth)?.into_map_key()?;
        let value = single_value(&value_field.kind(), value_field.name(), depth)?;
        return Some(Value::Map(HashMap::from([(key, value)])));
    }
    let value = single_value(&field.kind(), field.name(), depth)?;
    if field.is_list() {
        Some(Value::List(vec![value]))
    } else {
        Some(value)
    }
}

/// Strings carry their field's name, so the example reads as a template;
/// numbers, booleans and bytes stay at their defaults, which are printed
/// because the example shows every field.
fn single_value(kind: &Kind, field_name: &str, depth: usize) -> Option<Value> {
    Some(match kind {
        Kind::Message(nested) => {
            if depth >= MAX_DEPTH || LEFT_UNSET.contains(&nested.full_name()) {
                return None;
            }
            Value::Message(example_message(nested, depth + 1))
        }
        Kind::Enum(enumeration) => Value::EnumNumber(
            enumeration
                .values()
                .next()
                .map_or(0, |value| value.number()),
        ),
        Kind::String => Value::String(field_name.to_string()),
        Kind::Bytes => Value::Bytes(prost::bytes::Bytes::new()),
        Kind::Bool => Value::Bool(false),
        Kind::Double => Value::F64(0.0),
        Kind::Float => Value::F32(0.0),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Value::I32(0),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Value::I64(0),
        Kind::Uint32 | Kind::Fixed32 => Value::U32(0),
        Kind::Uint64 | Kind::Fixed64 => Value::U64(0),
    })
}

#[cfg(test)]
mod tests {
    use super::super::codec::encode;
    use super::super::fixture;
    use super::*;

    fn example_of(name: &str) -> serde_json::Value {
        let pool = fixture::shop().pool;
        let descriptor = pool.get_message_by_name(name).expect("in the fixture");
        serde_json::from_str(&example_json(&descriptor).expect("serializes")).expect("json")
    }

    #[test]
    fn every_field_shape_is_filled_in() {
        let item = example_of("demo.v1.Item");

        assert_eq!(item["name"], "name");
        assert_eq!(item["price"]["currency"], "currency");
        assert_eq!(item["price"]["units"], "0");
        assert_eq!(item["tags"], serde_json::json!(["tags"]));
        assert_eq!(item["stock"], serde_json::json!({ "key": 0 }));
        assert_eq!(item["kind"], "KIND_UNSPECIFIED");
        assert_eq!(item["at"], "1970-01-01T00:00:00Z");
        assert_eq!(item["history"][0]["currency"], "currency");
    }

    #[test]
    fn only_the_first_member_of_a_oneof_is_set() {
        let item = example_of("demo.v1.Item");

        assert_eq!(item["note"], "note");
        assert!(item.get("code").is_none(), "{item}");
    }

    #[test]
    fn a_message_that_contains_itself_stops_at_the_depth_limit() {
        let item = example_of("demo.v1.Item");

        let mut node = &item;
        let mut depth = 0;
        while let Some(child) = node.get("child").filter(|child| child.is_object()) {
            node = child;
            depth += 1;
        }
        assert_eq!(depth, MAX_DEPTH);
    }

    /// The guarantee the button makes: whatever it writes, Invoke accepts.
    /// Checked for every message type the fixture defines.
    #[test]
    fn every_example_encodes_as_its_own_type() {
        let pool = fixture::shop().pool;
        let mut checked = 0;
        for descriptor in pool.all_messages() {
            if descriptor.is_map_entry() || descriptor.full_name().starts_with("google.protobuf.") {
                continue;
            }
            let json = example_json(&descriptor).expect("serializes");
            encode(&descriptor, &json)
                .unwrap_or_else(|error| panic!("{}: {error}\n{json}", descriptor.full_name()));
            checked += 1;
        }
        assert_eq!(checked, 4, "GetRequest, Item, Summary, Money");
    }
}

// http_client/src-tauri/src/proto/codec.rs
//
// JSON text to protobuf bytes and back, following the proto3 JSON mapping
// that prost-reflect implements: 64-bit integers as strings, bytes as
// base64, enums by name, well-known types in their special forms.
use prost::Message as _;
use prost_reflect::{
    DeserializeOptions, DynamicMessage, MessageDescriptor, ReflectMessage as _, SerializeOptions,
};
use serde_json::ser::PrettyFormatter;

use super::ProtoError;

const INDENT: &[u8] = b"  ";

/// Encodes the editor's JSON as `message_type`.
///
/// An empty editor sends the empty message rather than failing: it is what
/// a user means when a method takes `google.protobuf.Empty` or a request
/// with nothing worth setting.
///
/// Unknown fields are an error, not silently dropped, so a typo in a field
/// name is reported rather than sent as a message without that field.
/// Integers are read from the text by serde_json, exactly, so an `int64`
/// typed as a bare number above 2^53 arrives intact.
pub fn encode(message_type: &MessageDescriptor, json: &str) -> Result<Vec<u8>, ProtoError> {
    if json.trim().is_empty() {
        return Ok(DynamicMessage::new(message_type.clone()).encode_to_vec());
    }
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let message = DynamicMessage::deserialize_with_options(
        message_type.clone(),
        &mut deserializer,
        &DeserializeOptions::new().deny_unknown_fields(true),
    )
    .map_err(|error| message_error(message_type, &error))?;
    deserializer
        .end()
        .map_err(|error| message_error(message_type, &error))?;
    Ok(message.encode_to_vec())
}

/// Decodes a received message to pretty JSON text.
///
/// `include_defaults` is the Settings switch: off follows the JSON mapping
/// and leaves out fields at their default value, on shows every field.
pub fn decode(
    message_type: &MessageDescriptor,
    bytes: &[u8],
    include_defaults: bool,
) -> Result<String, ProtoError> {
    let message = DynamicMessage::decode(message_type.clone(), bytes).map_err(|error| {
        ProtoError::Message(format!(
            "the response is not a valid {}: {error}",
            message_type.full_name()
        ))
    })?;
    to_json(&message, include_defaults)
}

/// Serializes a message the way every message leaves this module.
pub(super) fn to_json(
    message: &DynamicMessage,
    include_defaults: bool,
) -> Result<String, ProtoError> {
    let options = SerializeOptions::new()
        .stringify_64_bit_integers(true)
        .skip_default_fields(!include_defaults);
    let mut out = Vec::new();
    let mut serializer =
        serde_json::Serializer::with_formatter(&mut out, PrettyFormatter::with_indent(INDENT));
    message
        .serialize_with_options(&mut serializer, &options)
        .map_err(|error| {
            ProtoError::Message(format!(
                "could not write {} as JSON: {error}",
                message.descriptor().full_name()
            ))
        })?;
    String::from_utf8(out).map_err(|error| ProtoError::Message(error.to_string()))
}

fn message_error(message_type: &MessageDescriptor, error: &serde_json::Error) -> ProtoError {
    ProtoError::Message(format!(
        "the message is not a valid {}: {error}",
        message_type.full_name()
    ))
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    fn message(name: &str) -> MessageDescriptor {
        fixture::shop()
            .pool
            .get_message_by_name(name)
            .expect("in the fixture")
    }

    fn round_trip(name: &str, json: &str) -> serde_json::Value {
        let descriptor = message(name);
        let bytes = encode(&descriptor, json).expect("encodes");
        let text = decode(&descriptor, &bytes, false).expect("decodes");
        serde_json::from_str(&text).expect("valid JSON")
    }

    #[test]
    fn scalars_nested_messages_lists_and_maps_survive_a_round_trip() {
        let back = round_trip(
            "demo.v1.Item",
            r#"{"name":"book","price":{"currency":"EUR","units":"12"},"tags":["a","b"],
                "stock":{"x":3},"kind":"KIND_BOOK","note":"hi","blob":"AAEC",
                "at":"2026-09-27T10:00:00Z","flag":false}"#,
        );

        assert_eq!(back["name"], "book");
        assert_eq!(back["price"]["units"], "12");
        assert_eq!(back["tags"], serde_json::json!(["a", "b"]));
        assert_eq!(back["stock"]["x"], 3);
        assert_eq!(back["kind"], "KIND_BOOK");
        assert_eq!(back["note"], "hi");
        assert_eq!(back["blob"], "AAEC");
        assert_eq!(back["at"], "2026-09-27T10:00:00Z");
        // proto3 `optional`: explicitly false is present, not a default.
        assert_eq!(back["flag"], false);
    }

    #[test]
    fn sixty_four_bit_extremes_come_back_as_exact_strings() {
        let back = round_trip("demo.v1.Item", r#"{"count":"18446744073709551615"}"#);
        assert_eq!(back["count"], "18446744073709551615");

        let back = round_trip("demo.v1.GetRequest", r#"{"big":"-9223372036854775808"}"#);
        assert_eq!(back["big"], "-9223372036854775808");
    }

    /// The case section 3 point 5 of PLAN-GRPC.md is about: a bare number
    /// JavaScript would round (2^53 + 1) must reach the wire unchanged.
    #[test]
    fn an_int64_above_two_to_the_53_typed_as_a_number_is_exact() {
        let back = round_trip("demo.v1.GetRequest", r#"{"big": 9007199254740993}"#);

        assert_eq!(back["big"], "9007199254740993");
    }

    #[test]
    fn an_enum_may_be_given_by_number() {
        let back = round_trip("demo.v1.Item", r#"{"kind": 1}"#);

        assert_eq!(back["kind"], "KIND_BOOK");
    }

    #[test]
    fn an_unknown_field_is_an_error_naming_the_type() {
        let error = encode(&message("demo.v1.GetRequest"), r#"{"idd":"1"}"#).expect_err("typo");

        let text = error.to_string();
        assert!(matches!(error, ProtoError::Message(_)));
        assert!(text.contains("demo.v1.GetRequest"), "{text}");
        assert!(text.contains("idd"), "{text}");
    }

    #[test]
    fn malformed_json_and_trailing_text_are_errors() {
        let descriptor = message("demo.v1.GetRequest");

        assert!(encode(&descriptor, r#"{"id": }"#).is_err());
        assert!(encode(&descriptor, r#"{"id":"1"} {"id":"2"}"#).is_err());
    }

    #[test]
    fn an_empty_editor_sends_the_empty_message() {
        let descriptor = message("demo.v1.GetRequest");

        let bytes = encode(&descriptor, "  \n").expect("empty is fine");

        assert!(bytes.is_empty());
        assert_eq!(
            decode(&descriptor, &bytes, false).expect("decodes").trim(),
            "{}"
        );
    }

    #[test]
    fn default_values_appear_only_when_asked_for() {
        let descriptor = message("demo.v1.GetRequest");
        let bytes = encode(&descriptor, "{}").expect("encodes");

        let without: serde_json::Value =
            serde_json::from_str(&decode(&descriptor, &bytes, false).expect("decodes"))
                .expect("json");
        let with: serde_json::Value =
            serde_json::from_str(&decode(&descriptor, &bytes, true).expect("decodes"))
                .expect("json");

        assert_eq!(without, serde_json::json!({}));
        assert_eq!(with["id"], "");
        assert_eq!(with["big"], "0");
    }

    #[test]
    fn bytes_that_are_not_the_type_are_an_error() {
        let error = decode(&message("demo.v1.GetRequest"), &[0xff, 0xff, 0xff], false)
            .expect_err("garbage");

        assert!(error.to_string().contains("demo.v1.GetRequest"), "{error}");
    }
}

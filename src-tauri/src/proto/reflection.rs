// http_client/src-tauri/src/proto/reflection.rs
//
// The messages of the gRPC server reflection protocol, and nothing else. The
// conversation that uses them (list the services, fetch the files that
// define them, then every file those import) is 16e's
// `domain/services/grpc_reflection.rs`.
//
// Declared by hand with prost's derive rather than compiled from a vendored
// `reflection.proto` at runtime, as PLAN-GRPC.md first proposed: the client
// needs five small messages, the field numbers are fixed by the protocol,
// and a typed struct is simpler to use than a dynamic message. v1 and the
// deprecated v1alpha use identical messages under two service names, so one
// set of types serves both.
//
// Only the fields this client reads or writes are declared. prost skips the
// rest when decoding, which is how protobuf treats fields it does not know.
use prost::Message as _;
use protox::file::{File, FileResolver, GoogleFileResolver};

use super::ProtoError;

/// Tried first. Servers that predate it answer UNIMPLEMENTED.
pub const V1_PATH: &str = "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo";
/// The fallback, still the only version many servers offer.
pub const V1ALPHA_PATH: &str = "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo";

#[derive(Clone, PartialEq, prost::Message)]
pub struct ServerReflectionRequest {
    #[prost(string, tag = "1")]
    pub host: String,
    #[prost(oneof = "MessageRequest", tags = "3, 4, 7")]
    pub message_request: Option<MessageRequest>,
}

#[derive(Clone, PartialEq, prost::Oneof)]
pub enum MessageRequest {
    #[prost(string, tag = "3")]
    FileByFilename(String),
    #[prost(string, tag = "4")]
    FileContainingSymbol(String),
    /// The value is ignored by servers; the protocol sends an empty string.
    #[prost(string, tag = "7")]
    ListServices(String),
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ServerReflectionResponse {
    #[prost(string, tag = "1")]
    pub valid_host: String,
    #[prost(oneof = "MessageResponse", tags = "4, 6, 7")]
    pub message_response: Option<MessageResponse>,
}

#[derive(Clone, PartialEq, prost::Oneof)]
pub enum MessageResponse {
    #[prost(message, tag = "4")]
    FileDescriptorResponse(FileDescriptorResponse),
    #[prost(message, tag = "6")]
    ListServicesResponse(ListServiceResponse),
    #[prost(message, tag = "7")]
    ErrorResponse(ErrorResponse),
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct FileDescriptorResponse {
    /// Each entry is one serialized `google.protobuf.FileDescriptorProto`.
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub file_descriptor_proto: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ListServiceResponse {
    #[prost(message, repeated, tag = "1")]
    pub service: Vec<ServiceResponse>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ServiceResponse {
    #[prost(string, tag = "1")]
    pub name: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ErrorResponse {
    #[prost(int32, tag = "1")]
    pub error_code: i32,
    #[prost(string, tag = "2")]
    pub error_message: String,
}

/// What one reflection answer amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReflectionReply {
    Services(Vec<String>),
    /// Serialized `FileDescriptorProto`s.
    Files(Vec<Vec<u8>>),
    /// The server's own refusal, e.g. NOT_FOUND for an unknown symbol. The
    /// code is a gRPC status code.
    Refused {
        code: i32,
        message: String,
    },
}

fn request(message: MessageRequest) -> Vec<u8> {
    ServerReflectionRequest {
        host: String::new(),
        message_request: Some(message),
    }
    .encode_to_vec()
}

pub fn list_services() -> Vec<u8> {
    request(MessageRequest::ListServices(String::new()))
}

pub fn file_containing_symbol(symbol: &str) -> Vec<u8> {
    request(MessageRequest::FileContainingSymbol(symbol.to_string()))
}

pub fn file_by_filename(name: &str) -> Vec<u8> {
    request(MessageRequest::FileByFilename(name.to_string()))
}

pub fn parse_reply(bytes: &[u8]) -> Result<ReflectionReply, ProtoError> {
    let response = ServerReflectionResponse::decode(bytes).map_err(|error| {
        ProtoError::Message(format!(
            "the server's reflection answer could not be read: {error}"
        ))
    })?;
    match response.message_response {
        Some(MessageResponse::ListServicesResponse(list)) => Ok(ReflectionReply::Services(
            list.service
                .into_iter()
                .map(|service| service.name)
                .collect(),
        )),
        Some(MessageResponse::FileDescriptorResponse(files)) => {
            Ok(ReflectionReply::Files(files.file_descriptor_proto))
        }
        Some(MessageResponse::ErrorResponse(error)) => Ok(ReflectionReply::Refused {
            code: error.error_code,
            message: error.error_message,
        }),
        None => Err(ProtoError::Message(
            "the server's reflection answer carried nothing this client understands".to_string(),
        )),
    }
}

// --- Descriptor files ------------------------------------------------------
//
// Reflection answers with individual serialized `FileDescriptorProto`s. The
// client collects them, follows their imports, and stores the lot as one
// `FileDescriptorSet`: the same format an import is stored in, so a saved
// schema loads the same way whichever way it was made.

/// A file's name and the names of the files it imports.
pub fn describe_file(bytes: &[u8]) -> Result<(String, Vec<String>), ProtoError> {
    let file = File::decode_file_descriptor_proto(bytes).map_err(|error| {
        ProtoError::Schema(format!(
            "the server sent a file descriptor that could not be read: {error}"
        ))
    })?;
    Ok((
        file.name().to_string(),
        file.file_descriptor_proto().dependency.clone(),
    ))
}

/// The serialized descriptor of a well-known `google/protobuf/*` file, from
/// the copies protox carries. Servers often leave these out of their
/// reflection answers, since every client is expected to know them.
pub fn well_known_file(name: &str) -> Option<Vec<u8>> {
    GoogleFileResolver::new()
        .open_file(name)
        .ok()
        .map(|file| file.file_descriptor_proto().encode_to_vec())
}

/// Serialized `FileDescriptorProto`s joined into one serialized
/// `FileDescriptorSet`: field 1, repeated, each entry length-delimited.
/// Written out rather than decoded and re-encoded, so the server's bytes
/// are stored exactly as it sent them.
pub fn encode_set(files: &[Vec<u8>]) -> Vec<u8> {
    const FILE_FIELD_TAG: u8 = 0x0a;
    let mut out = Vec::with_capacity(files.iter().map(|file| file.len() + 6).sum());
    for file in files {
        out.push(FILE_FIELD_TAG);
        let mut length = file.len();
        loop {
            let low = (length & 0x7f) as u8;
            length >>= 7;
            if length == 0 {
                out.push(low);
                break;
            }
            out.push(low | 0x80);
        }
        out.extend_from_slice(file);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::compile::load;
    use super::super::fixture;
    use super::*;

    fn fixture_files() -> Vec<Vec<u8>> {
        fixture::shop()
            .pool
            .files()
            .map(|file| file.encode_to_vec())
            .collect()
    }

    #[test]
    fn a_file_is_described_by_its_name_and_imports() {
        let pool = fixture::shop().pool;
        let shop = pool
            .files()
            .find(|file| file.name() == "shop/shop.proto")
            .expect("in the fixture");

        let (name, imports) = describe_file(&shop.encode_to_vec()).expect("readable");

        assert_eq!(name, "shop/shop.proto");
        assert!(
            imports.contains(&"common/money.proto".to_string()),
            "{imports:?}"
        );
        assert!(
            imports.contains(&"google/protobuf/timestamp.proto".to_string()),
            "{imports:?}"
        );
        assert!(describe_file(&[0xff, 0xff]).is_err());
    }

    #[test]
    fn well_known_files_come_from_protox_and_nothing_else_does() {
        let timestamp =
            well_known_file("google/protobuf/timestamp.proto").expect("carried by protox");

        assert_eq!(
            describe_file(&timestamp).expect("readable").0,
            "google/protobuf/timestamp.proto"
        );
        assert!(well_known_file("common/money.proto").is_none());
    }

    #[test]
    fn joined_files_load_as_one_schema() {
        let pool = load(&encode_set(&fixture_files())).expect("loads");

        assert!(pool.get_service_by_name("demo.v1.Shop").is_some());
        assert!(pool.get_message_by_name("common.Money").is_some());
    }

    #[test]
    fn long_files_get_a_multi_byte_length() {
        let file = vec![7u8; 300];

        let set = encode_set(&[file]);

        // 300 = 0b10_0101100: low seven bits 0x2c with the high bit set, then 0x02.
        assert_eq!(&set[..3], &[0x0a, 0xac, 0x02]);
        assert_eq!(set.len(), 3 + 300);
    }

    /// Pinned bytes rather than a round trip through the same structs, so a
    /// wrong tag in the derive would be caught.
    #[test]
    fn requests_encode_to_the_protocols_field_numbers() {
        // Field 7, length-delimited (7 << 3 | 2 = 0x3a), empty.
        assert_eq!(list_services(), vec![0x3a, 0x00]);
        // Field 4 (0x22), "a.B".
        assert_eq!(
            file_containing_symbol("a.B"),
            vec![0x22, 3, b'a', b'.', b'B']
        );
        // Field 3 (0x1a), "x.proto".
        assert_eq!(
            file_by_filename("x.proto"),
            [&[0x1a, 7][..], b"x.proto".as_slice()].concat()
        );
    }

    #[test]
    fn a_service_list_is_read_from_hand_built_bytes() {
        // ServerReflectionResponse { list_services_response (6): { service (1): { name (1) } x2 } }
        let service = |name: &str| {
            [
                &[0x0a, name.len() as u8 + 2, 0x0a, name.len() as u8][..],
                name.as_bytes(),
            ]
            .concat()
        };
        let list = [
            service("a.A"),
            service("grpc.reflection.v1.ServerReflection"),
        ]
        .concat();
        let bytes = [&[0x32, list.len() as u8][..], list.as_slice()].concat();

        let reply = parse_reply(&bytes).expect("parses");

        assert_eq!(
            reply,
            ReflectionReply::Services(vec![
                "a.A".to_string(),
                "grpc.reflection.v1.ServerReflection".to_string()
            ])
        );
    }

    #[test]
    fn files_and_refusals_are_told_apart() {
        let files = ServerReflectionResponse {
            valid_host: String::new(),
            message_response: Some(MessageResponse::FileDescriptorResponse(
                FileDescriptorResponse {
                    file_descriptor_proto: vec![vec![1, 2], vec![3]],
                },
            )),
        }
        .encode_to_vec();
        let refused = ServerReflectionResponse {
            valid_host: String::new(),
            message_response: Some(MessageResponse::ErrorResponse(ErrorResponse {
                error_code: 5,
                error_message: "symbol not found".to_string(),
            })),
        }
        .encode_to_vec();

        assert_eq!(
            parse_reply(&files).expect("parses"),
            ReflectionReply::Files(vec![vec![1, 2], vec![3]])
        );
        assert_eq!(
            parse_reply(&refused).expect("parses"),
            ReflectionReply::Refused {
                code: 5,
                message: "symbol not found".to_string()
            }
        );
    }

    #[test]
    fn an_answer_with_no_known_field_is_an_error() {
        assert!(parse_reply(&[]).is_err());
        assert!(parse_reply(&[0xff]).is_err());
    }
}

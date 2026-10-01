// http_client/src-tauri/src/domain/services/grpc_reflection.rs
//
// Asking a server for its schema (PLAN-GRPC.md 16e): the gRPC server
// reflection protocol, spoken over the same GrpcTransport port as a call, so
// it is tested against a scripted server with no network.
//
// The conversation:
// 1. List the services, on `grpc.reflection.v1`. A server that answers
//    UNIMPLEMENTED is asked again on the deprecated `v1alpha`, which many
//    servers still only offer. Neither means no reflection: the user is told
//    to import the .proto files instead.
// 2. For each service, ask for the file that defines it.
// 3. Follow every import until nothing is missing. A well-known
//    `google/protobuf/*` file is taken from protox's own copies, since
//    servers often leave those out; every other file is asked for by name.
//
// Each question is its own short call: send one request, end the stream,
// read one answer. The protocol allows one long bidirectional stream, but a
// schema is a handful of files and one call per question keeps each answer
// tied to its question with nothing to match up.
//
// Like a call, a reflection call must run on the thread that opens it
// (16d), so `reflect` does its work on the calling thread. The command layer
// runs it on a blocking task.
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::domain::error::AppError;
use crate::domain::grpc_wire::{
    frame, is_grpc_response, status_of, Code, FrameDecoder, GrpcStatus,
};
use crate::domain::models::{GrpcCallRequest, KeyValue};
use crate::domain::ports::{GrpcTransport, GrpcWireEvent};
use crate::domain::services::grpc::POLL_WAIT;
use crate::proto::compile::load;
use crate::proto::reflection::{
    describe_file, encode_set, file_by_filename, file_containing_symbol, list_services,
    parse_reply, well_known_file, ReflectionReply, V1ALPHA_PATH, V1_PATH,
};

/// How long one reflection question may take before the server is given
/// up on. Generous: it covers connecting too.
const QUESTION_TIMEOUT: Duration = Duration::from_secs(15);

/// More files than any real schema has. A server that keeps naming new
/// imports past this is broken or hostile, and the walk stops.
const MAX_FILES: usize = 1_000;

/// The reflection service itself is not something the user came to call.
const REFLECTION_PACKAGE: &str = "grpc.reflection.";

const WELL_KNOWN_PREFIX: &str = "google/protobuf/";

/// Which version of the protocol the server answered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflectionVersion {
    V1,
    V1Alpha,
}

/// A schema as the server described it.
#[derive(Debug, Clone)]
pub struct ReflectedSchema {
    /// A serialized `FileDescriptorSet`, the format the schema library
    /// stores (16g) and `proto::compile::load` reads.
    pub encoded: Vec<u8>,
    /// The services the server listed, reflection itself left out.
    pub services: Vec<String>,
    pub version: ReflectionVersion,
}

/// What one question got back.
struct Answer {
    status: GrpcStatus,
    messages: Vec<Vec<u8>>,
}

#[derive(Clone)]
pub struct GrpcReflection {
    transport: Arc<dyn GrpcTransport>,
}

impl GrpcReflection {
    pub fn new(transport: Arc<dyn GrpcTransport>) -> Self {
        Self { transport }
    }

    /// Asks the server at `target` for its whole schema. `target` supplies
    /// the address, TLS, metadata, auth and settings, since a reflection
    /// endpoint is often behind the same credentials as the methods; its
    /// path is ignored.
    pub fn reflect(&self, target: &GrpcCallRequest) -> Result<ReflectedSchema, AppError> {
        let (path, version, listed) = match self.list(target, V1_PATH)? {
            Some(services) => (V1_PATH, ReflectionVersion::V1, services),
            None => match self.list(target, V1ALPHA_PATH)? {
                Some(services) => (V1ALPHA_PATH, ReflectionVersion::V1Alpha, services),
                None => {
                    return Err(AppError::InvalidRequest(
                        "the server does not offer reflection; import its .proto files instead"
                            .to_string(),
                    ))
                }
            },
        };
        let services: Vec<String> = listed
            .into_iter()
            .filter(|service| !service.starts_with(REFLECTION_PACKAGE))
            .collect();
        if services.is_empty() {
            return Err(AppError::InvalidRequest(
                "the server's reflection lists no services".to_string(),
            ));
        }

        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for service in &services {
            let found = self.files(target, path, &file_containing_symbol(service), service)?;
            for bytes in found {
                add(&mut files, bytes)?;
            }
        }
        while let Some(missing) = first_missing(&files)? {
            if files.len() >= MAX_FILES {
                return Err(AppError::InvalidRequest(format!(
                    "the server's schema names more than {MAX_FILES} files; stopped"
                )));
            }
            let found = match well_known(&missing) {
                Some(bytes) => vec![bytes],
                None => self.files(target, path, &file_by_filename(&missing), &missing)?,
            };
            if found.is_empty() {
                return Err(missing_file(&missing, "sent nothing for it"));
            }
            for bytes in found {
                add(&mut files, bytes)?;
            }
            // Asked for one file by name and given others: asking again
            // would get the same answer.
            if !files.contains_key(&missing) {
                return Err(missing_file(&missing, "answered with other files"));
            }
        }

        let encoded = encode_set(&files.into_values().collect::<Vec<_>>());
        // Loading is the proof that every type resolves, before anything is
        // stored or shown.
        load(&encoded)?;
        Ok(ReflectedSchema {
            encoded,
            services,
            version,
        })
    }

    /// The service list, or None when the server does not speak this
    /// version of the protocol.
    fn list(&self, target: &GrpcCallRequest, path: &str) -> Result<Option<Vec<String>>, AppError> {
        let answer = self.ask(target, path, &list_services())?;
        if answer.status.code == Code::Unimplemented {
            return Ok(None);
        }
        match first_reply(&answer)? {
            ReflectionReply::Services(services) => Ok(Some(services)),
            ReflectionReply::Refused { code, message } => {
                Err(refused("list its services", code, &message))
            }
            ReflectionReply::Files(_) => Err(AppError::Transport(
                "the server answered a request for its services with files".to_string(),
            )),
        }
    }

    fn files(
        &self,
        target: &GrpcCallRequest,
        path: &str,
        request: &[u8],
        asked_for: &str,
    ) -> Result<Vec<Vec<u8>>, AppError> {
        let answer = self.ask(target, path, request)?;
        match first_reply(&answer)? {
            ReflectionReply::Files(files) => Ok(files),
            ReflectionReply::Refused { code, message } => {
                Err(refused(&format!("describe {asked_for}"), code, &message))
            }
            ReflectionReply::Services(_) => Err(AppError::Transport(format!(
                "the server answered a request for {asked_for} with a service list"
            ))),
        }
    }

    /// One question: one request out, the stream ended, the answer read to
    /// the end of the call.
    fn ask(
        &self,
        target: &GrpcCallRequest,
        path: &str,
        request: &[u8],
    ) -> Result<Answer, AppError> {
        let mut question = target.clone();
        question.path = path.to_string();
        let framed = frame(request).map_err(|error| AppError::Internal(error.to_string()))?;

        let mut call = self.transport.open(&question)?;
        call.send(framed)?;
        call.end_stream()?;

        let deadline = Instant::now() + QUESTION_TIMEOUT;
        let mut decoder = FrameDecoder::new(question.settings.max_receive_bytes);
        let mut http_status = 0u16;
        let mut headers: Vec<KeyValue> = Vec::new();
        let mut trailers: Vec<KeyValue> = Vec::new();
        let mut messages = Vec::new();
        loop {
            if Instant::now() >= deadline {
                call.cancel();
                return Err(AppError::Transport(format!(
                    "the server did not answer reflection within {} s",
                    QUESTION_TIMEOUT.as_secs()
                )));
            }
            for event in call.poll(POLL_WAIT)? {
                match event {
                    GrpcWireEvent::Headers {
                        status,
                        headers: received,
                    } => {
                        http_status = status;
                        headers = received;
                    }
                    GrpcWireEvent::Data(bytes) => {
                        if is_grpc_response(u32::from(http_status), &headers) {
                            let decoded = decoder.push(&bytes).map_err(|error| {
                                call.cancel();
                                AppError::Transport(format!(
                                    "the reflection answer was not valid: {error}"
                                ))
                            })?;
                            messages.extend(decoded);
                        }
                    }
                    GrpcWireEvent::Trailers(received) => trailers = received,
                    GrpcWireEvent::Ended => {
                        return Ok(Answer {
                            status: status_of(u32::from(http_status), &headers, &trailers),
                            messages,
                        })
                    }
                    GrpcWireEvent::Failed(message) => return Err(AppError::Transport(message)),
                }
            }
        }
    }
}

/// The first message of an answer, as a reflection reply. A non-OK call
/// status (other than the UNIMPLEMENTED `list` handles) is the server
/// refusing reflection as a whole: an auth failure, typically.
fn first_reply(answer: &Answer) -> Result<ReflectionReply, AppError> {
    if answer.status.code != Code::Ok {
        return Err(AppError::Transport(format!(
            "the server refused reflection: {} {}",
            answer.status.code.name(),
            answer.status.message
        )));
    }
    let first = answer.messages.first().ok_or_else(|| {
        AppError::Transport("the server ended the reflection call without an answer".to_string())
    })?;
    Ok(parse_reply(first)?)
}

fn refused(what: &str, code: i32, message: &str) -> AppError {
    let name = u32::try_from(code).map_or("UNKNOWN", |code| Code::from_number(code).name());
    AppError::Transport(format!("the server would not {what}: {name} {message}"))
}

fn missing_file(name: &str, why: &str) -> AppError {
    AppError::Transport(format!(
        "the schema imports {name}, but when asked for it the server {why}"
    ))
}

fn add(files: &mut BTreeMap<String, Vec<u8>>, bytes: Vec<u8>) -> Result<(), AppError> {
    let (name, _) = describe_file(&bytes)?;
    files.entry(name).or_insert(bytes);
    Ok(())
}

/// The first import, in name order, that no collected file provides. Name
/// order makes the walk, and so the questions asked, the same every time.
fn first_missing(files: &BTreeMap<String, Vec<u8>>) -> Result<Option<String>, AppError> {
    for bytes in files.values() {
        let (_, imports) = describe_file(bytes)?;
        if let Some(missing) = imports
            .into_iter()
            .find(|import| !files.contains_key(import))
        {
            return Ok(Some(missing));
        }
    }
    Ok(None)
}

fn well_known(name: &str) -> Option<Vec<u8>> {
    if name.starts_with(WELL_KNOWN_PREFIX) {
        well_known_file(name)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use prost::Message as _;

    use super::*;
    use crate::domain::models::{Auth, GrpcSettings, GrpcTarget};
    use crate::domain::ports::GrpcCall;
    use crate::proto::catalog::catalog;
    use crate::proto::fixture;
    use crate::proto::reflection::{
        ErrorResponse, FileDescriptorResponse, ListServiceResponse, MessageRequest,
        MessageResponse, ServerReflectionRequest, ServerReflectionResponse, ServiceResponse,
    };

    /// A reflection server in memory, built from the fixture schema.
    struct State {
        /// Paths answered with UNIMPLEMENTED, as a server without that
        /// version of the protocol would.
        unimplemented: Vec<&'static str>,
        services: Vec<String>,
        /// File name → serialized descriptor: what the server hands out.
        files: HashMap<String, Vec<u8>>,
        /// Symbol → the one file it answers with.
        symbols: HashMap<String, String>,
        /// Fail every call at the transport.
        fail: bool,
        /// Every question asked, as (path, request).
        asked: Mutex<Vec<(String, MessageRequest)>>,
    }

    impl State {
        /// Serves the fixture's shop.proto and money.proto, but not the
        /// well-known timestamp.proto, as real servers often do.
        fn fixture() -> Self {
            let pool = fixture::shop().pool;
            let files = pool
                .files()
                .filter(|file| !file.name().starts_with(WELL_KNOWN_PREFIX))
                .map(|file| (file.name().to_string(), file.encode_to_vec()))
                .collect();
            Self {
                unimplemented: Vec::new(),
                services: vec![
                    "demo.v1.Shop".to_string(),
                    "grpc.reflection.v1.ServerReflection".to_string(),
                ],
                files,
                symbols: HashMap::from([(
                    "demo.v1.Shop".to_string(),
                    "shop/shop.proto".to_string(),
                )]),
                fail: false,
                asked: Mutex::new(Vec::new()),
            }
        }

        fn answer(&self, path: &str, request: &MessageRequest) -> Vec<GrpcWireEvent> {
            if self.fail {
                return vec![GrpcWireEvent::Failed("could not connect".to_string())];
            }
            if self.unimplemented.contains(&path) {
                return vec![
                    GrpcWireEvent::Headers {
                        status: 200,
                        headers: vec![
                            KeyValue::new("content-type", "application/grpc"),
                            KeyValue::new("grpc-status", "12"),
                        ],
                    },
                    GrpcWireEvent::Ended,
                ];
            }
            let response = match request {
                MessageRequest::ListServices(_) => {
                    MessageResponse::ListServicesResponse(ListServiceResponse {
                        service: self
                            .services
                            .iter()
                            .map(|name| ServiceResponse { name: name.clone() })
                            .collect(),
                    })
                }
                MessageRequest::FileContainingSymbol(symbol) => {
                    self.file_answer(self.symbols.get(symbol))
                }
                MessageRequest::FileByFilename(name) => self.file_answer(Some(name)),
            };
            let bytes = ServerReflectionResponse {
                valid_host: String::new(),
                message_response: Some(response),
            }
            .encode_to_vec();
            vec![
                GrpcWireEvent::Headers {
                    status: 200,
                    headers: vec![KeyValue::new("content-type", "application/grpc")],
                },
                GrpcWireEvent::Data(frame(&bytes).expect("fits")),
                GrpcWireEvent::Trailers(vec![KeyValue::new("grpc-status", "0")]),
                GrpcWireEvent::Ended,
            ]
        }

        fn file_answer(&self, name: Option<&String>) -> MessageResponse {
            match name.and_then(|name| self.files.get(name)) {
                Some(bytes) => MessageResponse::FileDescriptorResponse(FileDescriptorResponse {
                    file_descriptor_proto: vec![bytes.clone()],
                }),
                None => MessageResponse::ErrorResponse(ErrorResponse {
                    error_code: 5,
                    error_message: "not found".to_string(),
                }),
            }
        }
    }

    struct Server(Arc<State>);

    impl GrpcTransport for Server {
        fn open(&self, request: &GrpcCallRequest) -> Result<Box<dyn GrpcCall>, AppError> {
            Ok(Box::new(Question {
                state: self.0.clone(),
                path: request.path.clone(),
                sent: Vec::new(),
                answered: false,
            }))
        }
    }

    /// One reflection call to the in-memory server.
    struct Question {
        state: Arc<State>,
        path: String,
        sent: Vec<u8>,
        answered: bool,
    }

    impl GrpcCall for Question {
        fn send(&mut self, framed: Vec<u8>) -> Result<(), AppError> {
            self.sent.extend(framed);
            Ok(())
        }

        fn end_stream(&mut self) -> Result<(), AppError> {
            Ok(())
        }

        fn poll(&mut self, _timeout: Duration) -> Result<Vec<GrpcWireEvent>, AppError> {
            if self.answered {
                return Ok(Vec::new());
            }
            self.answered = true;
            let body = FrameDecoder::new(1 << 20)
                .push(&self.sent)
                .expect("framed")
                .remove(0);
            let request = ServerReflectionRequest::decode(body.as_slice())
                .expect("a reflection request")
                .message_request
                .expect("a question");
            self.state
                .asked
                .lock()
                .expect("asked")
                .push((self.path.clone(), request.clone()));
            Ok(self.state.answer(&self.path, &request))
        }

        fn cancel(&mut self) {}
    }

    fn reflect_with(
        state: State,
    ) -> (
        Result<ReflectedSchema, AppError>,
        Vec<(String, MessageRequest)>,
    ) {
        let state = Arc::new(state);
        let reflection = GrpcReflection::new(Arc::new(Server(state.clone())));
        let result = reflection.reflect(&target());
        let asked = state.asked.lock().expect("asked").clone();
        (result, asked)
    }

    fn target() -> GrpcCallRequest {
        GrpcCallRequest {
            target: GrpcTarget {
                authority: "127.0.0.1:50051".to_string(),
                tls: false,
            },
            path: String::new(),
            metadata: Vec::new(),
            auth: Auth::None,
            settings: GrpcSettings::default(),
        }
    }

    #[test]
    fn a_v1_server_is_reflected_whole_imports_and_well_known_types_included() {
        let (result, asked) = reflect_with(State::fixture());

        let schema = result.expect("reflected");
        assert_eq!(schema.version, ReflectionVersion::V1);
        assert_eq!(schema.services, ["demo.v1.Shop"]);
        let pool = load(&schema.encoded).expect("loads");
        let services = catalog(&pool);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].methods.len(), 4);
        assert!(pool.get_message_by_name("common.Money").is_some());
        assert!(pool
            .get_message_by_name("google.protobuf.Timestamp")
            .is_some());
        // The import was fetched by name; the well-known file never asked for.
        assert!(asked
            .iter()
            .any(|(_, q)| *q == MessageRequest::FileByFilename("common/money.proto".to_string())));
        assert!(!asked
            .iter()
            .any(|(_, q)| matches!(q, MessageRequest::FileByFilename(name) if name.starts_with(WELL_KNOWN_PREFIX))));
        assert!(asked.iter().all(|(path, _)| path == V1_PATH));
    }

    #[test]
    fn a_v1alpha_only_server_is_asked_again_on_v1alpha() {
        let mut state = State::fixture();
        state.unimplemented = vec![V1_PATH];

        let (result, asked) = reflect_with(state);

        assert_eq!(
            result.expect("reflected").version,
            ReflectionVersion::V1Alpha
        );
        assert_eq!(asked[0].0, V1_PATH);
        assert!(asked[1..].iter().all(|(path, _)| path == V1ALPHA_PATH));
    }

    #[test]
    fn a_server_without_reflection_is_told_to_import_proto_files() {
        let mut state = State::fixture();
        state.unimplemented = vec![V1_PATH, V1ALPHA_PATH];

        let (result, _) = reflect_with(state);

        let error = result.expect_err("no reflection");
        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(error.to_string().contains(".proto"), "{error}");
    }

    #[test]
    fn an_import_the_server_will_not_send_is_named() {
        let mut state = State::fixture();
        state.files.remove("common/money.proto");

        let (result, _) = reflect_with(state);

        let error = result.expect_err("missing import");
        assert!(error.to_string().contains("common/money.proto"), "{error}");
    }

    #[test]
    fn a_server_listing_only_reflection_itself_has_no_services() {
        let mut state = State::fixture();
        state.services = vec!["grpc.reflection.v1.ServerReflection".to_string()];

        let (result, _) = reflect_with(state);

        assert!(matches!(result, Err(AppError::InvalidRequest(_))));
    }

    #[test]
    fn a_transport_failure_is_a_transport_error() {
        let mut state = State::fixture();
        state.fail = true;

        let (result, _) = reflect_with(state);

        assert!(
            matches!(result, Err(AppError::Transport(message)) if message.contains("could not connect"))
        );
    }
}

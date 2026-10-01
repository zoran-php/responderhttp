// http_client/src-tauri/src/domain/services/grpc.rs
//
// gRPC calls in flight (PLAN-GRPC.md 16e): the registry that finds a call by
// id, and the loop that owns it. Everything it knows about libcurl comes
// through the GrpcTransport port, so every rule here is tested against a
// scripted call with no network.
//
// The loop runs on the thread that calls `invoke`, and `invoke` returns when
// the call ends. That thread is also where the call is opened, which is not
// a style choice: libcurl's multi handle is neither Send nor Sync, so the
// call must live and die on one thread (16d). The command layer runs
// `invoke` on a blocking task. Send, End Streaming and Cancel reach the loop
// through a channel and a flag; the loop reports back through an event sink.
//
// Rules this file owns:
// - A unary or server-streaming call sends its one message with Invoke and
//   ends the request stream at once. The two client-streaming kinds wait for
//   Send and End Streaming.
// - A message is encoded before it is queued, so bad JSON is refused to the
//   caller of `send`, synchronously, and never reaches the wire.
// - `Sent` is reported once the message is handed to the transport.
// - Only a gRPC response body is framed (`grpc_wire::is_grpc_response`).
// - The client ends a call, with a status of its own, for Cancel (CANCELLED),
//   a passed deadline (DEADLINE_EXCEEDED), a message over the size limit
//   (RESOURCE_EXHAUSTED), a response that is not valid (INTERNAL) and a
//   transport that failed (UNAVAILABLE, as gRPC clients report it).
// - A sink that refuses an event means nobody is listening: the call is
//   cancelled rather than run on unseen.
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use prost_reflect::{MessageDescriptor, MethodDescriptor};

use crate::domain::cancellation::CancellationToken;
use crate::domain::clock::now_ms;
use crate::domain::error::AppError;
use crate::domain::grpc_wire::{
    frame, is_grpc_response, status_of, Code, FrameDecoder, GrpcStatus,
};
use crate::domain::models::{GrpcCallRequest, GrpcEndedBy, GrpcEvent, KeyValue};
use crate::domain::ports::{GrpcCall, GrpcTransport, GrpcWireEvent};
use crate::domain::services::send_request::validate_proxy;
use crate::proto::catalog::{method_path, MethodKind};
use crate::proto::codec;

/// Minted by the UI, like a WebSocket's ConnectionId, because Send and
/// Cancel need it while `invoke` is still running.
pub type CallId = String;

/// Where a call's events go. Returning false means nobody is listening.
pub type GrpcEventSink = Box<dyn FnMut(GrpcEvent) -> bool + Send>;

/// The owner loop's wait for the transport between checks for Send, End
/// Streaming, Cancel and the deadline. On Windows any wait is rounded up to
/// the 15.6 ms timer tick, and a shorter one would not make arriving data
/// any faster (PLAN-GRPC.md 16a, G14; the ~16 ms floor was accepted). It
/// bounds how late a UI command is picked up.
pub const POLL_WAIT: Duration = Duration::from_millis(10);

struct Prepared {
    framed: Vec<u8>,
    /// The message as it will be shown in the log: re-serialized from the
    /// encoded bytes, so it is what the server gets, not what was typed.
    json: String,
    bytes: usize,
}

enum Command {
    Send(Prepared),
    EndStream,
}

struct Live {
    commands: Sender<Command>,
    cancel: CancellationToken,
    input: MessageDescriptor,
    include_defaults: bool,
    client_streams: bool,
    stream_ended: bool,
}

type Registry = Arc<Mutex<HashMap<CallId, Live>>>;

/// How a call ended, for the `invoke` result. The same facts as its `Ended`
/// event, which the UI may not have received if it stopped listening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcCallOutcome {
    pub status: GrpcStatus,
    pub by: GrpcEndedBy,
    pub trailers: Vec<KeyValue>,
    pub total_ms: u64,
}

/// Use-case: run gRPC calls and feed them. Cheap to clone (Arc).
#[derive(Clone)]
pub struct GrpcCalls {
    transport: Arc<dyn GrpcTransport>,
    live: Registry,
}

impl GrpcCalls {
    pub fn new(transport: Arc<dyn GrpcTransport>) -> Self {
        Self {
            transport,
            live: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Runs the call to its end, reporting through `sink` as it goes.
    ///
    /// `message` is the one message of a unary or server-streaming call
    /// (empty text is the empty message) and is ignored by the two
    /// client-streaming kinds, which take theirs through `send`.
    ///
    /// Err means nothing was sent: the request was refused (bad JSON, bad
    /// metadata, a duplicate id). Everything after the call starts, a
    /// failure included, is an `Ended` event and an Ok outcome.
    pub fn invoke(
        &self,
        id: CallId,
        mut request: GrpcCallRequest,
        method: &MethodDescriptor,
        message: &str,
        mut sink: GrpcEventSink,
    ) -> Result<GrpcCallOutcome, AppError> {
        validate(&request)?;
        request.path = method_path(method);
        let include_defaults = request.settings.include_defaults;
        let client_streams = MethodKind::of(method).client_streams();
        let first = if client_streams {
            None
        } else {
            Some(prepare(&method.input(), message, include_defaults)?)
        };

        let (commands, incoming) = mpsc::channel();
        let cancel = CancellationToken::new();
        {
            let mut live = lock(&self.live);
            if live.contains_key(&id) {
                return Err(AppError::InvalidRequest(format!(
                    "call {id} is already running"
                )));
            }
            live.insert(
                id.clone(),
                Live {
                    commands,
                    cancel: cancel.clone(),
                    input: method.input(),
                    include_defaults,
                    client_streams,
                    stream_ended: !client_streams,
                },
            );
        }

        let result = self.transport.open(&request).map(|call| {
            let owner = Owner {
                call,
                reader: Reader::new(
                    method.output(),
                    include_defaults,
                    request.settings.max_receive_bytes,
                ),
                incoming,
                cancel,
                started: Instant::now(),
                deadline: request.settings.deadline,
            };
            owner.run(first, &mut sink)
        });
        lock(&self.live).remove(&id);
        result
    }

    /// Queues one message of a client-streaming call. Encoded here, so bad
    /// JSON comes back to the caller rather than ending the call.
    pub fn send(&self, id: &str, json: &str) -> Result<(), AppError> {
        let live = lock(&self.live);
        let entry = live.get(id).ok_or_else(|| not_running(id))?;
        if !entry.client_streams {
            return Err(AppError::InvalidRequest(
                "this method takes one message, sent with Invoke".to_string(),
            ));
        }
        if entry.stream_ended {
            return Err(AppError::InvalidRequest(
                "the request stream has been ended; no more messages can be sent".to_string(),
            ));
        }
        let prepared = prepare(&entry.input, json, entry.include_defaults)?;
        entry
            .commands
            .send(Command::Send(prepared))
            .map_err(|_| not_running(id))
    }

    /// End Streaming. A second press is harmless.
    pub fn end_stream(&self, id: &str) -> Result<(), AppError> {
        let mut live = lock(&self.live);
        let entry = live.get_mut(id).ok_or_else(|| not_running(id))?;
        if !entry.client_streams {
            return Err(AppError::InvalidRequest(
                "this method's request stream ends by itself after Invoke".to_string(),
            ));
        }
        if entry.stream_ended {
            return Ok(());
        }
        entry.stream_ended = true;
        entry
            .commands
            .send(Command::EndStream)
            .map_err(|_| not_running(id))
    }

    /// Returns at once; the call reports `Ended` with CANCELLED. Unknown ids
    /// are ignored: a Cancel racing the call's own end is not an error.
    pub fn cancel(&self, id: &str) {
        if let Some(entry) = lock(&self.live).get(id) {
            entry.cancel.cancel();
        }
    }

    /// Called when the frontend starts, as `disconnect_all_web_sockets` is:
    /// a reloaded page leaves calls whose events go nowhere.
    pub fn cancel_all(&self) {
        for entry in lock(&self.live).values() {
            entry.cancel.cancel();
        }
    }

    pub fn is_running(&self, id: &str) -> bool {
        lock(&self.live).contains_key(id)
    }
}

fn validate(request: &GrpcCallRequest) -> Result<(), AppError> {
    if request.target.authority.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "enter the server's address, host:port".to_string(),
        ));
    }
    validate_proxy(request.settings.proxy.as_deref())
}

fn prepare(
    input: &MessageDescriptor,
    json: &str,
    include_defaults: bool,
) -> Result<Prepared, AppError> {
    let bytes = codec::encode(input, json)?;
    let shown = codec::decode(input, &bytes, include_defaults)?;
    let framed = frame(&bytes).map_err(|error| AppError::InvalidRequest(error.to_string()))?;
    Ok(Prepared {
        framed,
        json: shown,
        bytes: bytes.len(),
    })
}

fn not_running(id: &str) -> AppError {
    AppError::InvalidRequest(format!("call {id} is not running"))
}

/// A poisoned lock means a call thread panicked while holding it. The map
/// itself is still consistent (every change is one insert or one remove), so
/// the other calls carry on.
fn lock(live: &Registry) -> MutexGuard<'_, HashMap<CallId, Live>> {
    live.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What the loop does after an event.
enum Next {
    Continue,
    End(GrpcStatus, GrpcEndedBy, Vec<KeyValue>),
}

/// Turns what the transport reports into what the user sees.
struct Reader {
    output: MessageDescriptor,
    include_defaults: bool,
    decoder: FrameDecoder,
    http_status: u16,
    headers: Vec<KeyValue>,
    grpc: bool,
    trailers: Vec<KeyValue>,
}

impl Reader {
    fn new(output: MessageDescriptor, include_defaults: bool, max_receive_bytes: usize) -> Self {
        Self {
            output,
            include_defaults,
            decoder: FrameDecoder::new(max_receive_bytes),
            http_status: 0,
            headers: Vec::new(),
            grpc: false,
            trailers: Vec::new(),
        }
    }

    fn apply(&mut self, event: GrpcWireEvent, out: &mut Vec<GrpcEvent>) -> Next {
        match event {
            GrpcWireEvent::Headers { status, headers } => {
                self.http_status = status;
                self.grpc = is_grpc_response(u32::from(status), &headers);
                if self.grpc {
                    out.push(GrpcEvent::ResponseMetadata {
                        at_ms: now_ms(),
                        metadata: headers.clone(),
                    });
                }
                self.headers = headers;
                Next::Continue
            }
            GrpcWireEvent::Data(bytes) => {
                if !self.grpc {
                    return Next::Continue;
                }
                let messages = match self.decoder.push(&bytes) {
                    Ok(messages) => messages,
                    Err(error) => return client_end_from_wire(&error),
                };
                for message in messages {
                    match codec::decode(&self.output, &message, self.include_defaults) {
                        Ok(json) => out.push(GrpcEvent::Received {
                            at_ms: now_ms(),
                            json,
                            bytes: message.len(),
                        }),
                        Err(error) => {
                            return Next::End(
                                GrpcStatus::new(Code::Internal, error.to_string()),
                                GrpcEndedBy::Client,
                                Vec::new(),
                            )
                        }
                    }
                }
                Next::Continue
            }
            GrpcWireEvent::Trailers(trailers) => {
                self.trailers = trailers;
                Next::Continue
            }
            GrpcWireEvent::Ended => {
                if self.grpc {
                    if let Err(error) = self.decoder.finish() {
                        return client_end_from_wire(&error);
                    }
                }
                Next::End(
                    status_of(u32::from(self.http_status), &self.headers, &self.trailers),
                    GrpcEndedBy::Server,
                    std::mem::take(&mut self.trailers),
                )
            }
            GrpcWireEvent::Failed(message) => Next::End(
                GrpcStatus::new(Code::Unavailable, message),
                GrpcEndedBy::Client,
                Vec::new(),
            ),
        }
    }
}

fn client_end_from_wire(error: &crate::domain::grpc_wire::WireError) -> Next {
    let status = error
        .client_status()
        .unwrap_or_else(|| GrpcStatus::new(Code::Internal, error.to_string()));
    Next::End(status, GrpcEndedBy::Client, Vec::new())
}

/// The loop that owns one call, on the thread that called `invoke`.
struct Owner {
    call: Box<dyn GrpcCall>,
    reader: Reader,
    incoming: Receiver<Command>,
    cancel: CancellationToken,
    started: Instant,
    deadline: Option<Duration>,
}

impl Owner {
    fn run(mut self, first: Option<Prepared>, sink: &mut GrpcEventSink) -> GrpcCallOutcome {
        match self.drive(first, sink) {
            Ending::Reported(outcome) => outcome,
            Ending::Unreported(status, by, trailers) => {
                if by == GrpcEndedBy::Client {
                    self.call.cancel();
                }
                let outcome = self.outcome(status, by, trailers);
                let _ = sink(GrpcEvent::Ended {
                    at_ms: now_ms(),
                    status: outcome.status.clone(),
                    trailers: outcome.trailers.clone(),
                    by: outcome.by,
                    total_ms: outcome.total_ms,
                });
                outcome
            }
        }
    }

    fn outcome(
        &self,
        status: GrpcStatus,
        by: GrpcEndedBy,
        trailers: Vec<KeyValue>,
    ) -> GrpcCallOutcome {
        GrpcCallOutcome {
            status,
            by,
            trailers,
            total_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }

    /// Nobody is listening: stop the call without a word.
    fn abandon(&mut self) -> Ending {
        self.call.cancel();
        Ending::Reported(self.outcome(
            GrpcStatus::new(Code::Cancelled, "nobody is listening any more"),
            GrpcEndedBy::Client,
            Vec::new(),
        ))
    }

    fn drive(&mut self, first: Option<Prepared>, sink: &mut GrpcEventSink) -> Ending {
        if let Some(prepared) = first {
            if let Some(ending) = self.send(prepared, sink) {
                return ending;
            }
            if let Err(error) = self.call.end_stream() {
                return transport_failed(&error);
            }
        }
        let deadline = self.deadline.map(|deadline| self.started + deadline);
        loop {
            if self.cancel.is_cancelled() {
                return client_end(Code::Cancelled, "cancelled".to_string());
            }
            let now = Instant::now();
            if let (Some(at), Some(length)) = (deadline, self.deadline) {
                if now >= at {
                    return client_end(
                        Code::DeadlineExceeded,
                        format!("the {} ms deadline passed", length.as_millis()),
                    );
                }
            }
            while let Ok(command) = self.incoming.try_recv() {
                let ending = match command {
                    Command::Send(prepared) => self.send(prepared, sink),
                    Command::EndStream => {
                        if let Err(error) = self.call.end_stream() {
                            Some(transport_failed(&error))
                        } else if sink(GrpcEvent::StreamEnded { at_ms: now_ms() }) {
                            None
                        } else {
                            Some(self.abandon())
                        }
                    }
                };
                if let Some(ending) = ending {
                    return ending;
                }
            }
            let wait = deadline.map_or(POLL_WAIT, |at| {
                at.saturating_duration_since(now).min(POLL_WAIT)
            });
            let events = match self.call.poll(wait) {
                Ok(events) => events,
                Err(error) => return transport_failed(&error),
            };
            let mut out = Vec::new();
            let mut next = Next::Continue;
            for event in events {
                next = self.reader.apply(event, &mut out);
                if matches!(next, Next::End(..)) {
                    break;
                }
            }
            for event in out {
                if !sink(event) {
                    return self.abandon();
                }
            }
            if let Next::End(status, by, trailers) = next {
                return Ending::Unreported(status, by, trailers);
            }
        }
    }

    /// Hands one message to the transport, then reports it.
    fn send(&mut self, prepared: Prepared, sink: &mut GrpcEventSink) -> Option<Ending> {
        if let Err(error) = self.call.send(prepared.framed) {
            return Some(transport_failed(&error));
        }
        let sent = GrpcEvent::Sent {
            at_ms: now_ms(),
            json: prepared.json,
            bytes: prepared.bytes,
        };
        if sink(sent) {
            None
        } else {
            Some(self.abandon())
        }
    }
}

enum Ending {
    /// The outcome is final and nothing more is reported.
    Reported(GrpcCallOutcome),
    /// The call ended; `Ended` still has to be reported.
    Unreported(GrpcStatus, GrpcEndedBy, Vec<KeyValue>),
}

fn client_end(code: Code, message: String) -> Ending {
    Ending::Unreported(
        GrpcStatus::new(code, message),
        GrpcEndedBy::Client,
        Vec::new(),
    )
}

fn transport_failed(error: &AppError) -> Ending {
    client_end(Code::Unavailable, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::mpsc::Receiver;
    use std::thread;

    use super::*;
    use crate::domain::grpc_wire::DEFAULT_MAX_RECEIVE_BYTES;
    use crate::domain::models::{Auth, GrpcSettings, GrpcTarget};
    use crate::proto::catalog::find_method;
    use crate::proto::fixture;

    /// How a scripted call behaves.
    #[derive(Clone)]
    enum Behaviour {
        /// Answers every message with itself, and ends with status 0 when
        /// the request stream ends.
        Echo,
        /// These events, one batch per poll, after the first poll.
        Script(Vec<Vec<GrpcWireEvent>>),
        /// gRPC headers, then nothing, ever.
        Hang,
    }

    #[derive(Default)]
    struct Record {
        opened: usize,
        sent: Vec<Vec<u8>>,
        ended_stream: bool,
        cancelled: bool,
        paths: Vec<String>,
    }

    struct ScriptedTransport {
        behaviour: Behaviour,
        record: Arc<Mutex<Record>>,
    }

    impl GrpcTransport for ScriptedTransport {
        fn open(&self, request: &GrpcCallRequest) -> Result<Box<dyn GrpcCall>, AppError> {
            let mut record = self.record.lock().expect("record");
            record.opened += 1;
            record.paths.push(request.path.clone());
            let batches = match &self.behaviour {
                Behaviour::Script(batches) => batches.clone().into(),
                _ => VecDeque::new(),
            };
            Ok(Box::new(ScriptedCall {
                behaviour: self.behaviour.clone(),
                record: self.record.clone(),
                batches,
                pending: vec![grpc_headers()],
                done: false,
            }))
        }
    }

    struct ScriptedCall {
        behaviour: Behaviour,
        record: Arc<Mutex<Record>>,
        batches: VecDeque<Vec<GrpcWireEvent>>,
        pending: Vec<GrpcWireEvent>,
        done: bool,
    }

    impl GrpcCall for ScriptedCall {
        fn send(&mut self, framed: Vec<u8>) -> Result<(), AppError> {
            self.record
                .lock()
                .expect("record")
                .sent
                .push(framed.clone());
            if matches!(self.behaviour, Behaviour::Echo) {
                self.pending.push(GrpcWireEvent::Data(framed));
            }
            Ok(())
        }

        fn end_stream(&mut self) -> Result<(), AppError> {
            self.record.lock().expect("record").ended_stream = true;
            if matches!(self.behaviour, Behaviour::Echo) {
                self.pending.push(ok_trailers());
                self.pending.push(GrpcWireEvent::Ended);
            }
            Ok(())
        }

        fn poll(&mut self, timeout: Duration) -> Result<Vec<GrpcWireEvent>, AppError> {
            if self.done {
                return Ok(Vec::new());
            }
            let mut events = std::mem::take(&mut self.pending);
            if matches!(self.behaviour, Behaviour::Script(_)) && events.is_empty() {
                events = self.batches.pop_front().unwrap_or_default();
            }
            if events.is_empty() {
                thread::sleep(timeout.min(Duration::from_millis(1)));
            }
            if events
                .iter()
                .any(|e| matches!(e, GrpcWireEvent::Ended | GrpcWireEvent::Failed(_)))
            {
                self.done = true;
            }
            Ok(events)
        }

        fn cancel(&mut self) {
            self.record.lock().expect("record").cancelled = true;
            self.done = true;
        }
    }

    fn grpc_headers() -> GrpcWireEvent {
        GrpcWireEvent::Headers {
            status: 200,
            headers: vec![KeyValue::new("content-type", "application/grpc")],
        }
    }

    fn ok_trailers() -> GrpcWireEvent {
        GrpcWireEvent::Trailers(vec![KeyValue::new("grpc-status", "0")])
    }

    fn calls(behaviour: Behaviour) -> (GrpcCalls, Arc<Mutex<Record>>) {
        let record = Arc::new(Mutex::new(Record::default()));
        let transport = ScriptedTransport {
            behaviour,
            record: record.clone(),
        };
        (GrpcCalls::new(Arc::new(transport)), record)
    }

    fn method(name: &str) -> MethodDescriptor {
        find_method(&fixture::shop().pool, &format!("/demo.v1.Shop/{name}"))
            .expect("in the fixture")
    }

    fn request() -> GrpcCallRequest {
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

    fn events() -> (GrpcEventSink, Receiver<GrpcEvent>) {
        let (tx, rx) = mpsc::channel();
        (Box::new(move |event| tx.send(event).is_ok()), rx)
    }

    fn message_frame(json: &str, name: &str) -> Vec<u8> {
        let pool = fixture::shop().pool;
        let descriptor = pool.get_message_by_name(name).expect("in the fixture");
        frame(&codec::encode(&descriptor, json).expect("encodes")).expect("fits")
    }

    fn json_of(event: &GrpcEvent) -> serde_json::Value {
        match event {
            GrpcEvent::Sent { json, .. } | GrpcEvent::Received { json, .. } => {
                serde_json::from_str(json).expect("json")
            }
            other => panic!("not a message: {other:?}"),
        }
    }

    fn ended(events: &[GrpcEvent]) -> (Code, GrpcEndedBy) {
        match events.last() {
            Some(GrpcEvent::Ended { status, by, .. }) => (status.code, *by),
            other => panic!("the last event is not Ended: {other:?}"),
        }
    }

    fn wait_until_running(calls: &GrpcCalls, id: &str) {
        for _ in 0..2_000 {
            if calls.is_running(id) {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("call {id} never started");
    }

    #[test]
    fn a_unary_call_sends_one_message_ends_the_stream_and_reports_in_order() {
        let (calls, record) = calls(Behaviour::Echo);
        let (sink, rx) = events();

        let outcome = calls
            .invoke(
                "c1".into(),
                request(),
                &method("Get"),
                r#"{"id":"book"}"#,
                sink,
            )
            .expect("runs");

        let events: Vec<GrpcEvent> = rx.try_iter().collect();
        assert_eq!(outcome.status.code, Code::Ok);
        assert_eq!(outcome.by, GrpcEndedBy::Server);
        assert!(matches!(events[0], GrpcEvent::Sent { .. }));
        assert_eq!(json_of(&events[0])["id"], "book");
        assert!(matches!(events[1], GrpcEvent::ResponseMetadata { .. }));
        // The echo of a GetRequest read as an Item: field 1 is `name` there.
        assert_eq!(json_of(&events[2])["name"], "book");
        assert_eq!(ended(&events), (Code::Ok, GrpcEndedBy::Server));
        let record = record.lock().expect("record");
        assert_eq!(record.paths, vec!["/demo.v1.Shop/Get".to_string()]);
        assert!(record.ended_stream);
        assert!(!calls.is_running("c1"));
    }

    #[test]
    fn bad_json_is_refused_before_anything_is_opened() {
        let (calls, record) = calls(Behaviour::Echo);
        let (sink, _rx) = events();

        let error = calls
            .invoke(
                "c1".into(),
                request(),
                &method("Get"),
                r#"{"nope":1}"#,
                sink,
            )
            .expect_err("unknown field");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert_eq!(record.lock().expect("record").opened, 0);
        assert!(!calls.is_running("c1"));
    }

    #[test]
    fn an_empty_address_or_a_bad_proxy_is_refused() {
        let (calls, _) = calls(Behaviour::Echo);
        let mut no_address = request();
        no_address.target.authority = " ".to_string();
        let mut bad_proxy = request();
        bad_proxy.settings.proxy = Some("ftp://proxy".to_string());

        for request in [no_address, bad_proxy] {
            let (sink, _rx) = events();
            let result = calls.invoke("c".into(), request, &method("Get"), "{}", sink);
            assert!(matches!(result, Err(AppError::InvalidRequest(_))));
        }
    }

    #[test]
    fn server_streaming_messages_are_reported_in_order() {
        let batches = vec![
            vec![GrpcWireEvent::Data(
                [
                    message_frame(r#"{"name":"a"}"#, "demo.v1.Item"),
                    message_frame(r#"{"name":"b"}"#, "demo.v1.Item"),
                ]
                .concat(),
            )],
            vec![GrpcWireEvent::Data(message_frame(
                r#"{"name":"c"}"#,
                "demo.v1.Item",
            ))],
            vec![ok_trailers(), GrpcWireEvent::Ended],
        ];
        let (calls, _) = calls(Behaviour::Script(batches));
        let (sink, rx) = events();

        calls
            .invoke("c1".into(), request(), &method("Watch"), "{}", sink)
            .expect("runs");

        let names: Vec<String> = rx
            .try_iter()
            .filter(|e| matches!(e, GrpcEvent::Received { .. }))
            .map(|e| json_of(&e)["name"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn a_bidirectional_call_takes_send_and_end_streaming_from_another_thread() {
        let (calls, record) = calls(Behaviour::Echo);
        let (sink, rx) = events();
        let runner = calls.clone();
        let handle = thread::spawn(move || {
            runner.invoke("c1".into(), request(), &method("Chat"), "ignored", sink)
        });
        wait_until_running(&calls, "c1");

        calls.send("c1", r#"{"name":"one"}"#).expect("queued");
        calls.send("c1", r#"{"name":"two"}"#).expect("queued");
        calls.end_stream("c1").expect("ended");
        let outcome = handle.join().expect("no panic").expect("runs");

        let events: Vec<GrpcEvent> = rx.try_iter().collect();
        let received: Vec<String> = events
            .iter()
            .filter(|e| matches!(e, GrpcEvent::Received { .. }))
            .map(|e| json_of(e)["name"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(received, ["one", "two"]);
        assert!(events
            .iter()
            .any(|e| matches!(e, GrpcEvent::StreamEnded { .. })));
        assert_eq!(outcome.status.code, Code::Ok);
        // "ignored": a client-streaming call does not send the editor's
        // message on Invoke.
        assert_eq!(record.lock().expect("record").sent.len(), 2);
    }

    #[test]
    fn send_is_refused_for_unknown_ids_single_message_calls_and_ended_streams() {
        let (calls, _) = calls(Behaviour::Hang);
        assert!(matches!(
            calls.send("nope", "{}"),
            Err(AppError::InvalidRequest(_))
        ));

        let (sink, _rx) = events();
        let runner = calls.clone();
        let unary = thread::spawn(move || {
            runner.invoke("u".into(), request(), &method("Watch"), "{}", sink)
        });
        wait_until_running(&calls, "u");
        assert!(matches!(
            calls.send("u", "{}"),
            Err(AppError::InvalidRequest(_))
        ));
        assert!(matches!(
            calls.end_stream("u"),
            Err(AppError::InvalidRequest(_))
        ));
        calls.cancel("u");
        unary.join().expect("no panic").expect("runs");

        let (sink, _rx) = events();
        let runner = calls.clone();
        let bidi =
            thread::spawn(move || runner.invoke("b".into(), request(), &method("Chat"), "", sink));
        wait_until_running(&calls, "b");
        calls.end_stream("b").expect("ended");
        calls.end_stream("b").expect("a second press is harmless");
        assert!(matches!(
            calls.send("b", "{}"),
            Err(AppError::InvalidRequest(_))
        ));
        calls.cancel("b");
        bidi.join().expect("no panic").expect("runs");
    }

    #[test]
    fn bad_json_in_send_comes_back_to_the_caller_and_the_call_goes_on() {
        let (calls, _) = calls(Behaviour::Echo);
        let (sink, _rx) = events();
        let runner = calls.clone();
        let handle =
            thread::spawn(move || runner.invoke("c1".into(), request(), &method("Chat"), "", sink));
        wait_until_running(&calls, "c1");

        assert!(matches!(
            calls.send("c1", "{ not json"),
            Err(AppError::InvalidRequest(_))
        ));
        assert!(calls.is_running("c1"));
        calls.end_stream("c1").expect("ended");
        assert_eq!(
            handle.join().expect("no panic").expect("runs").status.code,
            Code::Ok
        );
    }

    #[test]
    fn a_duplicate_id_is_refused_while_the_first_call_runs() {
        let (calls, _) = calls(Behaviour::Hang);
        let (sink, _rx) = events();
        let runner = calls.clone();
        let first = thread::spawn(move || {
            runner.invoke("c1".into(), request(), &method("Watch"), "{}", sink)
        });
        wait_until_running(&calls, "c1");

        let (sink, _rx2) = events();
        let second = calls.invoke("c1".into(), request(), &method("Watch"), "{}", sink);

        assert!(matches!(second, Err(AppError::InvalidRequest(_))));
        calls.cancel("c1");
        first.join().expect("no panic").expect("runs");
    }

    #[test]
    fn cancel_ends_the_call_with_cancelled_by_the_client() {
        let (calls, record) = calls(Behaviour::Hang);
        let (sink, rx) = events();
        let runner = calls.clone();
        let handle = thread::spawn(move || {
            runner.invoke("c1".into(), request(), &method("Watch"), "{}", sink)
        });
        wait_until_running(&calls, "c1");

        calls.cancel("c1");
        let outcome = handle.join().expect("no panic").expect("runs");

        assert_eq!(
            (outcome.status.code, outcome.by),
            (Code::Cancelled, GrpcEndedBy::Client)
        );
        assert_eq!(
            ended(&rx.try_iter().collect::<Vec<_>>()),
            (Code::Cancelled, GrpcEndedBy::Client)
        );
        assert!(record.lock().expect("record").cancelled);
    }

    #[test]
    fn a_passed_deadline_ends_the_call_with_deadline_exceeded() {
        let (calls, record) = calls(Behaviour::Hang);
        let (sink, _rx) = events();
        let mut request = request();
        request.settings.deadline = Some(Duration::from_millis(30));

        let outcome = calls
            .invoke("c1".into(), request, &method("Watch"), "{}", sink)
            .expect("runs");

        assert_eq!(
            (outcome.status.code, outcome.by),
            (Code::DeadlineExceeded, GrpcEndedBy::Client)
        );
        assert!(
            outcome.status.message.contains("30 ms"),
            "{}",
            outcome.status.message
        );
        assert!(record.lock().expect("record").cancelled);
    }

    #[test]
    fn a_message_over_the_limit_ends_the_call_with_resource_exhausted() {
        let oversized = frame(&[0u8; 64]).expect("fits");
        let (calls, record) = calls(Behaviour::Script(vec![vec![GrpcWireEvent::Data(
            oversized,
        )]]));
        let (sink, _rx) = events();
        let mut request = request();
        request.settings.max_receive_bytes = 16;

        let outcome = calls
            .invoke("c1".into(), request, &method("Watch"), "{}", sink)
            .expect("runs");

        assert_eq!(
            (outcome.status.code, outcome.by),
            (Code::ResourceExhausted, GrpcEndedBy::Client)
        );
        assert!(record.lock().expect("record").cancelled);
    }

    #[test]
    fn a_message_that_is_not_the_output_type_ends_the_call_with_internal() {
        let garbage = frame(&[0xff, 0xff, 0xff]).expect("fits");
        let (calls, _) = calls(Behaviour::Script(vec![vec![GrpcWireEvent::Data(garbage)]]));
        let (sink, _rx) = events();

        let outcome = calls
            .invoke("c1".into(), request(), &method("Watch"), "{}", sink)
            .expect("runs");

        assert_eq!(
            (outcome.status.code, outcome.by),
            (Code::Internal, GrpcEndedBy::Client)
        );
        assert!(
            outcome.status.message.contains("demo.v1.Item"),
            "{}",
            outcome.status.message
        );
    }

    #[test]
    fn a_transport_failure_ends_the_call_with_unavailable() {
        let batches = vec![vec![GrpcWireEvent::Failed(
            "could not connect to 127.0.0.1:50051".to_string(),
        )]];
        let (calls, _) = calls(Behaviour::Script(batches));
        let (sink, _rx) = events();

        let outcome = calls
            .invoke("c1".into(), request(), &method("Watch"), "{}", sink)
            .expect("runs");

        assert_eq!(
            (outcome.status.code, outcome.by),
            (Code::Unavailable, GrpcEndedBy::Client)
        );
        assert!(outcome.status.message.contains("could not connect"));
    }

    #[test]
    fn a_trailers_only_error_is_the_servers_status() {
        let batches = vec![vec![GrpcWireEvent::Ended]];
        let record = Arc::new(Mutex::new(Record::default()));
        let transport = TrailersOnly(record.clone(), batches);
        let calls = GrpcCalls::new(Arc::new(transport));
        let (sink, rx) = events();

        let outcome = calls
            .invoke("c1".into(), request(), &method("Get"), "{}", sink)
            .expect("runs");

        assert_eq!(
            outcome.status,
            GrpcStatus::new(Code::NotFound, "no such book")
        );
        assert_eq!(outcome.by, GrpcEndedBy::Server);
        assert!(!rx
            .try_iter()
            .any(|e| matches!(e, GrpcEvent::Received { .. })));
    }

    /// A transport whose one call answers trailers-only: the status in the
    /// headers, no body.
    struct TrailersOnly(Arc<Mutex<Record>>, Vec<Vec<GrpcWireEvent>>);

    impl GrpcTransport for TrailersOnly {
        fn open(&self, _request: &GrpcCallRequest) -> Result<Box<dyn GrpcCall>, AppError> {
            Ok(Box::new(ScriptedCall {
                behaviour: Behaviour::Script(Vec::new()),
                record: self.0.clone(),
                batches: self.1.clone().into(),
                pending: vec![GrpcWireEvent::Headers {
                    status: 200,
                    headers: vec![
                        KeyValue::new("content-type", "application/grpc"),
                        KeyValue::new("grpc-status", "5"),
                        KeyValue::new("grpc-message", "no%20such%20book"),
                    ],
                }],
                done: false,
            }))
        }
    }

    #[test]
    fn a_body_that_is_not_grpc_is_never_framed() {
        let batches = vec![
            vec![GrpcWireEvent::Data(b"no such thing".to_vec())],
            vec![GrpcWireEvent::Ended],
        ];
        let record = Arc::new(Mutex::new(Record::default()));
        let transport = NotGrpc(record, batches);
        let calls = GrpcCalls::new(Arc::new(transport));
        let (sink, rx) = events();

        let outcome = calls
            .invoke("c1".into(), request(), &method("Get"), "{}", sink)
            .expect("runs");

        assert_eq!(
            (outcome.status.code, outcome.by),
            (Code::Unimplemented, GrpcEndedBy::Server)
        );
        let events: Vec<GrpcEvent> = rx.try_iter().collect();
        assert!(!events.iter().any(|e| matches!(
            e,
            GrpcEvent::ResponseMetadata { .. } | GrpcEvent::Received { .. }
        )));
    }

    /// A plain HTTP 404 with a text body.
    struct NotGrpc(Arc<Mutex<Record>>, Vec<Vec<GrpcWireEvent>>);

    impl GrpcTransport for NotGrpc {
        fn open(&self, _request: &GrpcCallRequest) -> Result<Box<dyn GrpcCall>, AppError> {
            Ok(Box::new(ScriptedCall {
                behaviour: Behaviour::Script(Vec::new()),
                record: self.0.clone(),
                batches: self.1.clone().into(),
                pending: vec![GrpcWireEvent::Headers {
                    status: 404,
                    headers: vec![KeyValue::new("content-type", "text/plain")],
                }],
                done: false,
            }))
        }
    }

    #[test]
    fn a_sink_that_stops_listening_cancels_the_call() {
        let (calls, record) = calls(Behaviour::Echo);
        let sink: GrpcEventSink = Box::new(|_| false);

        let outcome = calls
            .invoke("c1".into(), request(), &method("Get"), "{}", sink)
            .expect("runs");

        assert_eq!(outcome.by, GrpcEndedBy::Client);
        assert!(record.lock().expect("record").cancelled);
        assert!(!calls.is_running("c1"));
    }

    #[test]
    fn include_defaults_reaches_the_received_messages() {
        let (calls, _) = calls(Behaviour::Echo);
        let (sink, rx) = events();
        let mut request = request();
        request.settings.include_defaults = true;

        calls
            .invoke("c1".into(), request, &method("Get"), r#"{"id":"x"}"#, sink)
            .expect("runs");

        let received = rx
            .try_iter()
            .find(|e| matches!(e, GrpcEvent::Received { .. }))
            .expect("a message");
        assert_eq!(json_of(&received)["count"], "0");
    }

    #[test]
    fn the_default_receive_limit_is_grpcs_own() {
        assert_eq!(
            GrpcSettings::default().max_receive_bytes,
            DEFAULT_MAX_RECEIVE_BYTES
        );
    }
}

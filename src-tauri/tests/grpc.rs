// http_client/src-tauri/tests/grpc.rs
//
// CurlGrpcTransport against the local h2c server in support/grpc_server.rs
// (PLAN-GRPC.md 16d). Never a public server: these must pass offline.
//
// The responses are read with the app's own grpc_wire (framing, status), so
// what is tested is the path 16e will use: bytes from libcurl, messages and
// a status out of the domain rules.
//
// The server module is included by path rather than through support/mod.rs,
// so the WebSocket test binary does not compile it.
#[path = "support/grpc_server.rs"]
mod grpc_server;

use std::time::{Duration, Instant};

use grpc_server::GrpcServer;
use responderhttp_lib::domain::error::AppError;
use responderhttp_lib::domain::grpc_wire::{
    frame, is_grpc_response, status_of, Code, FrameDecoder, GrpcStatus, DEFAULT_MAX_RECEIVE_BYTES,
};
use responderhttp_lib::domain::models::{
    ApiKeyLocation, Auth, GrpcCallRequest, GrpcSettings, GrpcTarget, KeyValue,
};
use responderhttp_lib::domain::ports::{GrpcCall, GrpcTransport, GrpcWireEvent};
use responderhttp_lib::http::curl_grpc::CurlGrpcTransport;

/// The owner loop's wait in these tests. Short, so a test never waits long
/// for data that has already arrived.
const POLL: Duration = Duration::from_millis(10);
const LIMIT: Duration = Duration::from_secs(15);
const BIG_MESSAGE: usize = 16 * 1024 * 1024;

/// The bidirectional round trip. On Windows `multi.wait` does not return
/// early for arriving data: it sleeps its timeout rounded up to the 15.6 ms
/// timer tick (16a, G14). With the 10 ms wait below, a round trip is one
/// tick. This catches a slide back to two ticks (31 ms, the spike's first
/// loop) or worse.
const MAX_MEDIAN_ROUND_TRIP: Duration = Duration::from_millis(25);

fn server() -> GrpcServer {
    GrpcServer::start().expect("the test server starts")
}

fn request(port: u16, path: &str) -> GrpcCallRequest {
    GrpcCallRequest {
        target: GrpcTarget {
            authority: format!("127.0.0.1:{port}"),
            tls: false,
        },
        path: path.to_string(),
        metadata: Vec::new(),
        auth: Auth::None,
        settings: GrpcSettings::default(),
    }
}

#[derive(Debug, Default)]
struct Outcome {
    http_status: u16,
    headers: Vec<KeyValue>,
    messages: Vec<Vec<u8>>,
    arrivals: Vec<Duration>,
    trailers: Vec<KeyValue>,
    ended: bool,
    failure: Option<String>,
}

impl Outcome {
    fn status(&self) -> GrpcStatus {
        status_of(u32::from(self.http_status), &self.headers, &self.trailers)
    }

    fn texts(&self) -> Vec<String> {
        self.messages
            .iter()
            .map(|m| String::from_utf8_lossy(m).into_owned())
            .collect()
    }

    fn over(&self) -> bool {
        self.ended || self.failure.is_some()
    }
}

/// A minimal owner loop: what GrpcCalls (16e) will do, without the events
/// sink or the protobuf.
struct Driver {
    call: Box<dyn GrpcCall>,
    decoder: FrameDecoder,
    started: Instant,
    outcome: Outcome,
}

impl Driver {
    fn open(request: &GrpcCallRequest) -> Self {
        let call = match CurlGrpcTransport::new().open(request) {
            Ok(call) => call,
            Err(error) => panic!("open failed: {error}"),
        };
        Self {
            call,
            decoder: FrameDecoder::new(DEFAULT_MAX_RECEIVE_BYTES.max(BIG_MESSAGE)),
            started: Instant::now(),
            outcome: Outcome::default(),
        }
    }

    fn send(&mut self, message: &[u8]) {
        self.call
            .send(frame(message).expect("fits"))
            .expect("send accepted");
    }

    fn end(&mut self) {
        self.call.end_stream().expect("end accepted");
    }

    fn pump_until(&mut self, until: impl Fn(&Outcome) -> bool, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if until(&self.outcome) || self.outcome.over() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            for event in self.call.poll(POLL).expect("poll") {
                self.apply(event);
            }
        }
    }

    fn apply(&mut self, event: GrpcWireEvent) {
        match event {
            GrpcWireEvent::Headers { status, headers } => {
                self.outcome.http_status = status;
                self.outcome.headers = headers;
            }
            GrpcWireEvent::Data(bytes) => {
                // The rule 16e follows: a body that is not gRPC (a plain
                // 404, a proxy's error page) is never framed.
                if !is_grpc_response(u32::from(self.outcome.http_status), &self.outcome.headers) {
                    return;
                }
                for message in self.decoder.push(&bytes).expect("valid framing") {
                    self.outcome.arrivals.push(self.started.elapsed());
                    self.outcome.messages.push(message);
                }
            }
            GrpcWireEvent::Trailers(trailers) => self.outcome.trailers = trailers,
            GrpcWireEvent::Ended => self.outcome.ended = true,
            GrpcWireEvent::Failed(message) => self.outcome.failure = Some(message),
        }
    }

    fn finish(mut self) -> Outcome {
        let finished = self.pump_until(|_| false, LIMIT);
        assert!(
            finished,
            "the call did not end within {LIMIT:?}: {:?}",
            self.outcome
        );
        self.outcome
    }
}

fn unary(request: &GrpcCallRequest, message: &[u8]) -> Outcome {
    let mut driver = Driver::open(request);
    driver.send(message);
    driver.end();
    driver.finish()
}

fn refused(request: &GrpcCallRequest) -> AppError {
    match CurlGrpcTransport::new().open(request) {
        Ok(_) => panic!("{request:?} should have been refused"),
        Err(error) => error,
    }
}

#[test]
fn a_unary_call_over_h2c_is_echoed_with_status_ok() {
    let server = server();

    let outcome = unary(&request(server.port, "/test.Echo/Unary"), b"hello");

    assert!(outcome.ended, "{outcome:?}");
    assert_eq!(outcome.http_status, 200);
    assert_eq!(outcome.texts(), ["hello"]);
    assert_eq!(outcome.status(), GrpcStatus::new(Code::Ok, ""));
}

#[test]
fn a_trailers_only_error_is_read_from_the_headers() {
    let server = server();

    let outcome = unary(&request(server.port, "/test.Echo/Fail"), b"x");

    assert!(outcome.messages.is_empty());
    assert_eq!(
        outcome.status(),
        GrpcStatus::new(Code::NotFound, "not found")
    );
}

#[test]
fn an_unknown_method_is_unimplemented() {
    let server = server();

    let outcome = unary(&request(server.port, "/test.Echo/Nope"), b"x");

    assert_eq!(outcome.status().code, Code::Unimplemented);
}

#[test]
fn a_plain_http_404_maps_to_unimplemented() {
    let server = server();

    let outcome = unary(&request(server.port, "/notgrpc"), b"x");

    assert_eq!(outcome.http_status, 404);
    assert!(
        outcome.messages.is_empty(),
        "a text body was read as gRPC messages"
    );
    assert_eq!(outcome.status().code, Code::Unimplemented);
}

#[test]
fn server_streaming_messages_arrive_as_they_are_sent() {
    let server = server();

    let outcome = unary(&request(server.port, "/test.Echo/ServerStream"), b"5,100");

    assert_eq!(
        outcome.texts(),
        [
            "message 0",
            "message 1",
            "message 2",
            "message 3",
            "message 4"
        ]
    );
    let gaps: Vec<Duration> = outcome.arrivals.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(
        gaps.iter().all(|gap| *gap >= Duration::from_millis(70)),
        "arrivals bunched together, so they were not delivered live: {gaps:?}"
    );
    assert_eq!(outcome.status().code, Code::Ok);
}

#[test]
fn client_streaming_sends_each_message_then_ends_the_stream() {
    let server = server();
    let mut driver = Driver::open(&request(server.port, "/test.Echo/ClientStream"));

    for message in [b"one".as_slice(), b"two", b"three"] {
        driver.send(message);
        driver.pump_until(|_| false, Duration::from_millis(50));
    }
    driver.end();
    let outcome = driver.finish();

    assert_eq!(outcome.texts(), ["count=3 bytes=11"]);
    assert_eq!(outcome.status().code, Code::Ok);
}

/// Also the first message is sent before libcurl has a connection, the case
/// that failed in 16a run 1 (curl_easy_pause refused with 43).
#[test]
fn bidirectional_round_trips_are_full_duplex_and_quick() {
    const ROUND_TRIPS: usize = 200;
    let server = server();
    let mut driver = Driver::open(&request(server.port, "/test.Echo/Bidi"));

    let mut latencies = Vec::with_capacity(ROUND_TRIPS);
    for i in 0..ROUND_TRIPS {
        let payload = format!("ping {i}");
        let sent_at = Instant::now();
        driver.send(payload.as_bytes());
        let echoed = driver.pump_until(|o| o.messages.len() > i, Duration::from_secs(5));
        assert!(echoed, "echo {i} never came: {:?}", driver.outcome);
        assert_eq!(driver.outcome.messages[i], payload.as_bytes());
        latencies.push(sent_at.elapsed());
    }
    driver.end();
    let outcome = driver.finish();

    latencies.sort();
    let median = latencies[latencies.len() / 2];
    println!(
        "bidi round trip: median {} us, p95 {} us",
        median.as_micros(),
        latencies[latencies.len() * 95 / 100].as_micros()
    );
    assert!(
        median < MAX_MEDIAN_ROUND_TRIP,
        "median round trip {median:?}"
    );
    assert_eq!(outcome.status().code, Code::Ok);
}

#[test]
fn cancel_resets_the_stream_on_the_server() {
    let server = server();
    let mut driver = Driver::open(&request(server.port, "/test.Echo/Slow"));
    driver.end();

    assert!(driver.pump_until(|o| o.messages.len() >= 3, LIMIT));
    driver.call.cancel();
    assert!(driver.call.poll(POLL).expect("poll").is_empty());
    std::thread::sleep(Duration::from_millis(700));

    let seen = server.log.find("/test.Echo/Slow ended");
    assert!(
        seen.as_deref()
            .is_some_and(|line| line.contains("client stopped")),
        "the server did not see the stream stop: {:?}",
        server.log.lines()
    );
}

#[test]
fn metadata_auth_and_the_protocol_headers_reach_the_server() {
    let server = server();
    let mut request = request(server.port, "/test.Echo/Unary");
    request.metadata = vec![
        KeyValue::new("X-Trace", "abc"),
        KeyValue::new("", "untouched row"),
    ];
    request.auth = Auth::Bearer {
        token: "t0k".to_string(),
    };
    request.settings.deadline = Some(Duration::from_secs(5));

    unary(&request, b"x");

    let line = server
        .log
        .find("/test.Echo/Unary method=POST")
        .unwrap_or_else(|| panic!("no request logged: {:?}", server.log.lines()));
    for expected in [
        "content-type: application/grpc",
        "te: trailers",
        "user-agent: ResponderHTTP/",
        "grpc-timeout: 5000000u",
        "x-trace: abc",
        "authorization: Bearer t0k",
    ] {
        assert!(line.contains(expected), "{expected:?} missing from {line}");
    }
    assert!(
        !line.contains("accept:"),
        "libcurl's Accept was not dropped: {line}"
    );
}

#[test]
fn reserved_metadata_is_refused_before_anything_is_sent() {
    let mut request = request(1, "/test.Echo/Unary");
    request.metadata = vec![KeyValue::new("grpc-status", "0")];

    assert!(matches!(refused(&request), AppError::InvalidRequest(_)));
}

#[test]
fn an_api_key_in_the_query_string_is_refused() {
    let mut request = request(1, "/test.Echo/Unary");
    request.auth = Auth::ApiKey {
        key: "k".to_string(),
        value: "v".to_string(),
        location: ApiKeyLocation::Query,
    };

    assert!(matches!(refused(&request), AppError::InvalidRequest(_)));
}

#[test]
fn an_unreachable_server_fails_with_a_transport_message() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .expect("a free port");

    let outcome = unary(&request(port, "/test.Echo/Unary"), b"x");

    assert!(!outcome.ended);
    assert!(outcome.failure.is_some(), "{outcome:?}");
    assert!(outcome.messages.is_empty());
}

#[test]
fn a_sixteen_mib_message_goes_out_and_back_intact() {
    let server = server();
    let big: Vec<u8> = (0..BIG_MESSAGE)
        .map(|i| ((i * 31 + 7) % 251) as u8)
        .collect();

    let outcome = unary(&request(server.port, "/test.Echo/Unary"), &big);

    assert_eq!(outcome.messages.len(), 1);
    assert!(
        outcome.messages[0] == big,
        "the echo differs from what was sent"
    );
    assert_eq!(outcome.status().code, Code::Ok);
}

#[test]
fn sending_after_end_stream_is_refused() {
    let server = server();
    let mut driver = Driver::open(&request(server.port, "/test.Echo/Bidi"));
    driver.end();

    let error = driver
        .call
        .send(frame(b"late").expect("fits"))
        .expect_err("the stream is over");

    assert!(matches!(error, AppError::InvalidRequest(_)));
    driver.finish();
}

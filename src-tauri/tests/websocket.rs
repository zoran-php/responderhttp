// http_client/src-tauri/tests/websocket.rs
//
// Integration tests for the WebSocket transport (PLAN.md Phase 13b): the real
// libcurl connector and the real session service against a local server
// (tests/support/ws_server.rs). Nothing here leaves loopback, except the one
// test marked #[ignore], which talks to wss://echo.websocket.org on request.
//
// Not covered here: TLS. The local server speaks plain ws://, so wss://, the
// OS certificate store and certificate refusals rest on the 13a spike's
// Windows run, on the opt-in public test below, and on the manual checks.
mod support;

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use responderhttp_lib::domain::cancellation::CancellationToken;
use responderhttp_lib::domain::error::AppError;
use responderhttp_lib::domain::models::{
    ClosedBy, Cookie, KeyValue, WebSocketRequest, WebSocketSettings, WsEvent, WsPayload,
};
use responderhttp_lib::domain::ports::{CookieRepository, WebSocketConnector, WsConnectError};
use responderhttp_lib::domain::services::websocket::{
    EventSink, ReconnectPolicy, WebSocketSessions,
};
use responderhttp_lib::http::cookie_websocket::CookieWebSocketConnector;
use responderhttp_lib::http::curl_websocket::CurlWebSocketConnector;

use support::ws_server::{fnv1a, WsServer, BIG_FRAME_LEN};

const WAIT: Duration = Duration::from_secs(10);
const ID: &str = "ws-test";

fn request(url: String) -> WebSocketRequest {
    request_with(url, WebSocketSettings::default())
}

fn request_with(url: String, settings: WebSocketSettings) -> WebSocketRequest {
    WebSocketRequest {
        url,
        headers: Vec::new(),
        settings,
    }
}

fn sessions() -> WebSocketSessions {
    WebSocketSessions::new(Arc::new(CurlWebSocketConnector::new()))
}

fn sink() -> (EventSink, Receiver<WsEvent>) {
    let (tx, rx) = mpsc::channel();
    (Box::new(move |event| tx.send(event).is_ok()), rx)
}

fn next(rx: &Receiver<WsEvent>) -> WsEvent {
    rx.recv_timeout(WAIT).expect("an event should arrive")
}

/// Connects and returns the event stream, past the `Connected` event.
fn open(sessions: &WebSocketSessions, request: WebSocketRequest) -> Receiver<WsEvent> {
    let (sink, rx) = sink();
    sessions
        .connect(ID.into(), request, sink)
        .expect("should connect");
    match next(&rx) {
        WsEvent::Connected { status, .. } => assert_eq!(status, 101),
        other => panic!("expected Connected, got {other:?}"),
    }
    rx
}

fn received(event: WsEvent) -> WsPayload {
    match event {
        WsEvent::Received { payload, .. } => payload,
        other => panic!("expected Received, got {other:?}"),
    }
}

fn text(event: WsEvent) -> String {
    match received(event) {
        WsPayload::Text(text) => text,
        other => panic!("expected text, got {other:?}"),
    }
}

fn closed(event: WsEvent) -> (Option<u16>, String, ClosedBy) {
    match event {
        WsEvent::Closed {
            code, reason, by, ..
        } => (code, reason, by),
        other => panic!("expected Closed, got {other:?}"),
    }
}

fn until_closed(rx: &Receiver<WsEvent>) -> WsEvent {
    loop {
        let event = next(rx);
        if matches!(event, WsEvent::Closed { .. }) {
            return event;
        }
    }
}

#[test]
fn text_and_binary_messages_echo_through_the_real_transport() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/echo")));

    sessions
        .send(ID, WsPayload::Text("hello".into()))
        .expect("should queue");
    assert!(matches!(next(&rx), WsEvent::Sent { .. }));
    assert_eq!(text(next(&rx)), "hello");

    let bytes: Vec<u8> = (0..=255).collect();
    sessions
        .send(ID, WsPayload::Binary(bytes.clone()))
        .expect("should queue");
    assert!(matches!(next(&rx), WsEvent::Sent { .. }));
    assert_eq!(received(next(&rx)), WsPayload::Binary(bytes));

    sessions.disconnect(ID);
    until_closed(&rx);
}

#[test]
fn the_handshake_response_headers_are_reported() {
    let server = WsServer::start();
    let sessions = sessions();
    let (sink, rx) = sink();
    sessions
        .connect(ID.into(), request(server.url("/echo")), sink)
        .expect("should connect");

    match next(&rx) {
        WsEvent::Connected { headers, .. } => assert!(headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("upgrade") && h.value == "websocket")),
        other => panic!("expected Connected, got {other:?}"),
    }
    sessions.disconnect(ID);
    until_closed(&rx);
}

#[test]
fn a_fragmented_message_arrives_whole() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/fragmented")));

    assert_eq!(text(next(&rx)), "hello world");

    sessions.disconnect(ID);
    until_closed(&rx);
}

#[test]
fn a_frame_larger_than_the_receive_buffer_arrives_whole() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/big")));

    let WsPayload::Binary(bytes) = received(next(&rx)) else {
        panic!("expected a binary message");
    };

    assert_eq!(bytes.len(), BIG_FRAME_LEN);
    assert!(bytes
        .iter()
        .enumerate()
        .all(|(i, byte)| *byte == (i % 251) as u8));
    sessions.disconnect(ID);
    until_closed(&rx);
}

/// The 13a finding this design rests on: with automatic pong off, the server
/// gets its pong even though the client is only listening.
#[test]
fn a_server_ping_is_answered_while_the_client_only_listens() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/ping")));

    assert_eq!(text(next(&rx)), "pong-ok");
    assert!(server.wait_for("pong received: p1").is_some());

    sessions.disconnect(ID);
    until_closed(&rx);
}

#[test]
fn a_server_close_is_answered_and_reported_with_its_code() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/server-close")));

    let (code, reason, by) = closed(next(&rx));

    assert_eq!(
        (code, reason.as_str(), by),
        (Some(1001), "bye", ClosedBy::Server)
    );
    assert!(server.wait_for("close answered: code=Some(1001)").is_some());
    assert!(!sessions.is_open(ID));
}

/// Send and Disconnect back to back: the echo comes back while the
/// connection is closing, and is reported rather than dropped (decided
/// 2026-09-22, after the 13c smoke test).
#[test]
fn an_echo_that_arrives_while_closing_is_still_reported() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/echo")));

    sessions
        .send(ID, WsPayload::Text("last words".into()))
        .expect("should queue");
    sessions.disconnect(ID);

    assert!(matches!(next(&rx), WsEvent::Sent { .. }));
    assert_eq!(text(next(&rx)), "last words");
    let (code, _, by) = closed(next(&rx));
    assert_eq!((code, by), (Some(1000), ClosedBy::User));
}

#[test]
fn disconnect_closes_with_a_normal_close_the_server_sees() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/echo")));

    sessions.disconnect(ID);
    let (code, _, by) = closed(next(&rx));

    assert_eq!((code, by), (Some(1000), ClosedBy::User));
    assert!(server
        .wait_for("/echo: client close code=Some(1000)")
        .is_some());
    assert!(!sessions.is_open(ID));
}

#[test]
fn a_message_over_the_limit_closes_the_connection_with_1009() {
    let server = WsServer::start();
    let sessions = sessions();
    let settings = WebSocketSettings {
        max_message_bytes: 1024,
        ..WebSocketSettings::default()
    };
    let rx = open(&sessions, request_with(server.url("/oversize"), settings));

    let (code, _, by) = closed(next(&rx));

    assert_eq!((code, by), (Some(1009), ClosedBy::Error));
    assert!(server
        .wait_for("/oversize: client close code=Some(1009)")
        .is_some());
}

#[test]
fn a_refused_upgrade_is_reported_with_its_status() {
    let server = WsServer::start();
    let connector = CurlWebSocketConnector::new();

    for (path, expected) in [("/401", 401), ("/403", 403), ("/200", 200)] {
        let result = connector.connect(&request(server.url(path)), &CancellationToken::new());
        assert_eq!(
            result.err(),
            Some(WsConnectError::Refused { status: expected }),
            "{path}"
        );
    }
}

#[test]
fn a_refused_upgrade_through_the_service_names_the_status_and_opens_nothing() {
    let server = WsServer::start();
    let sessions = sessions();
    let (sink, _rx) = sink();

    let error = sessions
        .connect(ID.into(), request(server.url("/401")), sink)
        .expect_err("should be refused");

    assert!(error.to_string().contains("401"), "{error}");
    assert!(!sessions.is_open(ID));
}

#[test]
fn an_unreachable_host_is_a_plain_failure() {
    let connector = CurlWebSocketConnector::new();

    let result = connector.connect(
        &request("ws://127.0.0.1:1/".into()),
        &CancellationToken::new(),
    );

    assert!(matches!(result.err(), Some(WsConnectError::Failed(_))));
}

/// Disconnect pressed while the server has accepted TCP but not answered.
#[test]
fn disconnect_cancels_a_handshake_the_server_never_answers() {
    let server = WsServer::start();
    let sessions = sessions();
    let background = sessions.clone();
    let url = server.url("/hang");
    let (sink, _rx) = sink();
    let started = Instant::now();
    let handshake = thread::spawn(move || background.connect(ID.into(), request(url), sink));
    while !sessions.is_open(ID) && started.elapsed() < WAIT {
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(Duration::from_millis(100));

    sessions.disconnect(ID);
    let result = handshake
        .join()
        .expect("the handshake thread should finish");

    assert!(matches!(result, Err(AppError::Cancelled)), "{result:?}");
    // Well under the server's 10 s hang: the cancel reached libcurl.
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!sessions.is_open(ID));
}

#[test]
fn custom_headers_and_a_subprotocol_reach_the_handshake() {
    let server = WsServer::start();
    let sessions = sessions();
    let mut handshake = request(server.url("/headers"));
    handshake.headers = vec![
        KeyValue::new("X-Spike", "42"),
        KeyValue::new("Sec-WebSocket-Protocol", "chat"),
        // A blank row is the table's trailing spare, never a header.
        KeyValue::new("  ", "ignored"),
    ];
    let rx = open(&sessions, handshake);

    assert_eq!(text(next(&rx)), "x-spike=42;cookie=<absent>;protocol=chat");
    sessions.disconnect(ID);
    until_closed(&rx);
}

/// A jar in memory: the repository tests cover SQLite, this covers the
/// handshake's use of whatever jar it is given.
#[derive(Default)]
struct MemoryJar {
    cookies: Mutex<Vec<Cookie>>,
}

impl CookieRepository for MemoryJar {
    fn list(&self) -> Result<Vec<Cookie>, AppError> {
        Ok(self.cookies.lock().expect("jar lock").clone())
    }
    fn upsert(&self, cookie: &Cookie) -> Result<(), AppError> {
        let mut cookies = self.cookies.lock().expect("jar lock");
        cookies.retain(|existing| existing.name != cookie.name);
        cookies.push(cookie.clone());
        Ok(())
    }
    fn delete(&self, _domain: &str, _path: &str, _name: &str) -> Result<(), AppError> {
        Ok(())
    }
    fn clear(&self) -> Result<(), AppError> {
        Ok(())
    }
    fn clear_session(&self) -> Result<(), AppError> {
        Ok(())
    }
    fn purge_expired(&self, _now: u64) -> Result<(), AppError> {
        Ok(())
    }
}

#[test]
fn the_jar_sends_its_cookies_with_the_handshake_and_stores_what_it_sets() {
    let server = WsServer::start();
    let jar = Arc::new(MemoryJar::default());
    let sessions = WebSocketSessions::new(Arc::new(CookieWebSocketConnector::new(
        Arc::new(CurlWebSocketConnector::new()),
        jar.clone(),
    )));

    // The server sets a cookie on the upgrade...
    let rx = open(&sessions, request(server.url("/set-cookie")));
    sessions.disconnect(ID);
    until_closed(&rx);
    assert_eq!(jar.list().expect("jar")[0].name, "session");

    // ...and the next handshake to the same host carries it.
    let rx = open(&sessions, request(server.url("/headers")));
    assert_eq!(
        text(next(&rx)),
        "x-spike=<absent>;cookie=session=abc;protocol=<absent>"
    );
    sessions.disconnect(ID);
    until_closed(&rx);
}

/// 13a finding 7: a big message to a stalled reader comes back partial and
/// blocked, and the send loop has to finish it byte-exactly.
#[test]
fn a_big_message_to_a_slow_reader_arrives_byte_exact() {
    const LEN: usize = 8 * 1024 * 1024;
    let server = WsServer::start();
    let sessions = sessions();
    let settings = WebSocketSettings {
        max_message_bytes: LEN,
        ..WebSocketSettings::default()
    };
    let rx = open(
        &sessions,
        request_with(server.url("/slow-reader"), settings),
    );
    let payload: Vec<u8> = (0..LEN).map(|i| (i % 253) as u8).collect();
    let expected = format!("got op=0x2 len={LEN} fnv={:016x}", fnv1a(&payload));

    sessions
        .send(ID, WsPayload::Binary(payload))
        .expect("should queue");

    assert!(matches!(next(&rx), WsEvent::Sent { .. }));
    assert_eq!(text(next(&rx)), expected);
    sessions.disconnect(ID);
    until_closed(&rx);
}

#[test]
fn a_connection_dropped_without_a_close_frame_is_closed_by_error() {
    let server = WsServer::start();
    let sessions = sessions();
    let rx = open(&sessions, request(server.url("/drop")));

    assert_eq!(text(next(&rx)), "dropping");
    let (code, _, by) = closed(next(&rx));

    assert_eq!((code, by), (None, ClosedBy::Error));
    assert!(!sessions.is_open(ID));
}

#[test]
fn with_reconnect_on_a_dropped_connection_comes_back() {
    let server = WsServer::start();
    let sessions = WebSocketSessions::with_policy(
        Arc::new(CurlWebSocketConnector::new()),
        ReconnectPolicy {
            first_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(200),
            max_attempts: 3,
        },
    );
    let settings = WebSocketSettings {
        auto_reconnect: true,
        ..WebSocketSettings::default()
    };
    let rx = open(&sessions, request_with(server.url("/flaky"), settings));

    assert_eq!(text(next(&rx)), "first");
    assert!(matches!(next(&rx), WsEvent::Error { .. }));
    assert!(matches!(
        next(&rx),
        WsEvent::Reconnecting { attempt: 1, .. }
    ));
    assert!(matches!(next(&rx), WsEvent::Connected { .. }));
    assert_eq!(text(next(&rx)), "second");

    sessions.disconnect(ID);
    until_closed(&rx);
}

/// Real TLS against a real certificate through the OS store — the one thing
/// the local server cannot cover. Needs network, so it runs only on request:
/// `cargo test --test websocket -- --ignored`.
#[test]
#[ignore = "needs network: talks to wss://echo.websocket.org"]
fn a_real_wss_echo_server_answers() {
    let sessions = sessions();
    let rx = open(&sessions, request("wss://echo.websocket.org".into()));

    sessions
        .send(ID, WsPayload::Text("hello-wss".into()))
        .expect("should queue");
    let deadline = Instant::now() + WAIT;
    let mut echoed = false;
    while Instant::now() < deadline && !echoed {
        if let WsEvent::Received {
            payload: WsPayload::Text(text),
            ..
        } = next(&rx)
        {
            // The server greets with "Request served by …" first.
            echoed = text == "hello-wss";
        }
    }

    assert!(echoed, "no echo from wss://echo.websocket.org");
    sessions.disconnect(ID);
    until_closed(&rx);
}

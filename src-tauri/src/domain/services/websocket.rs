// http_client/src-tauri/src/domain/services/websocket.rs
//
// Live WebSocket connections (PLAN.md Phase 13b): the registry that finds one
// by id, the loop that owns it, and the reconnect policy. Everything it knows
// about libcurl comes through the WebSocketConnector port, so every rule here
// is tested against a scripted connection with no network.
//
// One thread per connection owns it for its whole life. libcurl's easy handle
// must not be shared between threads, so nothing else ever touches it: the UI
// reaches the thread through a channel of outgoing messages and a stop flag,
// and the thread reports back through an event sink.
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::domain::cancellation::CancellationToken;
use crate::domain::clock::now_ms;
use crate::domain::error::AppError;
use crate::domain::models::{ClosedBy, WebSocketRequest, WsEvent, WsHandshake, WsPayload};
use crate::domain::ports::{WebSocketConnection, WebSocketConnector, WsConnectError, WsPoll};
use crate::domain::services::send_request::validate_proxy;
use crate::domain::ws_frames::{
    close_payload, Assembled, CloseInfo, FrameKind, Reassembler, CLOSE_NORMAL,
};

/// Minted by the UI, like `RequestId`, because the UI needs it before the
/// handshake finishes — Disconnect has to be able to cancel a connect.
pub type ConnectionId = String;

/// Where a connection's events go. Returning false means nobody is listening
/// any more (the window was reloaded, the tab closed), and the connection
/// closes rather than running on unseen.
pub type EventSink = Box<dyn FnMut(WsEvent) -> bool + Send>;

/// How long the owner loop waits on the socket before checking for outgoing
/// messages and Disconnect. It bounds how late a Send can go out, and sets
/// the idle wake-up rate: forty a second, which costs nothing measurable
/// (PLAN.md Phase 13a, finding 5).
pub const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long Disconnect waits for the server to answer its close frame before
/// giving up on the courtesy and dropping the connection.
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

const SUPPORTED_SCHEMES: [&str; 2] = ["ws://", "wss://"];

/// When to try again after an unexpected drop: doubling from `first_delay`,
/// never longer than `max_delay`, at most `max_attempts` times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub first_delay: Duration,
    pub max_delay: Duration,
    pub max_attempts: u32,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            first_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_attempts: 10,
        }
    }
}

impl ReconnectPolicy {
    /// None once the attempts are used up. Attempts count from 1.
    pub fn delay(&self, attempt: u32) -> Option<Duration> {
        if attempt == 0 || attempt > self.max_attempts {
            return None;
        }
        let factor = 1u32.checked_shl(attempt - 1).unwrap_or(u32::MAX);
        Some(self.first_delay.saturating_mul(factor).min(self.max_delay))
    }
}

struct Live {
    outgoing: Sender<WsPayload>,
    stop: CancellationToken,
}

type Registry = Arc<Mutex<HashMap<ConnectionId, Live>>>;

/// Use-case: open, feed and close WebSocket connections. Cheap to clone
/// (Arc), so the command layer can move a clone into a blocking task.
#[derive(Clone)]
pub struct WebSocketSessions {
    connector: Arc<dyn WebSocketConnector>,
    policy: ReconnectPolicy,
    live: Registry,
}

impl WebSocketSessions {
    pub fn new(connector: Arc<dyn WebSocketConnector>) -> Self {
        Self::with_policy(connector, ReconnectPolicy::default())
    }

    /// The seam that lets tests reconnect in milliseconds instead of seconds.
    pub fn with_policy(connector: Arc<dyn WebSocketConnector>, policy: ReconnectPolicy) -> Self {
        Self {
            connector,
            policy,
            live: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Blocks until the handshake is done. Success reports `Connected`
    /// through the sink and returns; everything after that is reported
    /// through the sink by the connection's own thread. A failed handshake
    /// returns the error and reports nothing.
    pub fn connect(
        &self,
        id: ConnectionId,
        request: WebSocketRequest,
        mut sink: EventSink,
    ) -> Result<(), AppError> {
        validate_ws_url(&request.url)?;
        validate_proxy(request.settings.proxy.as_deref())?;

        let stop = CancellationToken::new();
        let (outgoing, incoming) = mpsc::channel();
        {
            let mut live = lock(&self.live);
            if live.contains_key(&id) {
                return Err(AppError::InvalidRequest(format!(
                    "connection {id} is already open"
                )));
            }
            live.insert(
                id.clone(),
                Live {
                    outgoing,
                    stop: stop.clone(),
                },
            );
        }

        let connection = match self.connector.connect(&request, &stop) {
            Ok(connection) => connection,
            Err(error) => {
                forget(&self.live, &id);
                return Err(error.into());
            }
        };
        // Disconnect pressed just as the handshake finished.
        if stop.is_cancelled() {
            forget(&self.live, &id);
            return Err(AppError::Cancelled);
        }
        if !sink(connected(&request.url, connection.handshake())) {
            forget(&self.live, &id);
            return Ok(());
        }

        let owner = Owner {
            id: id.clone(),
            request,
            sink,
            incoming,
            stop,
            connector: self.connector.clone(),
            policy: self.policy,
            live: self.live.clone(),
        };
        let spawned = thread::Builder::new()
            .name(format!("websocket {id}"))
            .spawn(move || owner.run(connection));
        if let Err(error) = spawned {
            forget(&self.live, &id);
            return Err(AppError::Internal(format!(
                "could not start the connection thread: {error}"
            )));
        }
        Ok(())
    }

    /// Queues a message; the `Sent` event follows once it is on the wire, so
    /// the log can never show a message the server did not get.
    pub fn send(&self, id: &str, payload: WsPayload) -> Result<(), AppError> {
        let live = lock(&self.live);
        let entry = live.get(id).ok_or_else(|| not_connected(id))?;
        entry.outgoing.send(payload).map_err(|_| not_connected(id))
    }

    /// Returns at once: the connection closes on its own thread and reports
    /// `Closed`, after sending any message `send` already accepted. Cancels a handshake or a reconnect wait in progress too.
    /// Unknown ids are ignored, for the same reason `cancel_request` ignores
    /// them: a Disconnect racing a drop is not the user's problem.
    pub fn disconnect(&self, id: &str) {
        if let Some(entry) = lock(&self.live).get(id) {
            entry.stop.cancel();
        }
    }

    /// Disconnects every connection, each through its normal close. Called
    /// when the frontend starts: a webview reload leaves connections whose
    /// events go nowhere, because the old page's channel can still accept
    /// them on this side (PLAN-WEBSOCKET.md 13c, known limit). In a shipped
    /// build the page loads once, so this finds nothing to close.
    pub fn disconnect_all(&self) {
        for entry in lock(&self.live).values() {
            entry.stop.cancel();
        }
    }

    pub fn is_open(&self, id: &str) -> bool {
        lock(&self.live).contains_key(id)
    }
}

/// How one connection ended.
enum Ending {
    /// Deliberately, by one side or by a broken rule. Carries the final event.
    Closed(WsEvent),
    /// Unexpectedly — the one case that may reconnect.
    Lost(String),
    /// Nobody is listening; there is no one to tell.
    Silent,
}

/// The thread that owns a connection.
struct Owner {
    id: ConnectionId,
    request: WebSocketRequest,
    sink: EventSink,
    incoming: Receiver<WsPayload>,
    stop: CancellationToken,
    connector: Arc<dyn WebSocketConnector>,
    policy: ReconnectPolicy,
    live: Registry,
}

impl Owner {
    fn run(mut self, first: Box<dyn WebSocketConnection>) {
        let mut connection = first;
        let last = loop {
            match self.serve(connection.as_mut()) {
                Ending::Lost(reason)
                    if self.request.settings.auto_reconnect && !self.stop.is_cancelled() =>
                {
                    drop(connection);
                    if !self.emit(WsEvent::Error {
                        at_ms: now_ms(),
                        message: reason,
                    }) {
                        break None;
                    }
                    match self.reconnect() {
                        Ok(next) => connection = next,
                        Err(last) => break last,
                    }
                }
                Ending::Lost(reason) => break Some(closed(None, reason, ClosedBy::Error)),
                Ending::Closed(event) => break Some(event),
                Ending::Silent => break None,
            }
        };
        // Forgotten before the last event goes out, so a UI that reacts to
        // `Closed` by connecting again under the same id is never refused.
        forget(&self.live, &self.id);
        if let Some(event) = last {
            (self.sink)(event);
        }
    }

    fn emit(&mut self, event: WsEvent) -> bool {
        (self.sink)(event)
    }

    fn serve(&mut self, connection: &mut dyn WebSocketConnection) -> Ending {
        let mut assembler = Reassembler::new(self.request.settings.max_message_bytes);
        loop {
            // Read before draining, and acted on after. A Send the command
            // layer accepted before Disconnect is then guaranteed to be in
            // the channel, and goes out ahead of the close frame instead of
            // vanishing after it was reported as queued. The token is SeqCst,
            // so seeing the cancel means seeing everything queued before it.
            let stopping = self.stop.is_cancelled();
            while let Ok(payload) = self.incoming.try_recv() {
                let (kind, bytes) = frame_of(&payload);
                if let Err(error) = connection.send(kind, bytes) {
                    return Ending::Lost(error.to_string());
                }
                if !self.emit(WsEvent::Sent {
                    at_ms: now_ms(),
                    payload,
                }) {
                    return close_silently(connection);
                }
            }
            if stopping {
                let limit = self.request.settings.max_message_bytes;
                return close_by_user(connection, limit, &mut |payload| {
                    (self.sink)(WsEvent::Received {
                        at_ms: now_ms(),
                        payload,
                    })
                });
            }
            let chunk = match connection.poll(POLL_INTERVAL) {
                Ok(WsPoll::Chunk(chunk)) => chunk,
                Ok(WsPoll::Idle) => continue,
                Ok(WsPoll::Ended) => {
                    return Ending::Lost(
                        "the server dropped the connection without closing it".into(),
                    )
                }
                Err(error) => return Ending::Lost(error.to_string()),
            };
            let payload = match assembler.push(chunk) {
                Ok(None) => continue,
                Ok(Some(Assembled::Text(text))) => WsPayload::Text(text),
                Ok(Some(Assembled::Binary(bytes))) => WsPayload::Binary(bytes),
                // Pings reach us because automatic pong is off: libcurl only
                // flushes its own pong lazily, so a connection that is just
                // listening would never answer (PLAN.md Phase 13a, finding 1).
                // Not shown in the log.
                Ok(Some(Assembled::Ping(data))) => {
                    if let Err(error) = connection.send(FrameKind::Pong, &data) {
                        return Ending::Lost(error.to_string());
                    }
                    continue;
                }
                Ok(Some(Assembled::Pong(_))) => continue,
                Ok(Some(Assembled::Close(info))) => return answer_server_close(connection, info),
                Err(error) => {
                    let code = error.close_code();
                    let _ =
                        connection.send(FrameKind::Close, &close_payload(code, &error.reason()));
                    return Ending::Closed(closed(Some(code), error.reason(), ClosedBy::Error));
                }
            };
            if !self.emit(WsEvent::Received {
                at_ms: now_ms(),
                payload,
            }) {
                return close_silently(connection);
            }
        }
    }

    /// Err carries the event that ends the session, or None when nobody is
    /// listening.
    fn reconnect(&mut self) -> Result<Box<dyn WebSocketConnection>, Option<WsEvent>> {
        let max_attempts = self.policy.max_attempts;
        for attempt in 1..=max_attempts {
            let Some(delay) = self.policy.delay(attempt) else {
                break;
            };
            if !self.emit(WsEvent::Reconnecting {
                at_ms: now_ms(),
                attempt,
                max_attempts,
                delay,
            }) {
                return Err(None);
            }
            if !self.wait_unless_stopped(delay) {
                return Err(Some(closed(None, String::new(), ClosedBy::User)));
            }
            // Nothing typed while disconnected is sent later by surprise.
            while self.incoming.try_recv().is_ok() {}

            match self.connector.connect(&self.request, &self.stop) {
                Ok(connection) => {
                    if !self.emit(connected(&self.request.url, connection.handshake())) {
                        return Err(None);
                    }
                    return Ok(connection);
                }
                // A server that refuses the upgrade will refuse it again;
                // asking every few seconds would only hammer it.
                Err(refused @ WsConnectError::Refused { .. }) => {
                    let reason = AppError::from(refused).to_string();
                    return Err(Some(closed(None, reason, ClosedBy::Error)));
                }
                Err(WsConnectError::Cancelled) => {
                    return Err(Some(closed(None, String::new(), ClosedBy::User)));
                }
                Err(WsConnectError::Failed(message)) => {
                    if !self.emit(WsEvent::Error {
                        at_ms: now_ms(),
                        message,
                    }) {
                        return Err(None);
                    }
                }
            }
        }
        Err(Some(closed(
            None,
            format!("gave up after {max_attempts} reconnect attempts"),
            ClosedBy::Error,
        )))
    }

    /// False when Disconnect arrived during the wait.
    fn wait_unless_stopped(&self, delay: Duration) -> bool {
        let deadline = Instant::now() + delay;
        loop {
            if self.stop.is_cancelled() {
                return false;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return true;
            }
            thread::sleep(remaining.min(POLL_INTERVAL));
        }
    }
}

/// libcurl does not answer a server's close frame by itself (PLAN.md Phase
/// 13a, finding 2), so it is answered here, echoing the server's code as
/// RFC 6455 section 5.5.1 asks. The send can fail if the server has already
/// gone, and that is fine: the connection is over either way.
fn answer_server_close(connection: &mut dyn WebSocketConnection, info: CloseInfo) -> Ending {
    let reply = info
        .code
        .map(|code| close_payload(code, ""))
        .unwrap_or_default();
    let _ = connection.send(FrameKind::Close, &reply);
    Ending::Closed(closed(info.code, info.reason, ClosedBy::Server))
}

/// Disconnect: send a normal close, give the server a moment to answer, and
/// report its answer if it gave one.
///
/// Messages that arrive before the answer are still reported, through
/// `on_message`: RFC 6455 lets a peer keep sending until it has sent its own
/// close, and the log should show everything the server actually sent
/// (decided 2026-09-22, after the 13c smoke test lost two echoes this way).
/// `on_message` returning false means nobody is listening, and the wait ends.
fn close_by_user(
    connection: &mut dyn WebSocketConnection,
    limit: usize,
    on_message: &mut dyn FnMut(WsPayload) -> bool,
) -> Ending {
    let _ = connection.send(FrameKind::Close, &close_payload(CLOSE_NORMAL, ""));
    let Some(answer) = await_close_answer(connection, limit, on_message) else {
        return Ending::Silent;
    };
    Ending::Closed(closed(
        Some(answer.code.unwrap_or(CLOSE_NORMAL)),
        answer.reason,
        ClosedBy::User,
    ))
}

/// None when nobody is listening any more. A server that never answers, or
/// drops the connection, gets an empty answer when the wait ends.
fn await_close_answer(
    connection: &mut dyn WebSocketConnection,
    limit: usize,
    on_message: &mut dyn FnMut(WsPayload) -> bool,
) -> Option<CloseInfo> {
    let mut assembler = Reassembler::new(limit);
    let deadline = Instant::now() + CLOSE_TIMEOUT;
    while Instant::now() < deadline {
        let chunk = match connection.poll(POLL_INTERVAL) {
            Ok(WsPoll::Chunk(chunk)) => chunk,
            Ok(WsPoll::Idle) => continue,
            Ok(WsPoll::Ended) | Err(_) => break,
        };
        let payload = match assembler.push(chunk) {
            Ok(Some(Assembled::Close(info))) => return Some(info),
            Ok(Some(Assembled::Text(text))) => WsPayload::Text(text),
            Ok(Some(Assembled::Binary(bytes))) => WsPayload::Binary(bytes),
            // A fragment, or a ping or pong. Pings go unanswered: our close
            // is already on its way.
            Ok(_) => continue,
            // A message that breaks a rule while closing ends the wait; the
            // connection is being closed already, so there is nothing to add.
            Err(_) => break,
        };
        if !on_message(payload) {
            return None;
        }
    }
    Some(CloseInfo {
        code: None,
        reason: String::new(),
    })
}

/// Nobody to report to, but the server still deserves a close frame.
fn close_silently(connection: &mut dyn WebSocketConnection) -> Ending {
    let _ = connection.send(FrameKind::Close, &close_payload(CLOSE_NORMAL, ""));
    Ending::Silent
}

fn frame_of(payload: &WsPayload) -> (FrameKind, &[u8]) {
    match payload {
        WsPayload::Text(text) => (FrameKind::Text, text.as_bytes()),
        WsPayload::Binary(bytes) => (FrameKind::Binary, bytes.as_slice()),
    }
}

fn connected(url: &str, handshake: &WsHandshake) -> WsEvent {
    WsEvent::Connected {
        at_ms: now_ms(),
        url: url.to_string(),
        status: handshake.status,
        headers: handshake.headers.clone(),
    }
}

fn closed(code: Option<u16>, reason: String, by: ClosedBy) -> WsEvent {
    WsEvent::Closed {
        at_ms: now_ms(),
        code,
        reason,
        by,
    }
}

fn not_connected(id: &str) -> AppError {
    AppError::InvalidRequest(format!("connection {id} is not open"))
}

/// Case-insensitive, unlike the HTTP check: `WSS://` is a scheme libcurl
/// accepts, and refusing it would be a surprise with no benefit.
fn validate_ws_url(url: &str) -> Result<(), AppError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidRequest("URL is empty".into()));
    }
    let lowered = trimmed.to_ascii_lowercase();
    if SUPPORTED_SCHEMES
        .iter()
        .any(|scheme| lowered.starts_with(scheme))
    {
        return Ok(());
    }
    Err(AppError::InvalidRequest(
        "A WebSocket URL must start with ws:// or wss://".into(),
    ))
}

/// A poisoned lock means a connection thread panicked mid-update. The map is
/// only a registry of handles, so recovering it beats refusing every later
/// connection — the same reasoning as `SendRequest::lock_in_flight`.
fn lock(registry: &Registry) -> MutexGuard<'_, HashMap<ConnectionId, Live>> {
    registry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn forget(registry: &Registry, id: &str) {
    lock(registry).remove(id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{KeyValue, WebSocketSettings};
    use crate::domain::ws_frames::{FrameChunk, CLOSE_MESSAGE_TOO_BIG};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc::RecvTimeoutError;

    const WAIT: Duration = Duration::from_secs(3);

    type SentFrames = Arc<Mutex<Vec<(FrameKind, Vec<u8>)>>>;

    enum Step {
        Chunk(FrameChunk),
        Ended,
        Fail(&'static str),
        /// Nothing more arrives until the client has sent its close frame.
        AwaitClose,
    }

    /// A server that plays its script, then echoes whatever it is sent —
    /// data messages back as data, a close frame back as a close frame.
    struct Scripted {
        steps: VecDeque<Step>,
        sent: SentFrames,
        handshake: WsHandshake,
        close_sent: bool,
    }

    impl WebSocketConnection for Scripted {
        fn handshake(&self) -> &WsHandshake {
            &self.handshake
        }

        fn poll(&mut self, timeout: Duration) -> Result<WsPoll, AppError> {
            if matches!(self.steps.front(), Some(Step::AwaitClose)) {
                if !self.close_sent {
                    thread::sleep(timeout.min(Duration::from_millis(1)));
                    return Ok(WsPoll::Idle);
                }
                self.steps.pop_front();
            }
            match self.steps.pop_front() {
                Some(Step::Chunk(chunk)) => Ok(WsPoll::Chunk(chunk)),
                Some(Step::Ended) => Ok(WsPoll::Ended),
                Some(Step::Fail(message)) => Err(AppError::Transport(message.into())),
                Some(Step::AwaitClose) | None => {
                    thread::sleep(timeout.min(Duration::from_millis(1)));
                    Ok(WsPoll::Idle)
                }
            }
        }

        fn send(&mut self, kind: FrameKind, payload: &[u8]) -> Result<(), AppError> {
            self.sent
                .lock()
                .expect("spy lock poisoned")
                .push((kind, payload.to_vec()));
            if kind == FrameKind::Close {
                self.close_sent = true;
            }
            if matches!(kind, FrameKind::Text | FrameKind::Binary | FrameKind::Close) {
                self.steps.push_back(Step::Chunk(whole(kind, payload)));
            }
            Ok(())
        }
    }

    enum Outcome {
        Open(Vec<Step>),
        Refuse(u16),
        Fail(&'static str),
        /// Hangs until cancelled, like a server that never answers.
        HangUntilCancelled,
    }

    #[derive(Default)]
    struct ScriptedConnector {
        outcomes: Mutex<VecDeque<Outcome>>,
        sent: SentFrames,
        attempts: AtomicU32,
    }

    impl ScriptedConnector {
        fn new(outcomes: Vec<Outcome>) -> Arc<Self> {
            Arc::new(Self {
                outcomes: Mutex::new(outcomes.into()),
                ..Self::default()
            })
        }

        fn sent(&self) -> Vec<(FrameKind, Vec<u8>)> {
            self.sent.lock().expect("spy lock poisoned").clone()
        }
    }

    impl WebSocketConnector for ScriptedConnector {
        fn connect(
            &self,
            _request: &WebSocketRequest,
            cancel: &CancellationToken,
        ) -> Result<Box<dyn WebSocketConnection>, WsConnectError> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let outcome = self.outcomes.lock().expect("spy lock poisoned").pop_front();
            match outcome {
                Some(Outcome::Open(steps)) => Ok(Box::new(Scripted {
                    steps: steps.into(),
                    sent: self.sent.clone(),
                    handshake: WsHandshake {
                        status: 101,
                        headers: vec![KeyValue::new("Upgrade", "websocket")],
                        set_cookies: Vec::new(),
                    },
                    close_sent: false,
                })),
                Some(Outcome::Refuse(status)) => Err(WsConnectError::Refused { status }),
                Some(Outcome::Fail(message)) => Err(WsConnectError::Failed(message.into())),
                Some(Outcome::HangUntilCancelled) => {
                    let deadline = Instant::now() + WAIT;
                    while !cancel.is_cancelled() && Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(WsConnectError::Cancelled)
                }
                None => Err(WsConnectError::Failed(
                    "no more scripted connections".into(),
                )),
            }
        }
    }

    fn whole(kind: FrameKind, data: &[u8]) -> FrameChunk {
        FrameChunk {
            kind,
            more_fragments: false,
            bytes_left: 0,
            data: data.to_vec(),
        }
    }

    fn fast_policy() -> ReconnectPolicy {
        ReconnectPolicy {
            first_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(4),
            max_attempts: 3,
        }
    }

    fn request(settings: WebSocketSettings) -> WebSocketRequest {
        WebSocketRequest {
            url: "wss://echo.test/socket".into(),
            headers: Vec::new(),
            settings,
        }
    }

    fn events() -> (EventSink, Receiver<WsEvent>) {
        let (tx, rx) = mpsc::channel();
        (Box::new(move |event| tx.send(event).is_ok()), rx)
    }

    fn next(rx: &Receiver<WsEvent>) -> WsEvent {
        rx.recv_timeout(WAIT).expect("an event should arrive")
    }

    fn open(connector: &Arc<ScriptedConnector>) -> (WebSocketSessions, Receiver<WsEvent>) {
        open_with(connector, WebSocketSettings::default())
    }

    fn open_with(
        connector: &Arc<ScriptedConnector>,
        settings: WebSocketSettings,
    ) -> (WebSocketSessions, Receiver<WsEvent>) {
        let sessions = WebSocketSessions::with_policy(connector.clone(), fast_policy());
        let (sink, rx) = events();
        sessions
            .connect("c1".into(), request(settings), sink)
            .expect("should connect");
        (sessions, rx)
    }

    fn close_event(event: WsEvent) -> (Option<u16>, String, ClosedBy) {
        match event {
            WsEvent::Closed {
                code, reason, by, ..
            } => (code, reason, by),
            other => panic!("expected Closed, got {other:?}"),
        }
    }

    fn wait_until_forgotten(sessions: &WebSocketSessions) {
        let deadline = Instant::now() + WAIT;
        while sessions.is_open("c1") && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_connection_reports_its_handshake_then_echoes_a_message() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(Vec::new())]);
        let (sessions, rx) = open(&connector);

        match next(&rx) {
            WsEvent::Connected {
                url,
                status,
                headers,
                ..
            } => {
                assert_eq!(url, "wss://echo.test/socket");
                assert_eq!(status, 101);
                assert_eq!(headers[0].name, "Upgrade");
            }
            other => panic!("expected Connected, got {other:?}"),
        }

        sessions
            .send("c1", WsPayload::Text("hello".into()))
            .expect("should queue");

        assert!(matches!(
            next(&rx),
            WsEvent::Sent { payload: WsPayload::Text(ref text), .. } if text == "hello"
        ));
        assert!(matches!(
            next(&rx),
            WsEvent::Received { payload: WsPayload::Text(ref text), .. } if text == "hello"
        ));
        sessions.disconnect("c1");
    }

    #[test]
    fn a_binary_message_goes_out_as_a_binary_frame() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(Vec::new())]);
        let (sessions, rx) = open(&connector);
        let _connected = next(&rx);

        sessions
            .send("c1", WsPayload::Binary(vec![0xde, 0xad]))
            .expect("should queue");
        let _sent = next(&rx);
        let received = next(&rx);

        assert_eq!(connector.sent()[0], (FrameKind::Binary, vec![0xde, 0xad]));
        assert!(matches!(
            received,
            WsEvent::Received { payload: WsPayload::Binary(ref bytes), .. } if bytes == &[0xde, 0xad]
        ));
        sessions.disconnect("c1");
    }

    #[test]
    fn a_refused_handshake_is_an_error_and_reports_nothing() {
        let connector = ScriptedConnector::new(vec![Outcome::Refuse(401)]);
        let sessions = WebSocketSessions::with_policy(connector, fast_policy());
        let (sink, rx) = events();

        let error = sessions
            .connect("c1".into(), request(WebSocketSettings::default()), sink)
            .expect_err("should fail");

        assert_eq!(
            error.to_string(),
            "server refused the WebSocket upgrade: HTTP 401"
        );
        assert!(!sessions.is_open("c1"));
        assert!(matches!(
            rx.recv_timeout(Duration::from_millis(50)),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn disconnect_sends_a_normal_close_and_reports_the_servers_answer() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(Vec::new())]);
        let (sessions, rx) = open(&connector);
        let _connected = next(&rx);

        sessions.disconnect("c1");
        let (code, _, by) = close_event(next(&rx));

        assert_eq!(by, ClosedBy::User);
        assert_eq!(code, Some(CLOSE_NORMAL));
        assert_eq!(
            connector.sent(),
            vec![(FrameKind::Close, close_payload(CLOSE_NORMAL, ""))]
        );
        assert!(!sessions.is_open("c1"));
    }

    #[test]
    fn a_server_close_is_answered_with_the_same_code_and_reported() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(vec![Step::Chunk(whole(
            FrameKind::Close,
            &close_payload(1001, "bye"),
        ))])]);
        let (sessions, rx) = open(&connector);
        let _connected = next(&rx);

        let (code, reason, by) = close_event(next(&rx));

        assert_eq!(
            (code, reason.as_str(), by),
            (Some(1001), "bye", ClosedBy::Server)
        );
        assert_eq!(
            connector.sent(),
            vec![(FrameKind::Close, close_payload(1001, ""))]
        );
        assert!(!sessions.is_open("c1"));
    }

    #[test]
    fn a_ping_is_answered_with_a_pong_and_never_shown() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(vec![
            Step::Chunk(whole(FrameKind::Ping, b"p1")),
            Step::Chunk(whole(FrameKind::Text, b"after")),
        ])]);
        let (sessions, rx) = open(&connector);
        let _connected = next(&rx);

        let event = next(&rx);

        assert!(matches!(
            event,
            WsEvent::Received { payload: WsPayload::Text(ref text), .. } if text == "after"
        ));
        assert_eq!(connector.sent()[0], (FrameKind::Pong, b"p1".to_vec()));
        sessions.disconnect("c1");
    }

    #[test]
    fn an_oversized_message_closes_the_connection_with_1009() {
        let settings = WebSocketSettings {
            max_message_bytes: 4,
            ..WebSocketSettings::default()
        };
        let connector = ScriptedConnector::new(vec![Outcome::Open(vec![Step::Chunk(whole(
            FrameKind::Binary,
            &[0; 5],
        ))])]);
        let (sessions, rx) = open_with(&connector, settings);
        let _connected = next(&rx);

        let (code, _, by) = close_event(next(&rx));

        assert_eq!((code, by), (Some(CLOSE_MESSAGE_TOO_BIG), ClosedBy::Error));
        assert_eq!(connector.sent()[0].0, FrameKind::Close);
        assert_eq!(
            crate::domain::ws_frames::parse_close(&connector.sent()[0].1).code,
            Some(CLOSE_MESSAGE_TOO_BIG)
        );
        assert!(!sessions.is_open("c1"));
    }

    #[test]
    fn a_drop_with_reconnect_off_is_reported_as_closed_by_error() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(vec![Step::Ended])]);
        let (sessions, rx) = open(&connector);
        let _connected = next(&rx);

        let (code, _, by) = close_event(next(&rx));

        assert_eq!((code, by), (None, ClosedBy::Error));
        assert_eq!(connector.attempts.load(Ordering::SeqCst), 1);
        assert!(!sessions.is_open("c1"));
    }

    #[test]
    fn a_transport_failure_is_a_drop_too() {
        let connector =
            ScriptedConnector::new(vec![Outcome::Open(vec![Step::Fail("connection reset")])]);
        let (_sessions, rx) = open(&connector);
        let _connected = next(&rx);

        let (_, reason, by) = close_event(next(&rx));

        assert_eq!((reason.as_str(), by), ("connection reset", ClosedBy::Error));
    }

    fn reconnecting() -> WebSocketSettings {
        WebSocketSettings {
            auto_reconnect: true,
            ..WebSocketSettings::default()
        }
    }

    #[test]
    fn a_drop_with_reconnect_on_connects_again() {
        let connector = ScriptedConnector::new(vec![
            Outcome::Open(vec![Step::Ended]),
            Outcome::Open(Vec::new()),
        ]);
        let (sessions, rx) = open_with(&connector, reconnecting());
        let _connected = next(&rx);

        assert!(matches!(next(&rx), WsEvent::Error { .. }));
        assert!(matches!(
            next(&rx),
            WsEvent::Reconnecting {
                attempt: 1,
                max_attempts: 3,
                ..
            }
        ));
        assert!(matches!(next(&rx), WsEvent::Connected { .. }));

        sessions
            .send("c1", WsPayload::Text("again".into()))
            .expect("the same id should work after reconnecting");
        let _sent = next(&rx);
        assert!(matches!(next(&rx), WsEvent::Received { .. }));
        sessions.disconnect("c1");
    }

    /// A 401 must not be hammered: one refusal ends the session.
    #[test]
    fn a_refusal_while_reconnecting_stops_retrying() {
        let connector = ScriptedConnector::new(vec![
            Outcome::Open(vec![Step::Ended]),
            Outcome::Refuse(401),
            Outcome::Open(Vec::new()),
        ]);
        let (sessions, rx) = open_with(&connector, reconnecting());
        let _connected = next(&rx);
        let _error = next(&rx);
        let _reconnecting = next(&rx);

        let (_, reason, by) = close_event(next(&rx));

        assert_eq!(by, ClosedBy::Error);
        assert!(reason.contains("401"), "{reason}");
        assert_eq!(connector.attempts.load(Ordering::SeqCst), 2);
        wait_until_forgotten(&sessions);
        assert!(!sessions.is_open("c1"));
    }

    #[test]
    fn reconnecting_gives_up_after_the_last_attempt() {
        let connector = ScriptedConnector::new(vec![
            Outcome::Open(vec![Step::Ended]),
            Outcome::Fail("unreachable"),
            Outcome::Fail("unreachable"),
            Outcome::Fail("unreachable"),
        ]);
        let (_sessions, rx) = open_with(&connector, reconnecting());
        let mut last = next(&rx);
        while !matches!(last, WsEvent::Closed { .. }) {
            last = next(&rx);
        }

        let (_, reason, by) = close_event(last);

        assert_eq!(by, ClosedBy::Error);
        assert!(reason.contains("gave up after 3"), "{reason}");
        assert_eq!(connector.attempts.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn disconnect_during_a_reconnect_wait_ends_the_session_as_the_users_choice() {
        let connector = ScriptedConnector::new(vec![
            Outcome::Open(vec![Step::Ended]),
            Outcome::Open(Vec::new()),
        ]);
        let sessions = WebSocketSessions::with_policy(
            connector.clone(),
            ReconnectPolicy {
                first_delay: Duration::from_secs(2),
                max_delay: Duration::from_secs(2),
                max_attempts: 3,
            },
        );
        let (sink, rx) = events();
        sessions
            .connect("c1".into(), request(reconnecting()), sink)
            .expect("should connect");
        let _connected = next(&rx);
        let _error = next(&rx);
        let _reconnecting = next(&rx);

        sessions.disconnect("c1");
        let (_, _, by) = close_event(next(&rx));

        assert_eq!(by, ClosedBy::User);
        assert_eq!(connector.attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn disconnect_during_the_handshake_cancels_it() {
        let connector = ScriptedConnector::new(vec![Outcome::HangUntilCancelled]);
        let sessions = WebSocketSessions::with_policy(connector, fast_policy());
        let background = sessions.clone();
        let (sink, _rx) = events();
        let handshake = thread::spawn(move || {
            background.connect("c1".into(), request(WebSocketSettings::default()), sink)
        });
        let deadline = Instant::now() + WAIT;
        while !sessions.is_open("c1") && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }

        sessions.disconnect("c1");
        let result = handshake
            .join()
            .expect("the handshake thread should finish");

        assert!(matches!(result, Err(AppError::Cancelled)));
        assert!(!sessions.is_open("c1"));
    }

    #[test]
    fn when_nobody_listens_any_more_the_connection_closes() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(vec![Step::Chunk(whole(
            FrameKind::Text,
            b"unseen",
        ))])]);
        let sessions = WebSocketSessions::with_policy(connector.clone(), fast_policy());
        let delivered = Arc::new(AtomicU32::new(0));
        let counter = delivered.clone();
        // Accepts Connected, then reports the listener gone.
        let sink: EventSink = Box::new(move |_| counter.fetch_add(1, Ordering::SeqCst) == 0);
        sessions
            .connect("c1".into(), request(WebSocketSettings::default()), sink)
            .expect("should connect");

        wait_until_forgotten(&sessions);

        assert!(!sessions.is_open("c1"));
        assert_eq!(connector.sent()[0].0, FrameKind::Close);
    }

    #[test]
    fn the_same_id_cannot_be_connected_twice() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(Vec::new())]);
        let (sessions, _rx) = open(&connector);
        let (sink, _second) = events();

        let error = sessions
            .connect("c1".into(), request(WebSocketSettings::default()), sink)
            .expect_err("should refuse");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        sessions.disconnect("c1");
    }

    #[test]
    fn sending_to_a_connection_that_is_not_open_is_refused() {
        let sessions = WebSocketSessions::new(ScriptedConnector::new(Vec::new()));

        let error = sessions
            .send("nope", WsPayload::Text("x".into()))
            .expect_err("should refuse");

        assert!(matches!(error, AppError::InvalidRequest(_)));
    }

    #[test]
    fn only_ws_and_wss_urls_are_accepted() {
        assert!(validate_ws_url("ws://a.test").is_ok());
        assert!(validate_ws_url("  WSS://a.test/x").is_ok());
        assert!(matches!(
            validate_ws_url("https://a.test"),
            Err(AppError::InvalidRequest(_))
        ));
        assert!(matches!(
            validate_ws_url("   "),
            Err(AppError::InvalidRequest(_))
        ));
    }

    #[test]
    fn a_proxy_without_a_scheme_is_refused_before_connecting() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(Vec::new())]);
        let sessions = WebSocketSessions::new(connector.clone());
        let (sink, _rx) = events();
        let settings = WebSocketSettings {
            proxy: Some("127.0.0.1:8080".into()),
            ..WebSocketSettings::default()
        };

        let error = sessions
            .connect("c1".into(), request(settings), sink)
            .expect_err("should refuse");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert_eq!(connector.attempts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn reconnect_delays_double_from_the_first_and_stop_at_the_cap() {
        let policy = ReconnectPolicy::default();
        let delays: Vec<u64> = (1..=10)
            .map(|attempt| policy.delay(attempt).expect("within the limit").as_secs())
            .collect();

        assert_eq!(delays, vec![1, 2, 4, 8, 16, 30, 30, 30, 30, 30]);
    }

    #[test]
    fn there_is_no_delay_outside_the_attempt_range() {
        let policy = ReconnectPolicy::default();

        assert_eq!(policy.delay(0), None);
        assert_eq!(policy.delay(11), None);
        assert_eq!(
            ReconnectPolicy {
                max_attempts: 40,
                ..policy
            }
            .delay(40),
            Some(Duration::from_secs(30))
        );
    }

    /// Found by the PLAN.md Phase 13c smoke test: Send, Send, Disconnect in
    /// quick succession closed the connection without sending either
    /// message, though both sends had reported success. The sink sends and
    /// disconnects before the owner thread starts, so the old order (stop
    /// checked before the queue) fails this every time rather than by luck.
    #[test]
    fn messages_queued_before_disconnect_go_out_before_the_close() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(Vec::new())]);
        let sessions = WebSocketSessions::with_policy(connector.clone(), fast_policy());
        let (mut forward, rx) = events();
        let handle = sessions.clone();
        let sink: EventSink = Box::new(move |event| {
            if matches!(event, WsEvent::Connected { .. }) {
                for text in ["first", "second"] {
                    handle
                        .send("c1", WsPayload::Text(text.into()))
                        .expect("should queue");
                }
                handle.disconnect("c1");
            }
            forward(event)
        });
        sessions
            .connect("c1".into(), request(WebSocketSettings::default()), sink)
            .expect("should connect");

        assert!(matches!(next(&rx), WsEvent::Connected { .. }));
        for expected in ["first", "second"] {
            assert!(matches!(
                next(&rx),
                WsEvent::Sent { payload: WsPayload::Text(ref text), .. } if text == expected
            ));
        }
        // The scripted server echoes both before answering the close, and
        // the echoes are reported rather than dropped.
        for expected in ["first", "second"] {
            assert!(matches!(
                next(&rx),
                WsEvent::Received { payload: WsPayload::Text(ref text), .. } if text == expected
            ));
        }
        assert_eq!(close_event(next(&rx)).2, ClosedBy::User);
        let kinds: Vec<FrameKind> = connector.sent().into_iter().map(|(kind, _)| kind).collect();
        assert_eq!(
            kinds,
            vec![FrameKind::Text, FrameKind::Text, FrameKind::Close]
        );
    }

    /// Messages the server sends after Disconnect and before its close answer
    /// are logged. The script holds the late message back until the client's
    /// close frame has gone, so it can only arrive in that window.
    #[test]
    fn a_message_that_arrives_while_closing_is_reported() {
        let connector = ScriptedConnector::new(vec![Outcome::Open(vec![
            Step::AwaitClose,
            Step::Chunk(whole(FrameKind::Text, b"late")),
        ])]);
        let (sessions, rx) = open(&connector);
        let _connected = next(&rx);

        sessions.disconnect("c1");

        assert!(matches!(
            next(&rx),
            WsEvent::Received { payload: WsPayload::Text(ref text), .. } if text == "late"
        ));
        let (code, _, by) = close_event(next(&rx));
        assert_eq!((code, by), (Some(CLOSE_NORMAL), ClosedBy::User));
    }

    #[test]
    fn disconnect_all_closes_every_connection_as_the_users_choice() {
        let connector =
            ScriptedConnector::new(vec![Outcome::Open(Vec::new()), Outcome::Open(Vec::new())]);
        let sessions = WebSocketSessions::with_policy(connector.clone(), fast_policy());
        let (first_sink, first) = events();
        let (second_sink, second) = events();
        sessions
            .connect(
                "a".into(),
                request(WebSocketSettings::default()),
                first_sink,
            )
            .expect("should connect");
        sessions
            .connect(
                "b".into(),
                request(WebSocketSettings::default()),
                second_sink,
            )
            .expect("should connect");
        let _ = (next(&first), next(&second));

        sessions.disconnect_all();

        assert_eq!(close_event(next(&first)).2, ClosedBy::User);
        assert_eq!(close_event(next(&second)).2, ClosedBy::User);
        let deadline = Instant::now() + WAIT;
        while (sessions.is_open("a") || sessions.is_open("b")) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(!sessions.is_open("a") && !sessions.is_open("b"));
    }
}

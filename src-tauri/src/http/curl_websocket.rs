// http_client/src-tauri/src/http/curl_websocket.rs
//
// The libcurl implementation of the WebSocket ports (PLAN.md Phase 13b).
// The handshake is set up through the curl crate's Easy2, exactly as an HTTP
// request is, so TLS verification, the OS certificate store and the proxy
// behave identically on both paths. Only what the crate cannot express goes
// through http/curl_ws_ffi.rs — the one file allowed `unsafe`.
//
// Nothing here knows about Tauri, threads or the event log: this file opens a
// connection and moves frames; WebSocketSessions decides what they mean.
use std::thread;
use std::time::{Duration, Instant};

use curl::easy::{Easy2, Handler, HttpVersion, List, WriteError};

use crate::domain::cancellation::CancellationToken;
use crate::domain::error::AppError;
use crate::domain::models::{KeyValue, WebSocketRequest, WsHandshake};
use crate::domain::ports::{WebSocketConnection, WebSocketConnector, WsConnectError, WsPoll};
use crate::domain::ws_frames::{FrameChunk, FrameKind};
use crate::http::curl_client::apply_tls_and_proxy;
use crate::http::curl_ws_ffi::{
    self as ffi, Received, Written, CURLWS_BINARY, CURLWS_CLOSE, CURLWS_CONT, CURLWS_PING,
    CURLWS_PONG, CURLWS_TEXT,
};
use crate::http::mapping::{encode_url_for_send, parse_header_line, status_from_curl};

const SET_COOKIE: &str = "set-cookie";
const SWITCHING_PROTOCOLS: u16 = 101;

/// Large enough that most messages arrive in one piece; a bigger frame is
/// handed over in several and joined by the Reassembler.
const RECV_BUFFER_BYTES: usize = 64 * 1024;

/// How long one outgoing message may stay blocked before the connection is
/// treated as dead. Generous: an 8 MiB message to a slow reader took 1.5 s in
/// the 13a spike.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest single wait while a send is blocked, so a stalled socket is
/// re-checked against the deadline regularly.
const SEND_WAIT_SLICE: Duration = Duration::from_millis(250);

/// Used only if libcurl cannot say which socket it is on, which should not
/// happen once connected: sleep briefly instead of blocking on the socket.
const FALLBACK_WAIT: Duration = Duration::from_millis(10);

/// Collects the handshake's response headers and lets Disconnect abort a
/// handshake in progress through libcurl's progress callback, the same way
/// the HTTP client's Collector does.
struct Handshake {
    headers: Vec<KeyValue>,
    set_cookies: Vec<String>,
    cancel: CancellationToken,
}

impl Handler for Handshake {
    /// A refusal can carry a body ("unauthorized"); nobody reads it.
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        Ok(data.len())
    }

    fn header(&mut self, data: &[u8]) -> bool {
        // A proxy's "200 Connection established" comes first; keep only the
        // headers of the upgrade response itself.
        if data.starts_with(b"HTTP/") {
            self.headers.clear();
        }
        if let Some(header) = parse_header_line(data) {
            if header.name.eq_ignore_ascii_case(SET_COOKIE) {
                self.set_cookies.push(header.value.clone());
            }
            self.headers.push(header);
        }
        true
    }

    fn progress(&mut self, _dltotal: f64, _dlnow: f64, _ultotal: f64, _ulnow: f64) -> bool {
        !self.cancel.is_cancelled()
    }
}

#[derive(Default)]
pub struct CurlWebSocketConnector;

impl CurlWebSocketConnector {
    pub fn new() -> Self {
        Self
    }
}

impl WebSocketConnector for CurlWebSocketConnector {
    fn connect(
        &self,
        request: &WebSocketRequest,
        cancel: &CancellationToken,
    ) -> Result<Box<dyn WebSocketConnection>, WsConnectError> {
        let mut easy = Easy2::new(Handshake {
            headers: Vec::new(),
            set_cookies: Vec::new(),
            cancel: cancel.clone(),
        });
        configure(&mut easy, request).map_err(|error| WsConnectError::Failed(error.to_string()))?;
        ffi::enable_websocket_mode(&mut easy)
            .map_err(|code| WsConnectError::Failed(code.describe()))?;

        let performed = easy.perform();
        let status = status_from_curl(&easy).unwrap_or(0);
        if let Err(error) = performed {
            return Err(classify(&error, status, cancel.is_cancelled()));
        }
        // libcurl refuses a non-101 answer itself (13a, finding 3); checked
        // again here so nothing downstream ever holds a half-upgraded handle.
        if status != SWITCHING_PROTOCOLS {
            return Err(WsConnectError::Refused { status });
        }

        let collected = easy.get_mut();
        let handshake = WsHandshake {
            status,
            headers: std::mem::take(&mut collected.headers),
            set_cookies: std::mem::take(&mut collected.set_cookies),
        };
        Ok(Box::new(CurlWebSocketConnection {
            easy,
            handshake,
            buffer: vec![0; RECV_BUFFER_BYTES],
        }))
    }
}

fn configure(easy: &mut Easy2<Handshake>, request: &WebSocketRequest) -> Result<(), curl::Error> {
    easy.url(&encode_url_for_send(request.url.trim()))?;
    // The upgrade is an HTTP/1.1 mechanism. WebSockets over HTTP/2 (RFC 8441)
    // are out of scope for this phase.
    easy.http_version(HttpVersion::V11)?;
    // The handshake only. The overall timeout does not limit an open
    // WebSocket (13a, finding 4), so it is not set at all.
    easy.connect_timeout(request.settings.connect_timeout)?;
    easy.progress(true)?;
    apply_tls_and_proxy(
        easy,
        request.settings.verify_tls,
        request.settings.proxy.as_deref(),
    )?;

    let mut list = List::new();
    for header in &request.headers {
        let name = header.name.trim();
        if name.is_empty() {
            continue;
        }
        list.append(&format!("{name}: {}", header.value))?;
    }
    easy.http_headers(list)
}

/// A refused upgrade is reported with its status so the UI can name it and
/// reconnecting can stop; the status is still readable after libcurl's
/// `CURLE_HTTP_RETURNED_ERROR` (13a, finding 3).
fn classify(error: &curl::Error, status: u16, cancelled: bool) -> WsConnectError {
    if cancelled || error.is_aborted_by_callback() {
        WsConnectError::Cancelled
    } else if error.is_http_returned_error() && status != 0 {
        WsConnectError::Refused { status }
    } else {
        WsConnectError::Failed(error.to_string())
    }
}

struct CurlWebSocketConnection {
    easy: Easy2<Handshake>,
    handshake: WsHandshake,
    buffer: Vec<u8>,
}

impl WebSocketConnection for CurlWebSocketConnection {
    fn handshake(&self) -> &WsHandshake {
        &self.handshake
    }

    /// Drains libcurl before waiting: it may already hold decoded data, and
    /// waiting on the socket first would sit on a message that has arrived.
    fn poll(&mut self, timeout: Duration) -> Result<WsPoll, AppError> {
        if let Some(poll) = self.receive_once()? {
            return Ok(poll);
        }
        self.wait(false, timeout);
        Ok(self.receive_once()?.unwrap_or(WsPoll::Idle))
    }

    /// Follows libcurl's send contract (13a, finding 7): a short count means
    /// call again with the rest; CURLE_AGAIN means wait until writable and
    /// call again with the same bytes.
    fn send(&mut self, kind: FrameKind, payload: &[u8]) -> Result<(), AppError> {
        let flags = flags_for(kind);
        let deadline = Instant::now() + SEND_TIMEOUT;
        let mut offset = 0;
        loop {
            match ffi::send(&mut self.easy, &payload[offset..], flags) {
                Written::Bytes(count) => {
                    offset += count;
                    if offset >= payload.len() {
                        return Ok(());
                    }
                }
                Written::Again => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(AppError::Transport(format!(
                            "the server stopped accepting data for {} s",
                            SEND_TIMEOUT.as_secs()
                        )));
                    }
                    self.wait(true, remaining.min(SEND_WAIT_SLICE));
                }
                Written::Failed(code) => return Err(AppError::Transport(code.describe())),
            }
        }
    }
}

impl CurlWebSocketConnection {
    fn receive_once(&mut self) -> Result<Option<WsPoll>, AppError> {
        match ffi::receive(&mut self.easy, &mut self.buffer) {
            Received::Chunk {
                flags,
                bytes_left,
                len,
            } => {
                let kind = kind_of(flags).ok_or_else(|| {
                    AppError::Transport(format!("unexpected WebSocket frame flags {flags:#x}"))
                })?;
                Ok(Some(WsPoll::Chunk(FrameChunk {
                    kind,
                    more_fragments: flags & CURLWS_CONT != 0,
                    bytes_left,
                    data: self.buffer[..len].to_vec(),
                })))
            }
            Received::Again => Ok(None),
            Received::Closed => Ok(Some(WsPoll::Ended)),
            Received::Failed(code) => Err(AppError::Transport(code.describe())),
        }
    }

    fn wait(&mut self, for_write: bool, timeout: Duration) {
        match ffi::active_socket(&mut self.easy) {
            Some(socket) => {
                ffi::wait_for_socket(socket, for_write, timeout);
            }
            None => thread::sleep(timeout.min(FALLBACK_WAIT)),
        }
    }
}

/// Control flags first: libcurl sets exactly one type flag per frame, and
/// checking the rarer ones first costs nothing.
fn kind_of(flags: u32) -> Option<FrameKind> {
    [
        (CURLWS_CLOSE, FrameKind::Close),
        (CURLWS_PING, FrameKind::Ping),
        (CURLWS_PONG, FrameKind::Pong),
        (CURLWS_TEXT, FrameKind::Text),
        (CURLWS_BINARY, FrameKind::Binary),
    ]
    .into_iter()
    .find(|(bit, _)| flags & bit != 0)
    .map(|(_, kind)| kind)
}

fn flags_for(kind: FrameKind) -> u32 {
    match kind {
        FrameKind::Text => CURLWS_TEXT,
        FrameKind::Binary => CURLWS_BINARY,
        FrameKind::Close => CURLWS_CLOSE,
        FrameKind::Ping => CURLWS_PING,
        FrameKind::Pong => CURLWS_PONG,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [FrameKind; 5] = [
        FrameKind::Text,
        FrameKind::Binary,
        FrameKind::Close,
        FrameKind::Ping,
        FrameKind::Pong,
    ];

    #[test]
    fn every_kind_maps_to_its_flag_and_back() {
        for kind in ALL_KINDS {
            assert_eq!(kind_of(flags_for(kind)), Some(kind));
        }
    }

    /// What the 13a spike saw for a non-final fragment: the type flag plus
    /// CONT. The continuation is a separate fact, not a separate kind.
    #[test]
    fn a_continued_fragment_keeps_its_kind() {
        assert_eq!(kind_of(CURLWS_TEXT | CURLWS_CONT), Some(FrameKind::Text));
    }

    #[test]
    fn flags_with_no_type_are_not_a_frame() {
        assert_eq!(kind_of(0), None);
        assert_eq!(kind_of(CURLWS_CONT), None);
    }

    /// CURLE_HTTP_RETURNED_ERROR (22) with a status is a refusal; the same
    /// code with no status, or any other code, is a plain failure.
    #[test]
    fn a_refused_upgrade_is_told_apart_from_a_failure() {
        let http_error = curl::Error::new(22);

        assert_eq!(
            classify(&http_error, 401, false),
            WsConnectError::Refused { status: 401 }
        );
        assert!(matches!(
            classify(&http_error, 0, false),
            WsConnectError::Failed(_)
        ));
        assert!(matches!(
            classify(&curl::Error::new(7), 0, false),
            WsConnectError::Failed(_)
        ));
    }

    #[test]
    fn a_cancelled_handshake_is_cancelled_whatever_libcurl_said() {
        assert_eq!(
            classify(&curl::Error::new(42), 0, false),
            WsConnectError::Cancelled
        );
        assert_eq!(
            classify(&curl::Error::new(7), 0, true),
            WsConnectError::Cancelled
        );
    }
}

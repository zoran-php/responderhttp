// http_client/src-tauri/src/http/curl_grpc.rs
//
// The libcurl implementation of the gRPC transport port (PLAN-GRPC.md 16d),
// built on what the 16a spike proved on Windows:
//
// - One `Easy2` inside a `Multi`, driven by the thread that created it.
//   Neither type is `Send`, so the call never leaves that thread.
// - HTTP/2 with prior knowledge for cleartext, ALPN h2 for TLS.
// - The request body comes from the read callback. With nothing queued it
//   answers `Pause`. Unpausing is only valid once libcurl has actually
//   paused: before the transfer has a connection, `curl_easy_pause` fails
//   with CURLE_BAD_FUNCTION_ARGUMENT (16a, run 1, G06).
// - Response headers and trailers both arrive in the header callback. A
//   line after the blank line that ends the header block is a trailer (G02).
// - Cancel takes the handle out of the multi, which resets the stream (G07).
//
// This file moves bytes and headers. It knows nothing of protobuf, framing
// or gRPC status: GrpcCalls (16e) decides what the bytes mean.
use std::collections::VecDeque;
use std::time::Duration;

use curl::easy::{Easy2, Handler, HttpVersion, List, ReadError, WriteError};
use curl::multi::{Easy2Handle, Multi};

use crate::domain::error::AppError;
use crate::domain::grpc_wire::{check_auth, check_metadata, request_headers};
use crate::domain::models::{GrpcCallRequest, KeyValue};
use crate::domain::ports::{GrpcCall, GrpcTransport, GrpcWireEvent};
use crate::http::auth::strategy_for;
use crate::http::curl_client::apply_tls_and_proxy;
use crate::http::mapping::parse_header_line;

/// libcurl sends `Accept: */*` on every request unless told otherwise. It
/// is harmless, but it is not part of a gRPC request (16a, G08).
const DROP_ACCEPT: &str = "Accept:";

/// Collects what libcurl reports for one call, and feeds it the request
/// body. Everything it sees becomes an event, taken by `poll`.
#[derive(Default)]
struct Collector {
    outgoing: VecDeque<u8>,
    /// End Streaming was asked for: once `outgoing` is empty, the read
    /// callback answers 0 and libcurl ends the request stream.
    finished: bool,
    /// The read callback's last answer was `Pause`.
    read_paused: bool,
    status: u16,
    headers: Vec<KeyValue>,
    /// The header block has ended: further header lines are trailers.
    headers_done: bool,
    /// `Headers` has been reported. It is held back until the response
    /// proves to be the real one (see `on_header`).
    headers_reported: bool,
    trailers: Vec<KeyValue>,
    events: Vec<GrpcWireEvent>,
}

impl Collector {
    /// A proxy's `HTTP/1.1 200 Connection established` block comes before
    /// the server's own, and a `100 Continue` could too. So a block is only
    /// reported once body data or a trailer shows it was the last one, or
    /// the call ends. A new status line starts a new block.
    fn on_header(&mut self, line: &[u8]) {
        if line.starts_with(b"HTTP/") {
            self.status = status_code(line);
            self.headers.clear();
            self.headers_done = false;
            return;
        }
        match parse_header_line(line) {
            Some(header) if self.headers_done => {
                self.report_headers();
                self.trailers.push(header);
            }
            Some(header) => self.headers.push(header),
            None => {
                let blank = line.iter().all(|b| matches!(b, b'\r' | b'\n'));
                if blank && !self.headers_done {
                    self.headers_done = true;
                }
            }
        }
    }

    fn on_data(&mut self, data: &[u8]) {
        self.report_headers();
        self.events.push(GrpcWireEvent::Data(data.to_vec()));
    }

    /// The call is over: whatever is still held back is reported, then the
    /// end itself.
    fn on_end(&mut self, failure: Option<String>) {
        self.report_headers();
        if !self.trailers.is_empty() {
            self.events
                .push(GrpcWireEvent::Trailers(std::mem::take(&mut self.trailers)));
        }
        self.events.push(match failure {
            None => GrpcWireEvent::Ended,
            Some(message) => GrpcWireEvent::Failed(message),
        });
    }

    fn report_headers(&mut self) {
        if self.headers_reported || !self.headers_done {
            return;
        }
        self.headers_reported = true;
        self.events.push(GrpcWireEvent::Headers {
            status: self.status,
            headers: std::mem::take(&mut self.headers),
        });
    }

    fn fill(&mut self, into: &mut [u8]) -> Result<usize, ReadError> {
        if self.outgoing.is_empty() {
            if self.finished {
                return Ok(0);
            }
            self.read_paused = true;
            return Err(ReadError::Pause);
        }
        let n = into.len().min(self.outgoing.len());
        for (slot, byte) in into.iter_mut().zip(self.outgoing.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }
}

/// `HTTP/2 200` → 200. Anything unreadable is 0, which no status mapping
/// treats as success.
fn status_code(line: &[u8]) -> u16 {
    String::from_utf8_lossy(line)
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.on_data(data);
        Ok(data.len())
    }

    fn read(&mut self, into: &mut [u8]) -> Result<usize, ReadError> {
        self.fill(into)
    }

    fn header(&mut self, line: &[u8]) -> bool {
        self.on_header(line);
        true
    }
}

#[derive(Default)]
pub struct CurlGrpcTransport;

impl CurlGrpcTransport {
    pub fn new() -> Self {
        Self
    }
}

impl GrpcTransport for CurlGrpcTransport {
    fn open(&self, request: &GrpcCallRequest) -> Result<Box<dyn GrpcCall>, AppError> {
        let headers = headers_for(request)?;
        let mut easy = Easy2::new(Collector::default());
        configure(&mut easy, request, &headers).map_err(|error| transport_error(&error))?;
        let multi = Multi::new();
        let handle = multi
            .add2(easy)
            .map_err(|error| AppError::Transport(error.to_string()))?;
        Ok(Box::new(CurlGrpcCall {
            multi,
            handle: Some(handle),
            leftover: Vec::new(),
            tls: request.target.tls,
        }))
    }
}

/// The protocol headers, then the user's metadata and what auth adds,
/// checked together: an auth header is metadata like any other, and a
/// reserved name is refused whichever of the two it came from.
fn headers_for(request: &GrpcCallRequest) -> Result<Vec<KeyValue>, AppError> {
    let invalid =
        |error: crate::domain::grpc_wire::WireError| AppError::InvalidRequest(error.to_string());
    check_auth(&request.auth).map_err(invalid)?;
    let mut metadata = request.metadata.clone();
    metadata.extend(strategy_for(&request.auth).apply().headers);
    let metadata = check_metadata(&metadata).map_err(invalid)?;
    Ok(request_headers(&metadata, request.settings.deadline))
}

fn configure(
    easy: &mut Easy2<Collector>,
    request: &GrpcCallRequest,
    headers: &[KeyValue],
) -> Result<(), curl::Error> {
    let (scheme, version) = if request.target.tls {
        ("https", HttpVersion::V2TLS)
    } else {
        ("http", HttpVersion::V2PriorKnowledge)
    };
    easy.url(&format!(
        "{scheme}://{}{}",
        request.target.authority.trim(),
        request.path
    ))?;
    easy.http_version(version)?;
    // POST with no size: the body is read, message by message, for as long
    // as the call lasts.
    easy.post(true)?;
    // Connecting only. No CURLOPT_TIMEOUT: a stream may rightly run for
    // hours, and the deadline is enforced by GrpcCalls, which can end the
    // call with DEADLINE_EXCEEDED rather than a transport error.
    easy.connect_timeout(request.settings.connect_timeout)?;
    easy.signal(false)?;
    apply_tls_and_proxy(
        easy,
        request.settings.verify_tls,
        request.settings.proxy.as_deref(),
    )?;

    let mut list = List::new();
    for header in headers {
        list.append(&format!("{}: {}", header.name, header.value))?;
    }
    list.append(DROP_ACCEPT)?;
    easy.http_headers(list)
}

fn transport_error(error: &curl::Error) -> AppError {
    AppError::Transport(error.to_string())
}

/// Why a call failed, as the UI shows it. The two failures a wrong TLS
/// setting produces are libcurl's least readable ones, so they get a hint
/// (found in the manual click-through, 16j): TLS against a plain-text port
/// fails the handshake (CURLE_SSL_CONNECT_ERROR), and plain-text HTTP/2
/// against a TLS port fails in the framing layer (CURLE_HTTP2). Worded as
/// "if", because a real TLS or HTTP/2 fault gives the same codes.
fn failure_text(error: &curl::Error, tls: bool) -> String {
    /// libcurl's CURLE_HTTP2. Compared by number: the `curl` crate's
    /// predicate for it is not something this code should depend on. A `u8`
    /// widened with `into()`, because `CURLcode` is `i32` on MSVC and `u32`
    /// on Linux, and a typed constant compiles on only one of them; both
    /// sides are compared as `i64`, which either type widens into.
    const CURLE_HTTP2: u8 = 16;
    let hint = if tls && error.is_ssl_connect_error() {
        Some("The TLS handshake failed. If the server uses plain text (h2c), turn TLS off.")
    } else if !tls && i64::from(error.code()) == i64::from(CURLE_HTTP2) {
        Some("The server did not answer plain-text HTTP/2. If it uses TLS, turn TLS on.")
    } else {
        None
    };
    match hint {
        Some(hint) => format!("{hint} ({error})"),
        None => error.to_string(),
    }
}

struct CurlGrpcCall {
    multi: Multi,
    /// None once the call has ended or been cancelled.
    handle: Option<Easy2Handle<Collector>>,
    /// Events collected when the call ended inside `send` or `end_stream`,
    /// handed out by the next `poll`.
    leftover: Vec<GrpcWireEvent>,
    /// Whether the call speaks TLS, for the hint in `failure_text`.
    tls: bool,
}

impl CurlGrpcCall {
    /// Lets libcurl do what it can without blocking, then collects a
    /// finished transfer. Returns true while the call is still running.
    fn drive(&mut self) -> Result<bool, AppError> {
        let Some(handle) = self.handle.as_ref() else {
            return Ok(false);
        };
        self.multi
            .perform()
            .map_err(|error| AppError::Transport(error.to_string()))?;
        let mut finished = None;
        self.multi.messages(|message| {
            if let Some(result) = message.result_for2(handle) {
                finished = Some(result);
            }
        });
        match finished {
            None => Ok(true),
            Some(result) => {
                let tls = self.tls;
                self.finish(result.err().map(|error| failure_text(&error, tls)))?;
                Ok(false)
            }
        }
    }

    fn finish(&mut self, failure: Option<String>) -> Result<(), AppError> {
        if let Some(handle) = self.handle.take() {
            let mut easy = self
                .multi
                .remove2(handle)
                .map_err(|error| AppError::Transport(error.to_string()))?;
            let collector = easy.get_mut();
            collector.on_end(failure);
            self.leftover.append(&mut collector.events);
        }
        Ok(())
    }

    fn take_events(&mut self) -> Vec<GrpcWireEvent> {
        let mut events = std::mem::take(&mut self.leftover);
        if let Some(handle) = self.handle.as_mut() {
            events.append(&mut handle.get_mut().events);
        }
        events
    }

    /// Unpauses only a read side that libcurl has actually paused (16a, run
    /// 1). The flag is cleared first, because unpausing may call the read
    /// callback at once, and that call may pause again. Then libcurl is
    /// driven straight away, so the message leaves now rather than on the
    /// next turn of the owner loop.
    fn resume(&mut self) -> Result<(), AppError> {
        if let Some(handle) = self.handle.as_mut() {
            if handle.get_ref().read_paused {
                handle.get_mut().read_paused = false;
                handle
                    .unpause_read()
                    .map_err(|error| transport_error(&error))?;
            }
        }
        self.drive().map(|_| ())
    }

    fn collector_mut(&mut self) -> Result<&mut Collector, AppError> {
        self.handle
            .as_mut()
            .map(Easy2Handle::get_mut)
            .ok_or_else(|| AppError::InvalidRequest("the call has already ended".to_string()))
    }
}

impl GrpcCall for CurlGrpcCall {
    fn send(&mut self, framed: Vec<u8>) -> Result<(), AppError> {
        let collector = self.collector_mut()?;
        if collector.finished {
            return Err(AppError::InvalidRequest(
                "the request stream has already been ended".to_string(),
            ));
        }
        collector.outgoing.extend(framed);
        self.resume()
    }

    fn end_stream(&mut self) -> Result<(), AppError> {
        self.collector_mut()?.finished = true;
        self.resume()
    }

    fn poll(&mut self, timeout: Duration) -> Result<Vec<GrpcWireEvent>, AppError> {
        let running = self.drive()?;
        let events = self.take_events();
        if !events.is_empty() || !running {
            return Ok(events);
        }
        // libcurl may need to act sooner than `timeout` (a connect or TLS
        // step, a flow-control update), and says so through its own timer.
        let wait = match self.multi.get_timeout() {
            Ok(Some(libcurl)) => timeout.min(libcurl),
            _ => timeout,
        };
        self.multi
            .wait(&mut [], wait)
            .map_err(|error| AppError::Transport(error.to_string()))?;
        self.drive()?;
        Ok(self.take_events())
    }

    fn cancel(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Removing the handle mid-transfer resets the stream. One more
            // turn lets libcurl write the RST_STREAM before the multi, and
            // with it the connection, is dropped.
            let _ = self.multi.remove2(handle);
            let _ = self.multi.perform();
        }
        self.leftover.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CURLE_COULDNT_CONNECT, CURLE_HTTP2 and CURLE_SSL_CONNECT_ERROR, as
    /// `u8` for the same reason as `failure_text`'s constant.
    const COULDNT_CONNECT: u8 = 7;
    const HTTP2: u8 = 16;
    const SSL_CONNECT: u8 = 35;

    #[test]
    fn a_failed_handshake_with_tls_on_suggests_turning_it_off() {
        let text = failure_text(&curl::Error::new(SSL_CONNECT.into()), true);
        assert!(text.starts_with("The TLS handshake failed."), "{text}");
        assert!(text.contains("turn TLS off"), "{text}");
    }

    #[test]
    fn a_framing_error_with_tls_off_suggests_turning_it_on() {
        let text = failure_text(&curl::Error::new(HTTP2.into()), false);
        assert!(text.contains("turn TLS on"), "{text}");
    }

    #[test]
    fn other_failures_are_libcurls_own_text() {
        for (code, tls) in [(COULDNT_CONNECT, true), (SSL_CONNECT, false), (HTTP2, true)] {
            let error = curl::Error::new(code.into());
            assert_eq!(failure_text(&error, tls), error.to_string());
        }
    }

    fn feed(collector: &mut Collector, lines: &[&str]) {
        for line in lines {
            collector.on_header(format!("{line}\r\n").as_bytes());
        }
    }

    #[test]
    fn a_normal_response_reports_headers_then_data_then_trailers() {
        let mut c = Collector::default();
        feed(
            &mut c,
            &["HTTP/2 200", "content-type: application/grpc", ""],
        );
        c.on_data(b"abc");
        feed(&mut c, &["grpc-status: 0"]);
        c.on_end(None);

        assert_eq!(
            c.events,
            vec![
                GrpcWireEvent::Headers {
                    status: 200,
                    headers: vec![KeyValue::new("content-type", "application/grpc")],
                },
                GrpcWireEvent::Data(b"abc".to_vec()),
                GrpcWireEvent::Trailers(vec![KeyValue::new("grpc-status", "0")]),
                GrpcWireEvent::Ended,
            ]
        );
    }

    #[test]
    fn a_trailers_only_response_keeps_the_status_in_the_headers() {
        let mut c = Collector::default();
        feed(
            &mut c,
            &[
                "HTTP/2 200",
                "content-type: application/grpc",
                "grpc-status: 5",
                "",
            ],
        );
        c.on_end(None);

        assert!(matches!(
            &c.events[0],
            GrpcWireEvent::Headers { headers, .. } if headers.iter().any(|h| h.name == "grpc-status")
        ));
        assert_eq!(c.events[1], GrpcWireEvent::Ended);
    }

    #[test]
    fn a_proxy_connect_block_is_not_reported_as_the_response() {
        let mut c = Collector::default();
        feed(&mut c, &["HTTP/1.1 200 Connection established", ""]);
        feed(
            &mut c,
            &["HTTP/2 200", "content-type: application/grpc", ""],
        );
        c.on_data(b"x");

        assert_eq!(
            c.events[0],
            GrpcWireEvent::Headers {
                status: 200,
                headers: vec![KeyValue::new("content-type", "application/grpc")],
            }
        );
    }

    #[test]
    fn a_failure_after_headers_reports_them_first() {
        let mut c = Collector::default();
        feed(
            &mut c,
            &["HTTP/2 200", "content-type: application/grpc", ""],
        );
        c.on_end(Some("connection reset".to_string()));

        assert!(matches!(
            c.events[0],
            GrpcWireEvent::Headers { status: 200, .. }
        ));
        assert_eq!(
            c.events[1],
            GrpcWireEvent::Failed("connection reset".to_string())
        );
    }

    #[test]
    fn a_failure_before_any_response_is_the_only_event() {
        let mut c = Collector::default();
        c.on_end(Some("could not connect".to_string()));

        assert_eq!(
            c.events,
            vec![GrpcWireEvent::Failed("could not connect".to_string())]
        );
    }

    #[test]
    fn the_read_side_pauses_when_empty_and_ends_when_finished() {
        let mut c = Collector::default();
        let mut buffer = [0u8; 4];

        assert!(matches!(c.fill(&mut buffer), Err(ReadError::Pause)));
        assert!(c.read_paused);

        c.outgoing.extend([1, 2, 3, 4, 5, 6]);
        assert_eq!(c.fill(&mut buffer).ok(), Some(4));
        assert_eq!(buffer, [1, 2, 3, 4]);
        assert_eq!(c.fill(&mut buffer).ok(), Some(2));

        c.finished = true;
        assert_eq!(c.fill(&mut buffer).ok(), Some(0));
    }

    #[test]
    fn status_lines_are_read_and_nonsense_is_zero() {
        assert_eq!(status_code(b"HTTP/2 200\r\n"), 200);
        assert_eq!(status_code(b"HTTP/1.1 404 Not Found\r\n"), 404);
        assert_eq!(status_code(b"HTTP/2\r\n"), 0);
    }
}

// http_client/src-tauri/tests/sse.rs
//
// The streaming half of the libcurl client (PLAN-SSE.md, 14b), against a
// local server that writes an event stream a piece at a time. httpmock
// answers in one go, which is exactly what these tests must not do.
//
// The server is a few lines of std here rather than a dependency, for the
// same reason tests/support/ws_server.rs is.
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use responderhttp_lib::domain::cancellation::CancellationToken;
use responderhttp_lib::domain::error::AppError;
use responderhttp_lib::domain::models::{
    Auth, HttpMethod, HttpRequest, HttpResponse, RequestBody, RequestSettings, ResponseBody,
};
use responderhttp_lib::domain::ports::{HttpClient, HttpStreamUpdate};
use responderhttp_lib::domain::sse::{SseBlock, SseBlockKind};
use responderhttp_lib::http::curl_client::CurlClient;

/// Long enough that a test never races the server, short enough that a
/// broken one fails quickly.
const STEP: Duration = Duration::from_millis(120);

struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("should bind");
        let port = listener
            .local_addr()
            .expect("should have an address")
            .port();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();

        thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    return;
                }
                match stream {
                    Ok(stream) => {
                        thread::spawn(move || serve(stream));
                    }
                    Err(_) => return,
                }
            }
        });

        Self { port, stop }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblocks the accept loop so the thread can see the flag.
        let _ = std::net::TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn serve(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("should clone"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => {}
            Err(_) => return,
        }
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");

    match path {
        "/stream" => {
            let _ = write_events_head(&mut stream);
            for line in [
                ": keep-alive\n\n",
                "data: A\n\n",
                "data: group\n\n",
                "event: end\nid: 7\ndata: Stream ended\n\n",
            ] {
                if stream.write_all(line.as_bytes()).is_err() || stream.flush().is_err() {
                    return;
                }
                thread::sleep(STEP / 4);
            }
        }
        // One event, a pause longer than the request timeout, then the rest.
        "/slow" => {
            let _ = write_events_head(&mut stream);
            let _ = stream.write_all(b"data: first\n\n");
            let _ = stream.flush();
            thread::sleep(STEP * 5);
            let _ = stream.write_all(b"data: last\n\n");
            let _ = stream.flush();
        }
        // Ends without the closing blank line.
        "/cut" => {
            let _ = write_events_head(&mut stream);
            let _ = stream.write_all(b"data: half");
            let _ = stream.flush();
        }
        // Says it is a stream and sends nothing at all.
        "/silent" => {
            let _ = write_events_head(&mut stream);
            thread::sleep(STEP);
        }
        // Never answers, so the request has to time out.
        "/hang" => {
            thread::sleep(Duration::from_secs(30));
        }
        _ => {
            let body = "plain body";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.flush();
        }
    }
}

fn write_events_head(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
    )?;
    stream.flush()
}

fn request(url: String) -> HttpRequest {
    HttpRequest {
        method: HttpMethod::Get,
        url,
        headers: Vec::new(),
        query_params: Vec::new(),
        body: RequestBody::None,
        auth: Auth::None,
        settings: RequestSettings::default(),
    }
}

/// Sends and collects everything reported on the way.
fn stream(request: &HttpRequest) -> (Result<HttpResponse, AppError>, Vec<HttpStreamUpdate>) {
    let mut seen = Vec::new();
    let result =
        CurlClient::new().send_streaming(request, &CancellationToken::new(), &mut |update| {
            seen.push(update);
            true
        });
    (result, seen)
}

fn events(updates: &[HttpStreamUpdate]) -> Vec<(String, String, Option<String>)> {
    updates
        .iter()
        .filter_map(|update| match update {
            HttpStreamUpdate::Block {
                block:
                    SseBlock {
                        kind: SseBlockKind::Event { name, data, id, .. },
                        ..
                    },
                ..
            } => Some((name.clone(), data.clone(), id.clone())),
            _ => None,
        })
        .collect()
}

fn text(response: &HttpResponse) -> String {
    match &response.body {
        ResponseBody::Text(text) => text.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn an_event_stream_is_reported_block_by_block_and_still_returns_a_response() {
    let server = Server::start();

    let (result, updates) = stream(&request(server.url("/stream")));

    let response = result.expect("the stream should finish");
    assert_eq!(response.status, 200);
    // The headers arrive first, long before the body is complete.
    match &updates[0] {
        HttpStreamUpdate::Headers {
            status,
            headers,
            bytes,
        } => {
            assert_eq!(*status, 200);
            assert!(headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("content-type")));
            // The status line, three headers and the blank line.
            assert!(*bytes > 60, "header block measured {bytes} bytes");
        }
        other => panic!("expected headers first, got {other:?}"),
    }
    assert!(matches!(
        &updates[1],
        HttpStreamUpdate::Block {
            block: SseBlock {
                kind: SseBlockKind::Comment { .. },
                ..
            },
            ..
        }
    ));
    assert_eq!(
        events(&updates),
        vec![
            ("message".to_string(), "A".to_string(), None),
            ("message".to_string(), "group".to_string(), None),
            (
                "end".to_string(),
                "Stream ended".to_string(),
                Some("7".to_string())
            ),
        ]
    );
    // The response still carries the stream as it arrived.
    assert!(text(&response).contains("data: group"));
}

/// Decision D3: the timeout guards getting a response, not keeping one.
#[test]
fn a_stream_is_not_cut_off_by_the_request_timeout() {
    let server = Server::start();
    let mut slow = request(server.url("/slow"));
    slow.settings.timeout = STEP;

    let (result, updates) = stream(&slow);

    result.expect("a stream must outlive the timeout");
    assert_eq!(
        events(&updates)
            .into_iter()
            .map(|(_, data, _)| data)
            .collect::<Vec<_>>(),
        vec!["first".to_string(), "last".to_string()]
    );
}

/// The other half of D3: a request that is not a stream still times out.
#[test]
fn a_request_that_never_answers_still_times_out() {
    let server = Server::start();
    let mut hanging = request(server.url("/hang"));
    hanging.settings.timeout = STEP;
    let started = Instant::now();

    let (result, updates) = stream(&hanging);

    let error = result.expect_err("a hanging request should fail");
    assert!(
        matches!(&error, AppError::Transport(message) if message.contains("timed out")),
        "got {error:?}"
    );
    assert!(started.elapsed() < STEP * 10, "waited too long");
    assert!(updates.is_empty());
}

#[test]
fn a_response_that_is_not_an_event_stream_reports_only_its_headers() {
    let server = Server::start();

    let (result, updates) = stream(&request(server.url("/plain")));

    let response = result.expect("should send");
    assert_eq!(text(&response), "plain body");
    assert_eq!(updates.len(), 1);
    assert!(matches!(updates[0], HttpStreamUpdate::Headers { .. }));
}

#[test]
fn a_stream_cut_off_mid_block_still_reports_what_arrived() {
    let server = Server::start();

    let (result, updates) = stream(&request(server.url("/cut")));

    result.expect("should send");
    assert_eq!(
        events(&updates)
            .into_iter()
            .map(|(_, data, _)| data)
            .collect::<Vec<_>>(),
        vec!["half".to_string()]
    );
}

#[test]
fn a_stream_that_sends_nothing_reports_nothing_but_its_headers() {
    let server = Server::start();

    let (result, updates) = stream(&request(server.url("/silent")));

    result.expect("should send");
    assert_eq!(updates.len(), 1);
}

/// A sink that says it has gone stops the transfer rather than leaving it
/// running with nobody to report to.
#[test]
fn a_sink_that_stops_listening_ends_the_transfer() {
    let server = Server::start();
    let mut seen = 0;

    let result = CurlClient::new().send_streaming(
        &request(server.url("/stream")),
        &CancellationToken::new(),
        &mut |_| {
            seen += 1;
            false
        },
    );

    assert!(result.is_err(), "the transfer should have been abandoned");
    assert_eq!(seen, 1);
}

#[test]
fn cancelling_a_stream_stops_it() {
    let server = Server::start();
    let cancel = CancellationToken::new();
    let mut seen = 0;

    let result =
        CurlClient::new().send_streaming(&request(server.url("/stream")), &cancel, &mut |_| {
            seen += 1;
            cancel.cancel();
            true
        });

    assert!(result.is_err(), "a cancelled stream should fail");
    assert!(seen <= 2, "kept reporting after cancel: {seen}");
}

/// The size shown while a stream runs has to be the size shown once it
/// ends, or the badge would jump at the last moment. The two are counted in
/// the same place for exactly that reason.
#[test]
fn the_running_size_and_the_finished_one_agree() {
    let server = Server::start();

    let (result, updates) = stream(&request(server.url("/stream")));

    let response = result.expect("the stream should finish");
    let reported_headers = updates.iter().find_map(|update| match update {
        HttpStreamUpdate::Headers { bytes, .. } => Some(*bytes),
        HttpStreamUpdate::Block { .. } => None,
    });
    let reported_body: u64 = updates
        .iter()
        .filter_map(|update| match update {
            HttpStreamUpdate::Block { block, .. } => Some(block.raw.len() as u64),
            HttpStreamUpdate::Headers { .. } => None,
        })
        .sum();

    assert_eq!(reported_headers, Some(response.sizes.response_headers));
    assert_eq!(reported_body, response.sizes.response_body);
    // The request half is libcurl's to report, and only once it has sent it.
    assert!(response.sizes.request_headers > 0);
    assert_eq!(response.sizes.request_body, 0);
}

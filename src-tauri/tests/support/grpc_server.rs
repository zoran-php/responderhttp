// http_client/src-tauri/tests/support/grpc_server.rs
//
// A local HTTP/2 cleartext (h2c) server that speaks just enough gRPC for the
// transport's integration tests (PLAN-GRPC.md 16d, D7 as revised). Ported
// from the 16a spike, where it was proven against the app's libcurl on
// Windows. Built on the h2 crate directly, rather than a gRPC framework, so
// it can misbehave on purpose and log exactly which headers arrived: the
// client cannot see either from its side.
//
// Messages are framed with the app's own `grpc_wire`, so the tests also
// check that the framing agrees with a real HTTP/2 peer.
//
// Every path is one behaviour:
//
//   /test.Echo/Unary         read every message, echo them all, status 0
//   /test.Echo/ServerStream  first message "count,interval_ms"; send that
//                            many messages that far apart, status 0
//   /test.Echo/ClientStream  count messages until END_STREAM, answer once
//   /test.Echo/Bidi          echo each message as it arrives
//   /test.Echo/Slow          a message every 100 ms until the client resets
//   /test.Echo/Fail          trailers-only: grpc-status 5 in the headers
//   /notgrpc                 plain HTTP 404 with a text body
//   anything else            trailers-only UNIMPLEMENTED (12)
use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use h2::server::{self, SendResponse};
use h2::{RecvStream, SendStream};
use http::{HeaderMap, HeaderValue, Request, Response};
use tokio::net::{TcpListener, TcpStream};

use responderhttp_lib::domain::grpc_wire::{frame, FrameDecoder, MAX_RECEIVE_BYTES_CAP};

const SLOW_INTERVAL: Duration = Duration::from_millis(100);
const SLOW_MAX_MESSAGES: usize = 200;

#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    fn push(&self, line: String) {
        if let Ok(mut lines) = self.0.lock() {
            lines.push(line);
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.0.lock().map(|lines| lines.clone()).unwrap_or_default()
    }

    pub fn find(&self, needle: &str) -> Option<String> {
        self.lines().into_iter().find(|line| line.contains(needle))
    }
}

pub struct GrpcServer {
    pub port: u16,
    pub log: Log,
}

impl GrpcServer {
    pub fn start() -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let log = Log::default();
        let server_log = log.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        std::thread::spawn(move || {
            runtime.block_on(async move {
                let listener = match TcpListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(error) => {
                        server_log.push(format!("listener: {error}"));
                        return;
                    }
                };
                loop {
                    match listener.accept().await {
                        Ok((socket, _)) => {
                            tokio::spawn(connection(socket, server_log.clone()));
                        }
                        Err(error) => server_log.push(format!("accept: {error}")),
                    }
                }
            });
        });
        Ok(Self { port, log })
    }
}

async fn connection(socket: TcpStream, log: Log) {
    let mut connection = match server::handshake(socket).await {
        Ok(connection) => connection,
        Err(error) => {
            log.push(format!("h2 handshake failed: {error}"));
            return;
        }
    };
    while let Some(next) = connection.accept().await {
        match next {
            Ok((request, respond)) => {
                tokio::spawn(stream(request, respond, log.clone()));
            }
            Err(error) => {
                log.push(format!("connection ended: {error}"));
                return;
            }
        }
    }
}

async fn stream(request: Request<RecvStream>, mut respond: SendResponse<Bytes>, log: Log) {
    let path = request.uri().path().to_string();
    let headers: Vec<String> = request
        .headers()
        .iter()
        .map(|(name, value)| format!("{name}: {}", value.to_str().unwrap_or("<binary>")))
        .collect();
    log.push(format!(
        "{path} method={} headers: {}",
        request.method(),
        headers.join(" | ")
    ));
    let mut body = request.into_body();
    let outcome = match path.as_str() {
        "/test.Echo/Unary" => unary(&mut body, &mut respond).await,
        "/test.Echo/ServerStream" => server_stream(&mut body, &mut respond).await,
        "/test.Echo/ClientStream" => client_stream(&mut body, &mut respond).await,
        "/test.Echo/Bidi" => bidi(&mut body, &mut respond).await,
        "/test.Echo/Slow" => slow(&mut respond).await,
        "/test.Echo/Fail" => trailers_only(&mut respond, "5", "not%20found"),
        "/notgrpc" => not_found(&mut respond).await,
        _ => trailers_only(&mut respond, "12", "unknown%20method"),
    };
    match outcome {
        Ok(summary) => log.push(format!("{path} completed: {summary}")),
        Err(error) => log.push(format!("{path} ended with error: {error}")),
    }
}

type Outcome = Result<String, String>;

fn grpc_headers() -> Response<()> {
    let mut response = Response::new(());
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static("application/grpc"));
    response
}

fn ok_trailers() -> HeaderMap {
    let mut trailers = HeaderMap::new();
    trailers.insert("grpc-status", HeaderValue::from_static("0"));
    trailers
}

fn start(respond: &mut SendResponse<Bytes>) -> Result<SendStream<Bytes>, String> {
    respond
        .send_response(grpc_headers(), false)
        .map_err(|error| format!("send headers: {error}"))
}

/// Sends `data`, waiting for flow-control capacity rather than letting h2
/// buffer an unbounded amount. An error here is how a client reset shows up.
async fn send_all(stream: &mut SendStream<Bytes>, mut data: Bytes) -> Result<(), String> {
    while !data.is_empty() {
        stream.reserve_capacity(data.len());
        let capacity = match poll_fn(|cx| stream.poll_capacity(cx)).await {
            Some(Ok(capacity)) => capacity,
            Some(Err(error)) => return Err(format!("waiting for capacity: {error}")),
            None => return Err("stream closed while waiting for capacity".to_string()),
        };
        if capacity == 0 {
            continue;
        }
        let chunk = data.split_to(capacity.min(data.len()));
        stream
            .send_data(chunk, false)
            .map_err(|error| format!("send data: {error}"))?;
    }
    Ok(())
}

async fn send_message(stream: &mut SendStream<Bytes>, message: &[u8]) -> Result<(), String> {
    let framed = frame(message).map_err(|error| error.to_string())?;
    send_all(stream, Bytes::from(framed)).await
}

fn finish(stream: &mut SendStream<Bytes>) -> Result<(), String> {
    stream
        .send_trailers(ok_trailers())
        .map_err(|error| format!("send trailers: {error}"))
}

struct Messages<'a> {
    body: &'a mut RecvStream,
    decoder: FrameDecoder,
    queue: VecDeque<Vec<u8>>,
}

impl<'a> Messages<'a> {
    fn new(body: &'a mut RecvStream) -> Self {
        Self {
            body,
            decoder: FrameDecoder::new(MAX_RECEIVE_BYTES_CAP),
            queue: VecDeque::new(),
        }
    }

    /// The next complete message, or None once the client ended its stream.
    async fn next(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            if let Some(message) = self.queue.pop_front() {
                return Ok(Some(message));
            }
            match self.body.data().await {
                Some(Ok(chunk)) => {
                    let _ = self.body.flow_control().release_capacity(chunk.len());
                    let messages = self
                        .decoder
                        .push(&chunk)
                        .map_err(|error| format!("framing: {error}"))?;
                    self.queue.extend(messages);
                }
                Some(Err(error)) => return Err(format!("reading request: {error}")),
                None => return Ok(None),
            }
        }
    }
}

async fn unary(body: &mut RecvStream, respond: &mut SendResponse<Bytes>) -> Outcome {
    let mut messages = Messages::new(body);
    let mut received = Vec::new();
    while let Some(message) = messages.next().await? {
        received.push(message);
    }
    let mut stream = start(respond)?;
    for message in &received {
        send_message(&mut stream, message).await?;
    }
    finish(&mut stream)?;
    let sizes: Vec<usize> = received.iter().map(Vec::len).collect();
    Ok(format!(
        "echoed {} message(s), sizes {sizes:?}",
        received.len()
    ))
}

async fn server_stream(body: &mut RecvStream, respond: &mut SendResponse<Bytes>) -> Outcome {
    let mut messages = Messages::new(body);
    let request = messages.next().await?.unwrap_or_default();
    let text = String::from_utf8_lossy(&request).into_owned();
    let (count, interval) = text
        .split_once(',')
        .and_then(|(c, i)| {
            Some((
                c.trim().parse::<usize>().ok()?,
                i.trim().parse::<u64>().ok()?,
            ))
        })
        .ok_or_else(|| format!("bad request {text:?}"))?;
    let mut stream = start(respond)?;
    for i in 0..count {
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(interval)).await;
        }
        send_message(&mut stream, format!("message {i}").as_bytes()).await?;
    }
    finish(&mut stream)?;
    Ok(format!("streamed {count} messages {interval} ms apart"))
}

async fn client_stream(body: &mut RecvStream, respond: &mut SendResponse<Bytes>) -> Outcome {
    let mut messages = Messages::new(body);
    let (mut count, mut bytes) = (0usize, 0usize);
    while let Some(message) = messages.next().await? {
        count += 1;
        bytes += message.len();
    }
    let answer = format!("count={count} bytes={bytes}");
    let mut stream = start(respond)?;
    send_message(&mut stream, answer.as_bytes()).await?;
    finish(&mut stream)?;
    Ok(answer)
}

async fn bidi(body: &mut RecvStream, respond: &mut SendResponse<Bytes>) -> Outcome {
    let mut stream = start(respond)?;
    let mut messages = Messages::new(body);
    let mut count = 0usize;
    while let Some(message) = messages.next().await? {
        send_message(&mut stream, &message).await?;
        count += 1;
    }
    finish(&mut stream)?;
    Ok(format!(
        "echoed {count} messages, then the client ended its stream"
    ))
}

async fn slow(respond: &mut SendResponse<Bytes>) -> Outcome {
    let mut stream = start(respond)?;
    for i in 0..SLOW_MAX_MESSAGES {
        if let Err(error) = send_message(&mut stream, format!("tick {i}").as_bytes()).await {
            return Err(format!(
                "client stopped the stream after {i} messages: {error}"
            ));
        }
        tokio::time::sleep(SLOW_INTERVAL).await;
    }
    finish(&mut stream)?;
    Ok(format!(
        "sent all {SLOW_MAX_MESSAGES} messages; nobody cancelled"
    ))
}

fn trailers_only(
    respond: &mut SendResponse<Bytes>,
    status: &'static str,
    message: &'static str,
) -> Outcome {
    let mut response = grpc_headers();
    response
        .headers_mut()
        .insert("grpc-status", HeaderValue::from_static(status));
    response
        .headers_mut()
        .insert("grpc-message", HeaderValue::from_static(message));
    respond
        .send_response(response, true)
        .map_err(|error| format!("send trailers-only response: {error}"))?;
    Ok(format!("trailers-only grpc-status {status}"))
}

async fn not_found(respond: &mut SendResponse<Bytes>) -> Outcome {
    let mut response = Response::new(());
    *response.status_mut() = http::StatusCode::NOT_FOUND;
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static("text/plain"));
    let mut stream = respond
        .send_response(response, false)
        .map_err(|error| format!("send 404: {error}"))?;
    stream
        .send_data(Bytes::from_static(b"no such thing"), true)
        .map_err(|error| format!("send 404 body: {error}"))?;
    Ok("plain 404".to_string())
}

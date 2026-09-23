// http_client/src-tauri/tests/support/ws_server.rs
//
// A small WebSocket server on 127.0.0.1 for the integration tests, std only
// (PLAN.md Phase 13b, local server decided 2026-09-21). Each URL path is one
// behaviour the client has to cope with. What the server itself observes —
// did a pong arrive, did the client answer a close, with which code — goes
// into `observations`, because the client cannot see those facts from its
// side. Ported from the Phase 13a spike, where it was checked against
// libcurl on Windows and Linux.
//
// Frames from the server are unmasked and frames from the client masked
// (RFC 6455 section 5.3). SHA-1 and base64 are written out here only because
// the handshake needs them and the tests should not pull in a crate for it.
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

const OP_CONT: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

/// Bigger than the transport's 64 KiB receive buffer, so libcurl has to hand
/// it over in pieces.
pub const BIG_FRAME_LEN: usize = 200_000;

/// What `/oversize` sends: past a 1 KiB limit set by the test.
pub const OVERSIZE_LEN: usize = 2_000;

/// How long `/hang` holds a connection without answering.
const HANG: Duration = Duration::from_secs(10);

#[derive(Clone, Default)]
struct Shared {
    observations: Arc<Mutex<Vec<String>>>,
    flaky_connections: Arc<AtomicUsize>,
}

pub struct WsServer {
    pub addr: SocketAddr,
    shared: Shared,
}

impl WsServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
        let addr = listener.local_addr().expect("test server has an address");
        let shared = Shared::default();
        let accepted = shared.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = accepted.clone();
                thread::spawn(move || {
                    let _ = handle(stream, &shared);
                });
            }
        });
        Self { addr, shared }
    }

    pub fn url(&self, path: &str) -> String {
        format!("ws://{}{}", self.addr, path)
    }

    /// Everything observed so far, oldest first.
    pub fn observations(&self) -> Vec<String> {
        self.shared
            .observations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Waits for the server to observe something matching `needle`.
    pub fn wait_for(&self, needle: &str) -> Option<String> {
        for _ in 0..300 {
            if let Some(found) = self.observations().into_iter().find(|o| o.contains(needle)) {
                return Some(found);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    }
}

fn observe(shared: &Shared, line: String) {
    shared
        .observations
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(line);
}

struct Request {
    path: String,
    headers: HashMap<String, String>,
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    while !buffer.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "eof in request",
            ));
        }
        buffer.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut lines = text.split("\r\n");
    let path = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or("/")
        .to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    Ok(Request { path, headers })
}

fn handle(mut stream: TcpStream, shared: &Shared) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let request = read_request(&mut stream)?;
    let path = request.path.split('?').next().unwrap_or("/").to_string();

    let refusal = match path.as_str() {
        "/401" => Some(("401 Unauthorized", "unauthorized")),
        "/403" => Some(("403 Forbidden", "forbidden")),
        "/200" => Some(("200 OK", "not a websocket")),
        _ => None,
    };
    if let Some((status, body)) = refusal {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        return stream.write_all(response.as_bytes());
    }
    if path == "/hang" {
        // Holds the connection open and says nothing, like a server that
        // accepted TCP and then stalled.
        thread::sleep(HANG);
        return Ok(());
    }

    let Some(key) = request.headers.get("sec-websocket-key") else {
        return stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
    };
    let accept = base64(&sha1(format!("{key}{GUID}").as_bytes()));
    let protocol = request
        .headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.split(',').next())
        .map(|first| format!("Sec-WebSocket-Protocol: {}\r\n", first.trim()))
        .unwrap_or_default();
    let cookie = if path == "/set-cookie" {
        "Set-Cookie: session=abc; Path=/\r\n"
    } else {
        ""
    };
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n{protocol}{cookie}\r\n"
    );
    stream.write_all(response.as_bytes())?;

    match path.as_str() {
        "/fragmented" => {
            write_frame(&mut stream, OP_TEXT, false, b"hel")?;
            write_frame(&mut stream, OP_CONT, false, b"lo ")?;
            write_frame(&mut stream, OP_CONT, true, b"world")?;
        }
        "/big" => {
            let payload: Vec<u8> = (0..BIG_FRAME_LEN).map(|i| (i % 251) as u8).collect();
            write_frame(&mut stream, OP_BINARY, true, &payload)?;
        }
        "/oversize" => write_frame(&mut stream, OP_BINARY, true, &[7; OVERSIZE_LEN])?,
        "/ping" => {
            write_frame(&mut stream, OP_PING, true, b"p1")?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            match read_frame(&mut stream) {
                Ok((_, OP_PONG, payload)) => {
                    observe(
                        shared,
                        format!("pong received: {}", String::from_utf8_lossy(&payload)),
                    );
                    write_frame(&mut stream, OP_TEXT, true, b"pong-ok")?;
                }
                other => observe(shared, format!("no pong: {:?}", other.map(|(_, op, _)| op))),
            }
            stream.set_read_timeout(None)?;
        }
        "/server-close" => {
            let mut payload = 1001u16.to_be_bytes().to_vec();
            payload.extend_from_slice(b"bye");
            write_frame(&mut stream, OP_CLOSE, true, &payload)?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            match read_frame(&mut stream) {
                Ok((_, OP_CLOSE, payload)) => observe(
                    shared,
                    format!("close answered: code={:?}", close_code(&payload)),
                ),
                other => observe(
                    shared,
                    format!("close not answered: {:?}", other.map(|(_, op, _)| op)),
                ),
            }
            return stream.shutdown(Shutdown::Both);
        }
        "/drop" => {
            write_frame(&mut stream, OP_TEXT, true, b"dropping")?;
            thread::sleep(Duration::from_millis(100));
            return stream.shutdown(Shutdown::Both);
        }
        "/flaky" => {
            // The first connection drops without a close frame; every later
            // one stays up. What reconnecting is for.
            if shared.flaky_connections.fetch_add(1, Ordering::SeqCst) == 0 {
                write_frame(&mut stream, OP_TEXT, true, b"first")?;
                thread::sleep(Duration::from_millis(100));
                return stream.shutdown(Shutdown::Both);
            }
            write_frame(&mut stream, OP_TEXT, true, b"second")?;
        }
        "/headers" => {
            let get = |name: &str| {
                request
                    .headers
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| "<absent>".into())
            };
            let report = format!(
                "x-spike={};cookie={};protocol={}",
                get("x-spike"),
                get("cookie"),
                get("sec-websocket-protocol")
            );
            write_frame(&mut stream, OP_TEXT, true, report.as_bytes())?;
        }
        "/slow-reader" => {
            // Lets the client's writes pile up in the socket buffers, so a
            // big send has to come back partial or with CURLE_AGAIN.
            thread::sleep(Duration::from_millis(1500));
            let (_, op, payload) = read_message(&mut stream)?;
            let report = format!(
                "got op={op:#x} len={} fnv={:016x}",
                payload.len(),
                fnv1a(&payload)
            );
            write_frame(&mut stream, OP_TEXT, true, report.as_bytes())?;
        }
        _ => {}
    }
    echo_loop(&mut stream, shared, &path)
}

/// Echoes data messages, answers pings, answers and records a client close.
fn echo_loop(stream: &mut TcpStream, shared: &Shared, path: &str) -> io::Result<()> {
    loop {
        let Ok((_, op, payload)) = read_message(stream) else {
            return Ok(());
        };
        match op {
            OP_TEXT | OP_BINARY => write_frame(stream, op, true, &payload)?,
            OP_PING => write_frame(stream, OP_PONG, true, &payload)?,
            OP_PONG => {}
            OP_CLOSE => {
                observe(
                    shared,
                    format!(
                        "{path}: client close code={:?} reason={:?}",
                        close_code(&payload),
                        String::from_utf8_lossy(payload.get(2..).unwrap_or_default())
                    ),
                );
                write_frame(stream, OP_CLOSE, true, &payload)?;
                return stream.shutdown(Shutdown::Both);
            }
            other => observe(shared, format!("{path}: unexpected opcode {other:#x}")),
        }
    }
}

fn read_message(stream: &mut TcpStream) -> io::Result<(bool, u8, Vec<u8>)> {
    let (fin, op, mut payload) = read_frame(stream)?;
    if fin || op >= OP_CLOSE {
        return Ok((true, op, payload));
    }
    loop {
        let (fin, _, more) = read_frame(stream)?;
        payload.extend_from_slice(&more);
        if fin {
            return Ok((true, op, payload));
        }
    }
}

fn read_frame(stream: &mut TcpStream) -> io::Result<(bool, u8, Vec<u8>)> {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head)?;
    let fin = head[0] & 0x80 != 0;
    let op = head[0] & 0x0F;
    let masked = head[1] & 0x80 != 0;
    let mut len = u64::from(head[1] & 0x7F);
    if len == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext)?;
        len = u64::from(u16::from_be_bytes(ext));
    } else if len == 127 {
        let mut ext = [0u8; 8];
        stream.read_exact(&mut ext)?;
        len = u64::from_be_bytes(ext);
    }
    let mut mask = [0u8; 4];
    if masked {
        stream.read_exact(&mut mask)?;
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(usize::MAX)];
    stream.read_exact(&mut payload)?;
    if masked {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }
    Ok((fin, op, payload))
}

fn write_frame(stream: &mut TcpStream, op: u8, fin: bool, payload: &[u8]) -> io::Result<()> {
    let mut frame = Vec::with_capacity(payload.len() + 10);
    frame.push(if fin { 0x80 } else { 0 } | op);
    match payload.len() {
        n if n < 126 => frame.push(n as u8),
        n if n <= 0xFFFF => {
            frame.push(126);
            frame.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            frame.push(127);
            frame.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(payload);
    stream.write_all(&frame)
}

fn close_code(payload: &[u8]) -> Option<u16> {
    (payload.len() >= 2).then(|| u16::from_be_bytes([payload[0], payload[1]]))
}

pub fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn the_handshake_digest_is_right() {
    // RFC 6455 section 1.3's own example key and accept value.
    let accept = base64(&sha1(format!("dGhlIHNhbXBsZSBub25jZQ=={GUID}").as_bytes()));
    assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
}

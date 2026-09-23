// spikes/ws-libcurl/src/server.rs
//
// A deliberately small WebSocket server on 127.0.0.1, std only, so every
// scenario runs without a network and without a crate. Each URL path is one
// behaviour the client has to cope with. What the server itself observes (did
// a pong arrive, did the client answer a close) goes into `observations`,
// because the client cannot see those facts from its side.
//
// Frames from the server are unmasked, frames from the client are masked
// (RFC 6455 section 5.3). SHA-1 and base64 are hand-written only because the
// cloud build of this spike has no crates available; the app would never do
// this.
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub const OP_CONT: u8 = 0x0;
pub const OP_TEXT: u8 = 0x1;
pub const OP_BINARY: u8 = 0x2;
pub const OP_CLOSE: u8 = 0x8;
pub const OP_PING: u8 = 0x9;
pub const OP_PONG: u8 = 0xA;

/// Size of the single large frame `/big` sends; bigger than the client's
/// receive buffer so libcurl has to hand it over in pieces.
pub const BIG_FRAME_LEN: usize = 200_000;

pub type Observations = Arc<Mutex<Vec<String>>>;

pub struct Server {
    pub addr: SocketAddr,
    pub observations: Observations,
}

impl Server {
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let observations: Observations = Arc::new(Mutex::new(Vec::new()));
        let shared = observations.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let obs = shared.clone();
                thread::spawn(move || {
                    let _ = handle(stream, obs);
                });
            }
        });
        Ok(Self { addr, observations })
    }

    pub fn url(&self, path: &str) -> String {
        format!("ws://{}{}", self.addr, path)
    }

    /// Everything observed since the last call, oldest first.
    pub fn take_observations(&self) -> Vec<String> {
        let mut guard = self.observations.lock().unwrap_or_else(|p| p.into_inner());
        std::mem::take(&mut *guard)
    }
}

fn observe(obs: &Observations, line: String) {
    obs.lock().unwrap_or_else(|p| p.into_inner()).push(line);
}

struct Request {
    path: String,
    headers: HashMap<String, String>,
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "eof in request",
            ));
        }
        buf.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let path = first.split(' ').nth(1).unwrap_or("/").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Ok(Request { path, headers })
}

fn handle(mut stream: TcpStream, obs: Observations) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let request = read_request(&mut stream)?;
    let path = request.path.as_str();

    // Refusals first: these never upgrade.
    let refusal = match path {
        "/401" => Some((
            "401 Unauthorized",
            "WWW-Authenticate: Bearer\r\n",
            "unauthorized",
        )),
        "/403" => Some(("403 Forbidden", "", "forbidden")),
        "/200" => Some(("200 OK", "", "not a websocket")),
        _ => None,
    };
    if let Some((status, extra, body)) = refusal {
        let response = format!(
            "HTTP/1.1 {status}\r\n{extra}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    let Some(key) = request.headers.get("sec-websocket-key") else {
        stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")?;
        return Ok(());
    };
    let accept = base64(&sha1(format!("{key}{GUID}").as_bytes()));
    // Echo a requested subprotocol back, as a conforming server would.
    let protocol = request
        .headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.split(',').next())
        .map(|first| format!("Sec-WebSocket-Protocol: {}\r\n", first.trim()))
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n{protocol}X-Spike-Server: 1\r\n\r\n"
    );
    stream.write_all(response.as_bytes())?;

    match path {
        "/fragmented" => {
            write_frame(&mut stream, OP_TEXT, false, b"hel")?;
            write_frame(&mut stream, OP_CONT, false, b"lo ")?;
            write_frame(&mut stream, OP_CONT, true, b"world")?;
            echo_loop(&mut stream, &obs, path)
        }
        "/big" => {
            let payload: Vec<u8> = (0..BIG_FRAME_LEN).map(|i| (i % 251) as u8).collect();
            write_frame(&mut stream, OP_BINARY, true, &payload)?;
            echo_loop(&mut stream, &obs, path)
        }
        "/ping" => {
            write_frame(&mut stream, OP_PING, true, b"p1")?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            match read_frame(&mut stream) {
                Ok((_, OP_PONG, payload)) => {
                    observe(
                        &obs,
                        format!(
                            "ping: pong received, payload={:?}",
                            String::from_utf8_lossy(&payload)
                        ),
                    );
                    write_frame(&mut stream, OP_TEXT, true, b"pong-ok")?;
                }
                Ok((_, op, _)) => observe(&obs, format!("ping: expected pong, got opcode {op:#x}")),
                Err(e) => observe(&obs, format!("ping: no pong within 3 s ({e})")),
            }
            stream.set_read_timeout(None)?;
            echo_loop(&mut stream, &obs, path)
        }
        "/server-close" => {
            let mut payload = 1001u16.to_be_bytes().to_vec();
            payload.extend_from_slice(b"bye");
            write_frame(&mut stream, OP_CLOSE, true, &payload)?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            match read_frame(&mut stream) {
                Ok((_, OP_CLOSE, payload)) => observe(
                    &obs,
                    format!(
                        "server-close: client answered with a close frame, code={:?}",
                        close_code(&payload)
                    ),
                ),
                Ok((_, op, _)) => observe(
                    &obs,
                    format!("server-close: client sent opcode {op:#x} instead of close"),
                ),
                Err(e) => observe(
                    &obs,
                    format!(
                        "server-close: client did not answer within 3 s ({})",
                        e.kind()
                    ),
                ),
            }
            let _ = stream.shutdown(Shutdown::Both);
            Ok(())
        }
        "/drop" => {
            write_frame(&mut stream, OP_TEXT, true, b"dropping")?;
            thread::sleep(Duration::from_millis(200));
            // No close frame: the TCP connection just goes away, which is
            // what a crashed server or a pulled cable looks like.
            let _ = stream.shutdown(Shutdown::Both);
            Ok(())
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
            echo_loop(&mut stream, &obs, path)
        }
        "/slow-reader" => {
            // Let the client's writes pile up in the socket buffers, so a big
            // send has to come back partial or with CURLE_AGAIN.
            thread::sleep(Duration::from_millis(1500));
            let (_, op, payload) = read_message(&mut stream)?;
            let report = format!(
                "got op={op:#x} len={} fnv={:016x}",
                payload.len(),
                fnv1a(&payload)
            );
            write_frame(&mut stream, OP_TEXT, true, report.as_bytes())?;
            echo_loop(&mut stream, &obs, path)
        }
        _ => echo_loop(&mut stream, &obs, path),
    }
}

/// Echo data messages back, answer pings, answer and record a client close.
fn echo_loop(stream: &mut TcpStream, obs: &Observations, path: &str) -> io::Result<()> {
    loop {
        let (_, op, payload) = match read_message(stream) {
            Ok(message) => message,
            Err(_) => return Ok(()),
        };
        match op {
            OP_TEXT | OP_BINARY => write_frame(stream, op, true, &payload)?,
            OP_PING => write_frame(stream, OP_PONG, true, &payload)?,
            OP_PONG => {}
            OP_CLOSE => {
                observe(
                    obs,
                    format!(
                        "{path}: client close received, code={:?} reason={:?}",
                        close_code(&payload),
                        String::from_utf8_lossy(payload.get(2..).unwrap_or_default())
                    ),
                );
                write_frame(stream, OP_CLOSE, true, &payload)?;
                let _ = stream.shutdown(Shutdown::Both);
                return Ok(());
            }
            other => observe(obs, format!("{path}: unexpected opcode {other:#x}")),
        }
    }
}

/// One whole message: data frames reassembled across continuations, control
/// frames returned as they come.
fn read_message(stream: &mut TcpStream) -> io::Result<(bool, u8, Vec<u8>)> {
    let (fin, op, mut payload) = read_frame(stream)?;
    if fin || op >= OP_CLOSE {
        return Ok((true, op, payload));
    }
    loop {
        let (fin, cont_op, more) = read_frame(stream)?;
        if cont_op != OP_CONT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "control frame inside fragmented message",
            ));
        }
        payload.extend_from_slice(&more);
        if fin {
            return Ok((true, op, payload));
        }
    }
}

pub fn read_frame(stream: &mut TcpStream) -> io::Result<(bool, u8, Vec<u8>)> {
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
    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload)?;
    if masked {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }
    Ok((fin, op, payload))
}

pub fn write_frame(stream: &mut TcpStream, op: u8, fin: bool, payload: &[u8]) -> io::Result<()> {
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

pub fn close_code(payload: &[u8]) -> Option<u16> {
    (payload.len() >= 2).then(|| u16::from_be_bytes([payload[0], payload[1]]))
}

pub fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

pub fn sha1(data: &[u8]) -> [u8; 20] {
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
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
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

pub fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
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

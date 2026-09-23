// spikes/ws-libcurl/src/main.rs
//
// PLAN.md Phase 13a: prove that the libcurl already linked into the app can
// drive a WebSocket from Rust, before anything in 13b depends on it.
//
// Two builds of the same source:
//   cargo build --release   the app's exact curl/curl-sys line, static libcurl
//                           with rustls — the build whose verdict counts.
//   rustc --cfg raw_libcurl against a self-built libcurl, no crates at all —
//                           the cloud run, used to find out libcurl's
//                           behaviour before the Windows run.
//
// Every scenario prints PASS, FAIL, INFO or SKIP. Scenarios marked [gate]
// decide D1: any FAIL among them means 13b switches to tungstenite.
// Output also goes to spike-report.txt in the working directory.
#![allow(clippy::too_many_lines)]

#[cfg(not(raw_libcurl))]
extern crate curl_sys;

#[cfg(not(raw_libcurl))]
mod easy2;
mod ffi;
mod server;
mod wait;
mod ws;

use std::fmt::Write as _;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use server::Server;
use ws::{ConnectOptions, Connection, Message, RecvError, Wait};

const SHORT: Duration = Duration::from_secs(3);
const LONG: Duration = Duration::from_secs(15);
const PUBLIC_ECHO: &str = "wss://echo.websocket.org";
const ROUND_TRIPS: usize = 200;

struct Report {
    text: String,
    failed_gates: Vec<&'static str>,
    failures: u32,
}

impl Report {
    fn line(&mut self, status: &str, id: &'static str, gate: bool, message: &str) {
        let marker = if gate { "[gate]" } else { "      " };
        let line = format!("{status:<4} {id:<4} {marker} {message}");
        println!("{line}");
        let _ = writeln!(self.text, "{line}");
        if status == "FAIL" {
            self.failures += 1;
            if gate && !self.failed_gates.contains(&id) {
                self.failed_gates.push(id);
            }
        }
    }
    fn pass(&mut self, id: &'static str, gate: bool, message: &str) {
        self.line("PASS", id, gate, message);
    }
    fn fail(&mut self, id: &'static str, gate: bool, message: &str) {
        self.line("FAIL", id, gate, message);
    }
    fn info(&mut self, id: &'static str, message: &str) {
        self.line("INFO", id, false, message);
    }
    fn skip(&mut self, id: &'static str, message: &str) {
        self.line("SKIP", id, false, message);
    }
}

fn text_of(message: &Message) -> String {
    String::from_utf8_lossy(&message.data).into_owned()
}

fn describe_recv(result: &Result<Message, RecvError>) -> String {
    match result {
        Ok(m) => format!(
            "message flags={} len={} chunks={}",
            ws::flag_names(m.flags),
            m.data.len(),
            m.chunks.len()
        ),
        Err(RecvError::Timeout { waits }) => format!("timeout after {waits} waits"),
        Err(RecvError::Curl(f)) => format!("curl error {} ({})", f.code, f.message),
    }
}

fn close_payload(message: &Message) -> String {
    let code = server::close_code(&message.data);
    let reason = String::from_utf8_lossy(message.data.get(2..).unwrap_or_default()).into_owned();
    format!("code={code:?} reason={reason:?}")
}

fn connect(report: &mut Report, id: &'static str, url: &str) -> Option<Connection> {
    match Connection::connect(&ConnectOptions::new(url)) {
        Ok(c) => Some(c),
        Err(e) => {
            report.fail(
                id,
                true,
                &format!(
                    "connect {url} failed: {} {} [{}] status={}",
                    e.code, e.message, e.detail, e.status
                ),
            );
            None
        }
    }
}

fn main() -> ExitCode {
    ws::global_init();
    let mut report = Report {
        text: String::new(),
        failed_gates: Vec::new(),
        failures: 0,
    };

    let build = if cfg!(raw_libcurl) {
        "rustc --cfg raw_libcurl (self-built libcurl, no crates)"
    } else {
        "cargo (curl-sys, static libcurl, rustls)"
    };
    report.info("S00", &format!("build: {build}"));
    report.info("S00", &format!("libcurl: {}", ffi::version()));
    report.info(
        "S00",
        &format!(
            "target: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
    let digest: String = server::sha1(b"abc")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest == "a9993e364706816aba3e25717850c26c9cd0d89d" {
        report.pass("S00", false, "test server SHA-1 self-check");
    } else {
        report.fail("S00", false, &format!("test server SHA-1 wrong: {digest}"));
    }

    let server = match Server::start() {
        Ok(s) => s,
        Err(e) => {
            report.fail(
                "S00",
                true,
                &format!("local test server did not start: {e}"),
            );
            return finish(report);
        }
    };
    report.info("S00", &format!("local test server on {}", server.addr));

    s01_text_echo(&server, &mut report);
    s02_binary_echo(&server, &mut report);
    s03_latency_and_idle(&server, &mut report);
    s04_fragmented(&server, &mut report);
    s05_large_frame(&server, &mut report);
    s06_ping(&server, &mut report);
    s07_server_close(&server, &mut report);
    s08_client_close(&server, &mut report);
    s09_refused(&server, &mut report);
    s10_unreachable(&mut report);
    s11_timeout_after_connect(&server, &mut report);
    s12_headers(&server, &mut report);
    s13_big_send(&server, &mut report);
    s14_drop(&server, &mut report);
    s15_easy2(&server, &mut report);
    s16_public_wss(&mut report);
    s17_tls_failure(&mut report);
    s18_proxy(&mut report);

    finish(report)
}

fn finish(mut report: Report) -> ExitCode {
    let verdict = if report.failed_gates.is_empty() {
        "VERDICT: every gate passed in this build.".to_string()
    } else {
        format!("VERDICT: gates failed: {}", report.failed_gates.join(", "))
    };
    report.info("END", &format!("{} failure(s) in total", report.failures));
    println!("{verdict}");
    let _ = writeln!(report.text, "{verdict}");
    if let Err(e) = std::fs::write("spike-report.txt", &report.text) {
        eprintln!("could not write spike-report.txt: {e}");
    }
    if report.failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn s01_text_echo(server: &Server, report: &mut Report) {
    for mode in [Wait::Sleep, Wait::Socket] {
        let Some(conn) = connect(report, "S01", &server.url("/echo")) else {
            return;
        };
        let upgrade = conn
            .response_headers()
            .iter()
            .any(|h| h.to_ascii_lowercase().starts_with("upgrade: websocket"));
        if conn.status == 101 && upgrade {
            report.pass(
                "S01",
                true,
                &format!(
                    "handshake: status 101, {} header lines captured",
                    conn.response_headers().len()
                ),
            );
        } else {
            report.fail(
                "S01",
                true,
                &format!(
                    "handshake: status {} upgrade header seen={upgrade}",
                    conn.status
                ),
            );
        }
        if let Err(e) = conn.send_text("hello", mode) {
            report.fail(
                "S01",
                true,
                &format!("{mode:?}: send failed {} {}", e.code, e.message),
            );
            continue;
        }
        match conn.recv(mode, SHORT) {
            Ok(m) if m.flags & ffi::CURLWS_TEXT != 0 && text_of(&m) == "hello" => {
                report.pass("S01", true, &format!("{mode:?}: text echo 'hello'"));
            }
            other => report.fail(
                "S01",
                true,
                &format!("{mode:?}: text echo: {}", describe_recv(&other)),
            ),
        }
    }
}

fn s02_binary_echo(server: &Server, report: &mut Report) {
    let Some(conn) = connect(report, "S02", &server.url("/echo")) else {
        return;
    };
    let payload: Vec<u8> = (0..=255u8).collect();
    if let Err(e) = conn.send(ffi::CURLWS_BINARY, &payload, Wait::Socket, SHORT) {
        report.fail(
            "S02",
            true,
            &format!("send failed {} {}", e.code, e.message),
        );
        return;
    }
    match conn.recv(Wait::Socket, SHORT) {
        Ok(m) if m.flags & ffi::CURLWS_BINARY != 0 && m.data == payload => {
            report.pass("S02", true, "binary echo, all 256 byte values intact");
        }
        other => report.fail(
            "S02",
            true,
            &format!("binary echo: {}", describe_recv(&other)),
        ),
    }
}

fn s03_latency_and_idle(server: &Server, report: &mut Report) {
    for mode in [Wait::Sleep, Wait::Socket] {
        let Some(conn) = connect(report, "S03", &server.url("/echo")) else {
            return;
        };
        let mut samples = Vec::with_capacity(ROUND_TRIPS);
        for i in 0..ROUND_TRIPS {
            let text = format!("m{i}");
            let started = Instant::now();
            if conn.send_text(&text, mode).is_err() {
                report.fail("S03", false, &format!("{mode:?}: send {i} failed"));
                return;
            }
            match conn.recv(mode, SHORT) {
                Ok(m) if text_of(&m) == text => samples.push(started.elapsed()),
                other => {
                    report.fail(
                        "S03",
                        false,
                        &format!("{mode:?}: round trip {i}: {}", describe_recv(&other)),
                    );
                    return;
                }
            }
        }
        samples.sort();
        let median = samples[samples.len() / 2];
        let p95 = samples[samples.len() * 95 / 100];
        report.info(
            "S03",
            &format!(
                "{mode:?}: {ROUND_TRIPS} echo round trips, median {} us, p95 {} us",
                median.as_micros(),
                p95.as_micros()
            ),
        );
        let started = Instant::now();
        match conn.recv(mode, Duration::from_secs(2)) {
            Err(RecvError::Timeout { waits }) => report.info(
                "S03",
                &format!(
                    "{mode:?}: idle for {:.1} s cost {waits} wake-ups",
                    started.elapsed().as_secs_f32()
                ),
            ),
            other => report.info(
                "S03",
                &format!("{mode:?}: idle check got {}", describe_recv(&other)),
            ),
        }
    }
}

fn s04_fragmented(server: &Server, report: &mut Report) {
    let Some(conn) = connect(report, "S04", &server.url("/fragmented")) else {
        return;
    };
    match conn.recv(Wait::Socket, SHORT) {
        Ok(m) if text_of(&m) == "hello world" => {
            let chunks: Vec<String> = m
                .chunks
                .iter()
                .map(|c| {
                    format!(
                        "[{} off={} left={} len={}]",
                        ws::flag_names(c.flags),
                        c.offset,
                        c.bytesleft,
                        c.len
                    )
                })
                .collect();
            report.pass(
                "S04",
                true,
                &format!(
                    "3 server fragments reassembled to 'hello world': {}",
                    chunks.join(" ")
                ),
            );
        }
        other => report.fail(
            "S04",
            true,
            &format!("fragmented message: {}", describe_recv(&other)),
        ),
    }
}

fn s05_large_frame(server: &Server, report: &mut Report) {
    let Some(conn) = connect(report, "S05", &server.url("/big")) else {
        return;
    };
    match conn.recv(Wait::Socket, LONG) {
        Ok(m) => {
            let intact = m.data.len() == server::BIG_FRAME_LEN
                && m.data
                    .iter()
                    .enumerate()
                    .all(|(i, b)| *b == (i % 251) as u8);
            let lefts: Vec<String> = m.chunks.iter().map(|c| c.bytesleft.to_string()).collect();
            let summary = format!(
                "{} byte frame arrived in {} chunks (bytesleft: {})",
                m.data.len(),
                m.chunks.len(),
                lefts.join(", ")
            );
            if intact {
                report.pass("S05", true, &summary);
            } else {
                report.fail("S05", true, &format!("corrupt: {summary}"));
            }
        }
        Err(e) => report.fail(
            "S05",
            true,
            &format!("large frame: {}", describe_recv(&Err(e))),
        ),
    }
}

fn s06_ping(server: &Server, report: &mut Report) {
    // (a) libcurl's automatic pong. Not a gate: the result is informational,
    // because libcurl queues the pong and only flushes it the next time the
    // app receives a data frame or sends something (lib/ws.c, curl_ws_recv
    // and ws_flush). A connection that is only listening never answers.
    let Some(conn) = connect(report, "S06", &server.url("/ping")) else {
        return;
    };
    let result = conn.recv(Wait::Socket, Duration::from_secs(5));
    let observations = server.take_observations();
    let ponged = observations.iter().any(|o| o.contains("pong received"));
    report.info(
        "S06",
        &format!(
            "(a) auto-pong while only receiving: server got a pong={ponged}; app recv: {}; server saw: {observations:?}",
            describe_recv(&result)
        ),
    );
    drop(conn);

    // (b) CURLWS_NOAUTOPONG: the PING is handed to the app, which answers.
    let url = server.url("/ping");
    let mut options = ConnectOptions::new(&url);
    options.no_auto_pong = true;
    let conn = match Connection::connect(&options) {
        Ok(c) => c,
        Err(e) => {
            report.fail(
                "S06",
                true,
                &format!("(b) connect failed {} {}", e.code, e.message),
            );
            return;
        }
    };
    let ping = conn.recv(Wait::Socket, SHORT);
    let Ok(ping) = ping else {
        report.fail(
            "S06",
            true,
            &format!("(b) expected a PING: {}", describe_recv(&ping)),
        );
        return;
    };
    if ping.flags & ffi::CURLWS_PING == 0 {
        report.fail(
            "S06",
            true,
            &format!(
                "(b) expected a PING, got flags={}",
                ws::flag_names(ping.flags)
            ),
        );
        return;
    }
    let pong = conn.send(ffi::CURLWS_PONG, &ping.data, Wait::Socket, SHORT);
    let next = conn.recv(Wait::Socket, SHORT);
    let observations = server.take_observations();
    match &next {
        Ok(m) if text_of(m) == "pong-ok" && observations.iter().any(|o| o.contains("payload=\"p1\"")) => report.pass(
            "S06",
            true,
            &format!("(b) NOAUTOPONG: PING {:?} surfaced, app answered it (send {:?}), server got the pong", text_of(&ping), pong.map(|s| s.calls).map_err(|e| e.code)),
        ),
        other => report.fail("S06", true, &format!("(b) after pong: {}; server saw: {observations:?}", describe_recv(other))),
    }
}

fn s07_server_close(server: &Server, report: &mut Report) {
    // (a) The app does nothing after the close: does libcurl answer by itself?
    let Some(conn) = connect(report, "S07", &server.url("/server-close")) else {
        return;
    };
    let first = conn.recv(Wait::Socket, SHORT);
    match &first {
        Ok(m) if m.flags & ffi::CURLWS_CLOSE != 0 => {
            report.pass(
                "S07",
                true,
                &format!(
                    "server close surfaced as a CLOSE message: {}",
                    close_payload(m)
                ),
            );
        }
        other => report.fail(
            "S07",
            true,
            &format!("expected a CLOSE message, got {}", describe_recv(other)),
        ),
    }
    let after = conn.recv(Wait::Socket, Duration::from_secs(5));
    report.info(
        "S07",
        &format!(
            "(a) no reply sent by the app; next recv: {}",
            describe_recv(&after)
        ),
    );
    let observed = server.take_observations();
    let auto_replied = observed.iter().any(|o| o.contains("answered with a close"));
    report.info(
        "S07",
        &format!(
            "(a) libcurl answered the close on its own: {auto_replied}; server saw: {observed:?}"
        ),
    );
    drop(conn);

    // (b) The app answers the close itself.
    let Some(conn) = connect(report, "S07", &server.url("/server-close")) else {
        return;
    };
    let _ = conn.recv(Wait::Socket, SHORT);
    let reply = conn.send_close(1001, "", Wait::Socket);
    let after = conn.recv(Wait::Socket, Duration::from_secs(5));
    thread::sleep(Duration::from_millis(200));
    let observed = server.take_observations();
    let answered = observed.iter().any(|o| o.contains("answered with a close"));
    let summary = format!(
        "(b) app replied with close: send={:?}; next recv: {}; server saw: {observed:?}",
        reply.as_ref().map(|s| s.calls).map_err(|e| e.code),
        describe_recv(&after)
    );
    if answered {
        report.pass("S07", true, &summary);
    } else {
        report.fail("S07", true, &summary);
    }
}

fn s08_client_close(server: &Server, report: &mut Report) {
    let Some(conn) = connect(report, "S08", &server.url("/echo")) else {
        return;
    };
    if let Err(e) = conn.send_close(1000, "done", Wait::Socket) {
        report.fail(
            "S08",
            true,
            &format!("sending close failed {} {}", e.code, e.message),
        );
        return;
    }
    let reply = conn.recv(Wait::Socket, SHORT);
    let after = conn.recv(Wait::Socket, SHORT);
    thread::sleep(Duration::from_millis(100));
    let observed = server.take_observations();
    match &reply {
        Ok(m) if m.flags & ffi::CURLWS_CLOSE != 0 && observed.iter().any(|o| o.contains("code=Some(1000)")) => report.pass(
            "S08",
            true,
            &format!("client close 1000 'done' reached the server; its echo came back as {}; next recv: {}", close_payload(m), describe_recv(&after)),
        ),
        other => report.fail("S08", true, &format!("client close: {}; server saw: {observed:?}", describe_recv(other))),
    }
}

fn s09_refused(server: &Server, report: &mut Report) {
    for (path, expected) in [("/401", 401), ("/403", 403), ("/200", 200)] {
        match Connection::connect(&ConnectOptions::new(&server.url(path))) {
            Ok(conn) => report.fail(
                "S09",
                true,
                &format!(
                    "{path}: connected with status {} — a refusal was accepted",
                    conn.status
                ),
            ),
            Err(e) if e.status == expected => report.pass(
                "S09",
                true,
                &format!(
                    "{path}: refused with curl {} ({}) [{}], status {} readable",
                    e.code, e.message, e.detail, e.status
                ),
            ),
            Err(e) => report.fail(
                "S09",
                true,
                &format!(
                    "{path}: curl {} ({}) [{}] but status read back as {}",
                    e.code, e.message, e.detail, e.status
                ),
            ),
        }
    }
}

fn s10_unreachable(report: &mut Report) {
    for url in ["ws://127.0.0.1:1/", "ws://no-such-host.invalid/"] {
        let mut options = ConnectOptions::new(url);
        options.connect_timeout_ms = 5_000;
        match Connection::connect(&options) {
            Ok(_) => report.fail("S10", true, &format!("{url}: connected?")),
            Err(e) => report.pass(
                "S10",
                true,
                &format!("{url}: curl {} ({}) [{}]", e.code, e.message, e.detail),
            ),
        }
    }
}

fn s11_timeout_after_connect(server: &Server, report: &mut Report) {
    let url = server.url("/echo");
    let mut options = ConnectOptions::new(&url);
    options.timeout_ms = Some(1_500);
    let conn = match Connection::connect(&options) {
        Ok(c) => c,
        Err(e) => {
            report.fail(
                "S11",
                false,
                &format!("connect failed {} {}", e.code, e.message),
            );
            return;
        }
    };
    thread::sleep(Duration::from_millis(2_500));
    let sent = conn.send_text("late", Wait::Socket);
    let got = conn.recv(Wait::Socket, SHORT);
    report.info(
        "S11",
        &format!(
            "CURLOPT_TIMEOUT_MS=1500, idle 2.5 s, then send: {:?}, recv: {} => the timeout {} an open WebSocket",
            sent.as_ref().map(|s| s.calls).map_err(|e| (e.code, e.message.clone())),
            describe_recv(&got),
            if got.is_ok() { "does NOT limit" } else { "DOES limit" }
        ),
    );
}

fn s12_headers(server: &Server, report: &mut Report) {
    let url = server.url("/headers");
    let mut options = ConnectOptions::new(&url);
    options.headers = &[
        "X-Spike: 42",
        "Cookie: a=b; c=d",
        "Sec-WebSocket-Protocol: chat",
    ];
    let conn = match Connection::connect(&options) {
        Ok(c) => c,
        Err(e) => {
            report.fail(
                "S12",
                true,
                &format!("connect failed {} {} [{}]", e.code, e.message, e.detail),
            );
            return;
        }
    };
    let echoed_protocol = conn
        .response_headers()
        .iter()
        .any(|h| h.eq_ignore_ascii_case("sec-websocket-protocol: chat"));
    match conn.recv(Wait::Socket, SHORT) {
        Ok(m) if text_of(&m) == "x-spike=42;cookie=a=b; c=d;protocol=chat" => report.pass(
            "S12",
            true,
            &format!("custom header, Cookie and subprotocol reached the handshake; protocol in 101 response: {echoed_protocol}"),
        ),
        other => report.fail("S12", true, &format!("server saw: {}", other.map(|m| text_of(&m)).unwrap_or_else(|e| format!("{e:?}")))),
    }
}

fn s13_big_send(server: &Server, report: &mut Report) {
    const LEN: usize = 8 * 1024 * 1024;
    let Some(conn) = connect(report, "S13", &server.url("/slow-reader")) else {
        return;
    };
    let payload: Vec<u8> = (0..LEN).map(|i| (i % 253) as u8).collect();
    let started = Instant::now();
    let stats = match conn.send(
        ffi::CURLWS_BINARY,
        &payload,
        Wait::Socket,
        Duration::from_secs(30),
    ) {
        Ok(s) => s,
        Err(e) => {
            report.fail(
                "S13",
                true,
                &format!("8 MiB send failed {} {}", e.code, e.message),
            );
            return;
        }
    };
    let elapsed = started.elapsed();
    let expected = format!("got op=0x2 len={LEN} fnv={:016x}", server::fnv1a(&payload));
    match conn.recv(Wait::Socket, Duration::from_secs(30)) {
        Ok(m) if text_of(&m) == expected => report.pass(
            "S13",
            true,
            &format!(
                "8 MiB to a reader stalled 1.5 s: {} calls, {} partial, {} CURLE_AGAIN, {:.2} s; server confirmed length and hash",
                stats.calls,
                stats.partials,
                stats.agains,
                elapsed.as_secs_f32()
            ),
        ),
        other => report.fail("S13", true, &format!("server reply: {}", other.map(|m| text_of(&m)).unwrap_or_else(|e| format!("{e:?}")))),
    }
}

fn s14_drop(server: &Server, report: &mut Report) {
    let Some(conn) = connect(report, "S14", &server.url("/drop")) else {
        return;
    };
    let first = conn.recv(Wait::Socket, SHORT);
    let after = conn.recv(Wait::Socket, SHORT);
    let send = conn.send_text("anyone?", Wait::Socket);
    let detected = matches!(after, Err(RecvError::Curl(_)));
    let summary = format!(
        "TCP dropped without a close frame: first recv {}, then recv {}, then send {:?}",
        describe_recv(&first),
        describe_recv(&after),
        send.map(|s| s.calls).map_err(|e| (e.code, e.message))
    );
    if detected {
        report.pass("S14", true, &summary);
    } else {
        report.fail(
            "S14",
            true,
            &format!("drop not reported as an error: {summary}"),
        );
    }
}

#[cfg(not(raw_libcurl))]
fn s15_easy2(server: &Server, report: &mut Report) {
    let conn = match easy2::connect(&server.url("/echo")) {
        Ok(c) => c,
        Err(e) => {
            report.fail(
                "S15",
                true,
                &format!(
                    "Easy2 handshake failed {} {} [{}]",
                    e.code, e.message, e.detail
                ),
            );
            return;
        }
    };
    let _ = conn.send_text("via easy2", Wait::Socket);
    match conn.recv(Wait::Socket, SHORT) {
        Ok(m) if text_of(&m) == "via easy2" => report.pass(
            "S15",
            true,
            &format!(
                "Easy2 did the handshake (status {}), frames over easy.raw()",
                conn.status
            ),
        ),
        other => report.fail(
            "S15",
            true,
            &format!("Easy2 handoff: {}", describe_recv(&other)),
        ),
    }
}

#[cfg(raw_libcurl)]
fn s15_easy2(_: &Server, report: &mut Report) {
    report.skip(
        "S15",
        "Easy2 handoff needs the curl crate: cargo build only",
    );
}

fn public_echo(report: &mut Report, id: &'static str, options: &ConnectOptions<'_>, label: &str) {
    let conn = match Connection::connect(options) {
        Ok(c) => c,
        Err(e) => {
            report.fail(
                id,
                true,
                &format!(
                    "{label}: connect failed {} ({}) [{}] status={}",
                    e.code, e.message, e.detail, e.status
                ),
            );
            return;
        }
    };
    if let Err(e) = conn.send_text("hello-wss", Wait::Socket) {
        report.fail(
            id,
            true,
            &format!("{label}: send failed {} {}", e.code, e.message),
        );
        return;
    }
    let mut seen = Vec::new();
    for _ in 0..3 {
        match conn.recv(Wait::Socket, LONG) {
            Ok(m) if text_of(&m) == "hello-wss" => {
                report.pass(
                    id,
                    true,
                    &format!(
                        "{label}: echo over TLS (status {}); before it: {seen:?}",
                        conn.status
                    ),
                );
                let _ = conn.send_close(1000, "", Wait::Socket);
                return;
            }
            Ok(m) => seen.push(text_of(&m)),
            Err(e) => {
                report.fail(id, true, &format!("{label}: {}", describe_recv(&Err(e))));
                return;
            }
        }
    }
    report.fail(id, true, &format!("{label}: no echo among {seen:?}"));
}

fn s16_public_wss(report: &mut Report) {
    if cfg!(raw_libcurl) {
        report.skip("S16", "wss:// needs TLS and network: cargo build only");
        return;
    }
    public_echo(
        report,
        "S16",
        &ConnectOptions::new(PUBLIC_ECHO),
        PUBLIC_ECHO,
    );
}

fn s17_tls_failure(report: &mut Report) {
    if cfg!(raw_libcurl) {
        report.skip("S17", "TLS failure needs TLS and network: cargo build only");
        return;
    }
    for url in ["wss://expired.badssl.com/", "wss://self-signed.badssl.com/"] {
        match Connection::connect(&ConnectOptions::new(url)) {
            Ok(c) => report.fail(
                "S17",
                true,
                &format!(
                    "{url}: connected (status {}) despite a bad certificate",
                    c.status
                ),
            ),
            Err(e) => report.pass(
                "S17",
                true,
                &format!(
                    "{url}: refused, curl {} ({}) [{}]",
                    e.code, e.message, e.detail
                ),
            ),
        }
    }
    // Verification off is the explicit per-request opt-in (CLAUDE.md rule 7).
    // badssl serves no WebSocket, so success here is getting *past* TLS to an
    // HTTP refusal rather than stopping at the certificate.
    let mut options = ConnectOptions::new("wss://self-signed.badssl.com/");
    options.verify_tls = false;
    match Connection::connect(&options) {
        Ok(c) => report.info(
            "S17",
            &format!("verify off: connected, status {}", c.status),
        ),
        Err(e) => report.info(
            "S17",
            &format!(
                "verify off: got past TLS to curl {} ({}) [{}] status {}",
                e.code, e.message, e.detail, e.status
            ),
        ),
    }
}

fn s18_proxy(report: &mut Report) {
    let Ok(proxy) = std::env::var("SPIKE_PROXY") else {
        report.skip(
            "S18",
            "set SPIKE_PROXY=http://host:port to test a proxied handshake",
        );
        return;
    };
    let mut options = ConnectOptions::new(PUBLIC_ECHO);
    options.proxy = Some(&proxy);
    public_echo(
        report,
        "S18",
        &options,
        &format!("{PUBLIC_ECHO} via {proxy}"),
    );
}

// spikes/grpc-libcurl/src/main.rs
//
// PLAN-GRPC.md 16a: prove that the libcurl already linked into the app can
// carry gRPC before anything in 16b–16j depends on it. Throwaway code.
//
// The question is not whether libcurl can POST over HTTP/2 (it can), but
// whether it delivers HTTP/2 trailers to the header callback, and whether a
// paused read callback plus a Multi loop gives full-duplex streaming.
//
// Every scenario prints PASS, FAIL, INFO or SKIP. Scenarios marked [gate]
// decide D1: any FAIL among them means the transport switches to tonic and
// PLAN-GRPC.md is reissued. The report is also written to spike-report.txt.
//
// Environment:
//   SPIKE_PROXY=http://host:port   run gate 11 through that proxy
//   SPIKE_VERBOSE=1                libcurl's own trace on stderr
#![allow(clippy::too_many_lines)]

mod client;
mod frame;
mod server;

use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use client::{Call, Target};
use server::Server;

const LONG: Duration = Duration::from_secs(15);
const ROUND_TRIPS: usize = 1000;
const G14_ROUND_TRIPS: usize = 200;
const BIG_MESSAGE: usize = 16 * 1024 * 1024;
/// A public, reflection-enabled gRPC test server over TLS. Its `Empty`
/// method takes and returns an empty message. Unreachable means SKIP, not
/// FAIL: the gate is about libcurl, not about someone else's server.
const PUBLIC_TLS_BASE: &str = "https://grpcb.in:9001";
const PUBLIC_TLS_METHOD: &str = "/grpcbin.GRPCBin/Empty";

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
    fn check(&mut self, id: &'static str, ok: bool, message: &str) {
        self.line(if ok { "PASS" } else { "FAIL" }, id, true, message);
    }
    fn info(&mut self, id: &'static str, message: &str) {
        self.line("INFO", id, false, message);
    }
    fn skip(&mut self, id: &'static str, message: &str) {
        self.line("SKIP", id, false, message);
    }
}

fn open(
    report: &mut Report,
    id: &'static str,
    target: &Target,
    path: &str,
    initial: &[&[u8]],
    end: bool,
) -> Option<Call> {
    match Call::open(target, path, initial, end) {
        Ok(call) => Some(call),
        Err(error) => {
            report.check(id, false, &format!("could not set up {path}: {error}"));
            None
        }
    }
}

fn messages_text(call: &Call) -> Vec<String> {
    call.collector()
        .messages
        .iter()
        .map(|(_, m)| String::from_utf8_lossy(m).into_owned())
        .collect()
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

fn percentile(sorted: &[Duration], p: usize) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    sorted[(sorted.len() - 1) * p / 100]
}

fn main() -> ExitCode {
    let mut report = Report {
        text: String::new(),
        failed_gates: Vec::new(),
        failures: 0,
    };

    let version = curl::Version::get();
    report.info(
        "S00",
        &format!(
            "libcurl {} http2={} ssl={}",
            version.version(),
            version.feature_http2(),
            version.ssl_version().unwrap_or("none")
        ),
    );
    report.info(
        "S00",
        &format!(
            "target: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );

    let server = match Server::start() {
        Ok(server) => server,
        Err(error) => {
            report.check(
                "S00",
                false,
                &format!("local server did not start: {error}"),
            );
            return finish(&report);
        }
    };
    let local = Target::local(server.port);
    report.info(
        "S00",
        &format!("local h2c server on 127.0.0.1:{}", server.port),
    );

    // G01 / G02 / G08 — unary over h2c; where the trailers land; what libcurl sent.
    if let Some(mut call) = open(
        &mut report,
        "G01",
        &local,
        "/spike.Echo/Unary",
        &[b"hello".as_slice()],
        true,
    ) {
        let ended = call.run_to_end(LONG);
        let c = call.collector();
        let http2 = c.status_line().is_some_and(|s| s.starts_with("HTTP/2"));
        report.check(
            "G01",
            ended
                && call.result.as_ref().is_some_and(Result::is_ok)
                && http2
                && messages_text(&call) == ["hello"],
            &format!(
                "unary h2c: result {}, status line {:?}, messages {:?}",
                call.error_text(),
                c.status_line(),
                messages_text(&call)
            ),
        );
        let as_trailer = c.trailer("grpc-status").is_some();
        let as_header = c.header("grpc-status").is_some();
        report.check(
            "G02",
            as_trailer && !as_header,
            &format!(
                "grpc-status as trailer={as_trailer}, as header={as_header}; sequence: {}",
                c.sequence()
            ),
        );
    }
    let unary_request = server
        .log
        .find("/spike.Echo/Unary method=")
        .unwrap_or_default();
    let lower = unary_request.to_ascii_lowercase();
    report.check(
        "G08",
        lower.contains("method=post")
            && lower.contains("te: trailers")
            && lower.contains("content-type: application/grpc")
            && !lower.contains("expect:")
            && !lower.contains("transfer-encoding"),
        &format!("server saw: {unary_request}"),
    );

    // G01b — unary over TLS with ALPN h2 and the Windows certificate store.
    let public = Target {
        base: PUBLIC_TLS_BASE.to_string(),
        tls: true,
        verify: true,
        proxy: None,
    };
    if let Some(mut call) = open(
        &mut report,
        "G01b",
        &public,
        PUBLIC_TLS_METHOD,
        &[b"".as_slice()],
        true,
    ) {
        let ended = call.run_to_end(LONG);
        if call.is_offline_failure() {
            report.skip(
                "G01b",
                &format!("{PUBLIC_TLS_BASE} unreachable: {}", call.error_text()),
            );
        } else {
            let c = call.collector();
            report.check(
                "G01b",
                ended
                    && c.status_line().is_some_and(|s| s.starts_with("HTTP/2"))
                    && c.trailer("grpc-status") == Some("0"),
                &format!(
                    "TLS {PUBLIC_TLS_BASE}{PUBLIC_TLS_METHOD}: result {}, sequence: {}",
                    call.error_text(),
                    c.sequence()
                ),
            );
        }
    }

    // G03 — trailers-only: the status arrives in the header block, no body.
    if let Some(mut call) = open(
        &mut report,
        "G03",
        &local,
        "/spike.Echo/Fail",
        &[b"x".as_slice()],
        true,
    ) {
        let ended = call.run_to_end(LONG);
        let c = call.collector();
        report.check(
            "G03",
            ended
                && call.result.as_ref().is_some_and(Result::is_ok)
                && c.header("grpc-status") == Some("5")
                && c.data_bytes() == 0,
            &format!(
                "result {}, grpc-status header {:?}, grpc-message {:?}, body bytes {}; sequence: {}",
                call.error_text(),
                c.header("grpc-status"),
                c.header("grpc-message"),
                c.data_bytes(),
                c.sequence()
            ),
        );
    }

    // G04 — server streaming arrives as it is sent, not at the end.
    if let Some(mut call) = open(
        &mut report,
        "G04",
        &local,
        "/spike.Echo/ServerStream",
        &[b"5,200".as_slice()],
        true,
    ) {
        let ended = call.run_to_end(LONG);
        let c = call.collector();
        let arrivals: Vec<Duration> = c.messages.iter().map(|(at, _)| *at).collect();
        let gaps: Vec<u128> = arrivals
            .windows(2)
            .map(|w| (w[1] - w[0]).as_millis())
            .collect();
        report.check(
            "G04",
            ended && c.messages.len() == 5 && gaps.iter().all(|gap| *gap >= 150) && c.trailer("grpc-status") == Some("0"),
            &format!(
                "{} messages, gaps between arrivals {gaps:?} ms (server sends 200 ms apart), result {}",
                c.messages.len(),
                call.error_text()
            ),
        );
    }

    // G05 — client streaming: pause with nothing queued, unpause per message,
    // END_STREAM when the read callback returns 0.
    if let Some(mut call) = open(
        &mut report,
        "G05",
        &local,
        "/spike.Echo/ClientStream",
        &[],
        false,
    ) {
        call.run_until(|_| false, Duration::from_millis(300));
        let paused_before_first = call.collector().read_pauses;
        let mut sent = Ok(());
        for message in [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()] {
            sent = sent.and_then(|()| call.send(message));
            call.run_until(|_| false, Duration::from_millis(100));
        }
        let ended_stream = call.end_stream();
        let ended = call.run_to_end(LONG);
        let c = call.collector();
        report.check(
            "G05",
            sent.is_ok()
                && ended_stream.is_ok()
                && ended
                && paused_before_first > 0
                && messages_text(&call) == ["count=3 bytes=11"]
                && c.trailer("grpc-status") == Some("0"),
            &format!(
                "send {sent:?}, end {ended_stream:?}, pauses before the first send {paused_before_first}, \
                 read calls {}, pauses {}, answer {:?}, result {}",
                c.read_calls,
                c.read_pauses,
                messages_text(&call),
                call.error_text()
            ),
        );
    }

    // G06 — bidirectional, full duplex: each echo arrives before the next send.
    if let Some(mut call) = open(&mut report, "G06", &local, "/spike.Echo/Bidi", &[], false) {
        let mut latencies = Vec::with_capacity(ROUND_TRIPS);
        let mut failure = None;
        let started = Instant::now();
        for i in 0..ROUND_TRIPS {
            let payload = format!("ping {i}");
            let sent_at = Instant::now();
            if let Err(error) = call.send(payload.as_bytes()) {
                failure = Some(format!("send {i}: {error}"));
                break;
            }
            let arrived = call.run_until(|c| c.messages.len() > i, Duration::from_secs(5));
            if !arrived
                || call.collector().messages.get(i).map(|(_, m)| m.as_slice())
                    != Some(payload.as_bytes())
            {
                failure = Some(format!(
                    "echo {i} missing or wrong after {} ms (result {})",
                    sent_at.elapsed().as_millis(),
                    call.error_text()
                ));
                break;
            }
            latencies.push(sent_at.elapsed());
        }
        let total = started.elapsed();
        let ended_stream = call.end_stream();
        let ended = call.run_to_end(LONG);
        latencies.sort();
        let status = call.collector().trailer("grpc-status").map(str::to_string);
        report.check(
            "G06",
            failure.is_none() && ended_stream.is_ok() && ended && status.as_deref() == Some("0"),
            &format!(
                "{} round trips in {} ms, median {} us, p95 {} us, max {} us; then end {ended_stream:?}, status {status:?}{}",
                latencies.len(),
                total.as_millis(),
                percentile(&latencies, 50).as_micros(),
                percentile(&latencies, 95).as_micros(),
                latencies.last().copied().unwrap_or_default().as_micros(),
                failure.map(|f| format!("; FAILED: {f}")).unwrap_or_default()
            ),
        );
    }

    // G14 — where does G06's 33 ms come from? Info only, not a gate.
    // (a) Receiving alone: a server stream 5 ms apart. If the gaps arrive
    //     near 5 ms, the wait wakes on incoming data; if they bunch into
    //     25 ms steps, it does not.
    // (b) Round trips under four loop styles.
    if let Some(mut call) = open(
        &mut report,
        "G14",
        &local,
        "/spike.Echo/ServerStream",
        &[b"200,5".as_slice()],
        true,
    ) {
        call.run_to_end(LONG);
        let arrivals: Vec<Duration> = call
            .collector()
            .messages
            .iter()
            .map(|(at, _)| *at)
            .collect();
        let mut gaps: Vec<Duration> = arrivals.windows(2).map(|w| w[1] - w[0]).collect();
        gaps.sort();
        report.info(
            "G14",
            &format!(
                "(a) server stream 5 ms apart: {} messages, gap median {} us, p95 {} us, max {} us",
                arrivals.len(),
                percentile(&gaps, 50).as_micros(),
                percentile(&gaps, 95).as_micros(),
                gaps.last().copied().unwrap_or_default().as_micros()
            ),
        );
    }
    for (label, style) in [
        ("baseline: wait 25 ms", client::LoopStyle::default()),
        (
            "perform right after send",
            client::LoopStyle {
                perform_after_send: true,
                ..client::LoopStyle::default()
            },
        ),
        (
            "wait capped by libcurl's timeout",
            client::LoopStyle {
                cap_by_libcurl_timeout: true,
                ..client::LoopStyle::default()
            },
        ),
        (
            "both",
            client::LoopStyle {
                perform_after_send: true,
                cap_by_libcurl_timeout: true,
                ..client::LoopStyle::default()
            },
        ),
        (
            "wait 1 ms",
            client::LoopStyle {
                wait: Duration::from_millis(1),
                ..client::LoopStyle::default()
            },
        ),
    ] {
        let Some(mut call) = open(&mut report, "G14", &local, "/spike.Echo/Bidi", &[], false)
        else {
            continue;
        };
        call.style = style;
        let mut latencies = Vec::with_capacity(G14_ROUND_TRIPS);
        for i in 0..G14_ROUND_TRIPS {
            let payload = format!("ping {i}");
            let sent_at = Instant::now();
            if call.send(payload.as_bytes()).is_err()
                || !call.run_until(|c| c.messages.len() > i, Duration::from_secs(5))
            {
                break;
            }
            latencies.push(sent_at.elapsed());
        }
        let _ = call.end_stream();
        call.run_to_end(LONG);
        latencies.sort();
        report.info(
            "G14",
            &format!(
                "(b) {label}: {} round trips, median {} us, p95 {} us",
                latencies.len(),
                percentile(&latencies, 50).as_micros(),
                percentile(&latencies, 95).as_micros()
            ),
        );
    }

    // G07 — Cancel mid-stream: the server must see the stream stop.
    if let Some(mut call) = open(&mut report, "G07", &local, "/spike.Echo/Slow", &[], true) {
        let got_three = call.run_until(|c| c.messages.len() >= 3, LONG);
        let cancelled = call.cancel();
        std::thread::sleep(Duration::from_millis(700));
        let seen = server
            .log
            .find("/spike.Echo/Slow ended")
            .or_else(|| server.log.find("/spike.Echo/Slow completed"));
        report.check(
            "G07",
            got_three
                && cancelled.is_ok()
                && seen
                    .as_deref()
                    .is_some_and(|s| s.contains("client stopped")),
            &format!(
                "after {} messages, cancel {cancelled:?}; server: {}",
                call.collector().messages.len(),
                seen.unwrap_or_else(|| "nothing logged yet (still streaming?)".to_string())
            ),
        );
    }

    // G09 — a non-gRPC answer: the HTTP status must be readable.
    if let Some(mut call) = open(
        &mut report,
        "G09",
        &local,
        "/notgrpc",
        &[b"x".as_slice()],
        true,
    ) {
        let ended = call.run_to_end(LONG);
        report.check(
            "G09",
            ended && call.status == 404,
            &format!(
                "status {}, result {}, body bytes {}, sequence: {}",
                call.status,
                call.error_text(),
                call.collector().data_bytes(),
                call.collector().sequence()
            ),
        );
    }

    // G10 — a 16 MiB message out and back.
    let big = pattern(BIG_MESSAGE);
    if let Some(mut call) = open(
        &mut report,
        "G10",
        &local,
        "/spike.Echo/Unary",
        &[big.as_slice()],
        true,
    ) {
        let started = Instant::now();
        let ended = call.run_to_end(Duration::from_secs(60));
        let c = call.collector();
        let same = c.messages.len() == 1 && c.messages[0].1 == big;
        report.check(
            "G10",
            ended && same && c.trailer("grpc-status") == Some("0"),
            &format!(
                "{} MiB echoed intact={same} in {} ms, sent {} bytes, result {}",
                BIG_MESSAGE / (1024 * 1024),
                started.elapsed().as_millis(),
                c.bytes_sent,
                call.error_text()
            ),
        );
    }

    // G11 — through a proxy (HTTP CONNECT, then h2 over TLS).
    match std::env::var("SPIKE_PROXY") {
        Ok(proxy) if !proxy.is_empty() => {
            let proxied = Target {
                base: PUBLIC_TLS_BASE.to_string(),
                tls: true,
                verify: true,
                proxy: Some(proxy.clone()),
            };
            if let Some(mut call) = open(
                &mut report,
                "G11",
                &proxied,
                PUBLIC_TLS_METHOD,
                &[b"".as_slice()],
                true,
            ) {
                let ended = call.run_to_end(LONG);
                report.check(
                    "G11",
                    ended && call.collector().trailer("grpc-status") == Some("0"),
                    &format!(
                        "via {proxy}: result {}, sequence: {}",
                        call.error_text(),
                        call.collector().sequence()
                    ),
                );
            }
        }
        _ => report.skip(
            "G11",
            "set SPIKE_PROXY=http://host:port to test a proxied call",
        ),
    }

    // G12 — the crate is built with `unsafe_code = "forbid"` (Cargo.toml).
    report.check(
        "G12",
        true,
        "client built on the safe curl crate API only: the crate forbids unsafe code and it compiled",
    );

    report.info("LOG", "server log follows");
    for line in server.log.lines() {
        report.info("LOG", &line);
    }
    finish(&report)
}

fn finish(report: &Report) -> ExitCode {
    let mut text = report.text.clone();
    let verdict = if report.failed_gates.is_empty() {
        "VERDICT: every gate that ran passed in this build.".to_string()
    } else {
        format!("VERDICT: gates failed: {}", report.failed_gates.join(", "))
    };
    println!("INFO END  {} failure(s) in total", report.failures);
    println!("{verdict}");
    let _ = writeln!(
        text,
        "INFO END         {} failure(s) in total",
        report.failures
    );
    let _ = writeln!(text, "{verdict}");
    let _ = std::fs::write("spike-report.txt", text);
    if report.failed_gates.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

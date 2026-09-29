// spikes/grpc-libcurl/src/client.rs
//
// A gRPC call over the curl crate's Multi API: the shape 16d's transport
// would take. One Easy2 per call, driven by the thread that owns it.
//
// - Request messages go through the read callback. With nothing queued it
//   returns `ReadError::Pause`. `send` queues a frame and unpauses. Returning
//   0 once `end_stream` has been called ends the request stream (END_STREAM).
// - Response headers and trailers both arrive in the header callback. They
//   are told apart by position: anything after the blank line that ends the
//   header block is a trailer. Gate 2 is whether libcurl delivers HTTP/2
//   trailers there at all.
// - Body chunks go through the write callback into the frame decoder.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use curl::easy::{Easy2, Handler, HttpVersion, List, ReadError, SslOpt, WriteError};
use curl::multi::{Easy2Handle, Multi};

use crate::frame;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Status(String),
    Header(String, String),
    HeadersEnd,
    Data(usize),
    Trailer(String, String),
    TrailersEnd,
}

pub struct Collector {
    started: Instant,
    pub events: Vec<(Duration, Event)>,
    pub messages: Vec<(Duration, Vec<u8>)>,
    decoder: frame::Decoder,
    outgoing: VecDeque<u8>,
    finished: bool,
    headers_done: bool,
    pub read_calls: u32,
    pub read_pauses: u32,
    pub bytes_sent: usize,
    /// True while the read callback's last answer was `Pause`. Unpausing is
    /// only valid then: before the transfer has a connection,
    /// `curl_easy_pause` refuses with CURLE_BAD_FUNCTION_ARGUMENT (43), which
    /// is what the first Windows run of G06 hit.
    read_paused: bool,
}

impl Collector {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            events: Vec::new(),
            messages: Vec::new(),
            decoder: frame::Decoder::default(),
            outgoing: VecDeque::new(),
            finished: false,
            headers_done: false,
            read_calls: 0,
            read_pauses: 0,
            read_paused: false,
            bytes_sent: 0,
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.events.iter().find_map(|(_, e)| match e {
            Event::Header(n, v) if n == name => Some(v.as_str()),
            _ => None,
        })
    }

    pub fn trailer(&self, name: &str) -> Option<&str> {
        self.events.iter().find_map(|(_, e)| match e {
            Event::Trailer(n, v) if n == name => Some(v.as_str()),
            _ => None,
        })
    }

    pub fn status_line(&self) -> Option<&str> {
        self.events.iter().find_map(|(_, e)| match e {
            Event::Status(s) => Some(s.as_str()),
            _ => None,
        })
    }

    pub fn data_bytes(&self) -> usize {
        self.events
            .iter()
            .map(|(_, e)| if let Event::Data(n) = e { *n } else { 0 })
            .sum()
    }

    /// The event sequence with data chunks collapsed, for the report.
    pub fn sequence(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (_, event) in &self.events {
            let part = match event {
                Event::Status(s) => format!("status[{s}]"),
                Event::Header(n, v) => format!("h[{n}={v}]"),
                Event::HeadersEnd => "end-of-headers".to_string(),
                Event::Data(_) => "data".to_string(),
                Event::Trailer(n, v) => format!("TRAILER[{n}={v}]"),
                Event::TrailersEnd => "end-of-trailers".to_string(),
            };
            if part == "data" && parts.last().is_some_and(|p| p.starts_with("data")) {
                continue;
            }
            parts.push(part);
        }
        parts.join(" ")
    }
}

fn split_header(line: &str) -> (String, String) {
    match line.split_once(':') {
        Some((name, value)) => (name.trim().to_ascii_lowercase(), value.trim().to_string()),
        None => (line.trim().to_ascii_lowercase(), String::new()),
    }
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        let at = self.started.elapsed();
        self.events.push((at, Event::Data(data.len())));
        for message in self.decoder.push(data) {
            self.messages.push((at, message));
        }
        Ok(data.len())
    }

    fn read(&mut self, into: &mut [u8]) -> Result<usize, ReadError> {
        self.read_calls += 1;
        if self.outgoing.is_empty() {
            if self.finished {
                return Ok(0);
            }
            self.read_pauses += 1;
            self.read_paused = true;
            return Err(ReadError::Pause);
        }
        let n = into.len().min(self.outgoing.len());
        for (slot, byte) in into.iter_mut().zip(self.outgoing.drain(..n)) {
            *slot = byte;
        }
        self.bytes_sent += n;
        Ok(n)
    }

    fn header(&mut self, data: &[u8]) -> bool {
        let at = self.started.elapsed();
        let line = String::from_utf8_lossy(data).trim_end().to_string();
        let event = if line.starts_with("HTTP/") {
            self.headers_done = false;
            Event::Status(line)
        } else if line.is_empty() {
            if self.headers_done {
                Event::TrailersEnd
            } else {
                self.headers_done = true;
                Event::HeadersEnd
            }
        } else {
            let (name, value) = split_header(&line);
            if self.headers_done {
                Event::Trailer(name, value)
            } else {
                Event::Header(name, value)
            }
        };
        self.events.push((at, event));
        true
    }
}

pub struct Target {
    /// Scheme, host and port, no path: `http://127.0.0.1:50051`.
    pub base: String,
    pub tls: bool,
    pub verify: bool,
    pub proxy: Option<String>,
}

impl Target {
    pub fn local(port: u16) -> Self {
        Self {
            base: format!("http://127.0.0.1:{port}"),
            tls: false,
            verify: true,
            proxy: None,
        }
    }
}

/// How the owner loop waits. The default is what G01–G12 ran with; G14
/// tries the alternatives to find where G06's 33 ms round trip comes from.
#[derive(Debug, Clone, Copy)]
pub struct LoopStyle {
    /// The longest `multi.wait` may block.
    pub wait: Duration,
    /// Shorten the wait to libcurl's own next timeout (`curl_multi_timeout`).
    pub cap_by_libcurl_timeout: bool,
    /// Run `multi.perform()` straight after queuing a message, instead of
    /// leaving it to the next turn of the loop.
    pub perform_after_send: bool,
}

impl Default for LoopStyle {
    fn default() -> Self {
        Self {
            wait: Duration::from_millis(25),
            cap_by_libcurl_timeout: false,
            perform_after_send: false,
        }
    }
}

pub struct Call {
    pub style: LoopStyle,
    multi: Multi,
    running: Option<Easy2Handle<Collector>>,
    done: Option<Easy2<Collector>>,
    pub result: Option<Result<(), curl::Error>>,
    pub status: u32,
    pub cancelled: bool,
}

fn describe(error: &curl::Error) -> String {
    format!(
        "curl {} ({}) [{}]",
        error.code(),
        error.description(),
        error.extra_description().unwrap_or_default()
    )
}

impl Call {
    /// `initial` messages are queued before the transfer starts. `end` also
    /// ends the request stream up front, which is what unary and
    /// server-streaming calls do.
    pub fn open(target: &Target, path: &str, initial: &[&[u8]], end: bool) -> Result<Self, String> {
        let mut collector = Collector::new();
        for message in initial {
            collector.outgoing.extend(frame::encode(message));
        }
        collector.finished = end;

        let e = |error: curl::Error| describe(&error);
        let mut easy = Easy2::new(collector);
        easy.url(&format!("{}{path}", target.base)).map_err(e)?;
        easy.http_version(if target.tls {
            HttpVersion::V2TLS
        } else {
            HttpVersion::V2PriorKnowledge
        })
        .map_err(e)?;
        // POST with no size: the body comes from the read callback, for as
        // long as it takes.
        easy.post(true).map_err(e)?;
        let mut headers = List::new();
        for header in [
            "content-type: application/grpc",
            "te: trailers",
            "user-agent: grpc-libcurl-spike/0",
        ] {
            headers.append(header).map_err(e)?;
        }
        easy.http_headers(headers).map_err(e)?;
        easy.ssl_options(SslOpt::new().native_ca(true)).map_err(e)?;
        easy.ssl_verify_peer(target.verify).map_err(e)?;
        easy.ssl_verify_host(target.verify).map_err(e)?;
        if let Some(proxy) = &target.proxy {
            easy.proxy(proxy).map_err(e)?;
        }
        easy.signal(false).map_err(e)?;
        easy.connect_timeout(Duration::from_secs(5)).map_err(e)?;
        if std::env::var_os("SPIKE_VERBOSE").is_some() {
            easy.verbose(true).map_err(e)?;
        }

        let multi = Multi::new();
        let handle = multi.add2(easy).map_err(|error| error.to_string())?;
        Ok(Self {
            style: LoopStyle::default(),
            multi,
            running: Some(handle),
            done: None,
            result: None,
            status: 0,
            cancelled: false,
        })
    }

    pub fn collector(&self) -> &Collector {
        match (&self.running, &self.done) {
            (Some(handle), _) => handle.get_ref(),
            (None, Some(easy)) => easy.get_ref(),
            (None, None) => unreachable!("a call always holds its handle or its finished easy"),
        }
    }

    pub fn is_done(&self) -> bool {
        self.running.is_none()
    }

    pub fn send(&mut self, message: &[u8]) -> Result<(), String> {
        let handle = self.running.as_mut().ok_or("the call is over")?;
        handle.get_mut().outgoing.extend(frame::encode(message));
        Self::resume_reading(handle)?;
        if self.style.perform_after_send {
            self.multi.perform().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn end_stream(&mut self) -> Result<(), String> {
        let handle = self.running.as_mut().ok_or("the call is over")?;
        handle.get_mut().finished = true;
        Self::resume_reading(handle)
    }

    /// Unpauses only a read side that is actually paused. If libcurl has not
    /// asked for data yet, what was queued is picked up by its first read.
    /// The flag is cleared *before* unpausing, because `curl_easy_pause` can
    /// call the read callback straight away, and that call may pause again.
    fn resume_reading(handle: &mut Easy2Handle<Collector>) -> Result<(), String> {
        if !handle.get_ref().read_paused {
            return Ok(());
        }
        handle.get_mut().read_paused = false;
        handle.unpause_read().map_err(|error| describe(&error))
    }

    /// One turn of the owner loop: let libcurl work, collect a finished
    /// transfer, then wait on the sockets for at most `timeout`.
    pub fn pump(&mut self, timeout: Duration) -> Result<(), String> {
        let Some(handle) = self.running.as_ref() else {
            return Ok(());
        };
        self.multi.perform().map_err(|error| error.to_string())?;
        let mut finished = None;
        self.multi.messages(|message| {
            if let Some(result) = message.result_for2(handle) {
                finished = Some(result);
            }
        });
        if let Some(result) = finished {
            self.finish(result)?;
            return Ok(());
        }
        let mut wait = timeout;
        if self.style.cap_by_libcurl_timeout {
            if let Ok(Some(libcurl)) = self.multi.get_timeout() {
                wait = wait.min(libcurl);
            }
        }
        self.multi
            .wait(&mut [], wait)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn finish(&mut self, result: Result<(), curl::Error>) -> Result<(), String> {
        if let Some(handle) = self.running.take() {
            let easy = self
                .multi
                .remove2(handle)
                .map_err(|error| error.to_string())?;
            self.status = easy.response_code().unwrap_or(0);
            self.done = Some(easy);
        }
        self.result = Some(result);
        Ok(())
    }

    /// Pumps until `until` holds, the call ends, or `limit` passes. True
    /// when `until` held or the call ended.
    pub fn run_until<F: Fn(&Collector) -> bool>(&mut self, until: F, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if until(self.collector()) || self.is_done() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            if self.pump(self.style.wait).is_err() {
                return self.is_done();
            }
        }
    }

    pub fn run_to_end(&mut self, limit: Duration) -> bool {
        self.run_until(|_| false, limit) && self.is_done()
    }

    /// Stops the call mid-flight by taking the handle out of the multi,
    /// which is how 16d's Cancel would work. The multi is pumped a little
    /// afterwards so anything libcurl queued (a RST_STREAM) goes out.
    pub fn cancel(&mut self) -> Result<(), String> {
        if let Some(handle) = self.running.take() {
            let easy = self
                .multi
                .remove2(handle)
                .map_err(|error| error.to_string())?;
            self.done = Some(easy);
            self.cancelled = true;
            let until = Instant::now() + Duration::from_millis(300);
            while Instant::now() < until {
                let _ = self.multi.perform();
                let _ = self.multi.wait(&mut [], Duration::from_millis(25));
            }
        }
        Ok(())
    }

    pub fn error_text(&self) -> String {
        match &self.result {
            Some(Err(error)) => describe(error),
            Some(Ok(())) => "ok".to_string(),
            None => "still running".to_string(),
        }
    }

    /// True when the call failed before reaching the server: no network,
    /// not something libcurl did wrong. Used to skip public-server gates.
    pub fn is_offline_failure(&self) -> bool {
        matches!(&self.result, Some(Err(error)) if error.is_couldnt_resolve_host()
            || error.is_couldnt_connect()
            || error.is_operation_timedout())
    }
}

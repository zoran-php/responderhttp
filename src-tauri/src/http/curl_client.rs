// http_client/src-tauri/src/http/curl_client.rs
//
// The libcurl implementation of domain::ports::HttpClient. libcurl is
// statically linked (see Cargo.toml); nothing here shells out to a curl
// binary, and nothing here knows about Tauri.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use curl::easy::{
    Easy2, Form, Handler, HttpVersion, List, PostRedirections, SslOpt, SslVersion, WriteError,
};

use crate::domain::cancellation::CancellationToken;
use crate::domain::clock::now_ms;
use crate::domain::error::AppError;
use crate::domain::models::{
    HttpMethod, HttpRequest, HttpResponse, HttpVersionPreference, KeyValue, RequestBody,
    RequestSettings, ResponseBody, TlsMinimum, TransferSizes,
};
use crate::domain::ports::{HttpClient, HttpStreamSink, HttpStreamUpdate};
use crate::domain::sse::SseParser;
use crate::http::auth::strategy_for;
use crate::http::mapping::{
    build_form, build_url, encode_body, parse_header_line, status_from_curl, timing_from_offsets,
};

const CONTENT_TYPE: &str = "content-type";
const SET_COOKIE: &str = "set-cookie";

/// The most raw body kept for an event stream. The events themselves have
/// already been reported by then; this is only what the final HttpResponse
/// carries, and a stream that never ends must not grow it without bound
/// (PLAN-SSE.md section 4).
const MAX_STREAM_BODY_BYTES: usize = 8 * 1024 * 1024;

const EVENT_STREAM: &str = "text/event-stream";

struct Collector<'a> {
    body: Vec<u8>,
    /// Every byte the body received, counted even past the stream cap: the
    /// size shown is what arrived, not what is still held.
    body_bytes: u64,
    headers: Vec<KeyValue>,
    /// The current hop's header block, status line and blank line included.
    /// Reset at each hop, as `headers` is.
    header_bytes: u64,
    /// Accumulated across every hop, unlike `headers`: a redirect that sets
    /// a cookie is the normal login flow, and clearing here would drop it.
    set_cookies: Vec<String>,
    cancel: CancellationToken,
    stream: Stream<'a>,
}

/// Everything about reporting a response while it is still arriving. With
/// no sink it costs one branch per chunk and nothing else.
struct Stream<'a> {
    sink: Option<HttpStreamSink<'a>>,
    parser: SseParser,
    /// Set when the response says `Content-Type: text/event-stream`, and
    /// cleared at each redirect hop until the final response says it.
    events: bool,
    status: u16,
    /// Nobody is listening any more; the transfer is abandoned.
    dropped: bool,
    /// When the request has run too long. `None` once the response turns out
    /// to be an event stream: a stream is meant to stay open (decision D3),
    /// and libcurl's own timeout cannot be relaxed mid-transfer, which is
    /// why the deadline lives here rather than in CURLOPT_TIMEOUT.
    deadline: Option<Instant>,
    timed_out: Arc<AtomicBool>,
}

impl<'a> Collector<'a> {
    fn new(
        cancel: CancellationToken,
        timeout: Duration,
        timed_out: Arc<AtomicBool>,
        sink: Option<HttpStreamSink<'a>>,
    ) -> Self {
        Self {
            body: Vec::new(),
            body_bytes: 0,
            headers: Vec::new(),
            header_bytes: 0,
            set_cookies: Vec::new(),
            cancel,
            stream: Stream {
                sink,
                parser: SseParser::new(),
                events: false,
                status: 0,
                dropped: false,
                deadline: (!timeout.is_zero()).then(|| Instant::now() + timeout),
                timed_out,
            },
        }
    }

    /// False when the sink has gone, which stops the transfer.
    ///
    /// Once it has gone it is never called again: libcurl finishes the chunk
    /// it is in before the progress callback can abort, and a sink that said
    /// it had stopped listening must not be handed another update in the
    /// meantime.
    fn report(&mut self, update: HttpStreamUpdate) -> bool {
        if self.stream.dropped {
            return false;
        }
        let Some(sink) = self.stream.sink.as_mut() else {
            return true;
        };
        if sink(update) {
            return true;
        }
        self.stream.dropped = true;
        false
    }

    /// The end of a header block: the response's status and headers are
    /// complete, and whether it is an event stream is now known.
    fn headers_complete(&mut self) {
        self.stream.events = self
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(CONTENT_TYPE))
            .is_some_and(|header| is_event_stream(&header.value));
        if self.stream.events {
            // A stream is not late, however long it stays open.
            self.stream.deadline = None;
        }
        let update = HttpStreamUpdate::Headers {
            status: self.stream.status,
            headers: self.headers.clone(),
            bytes: self.header_bytes,
        };
        self.report(update);
    }
}

/// `text/event-stream`, with whatever parameters follow it.
fn is_event_stream(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case(EVENT_STREAM))
}

/// `HTTP/1.1 200 OK` and its HTTP/2 equivalent.
fn status_from_line(line: &[u8]) -> u16 {
    String::from_utf8_lossy(line)
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

impl Handler for Collector<'_> {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        // Bytes, not String: a response may be an image or a gzip blob, and
        // the text decision belongs to ResponseBody::from_bytes.
        self.body_bytes += data.len() as u64;
        if !self.stream.events || self.body.len() < MAX_STREAM_BODY_BYTES {
            self.body.extend_from_slice(data);
        }
        if self.stream.events && self.stream.sink.is_some() {
            for block in self.stream.parser.push(data) {
                if !self.report(HttpStreamUpdate::Block {
                    at_ms: now_ms(),
                    block,
                }) {
                    break;
                }
            }
        }
        Ok(data.len())
    }

    fn header(&mut self, data: &[u8]) -> bool {
        if data.starts_with(b"HTTP/") {
            // A new status line means a redirect hop: keep only the final
            // response's headers, and count only its bytes.
            self.headers.clear();
            self.header_bytes = data.len() as u64;
            self.stream.status = status_from_line(data);
            return true;
        }
        self.header_bytes += data.len() as u64;
        if let Some(header) = parse_header_line(data) {
            if header.name.eq_ignore_ascii_case(SET_COOKIE) {
                self.set_cookies.push(header.value.clone());
            }
            self.headers.push(header);
            return true;
        }
        // The blank line that ends the header block. Tested for directly:
        // an obsolete folded continuation line also fails to parse as a
        // header, and must not be mistaken for the end.
        if self.stream.sink.is_some() && data.iter().all(u8::is_ascii_whitespace) {
            self.headers_complete();
        }
        true
    }

    /// libcurl polls this during the transfer; returning false aborts it.
    /// This is the only place a request in flight can be stopped, and since
    /// the total timeout moved here (decision D3), the only place it ends.
    fn progress(&mut self, _dltotal: f64, _dlnow: f64, _ultotal: f64, _ulnow: f64) -> bool {
        if self.cancel.is_cancelled() || self.stream.dropped {
            return false;
        }
        if self
            .stream
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.stream.timed_out.store(true, Ordering::SeqCst);
            return false;
        }
        true
    }
}

#[derive(Default)]
pub struct CurlClient;

impl CurlClient {
    pub fn new() -> Self {
        Self
    }
}

impl HttpClient for CurlClient {
    fn send(
        &self,
        request: &HttpRequest,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse, AppError> {
        run(request, cancel, None)
    }

    fn send_streaming(
        &self,
        request: &HttpRequest,
        cancel: &CancellationToken,
        on_update: HttpStreamSink<'_>,
    ) -> Result<HttpResponse, AppError> {
        run(request, cancel, Some(on_update))
    }
}

fn run(
    request: &HttpRequest,
    cancel: &CancellationToken,
    sink: Option<HttpStreamSink<'_>>,
) -> Result<HttpResponse, AppError> {
    // Built before the handle exists so a bad part is an InvalidRequest
    // rather than being flattened into a transport failure by the map
    // below. `perform` deals only in curl errors, and this is not one.
    let form = match &request.body {
        RequestBody::Multipart(parts) => Some(build_form(parts)?),
        _ => None,
    };
    // Shared because the handler owns itself inside the handle: the outer
    // code has to know afterwards why libcurl was told to stop.
    let timed_out = Arc::new(AtomicBool::new(false));
    perform(request, form, cancel, sink, timed_out.clone()).map_err(|err| {
        if timed_out.load(Ordering::SeqCst) {
            AppError::Transport(format!(
                "the request timed out after {} ms",
                request.settings.timeout.as_millis()
            ))
        } else {
            AppError::Transport(describe(&err))
        }
    })
}

fn perform(
    request: &HttpRequest,
    form: Option<Form>,
    cancel: &CancellationToken,
    sink: Option<HttpStreamSink<'_>>,
    timed_out: Arc<AtomicBool>,
) -> Result<HttpResponse, curl::Error> {
    let mut easy = Easy2::new(Collector::new(
        cancel.clone(),
        request.settings.timeout,
        timed_out,
        sink,
    ));

    // Resolved once: an API key bound to the query string has to be present
    // before the URL is built, and the headers are needed further down.
    let auth = strategy_for(&request.auth).apply();

    easy.url(&build_url(
        &request.url,
        &merge_query_params(&request.query_params, &auth.query_params),
        request.settings.encode_url,
    ))?;
    easy.custom_request(request.method.as_str())?;
    if request.method == HttpMethod::Head {
        // Otherwise libcurl waits for a body that will never arrive.
        easy.nobody(true)?;
    }
    easy.http_version(match request.settings.http_version {
        // Attempt h2 over TLS, fall back to 1.1 — what this client did before
        // the setting existed, so Auto is not a behaviour change.
        HttpVersionPreference::Auto => HttpVersion::V2TLS,
        HttpVersionPreference::Http11 => HttpVersion::V11,
        HttpVersionPreference::Http2 => HttpVersion::V2,
    })?;
    // Empty string means "every encoding this libcurl supports": it sets
    // Accept-Encoding and decompresses the response, so the viewer gets text
    // rather than a gzip blob.
    easy.accept_encoding("")?;
    // The progress callback is how cancellation reaches libcurl, so it has
    // to be enabled even though the numbers themselves go unused.
    easy.progress(true)?;

    apply_settings(&mut easy, &request.settings)?;
    apply_body_and_headers(&mut easy, request, &auth.headers)?;
    if let Some(form) = form {
        // libcurl owns the multipart body: it picks the boundary, sets
        // Content-Type, and reads any file parts from disk as it sends, so an
        // upload never sits in this process's memory. Like post_fields_copy
        // it switches the handle to POST, so the method is reasserted.
        easy.httppost(form)?;
        easy.custom_request(request.method.as_str())?;
    }

    easy.perform()?;

    // Whatever the server sent without its closing blank line still counts.
    let trailing: Vec<_> = easy.get_mut().stream.parser.finish();
    for block in trailing {
        let collector = easy.get_mut();
        if !collector.report(HttpStreamUpdate::Block {
            at_ms: now_ms(),
            block,
        }) {
            break;
        }
    }

    let status = status_from_curl(&easy)?;
    // Only the request half comes from libcurl: it is the only side that
    // knows what it sent, headers it added included. The response half is
    // counted in the collector so that a stream can show it growing, and so
    // that the running total and the final one cannot disagree.
    let request_headers = easy.request_size()?;
    // CURLINFO_SIZE_UPLOAD is a double; whole bytes are what is shown. max
    // also folds away a NaN, which would otherwise cast to something absurd.
    let request_body = easy.upload_size()?.max(0.0) as u64;
    let timing = timing_from_offsets(
        easy.namelookup_time()?,
        easy.connect_time()?,
        easy.appconnect_time()?,
        easy.starttransfer_time()?,
        easy.total_time()?,
    );

    let collector = easy.get_mut();
    Ok(HttpResponse {
        status,
        headers: std::mem::take(&mut collector.headers),
        body: ResponseBody::from_bytes(std::mem::take(&mut collector.body)),
        timing,
        sizes: TransferSizes {
            request_headers,
            request_body,
            response_headers: collector.header_bytes,
            response_body: collector.body_bytes,
        },
        set_cookies: std::mem::take(&mut collector.set_cookies),
    })
}

fn apply_settings<H>(easy: &mut Easy2<H>, settings: &RequestSettings) -> Result<(), curl::Error> {
    easy.follow_location(settings.follow_redirects)?;
    easy.max_redirections(settings.max_redirects)?;
    // Not CURLOPT_TIMEOUT: the deadline is enforced in the progress callback
    // so that it can be lifted when the response turns out to be an event
    // stream, which libcurl cannot do once a transfer has started
    // (PLAN-SSE.md, decision D3).
    apply_tls_and_proxy(easy, settings.verify_tls, settings.proxy.as_deref())?;

    // libcurl (and every browser) turns POST into GET on 301/302/303. Some
    // APIs expect the method to survive, so this is opt-in rather than a
    // silent difference from what curl on the command line would do.
    if settings.keep_method_on_redirect {
        let mut redirects = PostRedirections::new();
        redirects.redirect_all(true);
        easy.post_redirections(&redirects)?;
    }
    // Off by default deliberately: on, the Authorization header follows a
    // redirect to a different host, which is how a credential reaches a
    // server that was never meant to see it.
    easy.unrestricted_auth(settings.keep_auth_on_redirect)?;
    easy.http_09_allowed(settings.allow_http_09)?;

    // Auto leaves libcurl's default untouched rather than asserting one, so
    // the common path sets nothing here at all. rustls offers no version
    // below 1.2, which is why there is no lower floor to pick.
    match settings.tls_minimum {
        TlsMinimum::Auto => {}
        TlsMinimum::Tls12 => easy.ssl_version(SslVersion::Tlsv12)?,
        TlsMinimum::Tls13 => easy.ssl_version(SslVersion::Tlsv13)?,
    }
    Ok(())
}

/// TLS verification and the proxy, set the same way for an HTTP request and
/// a WebSocket handshake (http/curl_websocket.rs), so the two can never
/// disagree about either.
pub(crate) fn apply_tls_and_proxy<H>(
    easy: &mut Easy2<H>,
    verify_tls: bool,
    proxy: Option<&str>,
) -> Result<(), curl::Error> {
    // Verification is on unless the user explicitly turned it off for this
    // request (CLAUDE.md section 11, rule 7).
    easy.ssl_verify_peer(verify_tls)?;
    easy.ssl_verify_host(verify_tls)?;
    if verify_tls {
        // rustls ships no trust store of its own, so without this every HTTPS
        // request fails at handshake with "no server certificate verifier".
        // The OS store keeps the binary self-contained — see PLAN.md Phase 0.
        easy.ssl_options(SslOpt::new().native_ca(true))?;
    }
    match proxy.map(str::trim) {
        Some(proxy) if !proxy.is_empty() => easy.proxy(proxy)?,
        _ => {}
    }
    Ok(())
}

/// An API key in the query string is a query parameter like any other, so it
/// joins the user's own rather than being special-cased in `build_url`.
fn merge_query_params(request: &[KeyValue], auth: &[KeyValue]) -> Vec<KeyValue> {
    if auth.is_empty() {
        return request.to_vec();
    }
    let mut merged = request.to_vec();
    merged.extend_from_slice(auth);
    merged
}

fn apply_body_and_headers<H>(
    easy: &mut Easy2<H>,
    request: &HttpRequest,
    auth_headers: &[KeyValue],
) -> Result<(), curl::Error> {
    let encoded = encode_body(&request.body);
    let user_set_content_type = request
        .headers
        .iter()
        .any(|header| header.name.trim().eq_ignore_ascii_case(CONTENT_TYPE));

    let mut list = List::new();
    for header in &request.headers {
        let name = header.name.trim();
        if name.is_empty() {
            continue;
        }
        list.append(&format!("{name}: {}", header.value))?;
    }
    // Same rule as Content-Type below: a header typed into the Headers tab
    // beats the one the Auth tab generates, because typing it is deliberate.
    for header in auth_headers {
        let name = header.name.trim();
        if name.is_empty() {
            continue;
        }
        let overridden = request
            .headers
            .iter()
            .any(|existing| existing.name.trim().eq_ignore_ascii_case(name));
        if overridden {
            continue;
        }
        list.append(&format!("{name}: {}", header.value))?;
    }
    // An explicit header wins: the user may be overriding a charset or
    // sending a raw body under a different type on purpose.
    if let Some(content_type) = encoded
        .as_ref()
        .and_then(|body| body.content_type.as_deref())
        .filter(|_| !user_set_content_type)
    {
        list.append(&format!("Content-Type: {content_type}"))?;
    }
    // Stops libcurl announcing "Expect: 100-continue" on larger bodies, which
    // adds a round trip and confuses servers that never answer it.
    list.append("Expect:")?;
    easy.http_headers(list)?;

    if let Some(body) = encoded {
        // post_fields_copy switches the handle to POST, so the method has to
        // be reasserted afterwards to keep PUT/PATCH/DELETE intact.
        easy.post_fields_copy(&body.bytes)?;
        easy.custom_request(request.method.as_str())?;
    }
    Ok(())
}

/// curl error text is safe to surface: it describes transport state, never
/// request bodies or credentials (CLAUDE.md section 11, rule 6).
///
/// Display already appends `extra_description()`, so adding it again here
/// printed the reason twice.
fn describe(error: &curl::Error) -> String {
    error.to_string()
}

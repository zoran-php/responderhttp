// http_client/src-tauri/src/http/curl_client.rs
//
// The libcurl implementation of domain::ports::HttpClient. libcurl is
// statically linked (see Cargo.toml); nothing here shells out to a curl
// binary, and nothing here knows about Tauri.
use curl::easy::{
    Easy2, Form, Handler, HttpVersion, List, PostRedirections, SslOpt, SslVersion, WriteError,
};

use crate::domain::cancellation::CancellationToken;
use crate::domain::error::AppError;
use crate::domain::models::{
    HttpMethod, HttpRequest, HttpResponse, HttpVersionPreference, KeyValue, RequestBody,
    RequestSettings, ResponseBody, TlsMinimum,
};
use crate::domain::ports::HttpClient;
use crate::http::auth::strategy_for;
use crate::http::mapping::{
    build_form, build_url, encode_body, parse_header_line, status_from_curl, timing_from_offsets,
};

const CONTENT_TYPE: &str = "content-type";
const SET_COOKIE: &str = "set-cookie";

struct Collector {
    body: Vec<u8>,
    headers: Vec<KeyValue>,
    /// Accumulated across every hop, unlike `headers`: a redirect that sets
    /// a cookie is the normal login flow, and clearing here would drop it.
    set_cookies: Vec<String>,
    cancel: CancellationToken,
}

impl Collector {
    fn new(cancel: CancellationToken) -> Self {
        Self {
            body: Vec::new(),
            headers: Vec::new(),
            set_cookies: Vec::new(),
            cancel,
        }
    }
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        // Bytes, not String: a response may be an image or a gzip blob, and
        // the text decision belongs to ResponseBody::from_bytes.
        self.body.extend_from_slice(data);
        Ok(data.len())
    }

    fn header(&mut self, data: &[u8]) -> bool {
        if data.starts_with(b"HTTP/") {
            // A new status line means a redirect hop: keep only the final
            // response's headers.
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

    /// libcurl polls this during the transfer; returning false aborts it.
    /// This is the only place a request in flight can be stopped.
    fn progress(&mut self, _dltotal: f64, _dlnow: f64, _ultotal: f64, _ulnow: f64) -> bool {
        !self.cancel.is_cancelled()
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
        // Built before the handle exists so a bad part is an InvalidRequest
        // rather than being flattened into a transport failure by the map
        // below. `perform` deals only in curl errors, and this is not one.
        let form = match &request.body {
            RequestBody::Multipart(parts) => Some(build_form(parts)?),
            _ => None,
        };
        perform(request, form, cancel).map_err(|err| AppError::Transport(describe(&err)))
    }
}

fn perform(
    request: &HttpRequest,
    form: Option<Form>,
    cancel: &CancellationToken,
) -> Result<HttpResponse, curl::Error> {
    let mut easy = Easy2::new(Collector::new(cancel.clone()));

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

    let status = status_from_curl(&easy)?;
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
        set_cookies: std::mem::take(&mut collector.set_cookies),
    })
}

fn apply_settings<H>(easy: &mut Easy2<H>, settings: &RequestSettings) -> Result<(), curl::Error> {
    easy.follow_location(settings.follow_redirects)?;
    easy.max_redirections(settings.max_redirects)?;
    easy.timeout(settings.timeout)?;
    // Verification is on unless the user explicitly turned it off for this
    // request (CLAUDE.md section 11, rule 7).
    easy.ssl_verify_peer(settings.verify_tls)?;
    easy.ssl_verify_host(settings.verify_tls)?;
    if settings.verify_tls {
        // rustls ships no trust store of its own, so without this every HTTPS
        // request fails at handshake with "no server certificate verifier".
        // The OS store keeps the binary self-contained — see PLAN.md Phase 0.
        easy.ssl_options(SslOpt::new().native_ca(true))?;
    }
    match settings.proxy.as_deref().map(str::trim) {
        Some(proxy) if !proxy.is_empty() => easy.proxy(proxy)?,
        _ => {}
    }

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

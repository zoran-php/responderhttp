// http_client/src-tauri/tests/curl_client.rs
//
// Integration tests for the libcurl client against a local mock server.
// These never touch the network beyond loopback.
//
// Not covered here: TLS. httpmock serves plain HTTP, so the rustls path and
// the native-CA setting — the parts most likely to fail on a user's machine
// — are covered by spikes/static-link-proof and by manual checks instead.
use std::time::Duration;

use httpmock::prelude::*;

use responderhttp_lib::domain::cancellation::CancellationToken;
use responderhttp_lib::domain::error::AppError;
use responderhttp_lib::domain::models::{
    Auth, HttpMethod, HttpRequest, KeyValue, MultipartPart, RequestBody, RequestSettings,
    ResponseBody,
};
use responderhttp_lib::domain::ports::HttpClient;
use responderhttp_lib::http::curl_client::CurlClient;

/// "gzipped hello", gzip-compressed once and pasted here so the tests need
/// no compression crate just to build a fixture.
const GZIPPED_HELLO: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x4b, 0xaf, 0xca, 0x2c, 0x28, 0x48,
    0x4d, 0x51, 0xc8, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x65, 0x8f, 0x96, 0x8f, 0x0d, 0x00, 0x00,
    0x00,
];

/// A 1x1 PNG header: not valid UTF-8, so it must come back as binary.
const PNG_BYTES: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xfe];

fn request(method: HttpMethod, url: String) -> HttpRequest {
    HttpRequest {
        method,
        url,
        headers: Vec::new(),
        query_params: Vec::new(),
        body: RequestBody::None,
        auth: Auth::None,
        settings: RequestSettings::default(),
    }
}

/// The tests that do not exercise cancellation still need a token to pass.
fn send(
    client: &CurlClient,
    request: &HttpRequest,
) -> Result<responderhttp_lib::domain::models::HttpResponse, AppError> {
    client.send(request, &CancellationToken::new())
}

#[test]
fn returns_status_headers_and_text_body() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/hello");
        then.status(201)
            .header("content-type", "text/plain")
            .header("x-custom", "value")
            .body("hello");
    });

    let response = send(
        &CurlClient::new(),
        &request(HttpMethod::Get, server.url("/hello")),
    )
    .expect("request should succeed");

    mock.assert();
    assert_eq!(response.status, 201);
    assert_eq!(response.body, ResponseBody::Text("hello".into()));
    assert!(response
        .headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case("x-custom") && header.value == "value"));
    assert!(response.timing.total > Duration::ZERO);
}

#[test]
fn follows_redirects_and_keeps_only_the_final_headers() {
    let server = MockServer::start();
    let redirect = server.mock(|when, then| {
        when.method(GET).path("/start");
        then.status(301)
            .header("location", "/end")
            .header("x-hop", "first");
    });
    let destination = server.mock(|when, then| {
        when.method(GET).path("/end");
        then.status(200).header("x-hop", "last").body("arrived");
    });

    let response = send(
        &CurlClient::new(),
        &request(HttpMethod::Get, server.url("/start")),
    )
    .expect("redirect should be followed");

    redirect.assert();
    destination.assert();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, ResponseBody::Text("arrived".into()));
    let hops: Vec<&str> = response
        .headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case("x-hop"))
        .map(|header| header.value.as_str())
        .collect();
    assert_eq!(hops, vec!["last"], "headers from the redirect hop leaked");
}

#[test]
fn reports_a_timeout_as_a_transport_failure() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/slow");
        then.status(200).delay(Duration::from_secs(2)).body("late");
    });

    let mut slow = request(HttpMethod::Get, server.url("/slow"));
    slow.settings.timeout = Duration::from_millis(200);

    let error = send(&CurlClient::new(), &slow).expect_err("a slow response should time out");

    assert!(matches!(error, AppError::Transport(_)), "got {error:?}");
}

#[test]
fn decompresses_a_gzip_response() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/gzip");
        then.status(200)
            .header("content-encoding", "gzip")
            .body(GZIPPED_HELLO);
    });

    let response = send(
        &CurlClient::new(),
        &request(HttpMethod::Get, server.url("/gzip")),
    )
    .expect("gzip response should succeed");

    assert_eq!(response.body, ResponseBody::Text("gzipped hello".into()));
}

#[test]
fn keeps_a_binary_body_as_bytes() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/image");
        then.status(200)
            .header("content-type", "image/png")
            .body(PNG_BYTES);
    });

    let response = send(
        &CurlClient::new(),
        &request(HttpMethod::Get, server.url("/image")),
    )
    .expect("binary response should succeed");

    // The bytes themselves, not just the count: a download writes them, so
    // losing them here would only show up as a corrupt file.
    assert_eq!(
        response.body,
        ResponseBody::Binary {
            bytes: PNG_BYTES.to_vec()
        }
    );
}

#[test]
fn sends_the_requested_method_and_expects_no_body_for_head() {
    let server = MockServer::start();
    let deleted = server.mock(|when, then| {
        when.method(DELETE).path("/thing");
        then.status(204);
    });
    // No method matcher: httpmock's prelude does not export a HEAD constant,
    // and what matters here is that the response carries no body.
    let head = server.mock(|when, then| {
        when.path("/head-only");
        then.status(200).header("content-length", "9");
    });

    let client = CurlClient::new();
    let delete_response = send(&client, &request(HttpMethod::Delete, server.url("/thing")))
        .expect("DELETE should succeed");
    let head_response = send(
        &client,
        &request(HttpMethod::Head, server.url("/head-only")),
    )
    .expect("HEAD should succeed");

    deleted.assert();
    head.assert();
    assert_eq!(delete_response.status, 204);
    assert_eq!(head_response.status, 200);
    assert_eq!(head_response.body, ResponseBody::Text(String::new()));
}

#[test]
fn sends_headers_query_params_and_a_raw_body() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/submit")
            .query_param("q", "a b&c")
            .header("x-api-key", "secret")
            .header("content-type", "application/json")
            .body(r#"{"a":1}"#);
        then.status(200).body("ok");
    });

    let mut req = request(HttpMethod::Post, server.url("/submit"));
    req.headers = vec![KeyValue::new("x-api-key", "secret")];
    req.query_params = vec![KeyValue::new("q", "a b&c")];
    req.body = RequestBody::Raw {
        content_type: "application/json".into(),
        text: r#"{"a":1}"#.into(),
    };

    let response = send(&CurlClient::new(), &req).expect("request should succeed");

    mock.assert();
    assert_eq!(response.status, 200);
}

/// Since the Params tab writes into the URL, a value typed with a space ends
/// up in the URL as a space. libcurl refuses that outright, so the default
/// "encode URL automatically" setting has to fix it on the way out.
#[test]
fn a_url_typed_with_spaces_and_unicode_reaches_the_server() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/search")
            .query_param("q", "a b")
            .query_param("city", "Köln")
            .query_param("n", "1+1");
        then.status(200).body("ok");
    });

    let url = format!("{}?q=a b&city=Köln&n=1%2B1", server.url("/search"));
    let response =
        send(&CurlClient::new(), &request(HttpMethod::Get, url)).expect("request should succeed");

    mock.assert();
    assert_eq!(response.status, 200);
}

/// Off means off: the URL goes out as typed, and libcurl's own verdict on it
/// is what the user sees.
#[test]
fn with_encoding_off_a_url_with_a_space_is_refused_by_libcurl() {
    let server = MockServer::start();
    let mut req = request(HttpMethod::Get, format!("{}?q=a b", server.url("/search")));
    req.settings.encode_url = false;

    let error = send(&CurlClient::new(), &req).expect_err("libcurl should refuse it");

    assert!(matches!(error, AppError::Transport(_)), "{error:?}");
}

/// Replaces the two byte-level unit tests the hand-rolled encoder used to
/// have. libcurl builds the body now, so asserting our own bytes would be
/// asserting nothing — what matters is that a server receives the file.
///
/// Matched with `body_contains` rather than by reading the request back,
/// so the mock itself fails the test when the body is wrong.
#[test]
fn sends_a_multipart_body_with_a_file_part_read_from_disk() {
    let directory =
        std::env::temp_dir().join(format!("responderhttp-upload-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir should be creatable");
    let file = directory.join("notes.txt");
    std::fs::write(&file, b"file-part-contents").expect("fixture should be writable");

    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/upload")
            // The file's bytes came off disk, the basename travelled as the
            // filename, and the text part is still there alongside it.
            .body_contains("file-part-contents")
            .body_contains("filename=\"notes.txt\"")
            .body_contains("name=\"caption\"")
            .body_contains("hello");
        then.status(200).body("ok");
    });

    let mut req = request(HttpMethod::Post, server.url("/upload"));
    req.body = RequestBody::Multipart(vec![
        MultipartPart::Text {
            name: "caption".into(),
            value: "hello".into(),
        },
        MultipartPart::File {
            name: "upload".into(),
            path: file.clone(),
            content_type: Some("text/plain".into()),
        },
    ]);

    let response = send(&CurlClient::new(), &req).expect("multipart upload should succeed");

    mock.assert();
    assert_eq!(response.status, 200);

    let _ = std::fs::remove_file(&file);
}

#[test]
fn sends_a_form_urlencoded_body_with_the_matching_content_type() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/form")
            .header("content-type", "application/x-www-form-urlencoded")
            .body("field=a%20value&other=2");
        then.status(204);
    });

    let mut req = request(HttpMethod::Post, server.url("/form"));
    req.body = RequestBody::FormUrlEncoded(vec![
        KeyValue::new("field", "a value"),
        KeyValue::new("other", "2"),
    ]);

    let response = send(&CurlClient::new(), &req).expect("request should succeed");

    mock.assert();
    assert_eq!(response.status, 204);
}

#[test]
fn keeps_the_method_when_a_body_is_attached() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT).path("/resource").body("payload");
        then.status(200);
    });

    let mut req = request(HttpMethod::Put, server.url("/resource"));
    req.body = RequestBody::Raw {
        content_type: "text/plain".into(),
        text: "payload".into(),
    };

    send(&CurlClient::new(), &req).expect("PUT with a body should succeed");

    mock.assert();
}

#[test]
fn does_not_follow_redirects_when_the_setting_is_off() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/start");
        then.status(302).header("location", "/end");
    });

    let mut req = request(HttpMethod::Get, server.url("/start"));
    req.settings.follow_redirects = false;

    let response = send(&CurlClient::new(), &req).expect("request should succeed");

    assert_eq!(response.status, 302);
}

#[test]
fn a_cancelled_transfer_stops_and_reports_a_failure() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/slow");
        then.status(200).delay(Duration::from_secs(5)).body("late");
    });

    let cancel = CancellationToken::new();
    let cancel_handle = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        cancel_handle.cancel();
    });

    let started = std::time::Instant::now();
    let error = CurlClient::new()
        .send(&request(HttpMethod::Get, server.url("/slow")), &cancel)
        .expect_err("a cancelled transfer fails");

    assert!(matches!(error, AppError::Transport(_)), "got {error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "cancellation should stop the transfer early, took {:?}",
        started.elapsed()
    );
}

// http_client/src-tauri/src/domain/services/send_request.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::domain::cancellation::CancellationToken;
use crate::domain::error::AppError;
use crate::domain::models::{HttpRequest, HttpResponse, MultipartPart, RequestBody};
use crate::domain::ports::HttpClient;

/// Identifies one in-flight request so it can be cancelled while running.
/// Minted by the caller (the UI), because the UI needs the id before the
/// response comes back.
pub type RequestId = String;

/// Use-case: validate a request, register it as cancellable, hand it to
/// whichever HttpClient was injected. Cloning is cheap (Arc), so the command
/// layer can move a clone into a blocking task.
#[derive(Clone)]
pub struct SendRequest {
    client: Arc<dyn HttpClient>,
    in_flight: Arc<Mutex<HashMap<RequestId, CancellationToken>>>,
}

impl SendRequest {
    pub fn new(client: Arc<dyn HttpClient>) -> Self {
        Self {
            client,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn execute(&self, id: RequestId, request: HttpRequest) -> Result<HttpResponse, AppError> {
        validate_url(&request.url)?;
        validate_proxy(request.settings.proxy.as_deref())?;
        validate_multipart(&request.body)?;

        let token = CancellationToken::new();
        self.register(id.clone(), token.clone());

        let result = self.client.send(&request, &token);
        self.forget(&id);

        // libcurl reports an aborted transfer as a generic failure; the token
        // is what actually distinguishes "the user stopped it" from "it broke".
        match result {
            Err(AppError::Transport(_)) if token.is_cancelled() => Err(AppError::Cancelled),
            other => other,
        }
    }

    /// Unknown ids are ignored on purpose: a cancel that arrives just after
    /// the response is a race the user should never see as an error.
    pub fn cancel(&self, id: &str) {
        if let Some(token) = self.lock_in_flight().get(id) {
            token.cancel();
        }
    }

    fn register(&self, id: RequestId, token: CancellationToken) {
        self.lock_in_flight().insert(id, token);
    }

    fn forget(&self, id: &str) {
        self.lock_in_flight().remove(id);
    }

    /// A poisoned lock means another thread panicked mid-update. The map is
    /// just a registry of tokens, so recovering it is safe and beats killing
    /// every later request.
    fn lock_in_flight(&self) -> std::sync::MutexGuard<'_, HashMap<RequestId, CancellationToken>> {
        self.in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A file part carries a path, so a saved request can outlive the file it
/// points at — renamed, moved, on a machine the collection was copied from.
/// libcurl would report that as a generic transport failure with no mention
/// of which file, so it is caught here while the path is still in hand.
fn validate_multipart(body: &RequestBody) -> Result<(), AppError> {
    let RequestBody::Multipart(parts) = body else {
        return Ok(());
    };
    for part in parts {
        let MultipartPart::File { name, path, .. } = part else {
            continue;
        };
        // A blank name is the table's trailing row; it never reaches the wire
        // (build_form skips it), so it must not fail the send either.
        if name.trim().is_empty() {
            continue;
        }
        if !path.is_file() {
            return Err(AppError::InvalidRequest(format!(
                "file for part \"{}\" is missing: {}",
                name.trim(),
                path.display()
            )));
        }
    }
    Ok(())
}

const SUPPORTED_SCHEMES: [&str; 2] = ["http://", "https://"];

/// Kept separate from `execute` so it can be tested without a client, and
/// so the same rule can back an inline URL-bar warning later.
fn validate_url(url: &str) -> Result<(), AppError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidRequest("URL is empty".into()));
    }
    if !SUPPORTED_SCHEMES
        .iter()
        .any(|scheme| trimmed.starts_with(scheme))
    {
        return Err(AppError::InvalidRequest(
            "URL must start with http:// or https://".into(),
        ));
    }
    Ok(())
}

/// A proxy without a scheme is ambiguous to libcurl and silently behaves as
/// HTTP, which is a surprising way to leak traffic.
fn validate_proxy(proxy: Option<&str>) -> Result<(), AppError> {
    let Some(proxy) = proxy.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    const PROXY_SCHEMES: [&str; 4] = ["http://", "https://", "socks5://", "socks5h://"];
    if PROXY_SCHEMES.iter().any(|scheme| proxy.starts_with(scheme)) {
        return Ok(());
    }
    Err(AppError::InvalidRequest(
        "Proxy must start with http://, https://, socks5:// or socks5h://".into(),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;
    use crate::domain::models::{
        Auth, HttpMethod, KeyValue, RequestBody, RequestSettings, ResponseBody, Timing,
    };

    #[derive(Default)]
    struct MockHttpClient {
        seen: StdMutex<Vec<HttpRequest>>,
        error: Option<String>,
        cancel_while_sending: bool,
    }

    impl HttpClient for MockHttpClient {
        fn send(
            &self,
            request: &HttpRequest,
            cancel: &CancellationToken,
        ) -> Result<HttpResponse, AppError> {
            self.seen
                .lock()
                .expect("mock lock poisoned")
                .push(request.clone());
            if self.cancel_while_sending {
                // Stands in for the user hitting Cancel mid-transfer.
                cancel.cancel();
            }
            match &self.error {
                Some(message) => Err(AppError::Transport(message.clone())),
                None => Ok(ok_response()),
            }
        }
    }

    fn ok_response() -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![KeyValue::new("content-type", "text/plain")],
            body: ResponseBody::Text("hello".into()),
            timing: Timing::default(),
            set_cookies: Vec::new(),
        }
    }

    fn request(url: &str) -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Get,
            url: url.into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: RequestBody::None,
            auth: Auth::None,
            settings: RequestSettings::default(),
        }
    }

    #[test]
    fn passes_a_valid_request_to_the_client_and_returns_its_response() {
        let client = Arc::new(MockHttpClient::default());
        let service = SendRequest::new(client.clone());

        let response = service
            .execute("r1".into(), request("https://example.com/"))
            .expect("valid request should succeed");

        assert_eq!(response.status, 200);
        let seen = client.seen.lock().expect("mock lock poisoned");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].url, "https://example.com/");
    }

    #[test]
    fn rejects_an_empty_url_without_calling_the_client() {
        let client = Arc::new(MockHttpClient::default());
        let service = SendRequest::new(client.clone());

        let error = service
            .execute("r1".into(), request("   "))
            .expect_err("should reject");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(client.seen.lock().expect("mock lock poisoned").is_empty());
    }

    #[test]
    fn rejects_an_unsupported_scheme_without_calling_the_client() {
        let client = Arc::new(MockHttpClient::default());
        let service = SendRequest::new(client.clone());

        let error = service
            .execute("r1".into(), request("ftp://example.com/file"))
            .expect_err("should reject");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(client.seen.lock().expect("mock lock poisoned").is_empty());
    }

    #[test]
    fn rejects_a_multipart_file_that_is_no_longer_there_naming_the_path() {
        let client = Arc::new(MockHttpClient::default());
        let service = SendRequest::new(client.clone());
        let mut sending = request("https://example.com/upload");
        sending.body = RequestBody::Multipart(vec![MultipartPart::File {
            name: "upload".into(),
            path: std::path::PathBuf::from("/definitely/not/here.png"),
            content_type: None,
        }]);

        let error = service
            .execute("r1".into(), sending)
            .expect_err("should reject");

        match error {
            // The path has to be in the message: "transport failed" on a
            // request saved months ago tells the user nothing.
            AppError::InvalidRequest(message) => {
                assert!(message.contains("/definitely/not/here.png"), "{message}");
                assert!(message.contains("upload"), "{message}");
            }
            other => panic!("expected InvalidRequest, got {other:?}"),
        }
        assert!(client.seen.lock().expect("mock lock poisoned").is_empty());
    }

    /// The table always keeps a spare row. It is skipped when the form is
    /// built, so it must not fail the send either.
    #[test]
    fn ignores_a_blank_named_file_part_the_way_the_form_builder_does() {
        let client = Arc::new(MockHttpClient::default());
        let service = SendRequest::new(client);
        let mut sending = request("https://example.com/upload");
        sending.body = RequestBody::Multipart(vec![MultipartPart::File {
            name: "   ".into(),
            path: std::path::PathBuf::from("/definitely/not/here.png"),
            content_type: None,
        }]);

        service
            .execute("r1".into(), sending)
            .expect("a spare row should not stop the send");
    }

    #[test]
    fn rejects_a_proxy_without_a_scheme() {
        let client = Arc::new(MockHttpClient::default());
        let service = SendRequest::new(client.clone());
        let mut req = request("https://example.com/");
        req.settings.proxy = Some("127.0.0.1:8080".into());

        let error = service
            .execute("r1".into(), req)
            .expect_err("should reject");

        assert!(matches!(error, AppError::InvalidRequest(_)));
        assert!(client.seen.lock().expect("mock lock poisoned").is_empty());
    }

    #[test]
    fn surfaces_a_transport_failure_rather_than_swallowing_it() {
        let client = Arc::new(MockHttpClient {
            error: Some("connection reset".into()),
            ..MockHttpClient::default()
        });
        let service = SendRequest::new(client);

        let error = service
            .execute("r1".into(), request("https://example.com/"))
            .expect_err("transport failure should surface");

        assert!(matches!(error, AppError::Transport(message) if message == "connection reset"));
    }

    #[test]
    fn reports_a_cancelled_transfer_as_cancelled_not_as_a_failure() {
        let client = Arc::new(MockHttpClient {
            error: Some("aborted by callback".into()),
            cancel_while_sending: true,
            ..MockHttpClient::default()
        });
        let service = SendRequest::new(client);

        let error = service
            .execute("r1".into(), request("https://example.com/"))
            .expect_err("a cancelled transfer fails");

        assert!(matches!(error, AppError::Cancelled), "got {error:?}");
    }

    #[test]
    fn cancelling_an_unknown_id_is_a_no_op() {
        let service = SendRequest::new(Arc::new(MockHttpClient::default()));

        service.cancel("never-started");
    }

    #[test]
    fn forgets_a_request_once_it_completes() {
        let service = SendRequest::new(Arc::new(MockHttpClient::default()));

        service
            .execute("r1".into(), request("https://example.com/"))
            .expect("should succeed");

        assert!(service.lock_in_flight().is_empty());
    }
}

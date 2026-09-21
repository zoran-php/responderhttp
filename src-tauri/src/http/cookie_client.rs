// http_client/src-tauri/src/http/cookie_client.rs
//
// An HttpClient that adds a cookie jar around another one. A decorator
// rather than a change to CurlClient or SendRequest: curl_client.rs executes
// requests and persists nothing (CLAUDE.md section 7), and SendRequest
// validates and cancels. Cookies are a third job, so they get a third object,
// and the Liskov rule in section 7 means nothing upstream notices.
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::cancellation::CancellationToken;
use crate::domain::cookies::{
    cookie_header, cookies_for_request, parse_set_cookie, request_target,
};
use crate::domain::error::AppError;
use crate::domain::models::{HttpRequest, HttpResponse, KeyValue};
use crate::domain::ports::{CookieRepository, HttpClient};

const COOKIE_HEADER: &str = "Cookie";

pub struct CookieClient {
    inner: Arc<dyn HttpClient>,
    cookies: Arc<dyn CookieRepository>,
}

impl CookieClient {
    pub fn new(inner: Arc<dyn HttpClient>, cookies: Arc<dyn CookieRepository>) -> Self {
        Self { inner, cookies }
    }
}

impl HttpClient for CookieClient {
    fn send(
        &self,
        request: &HttpRequest,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse, AppError> {
        let Some(target) = request_target(&request.url) else {
            // Not a URL cookies apply to; the client below will report it.
            return self.inner.send(request, cancel);
        };
        let now = unix_now();

        let prepared = match self.cookie_header_for(request, &target, now)? {
            Some(header) => {
                let mut with_cookies = request.clone();
                with_cookies
                    .headers
                    .push(KeyValue::new(COOKIE_HEADER, header));
                with_cookies
            }
            None => request.clone(),
        };

        let response = self.inner.send(&prepared, cancel)?;
        self.store_from(&response, &target, now);
        Ok(response)
    }
}

impl CookieClient {
    fn cookie_header_for(
        &self,
        request: &HttpRequest,
        target: &crate::domain::cookies::RequestTarget,
        now: u64,
    ) -> Result<Option<String>, AppError> {
        if !request.settings.send_cookies {
            return Ok(None);
        }
        // Same precedence as Content-Type and the Auth tab: a header the user
        // typed wins over one the app would generate.
        let user_set = request
            .headers
            .iter()
            .any(|header| header.name.trim().eq_ignore_ascii_case(COOKIE_HEADER));
        if user_set {
            return Ok(None);
        }
        let jar = self.cookies.list()?;
        Ok(cookie_header(&cookies_for_request(&jar, target, now)))
    }

    fn store_from(
        &self,
        response: &HttpResponse,
        target: &crate::domain::cookies::RequestTarget,
        now: u64,
    ) {
        for line in &response.set_cookies {
            let Some(cookie) = parse_set_cookie(line, target, now) else {
                continue;
            };
            if let Err(error) = self.cookies.upsert(&cookie) {
                // The response already arrived, so failing the whole request
                // over a jar write would be worse than losing the cookie. The
                // message carries no cookie data (CLAUDE.md section 11, rule 6).
                log::warn!("failed to store a cookie: {error}");
            }
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{HttpMethod, RequestBody, RequestSettings, ResponseBody, Timing};
    use std::sync::Mutex;

    #[derive(Default)]
    struct SpyClient {
        seen: Mutex<Vec<HttpRequest>>,
        set_cookies: Vec<String>,
    }

    impl HttpClient for SpyClient {
        fn send(
            &self,
            request: &HttpRequest,
            _cancel: &CancellationToken,
        ) -> Result<HttpResponse, AppError> {
            self.seen
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(request.clone());
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: ResponseBody::Text(String::new()),
                timing: Timing::default(),
                set_cookies: self.set_cookies.clone(),
            })
        }
    }

    #[derive(Default)]
    struct SpyJar {
        stored: Mutex<Vec<crate::domain::models::Cookie>>,
        existing: Vec<crate::domain::models::Cookie>,
    }

    impl CookieRepository for SpyJar {
        fn list(&self) -> Result<Vec<crate::domain::models::Cookie>, AppError> {
            Ok(self.existing.clone())
        }
        fn upsert(&self, cookie: &crate::domain::models::Cookie) -> Result<(), AppError> {
            self.stored
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(cookie.clone());
            Ok(())
        }
        fn delete(&self, _domain: &str, _path: &str, _name: &str) -> Result<(), AppError> {
            Ok(())
        }
        fn clear(&self) -> Result<(), AppError> {
            Ok(())
        }
        fn clear_session(&self) -> Result<(), AppError> {
            Ok(())
        }
        fn purge_expired(&self, _now: u64) -> Result<(), AppError> {
            Ok(())
        }
    }

    fn request(url: &str) -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Get,
            url: url.into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: RequestBody::None,
            auth: crate::domain::models::Auth::None,
            settings: RequestSettings::default(),
        }
    }

    fn stored_cookie(name: &str, domain: &str) -> crate::domain::models::Cookie {
        crate::domain::models::Cookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            expires_at: None,
            secure: false,
            http_only: false,
            host_only: false,
            created_at: 1,
        }
    }

    fn header_of(request: &HttpRequest, name: &str) -> Option<String> {
        request
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.clone())
    }

    #[test]
    fn attaches_matching_cookies_to_the_outgoing_request() {
        let inner = Arc::new(SpyClient::default());
        let jar = Arc::new(SpyJar {
            existing: vec![stored_cookie("sid", "example.com")],
            ..SpyJar::default()
        });
        let client = CookieClient::new(inner.clone(), jar);

        client
            .send(&request("https://example.com/x"), &CancellationToken::new())
            .expect("should send");

        let seen = inner.seen.lock().expect("lock");
        assert_eq!(header_of(&seen[0], "Cookie").as_deref(), Some("sid=v"));
    }

    #[test]
    fn sends_nothing_for_a_host_the_jar_does_not_cover() {
        let inner = Arc::new(SpyClient::default());
        let jar = Arc::new(SpyJar {
            existing: vec![stored_cookie("sid", "other.com")],
            ..SpyJar::default()
        });
        let client = CookieClient::new(inner.clone(), jar);

        client
            .send(&request("https://example.com/x"), &CancellationToken::new())
            .expect("should send");

        let seen = inner.seen.lock().expect("lock");
        assert_eq!(header_of(&seen[0], "Cookie"), None);
    }

    #[test]
    fn a_cookie_header_the_user_typed_wins() {
        let inner = Arc::new(SpyClient::default());
        let jar = Arc::new(SpyJar {
            existing: vec![stored_cookie("sid", "example.com")],
            ..SpyJar::default()
        });
        let client = CookieClient::new(inner.clone(), jar);

        let mut manual = request("https://example.com/x");
        manual.headers.push(KeyValue::new("Cookie", "mine=1"));
        client
            .send(&manual, &CancellationToken::new())
            .expect("should send");

        let seen = inner.seen.lock().expect("lock");
        assert_eq!(header_of(&seen[0], "Cookie").as_deref(), Some("mine=1"));
        assert_eq!(seen[0].headers.len(), 1);
    }

    #[test]
    fn send_cookies_off_skips_the_jar_entirely() {
        let inner = Arc::new(SpyClient::default());
        let jar = Arc::new(SpyJar {
            existing: vec![stored_cookie("sid", "example.com")],
            ..SpyJar::default()
        });
        let client = CookieClient::new(inner.clone(), jar);

        let mut off = request("https://example.com/x");
        off.settings.send_cookies = false;
        client
            .send(&off, &CancellationToken::new())
            .expect("should send");

        let seen = inner.seen.lock().expect("lock");
        assert_eq!(header_of(&seen[0], "Cookie"), None);
    }

    #[test]
    fn stores_a_cookie_the_response_set() {
        let inner = Arc::new(SpyClient {
            set_cookies: vec!["sid=abc; Path=/".into()],
            ..SpyClient::default()
        });
        let jar = Arc::new(SpyJar::default());
        let client = CookieClient::new(inner, jar.clone());

        client
            .send(&request("https://example.com/x"), &CancellationToken::new())
            .expect("should send");

        let stored = jar.stored.lock().expect("lock");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "sid");
        assert_eq!(stored[0].domain, "example.com");
    }

    #[test]
    fn ignores_a_set_cookie_for_a_domain_the_host_cannot_set() {
        let inner = Arc::new(SpyClient {
            set_cookies: vec!["sid=abc; Domain=evil.com".into()],
            ..SpyClient::default()
        });
        let jar = Arc::new(SpyJar::default());
        let client = CookieClient::new(inner, jar.clone());

        client
            .send(&request("https://example.com/x"), &CancellationToken::new())
            .expect("should send");

        assert!(jar.stored.lock().expect("lock").is_empty());
    }
}

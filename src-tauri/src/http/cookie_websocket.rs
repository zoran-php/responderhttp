// http_client/src-tauri/src/http/cookie_websocket.rs
//
// The cookie jar for WebSocket handshakes: a decorator over another
// WebSocketConnector, the counterpart of CookieClient for HTTP. The
// handshake is an HTTP request, and a browser sends the host's cookies with
// it, so the jar applies to it exactly as it does to a request — same rule
// for when (`jar_cookie_header`), same rule for what a response may set
// (`store_set_cookies`). Nothing upstream knows cookies exist (CLAUDE.md
// section 7, Liskov).
use std::borrow::Cow;
use std::sync::Arc;

use crate::domain::cancellation::CancellationToken;
use crate::domain::cookies::request_target;
use crate::domain::models::{KeyValue, WebSocketRequest};
use crate::domain::ports::{
    CookieRepository, WebSocketConnection, WebSocketConnector, WsConnectError,
};
use crate::http::cookie_client::{jar_cookie_header, store_set_cookies, unix_now, COOKIE_HEADER};

pub struct CookieWebSocketConnector {
    inner: Arc<dyn WebSocketConnector>,
    cookies: Arc<dyn CookieRepository>,
}

impl CookieWebSocketConnector {
    pub fn new(inner: Arc<dyn WebSocketConnector>, cookies: Arc<dyn CookieRepository>) -> Self {
        Self { inner, cookies }
    }
}

impl WebSocketConnector for CookieWebSocketConnector {
    fn connect(
        &self,
        request: &WebSocketRequest,
        cancel: &CancellationToken,
    ) -> Result<Box<dyn WebSocketConnection>, WsConnectError> {
        let Some(target) = request_target(&cookie_url(&request.url)) else {
            // Not a URL cookies apply to; the connector below will report it.
            return self.inner.connect(request, cancel);
        };
        let now = unix_now();

        let header = jar_cookie_header(
            self.cookies.as_ref(),
            &request.headers,
            request.settings.send_cookies,
            &target,
            now,
        )
        .map_err(|error| WsConnectError::Failed(error.to_string()))?;
        let prepared = match header {
            Some(value) => {
                let mut with_cookies = request.clone();
                with_cookies
                    .headers
                    .push(KeyValue::new(COOKIE_HEADER, value));
                Cow::Owned(with_cookies)
            }
            None => Cow::Borrowed(request),
        };

        let connection = self.inner.connect(&prepared, cancel)?;
        store_set_cookies(
            self.cookies.as_ref(),
            &connection.handshake().set_cookies,
            &target,
            now,
        );
        Ok(connection)
    }
}

/// Cookies belong to the HTTP origin a WebSocket upgrades from: `ws://` is
/// matched as `http://` and `wss://` as `https://`, so a Secure cookie goes
/// over `wss://` and never over `ws://` — what browsers do. Anything else is
/// passed through for `request_target` to refuse.
fn cookie_url(url: &str) -> Cow<'_, str> {
    let trimmed = url.trim();
    for (from, to) in [("wss://", "https://"), ("ws://", "http://")] {
        if trimmed
            .get(..from.len())
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case(from))
        {
            return Cow::Owned(format!("{to}{}", &trimmed[from.len()..]));
        }
    }
    Cow::Borrowed(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::AppError;
    use crate::domain::models::{Cookie, WebSocketSettings, WsHandshake};
    use crate::domain::ports::WsPoll;
    use crate::domain::ws_frames::FrameKind;
    use std::sync::Mutex;
    use std::time::Duration;

    struct Opened {
        handshake: WsHandshake,
    }

    impl WebSocketConnection for Opened {
        fn handshake(&self) -> &WsHandshake {
            &self.handshake
        }
        fn poll(&mut self, _timeout: Duration) -> Result<WsPoll, AppError> {
            Ok(WsPoll::Idle)
        }
        fn send(&mut self, _kind: FrameKind, _payload: &[u8]) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct SpyConnector {
        seen: Mutex<Vec<WebSocketRequest>>,
        set_cookies: Vec<String>,
    }

    impl WebSocketConnector for SpyConnector {
        fn connect(
            &self,
            request: &WebSocketRequest,
            _cancel: &CancellationToken,
        ) -> Result<Box<dyn WebSocketConnection>, WsConnectError> {
            self.seen
                .lock()
                .expect("spy lock poisoned")
                .push(request.clone());
            Ok(Box::new(Opened {
                handshake: WsHandshake {
                    status: 101,
                    headers: Vec::new(),
                    set_cookies: self.set_cookies.clone(),
                },
            }))
        }
    }

    #[derive(Default)]
    struct SpyJar {
        existing: Vec<Cookie>,
        stored: Mutex<Vec<Cookie>>,
    }

    impl CookieRepository for SpyJar {
        fn list(&self) -> Result<Vec<Cookie>, AppError> {
            Ok(self.existing.clone())
        }
        fn upsert(&self, cookie: &Cookie) -> Result<(), AppError> {
            self.stored
                .lock()
                .expect("spy lock poisoned")
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

    fn cookie(name: &str, domain: &str, secure: bool) -> Cookie {
        Cookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            expires_at: None,
            secure,
            http_only: false,
            host_only: false,
            created_at: 1,
        }
    }

    fn request(url: &str) -> WebSocketRequest {
        WebSocketRequest {
            url: url.into(),
            headers: Vec::new(),
            settings: WebSocketSettings::default(),
        }
    }

    fn cookie_header(connector: &SpyConnector) -> Option<String> {
        connector.seen.lock().expect("spy lock poisoned")[0]
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("Cookie"))
            .map(|header| header.value.clone())
    }

    fn connect(jar: SpyJar, request: &WebSocketRequest) -> (Arc<SpyConnector>, Arc<SpyJar>) {
        let inner = Arc::new(SpyConnector::default());
        let jar = Arc::new(jar);
        CookieWebSocketConnector::new(inner.clone(), jar.clone())
            .connect(request, &CancellationToken::new())
            .map(|_| ())
            .expect("should connect");
        (inner, jar)
    }

    #[test]
    fn the_handshake_carries_the_hosts_cookies() {
        let jar = SpyJar {
            existing: vec![cookie("sid", "example.com", false)],
            ..SpyJar::default()
        };

        let (inner, _) = connect(jar, &request("wss://example.com/socket"));

        assert_eq!(cookie_header(&inner).as_deref(), Some("sid=v"));
    }

    /// Browsers' rule, and the reason for mapping the scheme at all.
    #[test]
    fn a_secure_cookie_goes_over_wss_but_never_over_ws() {
        let secure = || SpyJar {
            existing: vec![cookie("sid", "example.com", true)],
            ..SpyJar::default()
        };

        let (over_wss, _) = connect(secure(), &request("wss://example.com/"));
        let (over_ws, _) = connect(secure(), &request("ws://example.com/"));

        assert_eq!(cookie_header(&over_wss).as_deref(), Some("sid=v"));
        assert_eq!(cookie_header(&over_ws), None);
    }

    #[test]
    fn a_cookie_header_the_user_typed_wins() {
        let jar = SpyJar {
            existing: vec![cookie("sid", "example.com", false)],
            ..SpyJar::default()
        };
        let mut manual = request("ws://example.com/");
        manual.headers.push(KeyValue::new("Cookie", "mine=1"));

        let (inner, _) = connect(jar, &manual);

        assert_eq!(cookie_header(&inner).as_deref(), Some("mine=1"));
        assert_eq!(inner.seen.lock().expect("lock")[0].headers.len(), 1);
    }

    #[test]
    fn send_cookies_off_leaves_the_jar_out() {
        let jar = SpyJar {
            existing: vec![cookie("sid", "example.com", false)],
            ..SpyJar::default()
        };
        let mut request = request("ws://example.com/");
        request.settings.send_cookies = false;

        let (inner, _) = connect(jar, &request);

        assert_eq!(cookie_header(&inner), None);
    }

    #[test]
    fn a_cookie_set_by_the_handshake_is_stored() {
        let inner = Arc::new(SpyConnector {
            set_cookies: vec!["session=abc; Path=/".into()],
            ..SpyConnector::default()
        });
        let jar = Arc::new(SpyJar::default());

        CookieWebSocketConnector::new(inner, jar.clone())
            .connect(
                &request("wss://example.com/live"),
                &CancellationToken::new(),
            )
            .map(|_| ())
            .expect("should connect");

        let stored = jar.stored.lock().expect("spy lock poisoned");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "session");
        assert_eq!(stored[0].domain, "example.com");
    }

    #[test]
    fn websocket_schemes_map_to_their_http_origin() {
        assert_eq!(cookie_url("ws://a.test/x"), "http://a.test/x");
        assert_eq!(cookie_url("  WSS://a.test"), "https://a.test");
        assert_eq!(cookie_url("ftp://a.test"), "ftp://a.test");
    }
}

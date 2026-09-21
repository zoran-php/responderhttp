// http_client/src-tauri/src/http/auth.rs
//
// `AuthStrategy` plus one implementation per `Auth` variant. Adding a scheme
// is a new struct here and a new arm in `strategy_for` — never an edit to
// curl_client.rs (CLAUDE.md section 4, and section 7 open/closed).
//
// A strategy returns the headers and query parameters it contributes instead
// of mutating a curl handle, so every one of them is unit-testable with no
// transport in the way (CLAUDE.md section 8).
use crate::domain::models::{ApiKeyLocation, Auth, KeyValue};

const AUTHORIZATION: &str = "Authorization";

/// What a scheme adds to the outgoing request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthParts {
    pub headers: Vec<KeyValue>,
    pub query_params: Vec<KeyValue>,
}

impl AuthParts {
    fn header(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            headers: vec![KeyValue::new(name, value)],
            query_params: Vec::new(),
        }
    }

    fn query(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            headers: Vec::new(),
            query_params: vec![KeyValue::new(name, value)],
        }
    }
}

pub trait AuthStrategy {
    fn apply(&self) -> AuthParts;
}

pub struct NoAuth;

impl AuthStrategy for NoAuth {
    fn apply(&self) -> AuthParts {
        AuthParts::default()
    }
}

pub struct BasicAuth<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

impl AuthStrategy for BasicAuth<'_> {
    fn apply(&self) -> AuthParts {
        // Two empty fields are an untouched form, not a request to send
        // `Basic Og==` (the encoding of a lone colon).
        if self.username.is_empty() && self.password.is_empty() {
            return AuthParts::default();
        }
        let encoded = base64_encode(format!("{}:{}", self.username, self.password).as_bytes());
        AuthParts::header(AUTHORIZATION, format!("Basic {encoded}"))
    }
}

pub struct BearerAuth<'a> {
    pub token: &'a str,
}

impl AuthStrategy for BearerAuth<'_> {
    fn apply(&self) -> AuthParts {
        let token = self.token.trim();
        if token.is_empty() {
            return AuthParts::default();
        }
        AuthParts::header(AUTHORIZATION, format!("Bearer {token}"))
    }
}

pub struct ApiKeyAuth<'a> {
    pub key: &'a str,
    pub value: &'a str,
    pub location: ApiKeyLocation,
}

impl AuthStrategy for ApiKeyAuth<'_> {
    fn apply(&self) -> AuthParts {
        let key = self.key.trim();
        if key.is_empty() {
            return AuthParts::default();
        }
        match self.location {
            ApiKeyLocation::Header => AuthParts::header(key, self.value),
            ApiKeyLocation::Query => AuthParts::query(key, self.value),
        }
    }
}

pub struct CustomAuth<'a> {
    pub header_name: &'a str,
    pub header_value: &'a str,
}

impl AuthStrategy for CustomAuth<'_> {
    fn apply(&self) -> AuthParts {
        let name = self.header_name.trim();
        if name.is_empty() {
            return AuthParts::default();
        }
        AuthParts::header(name, self.header_value)
    }
}

/// The single place a variant maps to a strategy. Everything downstream
/// depends on the trait, not on the enum.
pub fn strategy_for(auth: &Auth) -> Box<dyn AuthStrategy + '_> {
    match auth {
        Auth::None => Box::new(NoAuth),
        Auth::Basic { username, password } => Box::new(BasicAuth { username, password }),
        Auth::Bearer { token } => Box::new(BearerAuth { token }),
        Auth::ApiKey {
            key,
            value,
            location,
        } => Box::new(ApiKeyAuth {
            key,
            value,
            location: *location,
        }),
        Auth::Custom {
            header_name,
            header_value,
        } => Box::new(CustomAuth {
            header_name,
            header_value,
        }),
    }
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Basic auth is the only thing in the codebase that needs base64, and in a
/// single-binary build every dependency has to earn its place (CLAUDE.md
/// section 4). Twenty lines of std is the cheaper trade.
fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len() * 4 / 3 + 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk.first().copied().unwrap_or(0));
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[((triple >> 18) & 63) as usize] as char);
        out.push(BASE64_ALPHABET[((triple >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(auth: &Auth) -> AuthParts {
        strategy_for(auth).apply()
    }

    fn header(name: &str, value: &str) -> Vec<KeyValue> {
        vec![KeyValue::new(name, value)]
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_handles_bytes_above_ascii() {
        assert_eq!(base64_encode(&[0xff, 0xee, 0xdd]), "/+7d");
    }

    #[test]
    fn none_contributes_nothing() {
        assert_eq!(parts(&Auth::None), AuthParts::default());
    }

    #[test]
    fn basic_sends_the_encoded_credentials() {
        let auth = Auth::Basic {
            username: "user".into(),
            password: "pass".into(),
        };
        assert_eq!(
            parts(&auth).headers,
            header(AUTHORIZATION, "Basic dXNlcjpwYXNz")
        );
    }

    #[test]
    fn basic_keeps_a_password_that_contains_a_colon() {
        let auth = Auth::Basic {
            username: "user".into(),
            password: "pa:ss".into(),
        };
        // Only the first colon separates the pair, so the password survives.
        assert_eq!(
            parts(&auth).headers,
            header(AUTHORIZATION, "Basic dXNlcjpwYTpzcw==")
        );
    }

    #[test]
    fn basic_with_both_fields_empty_sends_no_header() {
        let auth = Auth::Basic {
            username: String::new(),
            password: String::new(),
        };
        assert_eq!(parts(&auth), AuthParts::default());
    }

    #[test]
    fn bearer_prefixes_the_token() {
        let auth = Auth::Bearer {
            token: "  abc123  ".into(),
        };
        assert_eq!(parts(&auth).headers, header(AUTHORIZATION, "Bearer abc123"));
    }

    #[test]
    fn bearer_with_a_blank_token_sends_no_header() {
        let auth = Auth::Bearer {
            token: "   ".into(),
        };
        assert_eq!(parts(&auth), AuthParts::default());
    }

    #[test]
    fn api_key_can_travel_as_a_header() {
        let auth = Auth::ApiKey {
            key: "X-Api-Key".into(),
            value: "secret".into(),
            location: ApiKeyLocation::Header,
        };
        let applied = parts(&auth);
        assert_eq!(applied.headers, header("X-Api-Key", "secret"));
        assert!(applied.query_params.is_empty());
    }

    #[test]
    fn api_key_can_travel_as_a_query_parameter() {
        let auth = Auth::ApiKey {
            key: "api_key".into(),
            value: "secret".into(),
            location: ApiKeyLocation::Query,
        };
        let applied = parts(&auth);
        assert_eq!(
            applied.query_params,
            vec![KeyValue::new("api_key", "secret")]
        );
        assert!(applied.headers.is_empty());
    }

    #[test]
    fn api_key_without_a_name_is_ignored() {
        let auth = Auth::ApiKey {
            key: "  ".into(),
            value: "secret".into(),
            location: ApiKeyLocation::Header,
        };
        assert_eq!(parts(&auth), AuthParts::default());
    }

    #[test]
    fn custom_uses_the_given_header_name() {
        let auth = Auth::Custom {
            header_name: "X-Signature".into(),
            header_value: "abc".into(),
        };
        assert_eq!(parts(&auth).headers, header("X-Signature", "abc"));
    }

    #[test]
    fn custom_without_a_name_is_ignored() {
        let auth = Auth::Custom {
            header_name: String::new(),
            header_value: "abc".into(),
        };
        assert_eq!(parts(&auth), AuthParts::default());
    }
}

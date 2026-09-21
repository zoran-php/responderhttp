// http_client/src-tauri/src/domain/secrets.rs
//
// Which values are secrets, and the rules that follow them wherever they are
// stored (PLAN.md Phase 9). Pure: no key, no cipher, no storage — the same
// shape as domain/cookies.rs, so the rules test without any infrastructure.
use crate::domain::models::{Auth, HttpRequest};

/// Every sealed value's associated data starts with this, and the version in
/// it moves with the envelope format. Changing it makes every stored secret
/// unreadable, so it only ever changes together with a format version.
const SCOPE_PREFIX: &str = "responderhttp/v1";

/// Query parameter names whose value is a credential by convention. One list
/// for every place that has to recognise one — the log redactor and the
/// OpenAPI exporter — so they cannot disagree about what is sensitive.
pub const CREDENTIAL_PARAM_NAMES: [&str; 12] = [
    "key",
    "api_key",
    "apikey",
    "api-key",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "password",
    "secret",
    "signature",
    "sig",
];

/// Case-insensitive, and on the whole name: `token` is a credential,
/// `token_type` is not.
pub fn is_credential_param_name(name: &str) -> bool {
    let name = name.trim();
    CREDENTIAL_PARAM_NAMES
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

/// What loading a secret produced, beyond its value. A secret that could not
/// be read loads as an empty string with one of the non-`Ok` states, so a
/// missing secret costs one field rather than the whole request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretState {
    #[default]
    Ok,
    /// Gone for good: sealed under a key this machine no longer has (the
    /// database was copied, or the key was deleted), or damaged. The user has
    /// to type it again.
    NeedsReentry,
    /// The credential store could not be reached this session. The stored
    /// value may well be fine; saving over it is refused until it can be read.
    Unavailable,
}

impl SecretState {
    /// Folding several fields into one flag for a row: the worst one wins,
    /// and Unavailable is worse than NeedsReentry because it also blocks
    /// saving.
    pub fn worst(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unavailable, _) | (_, Self::Unavailable) => Self::Unavailable,
            (Self::NeedsReentry, _) | (_, Self::NeedsReentry) => Self::NeedsReentry,
            _ => Self::Ok,
        }
    }
}

/// Where a sealed value lives, used as its associated data. A value copied
/// into another row or field then fails to open instead of quietly becoming
/// that row's secret. Built from ids, never names, so a rename moves nothing.
pub fn secret_scope(table: &str, row_id: &str, field: &str) -> String {
    format!("{SCOPE_PREFIX}/{table}/{row_id}/{field}")
}

/// True when the whole value is one `{{placeholder}}`. Such a value holds no
/// secret — it names one — so history and examples may keep it, and a re-run
/// resolves it against whichever environment is active then.
///
/// Mirrors the placeholder rule in src/lib/variables.ts: no braces inside,
/// surrounding whitespace ignored. An empty name is not a placeholder.
pub fn is_placeholder_only(value: &str) -> bool {
    let trimmed = value.trim();
    let Some(inner) = trimmed
        .strip_prefix("{{")
        .and_then(|rest| rest.strip_suffix("}}"))
    else {
        return false;
    };
    !inner.trim().is_empty() && !inner.contains('{') && !inner.contains('}')
}

/// The one secret an auth scheme carries, with the field name used in its
/// scope. Usernames, key names and header names are not secrets: they say
/// how to authenticate, not with what.
pub fn auth_secret(auth: &Auth) -> Option<(&'static str, &str)> {
    match auth {
        Auth::None => None,
        Auth::Basic { password, .. } => Some(("auth.password", password)),
        Auth::Bearer { token } => Some(("auth.token", token)),
        Auth::ApiKey { value, .. } => Some(("auth.value", value)),
        Auth::Custom { header_value, .. } => Some(("auth.header_value", header_value)),
    }
}

/// The same scheme with its secret replaced. A scheme without one is
/// returned unchanged.
pub fn with_auth_secret(auth: &Auth, secret: String) -> Auth {
    match auth {
        Auth::None => Auth::None,
        Auth::Basic { username, .. } => Auth::Basic {
            username: username.clone(),
            password: secret,
        },
        Auth::Bearer { .. } => Auth::Bearer { token: secret },
        Auth::ApiKey { key, location, .. } => Auth::ApiKey {
            key: key.clone(),
            value: secret,
            location: *location,
        },
        Auth::Custom { header_name, .. } => Auth::Custom {
            header_name: header_name.clone(),
            header_value: secret,
        },
    }
}

/// History and saved responses keep no secrets (decided 2026-09-16). A
/// literal value is blanked; a value that is only a `{{placeholder}}` is
/// kept, because it contains nothing to leak and it is what makes a re-run
/// work without typing the secret again.
pub fn auth_without_literal_secret(auth: &Auth) -> Auth {
    match auth_secret(auth) {
        Some((_, value)) if !value.is_empty() && !is_placeholder_only(value) => {
            with_auth_secret(auth, String::new())
        }
        _ => auth.clone(),
    }
}

/// True when storing this auth in history or an example would store a
/// secret. The upgrade step uses it to find rows written before Phase 9.
pub fn has_literal_auth_secret(auth: &Auth) -> bool {
    matches!(auth_secret(auth), Some((_, value)) if !value.is_empty() && !is_placeholder_only(value))
}

/// A request as history and examples may store it.
pub fn request_without_literal_secrets(request: &HttpRequest) -> HttpRequest {
    HttpRequest {
        auth: auth_without_literal_secret(&request.auth),
        ..request.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{ApiKeyLocation, HttpMethod, RequestBody, RequestSettings};

    #[test]
    fn a_scope_names_table_row_and_field() {
        assert_eq!(
            secret_scope("requests", "req_1", "auth.token"),
            "responderhttp/v1/requests/req_1/auth.token"
        );
    }

    #[test]
    fn only_a_whole_value_placeholder_counts() {
        assert!(is_placeholder_only("{{token}}"));
        assert!(is_placeholder_only("  {{ token }}  "));
        assert!(!is_placeholder_only("Bearer {{token}}"));
        assert!(!is_placeholder_only("{{a}}{{b}}"));
        assert!(!is_placeholder_only("{{}}"));
        assert!(!is_placeholder_only("{{  }}"));
        assert!(!is_placeholder_only("secret"));
        assert!(!is_placeholder_only(""));
    }

    #[test]
    fn each_scheme_names_exactly_one_secret_field() {
        let basic = Auth::Basic {
            username: "ann".into(),
            password: "pw".into(),
        };
        assert_eq!(auth_secret(&basic), Some(("auth.password", "pw")));
        assert_eq!(
            auth_secret(&Auth::Bearer { token: "t".into() }),
            Some(("auth.token", "t"))
        );
        assert_eq!(auth_secret(&Auth::None), None);
    }

    #[test]
    fn replacing_a_secret_keeps_everything_else() {
        let api_key = Auth::ApiKey {
            key: "X-Api-Key".into(),
            value: "old".into(),
            location: ApiKeyLocation::Query,
        };

        assert_eq!(
            with_auth_secret(&api_key, "new".into()),
            Auth::ApiKey {
                key: "X-Api-Key".into(),
                value: "new".into(),
                location: ApiKeyLocation::Query,
            }
        );
    }

    #[test]
    fn a_literal_secret_is_blanked_and_a_placeholder_is_kept() {
        let literal = Auth::Custom {
            header_name: "X-Signature".into(),
            header_value: "abc123".into(),
        };
        let placeholder = Auth::Bearer {
            token: "{{token}}".into(),
        };

        assert_eq!(
            auth_without_literal_secret(&literal),
            Auth::Custom {
                header_name: "X-Signature".into(),
                header_value: String::new(),
            }
        );
        assert_eq!(auth_without_literal_secret(&placeholder), placeholder);
        assert!(has_literal_auth_secret(&literal));
        assert!(!has_literal_auth_secret(&placeholder));
        assert!(!has_literal_auth_secret(&Auth::Bearer {
            token: String::new()
        }));
    }

    #[test]
    fn stripping_a_request_touches_only_its_auth() {
        let request = HttpRequest {
            method: HttpMethod::Get,
            url: "https://example.test/{{path}}".into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: RequestBody::None,
            auth: Auth::Bearer {
                token: "live-token".into(),
            },
            settings: RequestSettings::default(),
        };

        let stripped = request_without_literal_secrets(&request);

        assert_eq!(stripped.url, request.url);
        assert_eq!(
            stripped.auth,
            Auth::Bearer {
                token: String::new()
            }
        );
    }

    #[test]
    fn credential_parameter_names_match_whole_names_in_any_case() {
        assert!(is_credential_param_name("token"));
        assert!(is_credential_param_name(" API_KEY "));
        assert!(is_credential_param_name("Sig"));
        assert!(!is_credential_param_name("token_type"));
        assert!(!is_credential_param_name("keyword"));
        assert!(!is_credential_param_name(""));
    }

    #[test]
    fn the_worst_state_wins() {
        use SecretState::*;
        assert_eq!(Ok.worst(Ok), Ok);
        assert_eq!(Ok.worst(NeedsReentry), NeedsReentry);
        assert_eq!(NeedsReentry.worst(Unavailable), Unavailable);
        assert_eq!(Unavailable.worst(Ok), Unavailable);
    }
}

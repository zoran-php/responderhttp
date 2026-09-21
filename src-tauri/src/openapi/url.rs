// http_client/src-tauri/src/openapi/url.rs
//
// Turns a saved request's URL into what OpenAPI wants from it: a Server, a
// templated path, the path parameters that templating implies, and the query
// parameters written into the URL itself.
//
// The rule, decided 2026-09-14: **`{{variables}}` are the only templating
// signal.** Guessing that `/users/123` means `/users/{id}` invents an API the
// user never described — and gets it wrong on `/v2/`, `/2024/reports` or any
// numeric that is really a constant. A variable is the user stating what
// varies, so it maps exactly and nothing else does.
//
// The cost is that a collection of hardcoded URLs exports literal paths. That
// is the honest result, and the fix is for the user to parameterise the URL —
// something they already know how to do, and which improves the request too.
//
// Pure: no I/O, no environment lookup. Resolving a variable to its value here
// would put live credentials in an exported file (see the export service).
use std::collections::BTreeSet;

/// OpenAPI templating uses single braces; this app uses double. Both appear in
/// this file, so the names are deliberate: `{{name}}` is ours, `{name}` is theirs.
const OPEN: &str = "{{";
const CLOSE: &str = "}}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedUrl {
    /// A Server Object url: either a concrete origin, or `{var}` when the
    /// request's URL began with one of our variables.
    pub server: String,
    /// Server Variable names appearing in `server`, in first-seen order.
    pub server_variables: Vec<String>,
    /// The templated path, always starting with `/`.
    pub path: String,
    /// Path parameter names, in first-seen order. Every one of these needs a
    /// `parameters` entry with `required: true` — the spec has no other option.
    pub path_parameters: Vec<String>,
    /// The pairs typed into the URL after `?`, in order, decoded. A name that
    /// repeats (`?id=1&id=2`) appears once per occurrence; collapsing them is
    /// the caller's decision, because the caller also merges the Params tab.
    pub query_parameters: Vec<QueryPair>,
}

/// One `name=value` from a URL's query string, percent-decoded. `value` is
/// empty for a bare flag (`?verbose`). `{{variables}}` are kept as typed:
/// in a query they are values, not templating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPair {
    pub name: String,
    pub value: String,
}

/// Splits a URL template into server and path.
///
/// Returns None only for input this app would not have let you send anyway —
/// `SendRequest::validate_url` already requires http(s) or a leading variable.
pub fn map_url(url: &str) -> Option<MappedUrl> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (server_raw, rest) = split_server(trimmed)?;
    // Everything after `#` never reaches a server, so it describes nothing.
    let before_fragment = rest.split('#').next().unwrap_or("");
    let (path_raw, query_raw) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));

    let mut server_variables = Vec::new();
    let server = template(server_raw, &mut server_variables);

    let mut path_parameters = Vec::new();
    let path = template(path_raw, &mut path_parameters);
    let path = if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };

    Some(MappedUrl {
        server,
        server_variables,
        path,
        path_parameters,
        query_parameters: parse_query(query_raw),
    })
}

/// `a=1&b=two%20words&flag` as pairs. Split on `&` only: `;` as a separator is
/// a legacy form servers disagree about, and splitting on it would break a
/// value that merely contains one. A pair with no name (`?=x`, `?&`) names
/// nothing a document could describe, so it is skipped.
fn parse_query(query: &str) -> Vec<QueryPair> {
    query
        .split('&')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let name = decode_component(name);
            let name = name.trim();
            (!name.is_empty()).then(|| QueryPair {
                name: name.to_string(),
                value: decode_component(value),
            })
        })
        .collect()
}

/// Percent-decoding as a browser does for a query string: `%XX` becomes the
/// byte, `+` becomes a space. A malformed escape (`%zz`, a trailing `%`) is
/// kept literally rather than rejected — the point is to describe what the
/// user typed — and bytes that do not form UTF-8 leave the whole component as
/// typed for the same reason.
fn decode_component(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => match (
                hex_value(bytes.get(index + 1)),
                hex_value(bytes.get(index + 2)),
            ) {
                (Some(high), Some(low)) => {
                    out.push(high * 16 + low);
                    index += 3;
                }
                _ => {
                    out.push(b'%');
                    index += 1;
                }
            },
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

fn hex_value(byte: Option<&u8>) -> Option<u8> {
    match byte? {
        digit @ b'0'..=b'9' => Some(digit - b'0'),
        letter @ b'a'..=b'f' => Some(letter - b'a' + 10),
        letter @ b'A'..=b'F' => Some(letter - b'A' + 10),
        _ => None,
    }
}

/// Server is everything up to the first path segment: a leading `{{variable}}`
/// if there is one, otherwise `scheme://host[:port]`.
fn split_server(url: &str) -> Option<(&str, &str)> {
    if url.starts_with(OPEN) {
        let end = url.find(CLOSE)? + CLOSE.len();
        return Some((&url[..end], &url[end..]));
    }
    let scheme_end = url.find("://")? + 3;
    let authority_len = url[scheme_end..]
        .find(['/', '?', '#'])
        .unwrap_or(url.len() - scheme_end);
    let split_at = scheme_end + authority_len;
    Some((&url[..split_at], &url[split_at..]))
}

/// Rewrites `{{name}}` as `{name}`, collecting the names. Anything that is not
/// a variable is copied through untouched, including characters OpenAPI would
/// rather we escaped — mangling a path the user typed would be worse than
/// reproducing it.
fn template(input: &str, names: &mut Vec<String>) -> String {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find(OPEN) {
        let after_open = &rest[start + OPEN.len()..];
        let Some(end) = after_open.find(CLOSE) else {
            break;
        };
        let name = after_open[..end].trim();
        out.push_str(&rest[..start]);
        if name.is_empty() {
            // `{{}}` is not a variable; leave it as the user typed it rather
            // than emitting an unnamed parameter the spec cannot express.
            out.push_str(OPEN);
            out.push_str(CLOSE);
        } else {
            out.push('{');
            out.push_str(name);
            out.push('}');
            if seen.insert(name) {
                names.push(name.to_string());
            }
        }
        rest = &after_open[end + CLOSE.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::{map_url, MappedUrl, QueryPair};

    fn pair(name: &str, value: &str) -> QueryPair {
        QueryPair {
            name: name.into(),
            value: value.into(),
        }
    }

    fn mapped(url: &str) -> MappedUrl {
        map_url(url).unwrap_or_else(|| panic!("should map {url}"))
    }

    #[test]
    fn a_concrete_url_splits_into_origin_and_literal_path() {
        let out = mapped("https://api.example.com/users");

        assert_eq!(out.server, "https://api.example.com");
        assert_eq!(out.path, "/users");
        assert!(out.path_parameters.is_empty());
        assert!(out.server_variables.is_empty());
    }

    #[test]
    fn a_port_stays_with_the_server() {
        let out = mapped("http://localhost:8080/v1/health");

        assert_eq!(out.server, "http://localhost:8080");
        assert_eq!(out.path, "/v1/health");
    }

    /// The whole point of the variables-only rule.
    #[test]
    fn a_leading_variable_becomes_the_server() {
        let out = mapped("{{base_url}}/users");

        assert_eq!(out.server, "{base_url}");
        assert_eq!(out.server_variables, vec!["base_url"]);
        assert_eq!(out.path, "/users");
    }

    #[test]
    fn interior_variables_become_path_parameters() {
        let out = mapped("{{base_url}}/users/{{userId}}/posts/{{postId}}");

        assert_eq!(out.path, "/users/{userId}/posts/{postId}");
        assert_eq!(out.path_parameters, vec!["userId", "postId"]);
        assert_eq!(out.server_variables, vec!["base_url"]);
    }

    /// A number is a number. Inventing `{id}` here is exactly what this
    /// design refuses to do.
    #[test]
    fn a_numeric_segment_stays_literal() {
        let out = mapped("https://api.example.com/v2/users/123");

        assert_eq!(out.path, "/v2/users/123");
        assert!(out.path_parameters.is_empty());
    }

    #[test]
    fn a_query_string_is_not_part_of_the_path() {
        let out = mapped("https://api.example.com/search?q=hello&limit=10");

        assert_eq!(out.path, "/search");
        assert!(out.path_parameters.is_empty());
    }

    #[test]
    fn the_query_string_becomes_pairs_in_order() {
        let out = mapped("https://api.example.com/search?q=hello&limit=10");

        assert_eq!(
            out.query_parameters,
            vec![pair("q", "hello"), pair("limit", "10")]
        );
    }

    #[test]
    fn a_url_without_a_query_has_no_pairs() {
        assert!(mapped("https://api.example.com/search")
            .query_parameters
            .is_empty());
        assert!(mapped("https://api.example.com/search?")
            .query_parameters
            .is_empty());
    }

    #[test]
    fn names_and_values_are_percent_decoded_and_plus_is_a_space() {
        let out = mapped("https://x.test/?first%20name=Ada+Lovelace&q=a%26b%3Dc&city=K%C3%B6ln");

        assert_eq!(
            out.query_parameters,
            vec![
                pair("first name", "Ada Lovelace"),
                pair("q", "a&b=c"),
                pair("city", "Köln"),
            ]
        );
    }

    #[test]
    fn a_malformed_escape_is_kept_as_typed() {
        let out = mapped("https://x.test/?a=100%&b=%zz&c=%E2%28");

        assert_eq!(
            out.query_parameters,
            vec![pair("a", "100%"), pair("b", "%zz"), pair("c", "%E2%28")]
        );
    }

    #[test]
    fn a_bare_flag_has_an_empty_value_and_only_the_first_equals_splits() {
        let out = mapped("https://x.test/?verbose&expr=a=b");

        assert_eq!(
            out.query_parameters,
            vec![pair("verbose", ""), pair("expr", "a=b")]
        );
    }

    #[test]
    fn a_pair_with_no_name_is_skipped() {
        let out = mapped("https://x.test/?&=orphan&%20=blank&ok=1&");

        assert_eq!(out.query_parameters, vec![pair("ok", "1")]);
    }

    #[test]
    fn the_fragment_is_not_part_of_the_query() {
        let out = mapped("https://x.test/page?a=1#b=2");

        assert_eq!(out.path, "/page");
        assert_eq!(out.query_parameters, vec![pair("a", "1")]);
    }

    #[test]
    fn a_fragment_before_any_query_hides_what_follows() {
        let out = mapped("https://x.test/page#section?a=1");

        assert_eq!(out.path, "/page");
        assert!(out.query_parameters.is_empty());
    }

    #[test]
    fn a_repeated_name_is_reported_each_time() {
        let out = mapped("https://x.test/?id=1&id=2");

        assert_eq!(out.query_parameters, vec![pair("id", "1"), pair("id", "2")]);
    }

    #[test]
    fn a_query_right_after_a_server_variable_is_still_parsed() {
        let out = mapped("{{base_url}}?page=2");

        assert_eq!(out.server, "{base_url}");
        assert_eq!(out.path, "/");
        assert_eq!(out.query_parameters, vec![pair("page", "2")]);
    }

    /// A variable used only in the query is not a path parameter — emitting one
    /// would produce a document whose path says it needs a value it never uses.
    #[test]
    fn a_variable_in_the_query_does_not_become_a_path_parameter() {
        let out = mapped("https://api.example.com/search?key={{api_key}}");

        assert_eq!(out.path, "/search");
        assert!(out.path_parameters.is_empty());
        // It is kept, as typed, as the query value it is.
        assert_eq!(out.query_parameters, vec![pair("key", "{{api_key}}")]);
    }

    #[test]
    fn a_bare_server_variable_gets_the_root_path() {
        let out = mapped("{{base_url}}");

        assert_eq!(out.server, "{base_url}");
        assert_eq!(out.path, "/");
    }

    #[test]
    fn an_origin_with_no_path_gets_the_root_path() {
        assert_eq!(mapped("https://api.example.com").path, "/");
    }

    #[test]
    fn the_same_variable_twice_is_reported_once() {
        let out = mapped("{{base_url}}/a/{{id}}/b/{{id}}");

        assert_eq!(out.path, "/a/{id}/b/{id}");
        assert_eq!(out.path_parameters, vec!["id"]);
    }

    #[test]
    fn whitespace_inside_the_braces_is_tolerated() {
        let out = mapped("{{ base_url }}/users/{{ userId }}");

        assert_eq!(out.server, "{base_url}");
        assert_eq!(out.path, "/users/{userId}");
        assert_eq!(out.path_parameters, vec!["userId"]);
    }

    /// OAS discourages a parameter that is only part of a segment, but it is
    /// what the user wrote, and dropping it would silently change the request.
    #[test]
    fn a_variable_inside_a_segment_is_templated_where_it_sits() {
        let out = mapped("https://api.example.com/files/report-{{year}}.csv");

        assert_eq!(out.path, "/files/report-{year}.csv");
        assert_eq!(out.path_parameters, vec!["year"]);
    }

    #[test]
    fn an_unclosed_variable_is_left_alone_rather_than_guessed_at() {
        let out = mapped("https://api.example.com/users/{{unclosed");

        assert_eq!(out.path, "/users/{{unclosed");
        assert!(out.path_parameters.is_empty());
    }

    #[test]
    fn an_empty_variable_is_not_a_parameter() {
        let out = mapped("https://api.example.com/a/{{}}/b");

        assert_eq!(out.path, "/a/{{}}/b");
        assert!(out.path_parameters.is_empty());
    }

    #[test]
    fn rejects_only_what_the_app_would_not_have_sent() {
        assert!(map_url("").is_none());
        assert!(map_url("api.example.com/users").is_none());
    }
}

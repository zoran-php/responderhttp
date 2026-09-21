// http_client/src-tauri/src/domain/cookies.rs
//
// RFC 6265 parsing and matching, as pure functions. This is the
// security-relevant part of the cookie feature — a mistake here sends a
// cookie to a host that should never see it — so it holds no I/O, no
// libcurl and no clock: callers pass `now` in.
//
// Known limitation, accepted deliberately (see PLAN.md): there is no public
// suffix list, so a host at `foo.co.uk` could set `Domain=co.uk`. The guard
// is that a Domain attribute must contain a dot and must domain-match the
// request host. That is right for a tool aimed at servers the user chose,
// and would not be enough for a browser.
use crate::domain::models::Cookie;

/// The parts of a URL cookie rules care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestTarget {
    pub secure: bool,
    pub host: String,
    pub path: String,
}

/// Deliberately minimal: enough to find scheme, host and path, without a URL
/// crate. Userinfo and port are stripped because neither takes part in
/// cookie matching.
pub fn request_target(url: &str) -> Option<RequestTarget> {
    let trimmed = url.trim();
    let (secure, rest) = match strip_prefix_ignore_case(trimmed, "https://") {
        Some(rest) => (true, rest),
        // Anything that is not http(s) has no cookie target at all.
        None => (false, strip_prefix_ignore_case(trimmed, "http://")?),
    };

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let after_authority = &rest[authority_end..];

    // Anything before '@' is credentials, not the host.
    let host_part = match authority.rfind('@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    };
    // Leave IPv6 literals intact; only strip a trailing :port.
    let host = if host_part.starts_with('[') {
        match host_part.find(']') {
            Some(end) => &host_part[..=end],
            None => host_part,
        }
    } else {
        match host_part.rfind(':') {
            Some(colon) => &host_part[..colon],
            None => host_part,
        }
    };
    if host.is_empty() {
        return None;
    }

    let path_end = after_authority
        .find(['?', '#'])
        .unwrap_or(after_authority.len());
    let raw_path = &after_authority[..path_end];
    let path = if raw_path.is_empty() {
        "/".to_string()
    } else {
        raw_path.to_string()
    };

    Some(RequestTarget {
        secure,
        host: host.to_ascii_lowercase(),
        path,
    })
}

/// RFC 6265 section 5.1.4.
pub fn default_path(request_path: &str) -> String {
    if !request_path.starts_with('/') {
        return "/".to_string();
    }
    match request_path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(last) => request_path[..last].to_string(),
    }
}

/// RFC 6265 section 5.1.3.
pub fn domain_matches(host: &str, domain: &str) -> bool {
    if host == domain {
        return true;
    }
    host.len() > domain.len()
        && host.ends_with(domain)
        && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
}

/// RFC 6265 section 5.1.4. `/foo` must not match `/foobar`, which is the
/// case a naive `starts_with` gets wrong.
pub fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if !request_path.starts_with(cookie_path) {
        return false;
    }
    cookie_path.ends_with('/') || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/')
}

/// Parses one `Set-Cookie` value. Returns None when the cookie must be
/// ignored — no name, or a Domain the sending host has no right to set.
pub fn parse_set_cookie(line: &str, target: &RequestTarget, now: u64) -> Option<Cookie> {
    let line = strip_prefix_ignore_case(line.trim(), "set-cookie:").unwrap_or(line.trim());

    let mut parts = line.split(';');
    let pair = parts.next()?;
    let (raw_name, raw_value) = pair.split_once('=')?;
    let name = raw_name.trim();
    if name.is_empty() {
        return None;
    }
    let value = unquote(raw_value.trim());

    let mut domain: Option<String> = None;
    let mut path: Option<String> = None;
    let mut expires: Option<u64> = None;
    let mut max_age: Option<i64> = None;
    let mut secure = false;
    let mut http_only = false;

    for attribute in parts {
        let (key, attr_value) = match attribute.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => (attribute.trim(), ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "domain" => {
                let candidate = attr_value.trim_start_matches('.').to_ascii_lowercase();
                if !candidate.is_empty() {
                    domain = Some(candidate);
                }
            }
            // A Path that is not absolute is ignored, and the default
            // path applies instead (RFC 6265 section 5.2.4).
            "path" if attr_value.starts_with('/') => path = Some(attr_value.to_string()),
            "expires" => expires = parse_http_date(attr_value),
            "max-age" => max_age = attr_value.parse::<i64>().ok(),
            "secure" => secure = true,
            "httponly" => http_only = true,
            _ => {}
        }
    }

    // Max-Age wins over Expires (RFC 6265 section 5.3 step 3), and a
    // non-positive Max-Age means "expire now".
    let expires_at = match max_age {
        Some(seconds) if seconds <= 0 => Some(0),
        Some(seconds) => Some(now.saturating_add(seconds as u64)),
        None => expires,
    };

    let (domain, host_only) = match domain {
        Some(candidate) => {
            // A host may only set cookies for itself or a parent it belongs
            // to, and never for something without a dot in it.
            if !candidate.contains('.') || !domain_matches(&target.host, &candidate) {
                return None;
            }
            (candidate, false)
        }
        None => (target.host.clone(), true),
    };

    Some(Cookie {
        name: name.to_string(),
        value,
        domain,
        path: path.unwrap_or_else(|| default_path(&target.path)),
        expires_at,
        secure,
        http_only,
        host_only,
        created_at: now,
    })
}

pub fn is_expired(cookie: &Cookie, now: u64) -> bool {
    matches!(cookie.expires_at, Some(expiry) if expiry <= now)
}

/// RFC 6265 section 5.4: everything that matches, longest path first, then
/// oldest first. Servers rely on that order when two cookies share a name.
pub fn cookies_for_request<'a>(
    jar: &'a [Cookie],
    target: &RequestTarget,
    now: u64,
) -> Vec<&'a Cookie> {
    let mut matched: Vec<&Cookie> = jar
        .iter()
        .filter(|cookie| {
            if is_expired(cookie, now) {
                return false;
            }
            if cookie.secure && !target.secure {
                return false;
            }
            let domain_ok = if cookie.host_only {
                target.host == cookie.domain
            } else {
                domain_matches(&target.host, &cookie.domain)
            };
            domain_ok && path_matches(&target.path, &cookie.path)
        })
        .collect();

    matched.sort_by(|a, b| {
        b.path
            .len()
            .cmp(&a.path.len())
            .then(a.created_at.cmp(&b.created_at))
    });
    matched
}

pub fn cookie_header(cookies: &[&Cookie]) -> Option<String> {
    if cookies.is_empty() {
        return None;
    }
    Some(
        cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// IMF-fixdate only (`Sun, 06 Nov 1994 08:49:37 GMT`), which is what servers
/// actually send. Anything else returns None, and the caller then treats the
/// cookie as a session cookie — erring toward expiring sooner, never later.
/// A date crate would buy the two obsolete formats and little else.
fn parse_http_date(value: &str) -> Option<u64> {
    let cleaned = value.trim().trim_end_matches("GMT").trim();
    let rest = match cleaned.split_once(',') {
        Some((_weekday, rest)) => rest.trim(),
        None => cleaned,
    };
    let mut fields = rest.split_whitespace();
    let day: u32 = fields.next()?.parse().ok()?;
    let month_name = fields.next()?.to_ascii_lowercase();
    let month = MONTHS.iter().position(|name| *name == month_name)? as u32 + 1;
    let year: i64 = fields.next()?.parse().ok()?;
    let mut clock = fields.next()?.split(':');
    let hour: u64 = clock.next()?.parse().ok()?;
    let minute: u64 = clock.next()?.parse().ok()?;
    let second: u64 = clock.next()?.parse().ok()?;

    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + (hour * 3_600 + minute * 60 + second) as i64;
    u64::try_from(seconds).ok()
}

/// Howard Hinnant's days_from_civil: days since 1970-01-01, no leap-year
/// special cases at the call site.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * shifted_month + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn unquote(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

fn strip_prefix_ignore_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&value[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000;

    fn target(url: &str) -> RequestTarget {
        request_target(url).expect("test urls should parse")
    }

    fn parse(line: &str, url: &str) -> Option<Cookie> {
        parse_set_cookie(line, &target(url), NOW)
    }

    #[test]
    fn splits_a_url_into_the_parts_matching_cares_about() {
        assert_eq!(
            target("https://Example.COM:8443/api/v1/users?q=1#frag"),
            RequestTarget {
                secure: true,
                host: "example.com".into(),
                path: "/api/v1/users".into(),
            }
        );
    }

    #[test]
    fn a_url_with_no_path_gets_the_root_path() {
        assert_eq!(target("http://example.com").path, "/");
        assert_eq!(target("http://example.com?q=1").path, "/");
    }

    #[test]
    fn credentials_are_not_mistaken_for_the_host() {
        assert_eq!(
            target("https://user:pass@example.com/x").host,
            "example.com"
        );
    }

    #[test]
    fn an_ipv6_literal_keeps_its_brackets() {
        assert_eq!(target("http://[::1]:8080/x").host, "[::1]");
    }

    #[test]
    fn a_url_without_a_supported_scheme_has_no_target() {
        assert!(request_target("ftp://example.com/x").is_none());
    }

    #[test]
    fn default_path_drops_the_last_segment() {
        assert_eq!(default_path("/api/v1/users"), "/api/v1");
        assert_eq!(default_path("/users"), "/");
        assert_eq!(default_path("/"), "/");
        assert_eq!(default_path("no-slash"), "/");
    }

    #[test]
    fn domain_matches_a_host_and_its_subdomains_only() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(domain_matches("api.example.com", "example.com"));
        assert!(!domain_matches("notexample.com", "example.com"));
        assert!(!domain_matches("example.com", "api.example.com"));
    }

    #[test]
    fn path_matching_respects_segment_boundaries() {
        assert!(path_matches("/foo", "/foo"));
        assert!(path_matches("/foo/bar", "/foo"));
        assert!(path_matches("/foo/bar", "/foo/"));
        assert!(path_matches("/foo", "/"));
        // The case a plain starts_with gets wrong.
        assert!(!path_matches("/foobar", "/foo"));
    }

    #[test]
    fn parses_a_bare_name_and_value() {
        let cookie = parse("sid=abc123", "https://example.com/api/v1/users").expect("parses");
        assert_eq!(cookie.name, "sid");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.domain, "example.com");
        assert!(cookie.host_only);
        assert_eq!(cookie.path, "/api/v1");
        assert_eq!(cookie.expires_at, None);
        assert!(!cookie.secure);
    }

    #[test]
    fn tolerates_the_header_name_being_included() {
        let cookie = parse("Set-Cookie: sid=abc", "https://example.com/").expect("parses");
        assert_eq!(cookie.name, "sid");
    }

    #[test]
    fn reads_the_attributes_it_supports() {
        let cookie = parse(
            "sid=abc; Domain=example.com; Path=/; Secure; HttpOnly",
            "https://api.example.com/x",
        )
        .expect("parses");
        assert_eq!(cookie.domain, "example.com");
        assert!(!cookie.host_only);
        assert_eq!(cookie.path, "/");
        assert!(cookie.secure);
        assert!(cookie.http_only);
    }

    #[test]
    fn strips_quotes_from_a_quoted_value() {
        let cookie = parse("sid=\"abc\"", "https://example.com/").expect("parses");
        assert_eq!(cookie.value, "abc");
    }

    #[test]
    fn a_leading_dot_on_domain_is_ignored() {
        let cookie = parse("sid=abc; Domain=.example.com", "https://example.com/").expect("parses");
        assert_eq!(cookie.domain, "example.com");
    }

    #[test]
    fn rejects_a_domain_the_host_does_not_belong_to() {
        assert!(parse("sid=abc; Domain=evil.com", "https://example.com/").is_none());
        assert!(parse("sid=abc; Domain=api.example.com", "https://example.com/").is_none());
    }

    #[test]
    fn rejects_a_domain_with_no_dot() {
        // The partial stand-in for a public suffix list.
        assert!(parse("sid=abc; Domain=com", "https://example.com/").is_none());
    }

    #[test]
    fn rejects_a_pair_with_no_name() {
        assert!(parse("=abc", "https://example.com/").is_none());
        assert!(parse("novalue", "https://example.com/").is_none());
    }

    #[test]
    fn max_age_wins_over_expires() {
        let cookie = parse(
            "sid=abc; Expires=Sun, 06 Nov 1994 08:49:37 GMT; Max-Age=60",
            "https://example.com/",
        )
        .expect("parses");
        assert_eq!(cookie.expires_at, Some(NOW + 60));
    }

    #[test]
    fn a_non_positive_max_age_expires_immediately() {
        let cookie = parse("sid=abc; Max-Age=0", "https://example.com/").expect("parses");
        assert_eq!(cookie.expires_at, Some(0));
        assert!(is_expired(&cookie, NOW));
    }

    #[test]
    fn parses_imf_fixdate_expires() {
        let cookie = parse(
            "sid=abc; Expires=Sun, 06 Nov 1994 08:49:37 GMT",
            "https://example.com/",
        )
        .expect("parses");
        assert_eq!(cookie.expires_at, Some(784_111_777));
    }

    #[test]
    fn handles_a_leap_day_and_a_century_boundary() {
        assert_eq!(
            parse_http_date("Thu, 29 Feb 2024 12:00:00 GMT"),
            Some(1_709_208_000)
        );
        assert_eq!(
            parse_http_date("Wed, 01 Mar 2000 00:00:00 GMT"),
            Some(951_868_800)
        );
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    }

    #[test]
    fn an_unparseable_expires_degrades_to_a_session_cookie() {
        let cookie = parse("sid=abc; Expires=not-a-date", "https://example.com/").expect("parses");
        assert_eq!(cookie.expires_at, None);
    }

    fn cookie(name: &str, domain: &str, path: &str, created_at: u64) -> Cookie {
        Cookie {
            name: name.into(),
            value: format!("v-{name}"),
            domain: domain.into(),
            path: path.into(),
            expires_at: None,
            secure: false,
            http_only: false,
            host_only: false,
            created_at,
        }
    }

    #[test]
    fn sends_only_cookies_whose_domain_and_path_match() {
        let jar = vec![
            cookie("a", "example.com", "/", 1),
            cookie("b", "other.com", "/", 2),
            cookie("c", "example.com", "/deep", 3),
        ];
        let matched = cookies_for_request(&jar, &target("https://example.com/x"), NOW);
        assert_eq!(
            matched.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
    }

    #[test]
    fn a_host_only_cookie_does_not_reach_subdomains() {
        let mut host_only = cookie("a", "example.com", "/", 1);
        host_only.host_only = true;
        let jar = vec![host_only];
        assert!(cookies_for_request(&jar, &target("https://api.example.com/"), NOW).is_empty());
        assert_eq!(
            cookies_for_request(&jar, &target("https://example.com/"), NOW).len(),
            1
        );
    }

    #[test]
    fn a_secure_cookie_never_travels_over_plain_http() {
        let mut secure = cookie("a", "example.com", "/", 1);
        secure.secure = true;
        let jar = vec![secure];
        assert!(cookies_for_request(&jar, &target("http://example.com/"), NOW).is_empty());
        assert_eq!(
            cookies_for_request(&jar, &target("https://example.com/"), NOW).len(),
            1
        );
    }

    #[test]
    fn an_expired_cookie_is_not_sent() {
        let mut expired = cookie("a", "example.com", "/", 1);
        expired.expires_at = Some(NOW - 1);
        assert!(cookies_for_request(&[expired], &target("https://example.com/"), NOW).is_empty());
    }

    #[test]
    fn longest_path_comes_first_then_oldest() {
        let jar = vec![
            cookie("short", "example.com", "/", 10),
            cookie("long", "example.com", "/api/v1", 20),
            cookie("older", "example.com", "/", 5),
        ];
        let matched = cookies_for_request(&jar, &target("https://example.com/api/v1/x"), NOW);
        assert_eq!(
            matched.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["long", "older", "short"]
        );
    }

    #[test]
    fn builds_the_header_a_server_expects() {
        let jar = vec![
            cookie("a", "example.com", "/", 1),
            cookie("b", "example.com", "/", 2),
        ];
        let matched = cookies_for_request(&jar, &target("https://example.com/"), NOW);
        assert_eq!(cookie_header(&matched).as_deref(), Some("a=v-a; b=v-b"));
        assert_eq!(cookie_header(&[]), None);
    }
}

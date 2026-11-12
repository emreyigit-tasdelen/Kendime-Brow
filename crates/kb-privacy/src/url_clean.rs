//! Tracking parameter stripping and redirect-wrapper skipping (architecture §4.2, task 2.4).

/// Query parameters removed from every URL (exact names, case-insensitive).
pub const TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "gbraid",
    "wbraid",
    "msclkid",
    "yclid",
    "twclid",
    "ttclid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "oly_anon_id",
    "oly_enc_id",
    "vero_id",
    "__s",
    "s_cid",
    "srsltid",
    "_ga",
    "_gl",
];

/// Prefixes removed from every URL (e.g. `utm_source`, `utm_medium`, …).
pub const TRACKING_PREFIXES: &[&str] = &["utm_", "pk_", "mtm_", "hsa_"];

fn is_tracking(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    TRACKING_PARAMS.contains(&n.as_str()) || TRACKING_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// Removes tracking parameters. Returns `None` when nothing changed.
pub fn strip_tracking_params(url: &str) -> Option<String> {
    let (base, fragment) = match url.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (url, None),
    };
    let (path, query) = base.split_once('?')?;
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| {
            let name = kv.split('=').next().unwrap_or_default();
            !name.is_empty() && !is_tracking(name)
        })
        .collect();
    let total = query.split('&').filter(|kv| !kv.is_empty()).count();
    if kept.len() == total {
        return None;
    }
    let mut out = path.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    Some(out)
}

/// Known redirect wrappers: (host, path prefix, parameter holding the real URL).
const WRAPPERS: &[(&str, &str, &str)] = &[
    ("www.google.com", "/url", "q"),
    ("www.google.com", "/url", "url"),
    ("l.facebook.com", "/l.php", "u"),
    ("lm.facebook.com", "/l.php", "u"),
    ("l.instagram.com", "/", "u"),
    ("out.reddit.com", "/", "url"),
    ("www.youtube.com", "/redirect", "q"),
    ("slack-redir.net", "/link", "url"),
    ("steamcommunity.com", "/linkfilter/", "url"),
    ("disq.us", "/url", "url"),
];

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let h = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(h, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// If `url` is a known redirect wrapper, returns the wrapped http(s) URL.
pub fn unwrap_redirect(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path_query) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
    let path_query = format!("/{path_query}");
    let (path, query) = path_query.split_once('?')?;
    let host = host.to_ascii_lowercase();
    for (h, prefix, param) in WRAPPERS {
        if host == *h && path.starts_with(prefix) {
            for kv in query.split('&') {
                if let Some((k, v)) = kv.split_once('=')
                    && k == *param
                {
                    let target = percent_decode(v)?;
                    if target.starts_with("https://") || target.starts_with("http://") {
                        return Some(target);
                    }
                }
            }
        }
    }
    None
}

/// Unwraps redirect wrappers and strips tracking parameters. `None` = unchanged.
pub fn clean_url(url: &str) -> Option<String> {
    let unwrapped = unwrap_redirect(url);
    let current = unwrapped.as_deref().unwrap_or(url);
    match strip_tracking_params(current) {
        Some(s) => Some(s),
        None => unwrapped,
    }
}

/// Lowercase host of an http(s) URL (`None` otherwise).
pub fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| format!("{h}]"))?
    } else {
        host.split(':').next()?.to_string()
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tracking_params() {
        assert_eq!(
            strip_tracking_params("https://a.org/p?id=7&utm_source=x&UTM_Medium=y&fbclid=z#top")
                .as_deref(),
            Some("https://a.org/p?id=7#top")
        );
        assert_eq!(
            strip_tracking_params("https://a.org/p?gclid=1").as_deref(),
            Some("https://a.org/p")
        );
        assert_eq!(strip_tracking_params("https://a.org/p?id=7&page=2"), None);
        assert_eq!(strip_tracking_params("https://a.org/p"), None);
        assert_eq!(strip_tracking_params("https://a.org/p?utmost=1"), None);
    }

    #[test]
    fn unwraps_redirects() {
        assert_eq!(
            unwrap_redirect(
                "https://www.google.com/url?sa=t&q=https%3A%2F%2Fexample.org%2Fa%3Fb%3D1&usg=x"
            )
            .as_deref(),
            Some("https://example.org/a?b=1")
        );
        assert_eq!(
            unwrap_redirect("https://l.facebook.com/l.php?u=https%3A%2F%2Fnews.org%2F&h=AT0")
                .as_deref(),
            Some("https://news.org/")
        );
        assert_eq!(
            unwrap_redirect("https://www.google.com/search?q=rust"),
            None
        );
        assert_eq!(
            unwrap_redirect("https://www.google.com/url?q=javascript%3Aalert(1)"),
            None
        );
        assert_eq!(
            unwrap_redirect("https://evil.org/url?q=https%3A%2F%2Fx.org"),
            None
        );
    }

    #[test]
    fn clean_combines_both() {
        assert_eq!(
            clean_url("https://l.facebook.com/l.php?u=https%3A%2F%2Fnews.org%2F%3Futm_source%3Dfb%26id%3D3").as_deref(),
            Some("https://news.org/?id=3")
        );
        assert_eq!(clean_url("https://news.org/?id=3"), None);
    }

    #[test]
    fn hosts() {
        assert_eq!(
            host_of("https://User@Www.Example.org:8443/x").as_deref(),
            Some("www.example.org")
        );
        assert_eq!(host_of("http://[::1]:80/").as_deref(), Some("[::1]"));
        assert_eq!(host_of("kb://ui/"), None);
    }
}

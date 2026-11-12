//! Origin matching for autofill (task 6.2). A saved login may only be offered to a page whose
//! origin (scheme, host, port) equals the entry's URL. No subdomain or parent-domain matching,
//! and a secret saved for HTTPS is never offered to plain HTTP. Plain HTTP is only accepted for
//! loopback addresses so local development works.

/// `(scheme, host, port)` with default ports filled in, host lower-cased.
fn origin(url: &str) -> Option<(String, String, u16)> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let hostport = authority.rsplit('@').next()?;
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p))
            if !h.is_empty() && !h.ends_with(']') || (h.starts_with('[') && h.ends_with(']')) =>
        {
            (h, Some(p.parse::<u16>().ok()?))
        }
        _ => (hostport, None),
    };
    if host.is_empty() {
        return None;
    }
    let default = match scheme.as_str() {
        "https" => 443,
        "http" => 80,
        _ => return None,
    };
    Some((scheme, host.to_ascii_lowercase(), port.unwrap_or(default)))
}

fn loopback(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "[::1]"
}

/// Whether a login saved for `entry_url` may be filled on `page_url`.
/// An entry URL without a scheme is treated as `https://`.
pub fn may_fill(entry_url: &str, page_url: &str) -> bool {
    let entry = if entry_url.contains("://") {
        entry_url.to_string()
    } else {
        format!("https://{}", entry_url.trim())
    };
    let (Some(e), Some(p)) = (origin(&entry), origin(page_url)) else {
        return false;
    };
    if e != p {
        return false;
    }
    p.0 == "https" || loopback(&p.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_origin_only() {
        assert!(may_fill(
            "https://example.com/login",
            "https://example.com/account?x=1"
        ));
        assert!(may_fill("example.com", "https://EXAMPLE.com/"));
        assert!(may_fill("https://example.com:443/", "https://example.com/"));
        assert!(!may_fill("https://example.com", "https://www.example.com"));
        assert!(!may_fill("https://www.example.com", "https://example.com"));
        assert!(!may_fill(
            "https://example.com",
            "https://example.com.evil.test/"
        ));
        assert!(!may_fill(
            "https://example.com",
            "https://example.com:8443/"
        ));
    }

    #[test]
    fn https_secrets_never_go_to_http_but_loopback_http_is_allowed() {
        assert!(!may_fill("https://example.com", "http://example.com/"));
        assert!(!may_fill("http://example.com", "http://example.com/"));
        assert!(may_fill(
            "http://127.0.0.1:3000",
            "http://127.0.0.1:3000/login"
        ));
        assert!(may_fill(
            "http://localhost:8080/",
            "http://localhost:8080/x"
        ));
        assert!(!may_fill(
            "http://localhost:8080/",
            "http://localhost:9090/x"
        ));
    }

    #[test]
    fn userinfo_cannot_spoof_the_host() {
        let spoof = format!("https://example.com{}evil.test/", '@');
        assert!(!may_fill("https://example.com", &spoof));
        assert!(may_fill("https://evil.test", &spoof));
    }

    #[test]
    fn garbage_never_matches() {
        assert!(!may_fill("", "https://example.com/"));
        assert!(!may_fill("https://example.com", "kb://newtab/"));
        assert!(!may_fill("https://example.com", "javascript:alert(1)"));
        assert!(!may_fill("https://", "https://"));
    }
}

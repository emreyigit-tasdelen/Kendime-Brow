//! `kb://` internal scheme: URL → embedded asset resolution (architecture §3.4).
//!
//! Pure logic, no CEF types, so it can be unit-tested. The CEF glue lives in `kb_scheme.rs`.

/// Scheme name (registered as standard + secure + CORS-enabled).
pub const SCHEME: &str = "kb";

/// Strict CSP for every `kb://` page: no inline scripts, no external network requests.
pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'; form-action 'none'";

/// An embedded asset: `host/path` key and bytes.
struct Asset {
    key: &'static str,
    bytes: &'static [u8],
}

macro_rules! assets {
    ($($key:literal),* $(,)?) => {
        &[$(Asset { key: $key, bytes: include_bytes!(concat!("../assets/", $key)) }),*]
    };
}

/// Embedded assets. Replaced by the Vite build output via `rust-embed` in task 1.7.
static ASSETS: &[Asset] = assets!["ui/index.html", "ui/style.css", "newtab/index.html"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: i32,
    pub status_text: &'static str,
    pub mime: &'static str,
    pub body: &'static [u8],
}

static NOT_FOUND: &[u8] = b"404 Not Found";

/// Resolves a full `kb://host/path?query#fragment` URL to an embedded asset.
pub fn resolve(url: &str) -> Response {
    match asset_key(url).and_then(|k| ASSETS.iter().find(|a| a.key == k)) {
        Some(asset) => Response {
            status: 200,
            status_text: "OK",
            mime: mime_for(asset.key),
            body: asset.bytes,
        },
        None => Response {
            status: 404,
            status_text: "Not Found",
            mime: "text/plain",
            body: NOT_FOUND,
        },
    }
}

/// `kb://ui/` → `ui/index.html`, `kb://ui/style.css?x=1` → `ui/style.css`.
/// Returns `None` for other schemes, empty hosts and any path traversal attempt.
pub fn asset_key(url: &str) -> Option<String> {
    let rest = url.strip_prefix("kb://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    if host.is_empty() || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let mut segments = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" => {}
            "." | ".." => return None,
            s if s.contains('\\') || s.contains('%') => return None,
            s => segments.push(s),
        }
    }
    let path = if segments.is_empty() {
        "index.html".to_string()
    } else {
        segments.join("/")
    };
    Some(format!("{host}/{path}"))
}

pub fn mime_for(key: &str) -> &'static str {
    match key.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html",
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(asset_key("kb://ui").as_deref(), Some("ui/index.html"));
        assert_eq!(asset_key("kb://ui/").as_deref(), Some("ui/index.html"));
        assert_eq!(
            asset_key("kb://UI/style.css?v=2#x").as_deref(),
            Some("ui/style.css")
        );
        assert_eq!(asset_key("kb://ui//a//b.js").as_deref(), Some("ui/a/b.js"));
    }

    #[test]
    fn rejects_traversal_and_foreign_urls() {
        for url in [
            "https://ui/index.html",
            "kb://",
            "kb:///index.html",
            "kb://ui/../secret",
            "kb://ui/./x",
            "kb://ui/%2e%2e/x",
            "kb://ui/a\\b",
            "kb://u_i/x",
        ] {
            assert_eq!(asset_key(url), None, "{url}");
        }
    }

    #[test]
    fn resolves_embedded_assets() {
        let r = resolve("kb://ui/");
        assert_eq!((r.status, r.mime), (200, "text/html"));
        assert!(r.body.starts_with(b"<!doctype html>"));
        assert_eq!(resolve("kb://ui/style.css").mime, "text/css");
        assert_eq!(resolve("kb://newtab").status, 200);
        assert_eq!(resolve("kb://ui/missing.js").status, 404);
    }

    #[test]
    fn csp_blocks_inline_and_external() {
        assert!(CSP.contains("default-src 'self'"));
        assert!(!CSP.contains("unsafe-inline"));
        assert!(!CSP.contains("http"));
    }
}

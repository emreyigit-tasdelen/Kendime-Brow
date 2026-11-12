//! Userscript support core (task 6.5): `==UserScript==` header parsing and URL matching for
//! `@match` (Chrome match patterns), `@include` / `@exclude` (globs, or `/regex/`) and
//! `@exclude-match`. Injection into pages lives in kb-shell.

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Meta {
    pub name: String,
    pub version: String,
    pub description: String,
    pub matches: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub exclude_matches: Vec<String>,
    pub grants: Vec<String>,
    /// `document-start`, `document-end` (default) or `document-idle`.
    pub run_at: String,
}

/// Parses the metadata block. Returns `None` when there is no `==UserScript==` block.
pub fn parse_meta(source: &str) -> Option<Meta> {
    let mut in_block = false;
    let mut found = false;
    let mut m = Meta {
        run_at: "document-end".into(),
        ..Meta::default()
    };
    for line in source.lines() {
        let line = line.trim();
        if !in_block {
            if line.starts_with("// ==UserScript==") {
                in_block = true;
                found = true;
            }
            continue;
        }
        if line.starts_with("// ==/UserScript==") {
            break;
        }
        let Some(rest) = line.strip_prefix("//") else {
            continue;
        };
        let rest = rest.trim();
        let Some(rest) = rest.strip_prefix('@') else {
            continue;
        };
        let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let value = value.trim().to_string();
        match key {
            "name" => m.name = value,
            "version" => m.version = value,
            "description" => m.description = value,
            "match" => m.matches.push(value),
            "include" => m.includes.push(value),
            "exclude" => m.excludes.push(value),
            "exclude-match" => m.exclude_matches.push(value),
            "grant" => m.grants.push(value),
            "run-at" => m.run_at = value,
            _ => {}
        }
    }
    found.then_some(m)
}

struct Url<'a> {
    scheme: &'a str,
    host: &'a str,
    path: &'a str,
}

fn split_url(url: &str) -> Option<Url<'_>> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    // user:pass@ and :port are not part of the match host.
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    // the path match includes the query string but not the fragment.
    let path = path.split('#').next().unwrap_or("/");
    Some(Url {
        scheme,
        host,
        path: if path.is_empty() { "/" } else { path },
    })
}

/// Glob with `*` as the only wildcard.
fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            return text.len() >= pos + part.len() && text[pos..].ends_with(part);
        } else if let Some(at) = text[pos..].find(part) {
            pos += at + part.len();
        } else {
            return false;
        }
    }
    true
}

/// Chrome match pattern: `<scheme>://<host><path>`; `*` scheme means http/https, `*.` host
/// prefix matches subdomains, `<all_urls>` matches http, https and file.
pub fn match_pattern(pattern: &str, url: &str) -> bool {
    let Some(u) = split_url(url) else {
        return false;
    };
    if pattern == "<all_urls>" {
        return matches!(u.scheme, "http" | "https" | "file");
    }
    let Some((scheme, rest)) = pattern.split_once("://") else {
        return false;
    };
    let end = rest.find('/').unwrap_or(rest.len());
    let (host, path) = rest.split_at(end);
    let scheme_ok = scheme == u.scheme || (scheme == "*" && matches!(u.scheme, "http" | "https"));
    if !scheme_ok {
        return false;
    }
    let host_ok = if host == "*" {
        true
    } else if let Some(base) = host.strip_prefix("*.") {
        u.host == base || u.host.ends_with(&format!(".{base}"))
    } else {
        host == u.host
    };
    host_ok && glob(if path.is_empty() { "/" } else { path }, u.path)
}

/// `@include` / `@exclude` entry: `/regex/` is not supported (no regex engine), a glob is.
fn include_match(entry: &str, url: &str) -> bool {
    if entry.len() > 1 && entry.starts_with('/') && entry.ends_with('/') {
        return false;
    }
    glob(&entry.to_ascii_lowercase(), &url.to_ascii_lowercase())
}

/// Whether the script should run on `url`. With no `@match` / `@include`, it runs nowhere.
pub fn should_run(m: &Meta, url: &str) -> bool {
    if m.exclude_matches.iter().any(|p| match_pattern(p, url))
        || m.excludes.iter().any(|p| include_match(p, url))
    {
        return false;
    }
    m.matches.iter().any(|p| match_pattern(p, url))
        || m.includes.iter().any(|p| include_match(p, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "// ==UserScript==\n// @name  Dark mode\n// @version 1.2\n// @match https://*.example.com/*\n// @match *://news.test/a/*\n// @exclude-match https://admin.example.com/*\n// @grant GM_getValue\n// @run-at document-start\n// ==/UserScript==\nconsole.log(1)";

    #[test]
    fn parses_the_header() {
        let m = parse_meta(SRC).unwrap();
        assert_eq!(
            (m.name.as_str(), m.version.as_str(), m.run_at.as_str()),
            ("Dark mode", "1.2", "document-start")
        );
        assert_eq!(m.matches.len(), 2);
        assert_eq!(m.grants, ["GM_getValue"]);
        assert!(parse_meta("console.log(1)").is_none());
        assert_eq!(
            parse_meta("// ==UserScript==\n// @name x\n// ==/UserScript==")
                .unwrap()
                .run_at,
            "document-end"
        );
    }

    #[test]
    fn match_patterns() {
        assert!(match_pattern(
            "https://*.example.com/*",
            "https://www.example.com/x?y=1#f"
        ));
        assert!(match_pattern(
            "https://*.example.com/*",
            "https://example.com/"
        ));
        assert!(!match_pattern(
            "https://*.example.com/*",
            "https://evilexample.com/"
        ));
        assert!(!match_pattern(
            "https://*.example.com/*",
            "http://www.example.com/"
        ));
        assert!(match_pattern("*://a.test/*", "http://a.test:8080/p"));
        assert!(match_pattern(
            "https://a.test/path/*",
            "https://a.test/path/deep/er"
        ));
        assert!(!match_pattern(
            "https://a.test/path",
            "https://a.test/path/more"
        ));
        assert!(match_pattern("<all_urls>", "https://x.y/"));
        assert!(!match_pattern("<all_urls>", "kb://newtab/"));
    }

    #[test]
    fn should_run_applies_excludes() {
        let m = parse_meta(SRC).unwrap();
        assert!(should_run(&m, "https://www.example.com/page"));
        assert!(should_run(&m, "https://news.test/a/1"));
        assert!(!should_run(&m, "https://admin.example.com/x"));
        assert!(!should_run(&m, "https://other.test/"));
    }

    #[test]
    fn include_globs_and_default_deny() {
        let m = Meta {
            includes: vec!["https://site.test/*".into()],
            excludes: vec!["*logout*".into()],
            ..Meta::default()
        };
        assert!(should_run(&m, "https://SITE.test/home"));
        assert!(!should_run(&m, "https://site.test/logout"));
        assert!(!should_run(&Meta::default(), "https://site.test/"));
    }

    #[test]
    fn user_info_and_ports_do_not_confuse_the_host() {
        assert!(match_pattern(
            "https://a.test/*",
            &format!("https://u:p{}a.test:444/x", '@')
        ));
        assert!(!match_pattern(
            "https://a.test/*",
            "https://a.test.evil.com/x"
        ));
    }
}

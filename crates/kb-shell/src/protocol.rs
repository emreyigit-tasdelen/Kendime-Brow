//! UI ↔ shell bridge protocol (architecture §3.3, task 1.8). Pure: JSON types and the
//! address-bar input → URL rule. Transport is the CEF message router (`bridge.rs`).

use serde::{Deserialize, Serialize};

/// Requests sent by `kb://` pages via `window.kbQuery({request: JSON})`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Command {
    /// One-shot: returns the current [`State`].
    State,
    /// Persistent: receives a [`State`] on every change until cancelled.
    Subscribe,
    NewTab {
        #[serde(default)]
        url: Option<String>,
    },
    CloseTab {
        id: u64,
    },
    ActivateTab {
        id: u64,
    },
    /// Address bar input (URL or search terms) for the active tab.
    Navigate {
        text: String,
    },
    Back,
    Forward,
    Reload,
    Stop,
}

pub fn parse(request: &str) -> Result<Command, String> {
    serde_json::from_str(request).map_err(|e| format!("bad request: {e}"))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabState {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub active: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct State {
    pub tabs: Vec<TabState>,
}

impl State {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"tabs\":[]}".into())
    }
}

/// Search engine used for non-URL address bar input. Privacy-respecting default; becomes a
/// setting in 1.12.
pub const SEARCH_URL: &str = "https://duckduckgo.com/?q=";

/// Turns address bar text into a URL:
/// - explicit schemes (`https:`, `kb:`, `file:`, `about:` …) are kept as they are,
/// - `localhost[:port]`, IPs and `host.tld[/…]` without spaces get `https://`,
/// - anything else becomes a search.
pub fn input_to_url(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return "kb://newtab/".into();
    }
    let lower = t.to_ascii_lowercase();
    const SCHEMES: &[&str] = &[
        "http://",
        "https://",
        "kb://",
        "file://",
        "about:",
        "data:",
        "view-source:",
        "ftp://",
    ];
    if SCHEMES.iter().any(|s| lower.starts_with(s)) {
        return t.to_string();
    }
    let host = t.split(['/', '?', '#']).next().unwrap_or_default();
    let host_no_port = host.rsplit_once(':').map_or(host, |(h, p)| {
        if p.chars().all(|c| c.is_ascii_digit()) {
            h
        } else {
            host
        }
    });
    let looks_like_host = !t.contains(char::is_whitespace)
        && (host_no_port.eq_ignore_ascii_case("localhost")
            || (host_no_port.contains('.')
                && !host_no_port.starts_with('.')
                && !host_no_port.ends_with('.')
                && host_no_port
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')));
    if looks_like_host {
        format!("https://{t}")
    } else {
        format!("{SEARCH_URL}{}", encode_query(t))
    }
}

fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Only `kb://` documents may use the bridge.
pub fn is_trusted_origin(url: &str) -> bool {
    url.get(..5)
        .is_some_and(|p| p.eq_ignore_ascii_case("kb://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(parse(r#"{"cmd":"state"}"#), Ok(Command::State));
        assert_eq!(
            parse(r#"{"cmd":"new-tab"}"#),
            Ok(Command::NewTab { url: None })
        );
        assert_eq!(
            parse(r#"{"cmd":"close-tab","id":3}"#),
            Ok(Command::CloseTab { id: 3 })
        );
        assert_eq!(
            parse(r#"{"cmd":"navigate","text":"example.org"}"#),
            Ok(Command::Navigate {
                text: "example.org".into()
            })
        );
        assert!(parse(r#"{"cmd":"rm-rf"}"#).is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn address_bar_rules() {
        assert_eq!(input_to_url("example.org"), "https://example.org");
        assert_eq!(
            input_to_url("  example.org/a?b=1 "),
            "https://example.org/a?b=1"
        );
        assert_eq!(input_to_url("localhost:8080"), "https://localhost:8080");
        assert_eq!(input_to_url("192.168.1.1"), "https://192.168.1.1");
        assert_eq!(input_to_url("http://x.y"), "http://x.y");
        assert_eq!(input_to_url("kb://settings"), "kb://settings");
        assert_eq!(input_to_url("about:blank"), "about:blank");
        assert_eq!(input_to_url(""), "kb://newtab/");
        assert_eq!(
            input_to_url("rust borrow checker"),
            format!("{SEARCH_URL}rust+borrow+checker")
        );
        assert_eq!(input_to_url("a&b"), format!("{SEARCH_URL}a%26b"));
        assert_eq!(input_to_url("word"), format!("{SEARCH_URL}word"));
        assert_eq!(input_to_url(".com"), format!("{SEARCH_URL}.com"));
    }

    #[test]
    fn trusted_origin() {
        assert!(is_trusted_origin("kb://ui/"));
        assert!(is_trusted_origin("KB://ui/"));
        assert!(!is_trusted_origin("https://kb.example/"));
        assert!(!is_trusted_origin("kb:"));
    }

    #[test]
    fn state_json_is_camel_case() {
        let s = State {
            tabs: vec![TabState {
                id: 1,
                can_go_back: true,
                ..Default::default()
            }],
        };
        assert!(s.to_json().contains("\"canGoBack\":true"));
    }
}

//! Content blocking (architecture §4.2, task 2.2): adblock-rust engine shared across threads
//! (CEF asks on the IO thread), fed with filter lists from the profile's `filters/` folder.
//! List download/update (2.3), cosmetic filtering + scriptlets (2.3) and per-site shields (2.4)
//! build on this.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub mod lists;

use adblock::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;

/// Resource types in the vocabulary adblock-rust understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Document,
    SubFrame,
    Stylesheet,
    Script,
    Image,
    Font,
    Media,
    Xhr,
    Ping,
    WebSocket,
    Other,
}

impl ResourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::SubFrame => "sub_frame",
            Self::Stylesheet => "stylesheet",
            Self::Script => "script",
            Self::Image => "image",
            Self::Font => "font",
            Self::Media => "media",
            Self::Xhr => "xmlhttprequest",
            Self::Ping => "ping",
            Self::WebSocket => "websocket",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cosmetic {
    pub css: String,
    pub script: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Decision {
    pub block: bool,
    /// Rewritten URL with tracking parameters removed (`$removeparam`).
    pub rewritten_url: Option<String>,
}

/// Thread-safe blocker. An empty blocker allows everything.
pub struct Blocker {
    engine: RwLock<Option<Engine>>,
}

impl Default for Blocker {
    fn default() -> Self {
        Self::new()
    }
}

impl Blocker {
    pub const fn new() -> Self {
        Self {
            engine: RwLock::new(None),
        }
    }

    /// Replaces the rules with the given list texts. Returns the number of lists loaded.
    pub fn load_lists<I, S>(&self, lists: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut set = FilterSet::new(false);
        let mut n = 0;
        for text in lists {
            set.add_filter_list(text.into(), ParseOptions::default());
            n += 1;
        }
        let engine = Engine::new_with_filter_set(set);
        if let Ok(mut e) = self.engine.write() {
            *e = Some(engine);
        }
        n
    }

    /// Loads every `*.txt` file in `dir` (missing folder = no rules).
    pub fn load_dir(&self, dir: &Path) -> std::io::Result<usize> {
        let mut texts = Vec::new();
        if dir.is_dir() {
            let mut paths: Vec<_> = std::fs::read_dir(dir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "txt"))
                .collect();
            paths.sort();
            for p in paths {
                texts.push(std::fs::read_to_string(p)?);
            }
        }
        Ok(self.load_lists(texts))
    }

    /// Loads exactly these files (missing ones are skipped).
    pub fn load_files(&self, files: &[PathBuf]) -> usize {
        let texts: Vec<String> = files
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .collect();
        self.load_lists(texts)
    }

    /// CSS selectors to hide on `url` (page-specific cosmetic filters), joined into one
    /// stylesheet, and the scriptlet injection script (empty without uBO resources).
    pub fn cosmetic(&self, url: &str) -> Cosmetic {
        let Ok(guard) = self.engine.read() else {
            return Cosmetic::default();
        };
        let Some(engine) = guard.as_ref() else {
            return Cosmetic::default();
        };
        let r = engine.url_cosmetic_resources(url);
        let mut selectors: Vec<String> = r
            .hide_selectors
            .into_iter()
            .filter(|s| !r.exceptions.contains(s))
            .collect();
        selectors.sort();
        let css = if selectors.is_empty() {
            String::new()
        } else {
            format!(
                "{} {{ display: none !important; }}",
                selectors.join(
                    ",
"
                )
            )
        };
        Cosmetic {
            css,
            script: r.injected_script,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.engine.read().is_ok_and(|e| e.is_some())
    }

    /// Checks one request. `source_url` is the URL of the page (frame) that made it.
    pub fn check(&self, url: &str, source_url: &str, kind: ResourceType) -> Decision {
        let Ok(guard) = self.engine.read() else {
            return Decision::default();
        };
        let Some(engine) = guard.as_ref() else {
            return Decision::default();
        };
        let Ok(request) = Request::new(url, source_url, kind.as_str(), "GET") else {
            return Decision::default();
        };
        let result = engine.check_network_request(&request);
        Decision {
            block: result.should_block(),
            rewritten_url: result.rewritten_url,
        }
    }
}

/// `true` for schemes that adblock rules apply to.
pub fn is_filterable(url: &str) -> bool {
    let l = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    l.starts_with("http://")
        || l.starts_with("https://")
        || l.starts_with("ws://")
        || l.starts_with("wss://")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = "\
||tracker.example^
||ads.example^$script
/banner/*$image
@@||ads.example/allowed.js
||cdn.example/annoying.js$third-party
";

    fn blocker() -> Blocker {
        let b = Blocker::new();
        assert_eq!(b.load_lists([RULES]), 1);
        b
    }

    #[test]
    fn empty_blocker_allows_everything() {
        let b = Blocker::new();
        assert!(!b.is_loaded());
        assert!(
            !b.check(
                "https://tracker.example/p.gif",
                "https://site.org/",
                ResourceType::Image
            )
            .block
        );
    }

    #[test]
    fn blocks_by_domain_type_and_path() {
        let b = blocker();
        let page = "https://news.org/article";
        assert!(
            b.check(
                "https://tracker.example/pixel.gif",
                page,
                ResourceType::Image
            )
            .block
        );
        assert!(
            b.check("https://ads.example/x.js", page, ResourceType::Script)
                .block
        );
        assert!(
            !b.check("https://ads.example/x.png", page, ResourceType::Image)
                .block
        );
        assert!(
            b.check("https://img.org/banner/1.png", page, ResourceType::Image)
                .block
        );
        assert!(
            !b.check("https://news.org/style.css", page, ResourceType::Stylesheet)
                .block
        );
    }

    #[test]
    fn exceptions_and_third_party() {
        let b = blocker();
        assert!(
            !b.check(
                "https://ads.example/allowed.js",
                "https://news.org/",
                ResourceType::Script
            )
            .block
        );
        assert!(
            b.check(
                "https://cdn.example/annoying.js",
                "https://news.org/",
                ResourceType::Script
            )
            .block
        );
        assert!(
            !b.check(
                "https://cdn.example/annoying.js",
                "https://cdn.example/",
                ResourceType::Script
            )
            .block
        );
    }

    /// adblock-rust also matches top-level documents (uBO "strict blocking"). The shell never
    /// asks about main-frame navigations, so a typed address always loads (decision in 2.2).
    #[test]
    fn documents_match_domain_rules() {
        let b = blocker();
        assert!(
            b.check("https://tracker.example/", "", ResourceType::Document)
                .block
        );
        assert!(
            !b.check("https://news.org/", "", ResourceType::Document)
                .block
        );
    }

    #[test]
    fn loads_txt_files_from_dir() {
        let dir = std::env::temp_dir().join(format!("kb-filters-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "||a.example^").unwrap();
        std::fs::write(dir.join("ignored.json"), "{}").unwrap();
        let b = Blocker::new();
        assert_eq!(b.load_dir(&dir).unwrap(), 1);
        assert!(
            b.check(
                "https://a.example/x.js",
                "https://s.org/",
                ResourceType::Script
            )
            .block
        );
        assert_eq!(Blocker::new().load_dir(&dir.join("missing")).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cosmetic_selectors_for_site() {
        let b = Blocker::new();
        b.load_lists(["news.org##.cookie-banner
news.org##.ad-box
news.org#@#.ad-box
"]);
        let c = b.cosmetic("https://news.org/a");
        assert!(c.css.contains(".cookie-banner"));
        assert!(!c.css.contains(".ad-box"));
        assert!(b.cosmetic("https://other.org/").css.is_empty());
    }

    #[test]
    fn filterable_schemes() {
        assert!(is_filterable("https://x"));
        assert!(is_filterable("WSS://x"));
        assert!(!is_filterable("kb://ui/"));
        assert!(!is_filterable("data:text/plain,1"));
    }
}

//! Filter list catalog, on-disk state and updates (architecture §4.2, task 2.3).
//!
//! Lists live in `<profile>/filters/<id>.txt`; `lists.json` stores which are enabled and when
//! each was fetched. Lists are downloaded at runtime (never bundled: EasyList & co. are
//! GPL / CC-BY-SA). Updating is the documented network exception of §4.1 and can be disabled.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub url: &'static str,
    pub default_enabled: bool,
}

/// Built-in catalog (§4.2). Users can add custom URLs later.
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "easylist",
        title: "EasyList (ads)",
        url: "https://easylist.to/easylist/easylist.txt",
        default_enabled: true,
    },
    CatalogEntry {
        id: "easyprivacy",
        title: "EasyPrivacy (trackers)",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        default_enabled: true,
    },
    CatalogEntry {
        id: "ublock-filters",
        title: "uBlock filters",
        url: "https://ublockorigin.github.io/uAssets/filters/filters.txt",
        default_enabled: true,
    },
    CatalogEntry {
        id: "ublock-privacy",
        title: "uBlock filters – Privacy",
        url: "https://ublockorigin.github.io/uAssets/filters/privacy.txt",
        default_enabled: true,
    },
    CatalogEntry {
        id: "peter-lowe",
        title: "Peter Lowe's ad and tracking server list",
        url: "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&showintro=0&mimetype=plaintext",
        default_enabled: true,
    },
    CatalogEntry {
        id: "turkish",
        title: "Turkish regional list",
        url: "https://raw.githubusercontent.com/easylist/easylist-turkish/master/easylist-turkish.txt",
        default_enabled: false,
    },
];

/// Hosts contacted by list updates (allowlist of the zero-connections check).
pub fn update_hosts() -> Vec<String> {
    let mut hosts: Vec<String> = CATALOG
        .iter()
        .filter_map(|e| e.url.split('/').nth(2).map(str::to_string))
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

/// Lists are refreshed when older than this.
pub const UPDATE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListState {
    pub id: String,
    pub enabled: bool,
    /// Unix seconds of the last successful download (0 = never).
    pub updated: i64,
    pub rules: usize,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListsFile {
    pub lists: Vec<ListState>,
}

impl ListsFile {
    /// Catalog merged with stored state (new catalog entries get their defaults).
    pub fn merged(mut self) -> Self {
        for e in CATALOG {
            if !self.lists.iter().any(|l| l.id == e.id) {
                self.lists.push(ListState {
                    id: e.id.into(),
                    enabled: e.default_enabled,
                    ..Default::default()
                });
            }
        }
        self.lists.retain(|l| CATALOG.iter().any(|e| e.id == l.id));
        self
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("lists.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_default()
            .merged()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("lists.json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, dir.join("lists.json"))
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        match self.lists.iter_mut().find(|l| l.id == id) {
            Some(l) => {
                l.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// Enabled lists that are missing or older than `UPDATE_INTERVAL` at `now`.
    pub fn due(&self, now: i64, force: bool) -> Vec<String> {
        self.lists
            .iter()
            .filter(|l| l.enabled && (force || now - l.updated >= UPDATE_INTERVAL.as_secs() as i64))
            .map(|l| l.id.clone())
            .collect()
    }
}

pub fn list_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.txt"))
}

/// Files the blocker should load: enabled lists that exist on disk, plus `custom*.txt`.
pub fn enabled_files(dir: &Path, state: &ListsFile) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = state
        .lists
        .iter()
        .filter(|l| l.enabled)
        .map(|l| list_path(dir, &l.id))
        .filter(|p| p.exists())
        .collect();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut custom: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("custom") && n.ends_with(".txt"))
            })
            .collect();
        custom.sort();
        files.extend(custom);
    }
    files
}

/// Rough rule count (non-empty, non-comment lines).
pub fn count_rules(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('!') && !l.starts_with('['))
        .count()
}

/// Sanity check before replacing a list on disk: reject HTML error pages and empty bodies.
pub fn looks_like_filter_list(text: &str) -> bool {
    let head = text
        .trim_start()
        .get(..200)
        .unwrap_or(text.trim_start())
        .to_ascii_lowercase();
    !head.starts_with("<!doctype") && !head.starts_with("<html") && count_rules(text) > 0
}

/// Downloads one list (blocking). Size-capped at 20 MB.
pub fn download(url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent("Kendime-Brow filter updater")
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let text = resp
        .body_mut()
        .with_config()
        .limit(20 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    if !looks_like_filter_list(&text) {
        return Err("response is not a filter list".into());
    }
    Ok(text)
}

/// Updates the given lists in `dir` using `fetch` (injectable for tests). Returns true if any
/// list changed on disk.
pub fn update_lists(
    dir: &Path,
    state: &mut ListsFile,
    ids: &[String],
    now: i64,
    fetch: &dyn Fn(&str) -> Result<String, String>,
) -> bool {
    let _ = std::fs::create_dir_all(dir);
    let mut changed = false;
    for id in ids {
        let Some(entry) = CATALOG.iter().find(|e| e.id == id) else {
            continue;
        };
        let Some(st) = state.lists.iter_mut().find(|l| l.id == *id) else {
            continue;
        };
        match fetch(entry.url) {
            Ok(text) => {
                let path = list_path(dir, id);
                let tmp = path.with_extension("txt.tmp");
                if std::fs::write(&tmp, &text)
                    .and_then(|_| std::fs::rename(&tmp, &path))
                    .is_ok()
                {
                    st.updated = now;
                    st.rules = count_rules(&text);
                    st.error.clear();
                    changed = true;
                }
            }
            Err(e) => st.error = e,
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kb-lists-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn catalog_is_consistent() {
        let mut ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        assert!(CATALOG.iter().all(|e| e.url.starts_with("https://")));
        assert!(update_hosts().contains(&"easylist.to".to_string()));
    }

    #[test]
    fn merge_defaults_and_drop_unknown() {
        let f = ListsFile {
            lists: vec![
                ListState {
                    id: "easylist".into(),
                    enabled: false,
                    ..Default::default()
                },
                ListState {
                    id: "gone".into(),
                    enabled: true,
                    ..Default::default()
                },
            ],
        }
        .merged();
        assert_eq!(f.lists.len(), CATALOG.len());
        assert!(!f.lists.iter().find(|l| l.id == "easylist").unwrap().enabled);
        assert!(
            f.lists
                .iter()
                .find(|l| l.id == "easyprivacy")
                .unwrap()
                .enabled
        );
        assert!(!f.lists.iter().find(|l| l.id == "turkish").unwrap().enabled);
    }

    #[test]
    fn due_respects_interval_and_enabled() {
        let mut f = ListsFile::default().merged();
        let now = 10 * 86_400;
        let enabled = f.lists.iter().filter(|l| l.enabled).count();
        assert_eq!(f.due(now, false).len(), enabled);
        for l in &mut f.lists {
            l.updated = now - 3600;
        }
        assert!(f.due(now, false).is_empty());
        assert_eq!(f.due(now, true).len(), enabled);
    }

    #[test]
    fn filter_list_sanity() {
        assert!(looks_like_filter_list("! Title: x\n||ads.example^\n"));
        assert!(!looks_like_filter_list("<!DOCTYPE html><html>error</html>"));
        assert!(!looks_like_filter_list("! only comments\n"));
        assert_eq!(count_rules("! c\n[Adblock Plus 2.0]\n\n||a^\n##.ad\n"), 2);
    }

    #[test]
    fn update_writes_files_and_state() {
        let dir = tmp("update");
        let mut f = ListsFile::default().merged();
        let ids = vec!["easylist".to_string(), "easyprivacy".to_string()];
        let fetch = |url: &str| {
            if url.contains("easyprivacy") {
                Err("boom".to_string())
            } else {
                Ok("||ads.example^\n".to_string())
            }
        };
        assert!(update_lists(&dir, &mut f, &ids, 1000, &fetch));
        let el = f.lists.iter().find(|l| l.id == "easylist").unwrap();
        assert_eq!((el.updated, el.rules), (1000, 1));
        assert_eq!(
            f.lists
                .iter()
                .find(|l| l.id == "easyprivacy")
                .unwrap()
                .error,
            "boom"
        );
        std::fs::write(dir.join("custom-mine.txt"), "||x^").unwrap();
        let files = enabled_files(&dir, &f);
        assert_eq!(files.len(), 2);
        f.save(&dir).unwrap();
        assert_eq!(ListsFile::load(&dir), f);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

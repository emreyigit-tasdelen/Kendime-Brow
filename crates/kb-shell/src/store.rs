//! Profile database `kb.sqlite` (architecture §3.7, task 1.12): settings, bookmarks, history
//! and the last session. Pure SQLite, no CEF types (tested with an in-memory database).

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS bookmarks (
    id      INTEGER PRIMARY KEY,
    url     TEXT NOT NULL UNIQUE,
    title   TEXT NOT NULL,
    created INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS history (
    url         TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    last_visit  INTEGER NOT NULL,
    visit_count INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS history_last_visit ON history(last_visit DESC);
CREATE TABLE IF NOT EXISTS shield_off (
    host TEXT PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS session (
    position INTEGER PRIMARY KEY,
    url      TEXT NOT NULL,
    active   INTEGER NOT NULL
);
";

/// Known settings with their defaults. Unknown keys are rejected by `set_setting`.
pub const SETTINGS: &[(&str, &str)] = &[
    ("search_url", crate::protocol::SEARCH_URL),
    ("restore_session", "true"),
    ("history_enabled", "true"),
    ("filters_auto_update", "true"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bookmark {
    pub id: i64,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub last_visit: i64,
    pub visit_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTab {
    pub url: String,
    pub active: bool,
    pub container: String,
}

pub struct Store {
    db: Connection,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Pages that are never written to history or the session.
pub fn is_internal(url: &str) -> bool {
    url.is_empty() || url.starts_with("kb://") || url.starts_with("about:")
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> rusqlite::Result<Self> {
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.execute_batch(SCHEMA)?;
        // v1 → v2: session tabs remember their container.
        let has_container: bool = db
            .prepare("SELECT 1 FROM pragma_table_info('session') WHERE name = 'container'")?
            .exists([])?;
        if !has_container {
            db.execute(
                "ALTER TABLE session ADD COLUMN container TEXT NOT NULL DEFAULT 'personal'",
                [],
            )?;
        }
        db.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { db })
    }

    // ------------------------------------------------------------ settings

    pub fn setting(&self, key: &str) -> Option<String> {
        let stored = self
            .db
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();
        stored.or_else(|| {
            SETTINGS
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        })
    }

    pub fn setting_bool(&self, key: &str) -> bool {
        self.setting(key).as_deref() == Some("true")
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        if !SETTINGS.iter().any(|(k, _)| *k == key) {
            return Err(format!("unknown setting `{key}`"));
        }
        self.db
            .execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// All known settings with their effective values.
    pub fn settings(&self) -> Vec<(String, String)> {
        SETTINGS
            .iter()
            .map(|(k, _)| (k.to_string(), self.setting(k).unwrap_or_default()))
            .collect()
    }

    // ------------------------------------------------------------ bookmarks

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.db
            .query_row("SELECT 1 FROM bookmarks WHERE url = ?1", [url], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    /// Adds the bookmark, or removes it if it exists. Returns true if it is now bookmarked.
    pub fn toggle_bookmark(&self, url: &str, title: &str) -> rusqlite::Result<bool> {
        if self
            .db
            .execute("DELETE FROM bookmarks WHERE url = ?1", [url])?
            > 0
        {
            return Ok(false);
        }
        self.db.execute(
            "INSERT INTO bookmarks(url, title, created) VALUES (?1, ?2, ?3)",
            params![url, title, now()],
        )?;
        Ok(true)
    }

    pub fn delete_bookmark(&self, id: i64) -> rusqlite::Result<()> {
        self.db
            .execute("DELETE FROM bookmarks WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn bookmarks(&self) -> rusqlite::Result<Vec<Bookmark>> {
        let mut st = self
            .db
            .prepare("SELECT id, url, title FROM bookmarks ORDER BY created DESC, id DESC")?;
        st.query_map([], |r| {
            Ok(Bookmark {
                id: r.get(0)?,
                url: r.get(1)?,
                title: r.get(2)?,
            })
        })?
        .collect()
    }

    // ------------------------------------------------------------ history

    pub fn record_visit(&self, url: &str, title: &str) -> rusqlite::Result<()> {
        if is_internal(url) || !self.setting_bool("history_enabled") {
            return Ok(());
        }
        self.db.execute(
            "INSERT INTO history(url, title, last_visit, visit_count) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(url) DO UPDATE SET
                 title = CASE WHEN excluded.title = '' THEN history.title ELSE excluded.title END,
                 last_visit = excluded.last_visit,
                 visit_count = history.visit_count + 1",
            params![url, title, now()],
        )?;
        Ok(())
    }

    /// Updates the title of an already recorded visit (titles arrive after the URL).
    pub fn update_title(&self, url: &str, title: &str) -> rusqlite::Result<()> {
        if is_internal(url) || title.is_empty() {
            return Ok(());
        }
        self.db.execute(
            "UPDATE history SET title = ?2 WHERE url = ?1",
            params![url, title],
        )?;
        Ok(())
    }

    pub fn history(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<HistoryEntry>> {
        let like = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut st = self.db.prepare(
            "SELECT url, title, last_visit, visit_count FROM history
             WHERE url LIKE ?1 ESCAPE '\\' OR title LIKE ?1 ESCAPE '\\'
             ORDER BY last_visit DESC LIMIT ?2",
        )?;
        st.query_map(params![like, limit as i64], |r| {
            Ok(HistoryEntry {
                url: r.get(0)?,
                title: r.get(1)?,
                last_visit: r.get(2)?,
                visit_count: r.get(3)?,
            })
        })?
        .collect()
    }

    pub fn clear_history(&self) -> rusqlite::Result<()> {
        self.db.execute("DELETE FROM history", [])?;
        Ok(())
    }

    // ------------------------------------------------------------ shields

    /// Hosts where content blocking is switched off.
    pub fn shield_off_hosts(&self) -> rusqlite::Result<Vec<String>> {
        let mut st = self
            .db
            .prepare("SELECT host FROM shield_off ORDER BY host")?;
        st.query_map([], |r| r.get(0))?.collect()
    }

    pub fn set_shield(&self, host: &str, enabled: bool) -> rusqlite::Result<()> {
        if enabled {
            self.db
                .execute("DELETE FROM shield_off WHERE host = ?1", [host])?;
        } else {
            self.db
                .execute("INSERT OR IGNORE INTO shield_off(host) VALUES (?1)", [host])?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ session

    pub fn save_session(&mut self, tabs: &[SessionTab]) -> rusqlite::Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM session", [])?;
        for (i, t) in tabs.iter().filter(|t| !is_internal(&t.url)).enumerate() {
            tx.execute(
                "INSERT INTO session(position, url, active, container) VALUES (?1, ?2, ?3, ?4)",
                params![i as i64, t.url, t.active, t.container],
            )?;
        }
        tx.commit()
    }

    pub fn load_session(&self) -> rusqlite::Result<Vec<SessionTab>> {
        let mut st = self
            .db
            .prepare("SELECT url, active, container FROM session ORDER BY position")?;
        st.query_map([], |r| {
            Ok(SessionTab {
                url: r.get(0)?,
                active: r.get(1)?,
                container: r.get(2)?,
            })
        })?
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_and_validation() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.setting_bool("restore_session"));
        assert!(s.setting("search_url").unwrap().starts_with("https://"));
        s.set_setting("restore_session", "false").unwrap();
        assert!(!s.setting_bool("restore_session"));
        assert!(s.set_setting("evil", "1").is_err());
        assert_eq!(s.settings().len(), SETTINGS.len());
    }

    #[test]
    fn bookmarks_toggle() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.toggle_bookmark("https://a.org/", "A").unwrap());
        assert!(s.is_bookmarked("https://a.org/"));
        assert_eq!(s.bookmarks().unwrap()[0].title, "A");
        assert!(!s.toggle_bookmark("https://a.org/", "A").unwrap());
        assert!(s.bookmarks().unwrap().is_empty());
        s.toggle_bookmark("https://b.org/", "B").unwrap();
        let id = s.bookmarks().unwrap()[0].id;
        s.delete_bookmark(id).unwrap();
        assert!(!s.is_bookmarked("https://b.org/"));
    }

    #[test]
    fn history_counts_titles_and_search() {
        let s = Store::open_in_memory().unwrap();
        s.record_visit("https://a.org/", "").unwrap();
        s.update_title("https://a.org/", "Alpha").unwrap();
        s.record_visit("https://a.org/", "").unwrap();
        s.record_visit("kb://newtab/", "New tab").unwrap();
        s.record_visit("https://b.org/100%", "B").unwrap();
        let all = s.history("", 10).unwrap();
        assert_eq!(all.len(), 2);
        let a = all.iter().find(|h| h.url == "https://a.org/").unwrap();
        assert_eq!((a.title.as_str(), a.visit_count), ("Alpha", 2));
        assert_eq!(s.history("alpha", 10).unwrap().len(), 1);
        assert_eq!(s.history("%", 10).unwrap().len(), 1);
        s.clear_history().unwrap();
        assert!(s.history("", 10).unwrap().is_empty());
    }

    #[test]
    fn history_can_be_disabled() {
        let s = Store::open_in_memory().unwrap();
        s.set_setting("history_enabled", "false").unwrap();
        s.record_visit("https://a.org/", "A").unwrap();
        assert!(s.history("", 10).unwrap().is_empty());
    }

    #[test]
    fn session_round_trip_skips_internal_pages() {
        let mut s = Store::open_in_memory().unwrap();
        let tabs = vec![
            SessionTab {
                url: "https://a.org/".into(),
                active: false,
                container: "personal".into(),
            },
            SessionTab {
                url: "kb://newtab/".into(),
                active: false,
                container: "personal".into(),
            },
            SessionTab {
                url: "https://b.org/".into(),
                active: true,
                container: "personal".into(),
            },
        ];
        s.save_session(&tabs).unwrap();
        let back = s.load_session().unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(
            back[1],
            SessionTab {
                url: "https://b.org/".into(),
                active: true,
                container: "personal".into()
            }
        );
        s.save_session(&[]).unwrap();
        assert!(s.load_session().unwrap().is_empty());
    }

    #[test]
    fn shield_exceptions() {
        let s = Store::open_in_memory().unwrap();
        s.set_shield("news.org", false).unwrap();
        s.set_shield("news.org", false).unwrap();
        assert_eq!(s.shield_off_hosts().unwrap(), vec!["news.org".to_string()]);
        s.set_shield("news.org", true).unwrap();
        assert!(s.shield_off_hosts().unwrap().is_empty());
    }

    #[test]
    fn file_database_persists() {
        let dir = std::env::temp_dir().join(format!("kb-store-test-{}", std::process::id()));
        let path = dir.join("kb.sqlite");
        {
            let s = Store::open(&path).unwrap();
            s.toggle_bookmark("https://a.org/", "A").unwrap();
        }
        assert!(Store::open(&path).unwrap().is_bookmarked("https://a.org/"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

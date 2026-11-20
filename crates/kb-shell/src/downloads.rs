//! Basic download manager (architecture §8.3, task 1.13): saves to the downloads folder
//! without a dialog, tracks progress and exposes cancel / pause / resume / reveal.

use std::cell::RefCell;
use std::path::PathBuf;

use cef::*;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownloadStatus {
    InProgress,
    Paused,
    Complete,
    Canceled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadState {
    pub id: u32,
    pub url: String,
    pub file_name: String,
    pub path: String,
    pub received: i64,
    pub total: i64,
    pub percent: i32,
    pub speed: i64,
    pub status: DownloadStatus,
    /// SHA-256 of the finished file (hex); empty until computed.
    pub sha256: String,
}

struct Entry {
    state: DownloadState,
    callback: Option<DownloadItemCallback>,
}

thread_local! {
    static DOWNLOADS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// Checksums of finished downloads (computed on a background thread); `None` while running.
static SHA: std::sync::Mutex<Option<std::collections::HashMap<u32, Option<String>>>> =
    std::sync::Mutex::new(None);

fn start_hash(id: u32, path: String) -> bool {
    {
        let mut g = SHA.lock().unwrap_or_else(|e| e.into_inner());
        let m = g.get_or_insert_with(Default::default);
        if m.contains_key(&id) {
            return false;
        }
        m.insert(id, None);
    }
    std::thread::spawn(move || {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let hex = std::fs::File::open(&path)
            .ok()
            .and_then(|mut f| {
                let mut h = Sha256::new();
                let mut buf = vec![0u8; 1 << 20];
                loop {
                    match f.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => h.update(&buf[..n]),
                        Err(_) => return None,
                    }
                }
                Some(
                    h.finalize()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>(),
                )
            })
            .unwrap_or_else(|| "unavailable".into());
        let mut g = SHA.lock().unwrap_or_else(|e| e.into_inner());
        g.get_or_insert_with(Default::default).insert(id, Some(hex));
        drop(g);
        let mut task = crate::shell::RefreshTask::new();
        post_task(ThreadId::UI, Some(&mut task));
    });
    true
}

pub fn list() -> Vec<DownloadState> {
    let sha = SHA.lock().unwrap_or_else(|e| e.into_inner());
    DOWNLOADS.with(|d| {
        d.borrow()
            .iter()
            .map(|e| {
                let mut s = e.state.clone();
                if let Some(Some(h)) = sha.as_ref().and_then(|m| m.get(&s.id)) {
                    s.sha256 = h.clone();
                }
                s
            })
            .collect()
    })
}

/// Picks the target path and starts the download without a dialog.
pub fn on_before_download(
    item: &mut DownloadItem,
    suggested_name: &str,
    callback: &mut BeforeDownloadCallback,
) -> bool {
    let setting = crate::shell::setting;
    let custom = setting("download_dir");
    let Some(dir) = (!custom.trim().is_empty())
        .then(|| PathBuf::from(custom.trim()))
        .or_else(kb_platform::files::downloads_dir)
    else {
        return false; // let CEF fall back to its default behaviour
    };
    let name = kb_platform::files::sanitize_file_name(suggested_name);
    let url = CefString::from(&item.url()).to_string();
    let risky = kb_platform::files::is_executable_type(std::path::Path::new(&name))
        || kb_platform::files::has_disguised_extension(&name);
    let policy = setting("download_dangerous");
    let blocked = (risky && policy == "block")
        || (setting("download_block_http") == "true" && url.starts_with("http://"));
    if blocked {
        // Not continuing the callback cancels the download; keep a visible record of it.
        record(item, &name, &dir.join(&name), DownloadStatus::Canceled);
        return true;
    }
    // Optional <profile>/download-rules.txt ("pdf,docx = Documents") wins over the built-in
    // by-type sorting.
    let sub = crate::shell::profile_dir()
        .and_then(|p| std::fs::read_to_string(p.join("download-rules.txt")).ok())
        .and_then(|rules| kb_platform::files::rule_subdir(&rules, &name))
        .or_else(|| {
            (setting("download_sort") == "true")
                .then(|| kb_platform::files::type_subdir(&name).map(String::from))
                .flatten()
        });
    let dir = sub.map_or(dir.clone(), |sub| dir.join(sub));
    let _ = std::fs::create_dir_all(&dir);
    // Ask where to save: always when chosen, and for risky files under the "ask" policy.
    let show_dialog = setting("download_ask") == "true" || (risky && policy == "ask");
    let path = kb_platform::files::unique_path(&dir, &name);
    let path_str = path.to_string_lossy().to_string();
    record(item, &name, &path, DownloadStatus::InProgress);
    callback.cont(
        Some(&CefString::from(path_str.as_str())),
        show_dialog as i32,
    );
    true
}

fn record(item: &mut DownloadItem, name: &str, path: &std::path::Path, status: DownloadStatus) {
    DOWNLOADS.with(|d| {
        d.borrow_mut().push(Entry {
            state: DownloadState {
                id: item.id(),
                url: CefString::from(&item.url()).to_string(),
                file_name: path
                    .file_name()
                    .map_or_else(|| name.to_string(), |n| n.to_string_lossy().to_string()),
                path: path.to_string_lossy().to_string(),
                received: 0,
                total: item.total_bytes(),
                percent: 0,
                speed: 0,
                status,
                sha256: String::new(),
            },
            callback: None,
        })
    });
}

/// Windows "Mark of the Web": tags the file as coming from the internet so SmartScreen and
/// Office Protected View treat it with suspicion.
#[cfg(windows)]
fn mark_of_the_web(path: &str, url: &str) {
    let host = url.split('/').nth(2).unwrap_or_default();
    let ads = format!("{path}:Zone.Identifier");
    let _ = std::fs::write(
        ads,
        format!("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl={url}\r\nReferrerUrl=https://{host}/\r\n"),
    );
}
#[cfg(not(windows))]
fn mark_of_the_web(_path: &str, _url: &str) {}

/// Progress update. Returns true if something changed.
pub fn on_download_updated(item: &mut DownloadItem, callback: Option<&mut DownloadItemCallback>) {
    let status = if item.is_complete() != 0 {
        DownloadStatus::Complete
    } else if item.is_canceled() != 0 {
        DownloadStatus::Canceled
    } else if item.is_interrupted() != 0 {
        DownloadStatus::Interrupted
    } else if item.is_paused() != 0 {
        DownloadStatus::Paused
    } else {
        DownloadStatus::InProgress
    };
    let full = CefString::from(&item.full_path()).to_string();
    if status == DownloadStatus::Complete && !full.is_empty() && start_hash(item.id(), full.clone())
    {
        {
            // First completion report for this download.
            if crate::shell::setting("download_motw") != "false" {
                mark_of_the_web(&full, &CefString::from(&item.url()).to_string());
            }
            let p = std::path::Path::new(&full);
            if crate::shell::setting("download_open_done") == "true"
                && !kb_platform::files::is_executable_type(p)
            {
                let _ = kb_platform::files::open_default(p);
            }
        }
    }
    DOWNLOADS.with(|d| {
        let mut d = d.borrow_mut();
        let Some(e) = d.iter_mut().find(|e| e.state.id == item.id()) else {
            return;
        };
        e.state.received = item.received_bytes();
        e.state.total = item.total_bytes();
        e.state.percent = item.percent_complete();
        e.state.speed = item.current_speed();
        e.state.status = status;
        if !full.is_empty() {
            e.state.path = full;
        }
        if let Some(cb) = callback {
            e.callback = Some(cb.clone());
        }
        if matches!(
            status,
            DownloadStatus::Complete | DownloadStatus::Canceled | DownloadStatus::Interrupted
        ) {
            e.callback = None;
        }
    });
}

fn callback_of(id: u32) -> Option<DownloadItemCallback> {
    DOWNLOADS.with(|d| {
        d.borrow()
            .iter()
            .find(|e| e.state.id == id)
            .and_then(|e| e.callback.clone())
    })
}

pub fn cancel(id: u32) {
    if let Some(cb) = callback_of(id) {
        cb.cancel();
    }
}

pub fn pause(id: u32) {
    if let Some(cb) = callback_of(id) {
        cb.pause();
    }
}

pub fn resume(id: u32) {
    if let Some(cb) = callback_of(id) {
        cb.resume();
    }
}

/// Opens a finished download with the default application (not for executable types).
pub fn open(id: u32) {
    let path = DOWNLOADS.with(|d| {
        d.borrow()
            .iter()
            .find(|e| e.state.id == id && e.state.status == DownloadStatus::Complete)
            .map(|e| PathBuf::from(&e.state.path))
    });
    if let Some(path) = path {
        let _ = kb_platform::files::open_default(&path);
    }
}

pub fn reveal(id: u32) {
    let path = DOWNLOADS.with(|d| {
        d.borrow()
            .iter()
            .find(|e| e.state.id == id)
            .map(|e| PathBuf::from(&e.state.path))
    });
    if let Some(path) = path {
        let _ = kb_platform::files::reveal(&path);
    }
}

/// Removes finished entries from the list (files stay on disk).
pub fn clear_finished() {
    DOWNLOADS.with(|d| {
        d.borrow_mut().retain(|e| {
            matches!(
                e.state.status,
                DownloadStatus::InProgress | DownloadStatus::Paused
            )
        })
    });
}

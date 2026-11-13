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

fn start_hash(id: u32, path: String) {
    {
        let mut g = SHA.lock().unwrap_or_else(|e| e.into_inner());
        let m = g.get_or_insert_with(Default::default);
        if m.contains_key(&id) {
            return;
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
    let Some(dir) = kb_platform::files::downloads_dir() else {
        return false; // let CEF fall back to its default behaviour
    };
    let _ = std::fs::create_dir_all(&dir);
    let name = kb_platform::files::sanitize_file_name(suggested_name);
    let path = kb_platform::files::unique_path(&dir, &name);
    let path_str = path.to_string_lossy().to_string();
    DOWNLOADS.with(|d| {
        d.borrow_mut().push(Entry {
            state: DownloadState {
                id: item.id(),
                url: CefString::from(&item.url()).to_string(),
                file_name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(name),
                path: path_str.clone(),
                received: 0,
                total: item.total_bytes(),
                percent: 0,
                speed: 0,
                status: DownloadStatus::InProgress,
                sha256: String::new(),
            },
            callback: None,
        })
    });
    callback.cont(Some(&CefString::from(path_str.as_str())), 0);
    true
}

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
    if status == DownloadStatus::Complete && !full.is_empty() {
        start_hash(item.id(), full.clone());
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

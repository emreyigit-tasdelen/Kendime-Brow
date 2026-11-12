//! Remote file sessions for `kb://files` (task 4A.5): SFTP sessions kept by id, operations
//! answered as JSON. Every call blocks, so the bridge runs them on a worker thread.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine;
use kb_ssh::sftp::Sftp;
use serde_json::{Value, json};

use crate::protocol::SshRequest;

/// Largest upload accepted through the page (it travels base64-encoded over the bridge).
const MAX_UPLOAD: usize = 64 * 1024 * 1024;

static SESSIONS: OnceLock<Mutex<HashMap<u32, Arc<Sftp>>>> = OnceLock::new();
static NEXT: AtomicU32 = AtomicU32::new(1);

fn sessions() -> &'static Mutex<HashMap<u32, Arc<Sftp>>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session(args: &Value) -> Result<Arc<Sftp>, String> {
    let id = args
        .get("sid")
        .and_then(Value::as_u64)
        .ok_or("sid required")? as u32;
    sessions()
        .lock()
        .map_err(|_| "poisoned")?
        .get(&id)
        .cloned()
        .ok_or_else(|| "session closed".into())
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Where downloads go, never overwriting an existing file.
fn download_target(name: &str) -> Result<PathBuf, String> {
    let safe: String = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_control() || ":*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if safe.is_empty() || safe == "." || safe == ".." {
        return Err("invalid file name".into());
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or("no home directory")?;
    let dir = PathBuf::from(home).join("Downloads");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (stem, ext) = match safe.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (safe.clone(), String::new()),
    };
    let mut path = dir.join(&safe);
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    Ok(path)
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    match op {
        "connect" => {
            let req: SshRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let t = crate::terminal::resolve_ssh(&req)?;
            let sftp = Sftp::connect(kb_ssh::Config {
                host: t.host,
                port: t.port,
                user: t.user,
                auth: t.auth,
                known_hosts: t.known_hosts,
                trust_new_host: t.trust_new_host,
                cols: 80,
                rows: 24,
            })
            .map_err(|e| match e {
                kb_ssh::SshError::UnknownHost { fingerprint } => {
                    format!("unknown-host:{fingerprint}")
                }
                kb_ssh::SshError::HostKeyChanged { expected, got } => {
                    format!("host-key-changed:{expected} -> {got}")
                }
                other => other.to_string(),
            })?;
            let home = sftp.realpath(".").map_err(|e| e.to_string())?;
            let sid = NEXT.fetch_add(1, Ordering::Relaxed);
            sessions()
                .lock()
                .map_err(|_| "poisoned")?
                .insert(sid, Arc::new(sftp));
            Ok(json!({ "sid": sid, "home": home }).to_string())
        }
        "close" => {
            if let Some(id) = args.get("sid").and_then(Value::as_u64) {
                sessions()
                    .lock()
                    .map_err(|_| "poisoned")?
                    .remove(&(id as u32));
            }
            Ok("{}".into())
        }
        "list" => {
            let s = session(args)?;
            let path = text(args, "path");
            let entries = s.list(path).map_err(|e| e.to_string())?;
            let items: Vec<Value> = entries
                .iter()
                .map(|e| json!({ "name": e.name, "dir": e.is_dir, "size": e.size, "modified": e.modified }))
                .collect();
            Ok(json!({ "path": path, "entries": items }).to_string())
        }
        "download" => {
            let s = session(args)?;
            let remote = text(args, "path");
            let local = download_target(remote)?;
            let n = s.download(remote, &local).map_err(|e| e.to_string())?;
            Ok(json!({ "saved": local.display().to_string(), "bytes": n }).to_string())
        }
        "upload" => {
            let s = session(args)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(text(args, "data"))
                .map_err(|e| e.to_string())?;
            if data.len() > MAX_UPLOAD {
                return Err("file too large (64 MB limit through the page)".into());
            }
            let remote = text(args, "path");
            // Staged in a temp file so the kb-ssh API stays path based.
            let tmp = std::env::temp_dir().join(format!("kb-upload-{}", std::process::id()));
            std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;
            let r = s.upload(&tmp, remote);
            let _ = std::fs::remove_file(&tmp);
            r.map(|n| json!({ "bytes": n }).to_string())
                .map_err(|e| e.to_string())
        }
        "mkdir" => {
            session(args)?
                .mkdir(text(args, "path"))
                .map_err(|e| e.to_string())?;
            Ok("{}".into())
        }
        "remove" => {
            let s = session(args)?;
            let path = text(args, "path");
            if args.get("dir").and_then(Value::as_bool).unwrap_or(false) {
                s.remove_dir(path)
            } else {
                s.remove_file(path)
            }
            .map_err(|e| e.to_string())?;
            Ok("{}".into())
        }
        "rename" => {
            session(args)?
                .rename(text(args, "from"), text(args, "to"))
                .map_err(|e| e.to_string())?;
            Ok("{}".into())
        }
        _ => Err(format!("unknown files op: {op}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_names_are_sanitized() {
        assert!(download_target("../..").is_err());
        assert!(download_target("").is_err());
        let p = download_target("a/b/c:d.txt").unwrap();
        assert_eq!(
            p.file_name().unwrap().to_string_lossy().replace(" (1)", ""),
            "c_d.txt"
        );
        assert_eq!(p.parent().unwrap().file_name().unwrap(), "Downloads");
    }
}

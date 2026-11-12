//! Saved connection profiles for `kb://connections` (task 4A.4). Stored as JSON in the profile
//! directory; no secrets live here (a profile only points at a vault entry).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conn {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    /// Vault entry holding the password or private key.
    #[serde(default)]
    pub vault_id: String,
}

fn default_port() -> u16 {
    22
}

static LOCK: Mutex<()> = Mutex::new(());

fn file() -> Result<PathBuf, String> {
    crate::shell::profile_dir()
        .map(|d| d.join("connections.json"))
        .ok_or_else(|| "no profile directory".into())
}

fn load(path: &Path) -> Vec<Conn> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(path: &Path, list: &[Conn]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn new_id() -> String {
    let mut b = [0u8; 6];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Parses the `Host` blocks of an OpenSSH client config. Wildcard patterns are skipped;
/// `HostName`, `User` and `Port` are honoured, everything else is ignored.
pub fn parse_ssh_config(text: &str) -> Vec<Conn> {
    let mut out: Vec<Conn> = Vec::new();
    let mut current: Option<Conn> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (
                k.to_ascii_lowercase(),
                v.trim().trim_start_matches('=').trim(),
            ),
            None => continue,
        };
        match key.as_str() {
            "host" => {
                out.extend(current.take());
                let alias = value.split_whitespace().next().unwrap_or_default();
                if !alias.is_empty() && !alias.contains(['*', '?', '!']) {
                    current = Some(Conn {
                        id: String::new(),
                        name: alias.to_string(),
                        host: alias.to_string(),
                        port: 22,
                        user: String::new(),
                        vault_id: String::new(),
                    });
                }
            }
            "hostname" => {
                if let Some(c) = current.as_mut() {
                    c.host = value.to_string();
                }
            }
            "user" => {
                if let Some(c) = current.as_mut() {
                    c.user = value.to_string();
                }
            }
            "port" => {
                if let (Some(c), Ok(p)) = (current.as_mut(), value.parse()) {
                    c.port = p;
                }
            }
            _ => {}
        }
    }
    out.extend(current);
    out
}

fn ssh_config_path() -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    Some(PathBuf::from(home).join(".ssh").join("config"))
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let path = file()?;
    let mut list = load(&path);
    match op {
        "list" => {}
        "put" => {
            let host = text(args, "host").trim();
            if host.is_empty() {
                return Err("host is required".into());
            }
            let port = text(args, "port").parse().unwrap_or(22);
            let id = match text(args, "id") {
                "" => new_id(),
                i => i.to_string(),
            };
            let conn = Conn {
                id: id.clone(),
                name: match text(args, "name").trim() {
                    "" => host.to_string(),
                    n => n.to_string(),
                },
                host: host.to_string(),
                port,
                user: text(args, "user").trim().to_string(),
                vault_id: text(args, "vault_id").to_string(),
            };
            match list.iter_mut().find(|c| c.id == id) {
                Some(slot) => *slot = conn,
                None => list.push(conn),
            }
            save(&path, &list)?;
        }
        "remove" => {
            list.retain(|c| c.id != text(args, "id"));
            save(&path, &list)?;
        }
        "import-ssh-config" => {
            let cfg = ssh_config_path().ok_or("no home directory")?;
            let text = std::fs::read_to_string(&cfg)
                .map_err(|_| format!("cannot read {}", cfg.display()))?;
            let mut added = 0;
            for mut c in parse_ssh_config(&text) {
                if list.iter().any(|x| x.name == c.name && x.host == c.host) {
                    continue;
                }
                c.id = new_id();
                list.push(c);
                added += 1;
            }
            save(&path, &list)?;
            return Ok(json!({ "added": added, "connections": list }).to_string());
        }
        _ => return Err(format!("unknown connections op: {op}")),
    }
    serde_json::to_string(&list).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_config_blocks() {
        let cfg = "\
# comment
Host *
    ServerAliveInterval 30

Host web prod-web
    HostName 203.0.113.7
    User deploy
    Port 2200
    IdentityFile ~/.ssh/id_ed25519

Host db
    HostName=db.internal.example
Host !skip
    HostName nope
";
        let list = parse_ssh_config(cfg);
        assert_eq!(list.len(), 2, "{list:?}");
        assert_eq!(list[0].name, "web");
        assert_eq!(
            (list[0].host.as_str(), list[0].user.as_str(), list[0].port),
            ("203.0.113.7", "deploy", 2200)
        );
        assert_eq!(list[1].host, "db.internal.example");
        assert_eq!(list[1].port, 22);
    }
}

//! Credential vault service behind `kb://vault` (task 4A.1b). Wraps `kb-vault` (Argon2id +
//! XChaCha20-Poly1305) in a process-wide handle with an idle auto-lock.
//!
//! Known gap: the vault still runs in the shell process. Moving it into a sandboxed host
//! (like `kb-doc-host`) is tracked in PROGRESS.md; secrets are zeroized on lock/drop meanwhile.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use kb_vault::{KdfParams, Kind, Secret, Vault, VaultError};
use serde_json::{Value, json};

/// Locks itself after this much time without a vault command.
const IDLE_LOCK: Duration = Duration::from_secs(5 * 60);

struct State {
    vault: Option<Vault>,
    last_use: Option<Instant>,
    watcher: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    vault: None,
    last_use: None,
    watcher: false,
});

fn idle(s: &State) -> bool {
    s.last_use.is_some_and(|t| t.elapsed() > IDLE_LOCK)
}

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

fn path() -> Result<std::path::PathBuf, String> {
    crate::shell::profile_dir()
        .map(|d| d.join("vault.kbv"))
        .ok_or_else(|| "no profile directory".to_string())
}

fn err(e: VaultError) -> String {
    e.to_string()
}

fn kind_of(s: &str) -> Result<Kind, String> {
    match s {
        "login" => Ok(Kind::Login),
        "ssh-key" => Ok(Kind::SshKey),
        "note" => Ok(Kind::Note),
        "token" => Ok(Kind::Token),
        _ => Err(format!("unknown kind: {s}")),
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Login => "login",
        Kind::SshKey => "ssh-key",
        Kind::Note => "note",
        Kind::Token => "token",
    }
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn lock_if_idle(s: &mut State) {
    if !idle(s) {
        return;
    }
    if let Some(v) = s.vault.as_mut() {
        v.lock();
    }
}

/// Applies the idle lock lazily and starts the watcher thread once.
fn touch(s: &mut State) {
    lock_if_idle(s);
    s.last_use = Some(Instant::now());
    if !s.watcher {
        s.watcher = true;
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(15));
                let mut s = state();
                lock_if_idle(&mut s);
            }
        });
    }
}

fn load(s: &mut State) -> Result<(), String> {
    if s.vault.is_none() {
        let p = path()?;
        if p.exists() {
            s.vault = Some(Vault::open(&p).map_err(err)?);
        }
    }
    Ok(())
}

fn status(s: &State) -> Value {
    json!({
        "exists": s.vault.is_some(),
        "unlocked": s.vault.as_ref().is_some_and(Vault::is_unlocked),
        "count": s.vault.as_ref().map_or(0, Vault::len),
        "idleLockSecs": IDLE_LOCK.as_secs(),
    })
}

fn new_id() -> String {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// One decrypted entry for in-process use (SSH login). Fails while the vault is locked.
pub fn secret(id: &str) -> Result<(Kind, Secret), String> {
    let mut s = state();
    touch(&mut s);
    load(&mut s)?;
    s.vault.as_ref().ok_or("no vault yet")?.get(id).map_err(err)
}

/// 32-byte key for encrypted session logs, created on first use and stored in the vault.
/// Needs an existing, unlocked vault.
pub fn log_key() -> Result<[u8; 32], String> {
    const ID: &str = "kb-session-log-key";
    let mut s = state();
    touch(&mut s);
    load(&mut s)?;
    let v = s
        .vault
        .as_mut()
        .ok_or("no vault yet: create and unlock the vault first")?;
    let hex = match v.get(ID) {
        Ok((_, sec)) => sec.password.clone(),
        Err(_) => {
            let mut k = [0u8; 32];
            getrandom::fill(&mut k).map_err(|e| e.to_string())?;
            let hex: String = k.iter().map(|b| format!("{b:02x}")).collect();
            let mut sec = Secret::default();
            sec.title = "Session log key (do not delete)".into();
            sec.password = hex.clone();
            v.put(ID, Kind::Note, &sec).map_err(err)?;
            v.save(&path()?).map_err(err)?;
            hex
        }
    };
    if hex.len() != 64 {
        return Err("session log key is damaged".into());
    }
    let mut key = [0u8; 32];
    for (i, b) in key.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(key)
}

/// Runs one vault operation; `args` is the JSON object sent by the page.
pub fn call(op: &str, args: &Value) -> Result<String, String> {
    let mut s = state();
    touch(&mut s);
    load(&mut s)?;
    match op {
        "status" => {}
        "create" => {
            if s.vault.is_some() {
                return Err("a vault already exists".into());
            }
            let pw = text(args, "password");
            if pw.chars().count() < 8 {
                return Err("master password must have at least 8 characters".into());
            }
            drop(s);
            // Argon2 (256 MiB) takes a moment; do not hold the lock meanwhile.
            let v = Vault::create(pw, KdfParams::STRONG).map_err(err)?;
            let mut s = state();
            v.save(&path()?).map_err(err)?;
            s.vault = Some(v);
            return Ok(status(&s).to_string());
        }
        "unlock" => {
            let pw = text(args, "password").to_string();
            let json = s.vault.as_ref().map(Vault::to_json).ok_or("no vault yet")?;
            drop(s);
            let mut v = Vault::from_json(&json).map_err(err)?;
            v.unlock(&pw).map_err(err)?;
            let mut s = state();
            s.vault = Some(v);
            s.last_use = Some(Instant::now());
            return Ok(status(&s).to_string());
        }
        "lock" => {
            if let Some(v) = s.vault.as_mut() {
                v.lock();
            }
        }
        "list" => {
            let v = s.vault.as_ref().ok_or("no vault yet")?;
            let items: Vec<Value> = v
                .list()
                .map_err(err)?
                .into_iter()
                .map(|(id, k, title)| json!({ "id": id, "kind": kind_name(k), "title": title }))
                .collect();
            return Ok(Value::Array(items).to_string());
        }
        "import-csv" => {
            let logins = kb_vault::csv::import_logins(text(args, "text"))?;
            let v = s.vault.as_mut().ok_or("no vault yet")?;
            let n = logins.len();
            for sec in &logins {
                v.put(&new_id(), Kind::Login, sec).map_err(err)?;
            }
            v.save(&path()?).map_err(err)?;
            return Ok(json!({ "imported": n }).to_string());
        }
        "export-csv" => {
            let v = s.vault.as_ref().ok_or("no vault yet")?;
            let mut logins = Vec::new();
            for (id, kind, _) in v.list().map_err(err)? {
                if kind == Kind::Login && id != "kb-session-log-key" {
                    logins.push(v.get(&id).map_err(err)?.1);
                }
            }
            return Ok(
                json!({ "csv": kb_vault::csv::export_logins(&logins), "count": logins.len() })
                    .to_string(),
            );
        }
        "get" => {
            let v = s.vault.as_ref().ok_or("no vault yet")?;
            let (k, sec) = v.get(text(args, "id")).map_err(err)?;
            return Ok(json!({
                "id": text(args, "id"), "kind": kind_name(k), "title": sec.title,
                "username": sec.username, "password": sec.password, "url": sec.url,
                "notes": sec.notes,
            })
            .to_string());
        }
        "put" => {
            let id = match text(args, "id") {
                "" => new_id(),
                i => i.to_string(),
            };
            let kind = kind_of(text(args, "kind"))?;
            let sec = Secret {
                title: text(args, "title").into(),
                username: text(args, "username").into(),
                password: text(args, "password").into(),
                url: text(args, "url").into(),
                notes: text(args, "notes").into(),
            };
            if sec.title.trim().is_empty() {
                return Err("title is required".into());
            }
            let v = s.vault.as_mut().ok_or("no vault yet")?;
            v.put(&id, kind, &sec).map_err(err)?;
            v.save(&path()?).map_err(err)?;
            return Ok(json!({ "id": id }).to_string());
        }
        "remove" => {
            let v = s.vault.as_mut().ok_or("no vault yet")?;
            v.remove(text(args, "id")).map_err(err)?;
            v.save(&path()?).map_err(err)?;
        }
        _ => return Err(format!("unknown vault op: {op}")),
    }
    Ok(status(&s).to_string())
}

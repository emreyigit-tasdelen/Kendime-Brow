//! Encrypted terminal session logs (task 4A.9): while a log is active, everything typed and
//! everything received on that terminal is sealed with `kb_vault::seclog` and appended to
//! `<profile>/session-logs/*.kbl`. The key lives in the vault, so the vault must be unlocked to
//! start a log or read one.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;

use kb_vault::seclog::{LogWriter, open_all};
use serde_json::{Value, json};

struct Active {
    writer: LogWriter,
    file: std::fs::File,
}

static LOGS: Mutex<Option<HashMap<u32, Active>>> = Mutex::new(None);

fn dir() -> Result<std::path::PathBuf, String> {
    let d = crate::shell::profile_dir()
        .ok_or("no profile directory")?
        .join("session-logs");
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

/// Records one chunk (`b'i'` = typed, `b'o'` = received). A no-op unless a log is active.
pub fn record(id: u32, direction: u8, data: &[u8]) {
    let Ok(mut g) = LOGS.lock() else { return };
    let Some(a) = g.as_mut().and_then(|m| m.get_mut(&id)) else {
        return;
    };
    let mut entry = Vec::with_capacity(data.len() + 1);
    entry.push(direction);
    entry.extend(data);
    if let Ok(bytes) = a.writer.seal(&entry) {
        let _ = a.file.write_all(&bytes);
    }
}

fn safe_name(s: &str) -> String {
    let n: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    if n.is_empty() { "terminal".into() } else { n }
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    match op {
        "start" => {
            let key = crate::vault::log_key()?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let label = safe_name(
                args.get("label")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            let name = format!("{stamp}-{label}.kbl");
            let file = std::fs::File::create(dir()?.join(&name)).map_err(|e| e.to_string())?;
            LOGS.lock()
                .map_err(|_| "poisoned")?
                .get_or_insert_with(HashMap::new)
                .insert(
                    id,
                    Active {
                        writer: LogWriter::new(&key, 0),
                        file,
                    },
                );
            Ok(json!({ "name": name }).to_string())
        }
        "stop" => {
            if let Some(m) = LOGS.lock().map_err(|_| "poisoned")?.as_mut() {
                m.remove(&id);
            }
            Ok("{}".into())
        }
        "list" => {
            let mut names: Vec<(String, u64)> = std::fs::read_dir(dir()?)
                .map_err(|e| e.to_string())?
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".kbl"))
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        e.metadata().map_or(0, |m| m.len()),
                    )
                })
                .collect();
            names.sort_by(|a, b| b.0.cmp(&a.0));
            Ok(Value::Array(
                names
                    .into_iter()
                    .map(|(n, s)| json!({ "name": n, "bytes": s }))
                    .collect(),
            )
            .to_string())
        }
        "read" => {
            let name = args.get("name").and_then(Value::as_str).unwrap_or_default();
            if name.contains(['/', '\\']) || !name.ends_with(".kbl") {
                return Err("invalid log name".into());
            }
            let key = crate::vault::log_key()?;
            let data = std::fs::read(dir()?.join(name)).map_err(|e| e.to_string())?;
            let entries = open_all(&key, &data).map_err(|e| e.to_string())?;
            let out: Vec<Value> = entries
                .iter()
                .filter(|e| !e.is_empty())
                .map(|e| {
                    json!({
                        "d": if e[0] == b'i' { "in" } else { "out" },
                        "t": String::from_utf8_lossy(&e[1..]),
                    })
                })
                .collect();
            Ok(Value::Array(out).to_string())
        }
        _ => Err(format!("unknown session-log op: {op}")),
    }
}

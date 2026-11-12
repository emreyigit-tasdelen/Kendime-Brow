//! Active SSH tunnels for `kb://connections` (task 4A.6). Tunnels live until stopped or until the
//! browser exits; they only ever listen on 127.0.0.1.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use kb_ssh::tunnel::{Tunnel, open_local, open_socks};
use serde_json::{Value, json};

use crate::protocol::SshRequest;

struct Active {
    label: String,
    port: u16,
    /// Held only so dropping the entry stops the tunnel.
    _tunnel: Tunnel,
}

static ACTIVE: OnceLock<Mutex<BTreeMap<u32, Active>>> = OnceLock::new();
static NEXT: AtomicU32 = AtomicU32::new(1);

fn active() -> &'static Mutex<BTreeMap<u32, Active>> {
    ACTIVE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn list_json() -> Result<String, String> {
    let a = active().lock().map_err(|_| "poisoned")?;
    let items: Vec<Value> = a
        .iter()
        .map(|(id, t)| json!({ "id": id, "label": t.label, "port": t.port }))
        .collect();
    Ok(Value::Array(items).to_string())
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    match op {
        "list" => list_json(),
        "stop" => {
            let id = args
                .get("id")
                .and_then(Value::as_u64)
                .ok_or("id required")? as u32;
            // Dropping the tunnel stops the listener.
            active().lock().map_err(|_| "poisoned")?.remove(&id);
            list_json()
        }
        "start" => {
            let req: SshRequest =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            let cfg = crate::terminal::ssh_config(&req)?;
            let bind = args
                .get("bind_port")
                .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .unwrap_or(0) as u16;
            let mode = args.get("mode").and_then(Value::as_str).unwrap_or("local");
            let (tunnel, label) = if mode == "socks" {
                let t = open_socks(cfg, bind).map_err(crate::terminal::ssh_err)?;
                let label = format!("SOCKS5 via {}", req.host);
                (t, label)
            } else {
                let host = args
                    .get("remote_host")
                    .and_then(Value::as_str)
                    .filter(|h| !h.is_empty())
                    .ok_or("remote host required")?
                    .to_string();
                let port = args
                    .get("remote_port")
                    .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                    .filter(|p| (1..=65535).contains(p))
                    .ok_or("remote port required")? as u16;
                let t =
                    open_local(cfg, bind, host.clone(), port).map_err(crate::terminal::ssh_err)?;
                let label = format!("{host}:{port} via {}", req.host);
                (t, label)
            };
            let port = tunnel.local_port;
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            active().lock().map_err(|_| "poisoned")?.insert(
                id,
                Active {
                    label,
                    port,
                    _tunnel: tunnel,
                },
            );
            list_json()
        }
        _ => Err(format!("unknown tunnels op: {op}")),
    }
}

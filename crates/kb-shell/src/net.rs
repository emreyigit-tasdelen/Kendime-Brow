//! Network inspector backend (task 4B.1): a DevTools message observer on every browser feeds
//! CDP `Network.*` events into a shared [`kb_net::Log`]; `kb://net` reads it through `call`.

use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};

use cef::*;
use serde_json::{Value, json};

/// Rows kept in memory; older ones are dropped.
const MAX_ROWS: usize = 3000;

static LOG: OnceLock<Mutex<kb_net::Log>> = OnceLock::new();

fn log() -> &'static Mutex<kb_net::Log> {
    LOG.get_or_init(|| Mutex::new(kb_net::Log::default()))
}

thread_local! {
    /// Registrations must stay alive or CEF stops delivering events.
    static REGISTRATIONS: RefCell<Vec<Registration>> = const { RefCell::new(Vec::new()) };
}

wrap_dev_tools_message_observer! {
    struct KbNetObserver;

    impl DevToolsMessageObserver {
        fn on_dev_tools_event(
            &self,
            _browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            let (Some(method), Some(params)) = (method, params) else { return };
            let method = method.to_string();
            if !method.starts_with("Network.") {
                return;
            }
            if let Ok(v) = serde_json::from_slice::<Value>(params)
                && let Ok(mut l) = log().lock()
            {
                l.ingest(&method, &v);
                l.trim(MAX_ROWS);
            }
        }
    }
}

/// Starts recording this browser's network traffic. Call once per browser, on the UI thread.
pub fn attach(browser: &mut Browser) {
    let Some(host) = browser.host() else { return };
    let mut observer = KbNetObserver::new();
    if let Some(reg) = host.add_dev_tools_message_observer(Some(&mut observer)) {
        REGISTRATIONS.with(|r| r.borrow_mut().push(reg));
    }
    host.execute_dev_tools_method(0, Some(&CefString::from("Network.enable")), None);
}

/// Answers `kb://net` operations: list, har, curl, clear.
pub fn call(op: &str, args: &Value) -> Result<String, String> {
    let mut l = log().lock().map_err(|_| "poisoned")?;
    match op {
        "list" => {
            let filter = kb_net::filter::Filter::parse(
                args.get("filter")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )?;
            let rows: Vec<Value> = l
                .requests()
                .iter()
                .filter(|r| !r.url.starts_with("kb://") && !r.url.starts_with("devtools://"))
                .filter(|r| filter.matches(r))
                .map(|r| {
                    json!({
                        "id": r.id, "url": r.url, "method": r.method, "status": r.status,
                        "mime": r.mime_type, "ms": r.duration_ms(), "bytes": r.encoded_bytes,
                        "error": r.error, "type": r.resource_type, "protocol": r.protocol,
                        "ip": r.remote_ip,
                    })
                })
                .collect();
            Ok(Value::Array(rows).to_string())
        }
        "har" => Ok(l.to_har().to_string()),
        "curl" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
            l.requests()
                .iter()
                .rev()
                .find(|r| r.id == id)
                .map(|r| json!({ "curl": r.to_curl() }).to_string())
                .ok_or_else(|| "request not found".into())
        }
        "clear" => {
            l.clear();
            Ok("{}".into())
        }
        _ => Err(format!("unknown net op: {op}")),
    }
}

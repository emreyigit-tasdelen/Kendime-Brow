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
/// Sends an edited copy of a captured request (task 4B.4) and returns status, headers and the
/// first 256 KB of the body. It is a fresh request from the shell: no cookies, no cache, not
/// logged in the inspector, and only http(s) URLs are allowed.
fn resend(args: &Value) -> Result<String, String> {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or_default();
    let url = s("url");
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("only http(s) URLs can be sent".into());
    }
    let method = s("method").to_ascii_uppercase();
    let mut b = ureq::http::Request::builder()
        .method(method.as_str())
        .uri(url);
    for h in args
        .get("headers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(k), Some(v)) = (
            h.get(0).and_then(Value::as_str),
            h.get(1).and_then(Value::as_str),
        ) {
            // Hop-by-hop / framing headers are set by the HTTP client.
            if !["host", "content-length", "connection", "cookie"]
                .contains(&k.to_ascii_lowercase().as_str())
            {
                b = b.header(k, v);
            }
        }
    }
    let req = b
        .body(s("body").as_bytes().to_vec())
        .map_err(|e| e.to_string())?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent.run(req).map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let headers: Vec<Value> = resp
        .headers()
        .iter()
        .map(|(k, v)| json!([k.as_str(), String::from_utf8_lossy(v.as_bytes())]))
        .collect();
    let mut buf = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(resp.body_mut().as_reader(), 256 * 1024),
        &mut buf,
    )
    .map_err(|e| e.to_string())?;
    Ok(
        json!({ "status": status, "headers": headers, "body": String::from_utf8_lossy(&buf) })
            .to_string(),
    )
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    if op == "resend" {
        return resend(args);
    }
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
        "har" => {
            // Internal pages are not part of what the user browsed.
            let mut visible = kb_net::Log::default();
            for r in l.requests() {
                if !r.url.starts_with("kb://") && !r.url.starts_with("devtools://") {
                    visible.push(r.clone());
                }
            }
            Ok(visible.to_har().to_string())
        }
        "curl" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
            l.requests()
                .iter()
                .rev()
                .find(|r| r.id == id)
                .map(|r| json!({ "curl": r.to_curl() }).to_string())
                .ok_or_else(|| "request not found".into())
        }
        "get" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
            l.requests()
                .iter()
                .rev()
                .find(|r| r.id == id)
                .map(|r| {
                    json!({ "method": r.method, "url": r.url,
                            "headers": r.request_headers, "body": r.post_data })
                    .to_string()
                })
                .ok_or_else(|| "request not found".into())
        }
        "clear" => {
            l.clear();
            Ok("{}".into())
        }
        _ => Err(format!("unknown net op: {op}")),
    }
}

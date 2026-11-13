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

/// NetLog (task 4B.2) is opt-in: a marker file in the profile makes the next start pass
/// `--log-net-log` (sensitive data such as cookies stays stripped, Chromium's default capture mode).
pub fn netlog_marker() -> Option<std::path::PathBuf> {
    crate::shell::profile_dir().map(|d| d.join("netlog.on"))
}

/// Marker file: present = start with software rendering (low memory mode, task 2.11).
pub fn lowmem_marker() -> Option<std::path::PathBuf> {
    crate::shell::profile_dir().map(|d| d.join("lowmem.on"))
}

pub fn netlog_path() -> Option<std::path::PathBuf> {
    crate::shell::profile_dir().map(|d| d.join("netlog.json"))
}

fn netlog(op: &str, args: &Value) -> Result<String, String> {
    let marker = netlog_marker().ok_or("no profile directory")?;
    if op == "netlog-set" {
        if args.get("on").and_then(Value::as_bool).unwrap_or(false) {
            std::fs::write(&marker, b"").map_err(|e| e.to_string())?;
        } else {
            let _ = std::fs::remove_file(&marker);
        }
    }
    Ok(
        json!({ "on": marker.exists(), "path": netlog_path().map(|p| p.display().to_string()) })
            .to_string(),
    )
}

/// Opens a pcapng capture (base64 in `data`) and returns dissected packets, reassembled TCP
/// streams (text preview) and statistics (task 4B.7).
fn pcap_open(args: &Value) -> Result<String, String> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(args.get("data").and_then(Value::as_str).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    let raw = kb_net::pcapng::read_packets(&data)?;
    let packets: Vec<kb_net::dissect::Packet> = raw
        .iter()
        .filter_map(|(_, frame)| kb_net::dissect::dissect(frame))
        .collect();
    let rows: Vec<Value> = packets
        .iter()
        .take(5000)
        .enumerate()
        .map(|(i, p)| {
            json!({ "n": i + 1, "src": p.src, "dst": p.dst, "proto": p.proto, "sport": p.sport,
                    "dport": p.dport, "len": p.payload.len(), "summary": p.summary })
        })
        .collect();
    let streams: Vec<Value> = kb_net::dissect::reassemble(&packets)
        .into_iter()
        .take(200)
        .map(|(k, v)| {
            let end = v.len().min(4000);
            json!({ "key": k, "bytes": v.len(), "text": String::from_utf8_lossy(&v[..end]) })
        })
        .collect();
    let s = kb_net::dissect::stats(&packets);
    Ok(json!({ "packets": rows, "streams": streams,
               "stats": { "packets": s.packets, "bytes": s.bytes, "by_proto": s.by_proto } })
    .to_string())
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    if op == "resend" {
        return resend(args);
    }
    if op == "pcap-open" {
        return pcap_open(args);
    }
    if op.starts_with("netlog-") {
        return netlog(op, args);
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
        "stats" => {
            // Per-host and per-type request counts, bytes and errors (task 4B.7, statistics).
            let mut hosts: std::collections::BTreeMap<String, (u64, u64, u64)> = Default::default();
            let mut types: std::collections::BTreeMap<String, (u64, u64)> = Default::default();
            for r in l.requests() {
                if r.url.starts_with("kb://") || r.url.starts_with("devtools://") {
                    continue;
                }
                let host = r
                    .url
                    .split_once("://")
                    .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default())
                    .unwrap_or_default()
                    .rsplit('@')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let bytes = r.encoded_bytes.unwrap_or(0);
                let h = hosts.entry(host).or_default();
                h.0 += 1;
                h.1 += bytes;
                h.2 += u64::from(r.error.is_some() || r.status.is_some_and(|s| s >= 400));
                let t = types.entry(r.resource_type.clone()).or_default();
                t.0 += 1;
                t.1 += bytes;
            }
            let mut hv: Vec<_> = hosts.into_iter().collect();
            hv.sort_by_key(|(_, v)| std::cmp::Reverse(v.1));
            Ok(json!({
                "hosts": hv.iter().take(30).map(|(h, v)| json!({"host": h, "requests": v.0, "bytes": v.1, "failed": v.2})).collect::<Vec<_>>(),
                "types": types.iter().map(|(t, v)| json!({"type": t, "requests": v.0, "bytes": v.1})).collect::<Vec<_>>(),
            })
            .to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn opens_a_capture_and_reassembles_the_stream() {
        let payload = b"GET /x HTTP/1.1
Host: a.test

";
        let mut f = vec![0u8; 12];
        f.extend([0x08, 0x00]);
        let mut ip = vec![
            0x45, 0, 0, 0, 0, 0, 0, 0, 64, 6, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2,
        ];
        let mut tcp = vec![0u8; 20];
        tcp[0..2].copy_from_slice(&5000u16.to_be_bytes());
        tcp[2..4].copy_from_slice(&80u16.to_be_bytes());
        tcp[12] = 0x50;
        ip.extend(&tcp);
        ip.extend(payload);
        f.extend(ip);
        let mut w = kb_net::pcapng::Writer::new(1);
        w.packet(1, &f);
        let b64 = base64::engine::general_purpose::STANDARD.encode(w.finish());
        let out: Value =
            serde_json::from_str(&pcap_open(&json!({ "data": b64 })).unwrap()).unwrap();
        assert_eq!(out["stats"]["packets"], 1);
        assert_eq!(out["packets"][0]["proto"], "TCP");
        assert!(
            out["streams"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("GET /x")
        );
        assert!(pcap_open(&json!({ "data": "!!" })).is_err());
    }
}

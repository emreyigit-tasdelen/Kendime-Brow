//! Request log built from Chrome DevTools Protocol `Network.*` events (task 4B.1, 4B.4).
//!
//! Feed every event with [`Log::ingest`]; read rows with [`Log::requests`]. Export with
//! [`Log::to_har`] and [`Request::to_curl`]. The CEF wiring that delivers the events lives in
//! kb-shell; this crate is pure so it can be tested without a browser.

pub mod dissect;
pub mod filter;
pub mod pcapng;

use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Request {
    pub id: String,
    pub url: String,
    pub method: String,
    pub request_headers: Vec<(String, String)>,
    pub post_data: Option<String>,
    pub status: Option<u16>,
    pub status_text: String,
    pub mime_type: String,
    pub response_headers: Vec<(String, String)>,
    pub remote_ip: Option<String>,
    pub protocol: Option<String>,
    /// Seconds (CDP monotonic clock) at which the request started.
    pub started: f64,
    /// Wall-clock start, seconds since the Unix epoch (CDP `wallTime`).
    pub wall_time: f64,
    pub finished: Option<f64>,
    pub encoded_bytes: Option<u64>,
    pub error: Option<String>,
    pub resource_type: String,
}

impl Request {
    pub fn duration_ms(&self) -> Option<f64> {
        self.finished
            .map(|f| ((f - self.started) * 1000.0).max(0.0))
    }

    /// A shell command that replays the request. Single quotes are escaped for POSIX shells.
    pub fn to_curl(&self) -> String {
        let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let mut parts = vec![format!("curl {}", q(&self.url))];
        if self.method != "GET" {
            parts.push(format!("-X {}", self.method));
        }
        for (k, v) in &self.request_headers {
            // curl sets these itself; copying them breaks HTTP/2 and chunking.
            if k.starts_with(':') || k.eq_ignore_ascii_case("content-length") {
                continue;
            }
            parts.push(format!("-H {}", q(&format!("{k}: {v}"))));
        }
        if let Some(body) = &self.post_data {
            parts.push(format!("--data-raw {}", q(body)));
        }
        parts.join(" \\\n  ")
    }
}

#[derive(Debug, Default)]
pub struct Log {
    rows: Vec<Request>,
}

/// UTC ISO 8601 with milliseconds, from Unix seconds (civil-from-days, no date crate needed).
fn iso8601(secs: f64) -> String {
    let ms = (secs * 1000.0).round() as i64;
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

fn headers(v: &Value) -> Vec<(String, String)> {
    v.as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl Log {
    pub fn requests(&self) -> &[Request] {
        &self.rows
    }

    /// Drops the oldest rows beyond `max`.
    pub fn trim(&mut self, max: usize) {
        if self.rows.len() > max {
            let drop = self.rows.len() - max;
            self.rows.drain(..drop);
        }
    }

    pub fn push(&mut self, r: Request) {
        self.rows.push(r);
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }

    fn find(&mut self, id: &str) -> Option<&mut Request> {
        self.rows.iter_mut().rev().find(|r| r.id == id)
    }

    /// Applies one CDP event (`method` + `params`). Unknown events are ignored.
    pub fn ingest(&mut self, method: &str, p: &Value) {
        let id = text(p, "requestId");
        match method {
            "Network.requestWillBeSent" => {
                let req = &p["request"];
                // A redirect re-uses the id: the earlier hop is completed by its response.
                if let Some(prev) = self.find(&id)
                    && let Some(resp) = p.get("redirectResponse")
                {
                    prev.status = resp["status"].as_u64().map(|s| s as u16);
                    prev.finished = p["timestamp"].as_f64();
                }
                self.rows.push(Request {
                    id,
                    url: text(req, "url"),
                    method: text(req, "method"),
                    request_headers: headers(&req["headers"]),
                    post_data: req
                        .get("postData")
                        .and_then(Value::as_str)
                        .map(String::from),
                    started: p["timestamp"].as_f64().unwrap_or(0.0),
                    wall_time: p["wallTime"].as_f64().unwrap_or(0.0),
                    resource_type: text(p, "type"),
                    ..Request::default()
                });
            }
            "Network.responseReceived" => {
                if let Some(r) = self.find(&id) {
                    let resp = &p["response"];
                    r.status = resp["status"].as_u64().map(|s| s as u16);
                    r.status_text = text(resp, "statusText");
                    r.mime_type = text(resp, "mimeType");
                    r.response_headers = headers(&resp["headers"]);
                    r.remote_ip = resp
                        .get("remoteIPAddress")
                        .and_then(Value::as_str)
                        .map(String::from);
                    r.protocol = resp
                        .get("protocol")
                        .and_then(Value::as_str)
                        .map(String::from);
                }
            }
            "Network.loadingFinished" => {
                if let Some(r) = self.find(&id) {
                    r.finished = p["timestamp"].as_f64();
                    r.encoded_bytes = p["encodedDataLength"].as_f64().map(|n| n as u64);
                }
            }
            "Network.loadingFailed" => {
                if let Some(r) = self.find(&id) {
                    r.finished = p["timestamp"].as_f64();
                    r.error = Some(text(p, "errorText"));
                }
            }
            _ => {}
        }
    }

    /// HAR 1.2 document for the current rows.
    pub fn to_har(&self) -> Value {
        let pairs = |h: &[(String, String)]| -> Vec<Value> {
            h.iter()
                .map(|(k, v)| json!({ "name": k, "value": v }))
                .collect()
        };
        let entries: Vec<Value> = self
            .rows
            .iter()
            .map(|r| {
                let time = r.duration_ms().unwrap_or(0.0);
                json!({
                    "startedDateTime": iso8601(r.wall_time),
                    "time": time,
                    "request": {
                        "method": r.method, "url": r.url, "httpVersion": r.protocol.clone().unwrap_or_default(),
                        "headers": pairs(&r.request_headers), "queryString": [], "cookies": [],
                        "headersSize": -1, "bodySize": r.post_data.as_ref().map_or(0, |b| b.len() as i64),
                        "postData": r.post_data.as_ref().map(|b| json!({ "mimeType": "", "text": b })),
                    },
                    "response": {
                        "status": r.status.unwrap_or(0), "statusText": r.status_text,
                        "httpVersion": r.protocol.clone().unwrap_or_default(),
                        "headers": pairs(&r.response_headers), "cookies": [],
                        "content": { "size": r.encoded_bytes.unwrap_or(0), "mimeType": r.mime_type },
                        "redirectURL": "", "headersSize": -1, "bodySize": r.encoded_bytes.map_or(-1, |n| n as i64),
                    },
                    "cache": {},
                    "timings": { "send": 0, "wait": time, "receive": 0 },
                    "serverIPAddress": r.remote_ip,
                })
            })
            .collect();
        json!({ "log": { "version": "1.2", "creator": { "name": "Kendime-Brow", "version": "0.0.1" }, "entries": entries } })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Log {
        let mut l = Log::default();
        l.ingest(
            "Network.requestWillBeSent",
            &json!({"requestId":"1","timestamp":10.0,"type":"XHR","request":{
                "url":"https://x.test/a?b=1","method":"POST",
                "headers":{"Content-Type":"application/json","Content-Length":"7"},"postData":"{\"a\":1}"}}),
        );
        l.ingest(
            "Network.responseReceived",
            &json!({"requestId":"1","response":{"status":201,"statusText":"Created","mimeType":"application/json",
                "headers":{"Server":"t"},"remoteIPAddress":"1.2.3.4","protocol":"h2"}}),
        );
        l.ingest(
            "Network.loadingFinished",
            &json!({"requestId":"1","timestamp":10.25,"encodedDataLength":321.0}),
        );
        l
    }

    #[test]
    fn builds_a_row_from_events() {
        let l = sample();
        let r = &l.requests()[0];
        assert_eq!(
            (r.status, r.mime_type.as_str()),
            (Some(201), "application/json")
        );
        assert_eq!(r.encoded_bytes, Some(321));
        assert!((r.duration_ms().unwrap() - 250.0).abs() < 1e-6);
    }

    #[test]
    fn failed_requests_keep_the_error() {
        let mut l = Log::default();
        l.ingest("Network.requestWillBeSent", &json!({"requestId":"9","timestamp":1.0,"request":{"url":"u","method":"GET","headers":{}}}));
        l.ingest(
            "Network.loadingFailed",
            &json!({"requestId":"9","timestamp":2.0,"errorText":"net::ERR_NAME_NOT_RESOLVED"}),
        );
        assert_eq!(
            l.requests()[0].error.as_deref(),
            Some("net::ERR_NAME_NOT_RESOLVED")
        );
    }

    #[test]
    fn redirects_make_a_new_row_and_close_the_old_one() {
        let mut l = Log::default();
        l.ingest("Network.requestWillBeSent", &json!({"requestId":"r","timestamp":1.0,"request":{"url":"http://a","method":"GET","headers":{}}}));
        l.ingest("Network.requestWillBeSent", &json!({"requestId":"r","timestamp":1.5,"request":{"url":"https://a","method":"GET","headers":{}},"redirectResponse":{"status":301}}));
        assert_eq!(l.requests().len(), 2);
        assert_eq!(l.requests()[0].status, Some(301));
        assert_eq!(l.requests()[1].url, "https://a");
    }

    #[test]
    fn curl_escapes_quotes_and_skips_content_length() {
        let mut l = sample();
        l.rows[0].post_data = Some("it's".into());
        let c = l.requests()[0].to_curl();
        assert!(c.contains("-X POST"));
        assert!(c.contains("-H 'Content-Type: application/json'"));
        assert!(!c.contains("Content-Length"));
        assert!(c.contains("--data-raw 'it'\\''s'"));
    }

    #[test]
    fn iso_dates_are_correct() {
        assert_eq!(iso8601(0.0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(1_700_000_000.123), "2023-11-14T22:13:20.123Z");
        assert_eq!(iso8601(951_782_400.0), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn har_has_one_entry_per_request() {
        let h = sample().to_har();
        assert_eq!(h["log"]["entries"].as_array().unwrap().len(), 1);
        assert_eq!(h["log"]["entries"][0]["response"]["status"], 201);
        assert_eq!(h["log"]["version"], "1.2");
    }
}

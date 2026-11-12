//! Request filter language (task 4B.3). Space-separated terms, all of which must match:
//!
//! - `text` or `"quoted text"`: substring of the URL (case-insensitive)
//! - `field:value`: substring match on `method`, `host`, `url`, `type`, `mime`, `protocol`, `ip`
//! - `status:404`, `status>=400`, `size>10k`, `time>500` (ms): numeric comparison
//!   (`> >= < <= = !=`; `k`/`m` suffixes on sizes)
//! - `error` / `-error`: has / has no error
//! - a leading `-` negates any term

use crate::Request;

#[derive(Debug, PartialEq)]
enum Cmp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

#[derive(Debug, PartialEq)]
enum Test {
    Text(String),
    Field(String, String),
    Num(String, Cmp, f64),
    HasError,
}

#[derive(Debug, PartialEq)]
pub struct Filter(Vec<(bool, Test)>);

fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn number(v: &str) -> Result<f64, String> {
    let v = v.to_ascii_lowercase();
    let (digits, mul) = match v.chars().last() {
        Some('k') => (&v[..v.len() - 1], 1024.0),
        Some('m') => (&v[..v.len() - 1], 1048576.0),
        _ => (v.as_str(), 1.0),
    };
    digits
        .parse::<f64>()
        .map(|n| n * mul)
        .map_err(|_| format!("not a number: {v}"))
}

const NUMERIC: [&str; 3] = ["status", "size", "time"];
const TEXTUAL: [&str; 7] = ["method", "host", "url", "type", "mime", "protocol", "ip"];

impl Filter {
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut terms = Vec::new();
        for tok in tokens(src) {
            let (neg, t) = match tok.strip_prefix('-') {
                Some(rest) if !rest.is_empty() => (true, rest.to_string()),
                _ => (false, tok),
            };
            if t.eq_ignore_ascii_case("error") {
                terms.push((neg, Test::HasError));
                continue;
            }
            // Operators first (longest match), so `status>=400` is not read as a text field.
            let op_at = t.find(['>', '<', '=', '!']);
            if let Some(i) = op_at {
                let field = t[..i].to_ascii_lowercase();
                let rest = &t[i..];
                let (cmp, len) = if rest.starts_with(">=") {
                    (Cmp::Ge, 2)
                } else if rest.starts_with("<=") {
                    (Cmp::Le, 2)
                } else if rest.starts_with("!=") {
                    (Cmp::Ne, 2)
                } else if rest.starts_with('>') {
                    (Cmp::Gt, 1)
                } else if rest.starts_with('<') {
                    (Cmp::Lt, 1)
                } else if rest.starts_with('=') {
                    (Cmp::Eq, 1)
                } else {
                    return Err(format!("bad operator in {t}"));
                };
                if !NUMERIC.contains(&field.as_str()) {
                    return Err(format!("unknown numeric field: {field}"));
                }
                terms.push((neg, Test::Num(field, cmp, number(&rest[len..])?)));
            } else if let Some((field, value)) = t.split_once(':') {
                let field = field.to_ascii_lowercase();
                if NUMERIC.contains(&field.as_str()) {
                    terms.push((neg, Test::Num(field, Cmp::Eq, number(value)?)));
                } else if TEXTUAL.contains(&field.as_str()) {
                    terms.push((neg, Test::Field(field, value.to_ascii_lowercase())));
                } else {
                    return Err(format!("unknown field: {field}"));
                }
            } else {
                terms.push((neg, Test::Text(t.to_ascii_lowercase())));
            }
        }
        Ok(Self(terms))
    }

    pub fn matches(&self, r: &Request) -> bool {
        self.0.iter().all(|(neg, t)| test(t, r) != *neg)
    }
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or_default()
}

fn test(t: &Test, r: &Request) -> bool {
    match t {
        Test::Text(s) => r.url.to_ascii_lowercase().contains(s),
        Test::HasError => r.error.is_some(),
        Test::Field(f, v) => {
            let hay = match f.as_str() {
                "method" => r.method.clone(),
                "host" => host(&r.url).to_string(),
                "url" => r.url.clone(),
                "type" => r.resource_type.clone(),
                "mime" => r.mime_type.clone(),
                "protocol" => r.protocol.clone().unwrap_or_default(),
                _ => r.remote_ip.clone().unwrap_or_default(),
            };
            hay.to_ascii_lowercase().contains(v)
        }
        Test::Num(f, cmp, n) => {
            let have = match f.as_str() {
                "status" => r.status.map(f64::from),
                "size" => r.encoded_bytes.map(|b| b as f64),
                _ => r.duration_ms(),
            };
            let Some(h) = have else { return false };
            match cmp {
                Cmp::Gt => h > *n,
                Cmp::Ge => h >= *n,
                Cmp::Lt => h < *n,
                Cmp::Le => h <= *n,
                Cmp::Eq => (h - n).abs() < 1e-9,
                Cmp::Ne => (h - n).abs() >= 1e-9,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(url: &str, method: &str, status: u16, bytes: u64) -> Request {
        Request {
            url: url.into(),
            method: method.into(),
            status: Some(status),
            encoded_bytes: Some(bytes),
            started: 0.0,
            finished: Some(0.3),
            resource_type: "XHR".into(),
            ..Request::default()
        }
    }

    fn hits(f: &str, r: &Request) -> bool {
        Filter::parse(f).unwrap().matches(r)
    }

    #[test]
    fn text_and_fields() {
        let r = req("https://API.example.com/v1/users?id=3", "POST", 200, 10);
        assert!(hits("users", &r));
        assert!(hits("host:example.com method:post", &r));
        assert!(!hits("host:other.org", &r));
        assert!(hits("\"v1/users\"", &r));
        assert!(hits("type:xhr", &r));
    }

    #[test]
    fn numeric_comparisons() {
        let r = req("https://a.test/", "GET", 404, 20 * 1024);
        assert!(hits("status>=400", &r));
        assert!(hits("status:404", &r));
        assert!(!hits("status<400", &r));
        assert!(hits("size>10k", &r));
        assert!(hits("time>=300 time<400", &r));
        assert!(hits("status!=200", &r));
    }

    #[test]
    fn negation_and_errors() {
        let mut r = req("https://a.test/x", "GET", 200, 1);
        assert!(hits("-error", &r));
        assert!(!hits("error", &r));
        r.error = Some("net::ERR_FAILED".into());
        assert!(hits("error", &r));
        assert!(hits("-host:b.test", &r));
        assert!(!hits("-x", &r));
    }

    #[test]
    fn bad_input_is_reported() {
        assert!(Filter::parse("colour:red").is_err());
        assert!(Filter::parse("status>abc").is_err());
        assert!(Filter::parse("method>3").is_err());
        assert!(Filter::parse("").unwrap().matches(&Request::default()));
    }

    #[test]
    fn requests_without_a_status_never_match_numeric_tests() {
        let r = Request::default();
        assert!(!hits("status>=0", &r));
    }
}

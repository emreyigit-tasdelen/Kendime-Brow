//! URL helpers for the built-in PDF viewer (pure, tested).

/// `file:///C:/dir/a%20b.pdf` → `C:/dir/a b.pdf` (Windows) / `/dir/a b.pdf` (Unix).
pub fn file_url_to_path(url: &str) -> Option<String> {
    let lower = url.get(..8)?.to_ascii_lowercase();
    if lower != "file:///" {
        return None;
    }
    let rest = url[8..].split(['?', '#']).next()?;
    let decoded = percent_decode(rest)?;
    // `file:///C:/x` → `C:/x`; `file:///home/x` → `/home/x`.
    let is_drive = decoded.as_bytes().get(1) == Some(&b':');
    Some(if is_drive {
        decoded
    } else {
        format!("/{decoded}")
    })
}

pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Viewer URL for a `file://` navigation to a PDF (`kb://pdf`) or spreadsheet (`kb://office`);
/// `None` for anything else.
pub fn viewer_url_for_file_url(url: &str) -> Option<String> {
    let path = file_url_to_path(url)?;
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit_once('.')?.1;
    let host = if ext == "pdf" {
        "pdf"
    } else if crate::pdf::SHEET_EXTENSIONS.contains(&ext) {
        "office"
    } else {
        return None;
    };
    Some(format!("kb://{host}/?file={}", percent_encode(&path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls() {
        assert_eq!(
            file_url_to_path("file:///C:/dir/a%20b.pdf").as_deref(),
            Some("C:/dir/a b.pdf")
        );
        assert_eq!(
            file_url_to_path("file:///home/u/x.pdf").as_deref(),
            Some("/home/u/x.pdf")
        );
        assert_eq!(file_url_to_path("https://x/a.pdf"), None);
        assert_eq!(
            file_url_to_path("file:///C:/a.pdf?page=2#x").as_deref(),
            Some("C:/a.pdf")
        );
    }

    #[test]
    fn viewer_urls_round_trip() {
        let v = viewer_url_for_file_url("file:///C:/My%20Docs/ş.PDF").unwrap();
        assert!(v.starts_with("kb://pdf/?file=C:/My%20Docs/"));
        let enc = v.strip_prefix("kb://pdf/?file=").unwrap();
        assert_eq!(percent_decode(enc).as_deref(), Some("C:/My Docs/ş.PDF"));
        assert_eq!(viewer_url_for_file_url("file:///C:/a.txt"), None);
        assert!(
            viewer_url_for_file_url("file:///C:/data.XLSX")
                .unwrap()
                .starts_with("kb://office/?file=")
        );
        assert_eq!(percent_decode("%zz"), None);
    }
}

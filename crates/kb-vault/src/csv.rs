//! CSV import and export of login entries (task 6.3). Understands the column names used by
//! Chrome/Edge (`name,url,username,password,note`), Firefox (`url,username,password`) and
//! Bitwarden (`name,login_uri,login_username,login_password,notes`).

use crate::Secret;

/// RFC 4180 parser: quoted fields, doubled quotes, CRLF or LF, embedded newlines.
pub fn parse(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.is_empty()));
    rows
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
}

/// Login entries found in a CSV export. Rows without a password are skipped.
pub fn import_logins(text: &str) -> Result<Vec<Secret>, String> {
    let rows = parse(text);
    let header = rows.first().ok_or("empty file")?;
    let pw = column(header, &["password", "login_password"]).ok_or("no password column")?;
    let user = column(header, &["username", "login_username", "user"]);
    let url = column(header, &["url", "login_uri", "website"]);
    let name = column(header, &["name", "title"]);
    let note = column(header, &["note", "notes"]);
    let get =
        |r: &Vec<String>, i: Option<usize>| i.and_then(|i| r.get(i)).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for r in &rows[1..] {
        let password = get(r, Some(pw));
        if password.is_empty() {
            continue;
        }
        let url = get(r, url);
        let mut title = get(r, name);
        if title.is_empty() {
            title = url
                .split_once("://")
                .map_or(url.as_str(), |(_, h)| h)
                .split('/')
                .next()
                .unwrap_or_default()
                .to_string();
        }
        if title.is_empty() {
            title = "Imported login".into();
        }
        let mut s = Secret::default();
        s.title = title;
        s.username = get(r, user);
        s.password = password;
        s.url = url;
        s.notes = get(r, note);
        out.push(s);
    }
    Ok(out)
}

fn quote(s: &str) -> String {
    // A leading =,+,-,@ would run as a formula when the file is opened in a spreadsheet.
    let guard = matches!(s.chars().next(), Some('=' | '+' | '-' | '@'));
    let s = if guard {
        format!("'{s}")
    } else {
        s.to_string()
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// Chrome-style CSV (`name,url,username,password,note`) of the given logins.
pub fn export_logins<'a>(entries: impl IntoIterator<Item = &'a Secret>) -> String {
    let mut out = String::from("name,url,username,password,note\r\n");
    for s in entries {
        out.push_str(&format!(
            "{},{},{},{},{}\r\n",
            quote(&s.title),
            quote(&s.url),
            quote(&s.username),
            quote(&s.password),
            quote(&s.notes)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_quotes_newlines_and_bom() {
        let rows = parse("\u{feff}a,b\r\n\"x,1\",\"say \"\"hi\"\"\"\r\n\"multi\nline\",z\n");
        assert_eq!(rows[1], ["x,1", "say \"hi\""]);
        assert_eq!(rows[2], ["multi\nline", "z"]);
    }

    #[test]
    fn imports_chrome_and_bitwarden_layouts() {
        let chrome = "name,url,username,password,note\nGH,https://github.com/login,me,pw1,n\n";
        let a = import_logins(chrome).unwrap();
        assert_eq!(
            (
                a[0].title.as_str(),
                a[0].username.as_str(),
                a[0].password.as_str()
            ),
            ("GH", "me", "pw1")
        );
        let bw = "folder,name,login_uri,login_username,login_password,notes\n,Mail,https://m.test,u,p2,\n";
        let b = import_logins(bw).unwrap();
        assert_eq!(
            (
                b[0].title.as_str(),
                b[0].url.as_str(),
                b[0].password.as_str()
            ),
            ("Mail", "https://m.test", "p2")
        );
    }

    #[test]
    fn firefox_style_rows_get_a_title_from_the_host_and_empty_passwords_are_skipped() {
        let ff = "url,username,password\nhttps://example.org/a,me,pw\nhttps://skip.test,me,\n";
        let v = import_logins(ff).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].title, "example.org");
    }

    #[test]
    fn missing_password_column_is_an_error() {
        assert!(import_logins("name,url\na,b\n").is_err());
        assert!(import_logins("").is_err());
    }

    #[test]
    fn export_round_trips_and_neutralises_formulas() {
        let mut s = Secret::default();
        s.title = "a,b".into();
        s.username = "=cmd".into();
        s.password = "p\"w".into();
        let csv = export_logins([&s]);
        assert!(csv.contains("'=cmd"));
        let back = import_logins(&csv).unwrap();
        assert_eq!(
            (back[0].title.as_str(), back[0].password.as_str()),
            ("a,b", "p\"w")
        );
    }
}

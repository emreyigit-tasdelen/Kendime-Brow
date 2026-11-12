//! KeePass (.kdbx 3/4) import (task 6.3): reads entries with a master password into
//! login secrets. Entries in the recycle bin are skipped. Keyfiles are not supported yet.

use crate::Secret;
use keepass::{Database, DatabaseKey};

pub fn import_logins(data: &[u8], password: &str) -> Result<Vec<Secret>, String> {
    let key = DatabaseKey::new().with_password(password);
    let db = Database::parse(data, key).map_err(|e| format!("cannot open KeePass file: {e}"))?;
    let mut trashed = std::collections::HashSet::new();
    if let Some(bin) = db.recycle_bin() {
        let mut stack = vec![bin.id()];
        while let Some(id) = stack.pop() {
            if let Some(g) = db.group(id) {
                trashed.extend(g.entry_ids());
                stack.extend(g.group_ids());
            }
        }
    }
    let mut out = Vec::new();
    for e in db.iter_all_entries() {
        if trashed.contains(&e.id()) {
            continue;
        }
        let s = |v: Option<&str>| v.unwrap_or_default().to_string();
        let title = s(e.get_title());
        let url = s(e.get_url());
        let password = s(e.get_password());
        if password.is_empty() && url.is_empty() {
            continue;
        }
        out.push(Secret {
            title: if title.is_empty() { url.clone() } else { title },
            username: s(e.get_username()),
            password,
            url,
            notes: s(e.get("Notes")),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut db = Database::new();
        db.root_mut().add_entry().edit(|e| {
            e.set_unprotected("Title", "Site");
            e.set_unprotected("UserName", "bob");
            e.set_protected("Password", "pw1");
            e.set_unprotected("URL", "https://site.test");
        });
        let mut out = Vec::new();
        db.save(&mut out, DatabaseKey::new().with_password("master"))
            .unwrap();
        out
    }

    #[test]
    fn imports_entries_and_rejects_wrong_password() {
        let data = sample();
        let v = import_logins(&data, "master").unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(
            (
                v[0].title.as_str(),
                v[0].username.as_str(),
                v[0].password.as_str(),
                v[0].url.as_str()
            ),
            ("Site", "bob", "pw1", "https://site.test")
        );
        assert!(import_logins(&data, "wrong").is_err());
        assert!(import_logins(b"not a database", "master").is_err());
    }
}

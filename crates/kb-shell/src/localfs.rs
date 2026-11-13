//! Local file operations for the `kb://localfiles` explorer (task 3C.6): list, mkdir, rename,
//! copy and delete. Nothing is overwritten (copy and rename refuse an existing target), and
//! delete asks for an explicit `confirm` flag in addition to the UI's own confirmation step.
//! Archives and drag and drop are not implemented yet.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn arg<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

fn path_arg(v: &Value, k: &str) -> Result<PathBuf, String> {
    let s = arg(v, k);
    if s.is_empty() {
        return Err(format!("{k} is required"));
    }
    Ok(PathBuf::from(s))
}

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            // Symlinks are skipped so a copy can never escape the chosen tree.
            if e.file_type()?.is_symlink() {
                continue;
            }
            copy_all(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    let io = |e: std::io::Error| e.to_string();
    match op {
        "home" => Ok(json!({ "path": home().display().to_string() }).to_string()),
        "list" => {
            let p = match arg(args, "path") {
                "" => home(),
                s => PathBuf::from(s),
            };
            let mut items: Vec<Value> = std::fs::read_dir(&p)
                .map_err(io)?
                .flatten()
                .take(5000)
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    let modified = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs());
                    Some(
                        json!({ "name": e.file_name().to_string_lossy(), "dir": m.is_dir(),
                                 "size": m.len(), "modified": modified }),
                    )
                })
                .collect();
            items.sort_by(|a, b| {
                let key = |v: &Value| {
                    (
                        !v["dir"].as_bool().unwrap_or(false),
                        v["name"].as_str().unwrap_or_default().to_lowercase(),
                    )
                };
                key(a).cmp(&key(b))
            });
            Ok(json!({ "path": p.display().to_string(), "entries": items }).to_string())
        }
        "mkdir" => {
            std::fs::create_dir(path_arg(args, "path")?).map_err(io)?;
            Ok("{}".into())
        }
        "rename" => {
            let (from, to) = (path_arg(args, "from")?, path_arg(args, "to")?);
            if to.exists() {
                return Err("the target already exists".into());
            }
            std::fs::rename(from, to).map_err(io)?;
            Ok("{}".into())
        }
        "copy" => {
            let (from, to) = (path_arg(args, "from")?, path_arg(args, "to")?);
            if to.exists() {
                return Err("the target already exists".into());
            }
            if to.starts_with(&from) {
                return Err("cannot copy a folder into itself".into());
            }
            copy_all(&from, &to).map_err(io)?;
            Ok("{}".into())
        }
        "delete" => {
            if !args
                .get("confirm")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err("delete needs confirm: true".into());
            }
            let p = path_arg(args, "path")?;
            if p.parent().is_none() || p == home() {
                return Err("refusing to delete a root or the home folder".into());
            }
            if p.is_dir() {
                std::fs::remove_dir_all(&p).map_err(io)?;
            } else {
                std::fs::remove_file(&p).map_err(io)?;
            }
            Ok("{}".into())
        }
        _ => Err(format!("unknown localfs op: {op}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_rename_delete_guards() {
        let d = std::env::temp_dir().join(format!("kb-localfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("a/sub")).unwrap();
        std::fs::write(d.join("a/sub/f.txt"), "x").unwrap();
        let s = |p: &Path| p.display().to_string();
        let copy = json!({ "from": s(&d.join("a")), "to": s(&d.join("b")) });
        call("copy", &copy).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("b/sub/f.txt")).unwrap(), "x");
        assert!(call("copy", &copy).is_err(), "never overwrites");
        let into = json!({ "from": s(&d.join("a")), "to": s(&d.join("a/in")) });
        assert!(call("copy", &into).is_err());
        let ren = json!({ "from": s(&d.join("b")), "to": s(&d.join("a")) });
        assert!(call("rename", &ren).is_err());
        let del = json!({ "path": s(&d.join("b")) });
        assert!(call("delete", &del).is_err(), "needs confirm");
        call(
            "delete",
            &json!({ "path": s(&d.join("b")), "confirm": true }),
        )
        .unwrap();
        assert!(!d.join("b").exists());
        let list: Value =
            serde_json::from_str(&call("list", &json!({ "path": s(&d) })).unwrap()).unwrap();
        assert_eq!(list["entries"][0]["name"], "a");
        assert!(call("delete", &json!({ "path": "/", "confirm": true })).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}

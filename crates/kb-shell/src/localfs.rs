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

/// Total uncompressed size and entry-count caps guard against zip bombs.
const MAX_UNPACKED: u64 = 4 << 30;
const MAX_ENTRIES: usize = 50_000;

fn extract_zip(from: &Path, to: &Path) -> Result<(), String> {
    use std::io::Read;
    let f = std::fs::File::open(from).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    if z.len() > MAX_ENTRIES {
        return Err("too many entries".into());
    }
    std::fs::create_dir(to).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    for i in 0..z.len() {
        let e = z.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name rejects absolute paths and `..` (zip-slip).
        let Some(rel) = e.enclosed_name() else {
            continue;
        };
        let dest = to.join(rel);
        if e.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(|e| e.to_string())?;
        let n = std::io::copy(&mut e.take(MAX_UNPACKED - total + 1), &mut out)
            .map_err(|e| e.to_string())?;
        total += n;
        if total > MAX_UNPACKED {
            return Err("archive too large when unpacked".into());
        }
    }
    Ok(())
}

fn add_dir(w: &mut zip::ZipWriter<std::fs::File>, root: &Path, dir: &Path) -> Result<(), String> {
    use std::io::Write;
    let opts = zip::write::SimpleFileOptions::default();
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        if e.file_type().map_err(|e| e.to_string())?.is_symlink() {
            continue;
        }
        let p = e.path();
        let name = p
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        if p.is_dir() {
            w.add_directory(format!("{name}/"), opts)
                .map_err(|e| e.to_string())?;
            add_dir(w, root, &p)?;
        } else {
            w.start_file(name, opts).map_err(|e| e.to_string())?;
            let data = std::fs::read(&p).map_err(|e| e.to_string())?;
            w.write_all(&data).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn create_zip(from: &Path, to: &Path) -> Result<(), String> {
    let out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|e| e.to_string())?;
    let mut w = zip::ZipWriter::new(out);
    if from.is_dir() {
        add_dir(&mut w, from, from)?;
    } else {
        use std::io::Write;
        let name = from.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        w.start_file(name, zip::write::SimpleFileOptions::default())
            .map_err(|e| e.to_string())?;
        w.write_all(&std::fs::read(from).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(())
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
        "zip-list" => {
            let f = std::fs::File::open(path_arg(args, "path")?).map_err(io)?;
            let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for i in 0..z.len().min(5000) {
                let e = z.by_index(i).map_err(|e| e.to_string())?;
                out.push(json!({ "name": e.enclosed_name().map(|p| p.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")), "size": e.size(), "dir": e.is_dir() }));
            }
            Ok(json!({ "entries": out }).to_string())
        }
        "zip-extract" => {
            let from = path_arg(args, "from")?;
            let to = path_arg(args, "to")?;
            if to.exists() {
                return Err("the target already exists".into());
            }
            extract_zip(&from, &to)?;
            Ok("{}".into())
        }
        "zip-create" => {
            let from = path_arg(args, "from")?;
            let to = path_arg(args, "to")?;
            if to.exists() {
                return Err("the target already exists".into());
            }
            if to.starts_with(&from) {
                return Err("cannot zip a folder into itself".into());
            }
            create_zip(&from, &to)?;
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

    #[test]
    fn zip_round_trip_and_zip_slip() {
        let d = std::env::temp_dir().join(format!("kb-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src/sub")).unwrap();
        std::fs::write(d.join("src/sub/a.txt"), "hello").unwrap();
        let s = |p: &Path| p.display().to_string();
        let z = json!({ "from": s(&d.join("src")), "to": s(&d.join("o.zip")) });
        call("zip-create", &z).unwrap();
        assert!(call("zip-create", &z).is_err(), "never overwrites");
        let x = json!({ "from": s(&d.join("o.zip")), "to": s(&d.join("out")) });
        call("zip-extract", &x).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.join("out/sub/a.txt")).unwrap(),
            "hello"
        );
        assert!(call("zip-extract", &x).is_err());
        let l: Value = serde_json::from_str(
            &call("zip-list", &json!({ "path": s(&d.join("o.zip")) })).unwrap(),
        )
        .unwrap();
        assert!(
            l["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["name"] == "sub/a.txt")
        );
        // zip-slip entry is skipped, not written outside the target.
        let evil = std::fs::File::create(d.join("evil.zip")).unwrap();
        let mut w = zip::ZipWriter::new(evil);
        use std::io::Write;
        w.start_file("../escape.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        call(
            "zip-extract",
            &json!({ "from": s(&d.join("evil.zip")), "to": s(&d.join("out2")) }),
        )
        .unwrap();
        assert!(!d.join("escape.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}

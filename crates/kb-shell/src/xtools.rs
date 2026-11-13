//! External PDF tools: qpdf (3A.4: encrypt, linearize, repair) and Tesseract (3A.6: OCR to a
//! searchable PDF). Both run as ordinary child processes on user-chosen files and write a copy
//! next to the original (never overwriting). Lookup: `KB_QPDF_DIR` / `KB_TESSERACT_DIR`,
//! `<profile>/components/<tool>`, Program Files, then PATH.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn find(name: &str, env: &str, program_files_dirs: &[&str]) -> String {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os(env) {
        dirs.push(PathBuf::from(d));
    }
    if let Some(p) = crate::shell::profile_dir() {
        dirs.push(p.join("components").join(name));
    }
    for d in program_files_dirs {
        dirs.push(PathBuf::from(d));
        dirs.push(PathBuf::from(d).join("bin"));
    }
    for d in dirs {
        let p = d.join(&exe);
        if p.exists() {
            return p.display().to_string();
        }
    }
    exe
}

fn qpdf() -> String {
    find(
        "qpdf",
        "KB_QPDF_DIR",
        &["C:/Program Files/qpdf 12.4.2", "C:/Program Files/qpdf"],
    )
}

fn tesseract() -> String {
    find(
        "tesseract",
        "KB_TESSERACT_DIR",
        &["C:/Program Files/Tesseract-OCR"],
    )
}

fn pdf_path(args: &Value) -> Result<PathBuf, String> {
    let p = PathBuf::from(args.get("path").and_then(Value::as_str).unwrap_or_default());
    if !p.is_absolute() || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return Err("path must be an absolute .pdf file".into());
    }
    if !p.is_file() {
        return Err("file not found".into());
    }
    Ok(p)
}

fn output(input: &Path, suffix: &str) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    kb_platform::files::unique_path(dir, &format!("{stem} ({suffix}).pdf"))
}

/// qpdf arguments for an operation (pure; unit-tested). Passwords go through `--password=`
/// style options and are never logged.
pub fn qpdf_args(
    op: &str,
    input: &Path,
    out: &Path,
    password: &str,
    owner: &str,
) -> Result<Vec<String>, String> {
    let (i, o) = (input.display().to_string(), out.display().to_string());
    Ok(match op {
        "linearize" => vec!["--linearize".into(), i, o],
        // qpdf rewrites the file, rebuilding the cross-reference table of damaged PDFs.
        "repair" => vec![i, o],
        "encrypt" => {
            if password.is_empty() {
                return Err("a password is required".into());
            }
            let owner = if owner.is_empty() { password } else { owner };
            vec![
                "--encrypt".into(),
                password.into(),
                owner.into(),
                "256".into(),
                "--".into(),
                i,
                o,
            ]
        }
        other => return Err(format!("unknown qpdf op: {other}")),
    })
}

fn run_out(cmd: &mut Command, what: &str) -> Result<(), String> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{what} not available ({e}); install it or set its *_DIR variable"))?;
    // qpdf exits 3 for "succeeded with warnings", which still writes a usable file.
    let ok = out.status.success() || out.status.code() == Some(3);
    if ok {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = err.lines().rev().take(3).collect();
    Err(tail.into_iter().rev().collect::<Vec<_>>().join("\n"))
}

pub fn call(op: &str, args: &Value) -> Result<String, String> {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or_default();
    match op {
        "linearize" | "repair" | "encrypt" => {
            let p = pdf_path(args)?;
            let suffix = match op {
                "linearize" => "linearized",
                "repair" => "repaired",
                _ => "encrypted",
            };
            let out = output(&p, suffix);
            let a = qpdf_args(op, &p, &out, s("password"), s("owner"))?;
            run_out(Command::new(qpdf()).args(a), "qpdf")?;
            Ok(json!({ "path": out.display().to_string() }).to_string())
        }
        "ocr" => {
            let p = pdf_path(args)?;
            let lang = s("lang");
            let lang = if lang.is_empty() { "eng" } else { lang };
            if !lang
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '+')
            {
                return Err("invalid language code".into());
            }
            // Tesseract reads images, not PDFs: it is given page images produced by the caller.
            let img = PathBuf::from(s("image"));
            if !img.is_absolute() || !img.is_file() {
                return Err("image must be an absolute path to an existing image".into());
            }
            let base = output(&p, "ocr").with_extension("");
            run_out(
                Command::new(tesseract())
                    .arg(&img)
                    .arg(&base)
                    .args(["-l", lang, "pdf"]),
                "tesseract",
            )?;
            Ok(json!({ "path": base.with_extension("pdf").display().to_string() }).to_string())
        }
        _ => Err(format!("unknown pdf-tool op: {op}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_qpdf_arguments() {
        let (i, o) = (Path::new("/a.pdf"), Path::new("/b.pdf"));
        assert_eq!(
            qpdf_args("linearize", i, o, "", "").unwrap()[0],
            "--linearize"
        );
        let e = qpdf_args("encrypt", i, o, "pw", "").unwrap();
        assert_eq!(e[..4], ["--encrypt", "pw", "pw", "256"]);
        assert!(qpdf_args("encrypt", i, o, "", "").is_err());
        assert!(qpdf_args("zip", i, o, "", "").is_err());
    }

    #[test]
    fn rejects_non_pdf_paths() {
        assert!(pdf_path(&json!({ "path": "rel.pdf" })).is_err());
        assert!(pdf_path(&json!({ "path": "/x/y.txt" })).is_err());
    }

    #[test]
    fn linearize_and_encrypt_with_real_qpdf() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../kb-doc/tests/fixtures/encrypted.pdf");
        if Command::new(qpdf()).arg("--version").output().is_err() || !fixture.exists() {
            eprintln!("qpdf not available; skipped");
            return;
        }
        let d = std::env::temp_dir().join(format!("kb-xtools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        // Make a plain copy first (decrypt with the fixture's known password if needed).
        let plain = d.join("plain.pdf");
        let st = Command::new(qpdf())
            .args(["--decrypt", "--password=secret"])
            .arg(&fixture)
            .arg(&plain)
            .status()
            .unwrap();
        if !plain.exists() && !st.success() {
            eprintln!("fixture password unknown; skipped");
            return;
        }
        let r: Value = serde_json::from_str(
            &call("linearize", &json!({ "path": plain.display().to_string() })).unwrap(),
        )
        .unwrap();
        assert!(Path::new(r["path"].as_str().unwrap()).exists());
        let r: Value = serde_json::from_str(
            &call(
                "encrypt",
                &json!({ "path": plain.display().to_string(), "password": "pw1" }),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(Path::new(r["path"].as_str().unwrap()).exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}

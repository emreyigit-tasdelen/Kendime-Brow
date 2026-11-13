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

fn soffice() -> String {
    find(
        "soffice",
        "KB_LIBREOFFICE_DIR",
        &["C:/Program Files/LibreOffice/program"],
    )
}

const OFFICE_TARGETS: &[&str] = &[
    "pdf", "docx", "odt", "xlsx", "ods", "pptx", "odp", "csv", "txt", "html",
];

/// Converts an office document with headless LibreOffice into `<name> (converted).<fmt>`.
fn office_convert(path: &str, fmt: &str) -> Result<PathBuf, String> {
    office_convert_to(path, fmt, None)
}

/// With `cache` the result goes there (replacing an older preview) instead of next to the source.
fn office_convert_to(path: &str, fmt: &str, cache: Option<&Path>) -> Result<PathBuf, String> {
    if !OFFICE_TARGETS.contains(&fmt) {
        return Err(format!("unsupported target format: {fmt}"));
    }
    let src = PathBuf::from(path);
    if !src.is_absolute() || !src.is_file() {
        return Err("path must be an absolute existing file".into());
    }
    let work = std::env::temp_dir().join(format!("kb-office-{}-{}", std::process::id(), fmt));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    // A throwaway profile keeps the conversion away from any running LibreOffice instance.
    let profile = work.join("profile");
    let url = format!(
        "-env:UserInstallation=file:///{}",
        profile.display().to_string().replace('\\', "/")
    );
    let r = run_out(
        Command::new(soffice())
            .arg(url)
            .args(["--headless", "--convert-to", fmt, "--outdir"])
            .arg(&work)
            .arg(&src),
        "LibreOffice",
    );
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let made = work.join(format!("{stem}.{fmt}"));
    let res = match r {
        Ok(()) if made.is_file() => {
            let dest = match cache {
                Some(dir) => {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                    dir.join(format!("{stem}.{fmt}"))
                }
                None => {
                    let dir = src.parent().unwrap_or(Path::new("."));
                    kb_platform::files::unique_path(dir, &format!("{stem} (converted).{fmt}"))
                }
            };
            std::fs::copy(&made, &dest)
                .map(|_| dest)
                .map_err(|e| e.to_string())
        }
        Ok(()) => Err("conversion produced no output".into()),
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_dir_all(&work);
    res
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
        // Lossless size reduction: stronger Flate, object streams, unused-resource removal.
        "compress" => vec![
            "--recompress-flate".into(),
            "--compression-level=9".into(),
            "--object-streams=generate".into(),
            "--remove-unreferenced-resources=yes".into(),
            i,
            o,
        ],
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
        "linearize" | "repair" | "encrypt" | "compress" => {
            let p = pdf_path(args)?;
            let suffix = match op {
                "linearize" => "linearized",
                "compress" => "compressed",
                "repair" => "repaired",
                _ => "encrypted",
            };
            let out = output(&p, suffix);
            let a = qpdf_args(op, &p, &out, s("password"), s("owner"))?;
            run_out(Command::new(qpdf()).args(a), "qpdf")?;
            Ok(json!({ "path": out.display().to_string() }).to_string())
        }
        "office-convert" => {
            let out = office_convert(s("path"), s("format"))?;
            Ok(json!({ "path": out.display().to_string() }).to_string())
        }
        "sign" => {
            let p = pdf_path(args)?;
            let read = |k: &str| std::fs::read_to_string(s(k)).map_err(|e| format!("{k}: {e}"));
            let (key, cert) = (read("key")?, read("cert")?);
            let name = if s("name").is_empty() {
                "Signer"
            } else {
                s("name")
            };
            let out = output(&p, "signed");
            crate::sign::sign_pdf(&qpdf(), &p, &out, &key, &cert, name)?;
            Ok(json!({ "path": out.display().to_string() }).to_string())
        }
        "verify" => {
            let p = pdf_path(args)?;
            let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
            Ok(json!({ "signatures": crate::sign::verify_pdf(&bytes)? }).to_string())
        }
        "office-preview" => {
            let dir = std::env::temp_dir().join("kb-office-preview");
            let out = office_convert_to(s("path"), "pdf", Some(&dir))?;
            Ok(json!({ "path": out.display().to_string() }).to_string())
        }
        "office-batch" => {
            let paths: Vec<&str> = args
                .get("paths")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).take(100).collect())
                .unwrap_or_default();
            let results: Vec<Value> = paths
                .iter()
                .map(|p| match office_convert(p, s("format")) {
                    Ok(o) => json!({ "path": p, "output": o.display().to_string() }),
                    Err(e) => json!({ "path": p, "error": e }),
                })
                .collect();
            Ok(json!({ "results": results }).to_string())
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
            // Render each page to a PNG, OCR it into a one-page searchable PDF, then merge.
            let meta = crate::pdf::open(crate::pdf::read_pdf(&p.display().to_string())?)?;
            let pages = serde_json::from_slice::<Value>(&meta)
                .ok()
                .and_then(|v| v["pages"].as_u64())
                .ok_or("could not read the page count")?
                .min(500) as usize;
            let work = std::env::temp_dir().join(format!("kb-ocr-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&work);
            std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
            let res = (|| -> Result<PathBuf, String> {
                let mut parts = Vec::new();
                for i in 0..pages {
                    let png = work.join(format!("p{i}.png"));
                    std::fs::write(&png, crate::pdf::render(i, 200.0)?)
                        .map_err(|e| e.to_string())?;
                    let base = work.join(format!("p{i}"));
                    run_out(
                        Command::new(tesseract())
                            .arg(&png)
                            .arg(&base)
                            .args(["-l", lang, "pdf"]),
                        "tesseract",
                    )?;
                    parts.push(base.with_extension("pdf"));
                }
                let out = output(&p, "ocr");
                let mut cmd = Command::new(qpdf());
                cmd.args(["--empty", "--pages"]);
                cmd.args(&parts);
                cmd.arg("--").arg(&out);
                run_out(&mut cmd, "qpdf")?;
                Ok(out)
            })();
            let _ = std::fs::remove_dir_all(&work);
            Ok(json!({ "path": res?.display().to_string() }).to_string())
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
    fn office_convert_validates_and_converts() {
        assert!(office_convert("/x.docx", "exe").is_err());
        assert!(office_convert("rel.docx", "pdf").is_err());
        if Command::new(soffice()).arg("--version").output().is_err() {
            eprintln!("LibreOffice not available; skipped");
            return;
        }
        let d = std::env::temp_dir().join(format!("kb-oc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("a.txt");
        std::fs::write(&f, "hello").unwrap();
        let out = office_convert(&f.display().to_string(), "pdf").unwrap();
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));
        let _ = std::fs::remove_dir_all(&d);
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

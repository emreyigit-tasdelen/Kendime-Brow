//! PDF viewing through the sandboxed `kb-doc-host` process (architecture §3.1, §6, task 3A.1).
//!
//! A dedicated thread owns a small tokio runtime and the host connection. The host is started
//! lazily, restarted if it dies, and the open document is re-sent after a restart. Requests come
//! from the bridge (UI thread / router) and are answered from the service thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};

use kb_ipc::launch::{Host, launch};
use kb_ipc::{pb, recv, send};
use kb_platform::sandbox::SpawnSpec;

/// Largest PDF handed to the host (limited by the IPC frame size).
pub const MAX_PDF_BYTES: usize = kb_ipc::MAX_FRAME_LEN - 1024;

type Reply = Sender<Result<Vec<u8>, String>>;

enum Request {
    Open(Vec<u8>, Reply),
    /// Generic call on the open document (or a stateless one like merge).
    Call {
        method: &'static str,
        body: Vec<u8>,
        reply: Reply,
    },
    Render {
        page: usize,
        dpi: f32,
        reply: Reply,
    },
}

static SERVICE: OnceLock<Mutex<Sender<Request>>> = OnceLock::new();

fn host_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.with_file_name(format!("kb-doc-host{}", std::env::consts::EXE_SUFFIX));
    p.exists().then_some(p)
}

/// Validates a user-supplied path: absolute, `.pdf`, existing file, size limit.
pub fn read_pdf(path: &str) -> Result<Vec<u8>, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("path must be absolute".into());
    }
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return Err("not a .pdf file".into());
    }
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a file".into());
    }
    if meta.len() as usize > MAX_PDF_BYTES {
        return Err(format!(
            "PDF larger than {} MB is not supported yet",
            MAX_PDF_BYTES >> 20
        ));
    }
    std::fs::read(p).map_err(|e| e.to_string())
}

fn service() -> &'static Mutex<Sender<Request>> {
    SERVICE.get_or_init(|| {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("kb-pdf".into())
            .spawn(move || run(rx))
            .expect("spawn pdf service thread");
        Mutex::new(tx)
    })
}

fn submit(make: impl FnOnce(Reply) -> Request) -> Result<Vec<u8>, String> {
    let (rtx, rrx) = channel();
    service()
        .lock()
        .map_err(|_| "service poisoned".to_string())?
        .send(make(rtx))
        .map_err(|_| "pdf service stopped".to_string())?;
    rrx.recv().map_err(|_| "pdf service stopped".to_string())?
}

/// Opens a document (JSON `{pages, width, height}` on success).
pub fn open(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    submit(|r| Request::Open(bytes, r))
}

/// Calls a document-editing method of the host (`pdf.merge`, `pdf.extract`, `pdf.rotate`).
pub fn call_host(method: &'static str, body: Vec<u8>) -> Result<Vec<u8>, String> {
    submit(|reply| Request::Call {
        method,
        body,
        reply,
    })
}

/// `<dir>/<stem> (<suffix>).pdf`, made unique.
/// Writes `<name> (unlocked).pdf` without encryption after opening `path` with `password`.
pub fn unlock_file(path: &str, password: &str) -> Result<String, String> {
    let mut body = password.as_bytes().to_vec();
    body.push(10);
    body.extend_from_slice(&read_pdf(path)?);
    let out = call_host("pdf.unlock", body)?;
    let target = output_path(path, "unlocked");
    std::fs::write(&target, out).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

pub fn output_path(input: &str, suffix: &str) -> PathBuf {
    let p = Path::new(input);
    let dir = p.parent().unwrap_or(Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
    kb_platform::files::unique_path(dir, &format!("{stem} ({suffix}).pdf"))
}

/// Runs `op` over every path, collecting a result per file (task 3A.9).
pub fn batch(
    op: &str,
    paths: &[String],
    pages: &str,
    degrees: i32,
    password: &str,
) -> Result<serde_json::Value, String> {
    if !matches!(op, "rotate" | "extract" | "unlock") {
        return Err(format!("unknown batch op: {op}"));
    }
    if paths.is_empty() || paths.len() > 200 {
        return Err("give between 1 and 200 files".into());
    }
    let results: Vec<serde_json::Value> = paths
        .iter()
        .map(|p| {
            let r = match op {
                "rotate" => {
                    let j = serde_json::json!({ "pages": pages, "degrees": degrees }).to_string();
                    edit_file(p, "pdf.rotate", j, "rotated")
                }
                "extract" => {
                    let j = serde_json::json!({ "pages": pages }).to_string();
                    edit_file(p, "pdf.extract", j, "pages")
                }
                _ => unlock_file(p, password),
            };
            match r {
                Ok(o) => serde_json::json!({ "path": p, "output": o }),
                Err(e) => serde_json::json!({ "path": p, "error": e }),
            }
        })
        .collect();
    Ok(serde_json::json!({ "results": results }))
}

/// Merges the given PDF files into `<first> (merged).pdf`; returns the output path.
pub fn merge_files(paths: &[String]) -> Result<String, String> {
    if paths.len() < 2 {
        return Err("select at least two PDF files".into());
    }
    let docs = paths
        .iter()
        .map(|p| read_pdf(p))
        .collect::<Result<Vec<_>, _>>()?;
    let total: usize = docs.iter().map(Vec::len).sum();
    if total > MAX_PDF_BYTES {
        return Err("combined size exceeds the 16 MB limit".into());
    }
    let out = call_host("pdf.merge", kb_doc_pack(&docs))?;
    let target = output_path(&paths[0], "merged");
    std::fs::write(&target, out).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

fn kb_doc_pack(docs: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for d in docs {
        out.extend_from_slice(&(d.len() as u32).to_le_bytes());
        out.extend_from_slice(d);
    }
    out
}

/// Opens `path`, runs `method` with `json` and writes `<name> (<suffix>).pdf`.
pub fn edit_file(
    path: &str,
    method: &'static str,
    json: String,
    suffix: &str,
) -> Result<String, String> {
    open(read_pdf(path)?)?;
    let out = call_host(method, json.into_bytes())?;
    let target = output_path(path, suffix);
    std::fs::write(&target, out).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Spreadsheet formats the preview supports.
pub const SHEET_EXTENSIONS: &[&str] = &["xlsx", "xlsm", "xls", "xlsb", "ods", "csv", "tsv"];

/// Reads a spreadsheet file and asks the host for a preview (JSON).
pub fn sheet_preview(path: &str) -> Result<Vec<u8>, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("path must be absolute".into());
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| SHEET_EXTENSIONS.contains(&e.as_str()))
        .ok_or("unsupported file type")?;
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() as usize > MAX_PDF_BYTES {
        return Err("file missing or larger than 16 MB".into());
    }
    let mut body = ext.into_bytes();
    body.push(b'\n');
    body.extend(std::fs::read(p).map_err(|e| e.to_string())?);
    call_host("sheet.preview", body)
}

/// Image formats the metadata viewer opens.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "tif", "tiff", "heic", "heif", "gif", "jxl", "bmp",
];

fn read_image(path: &str) -> Result<Vec<u8>, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("path must be absolute".into());
    }
    let ok = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if !ok {
        return Err("unsupported image type".into());
    }
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() as usize > MAX_PDF_BYTES {
        return Err("file missing or larger than 16 MB".into());
    }
    std::fs::read(p).map_err(|e| e.to_string())
}

/// EXIF of an image file (JSON from the host).
pub fn image_meta(path: &str) -> Result<Vec<u8>, String> {
    call_host("image.exif", read_image(path)?)
}

/// Writes `<name> (clean).jpg` without EXIF/XMP (JPEG only, lossless); returns the path.
pub fn image_strip(path: &str) -> Result<String, String> {
    let lower = path.to_ascii_lowercase();
    if !(lower.ends_with(".jpg") || lower.ends_with(".jpeg")) {
        return Err("metadata removal currently supports JPEG files".into());
    }
    let out = call_host("image.strip", read_image(path)?)?;
    let p = Path::new(path);
    let dir = p.parent().unwrap_or(Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let target = kb_platform::files::unique_path(dir, &format!("{stem} (clean).jpg"));
    std::fs::write(&target, out).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Converts an image to `format` (png, jpg, bmp, gif, webp) next to the original as
/// `<name> (converted).<ext>`; metadata is not carried over. Returns the new path.
pub fn image_convert(path: &str, format: &str) -> Result<String, String> {
    let mut body = format.as_bytes().to_vec();
    body.push(10);
    body.extend_from_slice(&read_image(path)?);
    let out = call_host("image.convert", body)?;
    let ext = match format {
        "jpeg" => "jpg",
        f => f,
    };
    let p = Path::new(path);
    let dir = p.parent().unwrap_or(Path::new("."));
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let target = kb_platform::files::unique_path(dir, &format!("{stem} (converted).{ext}"));
    std::fs::write(&target, out).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// The image as a base64 `data:` URL for the viewer (no parsing).
pub fn image_data_url(path: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = read_image(path)?;
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return Err("this format cannot be displayed in the viewer".into()),
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Renders a page to PNG bytes.
pub fn render(page: usize, dpi: f32) -> Result<Vec<u8>, String> {
    submit(|reply| Request::Render { page, dpi, reply })
}

async fn call(host: &mut Host, method: &str, body: Vec<u8>) -> Result<Vec<u8>, String> {
    send(
        &mut *host.stream,
        &pb::Envelope {
            id: 1,
            method: method.into(),
            body,
            error: String::new(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    let reply: pb::Envelope = recv(&mut *host.stream).await.map_err(|e| e.to_string())?;
    if reply.error.is_empty() {
        Ok(reply.body)
    } else {
        Err(reply.error)
    }
}

async fn start_host() -> Result<Host, String> {
    let exe = host_exe().ok_or("kb-doc-host not found next to the executable")?;
    launch(
        "doc",
        SpawnSpec {
            exe,
            memory_limit: Some(2 * 1024 * 1024 * 1024),
            max_processes: 1,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| e.to_string())
}

fn run(rx: Receiver<Request>) {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    rt.block_on(async move {
        let mut host: Option<Host> = None;
        let mut doc: Option<Vec<u8>> = None;
        while let Ok(req) = rx.recv() {
            // Two attempts: a dead host is replaced once and the document re-opened.
            let (result, reply) = match req {
                Request::Open(bytes, reply) => {
                    doc = Some(bytes.clone());
                    let mut r = Err("no host".to_string());
                    for _ in 0..2 {
                        if host.is_none() {
                            match start_host().await {
                                Ok(h) => host = Some(h),
                                Err(e) => {
                                    r = Err(e);
                                    break;
                                }
                            }
                        }
                        r = call(host.as_mut().expect("host"), "pdf.open", bytes.clone()).await;
                        if r.is_ok() || r.as_ref().is_err_and(|e| !is_transport(e)) {
                            break;
                        }
                        host = None;
                    }
                    (r, reply)
                }
                Request::Call {
                    method,
                    body,
                    reply,
                } => {
                    let mut r = Err("no host".to_string());
                    for _ in 0..2 {
                        if host.is_none() {
                            match start_host().await {
                                Ok(mut h) => {
                                    if !matches!(
                                        method,
                                        "pdf.merge"
                                            | "sheet.preview"
                                            | "image.exif"
                                            | "image.strip"
                                    ) {
                                        let Some(d) = doc.clone() else {
                                            r = Err("no document open".into());
                                            break;
                                        };
                                        if let Err(e) = call(&mut h, "pdf.open", d).await {
                                            r = Err(e);
                                            break;
                                        }
                                    }
                                    host = Some(h);
                                }
                                Err(e) => {
                                    r = Err(e);
                                    break;
                                }
                            }
                        }
                        r = call(host.as_mut().expect("host"), method, body.clone()).await;
                        if r.is_ok() || r.as_ref().is_err_and(|e| !is_transport(e)) {
                            break;
                        }
                        host = None;
                    }
                    (r, reply)
                }
                Request::Render { page, dpi, reply } => {
                    let body = format!("{{\"page\":{page},\"dpi\":{dpi}}}").into_bytes();
                    let mut r = Err("no document open".to_string());
                    for attempt in 0..2 {
                        if host.is_none() {
                            let Some(d) = doc.clone() else { break };
                            match start_host().await {
                                Ok(mut h) => {
                                    if let Err(e) = call(&mut h, "pdf.open", d).await {
                                        r = Err(e);
                                        break;
                                    }
                                    host = Some(h);
                                }
                                Err(e) => {
                                    r = Err(e);
                                    break;
                                }
                            }
                        }
                        r = call(host.as_mut().expect("host"), "pdf.render", body.clone()).await;
                        if r.is_ok() || attempt == 1 || r.as_ref().is_err_and(|e| !is_transport(e))
                        {
                            break;
                        }
                        host = None;
                    }
                    (r, reply)
                }
            };
            let _ = reply.send(result);
        }
    });
}

/// IO-level failures (host crashed / pipe closed) as opposed to errors reported by the host.
fn is_transport(e: &str) -> bool {
    let e = e.to_ascii_lowercase();
    e.contains("pipe")
        || e.contains("eof")
        || e.contains("broken")
        || e.contains("closed")
        || e.contains("reset")
        || e.contains("os error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_validation() {
        assert!(read_pdf("relative.pdf").is_err());
        assert!(
            read_pdf(if cfg!(windows) {
                "C:\\x\\notpdf.txt"
            } else {
                "/x/notpdf.txt"
            })
            .is_err()
        );
        assert!(
            read_pdf(if cfg!(windows) {
                "C:\\definitely\\missing.pdf"
            } else {
                "/definitely/missing.pdf"
            })
            .is_err()
        );
    }

    #[test]
    fn batch_validates_input() {
        assert!(batch("explode", &["a.pdf".into()], "", 0, "").is_err());
        assert!(batch("rotate", &[], "", 90, "").is_err());
        let r = batch("unlock", &["/no/such.pdf".into()], "", 0, "x").unwrap();
        assert!(
            r["results"][0]["error"].is_string(),
            "per-file error, no abort"
        );
    }

    #[test]
    fn transport_errors() {
        assert!(is_transport("The pipe is being closed. (os error 232)"));
        assert!(!is_transport("cannot open PDF: password required"));
    }
}

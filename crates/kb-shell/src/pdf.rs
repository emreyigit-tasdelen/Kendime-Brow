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
    Render { page: usize, dpi: f32, reply: Reply },
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
    fn transport_errors() {
        assert!(is_transport("The pipe is being closed. (os error 232)"));
        assert!(!is_transport("cannot open PDF: password required"));
    }
}

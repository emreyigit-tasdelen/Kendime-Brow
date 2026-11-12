//! IPC service of the `kb-doc` host (task 3A.1). One open document per host process; bodies are
//! JSON inside the `Envelope.body` of `kb-ipc`.
//!
//! Methods:
//! - `pdf.open`   body: raw PDF bytes (≤ `MAX_FRAME_LEN`; the sandboxed host cannot read user
//!   files, so the shell reads them and hands the bytes over) → JSON `{pages, width, height}`
//! - `pdf.render` body: JSON `{page, dpi}` → PNG bytes
//! - `pdf.close`  → empty

use kb_ipc::{Stream, pb, recv, send};
use pdfium_render::prelude::Pdfium;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RenderRequest {
    pub page: usize,
    #[serde(default = "default_dpi")]
    pub dpi: f32,
}

fn default_dpi() -> f32 {
    96.0
}

#[derive(Debug, Deserialize)]
pub struct PagesRequest {
    /// Page selection like `1-3,5,8-` (1-based).
    pub pages: String,
}

#[derive(Debug, Deserialize)]
pub struct RotateRequest {
    /// Page selection; empty = all pages.
    #[serde(default)]
    pub pages: String,
    pub degrees: i32,
}

/// Packs several documents into one body: repeated `u32 length (LE) + bytes`.
pub fn pack(docs: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for d in docs {
        out.extend_from_slice(&(d.len() as u32).to_le_bytes());
        out.extend_from_slice(d);
    }
    out
}

pub fn unpack(mut body: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut docs = Vec::new();
    while !body.is_empty() {
        let len = u32::from_le_bytes(
            body.get(..4)
                .ok_or("truncated body")?
                .try_into()
                .map_err(|_| "bad body")?,
        ) as usize;
        let d = body.get(4..4 + len).ok_or("truncated body")?;
        docs.push(d.to_vec());
        body = &body[4 + len..];
    }
    Ok(docs)
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OpenReply {
    pub pages: usize,
    pub width: f32,
    pub height: f32,
}

/// Handles one request. `doc` holds the bytes of the open document.
pub fn handle(
    pdfium: &Pdfium,
    doc: &mut Option<Vec<u8>>,
    method: &str,
    body: &[u8],
) -> Result<Vec<u8>, String> {
    match method {
        "pdf.open" => {
            let i = crate::info(pdfium, body, None).map_err(|e| e.to_string())?;
            let page = i.first_page.unwrap_or(crate::PageInfo {
                width: 0.0,
                height: 0.0,
            });
            *doc = Some(body.to_vec());
            serde_json::to_vec(&OpenReply {
                pages: i.pages,
                width: page.width,
                height: page.height,
            })
            .map_err(|e| e.to_string())
        }
        "pdf.render" => {
            let req: RenderRequest = serde_json::from_slice(body).map_err(|e| e.to_string())?;
            let data = doc.as_ref().ok_or("no document open")?;
            crate::render_page_png(pdfium, data, None, req.page, req.dpi).map_err(|e| e.to_string())
        }
        "pdf.merge" => {
            let docs = unpack(body)?;
            let refs: Vec<&[u8]> = docs.iter().map(Vec::as_slice).collect();
            crate::edit::merge(pdfium, &refs).map_err(|e| e.to_string())
        }
        "pdf.extract" => {
            let req: PagesRequest = serde_json::from_slice(body).map_err(|e| e.to_string())?;
            let data = doc.as_ref().ok_or("no document open")?;
            let total = crate::info(pdfium, data, None)
                .map_err(|e| e.to_string())?
                .pages;
            let pages = crate::edit::parse_page_ranges(&req.pages, total)?;
            crate::edit::extract(pdfium, data, &pages).map_err(|e| e.to_string())
        }
        "pdf.rotate" => {
            let req: RotateRequest = serde_json::from_slice(body).map_err(|e| e.to_string())?;
            let data = doc.as_ref().ok_or("no document open")?;
            let total = crate::info(pdfium, data, None)
                .map_err(|e| e.to_string())?
                .pages;
            let pages = if req.pages.trim().is_empty() {
                Vec::new()
            } else {
                crate::edit::parse_page_ranges(&req.pages, total)?
            };
            crate::edit::rotate(pdfium, data, &pages, req.degrees).map_err(|e| e.to_string())
        }
        "pdf.close" => {
            *doc = None;
            Ok(Vec::new())
        }
        other => Err(format!("unknown method {other}")),
    }
}

/// Serves requests until the peer disconnects.
pub async fn serve(stream: &mut dyn Stream, pdfium: &Pdfium) -> std::io::Result<()> {
    let mut doc: Option<Vec<u8>> = None;
    loop {
        let req: pb::Envelope = match recv(&mut *stream).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let (body, error) = match handle(pdfium, &mut doc, &req.method, &req.body) {
            Ok(b) => (b, String::new()),
            Err(e) => (Vec::new(), e),
        };
        send(
            &mut *stream,
            &pb::Envelope {
                id: req.id,
                method: req.method,
                body,
                error,
            },
        )
        .await?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trip() {
        let docs = vec![b"abc".to_vec(), Vec::new(), vec![1u8; 300]];
        assert_eq!(unpack(&pack(&docs)).unwrap(), docs);
        assert!(unpack(&[9, 0, 0, 0, 1]).is_err());
        assert!(unpack(&[1, 2]).is_err());
    }
}

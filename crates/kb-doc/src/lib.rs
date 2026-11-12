//! PDF engine wrapper (architecture §6, task 3A.1): PDFium through `pdfium-render`.
//!
//! The PDFium shared library (BSD-3, from bblanchon/pdfium-binaries, pinned in
//! `scripts/fetch-pdfium.*`) is loaded at runtime from `PDFIUM_PATH` or next to the executable.
//! This crate runs inside the `kb-doc` module host (no network, sandboxed); the IPC service
//! around it comes next.

use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;

#[derive(Debug)]
pub enum DocError {
    /// PDFium library not found / not loadable.
    Library(String),
    /// The document could not be opened (corrupt, encrypted without password, …).
    Open(String),
    /// Page index out of range or render failure.
    Render(String),
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Library(m) => write!(f, "PDF engine unavailable: {m}"),
            Self::Open(m) => write!(f, "cannot open PDF: {m}"),
            Self::Render(m) => write!(f, "cannot render page: {m}"),
        }
    }
}

impl std::error::Error for DocError {}

pub mod edit;
pub mod forms;
pub mod meta;
pub mod service;
pub mod sheets;
pub mod tables;

pub fn library_file_name() -> &'static str {
    if cfg!(windows) {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

/// Candidate library locations: `PDFIUM_PATH` (file or folder), the executable's folder, cwd.
pub fn library_candidates() -> Vec<PathBuf> {
    let mut c = Vec::new();
    if let Some(p) = std::env::var_os("PDFIUM_PATH").map(PathBuf::from) {
        c.push(if p.is_dir() {
            p.join(library_file_name())
        } else {
            p
        });
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        c.push(dir.join(library_file_name()));
    }
    c.push(PathBuf::from(library_file_name()));
    c
}

pub fn load_pdfium() -> Result<Pdfium, DocError> {
    let mut last = String::from("no candidate paths");
    for path in library_candidates() {
        match Pdfium::bind_to_library(&path) {
            Ok(b) => return Ok(Pdfium::new(b)),
            Err(e) => last = format!("{}: {e}", path.display()),
        }
    }
    Err(DocError::Library(last))
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageInfo {
    /// Page size in PDF points (1/72 inch).
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocInfo {
    pub pages: usize,
    pub first_page: Option<PageInfo>,
}

pub fn info(pdfium: &Pdfium, data: &[u8], password: Option<&str>) -> Result<DocInfo, DocError> {
    let doc = pdfium
        .load_pdf_from_byte_slice(data, password)
        .map_err(|e| DocError::Open(e.to_string()))?;
    let pages = doc.pages();
    let first_page = pages.get(0).ok().map(|p| PageInfo {
        width: p.width().value,
        height: p.height().value,
    });
    Ok(DocInfo {
        pages: pages.len() as usize,
        first_page,
    })
}

/// Plain text of every page (index = page).
pub fn extract_text(pdfium: &Pdfium, data: &[u8]) -> Result<Vec<String>, DocError> {
    let doc = pdfium
        .load_pdf_from_byte_slice(data, None)
        .map_err(|e| DocError::Open(e.to_string()))?;
    let mut out = Vec::new();
    for page in doc.pages().iter() {
        out.push(
            page.text()
                .map(|t| t.all())
                .map_err(|e| DocError::Render(e.to_string()))?,
        );
    }
    Ok(out)
}

/// Pages containing `query` (case-insensitive), with up to `max_hits` snippets per page.
pub fn search_pages(texts: &[String], query: &str) -> Vec<(usize, String)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    texts
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let lower = t.to_lowercase();
            let at = lower.find(&q)?;
            // Snippet around the first hit (char-boundary safe on the original text).
            let start = t.floor_char_boundary(at.saturating_sub(30));
            let end = t.ceil_char_boundary((at + q.len() + 50).min(t.len()));
            Some((
                i,
                t[start..end]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            ))
        })
        .collect()
}

/// Renders one page at `dpi` to PNG bytes.
pub fn render_page_png(
    pdfium: &Pdfium,
    data: &[u8],
    password: Option<&str>,
    page_index: usize,
    dpi: f32,
) -> Result<Vec<u8>, DocError> {
    let doc = pdfium
        .load_pdf_from_byte_slice(data, password)
        .map_err(|e| DocError::Open(e.to_string()))?;
    let page = doc
        .pages()
        .get(page_index as i32)
        .map_err(|e| DocError::Render(e.to_string()))?;
    let scale = dpi.clamp(36.0, 600.0) / 72.0;
    let cfg = PdfRenderConfig::new()
        .scale_page_by_factor(scale)
        .set_target_width((page.width().value * scale).round() as i32);
    let bitmap = page
        .render_with_config(&cfg)
        .map_err(|e| DocError::Render(e.to_string()))?;
    let image = bitmap
        .as_image()
        .map_err(|e| DocError::Render(e.to_string()))?
        .into_rgba8();
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| DocError::Render(e.to_string()))?;
    Ok(out)
}

/// Loads PDFium relative to `dir` (tests / dev tools).
pub fn load_pdfium_from(dir: &Path) -> Result<Pdfium, DocError> {
    let lib = dir.join(library_file_name());
    Pdfium::bind_to_library(&lib)
        .map(Pdfium::new)
        .map_err(|e| DocError::Library(format!("{}: {e}", lib.display())))
}

//! PDF page-level editing (architecture §6, task 3A.2): merge, extract / split / reorder and
//! rotate, implemented on PDFium's page import so annotations, forms and fonts survive.

use pdfium_render::prelude::*;

use crate::DocError;

fn open<'a>(pdfium: &'a Pdfium, data: &'a [u8]) -> Result<PdfDocument<'a>, DocError> {
    pdfium
        .load_pdf_from_byte_slice(data, None)
        .map_err(|e| DocError::Open(e.to_string()))
}

fn save(doc: &PdfDocument) -> Result<Vec<u8>, DocError> {
    doc.save_to_bytes()
        .map_err(|e| DocError::Render(format!("save failed: {e}")))
}

fn err(e: impl std::fmt::Display) -> DocError {
    DocError::Render(e.to_string())
}

/// Appends all pages of every input, in order.
pub fn merge(pdfium: &Pdfium, inputs: &[&[u8]]) -> Result<Vec<u8>, DocError> {
    if inputs.is_empty() {
        return Err(DocError::Open("nothing to merge".into()));
    }
    let mut out = pdfium.create_new_pdf().map_err(err)?;
    for data in inputs {
        let src = open(pdfium, data)?;
        out.pages_mut().append(&src).map_err(err)?;
    }
    save(&out)
}

/// New document made of `pages` (0-based, in the given order; repeats allowed). This is split,
/// extract and reorder in one primitive.
pub fn extract(pdfium: &Pdfium, data: &[u8], pages: &[usize]) -> Result<Vec<u8>, DocError> {
    if pages.is_empty() {
        return Err(DocError::Open("no pages selected".into()));
    }
    let src = open(pdfium, data)?;
    let count = src.pages().len() as usize;
    if let Some(bad) = pages.iter().find(|p| **p >= count) {
        return Err(DocError::Render(format!(
            "page {} out of range (document has {count})",
            bad + 1
        )));
    }
    let mut out = pdfium.create_new_pdf().map_err(err)?;
    for (dest, p) in pages.iter().enumerate() {
        out.pages_mut()
            .copy_page_from_document(&src, *p as i32, dest as i32)
            .map_err(err)?;
    }
    save(&out)
}

/// Rotates the selected pages (0-based) by a multiple of 90° clockwise, relative to their
/// current rotation. An empty selection means all pages.
pub fn rotate(
    pdfium: &Pdfium,
    data: &[u8],
    pages: &[usize],
    degrees: i32,
) -> Result<Vec<u8>, DocError> {
    if degrees % 90 != 0 {
        return Err(DocError::Render("rotation must be a multiple of 90".into()));
    }
    let doc = open(pdfium, data)?;
    let count = doc.pages().len() as usize;
    let selected: Vec<usize> = if pages.is_empty() {
        (0..count).collect()
    } else {
        pages.to_vec()
    };
    for p in selected {
        if p >= count {
            return Err(DocError::Render(format!("page {} out of range", p + 1)));
        }
        let mut page = doc.pages().get(p as i32).map_err(err)?;
        let current = match page.rotation().map_err(err)? {
            PdfPageRenderRotation::None => 0,
            PdfPageRenderRotation::Degrees90 => 90,
            PdfPageRenderRotation::Degrees180 => 180,
            PdfPageRenderRotation::Degrees270 => 270,
        };
        let new = (current + degrees).rem_euclid(360);
        page.set_rotation(match new {
            90 => PdfPageRenderRotation::Degrees90,
            180 => PdfPageRenderRotation::Degrees180,
            270 => PdfPageRenderRotation::Degrees270,
            _ => PdfPageRenderRotation::None,
        });
    }
    save(&doc)
}

/// Parses a page selection like `1-3,5,8-` (1-based; open ranges end at `total`) into 0-based
/// indices in the order written. Errors on page 0, reversed or out-of-range pages.
pub fn parse_page_ranges(spec: &str, total: usize) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), Some(b.trim())),
            None => (part, None),
        };
        let start: usize = a.parse().map_err(|_| format!("bad page `{a}`"))?;
        let end: usize = match b {
            None => start,
            Some("") => total,
            Some(b) => b.parse().map_err(|_| format!("bad page `{b}`"))?,
        };
        if start == 0 || end == 0 {
            return Err("pages start at 1".into());
        }
        if start > end {
            return Err(format!("range {start}-{end} is reversed"));
        }
        if end > total {
            return Err(format!("page {end} is out of range (document has {total})"));
        }
        out.extend(start - 1..end);
    }
    if out.is_empty() {
        return Err("no pages selected".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges() {
        assert_eq!(parse_page_ranges("1-3,5", 9).unwrap(), vec![0, 1, 2, 4]);
        assert_eq!(parse_page_ranges("8-", 9).unwrap(), vec![7, 8]);
        assert_eq!(parse_page_ranges("3,1,3", 4).unwrap(), vec![2, 0, 2]);
        assert!(parse_page_ranges("0", 4).is_err());
        assert!(parse_page_ranges("3-1", 4).is_err());
        assert!(parse_page_ranges("5", 4).is_err());
        assert!(parse_page_ranges("", 4).is_err());
        assert!(parse_page_ranges("a-b", 4).is_err());
    }
}

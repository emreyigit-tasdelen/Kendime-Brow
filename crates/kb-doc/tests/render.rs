//! PDFium round trip. Skipped (passes with a notice) when no PDFium library is available;
//! set `PDFIUM_PATH` to the folder/file of `pdfium.dll` / `libpdfium.so` to run it.

use kb_doc::{info, load_pdfium, render_page_png};

/// A minimal one-page PDF (200×100 pt) with a text line, built with correct xref offsets.
fn sample_pdf() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        {
            let stream = "BT /F1 24 Tf 20 40 Td (Kendime-Brow) Tj ET";
            format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream)
        },
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[test]
fn renders_a_page_to_png() {
    let Ok(pdfium) = load_pdfium() else {
        eprintln!("PDFium library not found (set PDFIUM_PATH) — skipping");
        return;
    };
    let pdf = sample_pdf();
    let i = info(&pdfium, &pdf, None).unwrap();
    assert_eq!(i.pages, 1);
    let page = i.first_page.unwrap();
    assert_eq!(
        (page.width.round() as i32, page.height.round() as i32),
        (200, 100)
    );

    let png = render_page_png(&pdfium, &pdf, None, 0, 144.0).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    let img = image::load_from_memory(&png).unwrap().into_rgba8();
    assert_eq!(
        (img.width(), img.height()),
        (400, 200),
        "144 dpi = 2x of 72 dpi points"
    );
    // The page must not be blank: some dark text pixels on the white page.
    let dark = img.pixels().filter(|p| p.0[0] < 100 && p.0[3] > 0).count();
    assert!(dark > 50, "no text rendered ({dark} dark pixels)");

    assert!(render_page_png(&pdfium, &pdf, None, 5, 72.0).is_err());
    assert!(info(&pdfium, b"not a pdf", None).is_err());
}

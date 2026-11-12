//! Merge / extract / rotate with the real PDFium (skipped when it is not available).

use kb_doc::edit::{extract, merge, rotate};
use kb_doc::{info, load_pdfium};

/// A PDF with `n` pages of 200×100 pt; page i shows the text `P<i>`.
fn pdf_with_pages(n: usize, label: &str) -> Vec<u8> {
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        String::new(), // pages, filled below
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    let mut kids = Vec::new();
    for i in 0..n {
        let page_id = objs.len() + 1;
        let content_id = page_id + 1;
        kids.push(format!("{page_id} 0 R"));
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents {content_id} 0 R /Resources << /Font << /F1 3 0 R >> >> >>"
        ));
        let stream = format!("BT /F1 24 Tf 20 40 Td ({label}{i}) Tj ET");
        objs.push(format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            stream.len(),
            stream
        ));
    }
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" "));
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

fn text_of_page(pdfium: &pdfium_render::prelude::Pdfium, data: &[u8], page: i32) -> String {
    let doc = pdfium.load_pdf_from_byte_slice(data, None).unwrap();
    let p = doc.pages().get(page).unwrap();
    p.text().unwrap().all()
}

#[test]
fn merge_extract_rotate() {
    let Ok(pdfium) = load_pdfium() else {
        eprintln!("PDFium library not found (set PDFIUM_PATH) — skipping");
        return;
    };
    let a = pdf_with_pages(3, "A");
    let b = pdf_with_pages(2, "B");

    let merged = merge(&pdfium, &[&a, &b]).unwrap();
    assert_eq!(info(&pdfium, &merged, None).unwrap().pages, 5);
    assert_eq!(text_of_page(&pdfium, &merged, 0).trim(), "A0");
    assert_eq!(text_of_page(&pdfium, &merged, 3).trim(), "B0");
    assert!(merge(&pdfium, &[]).is_err());

    // Split / reorder: pages 4,1,1 (1-based) of the merged file.
    let picked = extract(&pdfium, &merged, &[3, 0, 0]).unwrap();
    assert_eq!(info(&pdfium, &picked, None).unwrap().pages, 3);
    assert_eq!(text_of_page(&pdfium, &picked, 0).trim(), "B0");
    assert_eq!(text_of_page(&pdfium, &picked, 1).trim(), "A0");
    assert!(extract(&pdfium, &merged, &[9]).is_err());
    assert!(extract(&pdfium, &merged, &[]).is_err());

    // Rotate page 1 by 90° → rendered size swaps (200×100 → 100×200).
    let rotated = rotate(&pdfium, &a, &[0], 90).unwrap();
    let doc = pdfium.load_pdf_from_byte_slice(&rotated, None).unwrap();
    let w = |i: i32| doc.pages().get(i).unwrap().width().value.round() as i32;
    assert_eq!((w(0), w(1)), (100, 200));
    // Rotating by 270 more brings it back; rotation is relative.
    let back = rotate(&pdfium, &rotated, &[0], 270).unwrap();
    let doc2 = pdfium.load_pdf_from_byte_slice(&back, None).unwrap();
    assert_eq!(
        doc2.pages().get(0).unwrap().width().value.round() as i32,
        200
    );
    assert!(rotate(&pdfium, &a, &[], 45).is_err());
}

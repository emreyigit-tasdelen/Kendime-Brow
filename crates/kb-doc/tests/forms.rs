//! Form listing and filling with the real PDFium (skipped without it).

use std::collections::HashMap;

use kb_doc::forms::{fill, list_fields};
use kb_doc::load_pdfium;

fn form_pdf() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 6 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << /Font << /Helv 5 0 R >> >> /Annots [7 0 R 8 0 R] >>".to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Fields [7 0 R 8 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> /NeedAppearances true >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (full_name) /V () /Rect [20 60 180 85] /F 4 /DA (/Helv 12 Tf 0 g) /P 3 0 R >>".to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (agree) /V /Off /AS /Off /Rect [20 20 40 40] /F 4 /P 3 0 R >>".to_string(),
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
fn lists_and_fills_fields() {
    let Ok(pdfium) = load_pdfium() else {
        eprintln!("PDFium library not found (set PDFIUM_PATH) — skipping");
        return;
    };
    let pdf = form_pdf();
    let fields = list_fields(&pdfium, &pdf).unwrap();
    let names: Vec<_> = fields.iter().map(|f| (f.name.as_str(), f.kind)).collect();
    assert!(names.contains(&("full_name", "text")), "{names:?}");

    let mut values = HashMap::new();
    values.insert("full_name".to_string(), "Emre Taşdelen".to_string());
    let filled = fill(&pdfium, &pdf, &values).unwrap();
    let after = list_fields(&pdfium, &filled).unwrap();
    let name = after.iter().find(|f| f.name == "full_name").unwrap();
    assert_eq!(name.value, "Emre Taşdelen");

    values.insert("nope".to_string(), "x".to_string());
    assert!(
        fill(&pdfium, &pdf, &values).is_err(),
        "unknown field must be rejected"
    );
}

/// Writes the fixture to `$KB_WRITE_FORM_PDF` (used by manual/live checks).
#[test]
fn write_fixture_when_requested() {
    if let Some(path) = std::env::var_os("KB_WRITE_FORM_PDF") {
        std::fs::write(path, form_pdf()).unwrap();
    }
}

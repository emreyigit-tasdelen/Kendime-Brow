//! PDF annotations (task 3A.3): sticky notes and highlights added as an incremental update.
//! The input is rewritten by qpdf without object streams first, the result is a new file
//! `<name> (annotated).pdf`; the original is never touched. Pages whose `/Annots` is an
//! indirect reference are refused rather than risk damaging them.

use crate::sign::{add_to_dict, find_last, number_after, qpdf_text};
use std::path::Path;
use std::process::Command;

fn esc(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '\r')
        .map(|c| match c {
            '\\' => "\\\\".to_string(),
            '(' => "\\(".to_string(),
            ')' => "\\)".to_string(),
            '\n' => "\\n".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// `kind` is `note` (sticky note at the rect's lower-left corner) or `highlight` (yellow box).
/// `rect` is `[x1, y1, x2, y2]` in PDF points with the origin at the page's lower-left corner.
pub fn annotate(
    qpdf: &str,
    input: &Path,
    out: &Path,
    page_no: u64,
    kind: &str,
    rect: [f64; 4],
    text: &str,
) -> Result<(), String> {
    if !matches!(kind, "note" | "highlight") {
        return Err("kind must be note or highlight".into());
    }
    if page_no == 0 {
        return Err("pages start at 1".into());
    }
    let tmp = out.with_extension("kbtmp.pdf");
    let r = inner(qpdf, input, &tmp, page_no, kind, rect, text);
    let _ = std::fs::remove_file(&tmp);
    let bytes = r?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut f, &bytes).map_err(|e| e.to_string())
}

fn inner(
    qpdf: &str,
    input: &Path,
    tmp: &Path,
    page_no: u64,
    kind: &str,
    rect: [f64; 4],
    text: &str,
) -> Result<Vec<u8>, String> {
    let st = Command::new(qpdf)
        .arg("--object-streams=disable")
        .arg(input)
        .arg(tmp)
        .status()
        .map_err(|e| format!("qpdf not available: {e}"))?;
    if !st.success() && st.code() != Some(3) {
        return Err("could not prepare the document".into());
    }
    let tmp_s = tmp.display().to_string();
    let mut pdf = std::fs::read(tmp).map_err(|e| e.to_string())?;
    let sx = find_last(&pdf, b"startxref").ok_or("no startxref")?;
    let prev: u64 = String::from_utf8_lossy(&pdf[sx + 9..])
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or("bad startxref")?;
    let tr = find_last(&pdf, b"trailer").ok_or("no trailer")?;
    let trailer = String::from_utf8_lossy(&pdf[tr..sx]).into_owned();
    let size = number_after(&trailer, "/Size").ok_or("no /Size")?;
    let root = number_after(&trailer, "/Root").ok_or("no /Root")?;
    let pages = qpdf_text(qpdf, &[&tmp_s, "--show-pages"])?;
    let page = number_after(&pages, &format!("page {page_no}:")).ok_or("no such page")?;
    let page_obj = qpdf_text(qpdf, &[&tmp_s, &format!("--show-object={page}")])?;
    let annot_n = size;
    let new_page = if let Some(i) = page_obj.find("/Annots") {
        let rest = page_obj[i + 7..].trim_start();
        if !rest.starts_with('[') {
            return Err("this page has indirect annotations; not supported yet".into());
        }
        let open = page_obj[i..].find('[').unwrap_or(0) + i;
        format!(
            "{}{annot_n} 0 R {}",
            &page_obj[..=open],
            &page_obj[open + 1..]
        )
    } else {
        add_to_dict(page_obj.trim_end(), &format!("/Annots [{annot_n} 0 R]"))?
    };
    let [x1, y1, x2, y2] = rect;
    let body = if kind == "note" {
        format!(
            "<< /Type /Annot /Subtype /Text /Rect [{x1} {y1} {} {}] /Contents ({}) /Name /Comment /F 28 /C [1 0.9 0] /P {page} 0 R >>",
            x1 + 20.0,
            y1 + 20.0,
            esc(text)
        )
    } else {
        format!(
            "<< /Type /Annot /Subtype /Highlight /Rect [{x1} {y1} {x2} {y2}] /QuadPoints [{x1} {y2} {x2} {y2} {x1} {y1} {x2} {y1}] /Contents ({}) /F 4 /C [1 1 0] /P {page} 0 R >>",
            esc(text)
        )
    };
    if !pdf.ends_with(b"\n") {
        pdf.push(b'\n');
    }
    let page_off = pdf.len();
    pdf.extend_from_slice(format!("{page} 0 obj\n{new_page}\nendobj\n").as_bytes());
    let annot_off = pdf.len();
    pdf.extend_from_slice(format!("{annot_n} 0 obj\n{body}\nendobj\n").as_bytes());
    let xref_pos = pdf.len();
    let mut x = String::from("xref\n");
    let mut objs = [(page, page_off), (annot_n, annot_off)];
    objs.sort();
    for (n, off) in objs {
        x.push_str(&format!("{n} 1\n{off:010} 00000 n \n"));
    }
    x.push_str(&format!(
        "trailer\n<< /Size {} /Root {root} 0 R /Prev {prev} >>\nstartxref\n{xref_pos}\n%%EOF\n",
        size + 1
    ));
    pdf.extend_from_slice(x.as_bytes());
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_text() {
        assert_eq!(esc("a(b)\\\n"), "a\\(b\\)\\\\\\n");
    }

    #[test]
    fn adds_note_and_highlight() {
        let qpdf = "C:/Program Files/qpdf 12.4.2/bin/qpdf.exe";
        if Command::new(qpdf).arg("--version").output().is_err() {
            return;
        }
        let d = std::env::temp_dir().join(format!("kb-annot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let src = d.join("a.pdf");
        std::fs::write(
            &src,
            "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>\nendobj\ntrailer\n<< /Root 1 0 R /Size 4 >>\n%%EOF\n",
        )
        .unwrap();
        let one = d.join("one.pdf");
        annotate(
            qpdf,
            &src,
            &one,
            1,
            "note",
            [10.0, 10.0, 0.0, 0.0],
            "hi (there)",
        )
        .unwrap();
        let two = d.join("two.pdf");
        annotate(
            qpdf,
            &one,
            &two,
            1,
            "highlight",
            [20.0, 20.0, 80.0, 40.0],
            "mark",
        )
        .unwrap();
        let chk = Command::new(qpdf)
            .arg("--check")
            .arg(&two)
            .output()
            .unwrap();
        assert!(chk.status.success() || chk.status.code() == Some(3));
        let json = Command::new(qpdf).arg("--json").arg(&two).output().unwrap();
        let t = String::from_utf8_lossy(&json.stdout);
        assert!(t.contains("/Highlight") && t.contains("/Text"), "{t}");
        assert!(annotate(qpdf, &src, &two, 1, "note", [0.0; 4], "").is_err());
        assert!(annotate(qpdf, &src, &d.join("x.pdf"), 9, "note", [0.0; 4], "").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rejects_bad_kind() {
        let p = Path::new("x.pdf");
        assert!(annotate("qpdf", p, p, 1, "nope", [0.0; 4], "").is_err());
        assert!(annotate("qpdf", p, p, 0, "note", [0.0; 4], "").is_err());
    }
}

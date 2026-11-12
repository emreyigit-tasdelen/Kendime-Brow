//! Spreadsheet preview without LibreOffice (architecture §7a, task 3B.1): xlsx / xls / xlsb /
//! ods through `calamine`, plus a small CSV reader.

use std::io::Cursor;

use calamine::{Data, Reader, open_workbook_auto_from_rs};
use serde::Serialize;

pub const MAX_ROWS: usize = 500;
pub const MAX_COLS: usize = 60;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SheetPreview {
    pub name: String,
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
    pub total_cols: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub sheets: Vec<SheetPreview>,
}

fn cell(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        other => other.to_string(),
    }
}

/// Previews a workbook given as bytes. `ext` (lowercase, no dot) selects CSV handling.
pub fn preview(data: &[u8], ext: &str) -> Result<Preview, String> {
    if ext == "csv" || ext == "tsv" {
        let sep = if ext == "tsv" { '\t' } else { ',' };
        let text = String::from_utf8_lossy(data);
        let all = parse_csv(&text, sep);
        return Ok(Preview {
            sheets: vec![to_sheet("Sheet1".into(), all)],
        });
    }
    let mut wb = open_workbook_auto_from_rs(Cursor::new(data)).map_err(|e| e.to_string())?;
    let names = wb.sheet_names().to_vec();
    let mut sheets = Vec::new();
    for name in names {
        let range = wb.worksheet_range(&name).map_err(|e| e.to_string())?;
        let all: Vec<Vec<String>> = range.rows().map(|r| r.iter().map(cell).collect()).collect();
        sheets.push(to_sheet(name, all));
    }
    Ok(Preview { sheets })
}

fn to_sheet(name: String, all: Vec<Vec<String>>) -> SheetPreview {
    let total_rows = all.len();
    let total_cols = all.iter().map(Vec::len).max().unwrap_or(0);
    let rows = all
        .into_iter()
        .take(MAX_ROWS)
        .map(|r| r.into_iter().take(MAX_COLS).collect())
        .collect();
    SheetPreview {
        name,
        rows,
        total_rows,
        total_cols,
    }
}

/// Minimal RFC 4180 reader (quotes, doubled quotes, newlines in quoted fields).
pub fn parse_csv(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (c, false) if c == sep => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parsing() {
        let rows = parse_csv("a,b,c\n1,\"x,y\",\"he said \"\"hi\"\"\"\r\n3,4,5", ',');
        assert_eq!(rows[0], ["a", "b", "c"]);
        assert_eq!(rows[1], ["1", "x,y", "he said \"hi\""]);
        assert_eq!(rows[2], ["3", "4", "5"]);
        assert_eq!(parse_csv("\u{feff}x\ty", '\t'), vec![vec!["x", "y"]]);
        assert!(parse_csv("", ',').is_empty());
    }

    #[test]
    fn csv_preview_limits() {
        let mut text = String::new();
        for i in 0..(MAX_ROWS + 20) {
            text.push_str(&format!("{i},x\n"));
        }
        let p = preview(text.as_bytes(), "csv").unwrap();
        assert_eq!(p.sheets[0].rows.len(), MAX_ROWS);
        assert_eq!(
            (p.sheets[0].total_rows, p.sheets[0].total_cols),
            (MAX_ROWS + 20, 2)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(preview(b"definitely not a workbook", "xlsx").is_err());
    }
}

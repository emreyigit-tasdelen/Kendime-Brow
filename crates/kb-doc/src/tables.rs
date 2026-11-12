//! Heuristic table detection in extracted page text (task 3A.7): a table is a run of at least
//! two consecutive lines that split into the same number (>= 2) of cells, where cells are
//! separated by tabs or runs of two or more spaces. Works on text only, so ragged or image
//! tables are out of scope.

pub type Table = Vec<Vec<String>>;

fn cells(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut spaces = 0;
    for c in line.trim().chars() {
        if c == '\t' {
            out.push(std::mem::take(&mut cur));
            spaces = 0;
        } else if c == ' ' {
            spaces += 1;
        } else {
            if spaces >= 2 && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            } else if spaces == 1 {
                cur.push(' ');
            }
            spaces = 0;
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.retain(|c| !c.is_empty());
    out
}

pub fn find_tables(text: &str) -> Vec<Table> {
    let mut tables = Vec::new();
    let mut run: Table = Vec::new();
    let flush = |run: &mut Table, tables: &mut Vec<Table>| {
        if run.len() >= 2 {
            tables.push(std::mem::take(run));
        } else {
            run.clear();
        }
    };
    for line in text.lines() {
        let row = cells(line);
        if row.len() >= 2 && run.first().is_none_or(|r| r.len() == row.len()) {
            run.push(row);
        } else {
            flush(&mut run, &mut tables);
            if row.len() >= 2 {
                run.push(row);
            }
        }
    }
    flush(&mut run, &mut tables);
    tables
}

/// RFC 4180 CSV for one table.
pub fn to_csv(table: &Table) -> String {
    let esc = |s: &str| {
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    table
        .iter()
        .map(|r| r.iter().map(|c| esc(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_table_between_paragraphs() {
        let text = "Intro paragraph here.\nName    Qty   Price\nApple   3     1.20\nPear    10    0.80\nClosing words.";
        let t = find_tables(text);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].len(), 3);
        assert_eq!(t[0][1], ["Apple", "3", "1.20"]);
    }

    #[test]
    fn single_spaces_stay_inside_a_cell_and_tabs_split() {
        let t = find_tables("New York\t8.3\nLos Angeles\t3.9");
        assert_eq!(t[0][0], ["New York", "8.3"]);
        assert_eq!(t[0][1], ["Los Angeles", "3.9"]);
    }

    #[test]
    fn lone_rows_and_prose_are_not_tables() {
        assert!(find_tables("just a sentence\nA    B\nmore prose").is_empty());
    }

    #[test]
    fn csv_quotes_special_characters() {
        let t = vec![vec!["a,b".to_string(), "say \"hi\"".to_string()]];
        assert_eq!(to_csv(&t), "\"a,b\",\"say \"\"hi\"\"\"\r\n");
    }
}

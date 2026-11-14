#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableText {
    /// Tab-separated values: what Cmd+C copies from a table and a data grid pastes.
    Tsv,
    /// Comma-separated values (RFC 4180), as spreadsheets save and open them.
    Csv,
}

impl TableText {
    pub fn delimiter(self) -> char {
        match self {
            TableText::Tsv => '\t',
            TableText::Csv => ',',
        }
    }

    /// The usual file name extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            TableText::Tsv => "tsv",
            TableText::Csv => "csv",
        }
    }

    /// `rows` as text: one line per row (`\n` between them, none after the last),
    /// cells quoted when they hold the delimiter, a line break or a quote. Cells are
    /// written as they are: a spreadsheet opening the text takes a cell that starts
    /// with `=` as a formula. A last row of one empty cell is written `""`, so that
    /// [`TableText::read`] gives it back (an empty last line ends the text instead).
    pub fn write(self, rows: &[Vec<String>]) -> String {
        let mut text = write_with(rows, self.delimiter());
        if rows
            .last()
            .is_some_and(|row| row.len() == 1 && row[0].is_empty())
        {
            text.push_str("\"\"");
        }
        text
    }

    /// Split `text` into rows of cells. `\r\n`, `\n` and `\r` all end a row; a line
    /// break after the last row does not start another. A cell that starts with a quote
    /// runs to the closing quote, taking delimiters and line breaks in, with `""` for a
    /// quote. A byte order mark at the start (spreadsheets save CSV with one) is
    /// skipped. Rows keep the cells they have: they are not padded to the longest.
    pub fn read(self, text: &str) -> Vec<Vec<String>> {
        read_with(
            text.strip_prefix('\u{feff}').unwrap_or(text),
            self.delimiter(),
        )
    }
}

pub(crate) fn write(rows: &[Vec<String>]) -> String {
    TableText::Tsv.write(rows)
}

fn write_with(rows: &[Vec<String>], delimiter: char) -> String {
    let mut out = String::new();
    for (r, row) in rows.iter().enumerate() {
        if r > 0 {
            out.push('\n');
        }
        for (c, cell) in row.iter().enumerate() {
            if c > 0 {
                out.push(delimiter);
            }
            if cell.contains([delimiter, '\n', '\r', '"']) {
                out.push('"');
                out.push_str(&cell.replace('"', "\"\""));
                out.push('"');
            } else {
                out.push_str(cell);
            }
        }
    }
    out
}

pub(crate) fn read(text: &str) -> Vec<Vec<String>> {
    read_with(text, '\t')
}

fn read_with(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut cell_start = true;
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(ch);
            }
            continue;
        }
        match ch {
            '"' if cell_start => {
                quoted = true;
                cell_start = false;
            }
            ch if ch == delimiter => {
                row.push(std::mem::take(&mut cell));
                cell_start = true;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                cell_start = true;
            }
            _ => {
                cell.push(ch);
                cell_start = false;
            }
        }
    }
    if !cell_start || !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(cells: &[&[&str]]) -> Vec<Vec<String>> {
        cells
            .iter()
            .map(|r| r.iter().map(|c| (*c).to_owned()).collect())
            .collect()
    }

    #[test]
    fn a_last_row_of_one_empty_cell_reads_back() {
        for format in [TableText::Tsv, TableText::Csv] {
            for grid in [
                rows(&[&["Name"], &["a"], &[""]]),
                rows(&[&[""]]),
                rows(&[&[""], &[""]]),
            ] {
                let text = format.write(&grid);
                assert_eq!(format.read(&text), grid, "{format:?} wrote {text:?}");
            }
        }
        assert_eq!(TableText::Csv.write(&rows(&[&["a", ""]])), "a,");
        assert_eq!(TableText::Csv.write(&rows(&[&[""], &["b"]])), "\nb");
    }

    #[test]
    fn clipboard_text_preserves_the_same_empty_rows_as_exported_tsv() {
        for grid in [
            rows(&[&[""]]),
            rows(&[&["value"], &[""]]),
            rows(&[&[""], &[""]]),
            rows(&[&["line\nbreak"], &[""]]),
        ] {
            let text = write(&grid);
            assert_eq!(read(&text), grid, "clipboard wrote {text:?}");
            assert_eq!(text, TableText::Tsv.write(&grid));
        }
    }

    #[test]
    fn cells_round_trip_through_the_text() {
        let grid = rows(&[
            &["a", "b c", ""],
            &["tab\there", "line\nbreak", "say \"hi\""],
        ]);
        let text = write(&grid);
        assert_eq!(
            text,
            "a\tb c\t\n\"tab\there\"\t\"line\nbreak\"\t\"say \"\"hi\"\"\""
        );
        assert_eq!(read(&text), grid);
    }

    #[test]
    fn line_endings_of_any_kind_end_rows_and_a_last_one_is_ignored() {
        assert_eq!(read("1\t2\r\n3\t4\r\n"), rows(&[&["1", "2"], &["3", "4"]]));
        assert_eq!(read("1\r2\n"), rows(&[&["1"], &["2"]]));
        assert_eq!(read("only"), rows(&[&["only"]]));
        assert_eq!(read(""), Vec::<Vec<String>>::new());
        // An empty line in the middle is a row with one empty cell.
        assert_eq!(read("a\n\nb"), rows(&[&["a"], &[""], &["b"]]));
        // A trailing tab leaves an empty last cell.
        assert_eq!(read("a\t"), rows(&[&["a", ""]]));
    }

    #[test]
    fn csv_quotes_cells_with_commas_and_reads_them_back() {
        let grid = rows(&[
            &["Region", "Jan", "Note"],
            &["North, east", "1,200", "tab\there"],
            &["say \"hi\"", "", "two\nlines"],
        ]);
        let text = TableText::Csv.write(&grid);
        assert_eq!(
            text,
            "Region,Jan,Note\n\"North, east\",\"1,200\",tab\there\n\"say \"\"hi\"\"\",,\"two\nlines\""
        );
        assert_eq!(TableText::Csv.read(&text), grid);
        // Tabs are ordinary characters in CSV, and commas in TSV.
        assert_eq!(TableText::Tsv.read("a,b\tc"), rows(&[&["a,b", "c"]]));
        assert_eq!(
            TableText::Csv.read("\u{feff}a,b\r\n1,2\r\n"),
            rows(&[&["a", "b"], &["1", "2"]])
        );
        assert_eq!(TableText::Csv.extension(), "csv");
    }

    #[test]
    fn a_quote_inside_a_cell_is_just_a_character() {
        assert_eq!(read("5\" disk\tx"), rows(&[&["5\" disk", "x"]]));
    }
}

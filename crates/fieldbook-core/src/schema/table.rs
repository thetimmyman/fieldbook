//! RFC 4180 CSV reader, normal-form writer, and row sorting for Corpus registers.
//!
//! **Responsibility.** Read register bytes into [`Table`] and [`Row`] with 1-based
//! line and code-point column positions (`read`), write a [`Table`] in canonical
//! Corpus normal form (`write`), normalize register bytes with Unicode NFC and
//! canonical key ordering (`normalize`), and test normal-form compliance (`is_normal`).
//!
//! **Rules.** Pure: no I/O. Tolerant of UTF-8 BOM and CRLF on read; strict LF,
//! no BOM, minimal quoting, and NFC on write.

use std::cmp::Ordering;
use std::fmt;
use std::ops::Deref;

use unicode_normalization::UnicodeNormalization;

use crate::ids::{ClaimId, ConflictId, QuestionId, SourceId, TermId};
use crate::schema::corpus_toml::byte_offset_to_line_col;
use crate::schema::register::Register;

/// One row of a CSV register table, with its 1-based start line and per-cell
/// 1-based code-point start columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 1-based line number where this row begins.
    pub line: u32,
    /// 1-based code-point column where each cell begins on `line`.
    pub cols: Vec<u32>,
    /// Decoded cell texts in column order.
    pub cells: Vec<String>,
}

impl Deref for Row {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        &self.cells
    }
}

/// A parsed CSV register table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// The header row (line 1).
    pub header: Row,
    /// Data rows following the header row.
    pub rows: Vec<Row>,
}

/// Classification of a CSV register read failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableErrorKind {
    /// The input contains bytes that are not valid UTF-8.
    NonUtf8 {
        /// 0-based byte offset of the first invalid UTF-8 sequence.
        offset: usize,
    },
    /// A quoted cell is unclosed, a quote appears inside an unquoted cell,
    /// or characters appear after a closing quote before the delimiter.
    UnbalancedQuote,
    /// A data row has a different number of cells from the header row.
    CellCount {
        /// Number of cells in the header row.
        header_count: usize,
        /// Number of cells in the offending data row.
        row_count: usize,
    },
    /// A cell contains an embedded line break (`\n` or `\r`).
    LineBreakInCell,
}

/// Error returned by [`read`] and [`normalize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableError {
    kind: TableErrorKind,
    line: u32,
    col: u32,
    message: String,
}

impl TableError {
    /// Returns the failure classification.
    #[must_use]
    pub const fn kind(&self) -> &TableErrorKind {
        &self.kind
    }

    /// Returns the 1-based line number of the fault (always `>= 1`).
    #[must_use]
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// Returns the 1-based code-point column number of the fault (always `>= 1`).
    #[must_use]
    pub const fn col(&self) -> u32 {
        self.col
    }

    /// Returns a human-readable diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for TableError {}

/// Reads CSV register bytes into a [`Table`], stripping an optional UTF-8 BOM
/// and accepting both LF and CRLF row endings.
///
/// Reports non-UTF-8 bytes (with byte offset), unbalanced quotes, rows whose
/// cell count differs from the header (with both counts), and line breaks inside
/// a cell.
pub fn read(bytes: &[u8]) -> Result<Table, TableError> {
    let raw_text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(err) => {
            let offset = err.valid_up_to();
            let valid_prefix = std::str::from_utf8(&bytes[..offset]).unwrap_or("");
            let valid_prefix = valid_prefix
                .strip_prefix('\u{feff}')
                .unwrap_or(valid_prefix);
            let (line, col) = byte_offset_to_line_col(valid_prefix, valid_prefix.len());
            return Err(TableError {
                kind: TableErrorKind::NonUtf8 { offset },
                line,
                col,
                message: format!("invalid UTF-8 at byte offset {offset}; save as CSV UTF-8"),
            });
        }
    };

    let text = raw_text.strip_prefix('\u{feff}').unwrap_or(raw_text);
    if text.is_empty() {
        return Ok(Table {
            header: Row {
                line: 1,
                cols: vec![1],
                cells: vec![String::new()],
            },
            rows: Vec::new(),
        });
    }

    let mut chars = text.chars().peekable();
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    let mut header: Option<Row> = None;
    let mut rows: Vec<Row> = Vec::new();

    while chars.peek().is_some() {
        let row_line = line;
        let mut cols: Vec<u32> = vec![1];
        let mut cells: Vec<String> = vec![String::new()];
        let mut just_closed_quote = false;

        while let Some(&c) = chars.peek() {
            if just_closed_quote {
                match c {
                    ',' => {
                        chars.next();
                        just_closed_quote = false;
                        col = col.saturating_add(1);
                        cols.push(col);
                        cells.push(String::new());
                    }
                    '\r' => {
                        chars.next();
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            line = line.saturating_add(1);
                            col = 1;
                            break;
                        }
                        return Err(TableError {
                            kind: TableErrorKind::LineBreakInCell,
                            line,
                            col,
                            message: "line break inside a cell".to_owned(),
                        });
                    }
                    '\n' => {
                        chars.next();
                        line = line.saturating_add(1);
                        col = 1;
                        break;
                    }
                    _ => {
                        return Err(TableError {
                            kind: TableErrorKind::UnbalancedQuote,
                            line,
                            col,
                            message: "unexpected characters after closing quote".to_owned(),
                        });
                    }
                }
                continue;
            }

            match c {
                '"' => {
                    let current_empty = cells.last().is_some_and(String::is_empty);
                    if !current_empty {
                        return Err(TableError {
                            kind: TableErrorKind::UnbalancedQuote,
                            line,
                            col,
                            message: "unbalanced quote inside unquoted cell".to_owned(),
                        });
                    }
                    let quote_line = line;
                    let quote_col = col;
                    chars.next(); // consume opening quote
                    col = col.saturating_add(1);
                    let mut first_line_break: Option<(u32, u32)> = None;

                    loop {
                        let Some(qch) = chars.next() else {
                            return Err(TableError {
                                kind: TableErrorKind::UnbalancedQuote,
                                line: quote_line,
                                col: quote_col,
                                message: "unbalanced quote: unclosed quoted cell".to_owned(),
                            });
                        };
                        match qch {
                            '"' => {
                                if chars.peek() == Some(&'"') {
                                    chars.next();
                                    col = col.saturating_add(2);
                                    if let Some(last) = cells.last_mut() {
                                        last.push('"');
                                    }
                                } else {
                                    if let Some((lb_line, lb_col)) = first_line_break {
                                        return Err(TableError {
                                            kind: TableErrorKind::LineBreakInCell,
                                            line: lb_line,
                                            col: lb_col,
                                            message: "line break inside a cell".to_owned(),
                                        });
                                    }
                                    col = col.saturating_add(1);
                                    just_closed_quote = true;
                                    break;
                                }
                            }
                            '\r' => {
                                if first_line_break.is_none() {
                                    first_line_break = Some((line, col));
                                }
                                if chars.peek() == Some(&'\n') {
                                    chars.next();
                                }
                                line = line.saturating_add(1);
                                col = 1;
                            }
                            '\n' => {
                                if first_line_break.is_none() {
                                    first_line_break = Some((line, col));
                                }
                                line = line.saturating_add(1);
                                col = 1;
                            }
                            other => {
                                if let Some(last) = cells.last_mut() {
                                    last.push(other);
                                }
                                col = col.saturating_add(1);
                            }
                        }
                    }
                }
                ',' => {
                    chars.next();
                    col = col.saturating_add(1);
                    cols.push(col);
                    cells.push(String::new());
                }
                '\r' => {
                    chars.next();
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                        line = line.saturating_add(1);
                        col = 1;
                        break;
                    }
                    return Err(TableError {
                        kind: TableErrorKind::LineBreakInCell,
                        line,
                        col,
                        message: "line break inside a cell".to_owned(),
                    });
                }
                '\n' => {
                    chars.next();
                    line = line.saturating_add(1);
                    col = 1;
                    break;
                }
                other => {
                    chars.next();
                    if let Some(last) = cells.last_mut() {
                        last.push(other);
                    }
                    col = col.saturating_add(1);
                }
            }
        }

        let row = Row {
            line: row_line,
            cols,
            cells,
        };
        if let Some(ref hdr) = header {
            let header_count = hdr.cells.len();
            let row_count = row.cells.len();
            if row_count != header_count {
                return Err(TableError {
                    kind: TableErrorKind::CellCount {
                        header_count,
                        row_count,
                    },
                    line: row_line,
                    col: 1,
                    message: format!("row has {row_count} cells, header has {header_count}"),
                });
            }
            rows.push(row);
        } else {
            header = Some(row);
        }
    }

    let header = header.unwrap_or_else(|| Row {
        line: 1,
        cols: vec![1],
        cells: vec![String::new()],
    });
    Ok(Table { header, rows })
}

fn write_row(out: &mut String, cells: &[String], is_header: bool) {
    for (idx, cell) in cells.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let needs_quote =
            cell.contains([',', '"']) || (is_header && idx == 0 && cell.starts_with('\u{feff}'));
        if needs_quote {
            out.push('"');
            for ch in cell.chars() {
                if ch == '"' {
                    out.push_str("\"\"");
                } else {
                    out.push(ch);
                }
            }
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push('\n');
}

/// Writes `table` in canonical Corpus register normal form:
/// LF line endings, no byte-order mark, a cell quoted only when it holds a
/// comma or a double quote (with double quotes doubled), and one final newline.
#[must_use]
pub fn write(table: &Table) -> String {
    let mut out = String::new();
    write_row(&mut out, &table.header.cells, true);
    for row in &table.rows {
        write_row(&mut out, &row.cells, false);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RowSortKey {
    Sources(String, u32),
    Claims(ClaimId),
    Questions(String),
    Conflicts(u32),
    Glossary(String),
}

fn parse_positive_version(s: &str) -> Option<u32> {
    if s.is_empty() || s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u32>().ok().filter(|&n| n >= 1)
}

fn find_col(header: &[String], name: &str, fallback: usize) -> usize {
    header.iter().position(|h| h == name).unwrap_or(fallback)
}

fn extract_sort_key(register: Register, header: &[String], cells: &[String]) -> Option<RowSortKey> {
    match register {
        Register::Sources => {
            let (id_idx, ver_idx) = match (
                header.iter().position(|h| h == "source_id"),
                header.iter().position(|h| h == "version"),
            ) {
                (Some(i), Some(v)) => (i, v),
                (None, None) | (Some(0), None) | (None, Some(1)) => (0, 1),
                _ => return None,
            };
            let id_str = cells.get(id_idx)?;
            let ver_str = cells.get(ver_idx)?;
            let source_id = SourceId::parse(id_str).ok()?;
            let version = parse_positive_version(ver_str)?;
            Some(RowSortKey::Sources(source_id.as_str().to_owned(), version))
        }
        Register::Claims => {
            let id_idx = find_col(header, "claim_id", 0);
            let id_str = cells.get(id_idx)?;
            let claim_id = ClaimId::parse(id_str).ok()?;
            Some(RowSortKey::Claims(claim_id))
        }
        Register::Questions => {
            let id_idx = find_col(header, "question_id", 0);
            let id_str = cells.get(id_idx)?;
            let question_id = QuestionId::parse(id_str).ok()?;
            Some(RowSortKey::Questions(question_id.as_str().to_owned()))
        }
        Register::Conflicts => {
            let id_idx = find_col(header, "conflict_id", 0);
            let id_str = cells.get(id_idx)?;
            let conflict_id = ConflictId::parse(id_str).ok()?;
            Some(RowSortKey::Conflicts(conflict_id.number()))
        }
        Register::Glossary => {
            let id_idx = find_col(header, "term_id", 0);
            let id_str = cells.get(id_idx)?;
            let term_id = TermId::parse(id_str).ok()?;
            Some(RowSortKey::Glossary(term_id.as_str().to_owned()))
        }
    }
}

/// Reads `bytes`, normalizes every cell to Unicode NFC, sorts rows by `register`'s
/// canonical key (leaving rows with unreadable keys in place after the sorted rows),
/// and writes the result in normal form.
pub fn normalize(register: Register, bytes: &[u8]) -> Result<String, TableError> {
    let mut table = read(bytes)?;

    for cell in &mut table.header.cells {
        *cell = cell.nfc().collect();
    }
    for row in &mut table.rows {
        for cell in &mut row.cells {
            *cell = cell.nfc().collect();
        }
    }

    let mut keyed: Vec<(RowSortKey, Row)> = Vec::with_capacity(table.rows.len());
    let mut unkeyed: Vec<Row> = Vec::new();

    for row in table.rows.drain(..) {
        if let Some(key) = extract_sort_key(register, &table.header.cells, &row.cells) {
            keyed.push((key, row));
        } else {
            unkeyed.push(row);
        }
    }

    keyed.sort_by(|(k1, _), (k2, _)| -> Ordering { k1.cmp(k2) });

    table.rows.extend(keyed.into_iter().map(|(_, row)| row));
    table.rows.extend(unkeyed);

    Ok(write(&table))
}

/// Returns `true` when `bytes` is valid UTF-8 and already equals [`normalize`]`(register, bytes)`.
#[must_use]
pub fn is_normal(register: Register, bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    if text.starts_with('\u{feff}') {
        return false;
    }
    match normalize(register, bytes) {
        Ok(norm) => norm == text,
        Err(_) => false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Excel-saved example: UTF-8 BOM, CRLF line endings, unnecessarily quoted cell,
    /// quoted cell with a comma, and a doubled quote inside a quoted cell, plus
    /// decomposed Unicode (`Cafe\u{0301}`).
    pub const EXCEL_SAVED_SAMPLE: &[u8] =
        "\u{feff}question_id,area,question\r\n\"Q-010\",\"cmp\",\"Plain cell\"\r\nQ-002,cmp,\"Needs, comma and \"\"quote\"\" in Cafe\u{0301}\"\r\n"
            .as_bytes();

    #[test]
    fn write_is_normal_form() {
        let normalized = normalize(Register::Questions, EXCEL_SAVED_SAMPLE)
            .expect("Excel sample must normalize");
        let expected = "\
question_id,area,question
Q-002,cmp,\"Needs, comma and \"\"quote\"\" in Café\"
Q-010,cmp,Plain cell
";
        assert_eq!(normalized, expected);
        assert!(!is_normal(Register::Questions, EXCEL_SAVED_SAMPLE));
        assert!(is_normal(Register::Questions, normalized.as_bytes()));

        // Verify cell start columns (1-based code points)
        let parsed = read(normalized.as_bytes()).expect("normalized text must read");
        assert_eq!(parsed.header.line, 1);
        assert_eq!(parsed.header.cols, vec![1, 13, 18]);
        assert_eq!(parsed.rows[0].line, 2);
        assert_eq!(parsed.rows[0].cols, vec![1, 7, 11]);
        assert_eq!(parsed.rows[1].line, 3);
        assert_eq!(parsed.rows[1].cols, vec![1, 7, 11]);
    }

    #[test]
    fn version_10_sorts_after_2() {
        let input = "\
source_id,version,title
S-0001,10,Tenth version
S-0001,2,Second version
S-0001,1,First version
bad-id,1,Unreadable key stays at end
";
        let norm = normalize(Register::Sources, input.as_bytes()).expect("normalize sources");
        let expected = "\
source_id,version,title
S-0001,1,First version
S-0001,2,Second version
S-0001,10,Tenth version
bad-id,1,Unreadable key stays at end
";
        assert_eq!(norm, expected);
    }

    #[test]
    fn xc_100_sorts_after_xc_99() {
        let input = "\
conflict_id,area,topic
XC-100,cmp,Hundredth conflict
XC-99,cmp,Ninety-ninth conflict
XC-02,cmp,Second conflict
XC-bad,cmp,Unreadable conflict id
";
        let norm = normalize(Register::Conflicts, input.as_bytes()).expect("normalize conflicts");
        let expected = "\
conflict_id,area,topic
XC-02,cmp,Second conflict
XC-99,cmp,Ninety-ninth conflict
XC-100,cmp,Hundredth conflict
XC-bad,cmp,Unreadable conflict id
";
        assert_eq!(norm, expected);

        // Also check claims sorting by (area, number) so CL-CMP-9999 precedes CL-CMP-10000
        let claims_input = "\
claim_id,area,statement
CL-EST-0001,est,Est one
CL-CMP-10000,cmp,Cmp ten thousand
CL-CMP-9999,cmp,Cmp nine thousand nine hundred ninety-nine
";
        let claims_norm =
            normalize(Register::Claims, claims_input.as_bytes()).expect("normalize claims");
        let claims_expected = "\
claim_id,area,statement
CL-CMP-9999,cmp,Cmp nine thousand nine hundred ninety-nine
CL-CMP-10000,cmp,Cmp ten thousand
CL-EST-0001,est,Est one
";
        assert_eq!(claims_norm, claims_expected);
    }

    #[test]
    fn read_errors_carry_their_line() {
        // 1. Non-UTF-8 bytes (with offset)
        let non_utf8 = b"col1,col2\nok,val\nbad,\xff\n";
        let err = read(non_utf8).expect_err("non-UTF-8 must fail");
        assert_eq!(err.kind(), &TableErrorKind::NonUtf8 { offset: 21 });
        assert_eq!(err.line(), 3);
        assert_eq!(err.col(), 5);

        // 2. Unbalanced quote
        let unbalanced = b"col1,col2\nok,val\n\"unclosed,cell\n";
        let err = read(unbalanced).expect_err("unbalanced quote must fail");
        assert_eq!(err.kind(), &TableErrorKind::UnbalancedQuote);
        assert_eq!(err.line(), 3);
        assert_eq!(err.col(), 1);

        // 3. Row cell count differs from header (with both counts)
        let wrong_count = b"col1,col2,col3\n1,2,3\n1,2\n";
        let err = read(wrong_count).expect_err("wrong cell count must fail");
        assert_eq!(
            err.kind(),
            &TableErrorKind::CellCount {
                header_count: 3,
                row_count: 2,
            }
        );
        assert_eq!(err.line(), 3);

        // 4. Line break inside a cell
        let line_break = b"col1,col2\n\"line one\nline two\",val\n";
        let err = read(line_break).expect_err("line break inside a cell must fail");
        assert_eq!(err.kind(), &TableErrorKind::LineBreakInCell);
        assert_eq!(err.line(), 2);
        assert_eq!(err.col(), 10);
    }

    #[test]
    fn write_then_read_returns_the_same_table() {
        let norm = normalize(Register::Questions, EXCEL_SAVED_SAMPLE)
            .expect("Excel sample must normalize");
        let table = read(norm.as_bytes()).expect("read normalized table");
        let written = write(&table);
        let round_trip = read(written.as_bytes()).expect("read written table");
        assert_eq!(round_trip, table);
    }
}

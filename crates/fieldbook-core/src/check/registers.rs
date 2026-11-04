//! Pass `registers`: `FB0200`–`FB0203` (register presence and header columns, CSV syntax,
//! UTF-8 encoding and row keys) (`docs/reference/corpus-format.md#02-registers-and-sources`).

use std::collections::BTreeMap;

use crate::check::{Code, Diagnostic, Model, Options, Report, Tree};
use crate::ids::{ClaimId, ConflictId, QuestionId, SourceId, TermId};
use crate::schema::Register;
use crate::schema::corpus_toml::byte_offset_to_line_col;
use crate::schema::table::{self, Row, Table, TableErrorKind};

/// Return the canonical Corpus path for `register`.
#[must_use]
pub const fn register_path(register: Register) -> &'static str {
    match register {
        Register::Sources => "corpus/registers/sources.csv",
        Register::Claims => "corpus/registers/claims.csv",
        Register::Questions => "corpus/registers/questions.csv",
        Register::Conflicts => "corpus/registers/conflicts.csv",
        Register::Glossary => "corpus/registers/glossary.csv",
    }
}

/// Load the five registers for [`Model`]. A register that fails `FB0200`, `FB0201` or `FB0202`
/// returns `None` and offers no rows to later passes.
#[must_use]
pub(crate) fn load_tables(tree: &Tree) -> [Option<Table>; 5] {
    Register::ALL.map(|reg| check_table_load(tree, reg).ok())
}

/// Run the `registers` check pass (`FB0200`–`FB0203`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    for reg in Register::ALL {
        if let Some(table) = model.register(reg) {
            check_keys(reg, table, options.strict, report);
        } else if let Err((code, line, col, msg)) = check_table_load(model.tree(), reg) {
            report.push(Diagnostic::new(
                code,
                options.strict,
                register_path(reg),
                line,
                col,
                msg,
            ));
        }
    }
}

fn check_table_load(tree: &Tree, reg: Register) -> Result<Table, (Code, usize, usize, String)> {
    let path = register_path(reg);
    let Some(bytes) = tree.get_bytes(path) else {
        return Err((
            Code::Fb0200,
            1,
            1,
            format!("missing register file `{}`", reg.file_name()),
        ));
    };

    // Check UTF-8 validity and header row before reading data rows so a missing or wrong header
    // column raises FB0200 alone instead of also raising FB0201 on every row.
    if let Ok(raw_text) = std::str::from_utf8(bytes) {
        let text = raw_text.strip_prefix('\u{feff}').unwrap_or(raw_text);
        if text.is_empty() {
            return Err((
                Code::Fb0200,
                1,
                1,
                format!(
                    "register `{}` is empty; expected header starting with `{}`",
                    reg.file_name(),
                    reg.columns()[0].name
                ),
            ));
        }

        let first_line_end = text.find('\n').map_or(text.len(), |idx| idx + 1);
        let first_line = &text[..first_line_end];
        if let Ok(header_table) = table::read(first_line.as_bytes())
            && let Some((col, msg)) = check_header(reg, &header_table.header, first_line)
        {
            return Err((Code::Fb0200, 1, col, msg));
        }
    }

    match table::read(bytes) {
        Ok(table) => Ok(table),
        Err(err) => match err.kind() {
            TableErrorKind::NonUtf8 { offset } => {
                let (line, col) = byte_offset_to_line_col_in_bytes(bytes, *offset);
                Err((
                    Code::Fb0202,
                    line,
                    col,
                    format!(
                        "register `{}` is not valid UTF-8 at byte offset {offset}; save as CSV UTF-8",
                        reg.file_name()
                    ),
                ))
            }
            TableErrorKind::UnbalancedQuote
            | TableErrorKind::CellCount { .. }
            | TableErrorKind::LineBreakInCell => Err((
                Code::Fb0201,
                usize::try_from(err.line()).unwrap_or(1),
                usize::try_from(err.col()).unwrap_or(1),
                err.message().to_owned(),
            )),
        },
    }
}

fn byte_offset_to_line_col_in_bytes(bytes: &[u8], offset: usize) -> (usize, usize) {
    let prefix = std::str::from_utf8(&bytes[..offset.min(bytes.len())]).unwrap_or("");
    let prefix = prefix.strip_prefix('\u{feff}').unwrap_or(prefix);
    let (line, col) = byte_offset_to_line_col(prefix, prefix.len());
    (
        usize::try_from(line).unwrap_or(1),
        usize::try_from(col).unwrap_or(1),
    )
}

fn check_header(reg: Register, header: &Row, first_line: &str) -> Option<(usize, String)> {
    let expected = reg.columns();
    let actual = &header.cells;
    let cols = &header.cols;

    for (idx, (act, exp)) in actual.iter().zip(expected.iter()).enumerate() {
        if act != exp.name {
            let col = cols
                .get(idx)
                .copied()
                .and_then(|c| usize::try_from(c).ok())
                .unwrap_or(1);
            return Some((
                col,
                format!(
                    "register `{}` header column {} is `{}`, expected `{}`",
                    reg.file_name(),
                    idx + 1,
                    act,
                    exp.name
                ),
            ));
        }
    }

    if actual.len() > expected.len() {
        let idx = expected.len();
        let col = cols
            .get(idx)
            .copied()
            .and_then(|c| usize::try_from(c).ok())
            .unwrap_or(1);
        return Some((
            col,
            format!(
                "register `{}` header has unexpected column `{}` at position {}",
                reg.file_name(),
                actual[idx],
                idx + 1
            ),
        ));
    }

    if actual.len() < expected.len() {
        let idx = actual.len();
        let trimmed = first_line.trim_end_matches(['\r', '\n']);
        let col = trimmed.chars().count() + 1;
        return Some((
            col,
            format!(
                "register `{}` header is missing column `{}` at position {}",
                reg.file_name(),
                expected[idx].name,
                idx + 1
            ),
        ));
    }

    None
}

fn parse_positive_version(s: &str) -> Option<u32> {
    if s.is_empty() || s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u32>().ok().filter(|&n| n >= 1)
}

fn check_keys(reg: Register, table: &Table, strict: bool, report: &mut Report) {
    let path = register_path(reg);
    match reg {
        Register::Sources => {
            let mut seen: BTreeMap<(SourceId, u32), u32> = BTreeMap::new();
            for row in &table.rows {
                let line = usize::try_from(row.line).unwrap_or(1);
                let source_cell = row.cells.first().map_or("", String::as_str);
                let source_col = row
                    .cols
                    .first()
                    .copied()
                    .and_then(|c| usize::try_from(c).ok())
                    .unwrap_or(1);
                let version_cell = row.cells.get(1).map_or("", String::as_str);
                let version_col = row
                    .cols
                    .get(1)
                    .copied()
                    .and_then(|c| usize::try_from(c).ok())
                    .unwrap_or(1);

                let parsed_source = match SourceId::parse(source_cell) {
                    Ok(id) => Some(id),
                    Err(_) => {
                        report.push(Diagnostic::new(
                            Code::Fb0203,
                            strict,
                            path,
                            line,
                            source_col,
                            format!("malformed `source_id` key `{source_cell}`"),
                        ));
                        None
                    }
                };

                let parsed_version = if let Some(ver) = parse_positive_version(version_cell) {
                    Some(ver)
                } else {
                    report.push(Diagnostic::new(
                        Code::Fb0203,
                        strict,
                        path,
                        line,
                        version_col,
                        format!(
                            "malformed `version` key `{version_cell}`; expected an integer from 1"
                        ),
                    ));
                    None
                };

                if let (Some(source_id), Some(version)) = (parsed_source, parsed_version) {
                    if let Some(&first_line) = seen.get(&(source_id.clone(), version)) {
                        report.push(Diagnostic::new(
                            Code::Fb0203,
                            strict,
                            path,
                            line,
                            source_col,
                            format!(
                                "repeated key `{source_id}` version `{version}` (first on line {first_line})"
                            ),
                        ));
                    } else {
                        seen.insert((source_id, version), row.line);
                    }
                }
            }
        }
        Register::Claims => {
            check_single_key(path, "claim_id", &table.rows, strict, report, |s| {
                ClaimId::parse(s).is_ok()
            });
        }
        Register::Questions => {
            check_single_key(path, "question_id", &table.rows, strict, report, |s| {
                QuestionId::parse(s).is_ok()
            });
        }
        Register::Conflicts => {
            check_single_key(path, "conflict_id", &table.rows, strict, report, |s| {
                ConflictId::parse(s).is_ok()
            });
        }
        Register::Glossary => {
            check_single_key(path, "term_id", &table.rows, strict, report, |s| {
                TermId::parse(s).is_ok()
            });
        }
    }
}

fn check_single_key<F>(
    path: &str,
    col_name: &str,
    rows: &[Row],
    strict: bool,
    report: &mut Report,
    is_valid: F,
) where
    F: Fn(&str) -> bool,
{
    let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
    for row in rows {
        let line = usize::try_from(row.line).unwrap_or(1);
        let key_cell = row.cells.first().map_or("", String::as_str);
        let key_col = row
            .cols
            .first()
            .copied()
            .and_then(|c| usize::try_from(c).ok())
            .unwrap_or(1);

        if !is_valid(key_cell) {
            report.push(Diagnostic::new(
                Code::Fb0203,
                strict,
                path,
                line,
                key_col,
                format!("malformed `{col_name}` key `{key_cell}`"),
            ));
            continue;
        }

        if let Some(&first_line) = seen.get(key_cell) {
            report.push(Diagnostic::new(
                Code::Fb0203,
                strict,
                path,
                line,
                key_col,
                format!("repeated `{col_name}` key `{key_cell}` (first on line {first_line})"),
            ));
        } else {
            seen.insert(key_cell, row.line);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Report, Tree, load, run};
    use crate::date::CivilDate;
    use crate::schema::Register;

    use super::{check, register_path};

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const MINI_ASSISTANT_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const MINI_CHANGES_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const MINI_SOURCES_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const MINI_CLAIMS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const MINI_QUESTIONS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const MINI_CONFLICTS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const MINI_GLOSSARY_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );
        tree
    }

    #[test]
    fn clean_fixture_loads_all_five_registers_into_model() {
        let host = Host::new(mini_tree(), sample_today());
        let (model, report) = load(&host, &Options::default());
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        assert_eq!(model.register_rows(Register::Sources).len(), 4);
        assert_eq!(model.register_rows(Register::Claims).len(), 5);
        assert_eq!(model.register_rows(Register::Questions).len(), 2);
        assert_eq!(model.register_rows(Register::Conflicts).len(), 1);
        assert_eq!(model.register_rows(Register::Glossary).len(), 4);
    }

    #[test]
    fn missing_register_file_raises_fb0200_at_1_1_and_offers_no_rows() {
        let mut tree = Tree::new();
        for (path, entry) in mini_tree().iter() {
            if path != register_path(Register::Questions) {
                tree.insert(path, entry.clone());
            }
        }
        for dir in mini_tree().dirs() {
            tree.insert_dir(dir);
        }

        let (model, report) = load(&Host::new(tree, sample_today()), &Options::default());
        assert!(model.register(Register::Questions).is_none());
        assert!(model.register_rows(Register::Questions).is_empty());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0200);
        assert_eq!(d.path, "corpus/registers/questions.csv");
        assert_eq!((d.line, d.col), (1, 1));
    }

    #[test]
    fn missing_column_in_header_raises_fb0200_once_at_first_differing_cell() {
        // 1. Remove `claim_id,` from the beginning of line 1 in claims.csv (data rows still have 35 cells).
        let without_first = MINI_CLAIMS_CSV.replacen("claim_id,", "", 1);
        let mut tree = mini_tree();
        tree.insert_bytes("corpus/registers/claims.csv", without_first.into_bytes());
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::default());
        assert!(model.register(Register::Claims).is_none());
        assert!(model.register_rows(Register::Claims).is_empty());
        assert_eq!(
            report.diagnostics.len(),
            1,
            "missing header column must raise FB0200 once and not FB0201: {:?}",
            report.diagnostics
        );
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0200);
        assert_eq!(d.path, "corpus/registers/claims.csv");
        assert_eq!((d.line, d.col), (1, 1));

        // 2. Remove the 3rd column (`kind,`) from line 1: first differing cell is column 3 at 1:15.
        let without_third = MINI_CLAIMS_CSV.replacen("claim_id,area,kind,", "claim_id,area,", 1);
        let mut tree2 = mini_tree();
        tree2.insert_bytes("corpus/registers/claims.csv", without_third.into_bytes());
        let (model2, report2) = load(&Host::new(tree2, sample_today()), &Options::default());
        assert!(model2.register_rows(Register::Claims).is_empty());
        assert_eq!(report2.diagnostics.len(), 1, "{:?}", report2.diagnostics);
        let d2 = &report2.diagnostics[0];
        assert_eq!(d2.code, Code::Fb0200);
        assert_eq!(d2.path, "corpus/registers/claims.csv");
        assert_eq!((d2.line, d2.col), (1, 15));
    }

    #[test]
    fn csv_syntax_faults_raise_fb0201_and_offer_no_rows() {
        // 1. Line break inside a cell on line 2.
        let wrapped = MINI_CLAIMS_CSV.replacen(
            "Synthetic compliance reviews occur every 30 days under the synthetic rule.",
            "\"Synthetic compliance reviews occur\nevery 30 days under the synthetic rule.\"",
            1,
        );
        let mut tree = mini_tree();
        tree.insert_bytes("corpus/registers/claims.csv", wrapped.into_bytes());
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::default());
        assert!(model.register(Register::Claims).is_none());
        assert!(model.register_rows(Register::Claims).is_empty());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0201);
        assert_eq!(d.path, "corpus/registers/claims.csv");
        assert_eq!(d.line, 2);

        // 2. Wrong number of cells on a data row.
        let wrong_cells = format!("{MINI_CONFLICTS_CSV}XC-02,cmp,too_few\n");
        let mut tree2 = mini_tree();
        tree2.insert_bytes("corpus/registers/conflicts.csv", wrong_cells.into_bytes());
        let (model2, report2) = load(&Host::new(tree2, sample_today()), &Options::default());
        assert!(model2.register_rows(Register::Conflicts).is_empty());
        assert_eq!(report2.diagnostics.len(), 1, "{:?}", report2.diagnostics);
        assert_eq!(report2.diagnostics[0].code, Code::Fb0201);
        assert_eq!(report2.diagnostics[0].line, 3);

        // 3. Unbalanced quote on a data row.
        let header_line = MINI_GLOSSARY_CSV.lines().next().expect("header");
        let unbalanced =
            format!("{header_line}\nbad_quote,\"unclosed,def,b,cmp,,general_practice,,\n");
        let mut tree3 = mini_tree();
        tree3.insert_bytes("corpus/registers/glossary.csv", unbalanced.into_bytes());
        let (model3, report3) = load(&Host::new(tree3, sample_today()), &Options::default());
        assert!(model3.register_rows(Register::Glossary).is_empty());
        assert_eq!(report3.diagnostics.len(), 1, "{:?}", report3.diagnostics);
        assert_eq!(report3.diagnostics[0].code, Code::Fb0201);
        assert_eq!(
            (report3.diagnostics[0].line, report3.diagnostics[0].col),
            (2, 11)
        );
    }

    #[test]
    fn non_utf8_register_raises_fb0202_with_line_col_and_catalogue_message() {
        let mut bad_bytes = MINI_GLOSSARY_CSV.as_bytes().to_vec();
        // Append a line with a Windows-1252 en-dash byte (0x96) at column 10 of line 6.
        bad_bytes.extend_from_slice(b"term_six,\x96\n");

        let mut tree = mini_tree();
        tree.insert_bytes("corpus/registers/glossary.csv", bad_bytes);
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::default());
        assert!(model.register(Register::Glossary).is_none());
        assert!(model.register_rows(Register::Glossary).is_empty());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0202);
        assert_eq!(d.path, "corpus/registers/glossary.csv");
        assert_eq!((d.line, d.col), (6, 10));
        assert!(
            d.message.contains("save as CSV UTF-8"),
            "expected 'save as CSV UTF-8' in message, got: {}",
            d.message
        );
    }

    #[test]
    fn bom_and_crlf_register_reads_without_a_registers_finding() {
        let crlf_with_bom = format!("\u{feff}{}", MINI_QUESTIONS_CSV.replace('\n', "\r\n"));
        let mut tree = mini_tree();
        tree.insert_bytes("corpus/registers/questions.csv", crlf_with_bom.into_bytes());

        let (model, _) = load(&Host::new(tree, sample_today()), &Options::default());
        let mut reg_report = Report::new();
        check(&model, &Options::default(), &mut reg_report);
        assert!(
            reg_report.diagnostics.is_empty(),
            "expected no findings from registers::check on BOM + CRLF register, got {:?}",
            reg_report.diagnostics
        );
        let rows = model.register_rows(Register::Questions);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].line, 2);
        assert_eq!(rows[0].cols[0], 1);
        assert_eq!(rows[0].cells[0], "Q-001");
        assert_eq!(rows[1].line, 3);
        assert_eq!(rows[1].cells[0], "Q-002");
    }

    #[test]
    fn malformed_and_repeated_keys_raise_fb0203_at_cell_and_second_row() {
        // 1. Claims: duplicate CL-CMP-0001 on line 3, and malformed claim_id on line 4.
        let mut claims_lines: Vec<String> = MINI_CLAIMS_CSV.lines().map(str::to_owned).collect();
        let dup_row = claims_lines[1].clone();
        claims_lines.insert(2, dup_row);
        claims_lines[3] = claims_lines[3].replacen("CL-CMP-0002", "CL-bad", 1);
        let modified_claims = format!("{}\n", claims_lines.join("\n"));

        let mut tree = mini_tree();
        tree.insert_bytes("corpus/registers/claims.csv", modified_claims.into_bytes());
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::default());
        // FB0203 keeps the register rows in the model.
        assert_eq!(model.register_rows(Register::Claims).len(), 6);

        let fb0203: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0203)
            .collect();
        assert_eq!(fb0203.len(), 2, "{:?}", report.diagnostics);
        assert_eq!(fb0203[0].path, "corpus/registers/claims.csv");
        assert_eq!((fb0203[0].line, fb0203[0].col), (3, 1));
        assert!(
            fb0203[0].message.contains("line 2"),
            "duplicate key message must name line of first occurrence: {}",
            fb0203[0].message
        );
        assert_eq!((fb0203[1].line, fb0203[1].col), (4, 1));

        // 2. Sources: malformed version (0) at col 8, and repeated (S-0001, 1) on line 3.
        let mut sources_lines: Vec<String> = MINI_SOURCES_CSV.lines().map(str::to_owned).collect();
        let dup_source = sources_lines[1].clone();
        sources_lines.insert(2, dup_source);
        sources_lines[3] = sources_lines[3].replacen("S-0002,1,", "S-0002,0,", 1);
        let modified_sources = format!("{}\n", sources_lines.join("\n"));

        let mut tree2 = mini_tree();
        tree2.insert_bytes(
            "corpus/registers/sources.csv",
            modified_sources.into_bytes(),
        );
        let report2 = run(&Host::new(tree2, sample_today()), &Options::default());
        let src_fb0203: Vec<_> = report2
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0203)
            .collect();
        assert_eq!(src_fb0203.len(), 2, "{:?}", report2.diagnostics);
        assert_eq!((src_fb0203[0].line, src_fb0203[0].col), (3, 1));
        assert!(src_fb0203[0].message.contains("line 2"));
        assert_eq!((src_fb0203[1].line, src_fb0203[1].col), (4, 8));
    }
}

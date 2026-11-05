//! Verification that every file of `tests/fixtures/corpus-mini` parses cleanly
//! and every register is in canonical normal form.

use fieldbook_core::date::CivilDate;
use fieldbook_core::schema::rows::CellProblem;
use fieldbook_core::schema::table::{self, Row};
use fieldbook_core::schema::{
    ClaimRow, ConflictRow, CorpusToml, GlossaryRow, QuestionRow, Register, SourceRow, parse_meta,
    split,
};

const TODAY_RAW: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");
const CORPUS_TOML_RAW: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

const SOURCES_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
const CLAIMS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
const QUESTIONS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
const CONFLICTS_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
const GLOSSARY_CSV: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");

const PAGES: [(&str, &str); 9] = [
    (
        "corpus/pages/compliance/business-systems/cmp-clause-9900-101.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-clause-9900-101.md"
        ),
    ),
    (
        "corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md"
        ),
    ),
    (
        "corpus/pages/compliance/business-systems/cmp-review-steps.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
        ),
    ),
    (
        "corpus/pages/compliance/business-systems/cmp-system-review.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
        ),
    ),
    (
        "corpus/pages/compliance/business-systems/cmp-withhold-case.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-withhold-case.md"
        ),
    ),
    (
        "corpus/pages/estimating/pricing/est-baseline-template.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-baseline-template.md"
        ),
    ),
    (
        "corpus/pages/estimating/pricing/est-calc-01.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-calc-01.md"
        ),
    ),
    (
        "corpus/pages/estimating/pricing/est-rate-reference.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-reference.md"
        ),
    ),
    (
        "corpus/pages/estimating/pricing/est-rate-window-brief.md",
        include_str!(
            "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
        ),
    ),
];

fn check_register<T, F>(
    register: Register,
    raw: &str,
    parse_row: F,
    expected_rows: usize,
) -> Result<(), String>
where
    F: Fn(&Row) -> Result<T, Vec<CellProblem>>,
{
    let name = register.file_name();
    let tbl = table::read(raw.as_bytes()).map_err(|err| format!("{name}:{err}"))?;
    let expected_header: Vec<String> = register
        .columns()
        .iter()
        .map(|col| col.name.to_owned())
        .collect();
    if tbl.header.cells != expected_header {
        return Err(format!(
            "{name}:1:1: header columns do not match Register::{register:?}"
        ));
    }
    let mut row_problems = Vec::new();
    for row in &tbl.rows {
        if let Err(problems) = parse_row(row) {
            for p in problems {
                let col_idx = register
                    .columns()
                    .iter()
                    .position(|col| col.name == p.column)
                    .unwrap_or(0);
                let col_num = row.cols.get(col_idx).copied().unwrap_or(1);
                row_problems.push(format!(
                    "{name}:{}:{}: {} ({:?})",
                    row.line, col_num, p.column, p.kind
                ));
            }
        }
    }
    if !row_problems.is_empty() {
        return Err(row_problems.join("\n"));
    }
    if tbl.rows.len() != expected_rows {
        return Err(format!(
            "{name}: expected {expected_rows} rows, found {}",
            tbl.rows.len()
        ));
    }
    Ok(())
}

#[test]
fn every_register_row_parses() {
    let corpus = CorpusToml::parse(CORPUS_TOML_RAW).expect("corpus.toml should parse");
    let problems = corpus.validate();
    assert!(
        problems.is_empty(),
        "corpus.toml validation problems: {problems:?}"
    );
    assert_eq!(corpus.areas().len(), 2, "expected 2 areas in corpus.toml");
    assert_eq!(corpus.views().len(), 1, "expected 1 view in corpus.toml");

    assert_eq!(PAGES.len(), 9, "expected 9 pages in fixture");
    for (path, raw) in PAGES {
        let sp = split(raw).unwrap_or_else(|err| panic!("{path}:{err}"));
        let meta = parse_meta(&sp.front_matter).unwrap_or_else(|err| panic!("{path}:{err}"));
        assert!(
            !sp.body.trim().is_empty(),
            "{path}:{}: body should not be empty",
            sp.body_line
        );
        assert_eq!(
            meta.status.as_str(),
            "draft",
            "{path}: expected draft status"
        );
    }

    if let Err(msg) = check_register(Register::Sources, SOURCES_CSV, SourceRow::from_row, 4) {
        panic!("{msg}");
    }
    if let Err(msg) = check_register(Register::Claims, CLAIMS_CSV, ClaimRow::from_row, 5) {
        panic!("{msg}");
    }
    if let Err(msg) = check_register(Register::Questions, QUESTIONS_CSV, QuestionRow::from_row, 2) {
        panic!("{msg}");
    }
    if let Err(msg) = check_register(Register::Conflicts, CONFLICTS_CSV, ConflictRow::from_row, 1) {
        panic!("{msg}");
    }
    if let Err(msg) = check_register(Register::Glossary, GLOSSARY_CSV, GlossaryRow::from_row, 4) {
        panic!("{msg}");
    }
}

#[test]
fn registers_are_in_normal_form() {
    for (register, raw) in [
        (Register::Sources, SOURCES_CSV),
        (Register::Claims, CLAIMS_CSV),
        (Register::Questions, QUESTIONS_CSV),
        (Register::Conflicts, CONFLICTS_CSV),
        (Register::Glossary, GLOSSARY_CSV),
    ] {
        assert!(
            table::is_normal(register, raw.as_bytes()),
            "{} is not in canonical normal form",
            register.file_name()
        );
    }
}

#[test]
fn today_is_a_civil_date() {
    assert!(TODAY_RAW.ends_with('\n'), "TODAY must end with a newline");
    let trimmed = TODAY_RAW.strip_suffix('\n').unwrap_or(TODAY_RAW);
    assert!(
        !trimmed.contains('\n') && !trimmed.contains('\r'),
        "TODAY must hold exactly one line and one final newline"
    );
    let today = CivilDate::parse(trimmed).expect("TODAY must be a valid CivilDate");
    assert_eq!(today.to_string(), trimmed);
}

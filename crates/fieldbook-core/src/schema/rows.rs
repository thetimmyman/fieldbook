//! Shared cell-parsing helpers, [`CellProblem`], and column round-trip tests for register rows.
//!
//! **Responsibility.** Provide the per-cell parsers (`text`, `list`, `bool`, `int`,
//! `date`, `pdate`, `id`, `enum`) shared by [`SourceRow`](super::sources::SourceRow),
//! [`QuestionRow`](super::questions::QuestionRow),
//! [`ConflictRow`](super::conflicts::ConflictRow),
//! [`GlossaryRow`](super::glossary::GlossaryRow), and (in `core-schema-claims`)
//! `ClaimRow`.
//!
//! **Rules.** Pure: checks one cell at a time and leaves cross-column rules to
//! `check-register-cells`.

use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::date::{CivilDate, PartialDate};
use crate::ids::{ClaimId, IdError, VersionId};
use crate::schema::table::Row;
use crate::schema::vocab::VocabError;

/// Default maximum length in Unicode scalar values (`char`s) for a register `text` cell.
pub const MAX_TEXT_CHARS: usize = 1_000;

/// Maximum length in characters for a `quote` cell (`claims.csv`).
pub const MAX_QUOTE_CHARS: usize = 300;

/// Maximum length in characters for a glossary `definition` cell (`glossary.csv`).
pub const MAX_DEFINITION_CHARS: usize = 400;

/// Maximum length in characters for a glossary `boundary_rule` cell (`glossary.csv`).
pub const MAX_BOUNDARY_RULE_CHARS: usize = 300;

/// Maximum length in characters for a source `tag` cell (`sources.csv`).
pub const MAX_TAG_CHARS: usize = 40;

/// Classification of a single-cell validation failure in a register row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellProblemKind {
    /// A required cell (`ColumnNeed::Yes`) is empty.
    Empty,
    /// A cell does not match its required syntax or contains `|` outside a list column.
    Form,
    /// A `date` or `pdate` cell is malformed, impossible on the calendar, or partial where a full date is required.
    CalendarDate,
    /// An `enum` cell holds a value outside its closed vocabulary.
    Vocabulary,
    /// A `text` cell (or text list item) exceeds its character limit.
    TooLong,
}

/// A validation problem on one cell of a register row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellProblem {
    /// Name of the register column that failed.
    pub column: &'static str,
    /// Classification of the cell problem.
    pub kind: CellProblemKind,
}

impl CellProblem {
    /// Constructs a new [`CellProblem`].
    #[must_use]
    pub const fn new(column: &'static str, kind: CellProblemKind) -> Self {
        Self { column, kind }
    }

    /// Returns the column name.
    #[must_use]
    pub const fn column(&self) -> &'static str {
        self.column
    }

    /// Returns the problem kind.
    #[must_use]
    pub const fn kind(&self) -> CellProblemKind {
        self.kind
    }
}

impl fmt::Display for CellProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.column, self.kind)
    }
}

impl std::error::Error for CellProblem {}

/// A reference in `conflicts.csv` (`evidence`) or `glossary.csv` (`source_ref`),
/// naming either a [`ClaimId`] or a source [`VersionId`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RefId {
    /// A claim identifier (`CL-AAA-nnnn`).
    Claim(ClaimId),
    /// A source version identifier (`S-nnnn-vN`).
    SourceVersion(VersionId),
}

impl RefId {
    /// Parses either a [`ClaimId`] or a [`VersionId`].
    pub fn parse(input: &str) -> Result<Self, IdError> {
        if let Ok(c) = ClaimId::parse(input) {
            return Ok(Self::Claim(c));
        }
        if let Ok(v) = VersionId::parse(input) {
            return Ok(Self::SourceVersion(v));
        }
        Err(IdError::invalid("ref_id", input))
    }

    /// Returns the canonical string slice of the identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Claim(c) => c.as_str(),
            Self::SourceVersion(v) => v.as_str(),
        }
    }
}

impl fmt::Display for RefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for RefId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for RefId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

/// Safely retrieves the text of cell `idx` from `row`, returning `""` if missing.
#[must_use]
pub fn cell_at(row: &Row, idx: usize) -> &str {
    row.cells.get(idx).map_or("", String::as_str)
}

/// Parses a required text cell with a maximum character limit (`max_chars`),
/// rejecting empty cells (`Empty`), over-limit cells (`TooLong`), and `|` (`Form`).
pub fn req_text(
    row: &Row,
    idx: usize,
    column: &'static str,
    max_chars: usize,
    problems: &mut Vec<CellProblem>,
) -> Option<String> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    if raw.chars().count() > max_chars {
        problems.push(CellProblem::new(column, CellProblemKind::TooLong));
        return None;
    }
    if raw.contains('|') {
        problems.push(CellProblem::new(column, CellProblemKind::Form));
        return None;
    }
    Some(raw.to_owned())
}

/// Parses an optional text cell with a maximum character limit (`max_chars`).
pub fn opt_text(
    row: &Row,
    idx: usize,
    column: &'static str,
    max_chars: usize,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<String>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(None);
    }
    req_text(row, idx, column, max_chars, problems).map(Some)
}

/// Parses a required 64-character lowercase hexadecimal SHA-256 cell (`sha256`).
pub fn req_sha256(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<String> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    if raw.chars().count() > MAX_TEXT_CHARS {
        problems.push(CellProblem::new(column, CellProblemKind::TooLong));
        return None;
    }
    let valid_hex = raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !valid_hex {
        problems.push(CellProblem::new(column, CellProblemKind::Form));
        return None;
    }
    Some(raw.to_owned())
}

fn is_snake_case(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some((&first, _)) = bytes.split_first() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    s.split('_').all(|seg| {
        !seg.is_empty()
            && seg
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    })
}

/// Parses an optional snake_case text cell (`source_family`).
pub fn opt_snake_case_text(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<String>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(None);
    }
    if raw.chars().count() > MAX_TEXT_CHARS {
        problems.push(CellProblem::new(column, CellProblemKind::TooLong));
        return None;
    }
    if !is_snake_case(raw) {
        problems.push(CellProblem::new(column, CellProblemKind::Form));
        return None;
    }
    Some(Some(raw.to_owned()))
}

/// Splits a non-empty pipe-separated list cell, rejecting empty items and spaces
/// immediately before or after `|`, while preserving internal spaces.
fn split_pipe_items<'a>(raw: &'a str, column: &'static str) -> Result<Vec<&'a str>, CellProblem> {
    let parts: Vec<&'a str> = raw.split('|').collect();
    for (i, &part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(CellProblem::new(column, CellProblemKind::Form));
        }
        if i + 1 < parts.len() && part.ends_with(char::is_whitespace) {
            return Err(CellProblem::new(column, CellProblemKind::Form));
        }
        if i > 0 && part.starts_with(char::is_whitespace) {
            return Err(CellProblem::new(column, CellProblemKind::Form));
        }
    }
    Ok(parts)
}

/// Parses a required list of text items (`positions` in `conflicts.csv`),
/// requiring at least `min_items` items and at most `MAX_TEXT_CHARS` characters per item.
pub fn req_text_list(
    row: &Row,
    idx: usize,
    column: &'static str,
    min_items: usize,
    problems: &mut Vec<CellProblem>,
) -> Option<Vec<String>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    let parts = match split_pipe_items(raw, column) {
        Ok(p) => p,
        Err(prob) => {
            problems.push(prob);
            return None;
        }
    };
    if parts.len() < min_items {
        problems.push(CellProblem::new(column, CellProblemKind::Form));
        return None;
    }
    for &part in &parts {
        if part.chars().count() > MAX_TEXT_CHARS {
            problems.push(CellProblem::new(column, CellProblemKind::TooLong));
            return None;
        }
    }
    Some(parts.into_iter().map(str::to_owned).collect())
}

/// Parses an optional list of text items (`aliases` in `glossary.csv`).
pub fn opt_text_list(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<Vec<String>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(Vec::new());
    }
    req_text_list(row, idx, column, 1, problems)
}

/// Parses a required list of typed IDs (`claim_ids` in `conflicts.csv`).
pub fn req_id_list<T>(
    row: &Row,
    idx: usize,
    column: &'static str,
    parse_fn: fn(&str) -> Result<T, IdError>,
    problems: &mut Vec<CellProblem>,
) -> Option<Vec<T>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    let parts = match split_pipe_items(raw, column) {
        Ok(p) => p,
        Err(prob) => {
            problems.push(prob);
            return None;
        }
    };
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        match parse_fn(part) {
            Ok(val) => out.push(val),
            Err(_) => {
                problems.push(CellProblem::new(column, CellProblemKind::Form));
                return None;
            }
        }
    }
    Some(out)
}

/// Parses an optional list of typed IDs (`supersedes`, `superseded_by`, `evidence`, `source_ref`).
pub fn opt_id_list<T>(
    row: &Row,
    idx: usize,
    column: &'static str,
    parse_fn: fn(&str) -> Result<T, IdError>,
    problems: &mut Vec<CellProblem>,
) -> Option<Vec<T>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(Vec::new());
    }
    req_id_list(row, idx, column, parse_fn, problems)
}

/// Parses a required boolean cell (`true` or `false`).
pub fn req_bool(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<bool> {
    let raw = cell_at(row, idx);
    match raw {
        "" => {
            problems.push(CellProblem::new(column, CellProblemKind::Empty));
            None
        }
        "true" => Some(true),
        "false" => Some(false),
        _ => {
            problems.push(CellProblem::new(column, CellProblemKind::Form));
            None
        }
    }
}

/// Parses a required positive integer (`1, 2, ...`).
pub fn req_positive_u32(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<u32> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    if raw.starts_with('0') || !raw.bytes().all(|b| b.is_ascii_digit()) {
        problems.push(CellProblem::new(column, CellProblemKind::Form));
        return None;
    }
    match raw.parse::<u32>() {
        Ok(n) if n >= 1 => Some(n),
        _ => {
            problems.push(CellProblem::new(column, CellProblemKind::Form));
            None
        }
    }
}

/// Parses an optional positive integer (`1, 2, ...`).
pub fn opt_positive_u32(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<u32>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(None);
    }
    req_positive_u32(row, idx, column, problems).map(Some)
}

/// Parses a required full ISO 8601 calendar date (`YYYY-MM-DD`).
/// Any malformed, impossible, or partial date produces [`CellProblemKind::CalendarDate`].
pub fn req_date(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<CivilDate> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    match CivilDate::parse(raw) {
        Ok(d) => Some(d),
        Err(_) => {
            problems.push(CellProblem::new(column, CellProblemKind::CalendarDate));
            None
        }
    }
}

/// Parses an optional full ISO 8601 calendar date (`YYYY-MM-DD`).
pub fn opt_date(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<CivilDate>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(None);
    }
    req_date(row, idx, column, problems).map(Some)
}

/// Parses a required partial or full ISO 8601 date (`YYYY`, `YYYY-MM`, or `YYYY-MM-DD`).
pub fn req_pdate(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<PartialDate> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    match PartialDate::parse(raw) {
        Ok(d) => Some(d),
        Err(_) => {
            problems.push(CellProblem::new(column, CellProblemKind::CalendarDate));
            None
        }
    }
}

/// Parses an optional partial or full ISO 8601 date (`YYYY`, `YYYY-MM`, or `YYYY-MM-DD`).
pub fn opt_pdate(
    row: &Row,
    idx: usize,
    column: &'static str,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<PartialDate>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(None);
    }
    req_pdate(row, idx, column, problems).map(Some)
}

/// Parses a required identifier cell.
pub fn req_id<T>(
    row: &Row,
    idx: usize,
    column: &'static str,
    parse_fn: fn(&str) -> Result<T, IdError>,
    problems: &mut Vec<CellProblem>,
) -> Option<T> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    match parse_fn(raw) {
        Ok(id) => Some(id),
        Err(_) => {
            problems.push(CellProblem::new(column, CellProblemKind::Form));
            None
        }
    }
}

/// Parses an optional identifier cell.
pub fn opt_id<T>(
    row: &Row,
    idx: usize,
    column: &'static str,
    parse_fn: fn(&str) -> Result<T, IdError>,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<T>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(None);
    }
    req_id(row, idx, column, parse_fn, problems).map(Some)
}

/// Parses a required closed-vocabulary enum cell.
pub fn req_vocab<T>(
    row: &Row,
    idx: usize,
    column: &'static str,
    parse_fn: fn(&str) -> Result<T, VocabError>,
    problems: &mut Vec<CellProblem>,
) -> Option<T> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        problems.push(CellProblem::new(column, CellProblemKind::Empty));
        return None;
    }
    match parse_fn(raw) {
        Ok(val) => Some(val),
        Err(_) => {
            problems.push(CellProblem::new(column, CellProblemKind::Vocabulary));
            None
        }
    }
}

/// Parses an optional closed-vocabulary enum cell.
pub fn opt_vocab<T>(
    row: &Row,
    idx: usize,
    column: &'static str,
    parse_fn: fn(&str) -> Result<T, VocabError>,
    problems: &mut Vec<CellProblem>,
) -> Option<Option<T>> {
    let raw = cell_at(row, idx);
    if raw.is_empty() {
        return Some(None);
    }
    req_vocab(row, idx, column, parse_fn, problems).map(Some)
}

/// Joins a slice of items implementing `Display` with `|`.
#[must_use]
pub fn join_pipe<T: fmt::Display>(items: &[T]) -> String {
    let mut out = String::new();
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            out.push('|');
        }
        out.push_str(&item.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::claims::ClaimRow;
    use crate::schema::conflicts::ConflictRow;
    use crate::schema::glossary::GlossaryRow;
    use crate::schema::questions::QuestionRow;
    use crate::schema::register::Register;
    use crate::schema::sources::SourceRow;

    fn make_row(cells: Vec<String>) -> Row {
        let cols = (1..=u32::try_from(cells.len()).unwrap_or(1)).collect();
        Row {
            line: 2,
            cols,
            cells,
        }
    }

    const SOURCES_SAMPLES: [(&str, &str); 28] = [
        ("source_id", "S-0042"),
        ("version", "2"),
        ("tag", "FAR 15 codified"),
        ("title", "48 CFR Part 15 Contracting by Negotiation"),
        ("publisher", "Office of the Federal Register"),
        ("source_kind", "primary_law"),
        ("source_role", "primary_evidence"),
        ("authority_tier", "A"),
        ("licence", "US Government work, public domain"),
        ("source_family", "far_part_15"),
        ("family_basis", "reviewed"),
        ("location", "S-0042-v2.pdf"),
        (
            "sha256",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ),
        ("published", "2025-10"),
        ("effective_from", "2025-10-15"),
        ("as_of", "2025-11-01"),
        ("retrieved", "2025-11-02"),
        ("lifecycle", "active"),
        ("supersedes", "S-0042-v1"),
        ("volatility", "V1"),
        ("handling", "public"),
        ("note", "Synthetic sample note for round-trip verification."),
        ("purpose", "Answer pricing and cost analysis questions."),
        ("origin_system", "erp-core"),
        ("origin_stream", "commitments"),
        ("origin_key", "PO-2025-0042"),
        ("doc_kind", "quote"),
        ("series", "commitments"),
    ];

    const QUESTIONS_SAMPLES: [(&str, &str); 13] = [
        ("question_id", "Q-027"),
        ("area", "cmp"),
        (
            "question",
            "Which business systems are in scope for the first compliance review?",
        ),
        ("owner_role", "Finance lead"),
        ("blocks", "Compliance checklist sign-off"),
        (
            "why_it_matters",
            "Determines withholding exposure across active contracts.",
        ),
        (
            "default_assumption",
            "Assume all six DFARS business systems require baseline mapping.",
        ),
        ("work_can_continue", "partly"),
        ("day_one", "true"),
        ("status", "open"),
        ("answer", "Accounting and estimating systems are first."),
        ("answered_on", "2025-11-03"),
        ("adoption", "proposed"),
    ];

    const CONFLICTS_SAMPLES: [(&str, &str); 11] = [
        ("conflict_id", "XC-14"),
        ("area", "cmp"),
        ("topic", "Withholding cap percentage on single deficiency"),
        ("positions", "Cap is five percent|Cap is ten percent"),
        ("resolution", "Use five percent for a single deficiency."),
        ("rationale", "Primary clause text controls over summary."),
        ("evidence", "CL-CMP-0011|S-0042-v2"),
        ("resolved_on", "2025-11-02"),
        ("status", "resolved"),
        ("claim_ids", "CL-CMP-0011|CL-CMP-0012"),
        ("question_id", "Q-027"),
    ];

    const GLOSSARY_SAMPLES: [(&str, &str); 9] = [
        ("term_id", "material_weakness"),
        ("label", "material weakness"),
        (
            "definition",
            "A deficiency or combination of deficiencies in internal control.",
        ),
        (
            "boundary_rule",
            "Distinct from a significant deficiency with lower severity.",
        ),
        ("owner_area", "cmp"),
        ("source_ref", "S-0042-v2|CL-CMP-0011"),
        ("definition_status", "public_reference"),
        ("aliases", "MW|material control weakness"),
        ("superseded_by", "significant_deficiency"),
    ];

    const CLAIMS_SAMPLES: [(&str, &str); 35] = [
        ("claim_id", "CL-CMP-0011"),
        ("area", "cmp"),
        ("kind", "fact"),
        (
            "statement",
            "Single-system payment withholding is capped at five percent.",
        ),
        ("as_of", "2025-11-01"),
        ("source_id", "S-0042"),
        ("version", "2"),
        ("locator_kind", "pdf_page_box"),
        ("locator", "p. 14"),
        ("quote", "five percent for a single business system"),
        ("consequential", "true"),
        ("drafted_by", "codex"),
        ("verification", "unchecked"),
        ("applicability", "pending_company_confirmation"),
        ("adoption", "proposed"),
        ("data_origin", "unknown"),
        ("volatility", "V1"),
        ("value", "5"),
        ("unit", "percent"),
        ("effective_from", "2025-10-15"),
        ("effective_to", "2026-10-15"),
        ("instrument", "dfars_codified"),
        ("citation_codified", "48 CFR 252.242-7005"),
        ("citation_rfo", "RFO-252-7005"),
        ("edition_date", "2025-10"),
        ("text_status", "final"),
        (
            "applies_to",
            "event=award_date; entered_from=2025-01-01; contract_type=fixed_price",
        ),
        ("evidence_grade", "E"),
        ("verified_on", "2025-11-04"),
        ("verified_by_role", "owner"),
        ("supersedes", "CL-CMP-0009"),
        ("superseded_by", "CL-CMP-0012"),
        ("conflict_id", "XC-14"),
        ("question_id", "Q-027"),
        ("note", "Synthetic claim note for column round-trip test."),
    ];

    fn build_sample_cells(reg: Register, table: &[(&str, &str)]) -> Vec<String> {
        let cols = reg.columns();
        let mut out = Vec::with_capacity(cols.len());
        for col in cols {
            let Some(&(_, sample)) = table.iter().find(|&&(name, _)| name == col.name) else {
                panic!(
                    "missing sample for column `{}` in register {}",
                    col.name,
                    reg.file_name()
                );
            };
            assert!(
                !sample.is_empty(),
                "sample for column `{}` in {} must be non-empty",
                col.name,
                reg.file_name()
            );
            out.push(sample.to_owned());
        }
        out
    }

    fn assert_cells_match(reg: Register, expected: &[String], actual: &[String]) {
        let cols = reg.columns();
        assert_eq!(
            actual.len(),
            expected.len(),
            "cell count mismatch in {}",
            reg.file_name()
        );
        for (idx, (exp, act)) in expected.iter().zip(actual.iter()).enumerate() {
            let col_name = cols.get(idx).map_or("unknown", |c| c.name);
            assert_eq!(
                act,
                exp,
                "round-trip mismatch on column `{col_name}` in {}",
                reg.file_name()
            );
        }
    }

    #[test]
    fn every_column_survives_from_row_to_cells() {
        // 1. Sources (28 columns)
        assert_eq!(Register::Sources.columns().len(), 28);
        let src_cells = build_sample_cells(Register::Sources, &SOURCES_SAMPLES);
        let src_row = SourceRow::from_row(&make_row(src_cells.clone()))
            .expect("valid synthetic SourceRow must parse");
        assert_cells_match(Register::Sources, &src_cells, &src_row.to_cells());

        // 2. Questions (13 columns)
        let q_cells = build_sample_cells(Register::Questions, &QUESTIONS_SAMPLES);
        let q_row = QuestionRow::from_row(&make_row(q_cells.clone()))
            .expect("valid synthetic QuestionRow must parse");
        assert_cells_match(Register::Questions, &q_cells, &q_row.to_cells());

        // 3. Conflicts (11 columns)
        let c_cells = build_sample_cells(Register::Conflicts, &CONFLICTS_SAMPLES);
        let c_row = ConflictRow::from_row(&make_row(c_cells.clone()))
            .expect("valid synthetic ConflictRow must parse");
        assert_cells_match(Register::Conflicts, &c_cells, &c_row.to_cells());

        // 4. Glossary (9 columns)
        let g_cells = build_sample_cells(Register::Glossary, &GLOSSARY_SAMPLES);
        let g_row = GlossaryRow::from_row(&make_row(g_cells.clone()))
            .expect("valid synthetic GlossaryRow must parse");
        assert_cells_match(Register::Glossary, &g_cells, &g_row.to_cells());

        // 5. Claims (35 columns, all distinct non-empty samples)
        assert_eq!(Register::Claims.columns().len(), 35);
        let cl_cells = build_sample_cells(Register::Claims, &CLAIMS_SAMPLES);
        let unique_samples: std::collections::BTreeSet<&str> =
            cl_cells.iter().map(String::as_str).collect();
        assert_eq!(
            unique_samples.len(),
            35,
            "all 35 claim sample cells must be distinct"
        );
        let cl_row = ClaimRow::from_row(&make_row(cl_cells.clone()))
            .expect("valid synthetic ClaimRow must parse");
        assert_cells_match(Register::Claims, &cl_cells, &cl_row.to_cells());
    }

    #[test]
    fn a_text_cell_over_the_limit_is_too_long() {
        let mut src_cells = build_sample_cells(Register::Sources, &SOURCES_SAMPLES);
        let note_idx = Register::Sources
            .columns()
            .iter()
            .position(|c| c.name == "note")
            .expect("note column");
        src_cells[note_idx] = "a".repeat(1_001);
        let errs =
            SourceRow::from_row(&make_row(src_cells)).expect_err("1,001-char note must fail");
        assert_eq!(
            errs,
            vec![CellProblem::new("note", CellProblemKind::TooLong)]
        );
    }

    #[test]
    fn list_items_preserve_internal_spaces() {
        // ConflictRow with `Price is fixed|Price is adjustable`
        let mut c_cells = build_sample_cells(Register::Conflicts, &CONFLICTS_SAMPLES);
        let pos_idx = Register::Conflicts
            .columns()
            .iter()
            .position(|c| c.name == "positions")
            .expect("positions column");
        c_cells[pos_idx] = "Price is fixed|Price is adjustable".to_owned();
        let parsed_c = ConflictRow::from_row(&make_row(c_cells.clone()))
            .expect("ConflictRow with internal spaces in positions must succeed");
        assert_eq!(
            parsed_c.positions,
            vec![
                "Price is fixed".to_owned(),
                "Price is adjustable".to_owned()
            ]
        );
        assert_eq!(parsed_c.to_cells(), c_cells);

        // Reject space before or after `|` in ConflictRow
        let mut c_space_before = c_cells.clone();
        c_space_before[pos_idx] = "Price is fixed |Price is adjustable".to_owned();
        let err_before = ConflictRow::from_row(&make_row(c_space_before))
            .expect_err("space before bar must fail");
        assert_eq!(
            err_before,
            vec![CellProblem::new("positions", CellProblemKind::Form)]
        );

        let mut c_space_after = c_cells;
        c_space_after[pos_idx] = "Price is fixed| Price is adjustable".to_owned();
        let err_after =
            ConflictRow::from_row(&make_row(c_space_after)).expect_err("space after bar must fail");
        assert_eq!(
            err_after,
            vec![CellProblem::new("positions", CellProblemKind::Form)]
        );

        // GlossaryRow with `material weakness|MW`
        let mut g_cells = build_sample_cells(Register::Glossary, &GLOSSARY_SAMPLES);
        let alias_idx = Register::Glossary
            .columns()
            .iter()
            .position(|c| c.name == "aliases")
            .expect("aliases column");
        g_cells[alias_idx] = "material weakness|MW".to_owned();
        let parsed_g = GlossaryRow::from_row(&make_row(g_cells.clone()))
            .expect("GlossaryRow with internal spaces in aliases must succeed");
        assert_eq!(
            parsed_g.aliases,
            vec!["material weakness".to_owned(), "MW".to_owned()]
        );
        assert_eq!(parsed_g.to_cells(), g_cells);

        let mut g_space_before = g_cells.clone();
        g_space_before[alias_idx] = "material weakness |MW".to_owned();
        assert_eq!(
            GlossaryRow::from_row(&make_row(g_space_before))
                .expect_err("space before bar in aliases must fail"),
            vec![CellProblem::new("aliases", CellProblemKind::Form)]
        );

        let mut g_space_after = g_cells;
        g_space_after[alias_idx] = "material weakness| MW".to_owned();
        assert_eq!(
            GlossaryRow::from_row(&make_row(g_space_after))
                .expect_err("space after bar in aliases must fail"),
            vec![CellProblem::new("aliases", CellProblemKind::Form)]
        );
    }
}

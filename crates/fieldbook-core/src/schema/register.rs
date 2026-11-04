//! Column definitions and metadata for the five Corpus CSV registers.
//!
//! **Responsibility.** Enumerate the five registers (`sources.csv`, `claims.csv`,
//! `questions.csv`, `conflicts.csv`, `glossary.csv`), their file names, and their
//! ordered column schemas (`Column { name, kind, need }`).
//!
//! **Rules.** Pure data: no I/O. Tested against `docs/reference/corpus-format.md`.

use std::fmt;

/// The five CSV registers in `corpus/registers/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Register {
    /// `sources.csv` (28 columns).
    Sources,
    /// `claims.csv` (35 columns).
    Claims,
    /// `questions.csv` (13 columns).
    Questions,
    /// `conflicts.csv` (11 columns).
    Conflicts,
    /// `glossary.csv` (9 columns).
    Glossary,
}

/// The value type of a register column as given in the reference tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnKind {
    /// An identifier form (`id`).
    Id,
    /// A closed status or classification vocabulary (`enum`).
    Enum,
    /// Free or bounded plain text (`text`).
    Text,
    /// A positive integer (`int`).
    Int,
    /// Full ISO 8601 calendar date `YYYY-MM-DD` (`date`).
    Date,
    /// Partial or full ISO 8601 date `YYYY`, `YYYY-MM` or `YYYY-MM-DD` (`pdate`).
    Pdate,
    /// Exact decimal (`decimal`).
    Decimal,
    /// Boolean `true` or `false` (`bool`).
    Bool,
    /// Pipe-separated list (`list`).
    List,
}

impl ColumnKind {
    /// Returns the reference table keyword for this column kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Enum => "enum",
            Self::Text => "text",
            Self::Int => "int",
            Self::Date => "date",
            Self::Pdate => "pdate",
            Self::Decimal => "decimal",
            Self::Bool => "bool",
            Self::List => "list",
        }
    }

    /// Parses a column type keyword from the reference table.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "id" => Some(Self::Id),
            "enum" => Some(Self::Enum),
            "text" => Some(Self::Text),
            "int" => Some(Self::Int),
            "date" => Some(Self::Date),
            "pdate" => Some(Self::Pdate),
            "decimal" => Some(Self::Decimal),
            "bool" => Some(Self::Bool),
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

impl fmt::Display for ColumnKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether a register column is unconditionally required, optional, or governed by a cross-column rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnNeed {
    /// Required on every row (`yes` in the reference).
    Yes,
    /// Optional (`no` in the reference).
    No,
    /// Conditional on another column or mode (any other text in the Required cell).
    Rule,
}

/// One column in a register's fixed header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    /// Exact column header name.
    pub name: &'static str,
    /// Column value type.
    pub kind: ColumnKind,
    /// Whether the cell is required, optional, or rule-governed.
    pub need: ColumnNeed,
}

impl Column {
    const fn new(name: &'static str, kind: ColumnKind, need: ColumnNeed) -> Self {
        Self { name, kind, need }
    }
}

const SOURCES_COLUMNS: [Column; 28] = [
    Column::new("source_id", ColumnKind::Id, ColumnNeed::Yes),
    Column::new("version", ColumnKind::Int, ColumnNeed::Yes),
    Column::new("tag", ColumnKind::Text, ColumnNeed::No),
    Column::new("title", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("publisher", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("source_kind", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("source_role", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("authority_tier", ColumnKind::Enum, ColumnNeed::Rule),
    Column::new("licence", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("source_family", ColumnKind::Text, ColumnNeed::No),
    Column::new("family_basis", ColumnKind::Enum, ColumnNeed::Rule),
    Column::new("location", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("sha256", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("published", ColumnKind::Pdate, ColumnNeed::No),
    Column::new("effective_from", ColumnKind::Pdate, ColumnNeed::No),
    Column::new("as_of", ColumnKind::Pdate, ColumnNeed::Yes),
    Column::new("retrieved", ColumnKind::Date, ColumnNeed::Yes),
    Column::new("lifecycle", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("supersedes", ColumnKind::List, ColumnNeed::No),
    Column::new("volatility", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("handling", ColumnKind::List, ColumnNeed::Yes),
    Column::new("note", ColumnKind::Text, ColumnNeed::No),
    Column::new("purpose", ColumnKind::Text, ColumnNeed::No),
    Column::new("origin_system", ColumnKind::Text, ColumnNeed::No),
    Column::new("origin_stream", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("origin_key", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("doc_kind", ColumnKind::Enum, ColumnNeed::No),
    Column::new("series", ColumnKind::Text, ColumnNeed::No),
];

const CLAIMS_COLUMNS: [Column; 35] = [
    Column::new("claim_id", ColumnKind::Id, ColumnNeed::Yes),
    Column::new("area", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("kind", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("statement", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("as_of", ColumnKind::Pdate, ColumnNeed::Rule),
    Column::new("source_id", ColumnKind::Id, ColumnNeed::Rule),
    Column::new("version", ColumnKind::Int, ColumnNeed::Rule),
    Column::new("locator_kind", ColumnKind::Enum, ColumnNeed::Rule),
    Column::new("locator", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("quote", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("consequential", ColumnKind::Bool, ColumnNeed::Yes),
    Column::new("drafted_by", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("verification", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("applicability", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("adoption", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("data_origin", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("volatility", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("value", ColumnKind::Decimal, ColumnNeed::No),
    Column::new("unit", ColumnKind::Enum, ColumnNeed::Rule),
    Column::new("effective_from", ColumnKind::Pdate, ColumnNeed::No),
    Column::new("effective_to", ColumnKind::Pdate, ColumnNeed::No),
    Column::new("instrument", ColumnKind::Enum, ColumnNeed::No),
    Column::new("citation_codified", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("citation_rfo", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("edition_date", ColumnKind::Pdate, ColumnNeed::No),
    Column::new("text_status", ColumnKind::Enum, ColumnNeed::No),
    Column::new("applies_to", ColumnKind::Text, ColumnNeed::No),
    Column::new("evidence_grade", ColumnKind::Enum, ColumnNeed::Rule),
    Column::new("verified_on", ColumnKind::Date, ColumnNeed::Rule),
    Column::new("verified_by_role", ColumnKind::Enum, ColumnNeed::Rule),
    Column::new("supersedes", ColumnKind::List, ColumnNeed::No),
    Column::new("superseded_by", ColumnKind::List, ColumnNeed::No),
    Column::new("conflict_id", ColumnKind::Id, ColumnNeed::No),
    Column::new("question_id", ColumnKind::Id, ColumnNeed::No),
    Column::new("note", ColumnKind::Text, ColumnNeed::No),
];

const QUESTIONS_COLUMNS: [Column; 13] = [
    Column::new("question_id", ColumnKind::Id, ColumnNeed::Yes),
    Column::new("area", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("question", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("owner_role", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("blocks", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("why_it_matters", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("default_assumption", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("work_can_continue", ColumnKind::Enum, ColumnNeed::Rule),
    Column::new("day_one", ColumnKind::Bool, ColumnNeed::Yes),
    Column::new("status", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("answer", ColumnKind::Text, ColumnNeed::No),
    Column::new("answered_on", ColumnKind::Date, ColumnNeed::No),
    Column::new("adoption", ColumnKind::Enum, ColumnNeed::No),
];

const CONFLICTS_COLUMNS: [Column; 11] = [
    Column::new("conflict_id", ColumnKind::Id, ColumnNeed::Yes),
    Column::new("area", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("topic", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("positions", ColumnKind::List, ColumnNeed::Yes),
    Column::new("resolution", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("rationale", ColumnKind::Text, ColumnNeed::Rule),
    Column::new("evidence", ColumnKind::List, ColumnNeed::Rule),
    Column::new("resolved_on", ColumnKind::Date, ColumnNeed::Rule),
    Column::new("status", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("claim_ids", ColumnKind::List, ColumnNeed::Yes),
    Column::new("question_id", ColumnKind::Id, ColumnNeed::Rule),
];

const GLOSSARY_COLUMNS: [Column; 9] = [
    Column::new("term_id", ColumnKind::Id, ColumnNeed::Yes),
    Column::new("label", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("definition", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("boundary_rule", ColumnKind::Text, ColumnNeed::Yes),
    Column::new("owner_area", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("source_ref", ColumnKind::List, ColumnNeed::Rule),
    Column::new("definition_status", ColumnKind::Enum, ColumnNeed::Yes),
    Column::new("aliases", ColumnKind::List, ColumnNeed::No),
    Column::new("superseded_by", ColumnKind::Id, ColumnNeed::No),
];

impl Register {
    /// Every register in canonical order.
    pub const ALL: [Self; 5] = [
        Self::Sources,
        Self::Claims,
        Self::Questions,
        Self::Conflicts,
        Self::Glossary,
    ];

    /// The CSV file name in `corpus/registers/`.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Sources => "sources.csv",
            Self::Claims => "claims.csv",
            Self::Questions => "questions.csv",
            Self::Conflicts => "conflicts.csv",
            Self::Glossary => "glossary.csv",
        }
    }

    /// The ordered column schema for this register.
    #[must_use]
    pub const fn columns(self) -> &'static [Column] {
        match self {
            Self::Sources => &SOURCES_COLUMNS,
            Self::Claims => &CLAIMS_COLUMNS,
            Self::Questions => &QUESTIONS_COLUMNS,
            Self::Conflicts => &CONFLICTS_COLUMNS,
            Self::Glossary => &GLOSSARY_COLUMNS,
        }
    }
}

impl fmt::Display for Register {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.file_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE_MD: &str = include_str!("../../../../docs/reference/corpus-format.md");

    fn parse_summary_counts(md: &str) -> Vec<(String, usize)> {
        let mut in_registers = false;
        let mut out = Vec::new();
        for line in md.lines() {
            if line.trim() == "## Registers" {
                in_registers = true;
                continue;
            }
            if in_registers && line.starts_with("### ") {
                break;
            }
            if !in_registers || !line.starts_with('|') {
                continue;
            }
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            if cells.len() == 5 && cells[0].starts_with('`') && cells[0].ends_with(".csv`") {
                let file = cells[0].trim_matches('`').to_owned();
                let count: usize = cells[3]
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid column count in summary row: {line}"));
                out.push((file, count));
            }
        }
        out
    }

    fn parse_register_table(md: &str, heading: &str) -> Vec<(String, ColumnKind, ColumnNeed)> {
        let target = format!("### {heading}");
        let mut in_section = false;
        let mut in_table = false;
        let mut cols = Vec::new();
        for line in md.lines() {
            let trimmed = line.trim();
            if trimmed == target {
                in_section = true;
                continue;
            }
            if in_section && (trimmed.starts_with("### ") || trimmed.starts_with("## ")) {
                break;
            }
            if !in_section {
                continue;
            }
            if trimmed.starts_with("| # | Column | Type | Required |") {
                in_table = true;
                continue;
            }
            if in_table {
                if !trimmed.starts_with('|') {
                    break;
                }
                let cells: Vec<&str> = trimmed
                    .trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .collect();
                if cells.len() < 4 || cells[0].starts_with('-') {
                    continue;
                }
                let _num: usize = cells[0]
                    .parse()
                    .unwrap_or_else(|_| panic!("expected row number in {heading}: {line}"));
                let name = cells[1].trim_matches('`').to_owned();
                let kind = ColumnKind::parse(cells[2]).unwrap_or_else(|| {
                    panic!(
                        "unknown column type `{}` for `{name}` in {heading}",
                        cells[2]
                    )
                });
                let need = match cells[3] {
                    "yes" => ColumnNeed::Yes,
                    "no" => ColumnNeed::No,
                    _ => ColumnNeed::Rule,
                };
                cols.push((name, kind, need));
            }
        }
        cols
    }

    #[test]
    fn columns_match_the_reference() {
        let summary_counts = parse_summary_counts(REFERENCE_MD);
        assert_eq!(
            summary_counts,
            vec![
                ("sources.csv".to_owned(), 28),
                ("claims.csv".to_owned(), 35),
                ("questions.csv".to_owned(), 13),
                ("conflicts.csv".to_owned(), 11),
                ("glossary.csv".to_owned(), 9),
            ],
            "summary table in docs/reference/corpus-format.md must list the five registers and counts (28, 35, 13, 11, 9)"
        );

        for (reg, (file_name, expected_count)) in Register::ALL.into_iter().zip(summary_counts) {
            assert_eq!(reg.file_name(), file_name);
            let ref_cols = parse_register_table(REFERENCE_MD, &file_name);
            assert_eq!(
                ref_cols.len(),
                expected_count,
                "reference table for {file_name} has {} rows, summary says {expected_count}",
                ref_cols.len()
            );

            let code_cols = reg.columns();
            if code_cols.len() != expected_count {
                let missing: Vec<&str> = ref_cols
                    .iter()
                    .filter(|(rname, _, _)| !code_cols.iter().any(|c| c.name == rname))
                    .map(|(rname, _, _)| rname.as_str())
                    .collect();
                let extra: Vec<&str> = code_cols
                    .iter()
                    .filter(|c| !ref_cols.iter().any(|(rname, _, _)| rname == c.name))
                    .map(|c| c.name)
                    .collect();
                panic!(
                    "column count mismatch for {file_name}: code has {}, reference has {expected_count} (missing: {missing:?}, extra: {extra:?})",
                    code_cols.len()
                );
            }

            for (idx, (code_col, (ref_name, ref_kind, ref_need))) in
                code_cols.iter().zip(ref_cols.iter()).enumerate()
            {
                assert_eq!(
                    code_col.name,
                    ref_name,
                    "column #{} mismatch in {file_name}: code has `{}`, reference has `{ref_name}`",
                    idx + 1,
                    code_col.name
                );
                assert_eq!(
                    code_col.kind, *ref_kind,
                    "column `{ref_name}` in {file_name} has kind {:?} in code, {:?} in reference",
                    code_col.kind, ref_kind
                );
                assert_eq!(
                    code_col.need, *ref_need,
                    "column `{ref_name}` in {file_name} has need {:?} in code, {:?} in reference",
                    code_col.need, ref_need
                );
            }
        }
    }
}

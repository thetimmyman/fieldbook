//! Plain CSV copies of the registers for Excel (`data/<register>.csv` and `data/claims-<AREA>-<part>.csv`).

use std::collections::{BTreeMap, BTreeSet};

use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::{ColumnKind, Register};

/// Set of prefixes that require an apostrophe guard in CSV for Excel.
const GUARD_PREFIXES: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// Returns the site-relative output path for `register`'s plain CSV export (`data/<register>.csv`).
#[must_use]
pub fn output_path(register: Register) -> String {
    format!("data/{}", register.file_name())
}

/// Returns the site-relative output path for one claims part CSV (`data/claims-<AREA>-<part>.csv`).
#[must_use]
pub fn claims_part_output_path(area_code: &str, part: u32) -> String {
    format!("data/claims-{}-{part}.csv", area_code.to_ascii_uppercase())
}

/// Applies the Excel apostrophe guard to unsafe text prefixes.
///
/// Any cell of a column whose kind is NOT [`ColumnKind::Decimal`] that starts with
/// `=`, `+`, `-`, `@`, TAB (`\t`), or CR (`\r`) gets a leading `'` (an apostrophe).
/// A [`ColumnKind::Decimal`] column cell (such as `-5`) is a number and receives no apostrophe.
#[must_use]
pub fn guard_cell(kind: ColumnKind, cell: &str) -> String {
    if kind != ColumnKind::Decimal && cell.starts_with(GUARD_PREFIXES) {
        format!("'{cell}")
    } else {
        cell.to_owned()
    }
}

/// Kept identifier sets extracted from the permitted view.
struct KeptIds {
    claims: BTreeSet<String>,
    versions: BTreeSet<String>,
    questions: BTreeSet<String>,
    conflicts: BTreeSet<String>,
    terms: BTreeSet<String>,
}

impl KeptIds {
    fn from_view(view: &LabelledView) -> Self {
        Self {
            claims: view
                .claims()
                .iter()
                .map(|loc| loc.value.claim_id.to_string())
                .collect(),
            versions: view
                .sources()
                .iter()
                .map(|loc| format!("{}-v{}", loc.value.source_id, loc.value.version))
                .collect(),
            questions: view
                .questions()
                .iter()
                .map(|loc| loc.value.question_id.to_string())
                .collect(),
            conflicts: view
                .conflicts()
                .iter()
                .map(|loc| loc.value.conflict_id.to_string())
                .collect(),
            terms: view
                .glossary()
                .iter()
                .map(|loc| loc.value.term_id.to_string())
                .collect(),
        }
    }
}

/// Reference column classification for cross-record reference scrubbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefKind {
    PassThrough,
    ListVersions,
    ListClaims,
    ListClaimsOrVersions,
    SingleConflicts,
    SingleQuestions,
    SingleTerms,
}

fn column_ref_kind(register: Register, column_name: &str) -> RefKind {
    match register {
        Register::Sources => match column_name {
            "supersedes" => RefKind::ListVersions,
            _ => RefKind::PassThrough,
        },
        Register::Claims => match column_name {
            "supersedes" | "superseded_by" => RefKind::ListClaims,
            "conflict_id" => RefKind::SingleConflicts,
            "question_id" => RefKind::SingleQuestions,
            _ => RefKind::PassThrough,
        },
        Register::Questions => RefKind::PassThrough,
        Register::Conflicts => match column_name {
            "claim_ids" => RefKind::ListClaims,
            "evidence" => RefKind::ListClaimsOrVersions,
            "question_id" => RefKind::SingleQuestions,
            _ => RefKind::PassThrough,
        },
        Register::Glossary => match column_name {
            "source_ref" => RefKind::ListClaimsOrVersions,
            "superseded_by" => RefKind::SingleTerms,
            _ => RefKind::PassThrough,
        },
    }
}

/// Filters pipe-separated tokens in a list cell against a predicate, rejoining surviving tokens.
fn filter_list(cell: &str, kept: &dyn Fn(&str) -> bool) -> String {
    if cell.is_empty() {
        return String::new();
    }
    cell.split('|')
        .filter(|token| !token.is_empty() && kept(token))
        .collect::<Vec<_>>()
        .join("|")
}

/// Filters a single identifier cell against a predicate, clearing it if not kept.
fn filter_single(cell: &str, kept: &dyn Fn(&str) -> bool) -> String {
    if !cell.is_empty() && kept(cell) {
        cell.to_owned()
    } else {
        String::new()
    }
}

/// Returns the permitted data rows of `register` in column order, scrubbed of references to left-out records.
#[must_use]
pub fn rows(view: &LabelledView, register: Register) -> Vec<Vec<String>> {
    let kept = KeptIds::from_view(view);
    let columns = register.columns();
    let col_kinds: Vec<RefKind> = columns
        .iter()
        .map(|col| column_ref_kind(register, col.name))
        .collect();

    let reg_rows = view.register_rows(register);
    let mut out_rows = Vec::with_capacity(reg_rows.len());

    for row in reg_rows {
        let mut out_row = Vec::with_capacity(columns.len());
        for (idx, &ref_kind) in col_kinds.iter().enumerate() {
            let raw_cell = row.cells.get(idx).map_or("", String::as_str);
            let scrubbed = match ref_kind {
                RefKind::PassThrough => raw_cell.to_owned(),
                RefKind::ListVersions => filter_list(raw_cell, &|id| kept.versions.contains(id)),
                RefKind::ListClaims => filter_list(raw_cell, &|id| kept.claims.contains(id)),
                RefKind::ListClaimsOrVersions => filter_list(raw_cell, &|id| {
                    kept.claims.contains(id) || kept.versions.contains(id)
                }),
                RefKind::SingleConflicts => {
                    filter_single(raw_cell, &|id| kept.conflicts.contains(id))
                }
                RefKind::SingleQuestions => {
                    filter_single(raw_cell, &|id| kept.questions.contains(id))
                }
                RefKind::SingleTerms => filter_single(raw_cell, &|id| kept.terms.contains(id)),
            };
            out_row.push(scrubbed);
        }
        out_rows.push(out_row);
    }

    out_rows
}

fn render_rows(register: Register, data_rows: &[Vec<String>]) -> Vec<u8> {
    let columns = register.columns();
    let out = vec![0xEF, 0xBB, 0xBF];
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .quote_style(csv::QuoteStyle::Necessary)
        .from_writer(out);

    let header_record = columns.iter().map(|col| col.name);
    writer
        .write_record(header_record)
        .unwrap_or_else(|e| unreachable!("a CSV write into memory is infallible: {e}"));

    for row in data_rows {
        let guarded_record = row.iter().enumerate().map(|(idx, cell)| {
            let kind = columns.get(idx).map_or(ColumnKind::Text, |col| col.kind);
            guard_cell(kind, cell)
        });
        writer
            .write_record(guarded_record)
            .unwrap_or_else(|e| unreachable!("a CSV write into memory is infallible: {e}"));
    }

    writer
        .into_inner()
        .unwrap_or_else(|e| unreachable!("a CSV write into memory is infallible: {e}"))
}

/// Renders the CSV bytes for `register`, including UTF-8 BOM, header, and scrubbed, guarded data rows.
#[must_use]
pub fn render(view: &LabelledView, register: Register) -> Vec<u8> {
    let data_rows = rows(view, register);
    render_rows(register, &data_rows)
}

/// Renders one `(output_path, csv_bytes)` pair per claims part in [`crate::register::claims_parts`] order.
///
/// Scrubs [`Register::Claims`] once via [`rows`] and partitions rows in a single pass by
/// [`fieldbook_core::ids::claim_part`] of the first cell (`claim_id`). Any row that `claim_part`
/// cannot place is omitted, matching the part pages.
#[must_use]
pub fn render_claims_parts(view: &LabelledView) -> Vec<(String, Vec<u8>)> {
    let all_rows = rows(view, Register::Claims);
    let mut grouped: BTreeMap<(String, u32), Vec<Vec<String>>> = BTreeMap::new();
    for row in all_rows {
        if let Some(part_key) = row
            .first()
            .and_then(|id| fieldbook_core::ids::claim_part(id))
        {
            grouped.entry(part_key).or_default().push(row);
        }
    }

    let parts = crate::register::claims_parts(view);
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let part_rows = grouped
            .remove(&(part.area_code.clone(), part.part))
            .unwrap_or_default();
        out.push((
            claims_part_output_path(&part.area_code, part.part),
            render_rows(Register::Claims, &part_rows),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_cell_cases() {
        assert_eq!(guard_cell(ColumnKind::Text, "=1+1"), "'=1+1");
        assert_eq!(guard_cell(ColumnKind::Decimal, "-5"), "-5");
        assert_eq!(guard_cell(ColumnKind::Text, "-5"), "'-5");
        assert_eq!(guard_cell(ColumnKind::Text, "\tsome"), "'\tsome");
        assert_eq!(guard_cell(ColumnKind::Text, "\rsome"), "'\rsome");
        assert_eq!(guard_cell(ColumnKind::Text, "plain"), "plain");
        assert_eq!(guard_cell(ColumnKind::Text, "+1"), "'+1");
        assert_eq!(guard_cell(ColumnKind::Text, "@mention"), "'@mention");
        assert_eq!(guard_cell(ColumnKind::Text, ""), "");
        assert_eq!(guard_cell(ColumnKind::Decimal, "+5"), "+5");
    }

    #[test]
    fn test_filter_list() {
        let kept = |token: &str| token == "CL-01" || token == "S-0001-v1";
        assert_eq!(filter_list("", &kept), "");
        assert_eq!(filter_list("CL-01", &kept), "CL-01");
        assert_eq!(filter_list("CL-02", &kept), "");
        assert_eq!(
            filter_list("CL-01|CL-02|S-0001-v1", &kept),
            "CL-01|S-0001-v1"
        );
        assert_eq!(filter_list("CL-02|CL-03", &kept), "");
    }

    #[test]
    fn test_output_path() {
        assert_eq!(output_path(Register::Sources), "data/sources.csv");
        assert_eq!(output_path(Register::Claims), "data/claims.csv");
        assert_eq!(output_path(Register::Questions), "data/questions.csv");
        assert_eq!(output_path(Register::Conflicts), "data/conflicts.csv");
        assert_eq!(output_path(Register::Glossary), "data/glossary.csv");
        assert_eq!(claims_part_output_path("cmp", 1), "data/claims-CMP-1.csv");
        assert_eq!(claims_part_output_path("CMP", 12), "data/claims-CMP-12.csv");
    }
}

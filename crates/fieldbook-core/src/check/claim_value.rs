//! Check pass `claim_value`: `FB0500` on `corpus/registers/claims.csv`
//! (`docs/reference/corpus-format.md#claimscsv`, `#05-calculation`).
//!
//! **Responsibility.** Parse every non-empty `value` cell of `corpus/registers/claims.csv`
//! with [`fieldbook_calc::Dec::parse`] (the decimal grammar of `calc-day1`) and raise
//! `FB0500` at the `value` cell on failure.

use fieldbook_calc::Dec;

use crate::schema::Register;
use crate::schema::table::Row;

use super::{Code, Diagnostic, Model, Options, Report};

const CLAIMS_PATH: &str = "corpus/registers/claims.csv";
const VALUE_IDX: usize = 17;

/// Run the `claim_value` check pass (`FB0500`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    let raw_rows = model.register_rows(Register::Claims);
    if raw_rows.is_empty() {
        return;
    }

    let problems_by_line = model.register_problems(Register::Claims);

    for row in raw_rows {
        let has_value_problem = problems_by_line
            .iter()
            .find(|(line, _)| *line == row.line)
            .is_some_and(|(_, probs)| probs.iter().any(|p| p.column == "value"));
        if has_value_problem {
            continue;
        }

        let value_cell = cell_at(row, VALUE_IDX);
        if value_cell.is_empty() {
            continue;
        }

        if Dec::parse(value_cell).is_err() {
            let line = usize::try_from(row.line).unwrap_or(1);
            let col = col_at(row, VALUE_IDX);
            let claim_id = cell_at(row, 0);
            report.push(Diagnostic::new(
                Code::Fb0500,
                options.strict,
                CLAIMS_PATH,
                line,
                col,
                format!("claim `{claim_id}` `value` `{value_cell}` is not in the decimal grammar"),
            ));
        }
    }
}

fn col_at(row: &Row, idx: usize) -> usize {
    row.cols
        .get(idx)
        .copied()
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(1)
}

fn cell_at(row: &Row, idx: usize) -> &str {
    row.cells.get(idx).map_or("", String::as_str)
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Tree, run};
    use crate::date::CivilDate;

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
        CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("valid date: {e:?}"))
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
    fn valid_claim_values_pass() {
        let host = Host::new(mini_tree(), sample_today());
        let report = run(&host, &Options::new(true));
        let fb0500: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0500)
            .collect();
        assert!(fb0500.is_empty(), "unexpected FB0500: {fb0500:?}");
    }

    #[test]
    fn malformed_value_cell_raises_fb0500_at_cell() {
        let mut tree = mini_tree();
        let mutated = MINI_CLAIMS_CSV.replace(",5,percent,", ",\"1,000.00\",USD,");
        assert_ne!(mutated, MINI_CLAIMS_CSV, "must replace value cell");
        tree.insert_bytes("corpus/registers/claims.csv", mutated.into_bytes());
        let host = Host::new(tree, sample_today());
        let report = run(&host, &Options::new(true));
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![Code::Fb0500],
            "expected only FB0500, got {report:?}"
        );
        let diag = &report.diagnostics[0];
        assert_eq!(diag.path, "corpus/registers/claims.csv");
        assert_eq!(diag.line, 3);
        assert!(diag.col > 1);
    }
}

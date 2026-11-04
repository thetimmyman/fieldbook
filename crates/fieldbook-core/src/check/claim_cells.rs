//! Check pass `claim_cells`: `FB0306` and `FB0307` on `corpus/registers/claims.csv`
//! (`docs/reference/corpus-format.md#claimscsv`, `#03-citations`).
//!
//! **Responsibility.** Raise `FB0306` at `locator_kind` when a consequential claim has no fine
//! locator (`no_fine_locator` or empty `locator_kind`), and `FB0307` at `quote` when a
//! consequential claim has no `quote`. Skip any row that already has `FB0204` or `FB0205` in
//! `consequential`, `locator_kind` or `quote`.

use std::collections::BTreeSet;

use crate::schema::Register;
use crate::schema::locator::LocatorKind;
use crate::schema::table::Row;

use super::{Code, Diagnostic, Model, Options, Report};

const CLAIMS_PATH: &str = "corpus/registers/claims.csv";
const LOCATOR_KIND_IDX: usize = 7;
const QUOTE_IDX: usize = 9;
const CONSEQUENTIAL_IDX: usize = 10;

/// Run the `claim_cells` check pass (`FB0306`, `FB0307`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    let raw_rows = model.register_rows(Register::Claims);
    if raw_rows.is_empty() {
        return;
    }

    let mut checked_lines = BTreeSet::new();

    for located in model.claims() {
        checked_lines.insert(located.line);
        if !located.value.consequential {
            continue;
        }
        let Some(row) = raw_rows.iter().find(|r| r.line == located.line) else {
            continue;
        };
        let line = usize::try_from(located.line).unwrap_or(1);

        if matches!(
            located.value.locator_kind,
            None | Some(LocatorKind::NoFineLocator)
        ) {
            let col = col_at(row, LOCATOR_KIND_IDX);
            report.push(Diagnostic::new(
                Code::Fb0306,
                options.strict,
                CLAIMS_PATH,
                line,
                col,
                format!(
                    "consequential claim `{}` requires a fine locator in `locator_kind`",
                    located.value.claim_id
                ),
            ));
        }

        if located.value.quote.is_none() {
            let col = col_at(row, QUOTE_IDX);
            report.push(Diagnostic::new(
                Code::Fb0307,
                options.strict,
                CLAIMS_PATH,
                line,
                col,
                format!(
                    "consequential claim `{}` requires a non-empty `quote`",
                    located.value.claim_id
                ),
            ));
        }
    }

    // For rows skipped by `Model::claims` due to a problem in another column, check FB0306/FB0307
    // unless `consequential`, `locator_kind` or `quote` already has FB0204 or FB0205.
    let problems_by_line = model.register_problems(Register::Claims);
    for row in raw_rows {
        if checked_lines.contains(&row.line) {
            continue;
        }
        let row_problems = problems_by_line
            .iter()
            .find(|(line, _)| *line == row.line)
            .map_or(&[][..], |(_, probs)| probs.as_slice());

        if row_problems
            .iter()
            .any(|p| matches!(p.column, "consequential" | "locator_kind" | "quote"))
        {
            continue;
        }

        let consequential_cell = cell_at(row, CONSEQUENTIAL_IDX);
        let locator_kind_cell = cell_at(row, LOCATOR_KIND_IDX);
        let quote_cell = cell_at(row, QUOTE_IDX);

        if consequential_cell.contains('|')
            || locator_kind_cell.contains('|')
            || quote_cell.contains('|')
        {
            continue;
        }

        if consequential_cell != "true" {
            continue;
        }

        let claim_id = cell_at(row, 0);
        let line = usize::try_from(row.line).unwrap_or(1);

        if locator_kind_cell.is_empty() || locator_kind_cell == "no_fine_locator" {
            let col = col_at(row, LOCATOR_KIND_IDX);
            report.push(Diagnostic::new(
                Code::Fb0306,
                options.strict,
                CLAIMS_PATH,
                line,
                col,
                format!(
                    "consequential claim `{claim_id}` requires a fine locator in `locator_kind`"
                ),
            ));
        }

        if quote_cell.is_empty() {
            let col = col_at(row, QUOTE_IDX);
            report.push(Diagnostic::new(
                Code::Fb0307,
                options.strict,
                CLAIMS_PATH,
                line,
                col,
                format!("consequential claim `{claim_id}` requires a non-empty `quote`"),
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
    fn consequential_claim_with_fine_locator_and_quote_is_clean() {
        let tree = mini_tree();
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert!(
            report.diagnostics.is_empty(),
            "expected clean report, got: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn fb0306_raised_once_when_consequential_claim_has_no_fine_locator() {
        let mut tree = mini_tree();
        let claims = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,text_section,sec. 2,",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,no_fine_locator,,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let fb0306: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0306)
            .collect();
        assert_eq!(fb0306.len(), 1, "diagnostics: {:?}", report.diagnostics);
        assert_eq!(fb0306[0].path, "corpus/registers/claims.csv");
        assert_eq!(fb0306[0].line, 3);
    }

    #[test]
    fn fb0307_raised_once_when_consequential_claim_has_no_quote() {
        let mut tree = mini_tree();
        let claims = MINI_CLAIMS_CSV.replace(
            ",text_section,sec. 2,Synthetic withholding threshold is 5 percent.,true,",
            ",text_section,sec. 2,,true,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let fb0307: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0307)
            .collect();
        assert_eq!(fb0307.len(), 1, "diagnostics: {:?}", report.diagnostics);
        assert_eq!(fb0307[0].path, "corpus/registers/claims.csv");
        assert_eq!(fb0307[0].line, 3);
    }

    #[test]
    fn non_consequential_claim_with_no_fine_locator_and_no_quote_raises_nothing() {
        let mut tree = mini_tree();
        let claims = format!(
            "{MINI_CLAIMS_CSV}CL-EST-0004,est,fact,Synthetic non-consequential note for fixture tests.,2026-02-01,S-0002,2,no_fine_locator,,,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,Synthetic non-consequential claim.\n"
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert!(
            report.diagnostics.is_empty(),
            "expected no diagnostics for non-consequential claim, got: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn row_with_fb0204_or_fb0205_in_consequential_locator_kind_or_quote_raises_nothing_more() {
        let mut tree = mini_tree();
        let claims = format!(
            "{MINI_CLAIMS_CSV}\
CL-EST-0004,est,fact,Synthetic bad locator kind.,2026-02-01,S-0002,2,bad_kind,,,true,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,Note.\n\
CL-EST-0005,est,fact,Synthetic empty consequential.,2026-02-01,S-0002,2,no_fine_locator,,,,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,Note.\n\
CL-EST-0006,est,fact,Synthetic pipe in quote.,2026-02-01,S-0002,2,no_fine_locator,,bad|quote,true,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,Note.\n"
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| matches!(d.code, Code::Fb0306 | Code::Fb0307)),
            "expected no FB0306/FB0307 on rows with cell errors in consequential/locator_kind/quote: {:?}",
            report.diagnostics
        );
    }
}

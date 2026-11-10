//! The `regression` pass: `FB0126` to `FB0128` (`docs/reference/search.md#regression-set`,
//! `docs/reference/corpus-format.md#check-catalogue`).
//!
//! **Responsibility.**
//! - `FB0126` (error): `corpus/eval/regression.toml` is malformed, repeats a case `id`, names an
//!   expected unit id that is not a unit of the Corpus, or has cases that cannot run because the
//!   Corpus cannot be indexed and no other error says so.
//! - `FB0127` (warning): a case misses: an expected page is not within the first `top` page rows
//!   of the search page view (a section id counts for its page), or an `expect_none` query
//!   returns results.
//! - `FB0128` (error): a case marked `critical` misses.
//!
//! Each finding points at the case's `[[case]]` line. The cases are judged on the page view of
//! the index built from the shipped public Corpus, the pages `fieldbook search` shows (the flat
//! list stays available as `fieldbook search --units`).

use crate::eval::{self, REGRESSION_PATH};

use super::{Code, Diagnostic, Model, Options, Report};

/// Run the `regression` check pass over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }
    let strict = options.strict;
    let Some(bytes) = model.tree().get_bytes(REGRESSION_PATH) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };

    let cases = match eval::parse(text) {
        Ok(cases) => cases,
        Err(errors) => {
            for err in errors {
                report.push(Diagnostic::new(
                    Code::Fb0126,
                    strict,
                    REGRESSION_PATH,
                    err.line,
                    err.col,
                    err.message,
                ));
            }
            return;
        }
    };
    if cases.is_empty() {
        return;
    }

    let index = match eval::corpus_index(model.tree()) {
        Ok(index) => index,
        Err(err) => {
            // A refused Corpus is usually reported by its own pass. When no pass has said so, the
            // set must not silently not run: its critical cases are the release gate.
            if report.errors() == 0 {
                report.push(Diagnostic::new(
                    Code::Fb0126,
                    strict,
                    REGRESSION_PATH,
                    1,
                    1,
                    format!(
                        "the regression set cannot run because the Corpus cannot be indexed ({err})"
                    ),
                ));
            } else {
                report.push_note(format!(
                    "regression set skipped: the Corpus cannot be indexed ({err})"
                ));
            }
            return;
        }
    };

    for case in &cases {
        let unknown = eval::unknown_expected(&index, case);
        if !unknown.is_empty() {
            report.push(Diagnostic::new(
                Code::Fb0126,
                strict,
                REGRESSION_PATH,
                case.line,
                1,
                format!(
                    "case `{}` expects {} that {} not in the Corpus",
                    case.id,
                    unknown
                        .iter()
                        .map(|id| format!("`{id}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    if unknown.len() == 1 { "is" } else { "are" },
                ),
            ));
            continue;
        }
        if let Some(message) = eval::page_view_miss(&index, case) {
            let code = if case.critical {
                Code::Fb0128
            } else {
                Code::Fb0127
            };
            report.push(Diagnostic::new(
                code,
                strict,
                REGRESSION_PATH,
                case.line,
                1,
                message,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::REGRESSION_PATH;
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

    const PAGE_CMP_CLAUSE_9900_101: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-clause-9900-101.md"
    );
    const PAGE_CMP_ONBOARDING_CHECKLIST: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md"
    );
    const PAGE_CMP_REVIEW_STEPS: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const PAGE_CMP_WITHHOLD_CASE: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-withhold-case.md"
    );
    const PAGE_EST_BASELINE_TEMPLATE: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-baseline-template.md"
    );
    const PAGE_EST_CALC_01: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-calc-01.md"
    );
    const PAGE_EST_RATE_REFERENCE: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-reference.md"
    );
    const PAGE_EST_RATE_WINDOW_BRIEF: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    const PATH_CMP_CLAUSE_9900_101: &str =
        "corpus/pages/compliance/business-systems/cmp-clause-9900-101.md";
    const PATH_CMP_ONBOARDING_CHECKLIST: &str =
        "corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md";
    const PATH_CMP_REVIEW_STEPS: &str =
        "corpus/pages/compliance/business-systems/cmp-review-steps.md";
    const PATH_CMP_SYSTEM_REVIEW: &str =
        "corpus/pages/compliance/business-systems/cmp-system-review.md";
    const PATH_CMP_WITHHOLD_CASE: &str =
        "corpus/pages/compliance/business-systems/cmp-withhold-case.md";
    const PATH_EST_BASELINE_TEMPLATE: &str =
        "corpus/pages/estimating/pricing/est-baseline-template.md";
    const PATH_EST_CALC_01: &str = "corpus/pages/estimating/pricing/est-calc-01.md";
    const PATH_EST_RATE_REFERENCE: &str = "corpus/pages/estimating/pricing/est-rate-reference.md";
    const PATH_EST_RATE_WINDOW_BRIEF: &str =
        "corpus/pages/estimating/pricing/est-rate-window-brief.md";

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating");
        tree.insert_dir("corpus/pages/estimating/pricing");
        tree.insert_dir("corpus/eval");
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
        tree.insert_bytes(
            PATH_CMP_CLAUSE_9900_101,
            PAGE_CMP_CLAUSE_9900_101.as_bytes(),
        );
        tree.insert_bytes(
            PATH_CMP_ONBOARDING_CHECKLIST,
            PAGE_CMP_ONBOARDING_CHECKLIST.as_bytes(),
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, PAGE_CMP_REVIEW_STEPS.as_bytes());
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, PAGE_CMP_SYSTEM_REVIEW.as_bytes());
        tree.insert_bytes(PATH_CMP_WITHHOLD_CASE, PAGE_CMP_WITHHOLD_CASE.as_bytes());
        tree.insert_bytes(
            PATH_EST_BASELINE_TEMPLATE,
            PAGE_EST_BASELINE_TEMPLATE.as_bytes(),
        );
        tree.insert_bytes(PATH_EST_CALC_01, PAGE_EST_CALC_01.as_bytes());
        tree.insert_bytes(PATH_EST_RATE_REFERENCE, PAGE_EST_RATE_REFERENCE.as_bytes());
        tree.insert_bytes(
            PATH_EST_RATE_WINDOW_BRIEF,
            PAGE_EST_RATE_WINDOW_BRIEF.as_bytes(),
        );
        tree
    }

    #[test]
    fn a_case_the_page_view_misses_raises_fb0127() {
        let mut tree = mini_tree();
        let regression_toml = "\
[[case]]
id = \"rg-001\"
query = \"rate review window\"
expect = [\"est-rate-reference\"]
top = 1
";
        tree.insert_bytes(REGRESSION_PATH, regression_toml.as_bytes());
        let host = Host::new(tree, sample_today());
        let report = run(&host, &Options::new(false));

        let fb0127: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0127)
            .collect();
        assert_eq!(fb0127.len(), 1);
        let diag = fb0127[0];
        assert_eq!(diag.path, "corpus/eval/regression.toml");
        assert_eq!(diag.line, 1);
        assert!(
            diag.message.contains("ranks 2 among the page rows"),
            "unexpected message: {}",
            diag.message
        );
    }

    #[test]
    fn a_case_the_page_view_holds_raises_nothing() {
        let mut tree = mini_tree();
        let regression_toml = "\
[[case]]
id = \"rg-001\"
query = \"rate review window\"
expect = [\"est-rate-reference\"]
top = 5
";
        tree.insert_bytes(REGRESSION_PATH, regression_toml.as_bytes());
        let host = Host::new(tree, sample_today());
        let report = run(&host, &Options::new(false));

        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| matches!(d.code, Code::Fb0126 | Code::Fb0127 | Code::Fb0128))
        );
    }
}

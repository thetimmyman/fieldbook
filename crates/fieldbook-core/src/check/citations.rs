//! The `check-citations` pass (`FB0300`, `FB0301`, `FB0302`, `FB0303`, `FB0305`).
//!
//! **Responsibility.** Emit diagnostics for every saved citation lexer and resolution finding in
//! [`Model::citation_findings`](super::Model) without reparsing page bodies or dropping failed
//! citation attempts.

use super::{Diagnostic, Model, Options, Report};

/// Run the citation check pass (`FB0300` to `FB0303` and `FB0305`) on `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    let strict = options.strict;
    for entry in model.citation_findings() {
        report.push(Diagnostic::new(
            entry.finding.code,
            strict,
            &entry.path,
            entry.finding.span.line,
            entry.finding.span.col,
            entry.finding.message.clone(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Tree, load};
    use crate::date::CivilDate;
    use crate::ids::PageId;

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

    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const PATH_CMP_SYSTEM_REVIEW: &str =
        "corpus/pages/compliance/business-systems/cmp-system-review.md";

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
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, PAGE_CMP_SYSTEM_REVIEW.as_bytes());
        tree
    }

    #[test]
    fn malformed_and_touching_attempts_survive_load() {
        let mut tree = mini_tree();
        // Replace all valid citations in the page so the resolved citation slice is completely empty,
        // while planting malformed (`[@S-42]`) and touching (`[@CL-CMP-9998][@CL-CMP-9999]`) attempts.
        let faulty_page = PAGE_CMP_SYSTEM_REVIEW
            .replace(
                "A synthetic system review runs on a recurring cycle of 30 days [@CL-CMP-0001]. When a synthetic deficiency remains open, the synthetic billing withhold rate is 5 percent [@CL-CMP-0002].",
                "Malformed attempt [@S-42] and touching unresolved attempts [@CL-CMP-9998][@CL-CMP-9999].",
            )
            .replace(
                "Confirm the approving role and escalation path [@Q-001; @XC-01].",
                "Confirm the approving role and escalation path.",
            );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, faulty_page.as_bytes());

        let (model, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let pid = PageId::parse("cmp-system-review").unwrap();
        assert!(
            model.citations(&pid).is_empty(),
            "resolved citation slice must be empty when no citation resolves"
        );

        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&Code::Fb0300),
            "expected FB0300 to survive when resolved slice is empty, got {codes:?}"
        );
        assert!(
            codes.contains(&Code::Fb0302),
            "expected FB0302 to survive when resolved slice is empty, got {codes:?}"
        );
        assert!(
            codes.contains(&Code::Fb0303),
            "expected FB0303 to survive when resolved slice is empty, got {codes:?}"
        );
    }
}

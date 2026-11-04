//! Citation resolution against [`crate::check::Index`].
//!
//! Resolves each [`Citation`] produced by [`super::lexer`] against the loaded registers
//! (`sources.csv`, `claims.csv`, `questions.csv`, `conflicts.csv`), emitting `FB0303`
//! at the citation's line and column when a key does not exist, while preserving resolved
//! citations (and any unsupported extension locator fact) in reading order.

use crate::check::{Code, Index};
use crate::ids::{ClaimId, ConflictId, QuestionId, VersionId};
use crate::schema::locator::ExtensionResolution;

use super::lexer::{Citation, KeyKind, LexFinding, LexOutcome};

/// Resolved target of a [`Citation`] in [`Index`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedKey {
    /// A known source version in `sources.csv`, with optional [`ExtensionResolution`] when the
    /// citation carries an `ext <kind> <payload>` locator.
    SourceVersion {
        /// The resolved [`VersionId`].
        id: VersionId,
        /// 1-based line in `sources.csv`.
        line: u32,
        /// `Some(ExtensionResolution::Unsupported { .. })` when the citation carries an extension locator.
        extension: Option<ExtensionResolution>,
    },
    /// A known claim in `claims.csv`.
    Claim {
        /// The resolved [`ClaimId`].
        id: ClaimId,
        /// 1-based line in `claims.csv`.
        line: u32,
    },
    /// A known question in `questions.csv`.
    Question {
        /// The resolved [`QuestionId`].
        id: QuestionId,
        /// 1-based line in `questions.csv`.
        line: u32,
    },
    /// A known conflict in `conflicts.csv`.
    Conflict {
        /// The resolved [`ConflictId`].
        id: ConflictId,
        /// 1-based line in `conflicts.csv`.
        line: u32,
    },
}

/// Resolves `lexed` citations against `index` in reading order.
///
/// Known keys stay in the returned `Vec<Citation>` (together with their corresponding
/// [`ResolvedKey`]s) in reading order; unknown keys emit an `FB0303` [`LexFinding`] at the
/// citation's span and are excluded from the resolved slice.
#[must_use]
pub fn resolve_citations(
    lexed: LexOutcome,
    index: &Index,
) -> (Vec<Citation>, Vec<ResolvedKey>, Vec<LexFinding>) {
    let mut resolved_cites = Vec::new();
    let mut resolved_keys = Vec::new();
    let mut findings = lexed.findings;

    for cite in lexed.citations {
        match resolve_one(&cite, index) {
            Some(rk) => {
                resolved_cites.push(cite);
                resolved_keys.push(rk);
            }
            None => {
                findings.push(LexFinding {
                    code: Code::Fb0303,
                    span: cite.span,
                    message: format!("citation `@{}` does not exist in its register", cite.key),
                });
            }
        }
    }

    (resolved_cites, resolved_keys, findings)
}

fn resolve_one(cite: &Citation, index: &Index) -> Option<ResolvedKey> {
    match cite.key_kind {
        KeyKind::Version => {
            let vid = VersionId::parse(&cite.key).ok()?;
            let entry = index.source_version(&vid)?;
            let extension = cite
                .locator
                .as_ref()
                .and_then(|loc| loc.extension_resolution());
            Some(ResolvedKey::SourceVersion {
                id: vid,
                line: entry.line,
                extension,
            })
        }
        KeyKind::Claim => {
            let cid = ClaimId::parse(&cite.key).ok()?;
            let &line = index.claims.get(&cid)?;
            Some(ResolvedKey::Claim { id: cid, line })
        }
        KeyKind::Question => {
            let qid = QuestionId::parse(&cite.key).ok()?;
            let &line = index.questions.get(&qid)?;
            Some(ResolvedKey::Question { id: qid, line })
        }
        KeyKind::Conflict => {
            let xcid = ConflictId::parse(&cite.key).ok()?;
            let &line = index.conflicts.get(&xcid)?;
            Some(ResolvedKey::Conflict { id: xcid, line })
        }
        KeyKind::Note => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{Host, Options, Tree, load};
    use crate::cite::{Mode, claims_of, lex_from_line};
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
    fn citations_are_loaded_in_reading_order() {
        let mut tree = mini_tree();
        // Place citations in reverse-alphabetical key order in the body to verify reading order
        // (and verify `claims_of` sorts and deduplicates claims separately).
        let reverse_order_page = PAGE_CMP_SYSTEM_REVIEW.replace(
            "A synthetic system review runs on a recurring cycle of 30 days [@CL-CMP-0001]. When a synthetic deficiency remains open, the synthetic billing withhold rate is 5 percent [@CL-CMP-0002].",
            "First [@S-0002-v2, sec. 1], then [@CL-CMP-0002], then [@CL-CMP-0001], and again [@CL-CMP-0002].",
        );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, reverse_order_page.as_bytes());

        let (model, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

        let page_id = PageId::parse("cmp-system-review").unwrap();
        let cites = model.citations(&page_id);
        let keys: Vec<&str> = cites.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "S-0002-v2",
                "CL-CMP-0002",
                "CL-CMP-0001",
                "CL-CMP-0002",
                "Q-001",
                "XC-01",
            ],
            "Model::citations must preserve exact reading order"
        );

        let unique_claims = claims_of(cites);
        assert_eq!(
            unique_claims,
            vec![
                ClaimId::parse("CL-CMP-0001").unwrap(),
                ClaimId::parse("CL-CMP-0002").unwrap(),
            ],
            "claims_of must return sorted unique ClaimIds"
        );
    }

    #[test]
    fn resolve_distinguishes_missing_source_from_available_source_with_unavailable_extension() {
        let (model, _) = load(
            &Host::new(mini_tree(), sample_today()),
            &Options::new(false),
        );
        let index = model.index();

        // 1. Available source `S-0002-v2` with an unsupported extension locator resolves the source
        // and preserves `ExtensionResolution::Unsupported { kind, payload }`.
        let lexed_ok = lex_from_line(
            "See [@S-0002-v2, ext structured_path %2Fitems%2F0].",
            10,
            Mode::Strict,
        );
        let (resolved_cites, resolved_keys, findings) = resolve_citations(lexed_ok, index);
        assert!(
            findings.is_empty(),
            "expected no FB0303 for existing source"
        );
        assert_eq!(resolved_cites.len(), 1);
        assert_eq!(
            resolved_keys,
            vec![ResolvedKey::SourceVersion {
                id: VersionId::parse("S-0002-v2").unwrap(),
                line: 4,
                extension: Some(ExtensionResolution::Unsupported {
                    kind: "structured_path".to_owned(),
                    payload: "/items/0".to_owned(),
                }),
            }]
        );

        // 2. Missing source `S-9999-v1` with the same extension locator emits FB0303 and does not resolve.
        let lexed_missing = lex_from_line(
            "See [@S-9999-v1, ext structured_path %2Fitems%2F0].",
            10,
            Mode::Strict,
        );
        let (resolved_cites_missing, resolved_keys_missing, findings_missing) =
            resolve_citations(lexed_missing, index);
        assert!(resolved_cites_missing.is_empty());
        assert!(resolved_keys_missing.is_empty());
        assert_eq!(findings_missing.len(), 1);
        assert_eq!(findings_missing[0].code, Code::Fb0303);
        assert_eq!(findings_missing[0].span.line, 10);
        assert_eq!(findings_missing[0].span.col, 5);
    }
}

//! [`Index`]: resolved register, page and `corpus.toml` identifiers built once in [`Model`](super::Model).
//!
//! **Responsibility.** Hold the source versions (with `lifecycle` and the computed `superseded`
//! state), claims, questions, conflicts, terms, areas and pages, with the 1-based line number of
//! each register row.

use std::collections::{BTreeMap, BTreeSet};

use crate::ids::{AreaCode, ClaimId, ConflictId, PageId, QuestionId, TermId, VersionId};
use crate::schema::rows::RefId;
use crate::schema::vocab::Lifecycle;
use crate::schema::{ClaimRow, ConflictRow, CorpusToml, GlossaryRow, QuestionRow, SourceRow};

use super::model::{Located, PageEntry};

/// Resolved metadata for one source version in [`Index`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceVersionEntry {
    /// 1-based line number of the version row in `corpus/registers/sources.csv`.
    pub line: u32,
    /// The version's `lifecycle` (`active`, `quarantined` or `withdrawn`).
    pub lifecycle: Lifecycle,
    /// `true` when another typed row in `sources.csv` lists this version in `supersedes`.
    pub superseded: bool,
}

/// Identifier index built once from the typed register rows and `corpus.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    /// Source versions keyed by [`VersionId`].
    pub source_versions: BTreeMap<VersionId, SourceVersionEntry>,
    /// Claims keyed by [`ClaimId`] to their 1-based line number in `claims.csv`.
    pub claims: BTreeMap<ClaimId, u32>,
    /// Questions keyed by [`QuestionId`] to their 1-based line number in `questions.csv`.
    pub questions: BTreeMap<QuestionId, u32>,
    /// Conflicts keyed by [`ConflictId`] to their 1-based line number in `conflicts.csv`.
    pub conflicts: BTreeMap<ConflictId, u32>,
    /// Glossary terms keyed by [`TermId`] to their 1-based line number in `glossary.csv`.
    pub terms: BTreeMap<TermId, u32>,
    /// Area codes declared in `corpus/corpus.toml`.
    pub areas: BTreeSet<AreaCode>,
    /// Page identifiers of parsed pages in `corpus/pages/`.
    pub pages: BTreeSet<PageId>,
}

impl Index {
    /// Build an [`Index`] from `corpus`, the typed register slices and the parsed `pages`.
    #[must_use]
    pub fn build(
        corpus: Option<&CorpusToml>,
        sources: &[Located<SourceRow>],
        claims: &[Located<ClaimRow>],
        questions: &[Located<QuestionRow>],
        conflicts: &[Located<ConflictRow>],
        glossary: &[Located<GlossaryRow>],
        pages: &[PageEntry],
    ) -> Self {
        let mut superseded_versions: BTreeSet<VersionId> = BTreeSet::new();
        for located in sources {
            let Ok(self_vid) = VersionId::parse(&format!(
                "{}-v{}",
                located.value.source_id, located.value.version
            )) else {
                continue;
            };
            for sup in &located.value.supersedes {
                if *sup != self_vid {
                    superseded_versions.insert(sup.clone());
                }
            }
        }

        let mut source_versions = BTreeMap::new();
        for located in sources {
            let Ok(vid) = VersionId::parse(&format!(
                "{}-v{}",
                located.value.source_id, located.value.version
            )) else {
                continue;
            };
            let superseded = superseded_versions.contains(&vid);
            source_versions.entry(vid).or_insert(SourceVersionEntry {
                line: located.line,
                lifecycle: located.value.lifecycle,
                superseded,
            });
        }

        let mut claim_map = BTreeMap::new();
        for located in claims {
            claim_map
                .entry(located.value.claim_id.clone())
                .or_insert(located.line);
        }

        let mut question_map = BTreeMap::new();
        for located in questions {
            question_map
                .entry(located.value.question_id.clone())
                .or_insert(located.line);
        }

        let mut conflict_map = BTreeMap::new();
        for located in conflicts {
            conflict_map
                .entry(located.value.conflict_id.clone())
                .or_insert(located.line);
        }

        let mut term_map = BTreeMap::new();
        for located in glossary {
            term_map
                .entry(located.value.term_id.clone())
                .or_insert(located.line);
        }

        let mut areas = BTreeSet::new();
        if let Some(c) = corpus {
            for area in c.areas() {
                if let Ok(code) = AreaCode::parse(&area.code) {
                    areas.insert(code);
                }
            }
        }

        let page_set: BTreeSet<PageId> = pages.iter().map(|p| p.id.clone()).collect();

        Self {
            source_versions,
            claims: claim_map,
            questions: question_map,
            conflicts: conflict_map,
            terms: term_map,
            areas,
            pages: page_set,
        }
    }

    /// Look up a source version by [`VersionId`].
    #[must_use]
    pub fn source_version(&self, id: &VersionId) -> Option<&SourceVersionEntry> {
        self.source_versions.get(id)
    }

    /// Return `true` when `id` is a known source version.
    #[must_use]
    pub fn contains_source_version(&self, id: &VersionId) -> bool {
        self.source_versions.contains_key(id)
    }

    /// Return `true` when `id` is a known claim.
    #[must_use]
    pub fn contains_claim(&self, id: &ClaimId) -> bool {
        self.claims.contains_key(id)
    }

    /// Return `true` when `id` is a known question.
    #[must_use]
    pub fn contains_question(&self, id: &QuestionId) -> bool {
        self.questions.contains_key(id)
    }

    /// Return `true` when `id` is a known conflict.
    #[must_use]
    pub fn contains_conflict(&self, id: &ConflictId) -> bool {
        self.conflicts.contains_key(id)
    }

    /// Return `true` when `id` is a known glossary term.
    #[must_use]
    pub fn contains_term(&self, id: &TermId) -> bool {
        self.terms.contains_key(id)
    }

    /// Return `true` when `code` is a known area in `corpus.toml`.
    #[must_use]
    pub fn contains_area(&self, code: &AreaCode) -> bool {
        self.areas.contains(code)
    }

    /// Return `true` when `id` is a known parsed page.
    #[must_use]
    pub fn contains_page(&self, id: &PageId) -> bool {
        self.pages.contains(id)
    }

    /// Return `true` when `r` resolves to a known claim or source version.
    #[must_use]
    pub fn contains_ref(&self, r: &RefId) -> bool {
        match r {
            RefId::Claim(cid) => self.contains_claim(cid),
            RefId::SourceVersion(vid) => self.contains_source_version(vid),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{Host, Options, Tree, load};
    use crate::date::CivilDate;
    use crate::ids::{AreaCode, ClaimId, ConflictId, QuestionId, TermId, VersionId};
    use crate::schema::vocab::Lifecycle;

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
    fn index_accessor_returns_resolved_keys() {
        let (model, _report) = load(
            &Host::new(mini_tree(), sample_today()),
            &Options::new(false),
        );
        let idx = model.index();

        let s1v1 = VersionId::parse("S-0001-v1").unwrap();
        let s2v1 = VersionId::parse("S-0002-v1").unwrap();
        let s2v2 = VersionId::parse("S-0002-v2").unwrap();
        let s3v1 = VersionId::parse("S-0003-v1").unwrap();

        assert_eq!(idx.source_versions.len(), 4);
        assert_eq!(
            idx.source_version(&s1v1),
            Some(&super::SourceVersionEntry {
                line: 2,
                lifecycle: Lifecycle::Active,
                superseded: false,
            })
        );
        assert_eq!(
            idx.source_version(&s2v1),
            Some(&super::SourceVersionEntry {
                line: 3,
                lifecycle: Lifecycle::Active,
                superseded: true,
            })
        );
        assert_eq!(
            idx.source_version(&s2v2),
            Some(&super::SourceVersionEntry {
                line: 4,
                lifecycle: Lifecycle::Active,
                superseded: false,
            })
        );
        assert_eq!(
            idx.source_version(&s3v1),
            Some(&super::SourceVersionEntry {
                line: 5,
                lifecycle: Lifecycle::Active,
                superseded: false,
            })
        );

        assert_eq!(idx.claims.len(), 5);
        assert_eq!(
            idx.claims.get(&ClaimId::parse("CL-CMP-0001").unwrap()),
            Some(&2)
        );
        assert_eq!(
            idx.claims.get(&ClaimId::parse("CL-CMP-0002").unwrap()),
            Some(&3)
        );
        assert_eq!(
            idx.claims.get(&ClaimId::parse("CL-EST-0001").unwrap()),
            Some(&4)
        );
        assert_eq!(
            idx.claims.get(&ClaimId::parse("CL-EST-0002").unwrap()),
            Some(&5)
        );
        assert_eq!(
            idx.claims.get(&ClaimId::parse("CL-EST-0003").unwrap()),
            Some(&6)
        );

        assert_eq!(idx.questions.len(), 2);
        assert_eq!(
            idx.questions.get(&QuestionId::parse("Q-001").unwrap()),
            Some(&2)
        );
        assert_eq!(
            idx.questions.get(&QuestionId::parse("Q-002").unwrap()),
            Some(&3)
        );

        assert_eq!(idx.conflicts.len(), 1);
        assert_eq!(
            idx.conflicts.get(&ConflictId::parse("XC-01").unwrap()),
            Some(&2)
        );

        assert_eq!(idx.terms.len(), 4);
        assert_eq!(
            idx.terms.get(&TermId::parse("billing_withhold").unwrap()),
            Some(&2)
        );
        assert_eq!(
            idx.terms.get(&TermId::parse("cost_baseline").unwrap()),
            Some(&3)
        );
        assert_eq!(
            idx.terms.get(&TermId::parse("rate_window").unwrap()),
            Some(&4)
        );
        assert_eq!(
            idx.terms.get(&TermId::parse("system_review").unwrap()),
            Some(&5)
        );

        assert_eq!(idx.areas.len(), 2);
        assert!(idx.contains_area(&AreaCode::parse("cmp").unwrap()));
        assert!(idx.contains_area(&AreaCode::parse("est").unwrap()));
    }
}

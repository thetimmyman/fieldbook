//! The `bm25f@1` [`IndexPort`] adapter over the in-memory search index.
//!
//! **Responsibility.** Build the same [`Index`] that `build` and `search` build for one
//! [`Model`] under a label predicate, and answer [`IndexPort::query`] calls from it.
//!
//! **Rules.** Pure: no I/O, no clock, no environment. Deterministic for equal inputs and
//! labels, because the unit builder and the index builder are the shared search code.

use std::collections::BTreeSet;

use crate::check::Model;
use crate::ids::VersionId;
use crate::labels::LabelSet;
use crate::ports::index::{Hit, Hits, IndexError, IndexPort, Query, Tier, UnitRef};
use crate::schema::vocab::Lifecycle;
use crate::schema::{ClaimRow, ConflictRow, GlossaryRow, QuestionRow, SourceRow};
use crate::search::units::kept_glossary;
use crate::search::{Index, Unit, build_corpus_units_with, query as search_query};

/// Registered adapter identifier of this index.
pub const ID: &str = "bm25f@1";

/// The `bm25f@1` [`IndexPort`]: the built search index and the units it was built from.
#[derive(Debug, Clone)]
pub struct Bm25Index {
    index: Index,
    units: Vec<Unit>,
}

impl Bm25Index {
    /// Builds an index over the units of `model` that `keep` accepts, using the same unit and
    /// index builders as `build` and `search`.
    ///
    /// A unit whose labels `keep` rejects is left out, and a glossary row `keep` rejects
    /// contributes no synonym.
    #[must_use]
    pub fn from_model<F>(model: &Model, keep: F) -> Self
    where
        F: Fn(&LabelSet) -> bool,
    {
        let sources: Vec<SourceRow> = model.sources().iter().map(|s| s.value.clone()).collect();
        let claims: Vec<ClaimRow> = model.claims().iter().map(|c| c.value.clone()).collect();
        let questions: Vec<QuestionRow> =
            model.questions().iter().map(|q| q.value.clone()).collect();
        let conflicts: Vec<ConflictRow> =
            model.conflicts().iter().map(|c| c.value.clone()).collect();
        let terms: Vec<GlossaryRow> = model.glossary().iter().map(|t| t.value.clone()).collect();

        let mut quarantined = BTreeSet::new();
        for row in &sources {
            if row.lifecycle == Lifecycle::Quarantined
                && let Ok(vid) = VersionId::parse(&format!("{}-v{}", row.source_id, row.version))
            {
                quarantined.insert(vid);
            }
        }

        let glossary = kept_glossary(&terms, &sources, &claims, &keep);
        let units = build_corpus_units_with(
            model.pages(),
            &sources,
            &claims,
            &questions,
            &conflicts,
            &terms,
            &quarantined,
            &glossary,
            &keep,
        );
        let index = Index::build(units.clone(), &glossary);
        Self { index, units }
    }

    /// Builds an index over the `public` units of `model`, the default index of the repository
    /// Corpus.
    #[must_use]
    pub fn public(model: &Model) -> Self {
        Self::from_model(model, LabelSet::is_public)
    }

    /// Wraps an already-built search [`Index`] with no unit table, for callers that hold one.
    #[must_use]
    pub fn from_index(index: Index) -> Self {
        Self {
            index,
            units: Vec::new(),
        }
    }

    /// The units this index was built from, empty when it wrapped an index with
    /// [`Bm25Index::from_index`].
    #[must_use]
    pub fn units(&self) -> &[Unit] {
        &self.units
    }
}

impl IndexPort for Bm25Index {
    fn id(&self) -> &'static str {
        ID
    }

    fn query(&self, q: &Query) -> Result<Hits, IndexError> {
        let results = search_query(&self.index, &q.text, usize::MAX);
        let mut hits = Vec::new();
        for hit in results.hits {
            if !q.scope.allows(&hit.kind) {
                continue;
            }
            let tier = if hit.exact { Tier::Exact } else { Tier::Ranked };
            hits.push(Hit {
                unit: UnitRef::new(hit.id, hit.kind),
                score: hit.score,
                tier,
            });
        }
        let total = hits.len();
        hits.truncate(q.limit);
        Ok(Hits { hits, total })
    }
}

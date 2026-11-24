//! The `exact-ids+bm25@1` [`Correlator`] adapter.
//!
//! **Responsibility.** Turn one candidate and the Corpus into the links a person needs to judge
//! it: an exact hash match, ids its text names, the source it may be a new version of, the units
//! its text sits near, and the open question it may answer. Every link carries its reason.
//!
//! **Rules.** Pure and deterministic: only the id lexer, the [`IndexPort`] and the registers, and
//! no model or clock. A link is never built without support.

use std::collections::{BTreeMap, BTreeSet};

use crate::extraction::Extraction;
use crate::ids::QuestionId;
use crate::pipeline::candidate::Candidate;
use crate::pipeline::suggest::{By, Correlator, Link, LinkKind, StageError, Support};
use crate::pipeline::view::CorpusView;
use crate::ports::index::{Hit, IndexPort, Query, Scope};
use crate::quote;
use crate::schema::vocab::QuestionStatus;
use crate::search::{lex, normalise};

/// Registered adapter identifier of this correlator.
pub const ID: &str = "exact-ids+bm25@1";

/// Most related units one candidate can offer.
pub const RELATED_LIMIT: usize = 5;

/// Words of page 1 that make up the related-units query together with the first line.
pub const QUERY_WORDS: usize = 50;

/// Lowest question score that becomes an `Answers` link.
///
/// Chosen from the `corpus-mini` fixture. The candidate that repeats the `Q-001` text scores
/// 21534 on `Q-001`, while an unrelated candidate's best open question (`Q-002`, from
/// "Synthetic escalation applies.") scores 5232. A floor of 8000 accepts the first and rejects
/// the second, and it also rejects the 2642 that the matching candidate scores on `Q-002`. The
/// correlator tests pin both sides of the floor.
pub const PURPOSE_MIN_SCORE: u32 = 8000;

/// The deterministic exact-id and BM25 correlator.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExactIdsBm25;

impl Correlator for ExactIdsBm25 {
    fn id(&self) -> &'static str {
        ID
    }

    fn correlate(
        &self,
        candidate: &Candidate,
        view: &CorpusView,
        index: &dyn IndexPort,
    ) -> Result<Vec<Link>, StageError> {
        // (1) A hash the register already holds ends the run: Present and nothing else.
        if let Some(version) = view.has_hash(&candidate.sha256) {
            let id = version.as_str().to_owned();
            return Ok(vec![Link {
                kind: LinkKind::Present,
                target: id.clone(),
                by: By::Correlator,
                support: vec![Support::Id { id, score: 0 }],
            }]);
        }

        // (2) A candidate with no extraction gets no further link.
        let Some(extraction) = candidate.extraction.as_ref() else {
            return Ok(Vec::new());
        };

        let mut links = Vec::new();
        let targets = canonical_targets(view);

        // (3) Exact ids named by the text, as a Related link with the page and the token.
        let mut seen = BTreeSet::new();
        for page in &extraction.pages {
            for token in lex(&page.text) {
                if !matches!(token.shape, 1 | 2 | 3 | 4 | 5 | 9) {
                    continue;
                }
                let Some(target) = targets.get(&token.text) else {
                    continue;
                };
                if !seen.insert((target.clone(), page.number)) {
                    continue;
                }
                links.push(Link {
                    kind: LinkKind::Related,
                    target: target.clone(),
                    by: By::Correlator,
                    support: vec![Support::Quote {
                        page: page.number,
                        quote: token.text.clone(),
                    }],
                });
            }
        }

        // (4) VersionOf for a source whose title and publisher both appear on page 1.
        let page_count = u32::try_from(extraction.pages.len()).unwrap_or(u32::MAX);
        let pages: Vec<&str> = extraction.pages.iter().map(|p| p.text.as_str()).collect();
        let mut own_sources = BTreeSet::new();
        if page_count > 0 {
            for located in view.sources() {
                let source = &located.value;
                let source_id = source.source_id.as_str();
                if own_sources.contains(source_id) {
                    continue;
                }
                let Some(title_span) = quote::find(&source.title, &pages, 1..=page_count) else {
                    continue;
                };
                if title_span.page != 1 {
                    continue;
                }
                let Some(publisher_span) = quote::find(&source.publisher, &pages, 1..=page_count)
                else {
                    continue;
                };
                if publisher_span.page != 1 {
                    continue;
                }

                let mut support = vec![
                    Support::Quote {
                        page: 1,
                        quote: quote_text(extraction, title_span, &source.title),
                    },
                    Support::Quote {
                        page: 1,
                        quote: quote_text(extraction, publisher_span, &source.publisher),
                    },
                ];
                let version_id = format!("{source_id}-v{}", source.version);
                let named = quote::find(source_id, &pages, 1..=page_count)
                    .map(|span| (span, source_id.to_owned()))
                    .or_else(|| {
                        quote::find(&version_id, &pages, 1..=page_count)
                            .map(|span| (span, version_id.clone()))
                    });
                if let Some((span, fallback)) = named {
                    support.push(Support::Quote {
                        page: span.page,
                        quote: quote_text(extraction, span, &fallback),
                    });
                }

                own_sources.insert(source_id.to_owned());
                links.push(Link {
                    kind: LinkKind::VersionOf,
                    target: source_id.to_owned(),
                    by: By::Correlator,
                    support,
                });
            }
        }

        // (5) Related units and (6) the open question, both from one query of page 1.
        if let Some(query_text) = query_text(candidate) {
            let hits = index
                .query(&Query::new(query_text.clone(), Scope::all(), usize::MAX))
                .map_err(|error| StageError::Failed(error.code))?;
            let mut related: Vec<Hit> = hits
                .hits
                .into_iter()
                .filter(|hit| {
                    view.has_target(&hit.unit.id) && !is_own_source(&hit.unit.id, &own_sources)
                })
                .take(RELATED_LIMIT)
                .collect();
            related.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| a.unit.id.cmp(&b.unit.id))
            });
            for hit in related {
                links.push(Link {
                    kind: LinkKind::Related,
                    target: hit.unit.id.clone(),
                    by: By::Correlator,
                    support: vec![Support::Id {
                        id: hit.unit.id,
                        score: hit.score,
                    }],
                });
            }

            let question_scope =
                Scope::new(["question"]).map_err(|error| StageError::Failed(error.code))?;
            let questions = index
                .query(&Query::new(query_text, question_scope, usize::MAX))
                .map_err(|error| StageError::Failed(error.code))?;
            for hit in questions.hits {
                let Ok(id) = QuestionId::parse(&hit.unit.id) else {
                    continue;
                };
                let Some(row) = view.question(&id) else {
                    continue;
                };
                if row.status != QuestionStatus::Open {
                    continue;
                }
                if hit.score >= PURPOSE_MIN_SCORE {
                    links.push(Link {
                        kind: LinkKind::Answers,
                        target: hit.unit.id.clone(),
                        by: By::Correlator,
                        support: vec![Support::Id {
                            id: hit.unit.id,
                            score: hit.score,
                        }],
                    });
                }
                break;
            }
        }

        Ok(links)
    }
}

/// Maps the searched spelling of every Corpus row id to the id as written.
fn canonical_targets(view: &CorpusView) -> BTreeMap<String, String> {
    let mut targets = BTreeMap::new();
    for located in view.sources() {
        let source = &located.value;
        let source_id = source.source_id.as_str();
        targets
            .entry(normalise(source_id))
            .or_insert_with(|| source_id.to_owned());
        let version_id = format!("{source_id}-v{}", source.version);
        targets.entry(normalise(&version_id)).or_insert(version_id);
    }
    for located in view.claims() {
        let id = located.value.claim_id.as_str().to_owned();
        targets.entry(normalise(&id)).or_insert(id);
    }
    for located in view.questions() {
        let id = located.value.question_id.as_str().to_owned();
        targets.entry(normalise(&id)).or_insert(id);
    }
    for located in view.conflicts() {
        let id = located.value.conflict_id.as_str().to_owned();
        targets.entry(normalise(&id)).or_insert(id);
    }
    targets
}

/// The first line and first [`QUERY_WORDS`] words of page 1, joined by a space.
fn query_text(candidate: &Candidate) -> Option<String> {
    let mut parts = Vec::new();
    if let Some((_, line)) = candidate.first_line() {
        parts.push(line.to_owned());
    }
    if let Some(words) = candidate.first_words(QUERY_WORDS) {
        parts.push(words);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// The text a [`quote::Span`] matched, or `fallback` when the slice cannot be read.
fn quote_text(extraction: &Extraction, span: quote::Span, fallback: &str) -> String {
    extraction
        .pages
        .iter()
        .find(|page| page.number == span.page)
        .and_then(|page| page.text.get(span.start..span.end))
        .map_or_else(|| fallback.to_owned(), str::to_owned)
}

/// Whether `id` is one of the candidate's own source units (its source id or a version of it).
fn is_own_source(id: &str, own_sources: &BTreeSet<String>) -> bool {
    own_sources
        .iter()
        .any(|source_id| id == source_id || id.starts_with(&format!("{source_id}-v")))
}

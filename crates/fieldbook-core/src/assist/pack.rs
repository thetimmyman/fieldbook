//! Pure context pack builder (`page-context-packs` and `handoff-context`).
//!
//! **Responsibility.** Builds [`ContextPack`]s from a [`LabelledView`] under a Unicode-scalar
//! budget, for both page context packs ([`page_pack`], [`PackSource`]) and question-driven
//! context packs ([`context_pack`], [`context_pack_for`]).
//!
//! **Rules.** Pure: no I/O, no clock, no environment. `page_pack` preserves its exact `to_text()`
//! output bytes.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::check::PageEntry;
use crate::check::markdown;
use crate::cite::KeyKind;
use crate::egress::{Dest, LabelledView};
use crate::ids::{ClaimId, ConflictId, PageId, VersionId};
use crate::labels::{LabelId, LabelSet};
use crate::policy::EffectivePolicy;
use crate::schema::vocab::{AuthorityTier, Lifecycle, Verification};
use crate::schema::{ClaimRow, ConflictRow, GlossaryRow, QuestionRow, SourceRow};
use crate::search::index::DocEntry;
use crate::search::units::{
    build_corpus_units_with, extract_first_paragraph_text, kept_glossary, labels_of,
};
use crate::search::{Glossary, Index, Unit, query};
use crate::strings;

use super::context::{
    Budget, ContextItem, Include, Origin, RequestBudget, Scope, Why, question_section_scalars,
    rules_preamble_scalars, scalar_count,
};
use super::view::{UnitRecord, ViewCatalog, view_fingerprint};

/// Top related units limit (`8`).
pub const RELATED_LIMIT: usize = 8;

/// One whole block of a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackBlock {
    /// Unit identifier of the block.
    pub id: String,
    /// Kind of the block (`page`, `clause`, `source`, `claim`, `question`, `conflict`, `term`).
    pub kind: String,
    /// Complete block text without a trailing newline.
    pub text: String,
}

/// A self-contained context pack built for an assistant prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextPack {
    request_budget: RequestBudget,
    question: String,
    scope: Scope,
    items: Vec<ContextItem>,
    chars: usize,
    budget: usize,
    labels: LabelSet,
    left_out_line: String,
    view_fingerprint: String,
    #[serde(skip)]
    blocks: Vec<PackBlock>,
    #[serde(skip)]
    left_out_budget: Vec<PackBlock>,
    #[serde(skip)]
    preamble: String,
    #[serde(skip)]
    question_labels: Option<LabelSet>,
    #[serde(skip)]
    banners: BTreeMap<LabelId, String>,
}

impl ContextPack {
    /// Returns the included blocks of the context pack in selection order.
    #[must_use]
    pub fn blocks(&self) -> &[PackBlock] {
        &self.blocks
    }

    /// Returns the union of data labels carried by the included blocks, preamble, and question.
    #[must_use]
    pub const fn labels(&self) -> &LabelSet {
        &self.labels
    }

    /// Returns the blocks dropped due to character budget constraints, in selection order.
    #[must_use]
    pub fn left_out_budget(&self) -> &[PackBlock] {
        &self.left_out_budget
    }

    /// Returns the question text (empty for a page context pack).
    #[must_use]
    pub fn question(&self) -> &str {
        &self.question
    }

    /// Returns the selection [`Scope`].
    #[must_use]
    pub const fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Returns every considered [`ContextItem`] in order (included and excluded).
    #[must_use]
    pub fn items(&self) -> &[ContextItem] {
        &self.items
    }

    /// Returns the Unicode scalar count of [`Self::to_text`].
    #[must_use]
    pub const fn chars(&self) -> usize {
        self.chars
    }

    /// Returns the configured character budget limit.
    #[must_use]
    pub const fn budget(&self) -> usize {
        self.budget
    }

    /// Returns the full [`RequestBudget`] accounting for this pack.
    #[must_use]
    pub const fn request_budget(&self) -> &RequestBudget {
        &self.request_budget
    }

    /// Returns the final policy-omission line appended to the pack text.
    #[must_use]
    pub fn left_out_line(&self) -> &str {
        &self.left_out_line
    }

    /// Returns the `view_fingerprint` of the [`LabelledView`] this pack was built from.
    #[must_use]
    pub fn view_fingerprint(&self) -> &str {
        &self.view_fingerprint
    }

    pub(crate) const fn question_labels_ref(&self) -> Option<&LabelSet> {
        self.question_labels.as_ref()
    }

    pub(crate) const fn banners_ref(&self) -> &BTreeMap<LabelId, String> {
        &self.banners
    }

    /// Serializes the context pack to plain text according to the frozen pack layout (`LF` only,
    /// one trailing newline, no timestamp).
    #[must_use]
    pub fn to_text(&self) -> String {
        render_pack_text(
            &self.preamble,
            &self.labels,
            &self.banners,
            &self.question,
            &self.blocks,
            &self.left_out_budget,
            &self.left_out_line,
        )
    }
}

fn render_pack_text(
    preamble: &str,
    labels: &LabelSet,
    banners: &BTreeMap<LabelId, String>,
    question: &str,
    blocks: &[PackBlock],
    left_out_budget: &[PackBlock],
    left_out_line: &str,
) -> String {
    let mut lines = Vec::new();

    if !preamble.is_empty() {
        for line in preamble.lines() {
            lines.push(line.to_owned());
        }
        lines.push(String::new());
    }

    let label_ids: Vec<&str> = labels.ids().iter().map(LabelId::as_str).collect();
    lines.push(format!("Labels: {}", label_ids.join(", ")));
    for id in labels.ids() {
        if let Some(banner) = banners.get(id) {
            lines.push(banner.clone());
        }
    }

    if !question.is_empty() {
        lines.push(String::new());
        lines.push(format!("Question: {question}"));
    }

    for block in blocks {
        lines.push(String::new());
        for line in block.text.lines() {
            lines.push(line.to_owned());
        }
    }

    if !left_out_budget.is_empty() {
        lines.push(String::new());
        lines.push(strings::left_out_budget().to_owned());
        for b in left_out_budget {
            lines.push(format!("- {} {}", b.kind, b.id));
        }
    }

    lines.push(String::new());
    lines.push(left_out_line.to_owned());

    format!("{}\n", lines.join("\n"))
}

/// In-memory search index and labels built once from a view for generating context packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSource<'a> {
    view: &'a LabelledView,
    index: Index,
    labels: BTreeMap<String, LabelSet>,
    fingerprint: String,
}

impl<'a> PackSource<'a> {
    /// Builds a [`PackSource`] by indexing every unit retained by `view`.
    #[must_use]
    pub fn new(view: &'a LabelledView) -> Self {
        let (units, glossary) = view_units_and_glossary(view);
        let mut labels = BTreeMap::new();
        for u in &units {
            labels.insert(u.id.clone(), labels_of(u).clone());
        }
        let index = Index::build(units, &glossary);
        let fingerprint = view_fingerprint(view);
        Self {
            view,
            index,
            labels,
            fingerprint,
        }
    }

    /// Builds a [`ContextPack`] for `page_id` under `budget` characters for `dest`.
    #[must_use]
    pub fn page_pack(&self, dest: &Dest, page_id: &PageId, budget: usize) -> ContextPack {
        let preamble = read_preamble(self.view);
        let left_out_line = self.view.left_out_line(dest);
        let scope = Scope::Page(page_id.as_str().to_owned());

        let Some(page) = self.view.pages().iter().find(|p| &p.id == page_id) else {
            let labels = if !preamble.is_empty() {
                LabelSet::public()
            } else {
                LabelSet::unlabelled()
            };
            let text = render_pack_text(
                &preamble,
                &labels,
                &BTreeMap::new(),
                "",
                &[],
                &[],
                &left_out_line,
            );
            let total = scalar_count(&text);
            let request_budget = RequestBudget {
                limit: budget,
                rules: total,
                question: 0,
                history: 0,
                context: 0,
                total,
                sendable: total <= budget,
                over_by: total.saturating_sub(budget),
            };
            return ContextPack {
                request_budget,
                question: String::new(),
                scope,
                items: Vec::new(),
                chars: total,
                budget,
                labels,
                left_out_line,
                view_fingerprint: self.fingerprint.clone(),
                blocks: Vec::new(),
                left_out_budget: Vec::new(),
                preamble,
                question_labels: None,
                banners: BTreeMap::new(),
            };
        };

        let claims_by_id: BTreeMap<&str, &ClaimRow> = self
            .view
            .claims()
            .iter()
            .map(|c| (c.value.claim_id.as_str(), &c.value))
            .collect();

        let conflicts_by_id: BTreeMap<&str, &ConflictRow> = self
            .view
            .conflicts()
            .iter()
            .map(|x| (x.value.conflict_id.as_str(), &x.value))
            .collect();

        let sources_by_id: BTreeMap<String, &SourceRow> = self
            .view
            .sources()
            .iter()
            .map(|s| {
                (
                    format!("{}-v{}", s.value.source_id, s.value.version),
                    &s.value,
                )
            })
            .collect();

        let mut seen_block_ids = BTreeSet::new();
        let mut selection_units: Vec<Vec<(PackBlock, Why, Option<String>)>> = Vec::new();

        // 1. The page (own-page block)
        let page_block = format_page_block(page);
        seen_block_ids.insert(page_block.id.clone());
        selection_units.push(vec![(page_block, Why::Scope, None)]);

        // 2. Claims the page cites
        for c in self.view.citations(page_id) {
            if c.key_kind != KeyKind::Claim {
                continue;
            }
            let Ok(claim_id) = ClaimId::parse(&c.key) else {
                continue;
            };
            let Some(claim) = claims_by_id.get(claim_id.as_str()) else {
                continue;
            };
            if let Some(unit) = build_claim_selection_unit(
                claim,
                &mut seen_block_ids,
                &conflicts_by_id,
                &claims_by_id,
            ) {
                let via = Some(page.id.as_str().to_owned());
                let tagged = unit
                    .into_iter()
                    .enumerate()
                    .map(|(idx, b)| {
                        let via_ref = if idx == 0 {
                            via.clone()
                        } else {
                            Some(claim.claim_id.as_str().to_owned())
                        };
                        (b, Why::Closure, via_ref)
                    })
                    .collect();
                selection_units.push(tagged);
            }
        }

        // 3. Related units
        let results = query(&self.index, &page.meta.title, 200);
        let page_prefix = format!("{}#", page.id);
        let mut related_count = 0;

        for hit in &results.hits {
            if related_count >= RELATED_LIMIT {
                break;
            }

            // Skip: any unit of the page itself
            if hit.id == page.id.as_str() || hit.id.starts_with(&page_prefix) {
                continue;
            }

            // Skip: kind "record"/"text"
            if hit.kind == "record" || hit.kind == "text" {
                continue;
            }

            // Already emitted by an earlier unit: the hit is one of the top related units, and
            // the block is de-duplicated, not repeated.
            if seen_block_ids.contains(&hit.id) {
                related_count += 1;
                continue;
            }

            if hit.kind == "claim" {
                let Some(claim) = claims_by_id.get(hit.id.as_str()) else {
                    continue;
                };
                if claim.verification == Verification::Contradicted
                    || !claim.superseded_by.is_empty()
                {
                    continue;
                }

                if let Some(unit) = build_claim_selection_unit(
                    claim,
                    &mut seen_block_ids,
                    &conflicts_by_id,
                    &claims_by_id,
                ) {
                    let tagged = unit
                        .into_iter()
                        .enumerate()
                        .map(|(idx, b)| {
                            if idx == 0 {
                                (b, Why::Hit, None)
                            } else {
                                (b, Why::Closure, Some(claim.claim_id.as_str().to_owned()))
                            }
                        })
                        .collect();
                    selection_units.push(tagged);
                    related_count += 1;
                }
                continue;
            }

            if hit.kind == "source" {
                let Some(source) = sources_by_id.get(&hit.id) else {
                    continue;
                };
                let is_superseded = VersionId::parse(&hit.id)
                    .ok()
                    .and_then(|vid| self.view.index().source_versions.get(&vid))
                    .is_some_and(|entry| entry.superseded);
                if source.authority_tier == Some(AuthorityTier::D)
                    || source.lifecycle == Lifecycle::Withdrawn
                    || is_superseded
                {
                    continue;
                }
            }

            if hit.kind == "conflict" {
                let Some(conflict) = conflicts_by_id.get(hit.id.as_str()) else {
                    continue;
                };
                let block = format_conflict_block(conflict);
                seen_block_ids.insert(block.id.clone());
                selection_units.push(vec![(block, Why::Hit, None)]);
                related_count += 1;
                continue;
            }

            // Kind page, clause, source, question, term
            if let Some(doc) = self.index.docs.get(hit.unit) {
                let block = format_doc_block(doc);
                seen_block_ids.insert(block.id.clone());
                selection_units.push(vec![(block, Why::Hit, None)]);
                related_count += 1;
            }
        }

        // Budget evaluation (exact page-context-packs logic so page_pack().to_text() bytes never change)
        let mut running_total = preamble.chars().count();
        let mut kept_blocks: Vec<PackBlock> = Vec::new();
        let mut left_out_budget: Vec<PackBlock> = Vec::new();
        let mut left_out_seen = BTreeSet::new();
        let mut items: Vec<ContextItem> = Vec::new();
        let mut budget_exceeded = false;

        for unit in selection_units {
            if !budget_exceeded {
                let unit_size: usize = unit
                    .iter()
                    .map(|(b, _, _)| b.text.chars().count() + 2)
                    .sum();
                if running_total + unit_size <= budget {
                    running_total += unit_size;
                    for (block, why, via) in unit {
                        items.push(ContextItem {
                            unit: block.id.clone(),
                            origin: Origin::Auto,
                            why,
                            via,
                            included: true,
                            stale: false,
                        });
                        kept_blocks.push(block);
                    }
                    continue;
                }
                budget_exceeded = true;
            }
            for (block, why, via) in unit {
                if left_out_seen.insert(block.id.clone()) {
                    items.push(ContextItem {
                        unit: block.id.clone(),
                        origin: Origin::Auto,
                        why,
                        via,
                        included: false,
                        stale: false,
                    });
                    left_out_budget.push(block);
                }
            }
        }

        // Labels
        let mut acc: Option<LabelSet> = if !preamble.is_empty() {
            Some(LabelSet::public())
        } else {
            None
        };

        for block in &kept_blocks {
            let block_label = self
                .labels
                .get(&block.id)
                .cloned()
                .unwrap_or_else(LabelSet::unlabelled);
            acc = match acc {
                Some(cur) => Some(cur.union(&block_label)),
                None => Some(block_label),
            };
        }

        let labels = acc.unwrap_or_else(LabelSet::unlabelled);
        let text = render_pack_text(
            &preamble,
            &labels,
            &BTreeMap::new(),
            "",
            &kept_blocks,
            &left_out_budget,
            &left_out_line,
        );
        let total = scalar_count(&text);
        let context_chars: usize = kept_blocks
            .iter()
            .map(|b| scalar_count(&b.text) + 2)
            .sum::<usize>()
            + left_out_section_scalars(&left_out_budget);
        let rules_chars = total.saturating_sub(context_chars);
        let request_budget = RequestBudget {
            limit: budget,
            rules: rules_chars,
            question: 0,
            history: 0,
            context: context_chars,
            total,
            sendable: total <= budget,
            over_by: total.saturating_sub(budget),
        };

        ContextPack {
            request_budget,
            question: String::new(),
            scope,
            items,
            chars: total,
            budget,
            labels,
            left_out_line,
            view_fingerprint: self.fingerprint.clone(),
            blocks: kept_blocks,
            left_out_budget,
            preamble,
            question_labels: None,
            banners: BTreeMap::new(),
        }
    }
}

/// The in-memory index of the view: every unit the view kept, every kind.
#[must_use]
pub fn index_of(view: &LabelledView) -> Index {
    let (units, glossary) = view_units_and_glossary(view);
    Index::build(units, &glossary)
}

/// Convenience: `PackSource::new(view).page_pack(dest, page_id, budget)`.
#[must_use]
pub fn page_pack(view: &LabelledView, dest: &Dest, page_id: &PageId, budget: usize) -> ContextPack {
    PackSource::new(view).page_pack(dest, page_id, budget)
}

/// Builds a question-driven [`ContextPack`] from `view` for the `handoff` destination under
/// `scope`, `budget`, and `include` (`handoff-context` step 2).
///
/// Uses `LabelSet::owner_supplied()` for the question and `Dest::Handoff` for the omission line.
/// Use [`context_pack_for`] when supplying a specific destination, policy banners, or
/// marking-derived question labels.
#[must_use]
pub fn context_pack(
    view: &LabelledView,
    question: &str,
    scope: &Scope,
    budget: impl Into<Budget>,
    include: &Include,
) -> ContextPack {
    context_pack_for(
        view,
        &Dest::Handoff,
        None,
        question,
        &LabelSet::owner_supplied(),
        scope,
        budget,
        include,
    )
}

/// Builds a question-driven [`ContextPack`] with explicit destination, optional policy (for
/// label banner lines), and pre-labelled question [`LabelSet`].
#[expect(
    clippy::too_many_arguments,
    reason = "context pack construction names view, destination, optional policy, question, question labels, scope, budget, and include flags"
)]
#[must_use]
pub fn context_pack_for(
    view: &LabelledView,
    dest: &Dest,
    policy: Option<&EffectivePolicy>,
    question: &str,
    question_labels: &LabelSet,
    scope: &Scope,
    budget: impl Into<Budget>,
    include: &Include,
) -> ContextPack {
    let catalog = ViewCatalog::from_view(view);
    let budget = budget.into();
    let left_out_line = view.left_out_line(dest);
    let banners = collect_banners(policy);

    let mut items: Vec<ContextItem> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut non_budget_excluded: BTreeSet<String> = BTreeSet::new();
    let mut conflict_opponents: BTreeMap<String, ConflictId> = BTreeMap::new();

    // Helper: add a claim's conflict dependencies (conflict row + opposing claims).
    // Conflict dependencies of a selected claim are included even when contradicted.
    let add_claim_conflicts =
        |claim_id: &str,
         items: &mut Vec<ContextItem>,
         seen: &mut BTreeSet<String>,
         non_budget_excluded: &mut BTreeSet<String>,
         conflict_opponents: &mut BTreeMap<String, ConflictId>| {
            // A conflict dependency seen earlier as a default exclusion (contradicted,
            // superseded, Tier D) is kept with the claim that needs it: HG-02 keeps a claim and
            // its conflict together, whichever of them the walk reached first.
            let promote = |id: &str,
                           items: &mut Vec<ContextItem>,
                           non_budget_excluded: &mut BTreeSet<String>| {
                if non_budget_excluded.remove(id)
                    && let Some(item) = items
                        .iter_mut()
                        .find(|item| item.unit == id && !item.included)
                {
                    item.included = true;
                }
            };
            if let Some(entries) = catalog.claim_conflicts.get(claim_id) {
                for (xid, opponents) in entries {
                    let xid_str = xid.as_str().to_owned();
                    if catalog.units.contains_key(&xid_str) {
                        if seen.insert(xid_str.clone()) {
                            items.push(ContextItem {
                                unit: xid_str,
                                origin: Origin::Auto,
                                why: Why::Closure,
                                via: Some(claim_id.to_owned()),
                                included: true,
                                stale: false,
                            });
                        } else {
                            promote(&xid_str, items, non_budget_excluded);
                        }
                    }
                    for opp in opponents {
                        let opp_str = opp.as_str().to_owned();
                        conflict_opponents
                            .entry(opp_str.clone())
                            .or_insert_with(|| xid.clone());
                        if catalog.units.contains_key(&opp_str) {
                            if seen.insert(opp_str.clone()) {
                                items.push(ContextItem {
                                    unit: opp_str,
                                    origin: Origin::Auto,
                                    why: Why::Closure,
                                    via: Some(claim_id.to_owned()),
                                    included: true,
                                    stale: false,
                                });
                            } else {
                                promote(&opp_str, items, non_budget_excluded);
                            }
                        }
                    }
                }
            }
        };

    // Helper: add a claim's source version and linked terms as Closure items.
    let add_claim_closure =
        |claim_id: &str,
         items: &mut Vec<ContextItem>,
         seen: &mut BTreeSet<String>,
         non_budget_excluded: &mut BTreeSet<String>| {
            let Some(claim_unit) = catalog.units.get(claim_id) else {
                return;
            };
            for dep_id in &claim_unit.closure {
                let Some(dep_unit) = catalog.units.get(dep_id) else {
                    continue;
                };
                if !seen.insert(dep_id.clone()) {
                    continue;
                }
                let named = question_names_id(question, dep_id);
                if is_default_excluded(dep_unit, include, named) {
                    items.push(ContextItem {
                        unit: dep_id.clone(),
                        origin: Origin::Auto,
                        why: if named { Why::Named } else { Why::Closure },
                        via: if named {
                            None
                        } else {
                            Some(claim_id.to_owned())
                        },
                        included: false,
                        stale: false,
                    });
                    non_budget_excluded.insert(dep_id.clone());
                } else {
                    items.push(ContextItem {
                        unit: dep_id.clone(),
                        origin: Origin::Auto,
                        why: if named { Why::Named } else { Why::Closure },
                        via: if named {
                            None
                        } else {
                            Some(claim_id.to_owned())
                        },
                        included: true,
                        stale: false,
                    });
                }
            }
        };

    // Helper: expand a selected unit (hit or scope entry) with its AD-18 closure and conflicts.
    let expand_unit =
        |unit_id: &str,
         why: Why,
         via: Option<String>,
         items: &mut Vec<ContextItem>,
         seen: &mut BTreeSet<String>,
         non_budget_excluded: &mut BTreeSet<String>,
         conflict_opponents: &mut BTreeMap<String, ConflictId>| {
            let Some(unit) = catalog.units.get(unit_id) else {
                return;
            };
            if !seen.insert(unit_id.to_owned()) {
                return;
            }
            let named = matches!(why, Why::Named) || question_names_id(question, unit_id);
            let effective_why = if matches!(why, Why::Hit) && named {
                Why::Named
            } else {
                why
            };

            if is_default_excluded(unit, include, named) {
                items.push(ContextItem {
                    unit: unit_id.to_owned(),
                    origin: Origin::Auto,
                    why: effective_why,
                    via,
                    included: false,
                    stale: false,
                });
                non_budget_excluded.insert(unit_id.to_owned());
                return;
            }

            items.push(ContextItem {
                unit: unit_id.to_owned(),
                origin: Origin::Auto,
                why: effective_why,
                via,
                included: true,
                stale: false,
            });

            match unit.kind.as_str() {
                "claim" => {
                    add_claim_conflicts(
                        unit_id,
                        items,
                        seen,
                        non_budget_excluded,
                        conflict_opponents,
                    );
                    add_claim_closure(unit_id, items, seen, non_budget_excluded);
                }
                "page" | "clause" => {
                    for dep_id in &unit.closure {
                        let Some(dep_unit) = catalog.units.get(dep_id) else {
                            continue;
                        };
                        if !seen.insert(dep_id.clone()) {
                            continue;
                        }
                        let dep_named = question_names_id(question, dep_id);
                        if is_default_excluded(dep_unit, include, dep_named) {
                            items.push(ContextItem {
                                unit: dep_id.clone(),
                                origin: Origin::Auto,
                                why: Why::Closure,
                                via: Some(unit_id.to_owned()),
                                included: false,
                                stale: false,
                            });
                            non_budget_excluded.insert(dep_id.clone());
                            continue;
                        }
                        items.push(ContextItem {
                            unit: dep_id.clone(),
                            origin: Origin::Auto,
                            why: Why::Closure,
                            via: Some(unit_id.to_owned()),
                            included: true,
                            stale: false,
                        });
                        if dep_unit.kind == "claim" {
                            add_claim_conflicts(
                                dep_id,
                                items,
                                seen,
                                non_budget_excluded,
                                conflict_opponents,
                            );
                            add_claim_closure(dep_id, items, seen, non_budget_excluded);
                        }
                    }
                }
                "source" => {
                    for dep_id in &unit.closure {
                        let Some(dep_unit) = catalog.units.get(dep_id) else {
                            continue;
                        };
                        if !seen.insert(dep_id.clone()) {
                            continue;
                        }
                        let dep_named = question_names_id(question, dep_id);
                        let excluded = is_default_excluded(dep_unit, include, dep_named);
                        items.push(ContextItem {
                            unit: dep_id.clone(),
                            origin: Origin::Auto,
                            why: Why::Closure,
                            via: Some(unit_id.to_owned()),
                            included: !excluded,
                            stale: false,
                        });
                        if excluded {
                            non_budget_excluded.insert(dep_id.clone());
                        }
                    }
                }
                _ => {}
            }
        };

    // (a) Scope::Page puts the page's own unit first (why = Scope) and its cited claims/closure next.
    if let Scope::Page(page_id) = scope {
        expand_unit(
            page_id,
            Why::Scope,
            None,
            &mut items,
            &mut seen,
            &mut non_budget_excluded,
            &mut conflict_opponents,
        );
    }

    // (a) Top 8 search hits over `index_of(view)` (already built in `catalog.index`).
    let results = query(&catalog.index, question, catalog.index.docs.len().max(1));
    let mut hit_count = 0usize;
    let page_prefix = match scope {
        Scope::Page(pid) => Some((pid.as_str(), format!("{pid}#"))),
        Scope::All | Scope::Area(_) => None,
    };

    for hit in &results.hits {
        if hit_count >= RELATED_LIMIT {
            break;
        }
        let Some(unit) = catalog.units.get(&hit.id) else {
            continue;
        };
        if let Scope::Area(code) = scope
            && unit.area != *code
        {
            continue;
        }
        if let Some((pid, prefix)) = &page_prefix
            && (hit.id == *pid || hit.id.starts_with(prefix))
        {
            continue;
        }
        hit_count += 1;
        let why = if question_names_id(question, &hit.id) {
            Why::Named
        } else {
            Why::Hit
        };
        expand_unit(
            &hit.id,
            why,
            None,
            &mut items,
            &mut seen,
            &mut non_budget_excluded,
            &mut conflict_opponents,
        );
    }

    let q_labels = if question.is_empty() {
        None
    } else {
        Some(question_labels.clone())
    };
    let mandatory = BTreeSet::new();

    finalize_pack(
        view,
        &catalog,
        question.to_owned(),
        scope.clone(),
        q_labels,
        banners,
        left_out_line,
        items,
        &mandatory,
        &non_budget_excluded,
        &conflict_opponents,
        budget,
        false,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "shared finalization builds atomic dependency groups, trims optional groups in reverse rank order, and computes RequestBudget"
)]
pub(crate) fn finalize_pack(
    view: &LabelledView,
    catalog: &ViewCatalog,
    question: String,
    scope: Scope,
    question_labels: Option<LabelSet>,
    banners: BTreeMap<LabelId, String>,
    left_out_line: String,
    mut items: Vec<ContextItem>,
    mandatory: &BTreeSet<String>,
    non_budget_excluded: &BTreeSet<String>,
    conflict_opponents: &BTreeMap<String, ConflictId>,
    budget: Budget,
    has_unavailable_pin: bool,
) -> ContextPack {
    let preamble = read_preamble(view);
    let fp = catalog.fingerprint();

    // Build atomic dependency groups over currently included items so a claim and its conflict
    // dependencies are always kept or dropped together.
    let groups = build_dependency_groups(&items, catalog, mandatory);

    // Track which groups remain included (start with all groups included, trim optional groups in
    // reverse rank order until the exact serialized scalar count fits `budget.limit`).
    let mut group_included = vec![true; groups.len()];

    let build_blocks_and_budget = |items: &[ContextItem],
                                   groups: &[DependencyGroup],
                                   group_included: &[bool]|
     -> (
        Vec<PackBlock>,
        Vec<PackBlock>,
        LabelSet,
        String,
        RequestBudget,
    ) {
        let mut included_ids = BTreeSet::new();
        let mut budget_dropped_ids = BTreeSet::new();
        for (g_idx, grp) in groups.iter().enumerate() {
            for &item_idx in &grp.item_indices {
                if group_included[g_idx] {
                    included_ids.insert(items[item_idx].unit.clone());
                } else {
                    budget_dropped_ids.insert(items[item_idx].unit.clone());
                }
            }
        }

        let mut blocks = Vec::new();
        let mut left_out_budget = Vec::new();
        for item in items {
            if included_ids.contains(&item.unit) {
                if let Some(block) =
                    block_for_unit(&item.unit, catalog, conflict_opponents, &included_ids)
                {
                    blocks.push(block);
                }
            } else if budget_dropped_ids.contains(&item.unit)
                && !non_budget_excluded.contains(&item.unit)
                && let Some(block) =
                    block_for_unit(&item.unit, catalog, conflict_opponents, &included_ids)
            {
                left_out_budget.push(block);
            }
        }

        let labels = compute_pack_labels(
            &preamble,
            question_labels.as_ref(),
            &blocks,
            &catalog.labels,
        );
        let text = render_pack_text(
            &preamble,
            &labels,
            &banners,
            &question,
            &blocks,
            &left_out_budget,
            &left_out_line,
        );

        let text_scalars = scalar_count(&text);
        let q_scalars = question_section_scalars(&question);
        let context_scalars: usize = blocks
            .iter()
            .map(|b| scalar_count(&b.text) + 2)
            .sum::<usize>()
            + left_out_section_scalars(&left_out_budget);
        // `rules` holds preamble + fixed pack framing (labels line, banners, footer) so that
        // `rules + question + history + context + repair == total` and equals exact scalars.
        let rules_scalars = text_scalars
            .saturating_sub(q_scalars)
            .saturating_sub(context_scalars)
            .max(rules_preamble_scalars(&preamble));
        let total = text_scalars
            .saturating_add(budget.history)
            .saturating_add(budget.repair);
        let over_by = total.saturating_sub(budget.limit);
        let sendable = total <= budget.limit && !has_unavailable_pin;

        let request_budget = RequestBudget {
            limit: budget.limit,
            rules: rules_scalars,
            question: q_scalars,
            history: budget.history,
            context: context_scalars,
            total,
            sendable,
            over_by,
        };

        (blocks, left_out_budget, labels, text, request_budget)
    };

    loop {
        let (_, _, _, _, rb) = build_blocks_and_budget(&items, &groups, &group_included);
        if rb.total <= budget.limit {
            break;
        }
        // Trim the last (lowest-ranked) optional dependency group.
        let Some(drop_idx) = (0..groups.len())
            .rev()
            .find(|&i| group_included[i] && !groups[i].mandatory)
        else {
            break;
        };
        group_included[drop_idx] = false;
    }

    // Apply final group inclusion back to `items`.
    for (g_idx, grp) in groups.iter().enumerate() {
        let keep = group_included[g_idx];
        for &item_idx in &grp.item_indices {
            items[item_idx].included = keep;
        }
    }

    let (blocks, left_out_budget, labels, text, request_budget) =
        build_blocks_and_budget(&items, &groups, &group_included);
    let chars = scalar_count(&text);

    ContextPack {
        request_budget,
        question,
        scope,
        items,
        chars,
        budget: budget.limit,
        labels,
        left_out_line,
        view_fingerprint: fp,
        blocks,
        left_out_budget,
        preamble,
        question_labels,
        banners,
    }
}

struct DependencyGroup {
    item_indices: Vec<usize>,
    mandatory: bool,
}

/// Groups included items into atomic dependency groups in selection order.
///
/// Any claim and its conflict row(s) and opposing claims form a single connected group so they
/// are always kept or dropped together. If any member of a group is mandatory (pinned or required
/// pin closure), the entire group is mandatory.
fn build_dependency_groups(
    items: &[ContextItem],
    catalog: &ViewCatalog,
    mandatory: &BTreeSet<String>,
) -> Vec<DependencyGroup> {
    let included_pos: BTreeMap<&str, usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.included && catalog.units.contains_key(&item.unit))
        .map(|(idx, item)| (item.unit.as_str(), idx))
        .collect();

    // Build undirected adjacency for conflict-coupled items (claim <-> conflict <-> opponent claim).
    let mut adj: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (&id, &idx) in &included_pos {
        if let Some(unit) = catalog.units.get(id)
            && unit.kind == "claim"
        {
            for dep_id in &unit.conflicts {
                if let Some(&dep_idx) = included_pos.get(dep_id.as_str()) {
                    adj.entry(idx).or_default().insert(dep_idx);
                    adj.entry(dep_idx).or_default().insert(idx);
                }
            }
        }
    }

    let mut visited = BTreeSet::new();
    let mut groups = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if !item.included || !catalog.units.contains_key(&item.unit) || visited.contains(&idx) {
            continue;
        }
        // BFS over conflict-coupled neighbors.
        let mut comp = Vec::new();
        let mut queue = vec![idx];
        visited.insert(idx);
        while let Some(cur) = queue.pop() {
            comp.push(cur);
            if let Some(neighbors) = adj.get(&cur) {
                for &n in neighbors {
                    if visited.insert(n) {
                        queue.push(n);
                    }
                }
            }
        }
        comp.sort_unstable();
        let is_mandatory = comp.iter().any(|&i| mandatory.contains(&items[i].unit));
        groups.push(DependencyGroup {
            item_indices: comp,
            mandatory: is_mandatory,
        });
    }
    groups
}

fn block_for_unit(
    unit_id: &str,
    catalog: &ViewCatalog,
    conflict_opponents: &BTreeMap<String, ConflictId>,
    included_ids: &BTreeSet<String>,
) -> Option<PackBlock> {
    let unit = catalog.units.get(unit_id)?;
    if unit.kind == "claim"
        && let Some(xid) = conflict_opponents.get(unit_id)
        && included_ids.contains(xid.as_str())
        && let Some(claim) = catalog.claims_by_id.get(unit_id)
    {
        // Only apply opponent styling if this claim is not the first-selected claim of that conflict
        // OR if it was recorded as an opponent via `conflict_opponents`.
        return Some(format_claim_block(claim, Some(xid)));
    }
    Some(PackBlock {
        id: unit.id.clone(),
        kind: unit.kind.clone(),
        text: unit.block.clone(),
    })
}

fn compute_pack_labels(
    preamble: &str,
    question_labels: Option<&LabelSet>,
    blocks: &[PackBlock],
    unit_labels: &BTreeMap<String, LabelSet>,
) -> LabelSet {
    let mut acc: Option<LabelSet> = if !preamble.is_empty() {
        Some(LabelSet::public())
    } else {
        None
    };
    if let Some(ql) = question_labels {
        acc = match acc {
            Some(cur) => Some(cur.union(ql)),
            None => Some(ql.clone()),
        };
    }
    for block in blocks {
        let block_label = unit_labels
            .get(&block.id)
            .cloned()
            .unwrap_or_else(LabelSet::unlabelled);
        acc = match acc {
            Some(cur) => Some(cur.union(&block_label)),
            None => Some(block_label),
        };
    }
    acc.unwrap_or_else(LabelSet::unlabelled)
}

fn collect_banners(policy: Option<&EffectivePolicy>) -> BTreeMap<LabelId, String> {
    let mut banners = BTreeMap::new();
    if let Some(eff) = policy {
        let table = eff.label_table();
        for id in table.known_ids() {
            if let Some(banner) = eff.banner(id) {
                banners.insert(id.clone(), banner.to_owned());
            }
        }
    }
    banners
}

fn left_out_section_scalars(left_out: &[PackBlock]) -> usize {
    if left_out.is_empty() {
        return 0;
    }
    // "\n" + heading + "\n" + each "- <kind> <id>\n"
    let mut total = 1 + scalar_count(strings::left_out_budget()) + 1;
    for b in left_out {
        total += 2 + scalar_count(&b.kind) + 1 + scalar_count(&b.id) + 1;
    }
    total
}

fn is_default_excluded(unit: &UnitRecord, include: &Include, named: bool) -> bool {
    if named {
        return false;
    }
    if unit.tier.as_deref() == Some("D") && !include.tier_d {
        return true;
    }
    if unit.superseded && !include.superseded {
        return true;
    }
    if unit.contradicted && !include.contradicted {
        return true;
    }
    false
}

/// Returns `true` when `question` mentions `id` as a whole identifier token (case-insensitively).
fn question_names_id(question: &str, id: &str) -> bool {
    if question.is_empty() || id.is_empty() {
        return false;
    }
    let q_lower = question.to_ascii_lowercase();
    let id_lower = id.to_ascii_lowercase();
    let is_id_char = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'#');

    let q_bytes = q_lower.as_bytes();
    let id_len = id_lower.len();
    let mut start = 0usize;
    while let Some(rel) = q_lower[start..].find(&id_lower) {
        let pos = start + rel;
        let end = pos + id_len;
        let before_ok = pos == 0 || !is_id_char(q_bytes[pos - 1]);
        let after_ok = end == q_bytes.len() || !is_id_char(q_bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        start = pos + 1;
    }
    false
}

pub(crate) fn view_units_and_glossary(view: &LabelledView) -> (Vec<Unit>, Glossary) {
    let sources: Vec<SourceRow> = view.sources().iter().map(|s| s.value.clone()).collect();
    let claims: Vec<ClaimRow> = view.claims().iter().map(|c| c.value.clone()).collect();
    let questions: Vec<QuestionRow> = view.questions().iter().map(|q| q.value.clone()).collect();
    let conflicts: Vec<ConflictRow> = view.conflicts().iter().map(|x| x.value.clone()).collect();
    let terms: Vec<GlossaryRow> = view.glossary().iter().map(|t| t.value.clone()).collect();

    let mut quarantined = BTreeSet::new();
    for row in &sources {
        if row.lifecycle == Lifecycle::Quarantined
            && let Ok(vid) = VersionId::parse(&format!("{}-v{}", row.source_id, row.version))
        {
            quarantined.insert(vid);
        }
    }

    let glossary = kept_glossary(&terms, &sources, &claims, |_| true);
    let units = build_corpus_units_with(
        view.pages(),
        &sources,
        &claims,
        &questions,
        &conflicts,
        &terms,
        &quarantined,
        &glossary,
        |_| true,
    );
    (units, glossary)
}

pub(crate) fn read_preamble(view: &LabelledView) -> String {
    view.tree()
        .get_bytes("corpus/ASSISTANT.md")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or("")
        .to_owned()
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn page_lead(page: &PageEntry) -> String {
    let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
    let evs = markdown::events(&page.body, base_line);
    let first_h2_event = page
        .parts()
        .parts()
        .first()
        .map_or(evs.len(), |p| p.heading_event);
    let lead_evs = evs.get(..first_h2_event).unwrap_or(&[]);
    let mut lead = extract_first_paragraph_text(lead_evs);
    if lead.is_empty()
        && let Some(first_part) = page.parts().parts().first()
    {
        let first_sec_evs = evs.get(first_part.event_range.clone()).unwrap_or(&[]);
        lead = extract_first_paragraph_text(first_sec_evs);
    }
    collapse_whitespace(&lead)
}

fn format_page_block(page: &PageEntry) -> PackBlock {
    format_page_block_kind(page, "page")
}

pub(crate) fn format_page_block_kind(page: &PageEntry, kind: &str) -> PackBlock {
    let id = page.id.as_str().to_owned();
    let title = collapse_whitespace(&page.meta.title);
    let as_of = page.meta.as_of.to_string();
    let status = page.meta.status.as_str();

    let mut lines = Vec::new();
    lines.push(format!("--- {kind} {id} ---"));
    lines.push(format!("Title: {title}"));
    lines.push(format!("As of: {as_of}"));
    lines.push(format!("Status: {status}"));

    let lead = page_lead(page);
    if !lead.is_empty() {
        lines.push(format!("Text: {lead}"));
    }

    let text = lines.join("\n");
    PackBlock {
        id,
        kind: kind.to_owned(),
        text,
    }
}

pub(crate) fn format_claim_block(
    claim: &ClaimRow,
    closure_conflict_id: Option<&ConflictId>,
) -> PackBlock {
    let id = claim.claim_id.as_str().to_owned();
    let title = collapse_whitespace(&claim.statement);
    let as_of = claim
        .as_of
        .as_ref()
        .map_or_else(|| "unknown".to_owned(), ToString::to_string);

    let status = match closure_conflict_id {
        Some(xid) => format!("contradicted (conflict {xid})"),
        None => claim.verification.as_str().to_owned(),
    };

    let mut lines = Vec::new();
    lines.push(format!("--- claim {id} ---"));
    lines.push(format!("Title: {title}"));
    lines.push(format!("As of: {as_of}"));
    lines.push(format!("Status: {status}"));

    if let (Some(source_id), Some(version)) = (&claim.source_id, claim.version) {
        if let Some(ref loc) = claim.locator
            && !loc.is_empty()
        {
            lines.push(format!("Source: {source_id}-v{version}, {loc}"));
        } else {
            lines.push(format!("Source: {source_id}-v{version}"));
        }
    }

    if let Some(ref q) = claim.quote {
        let collapsed = collapse_whitespace(q);
        if !collapsed.is_empty() {
            lines.push(format!("Quote: \"{collapsed}\""));
        }
    }

    if let Some(ref xid) = claim.conflict_id {
        lines.push(format!("Conflict: {xid}"));
    }

    lines.push(format!("cite as: [@{id}]"));

    let text = lines.join("\n");
    PackBlock {
        id,
        kind: "claim".to_owned(),
        text,
    }
}

pub(crate) fn format_conflict_block(conflict: &ConflictRow) -> PackBlock {
    let id = conflict.conflict_id.as_str().to_owned();
    let title = collapse_whitespace(&conflict.topic);
    let as_of = conflict
        .resolved_on
        .as_ref()
        .map_or_else(|| "unknown".to_owned(), ToString::to_string);
    let status = conflict.status.as_str();

    let mut lines = Vec::new();
    lines.push(format!("--- conflict {id} ---"));
    lines.push(format!("Title: {title}"));
    lines.push(format!("As of: {as_of}"));
    lines.push(format!("Status: {status}"));

    for p in &conflict.positions {
        lines.push(format!("Position: {p}"));
    }

    let claim_ids: Vec<&str> = conflict.claim_ids.iter().map(ClaimId::as_str).collect();
    lines.push(format!("Claims: {}", claim_ids.join(", ")));
    lines.push(format!("cite as: [@{id}]"));

    let text = lines.join("\n");
    PackBlock {
        id,
        kind: "conflict".to_owned(),
        text,
    }
}

pub(crate) fn format_doc_block(doc: &DocEntry) -> PackBlock {
    let id = doc.id.clone();
    let kind = doc.kind.clone();

    let title = if kind == "clause" && !doc.parent.is_empty() {
        collapse_whitespace(&format!("{} / {}", doc.parent, doc.title))
    } else {
        collapse_whitespace(&doc.title)
    };

    let as_of = if !doc.as_of.is_empty() {
        doc.as_of.clone()
    } else {
        "unknown".to_owned()
    };

    let mut lines = Vec::new();
    lines.push(format!("--- {kind} {id} ---"));
    lines.push(format!("Title: {title}"));
    lines.push(format!("As of: {as_of}"));

    if !doc.status.is_empty() {
        lines.push(format!("Status: {}", doc.status));
    }

    if !doc.snippet.is_empty() {
        lines.push(format!("Text: {}", doc.snippet));
    }

    if kind == "source" || kind == "question" {
        lines.push(format!("cite as: [@{id}]"));
    }

    let text = lines.join("\n");
    PackBlock { id, kind, text }
}

fn build_claim_selection_unit(
    claim: &ClaimRow,
    seen_block_ids: &mut BTreeSet<String>,
    conflicts_by_id: &BTreeMap<&str, &ConflictRow>,
    claims_by_id: &BTreeMap<&str, &ClaimRow>,
) -> Option<Vec<PackBlock>> {
    let cid = claim.claim_id.as_str();
    if seen_block_ids.contains(cid) {
        return None;
    }

    let mut blocks = Vec::new();
    let claim_block = format_claim_block(claim, None);
    seen_block_ids.insert(claim_block.id.clone());
    blocks.push(claim_block);

    if let Some(ref xid) = claim.conflict_id
        && let Some(conflict) = conflicts_by_id.get(xid.as_str())
    {
        if !seen_block_ids.contains(conflict.conflict_id.as_str()) {
            let conflict_block = format_conflict_block(conflict);
            seen_block_ids.insert(conflict_block.id.clone());
            blocks.push(conflict_block);
        }

        for opp_cid in &conflict.claim_ids {
            let opp_id_str = opp_cid.as_str();
            if !seen_block_ids.contains(opp_id_str)
                && let Some(opp_claim) = claims_by_id.get(opp_id_str)
            {
                let opp_block = format_claim_block(opp_claim, Some(xid));
                seen_block_ids.insert(opp_block.id.clone());
                blocks.push(opp_block);
            }
        }
    }

    Some(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_limit_is_eight() {
        assert_eq!(RELATED_LIMIT, 8);
    }

    #[test]
    fn whitespace_collapsing() {
        assert_eq!(collapse_whitespace("  hello \n \t world  "), "hello world");
        assert_eq!(collapse_whitespace(""), "");
    }

    #[test]
    fn question_names_id_respects_boundaries() {
        assert!(question_names_id(
            "Compare S-0003-v1 and S-0001-v1?",
            "S-0003-v1"
        ));
        assert!(question_names_id("what about cl-cmp-0002", "CL-CMP-0002"));
        assert!(!question_names_id("Compare S-0002-v10", "S-0002-v1"));
        assert!(!question_names_id("", "S-0001-v1"));
    }
}

//! Pure context pack builder (`page-context-packs`).
//!
//! **Responsibility.** Builds a [`ContextPack`] from a [`LabelledView`] under a character
//! budget, containing the own-page lead, cited claims with conflict closure, and top related
//! units from search.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

use std::collections::{BTreeMap, BTreeSet};

use crate::check::PageEntry;
use crate::check::markdown;
use crate::cite::KeyKind;
use crate::egress::{Dest, LabelledView};
use crate::ids::{ClaimId, ConflictId, PageId, VersionId};
use crate::labels::{LabelId, LabelSet};
use crate::schema::vocab::{AuthorityTier, Lifecycle, Verification};
use crate::schema::{ClaimRow, ConflictRow, GlossaryRow, QuestionRow, SourceRow};
use crate::search::index::DocEntry;
use crate::search::units::{
    build_corpus_units_with, extract_first_paragraph_text, kept_glossary, labels_of,
};
use crate::search::{Glossary, Index, Unit, query};
use crate::strings;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPack {
    blocks: Vec<PackBlock>,
    labels: LabelSet,
    left_out_budget: Vec<PackBlock>,
    preamble: String,
    left_out_line: String,
}

impl ContextPack {
    /// Returns the included blocks of the context pack in selection order.
    #[must_use]
    pub fn blocks(&self) -> &[PackBlock] {
        &self.blocks
    }

    /// Returns the union of data labels carried by the included blocks and preamble.
    #[must_use]
    pub const fn labels(&self) -> &LabelSet {
        &self.labels
    }

    /// Returns the blocks dropped due to character budget constraints, in selection order.
    #[must_use]
    pub fn left_out_budget(&self) -> &[PackBlock] {
        &self.left_out_budget
    }

    /// Serializes the context pack to plain text according to the frozen pack layout.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();

        if !self.preamble.is_empty() {
            for line in self.preamble.lines() {
                lines.push(line.to_owned());
            }
            lines.push(String::new());
        }

        let label_ids: Vec<&str> = self.labels.ids().iter().map(LabelId::as_str).collect();
        lines.push(format!("Labels: {}", label_ids.join(", ")));

        for block in &self.blocks {
            lines.push(String::new());
            for line in block.text.lines() {
                lines.push(line.to_owned());
            }
        }

        if !self.left_out_budget.is_empty() {
            lines.push(String::new());
            lines.push(strings::left_out_budget().to_owned());
            for b in &self.left_out_budget {
                lines.push(format!("- {} {}", b.kind, b.id));
            }
        }

        lines.push(String::new());
        lines.push(self.left_out_line.clone());

        format!("{}\n", lines.join("\n"))
    }
}

/// In-memory search index and labels built once from a view for generating context packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSource<'a> {
    view: &'a LabelledView,
    index: Index,
    labels: BTreeMap<String, LabelSet>,
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
        Self {
            view,
            index,
            labels,
        }
    }

    /// Builds a [`ContextPack`] for `page_id` under `budget` characters for `dest`.
    #[must_use]
    pub fn page_pack(&self, dest: &Dest, page_id: &PageId, budget: usize) -> ContextPack {
        let preamble = read_preamble(self.view);

        let Some(page) = self.view.pages().iter().find(|p| &p.id == page_id) else {
            let labels = if !preamble.is_empty() {
                LabelSet::public()
            } else {
                LabelSet::unlabelled()
            };
            return ContextPack {
                blocks: Vec::new(),
                labels,
                left_out_budget: Vec::new(),
                preamble,
                left_out_line: self.view.left_out_line(dest),
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
        let mut selection_units: Vec<Vec<PackBlock>> = Vec::new();

        // 1. The page (own-page block)
        let page_block = format_page_block(page);
        seen_block_ids.insert(page_block.id.clone());
        selection_units.push(vec![page_block]);

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
                selection_units.push(unit);
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
                    selection_units.push(unit);
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
                selection_units.push(vec![block]);
                related_count += 1;
                continue;
            }

            // Kind page, clause, source, question, term
            if let Some(doc) = self.index.docs.get(hit.unit) {
                let block = format_doc_block(doc);
                seen_block_ids.insert(block.id.clone());
                selection_units.push(vec![block]);
                related_count += 1;
            }
        }

        // Budget evaluation
        let mut running_total = preamble.chars().count();
        let mut kept_blocks: Vec<PackBlock> = Vec::new();
        let mut left_out_budget: Vec<PackBlock> = Vec::new();
        let mut left_out_seen = BTreeSet::new();
        let mut budget_exceeded = false;

        for unit in selection_units {
            if !budget_exceeded {
                let unit_size: usize = unit.iter().map(|b| b.text.chars().count() + 2).sum();
                if running_total + unit_size <= budget {
                    running_total += unit_size;
                    for block in unit {
                        kept_blocks.push(block);
                    }
                    continue;
                }
                budget_exceeded = true;
            }
            for block in unit {
                if left_out_seen.insert(block.id.clone()) {
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

        ContextPack {
            blocks: kept_blocks,
            labels,
            left_out_budget,
            preamble,
            left_out_line: self.view.left_out_line(dest),
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

fn view_units_and_glossary(view: &LabelledView) -> (Vec<Unit>, Glossary) {
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

fn read_preamble(view: &LabelledView) -> String {
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
    let id = page.id.as_str().to_owned();
    let title = collapse_whitespace(&page.meta.title);
    let as_of = page.meta.as_of.to_string();
    let status = page.meta.status.as_str();

    let mut lines = Vec::new();
    lines.push(format!("--- page {id} ---"));
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
        kind: "page".to_owned(),
        text,
    }
}

fn format_claim_block(claim: &ClaimRow, closure_conflict_id: Option<&ConflictId>) -> PackBlock {
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

fn format_conflict_block(conflict: &ConflictRow) -> PackBlock {
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

fn format_doc_block(doc: &DocEntry) -> PackBlock {
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
}

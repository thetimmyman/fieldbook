//! View-scoped unit export, deterministic view fingerprinting, and saved-pack resolution
//! (`handoff-context` step 4).
//!
//! **Responsibility.** [`export_units`] is the single pure function that determines which units
//! an assistant surface may see from a [`LabelledView`], along with their precomputed closure,
//! conflict dependencies, and exact pack block text. [`view_fingerprint`] hashes every unit's id,
//! text digest, and effective labels in id order. [`resolve_pack`] resolves a [`NamedPack`]
//! against the current view without ever reviving an unavailable pin.
//!
//! **Rules.** Pure: reads only [`LabelledView`] (never an unfiltered [`Model`]). Quarantined
//! source versions are never exported.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::cite::KeyKind;
use crate::egress::LabelledView;
use crate::hash::{Sha256Stream, sha256_hex};
use crate::ids::{ClaimId, ConflictId, VersionId};
use crate::labels::LabelSet;
use crate::schema::rows::RefId;
use crate::schema::vocab::{Lifecycle, Verification};
use crate::schema::{ClaimRow, ConflictRow, GlossaryRow, PageType, SourceRow};
use crate::search::Index;
use crate::search::units::labels_of;

use super::context::{ContextItem, NamedPack, Origin, Why};
use super::pack::{
    format_claim_block, format_conflict_block, format_doc_block, format_page_block_kind,
    view_units_and_glossary,
};

/// One assistant-visible unit exported from a [`LabelledView`] (`handoff-context` step 4).
///
/// `assistant-page` writes `assist-data.js` from [`export_units`], so every flag, block text,
/// closure edge, and conflict dependency that `context_pack` and `apply_edits` rely on lives here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRecord {
    /// Unit identifier (`cmp-system-review`, `cmp-system-review#the-rule`, `CL-CMP-0001`, ...).
    pub id: String,
    /// Unit kind (`page`, `clause`, `source`, `claim`, `question`, `conflict`, `term`).
    pub kind: String,
    /// Display title.
    pub title: String,
    /// Owning area code (`""` for source versions, which have no area column).
    pub area: String,
    /// `as_of` date as written (`""` when absent).
    pub as_of: String,
    /// Authority tier (`A`, `B`, `C`, `D`) for source versions; omitted for other kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// `true` when this source version, claim, page, or term is superseded (or withdrawn).
    pub superseded: bool,
    /// `true` when this claim carries `verification = contradicted`.
    pub contradicted: bool,
    /// Exact block text emitted for this unit in a context pack.
    pub block: String,
    /// Precomputed direct closure unit ids in deterministic order (all present in the view).
    pub closure: Vec<String>,
    /// Precomputed conflict row ids and opposing claim ids for a claim (all present in the view).
    pub conflicts: Vec<String>,
}

/// Internal bundle of exported units alongside their per-unit effective [`LabelSet`]s and
/// conflict-opponent mapping, built from a [`LabelledView`] in one pass.
pub(crate) struct ViewCatalog {
    pub units: BTreeMap<String, UnitRecord>,
    pub labels: BTreeMap<String, LabelSet>,
    pub index: Index,
    pub claims_by_id: BTreeMap<String, ClaimRow>,
    pub claim_conflicts: BTreeMap<String, Vec<(ConflictId, Vec<ClaimId>)>>,
}

impl ViewCatalog {
    #[must_use]
    pub(crate) fn from_view(view: &LabelledView) -> Self {
        let (raw_units, glossary) = view_units_and_glossary(view);
        let mut labels = BTreeMap::new();
        for u in &raw_units {
            labels.insert(u.id.clone(), labels_of(u).clone());
        }
        let index = Index::build(raw_units, &glossary);

        let pages_by_id: BTreeMap<&str, &crate::check::PageEntry> =
            view.pages().iter().map(|p| (p.id.as_str(), p)).collect();

        let sources_by_id: BTreeMap<String, &SourceRow> = view
            .sources()
            .iter()
            .filter(|s| s.value.lifecycle != Lifecycle::Quarantined)
            .map(|s| {
                (
                    format!("{}-v{}", s.value.source_id, s.value.version),
                    &s.value,
                )
            })
            .collect();

        let claims_by_id: BTreeMap<String, ClaimRow> = view
            .claims()
            .iter()
            .map(|c| (c.value.claim_id.as_str().to_owned(), c.value.clone()))
            .collect();

        let questions_by_id: BTreeMap<&str, &crate::schema::QuestionRow> = view
            .questions()
            .iter()
            .map(|q| (q.value.question_id.as_str(), &q.value))
            .collect();

        let conflicts_by_id: BTreeMap<&str, &ConflictRow> = view
            .conflicts()
            .iter()
            .map(|x| (x.value.conflict_id.as_str(), &x.value))
            .collect();

        let glossary_by_id: BTreeMap<&str, &GlossaryRow> = view
            .glossary()
            .iter()
            .map(|g| (g.value.term_id.as_str(), &g.value))
            .collect();

        // Map claim_id -> Vec<(ConflictId, Vec<opponent ClaimId>)> from both `claim.conflict_id`
        // and `conflict.claim_ids` (handles fixtures where `claims.csv` leaves `conflict_id` blank
        // while `conflicts.csv` lists the claim in `claim_ids`).
        let mut claim_conflict_ids: BTreeMap<String, BTreeSet<ConflictId>> = BTreeMap::new();
        for (cid, claim) in &claims_by_id {
            if let Some(xid) = &claim.conflict_id
                && conflicts_by_id.contains_key(xid.as_str())
            {
                claim_conflict_ids
                    .entry(cid.clone())
                    .or_default()
                    .insert(xid.clone());
            }
        }
        for loc in view.conflicts() {
            let xid = &loc.value.conflict_id;
            for cid in &loc.value.claim_ids {
                if claims_by_id.contains_key(cid.as_str()) {
                    claim_conflict_ids
                        .entry(cid.as_str().to_owned())
                        .or_default()
                        .insert(xid.clone());
                }
            }
        }

        let mut claim_conflicts: BTreeMap<String, Vec<(ConflictId, Vec<ClaimId>)>> =
            BTreeMap::new();
        for (cid, xids) in claim_conflict_ids {
            let mut entries = Vec::new();
            for xid in xids {
                if let Some(conflict) = conflicts_by_id.get(xid.as_str()) {
                    let opponents: Vec<ClaimId> = conflict
                        .claim_ids
                        .iter()
                        .filter(|opp| {
                            opp.as_str() != cid && claims_by_id.contains_key(opp.as_str())
                        })
                        .cloned()
                        .collect();
                    entries.push((xid, opponents));
                }
            }
            claim_conflicts.insert(cid, entries);
        }

        // Precompute linked glossary terms for each claim and source version.
        let mut terms_by_claim: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut terms_by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for loc in view.glossary() {
            let tid = loc.value.term_id.as_str().to_owned();
            for r in &loc.value.source_ref {
                match r {
                    RefId::Claim(cid) => {
                        let list = terms_by_claim.entry(cid.as_str().to_owned()).or_default();
                        if !list.contains(&tid) {
                            list.push(tid.clone());
                        }
                    }
                    RefId::SourceVersion(vid) => {
                        let list = terms_by_source.entry(vid.as_str().to_owned()).or_default();
                        if !list.contains(&tid) {
                            list.push(tid.clone());
                        }
                    }
                }
            }
        }

        let mut units = BTreeMap::new();
        for doc in &index.docs {
            if doc.kind == "record" || doc.kind == "text" {
                continue;
            }
            let id = doc.id.clone();

            let (
                kind,
                title,
                area,
                as_of,
                tier,
                superseded,
                contradicted,
                block,
                closure,
                conflicts,
            ) = match doc.kind.as_str() {
                "page" | "clause" => {
                    let base_id = id.split_once('#').map_or(id.as_str(), |(b, _)| b);
                    let Some(page) = pages_by_id.get(base_id) else {
                        continue;
                    };
                    let is_section = id.contains('#');
                    let kind = if page.meta.r#type == PageType::ClauseCard {
                        "clause".to_owned()
                    } else {
                        "page".to_owned()
                    };
                    let block = if is_section {
                        format_doc_block(doc).text
                    } else {
                        format_page_block_kind(page, &kind).text
                    };
                    let title = if is_section && !doc.parent.is_empty() {
                        format!("{} / {}", doc.parent, doc.title)
                    } else {
                        doc.title.clone()
                    };
                    let mut closure = Vec::new();
                    let mut seen = BTreeSet::new();
                    for cite in view.citations(&page.id) {
                        match cite.key_kind {
                            KeyKind::Claim => {
                                if claims_by_id.contains_key(cite.key.as_str())
                                    && seen.insert(cite.key.clone())
                                {
                                    closure.push(cite.key.clone());
                                }
                            }
                            KeyKind::Version => {
                                if sources_by_id.contains_key(cite.key.as_str())
                                    && seen.insert(cite.key.clone())
                                {
                                    closure.push(cite.key.clone());
                                }
                            }
                            KeyKind::Question | KeyKind::Conflict | KeyKind::Note => {}
                        }
                    }
                    for term_id in &page.meta.terms {
                        let tid = term_id.as_str();
                        if glossary_by_id.contains_key(tid) && seen.insert(tid.to_owned()) {
                            closure.push(tid.to_owned());
                        }
                    }
                    (
                        kind,
                        title,
                        page.meta.area.as_str().to_owned(),
                        page.meta.as_of.to_string(),
                        None,
                        !page.meta.superseded_by.is_empty(),
                        false,
                        block,
                        closure,
                        Vec::new(),
                    )
                }
                "source" => {
                    let Some(source) = sources_by_id.get(&id) else {
                        continue;
                    };
                    let is_superseded = VersionId::parse(&id)
                        .ok()
                        .and_then(|vid| view.index().source_versions.get(&vid))
                        .is_some_and(|entry| entry.superseded)
                        || source.lifecycle == Lifecycle::Withdrawn;
                    let closure = terms_by_source.get(&id).cloned().unwrap_or_default();
                    (
                        "source".to_owned(),
                        source.title.clone(),
                        String::new(),
                        source.as_of.to_string(),
                        source.authority_tier.map(|t| t.as_str().to_owned()),
                        is_superseded,
                        false,
                        format_doc_block(doc).text,
                        closure,
                        Vec::new(),
                    )
                }
                "claim" => {
                    let Some(claim) = claims_by_id.get(&id) else {
                        continue;
                    };
                    let mut closure = Vec::new();
                    let mut seen = BTreeSet::new();
                    if let (Some(sid), Some(ver)) = (&claim.source_id, claim.version) {
                        let vid = format!("{sid}-v{ver}");
                        if sources_by_id.contains_key(&vid) && seen.insert(vid.clone()) {
                            closure.push(vid);
                        }
                    }
                    if let Some(terms) = terms_by_claim.get(&id) {
                        for tid in terms {
                            if seen.insert(tid.clone()) {
                                closure.push(tid.clone());
                            }
                        }
                    }

                    let mut conflicts = Vec::new();
                    let mut c_seen = BTreeSet::new();
                    if let Some(entries) = claim_conflicts.get(&id) {
                        for (xid, opponents) in entries {
                            if c_seen.insert(xid.as_str().to_owned()) {
                                conflicts.push(xid.as_str().to_owned());
                            }
                            for opp in opponents {
                                if c_seen.insert(opp.as_str().to_owned()) {
                                    conflicts.push(opp.as_str().to_owned());
                                }
                            }
                        }
                    }

                    let superseded = !claim.superseded_by.is_empty()
                        || claim.adoption.as_str() == crate::strings::CITE_STATUSES[3];
                    let contradicted = claim.verification == Verification::Contradicted;
                    let as_of = claim
                        .as_of
                        .as_ref()
                        .map_or_else(String::new, ToString::to_string);
                    (
                        "claim".to_owned(),
                        claim.statement.clone(),
                        claim.area.as_str().to_owned(),
                        as_of,
                        None,
                        superseded,
                        contradicted,
                        format_claim_block(claim, None).text,
                        closure,
                        conflicts,
                    )
                }
                "question" => {
                    let Some(q) = questions_by_id.get(id.as_str()) else {
                        continue;
                    };
                    (
                        "question".to_owned(),
                        q.question.clone(),
                        q.area.as_str().to_owned(),
                        String::new(),
                        None,
                        false,
                        false,
                        format_doc_block(doc).text,
                        Vec::new(),
                        Vec::new(),
                    )
                }
                "conflict" => {
                    let Some(x) = conflicts_by_id.get(id.as_str()) else {
                        continue;
                    };
                    let as_of = x
                        .resolved_on
                        .as_ref()
                        .map_or_else(String::new, ToString::to_string);
                    (
                        "conflict".to_owned(),
                        x.topic.clone(),
                        x.area.as_str().to_owned(),
                        as_of,
                        None,
                        false,
                        false,
                        format_conflict_block(x).text,
                        Vec::new(),
                        Vec::new(),
                    )
                }
                "term" => {
                    let Some(g) = glossary_by_id.get(id.as_str()) else {
                        continue;
                    };
                    (
                        "term".to_owned(),
                        g.label.clone(),
                        g.owner_area.as_str().to_owned(),
                        String::new(),
                        None,
                        g.superseded_by.is_some(),
                        false,
                        format_doc_block(doc).text,
                        Vec::new(),
                        Vec::new(),
                    )
                }
                _ => continue,
            };

            units.insert(
                id.clone(),
                UnitRecord {
                    id,
                    kind,
                    title,
                    area,
                    as_of,
                    tier,
                    superseded,
                    contradicted,
                    block,
                    closure,
                    conflicts,
                },
            );
        }

        Self {
            units,
            labels,
            index,
            claims_by_id,
            claim_conflicts,
        }
    }

    #[must_use]
    pub(crate) fn fingerprint(&self) -> String {
        let mut hasher = Sha256Stream::new();
        for (id, record) in &self.units {
            let text_sha = sha256_hex(record.block.as_bytes());
            let label_cell = self
                .labels
                .get(id)
                .map_or_else(|| LabelSet::unlabelled().to_cell(), LabelSet::to_cell);
            let line = format!("{id}\t{text_sha}\t{label_cell}\n");
            hasher.update(line.as_bytes());
        }
        hasher.finish()
    }
}

/// Returns every assistant-visible unit in `view`, sorted by `id` (`handoff-context` step 4).
///
/// This is the single source of truth for what an assistant may see: records outside `view` and
/// quarantined source versions are absent.
#[must_use]
pub fn export_units(view: &LabelledView) -> Vec<UnitRecord> {
    ViewCatalog::from_view(view).units.into_values().collect()
}

/// Computes the deterministic SHA-256 hex fingerprint over every unit in `view` in id order:
/// `<id>\t<SHA-256 of its block text>\t<effective labels joined by |>\n`.
#[must_use]
pub fn view_fingerprint(view: &LabelledView) -> String {
    ViewCatalog::from_view(view).fingerprint()
}

/// Resolves a saved [`NamedPack`] against `view` (`handoff-context` step 4).
///
/// Every entry enters with `origin = Origin::Pinned` and `why = Why::Named`.
/// - An id absent from `view` is `stale = true, included = false`, regardless of whether the
///   view fingerprint matches or changed (a fingerprint change never restores permission).
/// - When `named.view_fingerprint` differs from `view_fingerprint(view)`, every available item
///   is marked `stale = true` while remaining `included = true`.
#[must_use]
pub fn resolve_pack(view: &LabelledView, named: &NamedPack) -> Vec<ContextItem> {
    let catalog = ViewCatalog::from_view(view);
    let current_fp = catalog.fingerprint();
    let fp_changed = named.view_fingerprint != current_fp;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for id in &named.ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let available = catalog.units.contains_key(id);
        out.push(ContextItem {
            unit: id.clone(),
            origin: Origin::Pinned,
            why: Why::Named,
            via: None,
            included: available,
            stale: !available || fp_changed,
        });
    }
    out
}

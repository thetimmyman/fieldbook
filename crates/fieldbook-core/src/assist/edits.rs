//! Owner and assistant context-pack edits (`apply_edits`, `Edits`, `handoff-context` steps 3, 12).
//!
//! **Responsibility.** Applies pinned, removed, added, and suggested unit id lists on top of a
//! [`ContextPack`] against the current [`LabelledView`], preserving mandatory pins and their
//! required closure, keeping unavailable pins explicit (`stale = true, included = false`) so they
//! block send, trimming only optional dependency groups in reverse rank order, and rebuilding
//! exact text, labels, and [`RequestBudget`](super::context::RequestBudget).
//!
//! **Rules.** Pure: no I/O, no clock, no environment. A unit absent from `view` can never be
//! brought in by any edit.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::egress::LabelledView;
use crate::ids::ConflictId;

use super::context::{Budget, ContextItem, Origin, Why};
use super::pack::{ContextPack, finalize_pack};
use super::view::ViewCatalog;

/// Caller-supplied edits applied on top of a [`ContextPack`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edits {
    /// Unit ids to pin at the front (`origin = Pinned`, mandatory).
    #[serde(default)]
    pub pinned: Vec<String>,
    /// Unit ids to exclude (`included = false`, kept listed).
    #[serde(default)]
    pub removed: Vec<String>,
    /// Unit ids added by the owner (`origin = Added`, `why = Named`).
    #[serde(default)]
    pub added: Vec<String>,
    /// Unit ids suggested by a model or helper (`origin = Suggested`, `why = Named`).
    #[serde(default)]
    pub suggested: Vec<String>,
}

/// Applies `edits` to `pack` under `view` and `budget`, returning a rebuilt [`ContextPack`].
///
/// - `pinned` moves items first with `origin = Pinned`; an id absent from `view` stays listed as
///   `stale = true, included = false` and makes `request_budget.sendable = false` until removed.
/// - `added` and `suggested` append items with that origin and `why = Named`, pulling in conflict
///   rows and opponents as `Closure` items for any added/suggested claim; an id absent from `view`
///   is listed as `stale = true, included = false`.
/// - `removed` sets `included = false` while keeping the item listed.
/// - Mandatory content (rules, question, history, pins, and required pin closure) is never cut;
///   only optional dependency groups are trimmed in reverse rank order.
#[must_use]
pub fn apply_edits(
    view: &LabelledView,
    pack: &ContextPack,
    edits: &Edits,
    budget: impl Into<Budget>,
) -> ContextPack {
    let catalog = ViewCatalog::from_view(view);
    let budget = budget.into();
    let removed_set: BTreeSet<&str> = edits.removed.iter().map(String::as_str).collect();
    let mut conflict_opponents: BTreeMap<String, ConflictId> = BTreeMap::new();

    // Index existing items from `pack` preserving their order.
    let mut existing: Vec<ContextItem> = pack.items().to_vec();
    // Re-validate availability against the current view and mark any unavailable item stale.
    for item in &mut existing {
        if !catalog.units.contains_key(&item.unit) {
            item.included = false;
            item.stale = true;
        }
    }

    // Track items that should start as included candidates before budget trimming.
    let mut items: Vec<ContextItem> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    // Helper: append conflict dependencies (conflict row + opposing claims) for a claim id.
    let push_conflict_deps =
        |claim_id: &str,
         origin: Origin,
         items: &mut Vec<ContextItem>,
         seen: &mut BTreeSet<String>,
         conflict_opponents: &mut BTreeMap<String, ConflictId>| {
            if let Some(entries) = catalog.claim_conflicts.get(claim_id) {
                for (xid, opponents) in entries {
                    let xid_str = xid.as_str().to_owned();
                    if catalog.units.contains_key(&xid_str) && seen.insert(xid_str.clone()) {
                        items.push(ContextItem {
                            unit: xid_str,
                            origin,
                            why: Why::Closure,
                            via: Some(claim_id.to_owned()),
                            included: true,
                            stale: false,
                        });
                    }
                    for opp in opponents {
                        let opp_str = opp.as_str().to_owned();
                        conflict_opponents
                            .entry(opp_str.clone())
                            .or_insert_with(|| xid.clone());
                        if catalog.units.contains_key(&opp_str) && seen.insert(opp_str.clone()) {
                            items.push(ContextItem {
                                unit: opp_str,
                                origin,
                                why: Why::Closure,
                                via: Some(claim_id.to_owned()),
                                included: true,
                                stale: false,
                            });
                        }
                    }
                }
            }
        };

    // Helper: append full required closure for a pinned item (conflict deps + AD-18 closure).
    let push_pin_closure =
        |pin_id: &str,
         items: &mut Vec<ContextItem>,
         seen: &mut BTreeSet<String>,
         conflict_opponents: &mut BTreeMap<String, ConflictId>| {
            push_conflict_deps(pin_id, Origin::Auto, items, seen, conflict_opponents);
            if let Some(unit) = catalog.units.get(pin_id) {
                for dep_id in &unit.closure {
                    if !catalog.units.contains_key(dep_id) {
                        continue;
                    }
                    if seen.insert(dep_id.clone()) {
                        items.push(ContextItem {
                            unit: dep_id.clone(),
                            origin: Origin::Auto,
                            why: Why::Closure,
                            via: Some(pin_id.to_owned()),
                            included: true,
                            stale: false,
                        });
                    }
                    if catalog.units.get(dep_id).is_some_and(|u| u.kind == "claim") {
                        push_conflict_deps(dep_id, Origin::Auto, items, seen, conflict_opponents);
                        if let Some(claim_unit) = catalog.units.get(dep_id) {
                            for sub_dep in &claim_unit.closure {
                                if catalog.units.contains_key(sub_dep)
                                    && seen.insert(sub_dep.clone())
                                {
                                    items.push(ContextItem {
                                        unit: sub_dep.clone(),
                                        origin: Origin::Auto,
                                        why: Why::Closure,
                                        via: Some(dep_id.clone()),
                                        included: true,
                                        stale: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        };

    // 1. Pinned items first: explicit `edits.pinned` in order, then any previously-pinned items.
    let mut pin_ids: Vec<String> = Vec::new();
    for id in &edits.pinned {
        if !pin_ids.contains(id) {
            pin_ids.push(id.clone());
        }
    }
    for item in &existing {
        if item.origin == Origin::Pinned && !pin_ids.contains(&item.unit) {
            pin_ids.push(item.unit.clone());
        }
    }

    let existing_by_id: BTreeMap<String, &ContextItem> = existing
        .iter()
        .map(|item| (item.unit.clone(), item))
        .collect();

    for pin_id in &pin_ids {
        if !seen.insert(pin_id.clone()) {
            continue;
        }
        let available = catalog.units.contains_key(pin_id);
        let prev = existing_by_id.get(pin_id).copied();
        let why = prev.map_or(Why::Named, |p| p.why);
        let via = prev.and_then(|p| p.via.clone());
        let stale = !available || prev.is_some_and(|p| p.stale);
        items.push(ContextItem {
            unit: pin_id.clone(),
            origin: Origin::Pinned,
            why,
            via,
            included: available,
            stale,
        });
        if available {
            push_pin_closure(pin_id, &mut items, &mut seen, &mut conflict_opponents);
        }
    }

    // 2. Remaining existing items in their original order.
    // Re-enable items that were only dropped for budget earlier so the new budget is evaluated fresh.
    let budget_dropped: BTreeSet<&str> = pack
        .left_out_budget()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    for item in &existing {
        if !seen.insert(item.unit.clone()) {
            continue;
        }
        let mut next = item.clone();
        if !next.included && !next.stale && budget_dropped.contains(next.unit.as_str()) {
            next.included = true;
        }
        // Preserve opponent styling only for claims brought in as conflict closure of an earlier claim.
        if next.why == Why::Closure
            && let Some(ref via_id) = next.via
            && let Some(entries) = catalog.claim_conflicts.get(via_id)
        {
            for (xid, opponents) in entries {
                if opponents.iter().any(|opp| opp.as_str() == next.unit) {
                    conflict_opponents
                        .entry(next.unit.clone())
                        .or_insert_with(|| xid.clone());
                }
            }
        }
        items.push(next);
    }

    // 3. Added and suggested items.
    for (list, origin) in [
        (&edits.added, Origin::Added),
        (&edits.suggested, Origin::Suggested),
    ] {
        for id in list {
            let available = catalog.units.contains_key(id);
            if let Some(pos) = items.iter().position(|it| &it.unit == id) {
                if available {
                    items[pos].included = true;
                    if items[pos].origin != Origin::Pinned {
                        items[pos].origin = origin;
                    }
                    push_conflict_deps(id, origin, &mut items, &mut seen, &mut conflict_opponents);
                }
                continue;
            }
            if !seen.insert(id.clone()) {
                continue;
            }
            items.push(ContextItem {
                unit: id.clone(),
                origin,
                why: Why::Named,
                via: None,
                included: available,
                stale: !available,
            });
            if available {
                push_conflict_deps(id, origin, &mut items, &mut seen, &mut conflict_opponents);
            }
        }
    }

    // Ensure any already-present conflict row/opponent for an active included claim remains
    // included (unless explicitly removed) and opponents after the claim carry conflict styling.
    let active_claims: Vec<(usize, String)> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            it.included
                && !removed_set.contains(it.unit.as_str())
                && catalog
                    .units
                    .get(&it.unit)
                    .is_some_and(|u| u.kind == "claim")
        })
        .map(|(idx, it)| (idx, it.unit.clone()))
        .collect();
    for (claim_idx, cid) in &active_claims {
        if let Some(entries) = catalog.claim_conflicts.get(cid) {
            for (xid, opponents) in entries {
                if let Some(item) = items.iter_mut().find(|it| it.unit == xid.as_str())
                    && !removed_set.contains(item.unit.as_str())
                {
                    item.included = true;
                }
                for opp in opponents {
                    if let Some((opp_idx, item)) = items
                        .iter_mut()
                        .enumerate()
                        .find(|(_, it)| it.unit == opp.as_str())
                    {
                        if opp_idx > *claim_idx {
                            conflict_opponents
                                .entry(opp.as_str().to_owned())
                                .or_insert_with(|| xid.clone());
                        }
                        if !removed_set.contains(item.unit.as_str()) {
                            item.included = true;
                        }
                    }
                }
            }
        }
    }

    // 4. Apply `removed`: mark `included = false`, keep listed, and track explicitly excluded items
    // so they are never listed under `Left out (budget)`.
    let mut non_budget_excluded: BTreeSet<String> = BTreeSet::new();
    for item in &mut items {
        if removed_set.contains(item.unit.as_str()) {
            item.included = false;
            non_budget_excluded.insert(item.unit.clone());
        } else if !item.included {
            non_budget_excluded.insert(item.unit.clone());
        }
    }

    // Determine mandatory items: every non-removed Pinned item and its required closure/conflicts.
    let mut mandatory: BTreeSet<String> = BTreeSet::new();
    let mut has_unavailable_pin = false;
    let active_pins: Vec<String> = items
        .iter()
        .filter(|it| it.origin == Origin::Pinned && !removed_set.contains(it.unit.as_str()))
        .map(|it| it.unit.clone())
        .collect();

    for pin_id in &active_pins {
        if !catalog.units.contains_key(pin_id) {
            has_unavailable_pin = true;
            continue;
        }
        mandatory.insert(pin_id.clone());
        // Required closure & conflict deps of this pin.
        let mut stack = vec![pin_id.clone()];
        while let Some(cur) = stack.pop() {
            if let Some(unit) = catalog.units.get(&cur) {
                for dep in unit.conflicts.iter().chain(unit.closure.iter()) {
                    if !removed_set.contains(dep.as_str())
                        && catalog.units.contains_key(dep)
                        && mandatory.insert(dep.clone())
                    {
                        // Follow one more hop for claims (their source, terms, conflicts).
                        if catalog.units.get(dep).is_some_and(|u| u.kind == "claim") {
                            stack.push(dep.clone());
                        }
                    }
                }
            }
        }
    }

    // Ensure every mandatory item present in `items` is marked included before budget evaluation.
    for item in &mut items {
        if mandatory.contains(&item.unit) {
            item.included = true;
            non_budget_excluded.remove(&item.unit);
        }
    }

    finalize_pack(
        view,
        &catalog,
        pack.question().to_owned(),
        pack.scope().clone(),
        pack.question_labels_ref().cloned(),
        pack.banners_ref().clone(),
        pack.left_out_line().to_owned(),
        items,
        &mandatory,
        &non_budget_excluded,
        &conflict_opponents,
        budget,
        has_unavailable_pin,
    )
}

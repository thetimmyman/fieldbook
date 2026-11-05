//! Provenance dependency graph (`ItemRef`, `LabelNode`, `LabelGraph`) and recursive
//! effective-label evaluation (`effective_item`)
//! (`docs/decisions/0004-company-boundary.md#2-labels`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::ledger::{self, Ledger};
use super::set::LabelSet;
use super::store::StoreId;

/// Store-qualified item identifier (`store` + `id`, never a display title).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ItemRef {
    /// Store holding the item.
    pub store: StoreId,
    /// Stable item identifier within `store`.
    pub id: String,
}

impl ItemRef {
    /// Creates a new [`ItemRef`].
    #[must_use]
    pub fn new(store: StoreId, id: impl Into<String>) -> Self {
        Self {
            store,
            id: id.into(),
        }
    }
}

/// One node in the provenance dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelNode {
    /// The item's own initial `handling` label set.
    pub own: LabelSet,
    /// Direct persisted input references on which this item rests.
    pub inputs: Vec<ItemRef>,
}

impl LabelNode {
    /// Creates a new [`LabelNode`].
    #[must_use]
    pub fn new(own: LabelSet, inputs: Vec<ItemRef>) -> Self {
        Self { own, inputs }
    }
}

/// Read-only view of item nodes and store ledgers for one coherent read generation.
pub trait LabelGraph {
    /// Looks up the [`LabelNode`] for `item` by its store-qualified id.
    fn node(&self, item: &ItemRef) -> Option<LabelNode>;

    /// Returns the [`Ledger`] for `store`.
    fn ledger(&self, store: StoreId) -> &Ledger;
}

/// Computes the effective [`LabelSet`] of `item` in `graph` by unioning the item's
/// ledger-effective own labels with the recursive effective labels of all its inputs.
///
/// - Missing nodes and dependency cycles add `unlabelled`.
/// - Ledger classification on an item resolves only that item's own `unlabelled` cell and never
///   clears `unlabelled` or `marked` inherited from an input.
/// - Memoization is scoped strictly to this single evaluation (one read generation).
#[must_use]
pub fn effective_item(graph: &dyn LabelGraph, item: &ItemRef) -> LabelSet {
    let mut visiting = BTreeSet::new();
    let mut memo = BTreeMap::new();
    eval_node(graph, item, &mut visiting, &mut memo).0
}

fn eval_node(
    graph: &dyn LabelGraph,
    item: &ItemRef,
    visiting: &mut BTreeSet<ItemRef>,
    memo: &mut BTreeMap<ItemRef, LabelSet>,
) -> (LabelSet, bool) {
    if let Some(cached) = memo.get(item) {
        return (cached.clone(), false);
    }
    if !visiting.insert(item.clone()) {
        return (LabelSet::unlabelled(), true);
    }

    let Some(node) = graph.node(item) else {
        visiting.remove(item);
        return (LabelSet::unlabelled(), false);
    };

    let mut acc = ledger::effective(&node.own, &item.id, graph.ledger(item.store));
    let mut saw_cycle = false;

    for input in &node.inputs {
        let (input_labels, input_cycle) = eval_node(graph, input, visiting, memo);
        acc = acc.union(&input_labels);
        if input_cycle {
            saw_cycle = true;
        }
    }

    visiting.remove(item);
    if !saw_cycle {
        memo.insert(item.clone(), acc.clone());
    }
    (acc, saw_cycle)
}

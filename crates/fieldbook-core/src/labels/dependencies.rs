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
/// - Strongly connected components are resolved once in linear time (`O(V + E)`), preserving all
///   restrictive labels across cyclic and shared-dependency subgraphs.
/// - Ledger classification on an item resolves only that item's own `unlabelled` cell and never
///   clears `unlabelled` or `marked` inherited from an input.
/// - Memoization is scoped strictly to this single evaluation (one read generation).
#[must_use]
pub fn effective_item(graph: &dyn LabelGraph, item: &ItemRef) -> LabelSet {
    let mut state = TarjanState::new(graph);
    state.strongconnect(item);
    state.memo.remove(item).unwrap_or_else(LabelSet::unlabelled)
}

struct TarjanState<'a> {
    graph: &'a dyn LabelGraph,
    next_index: usize,
    indices: BTreeMap<ItemRef, usize>,
    lowlink: BTreeMap<ItemRef, usize>,
    stack: Vec<ItemRef>,
    on_stack: BTreeSet<ItemRef>,
    nodes: BTreeMap<ItemRef, Option<LabelNode>>,
    memo: BTreeMap<ItemRef, LabelSet>,
}

impl<'a> TarjanState<'a> {
    fn new(graph: &'a dyn LabelGraph) -> Self {
        Self {
            graph,
            next_index: 0,
            indices: BTreeMap::new(),
            lowlink: BTreeMap::new(),
            stack: Vec::new(),
            on_stack: BTreeSet::new(),
            nodes: BTreeMap::new(),
            memo: BTreeMap::new(),
        }
    }

    fn strongconnect(&mut self, v: &ItemRef) {
        let v_idx = self.next_index;
        self.next_index += 1;
        self.indices.insert(v.clone(), v_idx);
        self.lowlink.insert(v.clone(), v_idx);
        self.stack.push(v.clone());
        self.on_stack.insert(v.clone());

        let node_opt = self.graph.node(v);
        let inputs = node_opt
            .as_ref()
            .map(|n| n.inputs.clone())
            .unwrap_or_default();
        self.nodes.insert(v.clone(), node_opt);

        for w in &inputs {
            if !self.indices.contains_key(w) {
                self.strongconnect(w);
                if let (Some(w_low), Some(v_low)) =
                    (self.lowlink.get(w).copied(), self.lowlink.get_mut(v))
                    && w_low < *v_low
                {
                    *v_low = w_low;
                }
            } else if self.on_stack.contains(w)
                && let (Some(w_index), Some(v_low)) =
                    (self.indices.get(w).copied(), self.lowlink.get_mut(v))
                && w_index < *v_low
            {
                *v_low = w_index;
            }
        }

        if self.lowlink.get(v).copied() == Some(v_idx) {
            let mut scc = Vec::new();
            while let Some(w) = self.stack.pop() {
                self.on_stack.remove(&w);
                let done = &w == v;
                scc.push(w);
                if done {
                    break;
                }
            }

            let is_cyclic = scc.len() > 1
                || scc.first().is_some_and(|only| {
                    self.nodes
                        .get(only)
                        .and_then(Option::as_ref)
                        .is_some_and(|n| n.inputs.contains(only))
                });

            let mut acc: Option<LabelSet> = if is_cyclic {
                Some(LabelSet::unlabelled())
            } else {
                None
            };

            for member in &scc {
                let member_labels = match self.nodes.get(member).and_then(Option::as_ref) {
                    Some(node) => {
                        let mut own = ledger::effective(
                            &node.own,
                            &member.id,
                            self.graph.ledger(member.store),
                        );
                        for inp in &node.inputs {
                            if let Some(inp_labels) = self.memo.get(inp) {
                                own = own.union(inp_labels);
                            }
                        }
                        own
                    }
                    None => LabelSet::unlabelled(),
                };
                acc = Some(match acc {
                    Some(existing) => existing.union(&member_labels),
                    None => member_labels,
                });
            }

            let final_labels = acc.unwrap_or_else(LabelSet::unlabelled);
            for member in scc {
                self.memo.insert(member, final_labels.clone());
            }
        }
    }
}

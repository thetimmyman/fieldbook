//! Provenance dependency graph tests for `labels::effective_item` (`core-labels` step 11).

use std::collections::BTreeMap;

use fieldbook_core::labels::{
    ItemRef, LabelGraph, LabelId, LabelNode, LabelSet, Ledger, LedgerOrigin, StoreId,
    effective_item,
};

fn id(s: &str) -> LabelId {
    LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
}

#[derive(Default)]
struct TestGraph {
    nodes: BTreeMap<ItemRef, LabelNode>,
    ledgers: BTreeMap<StoreId, Ledger>,
    empty_ledger: Ledger,
}

impl TestGraph {
    fn insert_node(&mut self, item: ItemRef, own: LabelSet, inputs: Vec<ItemRef>) {
        self.nodes.insert(item, LabelNode::new(own, inputs));
    }

    fn set_ledger(&mut self, store: StoreId, ledger: Ledger) {
        self.ledgers.insert(store, ledger);
    }
}

impl LabelGraph for TestGraph {
    fn node(&self, item: &ItemRef) -> Option<LabelNode> {
        self.nodes.get(item).cloned()
    }

    fn ledger(&self, store: StoreId) -> &Ledger {
        self.ledgers.get(&store).unwrap_or(&self.empty_ledger)
    }
}

#[test]
fn source_reclassification_restricts_every_derivative() {
    let np = id("np-0007");
    let ctl = id("ctl-0002");

    let source = ItemRef::new(StoreId::Overlay, "SO-000042-v1");
    let note = ItemRef::new(StoreId::Working, "N-0001");
    let pack = ItemRef::new(StoreId::State, "H-0001");
    let call = ItemRef::new(StoreId::State, "call-0001");
    let calc = ItemRef::new(StoreId::Overlay, "CM-000012-v1");

    let mut graph = TestGraph::default();
    graph.insert_node(source.clone(), LabelSet::from_ids([np.clone()]), vec![]);
    graph.insert_node(
        note.clone(),
        LabelSet::from_ids([np.clone()]),
        vec![source.clone()],
    );
    graph.insert_node(
        pack.clone(),
        LabelSet::from_ids([np.clone()]),
        vec![source.clone(), note.clone()],
    );
    graph.insert_node(
        call.clone(),
        LabelSet::from_ids([np.clone()]),
        vec![pack.clone()],
    );
    graph.insert_node(
        calc.clone(),
        LabelSet::from_ids([np.clone()]),
        vec![source.clone()],
    );

    // Before ledger reclassification, every derivative has `[np-0007]`.
    for target in [&source, &note, &pack, &call, &calc] {
        assert_eq!(
            effective_item(&graph, target),
            LabelSet::from_ids([np.clone()])
        );
    }

    // Reclassify the source in the overlay ledger by adding `ctl-0002`.
    let plan = Ledger::empty().plan_add(
        ["SO-000042-v1"],
        &LabelSet::from_ids([ctl.clone()]),
        LedgerOrigin::Person,
    );
    graph.set_ledger(StoreId::Overlay, Ledger::parse(&plan.text).unwrap());

    // On the next read, the note, pack, call record and calculation all carry `ctl-0002` as well.
    let expected = LabelSet::from_ids([ctl, np]);
    for target in [&source, &note, &pack, &call, &calc] {
        assert_eq!(effective_item(&graph, target), expected);
    }
}

#[test]
fn missing_input_is_unresolved() {
    let np = id("np-0007");
    let missing_source = ItemRef::new(StoreId::Overlay, "SO-999999-v1");
    let note = ItemRef::new(StoreId::Working, "N-0001");

    let mut graph = TestGraph::default();
    graph.insert_node(
        note.clone(),
        LabelSet::from_ids([np.clone()]),
        vec![missing_source.clone()],
    );

    assert_eq!(
        effective_item(&graph, &missing_source),
        LabelSet::unlabelled()
    );

    let eff = effective_item(&graph, &note);
    assert!(eff.is_unresolved());
    assert!(eff.contains(&fieldbook_core::labels::UNLABELLED));
    assert!(eff.contains(&np));
}

#[test]
fn cycle_is_unresolved() {
    let np = id("np-0007");
    let a = ItemRef::new(StoreId::Overlay, "CM-000001-v1");
    let b = ItemRef::new(StoreId::Overlay, "CM-000002-v1");

    let mut graph = TestGraph::default();
    graph.insert_node(a.clone(), LabelSet::from_ids([np.clone()]), vec![b.clone()]);
    graph.insert_node(b.clone(), LabelSet::from_ids([np.clone()]), vec![a.clone()]);

    let eff_a = effective_item(&graph, &a);
    assert!(eff_a.is_unresolved());
    assert!(eff_a.contains(&fieldbook_core::labels::UNLABELLED));
    assert!(eff_a.contains(&np));

    let eff_b = effective_item(&graph, &b);
    assert!(eff_b.is_unresolved());
    assert!(eff_b.contains(&fieldbook_core::labels::UNLABELLED));
}

#[test]
fn classification_does_not_clear_an_inputs_unresolved_label() {
    let np = id("np-0007");
    let unresolved_source = ItemRef::new(StoreId::Overlay, "SO-000042-v1");
    let derived_note = ItemRef::new(StoreId::Working, "N-0001");

    let mut graph = TestGraph::default();
    graph.insert_node(unresolved_source.clone(), LabelSet::unlabelled(), vec![]);
    graph.insert_node(
        derived_note.clone(),
        LabelSet::unlabelled(),
        vec![unresolved_source],
    );

    // Explicitly classify `derived_note` in the `working` ledger.
    let plan = Ledger::empty().plan_add(
        ["N-0001"],
        &LabelSet::from_ids([np.clone()]),
        LedgerOrigin::Person,
    );
    graph.set_ledger(StoreId::Working, Ledger::parse(&plan.text).unwrap());

    // The note's own `unlabelled` was resolved to `np-0007`, but its input `SO-000042-v1` is
    // still `unlabelled`, so `effective_item` keeps `unlabelled`.
    let eff = effective_item(&graph, &derived_note);
    assert!(eff.is_unresolved());
    assert!(eff.contains(&fieldbook_core::labels::UNLABELLED));
    assert!(eff.contains(&np));
}

#[test]
fn a_public_graph_remains_public() {
    let src = ItemRef::new(StoreId::Corpus, "S-0001-v1");
    let page = ItemRef::new(StoreId::Corpus, "fnd-example");
    let pack = ItemRef::new(StoreId::State, "H-0001");

    let mut graph = TestGraph::default();
    graph.insert_node(src.clone(), LabelSet::public(), vec![]);
    graph.insert_node(page.clone(), LabelSet::public(), vec![src.clone()]);
    graph.insert_node(pack.clone(), LabelSet::public(), vec![src, page]);

    let eff = effective_item(&graph, &pack);
    assert_eq!(eff, LabelSet::public());
    assert!(eff.is_public());
    assert!(!eff.is_unresolved());
}

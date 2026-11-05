//! Cross-module integration tests for `fieldbook_core::labels` (`core-labels` step 9).

use fieldbook_core::check::Code;
use fieldbook_core::labels::{
    LabelAllow, LabelContext, LabelId, LabelSet, Ledger, LedgerOrigin, MarkingRules, PublicAct,
    RefuseReason, StoreId, StoreRule, admit, effective, for_text,
};
use fieldbook_core::scan::marking as banner_scan;

fn id(s: &str) -> LabelId {
    LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_derived_set_checked_against_a_store() {
    let np = id("np-0007");
    let bid = id("bid-0003");
    let input_a = LabelSet::from_ids([np.clone()]);
    let input_b = LabelSet::from_ids([bid.clone()]);
    let derived = input_a.union(&input_b);

    let store_holding_one = StoreRule {
        store: StoreId::Overlay,
        holds: LabelAllow::from_ids([
            fieldbook_core::labels::PUBLIC,
            fieldbook_core::labels::UNLABELLED,
            np.clone(),
        ]),
        default: LabelSet::unlabelled(),
        readers: None,
        changers: None,
        hold: false,
    };
    let err = admit(&derived, &store_holding_one).unwrap_err();
    assert_eq!(err.code, Code::Fb0412);
    assert_eq!(err.reason, RefuseReason::Store);

    let store_holding_both = StoreRule {
        store: StoreId::Overlay,
        holds: LabelAllow::from_ids([
            fieldbook_core::labels::PUBLIC,
            fieldbook_core::labels::UNLABELLED,
            np,
            bid,
        ]),
        default: LabelSet::unlabelled(),
        readers: None,
        changers: None,
        hold: false,
    };
    assert_eq!(admit(&derived, &store_holding_both), Ok(()));
}

#[test]
fn an_added_label_is_admitted_by_the_store() {
    let np = id("np-0007");
    let ledger = Ledger::empty();
    let plan = ledger.plan_add(
        ["SO-000042-v1"],
        &LabelSet::from_ids([np.clone()]),
        LedgerOrigin::Person,
    );
    let updated_ledger = Ledger::parse(&plan.text).unwrap();
    let eff = effective(&LabelSet::unlabelled(), "SO-000042-v1", &updated_ledger);
    assert_eq!(eff, LabelSet::from_ids([np.clone()]));
    assert!(!eff.is_unresolved());

    let floor_overlay = StoreRule::floor(StoreId::Overlay, false);
    assert_eq!(admit(&eff, &floor_overlay).unwrap_err().code, Code::Fb0412);

    let configured_overlay = StoreRule {
        store: StoreId::Overlay,
        holds: LabelAllow::from_ids([
            fieldbook_core::labels::PUBLIC,
            fieldbook_core::labels::UNLABELLED,
            np,
        ]),
        default: LabelSet::unlabelled(),
        readers: None,
        changers: None,
        hold: false,
    };
    assert_eq!(admit(&eff, &configured_overlay), Ok(()));
}

#[test]
fn the_repository_corpus_path_end_to_end() {
    let corpus_rule = StoreRule::corpus();
    let rules = MarkingRules::builtin();
    let plain_text = "Synthetic public regulation excerpt with no banner.";

    // 1. Plain text with no act -> `unlabelled`, refused by `corpus` with FB0410.
    let no_act_labels = for_text(plain_text, &rules, &LabelContext::none());
    assert_eq!(no_act_labels, LabelSet::unlabelled());
    let err_no_act = admit(&no_act_labels, &corpus_rule).unwrap_err();
    assert_eq!(err_no_act.code, Code::Fb0410);
    assert_eq!(err_no_act.reason, RefuseReason::Store);

    // 2. Same text with `PublicAct::edition()` -> `public`, admitted.
    let edition_labels = for_text(plain_text, &rules, &LabelContext::act(PublicAct::edition()));
    assert_eq!(edition_labels, LabelSet::public());
    assert_eq!(admit(&edition_labels, &corpus_rule), Ok(()));

    // 3. Same text with `PublicAct::typed(false, true)` -> `public`, admitted.
    let typed_act = PublicAct::typed(false, true).unwrap();
    let typed_labels = for_text(plain_text, &rules, &LabelContext::act(typed_act));
    assert_eq!(typed_labels, LabelSet::public());
    assert_eq!(admit(&typed_labels, &corpus_rule), Ok(()));

    // 4. Text with a synthetic banner -> `marked`, refused by `corpus` with FB0400 with or without act.
    let banner = banner_scan::PATTERNS[0].text;
    let banner_text = format!("{banner} synthetic controlled test line.");

    let marked_no_act = for_text(&banner_text, &rules, &LabelContext::none());
    assert_eq!(marked_no_act, LabelSet::marked());
    assert_eq!(
        admit(&marked_no_act, &corpus_rule).unwrap_err().code,
        Code::Fb0400
    );

    let marked_with_edition = for_text(
        &banner_text,
        &rules,
        &LabelContext::act(PublicAct::edition()),
    );
    assert_eq!(marked_with_edition, LabelSet::marked());
    assert_eq!(
        admit(&marked_with_edition, &corpus_rule).unwrap_err().code,
        Code::Fb0400
    );

    let marked_with_typed = for_text(&banner_text, &rules, &LabelContext::act(typed_act));
    assert_eq!(marked_with_typed, LabelSet::marked());
    assert_eq!(
        admit(&marked_with_typed, &corpus_rule).unwrap_err().code,
        Code::Fb0400
    );
}

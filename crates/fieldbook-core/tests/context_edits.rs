//! Context-pack edits, view fingerprinting, saved-pack resolution, and Unicode-scalar
//! request budget vector tests (`handoff-context` steps 3, 4, 10, 12, 14).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

use std::path::Path;

use fieldbook_core::assist::{
    Budget, ContextIds, ContextPack, Edits, Include, NamedPack, Origin, Scope, Why, apply_edits,
    context_pack, resolve_pack, scalar_count, view_fingerprint,
};
use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Model, Options, Tree, load};
use fieldbook_core::date::CivilDate;
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};

const POLICY_CONTEXT: &str = include_str!("../../../tests/fixtures/policy/context.toml");
const TODAY_RAW: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");
const ASSISTANT_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
const BUDGET: usize = 12_000;

fn today() -> CivilDate {
    CivilDate::parse(TODAY_RAW.trim()).unwrap()
}

fn now() -> Now {
    Now::new(today(), 43_200).unwrap()
}

fn insert_all(tree: &mut Tree, dir: &Path, rel: &str) {
    tree.insert_dir(rel);
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let child = format!("{rel}/{name}");
        if entry.file_type().unwrap().is_dir() {
            insert_all(tree, &entry.path(), &child);
        } else {
            tree.insert_bytes(child, std::fs::read(entry.path()).unwrap());
        }
    }
}

fn mini_tree() -> Tree {
    let corpus =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini/corpus");
    let mut tree = Tree::new();
    insert_all(&mut tree, &corpus, "corpus");
    tree
}

fn load_tree(tree: Tree) -> Model {
    load(&Host::new(tree, today()), &Options::new(false)).0
}

fn policy_default() -> EffectivePolicy {
    let policy = Policy::parse(POLICY_CONTEXT, None).unwrap();
    EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset())
}

fn policy_with_np() -> EffectivePolicy {
    let text = POLICY_CONTEXT.replace(
        "allow = [\"public\", \"owner-supplied\"]",
        "allow = [\"public\", \"owner-supplied\", \"np-0007\"]",
    );
    let policy = Policy::parse(&text, None).unwrap();
    EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset())
}

fn view_for(model: &Model, eff: &EffectivePolicy) -> LabelledView {
    let Decision::Allowed(token) = check(eff, &Dest::Handoff, None, None, now(), Shipped::Hours(1))
    else {
        panic!("handoff selection must be allowed");
    };
    LabelledView::new(model, &token, &ReadableStores::corpus())
}

/// The `Status:` line of a claim's block, which is where an opponent is marked contradicted.
fn claim_status(pack: &ContextPack, claim_id: &str) -> String {
    let block = pack
        .blocks()
        .iter()
        .find(|b| b.id == claim_id && b.kind == "claim")
        .unwrap_or_else(|| panic!("{claim_id} has no block"));
    block
        .text
        .lines()
        .find_map(|line| line.strip_prefix("Status: "))
        .unwrap_or_else(|| panic!("{claim_id} has no status line"))
        .to_owned()
}

// -----------------------------------------------------------------------------
// Edits (step 3 & step 10)
// -----------------------------------------------------------------------------

#[test]
fn a_removed_item_is_listed_and_not_included() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(
        &view,
        "rate window",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let target = base
        .items()
        .iter()
        .find(|i| i.included)
        .unwrap()
        .unit
        .clone();

    let edits = Edits {
        removed: vec![target.clone()],
        ..Edits::default()
    };
    let edited = apply_edits(&view, &base, &edits, BUDGET);
    let item = edited.items().iter().find(|i| i.unit == target).unwrap();
    assert!(!item.included, "removed item must have included = false");
    assert!(!edited.blocks().iter().any(|b| b.id == target));
    assert!(!ContextIds::from_pack(&edited).contains(&target));
}

#[test]
fn a_pinned_item_goes_first_and_keeps_its_origin() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(
        &view,
        "rate window",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let pin_id = "est-rate-reference".to_owned();

    let edits = Edits {
        pinned: vec![pin_id.clone()],
        ..Edits::default()
    };
    let edited = apply_edits(&view, &base, &edits, BUDGET);
    assert_eq!(edited.items()[0].unit, pin_id);
    assert_eq!(edited.items()[0].origin, Origin::Pinned);
    assert!(edited.items()[0].included);
    assert_eq!(edited.blocks()[0].id, pin_id);
}

#[test]
fn an_added_claim_brings_its_conflict() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(
        &view,
        "escalation",
        &Scope::Area("est".to_owned()),
        BUDGET,
        &Include::default(),
    );
    assert!(!base.items().iter().any(|i| i.unit == "CL-CMP-0001"));

    let edits = Edits {
        added: vec!["CL-CMP-0001".to_owned()],
        ..Edits::default()
    };
    let edited = apply_edits(&view, &base, &edits, BUDGET);
    let c1 = edited
        .items()
        .iter()
        .find(|i| i.unit == "CL-CMP-0001")
        .unwrap();
    assert_eq!(c1.origin, Origin::Added);
    assert_eq!(c1.why, Why::Named);
    assert!(c1.included);

    let xc = edited.items().iter().find(|i| i.unit == "XC-01").unwrap();
    assert_eq!(xc.why, Why::Closure);
    assert_eq!(xc.via.as_deref(), Some("CL-CMP-0001"));
    assert!(xc.included);

    let opp = edited
        .items()
        .iter()
        .find(|i| i.unit == "CL-CMP-0002")
        .unwrap();
    assert_eq!(opp.why, Why::Closure);
    assert_eq!(opp.via.as_deref(), Some("CL-CMP-0001"));
    assert!(opp.included);
}

// control: HG-02
#[test]
fn an_added_opponent_does_not_relabel_the_claim_it_opposes() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let question = "CL-CMP-0001";
    let base = context_pack(&view, question, &Scope::All, BUDGET, &Include::default());
    assert_eq!(
        claim_status(&base, "CL-CMP-0001"),
        "primary_source_supported",
        "the selected claim starts with its own status"
    );

    // `CL-CMP-0002` is already the opposing claim of `CL-CMP-0001`. Naming it must not turn the
    // claim the pack was built around into the contradicted opponent (HG-02).
    for (name, edits) in [
        (
            "added",
            Edits {
                added: vec!["CL-CMP-0002".to_owned()],
                ..Edits::default()
            },
        ),
        (
            "suggested",
            Edits {
                suggested: vec!["CL-CMP-0002".to_owned()],
                ..Edits::default()
            },
        ),
    ] {
        let edited = apply_edits(&view, &base, &edits, BUDGET);
        assert_eq!(
            claim_status(&edited, "CL-CMP-0001"),
            "primary_source_supported",
            "{name} CL-CMP-0002 must not relabel CL-CMP-0001"
        );
        assert_eq!(
            claim_status(&edited, "CL-CMP-0002"),
            "contradicted (conflict XC-01)",
            "{name} keeps the opponent its own status line"
        );
    }
}

// control: HG-02
#[test]
fn a_pinned_opponent_takes_the_status_leaving_the_claim_it_opposes_clean() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(
        &view,
        "CL-CMP-0001",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );

    // Pinning an opponent puts it first, so it becomes the anchor of `XC-01`: it keeps its own
    // status and the claim it opposes is the one shown as contradicted.
    let edited = apply_edits(
        &view,
        &base,
        &Edits {
            pinned: vec!["CL-CMP-0002".to_owned()],
            ..Edits::default()
        },
        BUDGET,
    );
    assert_eq!(edited.blocks()[0].id, "CL-CMP-0002");
    assert_eq!(claim_status(&edited, "CL-CMP-0002"), "unchecked");
    assert_eq!(
        claim_status(&edited, "CL-CMP-0001"),
        "contradicted (conflict XC-01)"
    );
}

#[test]
fn an_unknown_id_is_stale_and_not_included() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(
        &view,
        "rate window",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let edits = Edits {
        added: vec!["NO-SUCH-9999".to_owned()],
        ..Edits::default()
    };
    let edited = apply_edits(&view, &base, &edits, BUDGET);
    let unknown = edited
        .items()
        .iter()
        .find(|i| i.unit == "NO-SUCH-9999")
        .unwrap();
    assert!(unknown.stale);
    assert!(!unknown.included);
    assert_eq!(unknown.origin, Origin::Added);
    assert_eq!(unknown.why, Why::Named);
}

// -----------------------------------------------------------------------------
// View fingerprint and resolve_pack (step 4 & step 10)
// -----------------------------------------------------------------------------

#[test]
fn view_fingerprint_changes_with_a_unit_text() {
    let eff = policy_default();
    let v1 = view_for(&load_tree(mini_tree()), &eff);
    let fp1 = view_fingerprint(&v1);

    let mut tree2 = mini_tree();
    let path = "corpus/pages/compliance/business-systems/cmp-system-review.md";
    let body = String::from_utf8(tree2.get_bytes(path).unwrap().to_vec()).unwrap();
    let updated = body.replace(
        "Explain how a synthetic system review interacts with a synthetic billing withhold.",
        "Explain how an updated synthetic system review interacts with a synthetic billing withhold.",
    );
    tree2.insert_bytes(path, updated.into_bytes());
    let v2 = view_for(&load_tree(tree2), &eff);
    let fp2 = view_fingerprint(&v2);

    assert_ne!(
        fp1, fp2,
        "changing a unit's text must change view_fingerprint"
    );
}

#[test]
fn view_fingerprint_changes_with_effective_labels() {
    let eff = policy_with_np();
    let v1 = view_for(&load_tree(mini_tree()), &eff);
    let fp1 = view_fingerprint(&v1);

    let mut tree2 = mini_tree();
    let path = "corpus/pages/compliance/business-systems/cmp-system-review.md";
    let body = String::from_utf8(tree2.get_bytes(path).unwrap().to_vec()).unwrap();
    let updated = body.replace("handling = [\"public\"]", "handling = [\"np-0007\"]");
    tree2.insert_bytes(path, updated.into_bytes());
    let v2 = view_for(&load_tree(tree2), &eff);
    let fp2 = view_fingerprint(&v2);

    assert_ne!(
        fp1, fp2,
        "changing a unit's effective labels must change view_fingerprint"
    );
}

#[test]
fn resolve_pack_never_reincludes_an_unavailable_pin() {
    let eff = policy_default();
    let view = view_for(&load_tree(mini_tree()), &eff);
    let named = NamedPack {
        name: "saved-test".to_owned(),
        ids: vec!["cmp-system-review".to_owned(), "MISSING-UNIT-01".to_owned()],
        view_fingerprint: "0".repeat(64), // intentionally different fingerprint
    };
    let resolved = resolve_pack(&view, &named);
    assert_eq!(resolved.len(), 2);

    let avail = &resolved[0];
    assert_eq!(avail.unit, "cmp-system-review");
    assert_eq!(avail.origin, Origin::Pinned);
    assert_eq!(avail.why, Why::Named);
    assert!(avail.included);
    assert!(
        avail.stale,
        "different fingerprint marks available item stale"
    );

    let missing = &resolved[1];
    assert_eq!(missing.unit, "MISSING-UNIT-01");
    assert_eq!(missing.origin, Origin::Pinned);
    assert_eq!(missing.why, Why::Named);
    assert!(
        !missing.included,
        "a changed fingerprint must never re-include an unavailable pin"
    );
    assert!(missing.stale);

    // Passing the unavailable pin into apply_edits disables send until removed.
    let base = context_pack(
        &view,
        "withholding",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let with_missing_pin = apply_edits(
        &view,
        &base,
        &Edits {
            pinned: vec!["MISSING-UNIT-01".to_owned()],
            ..Edits::default()
        },
        BUDGET,
    );
    assert!(!with_missing_pin.request_budget().sendable);

    let removed_pin = apply_edits(
        &view,
        &with_missing_pin,
        &Edits {
            removed: vec!["MISSING-UNIT-01".to_owned()],
            ..Edits::default()
        },
        BUDGET,
    );
    assert!(removed_pin.request_budget().sendable);
}

#[test]
fn resolve_pack_marks_every_item_stale_when_the_fingerprint_differs() {
    let eff = policy_default();
    let view = view_for(&load_tree(mini_tree()), &eff);
    let current_fp = view_fingerprint(&view);

    let fresh = NamedPack {
        name: "fresh".to_owned(),
        ids: vec!["cmp-system-review".to_owned(), "CL-CMP-0001".to_owned()],
        view_fingerprint: current_fp,
    };
    let resolved_fresh = resolve_pack(&view, &fresh);
    assert!(resolved_fresh.iter().all(|i| i.included && !i.stale));

    let stale = NamedPack {
        name: "stale".to_owned(),
        ids: vec!["cmp-system-review".to_owned(), "CL-CMP-0001".to_owned()],
        view_fingerprint: "f".repeat(64),
    };
    let resolved_stale = resolve_pack(&view, &stale);
    assert!(resolved_stale.iter().all(|i| i.included && i.stale));
}

// -----------------------------------------------------------------------------
// RequestBudget and Budget vectors (steps 12 & 14)
// -----------------------------------------------------------------------------

#[test]
fn pins_over_budget_disable_send() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(
        &view,
        "withholding",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );

    let edits = Edits {
        pinned: vec![
            "cmp-system-review".to_owned(),
            "est-rate-reference".to_owned(),
        ],
        ..Edits::default()
    };
    let tiny = 400;
    let edited = apply_edits(&view, &base, &edits, tiny);
    let rb = edited.request_budget();

    assert!(!rb.sendable, "over-budget pins must disable send");
    assert!(rb.over_by > 0, "over_by must be positive");
    assert_eq!(rb.over_by, rb.total - rb.limit);
    // Pinned items remain included and visible in preview text.
    assert!(
        edited
            .items()
            .iter()
            .any(|i| i.unit == "cmp-system-review" && i.included && i.origin == Origin::Pinned)
    );
    assert!(
        edited
            .items()
            .iter()
            .any(|i| i.unit == "est-rate-reference" && i.included && i.origin == Origin::Pinned)
    );
    assert!(edited.to_text().contains("cmp-system-review"));
    assert!(edited.to_text().contains("est-rate-reference"));
}

#[test]
fn pin_closure_is_not_cut() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(
        &view,
        "escalation",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );

    // Pinning CL-CMP-0001 brings its required conflict closure (XC-01, CL-CMP-0002) and source (S-0001-v1).
    let edits = Edits {
        pinned: vec!["CL-CMP-0001".to_owned()],
        ..Edits::default()
    };
    let full_pinned = apply_edits(&view, &base, &edits, BUDGET);
    let c1_block_len = full_pinned
        .blocks()
        .iter()
        .find(|b| b.id == "CL-CMP-0001")
        .unwrap()
        .text
        .chars()
        .count();
    let tight = full_pinned.request_budget().rules
        + full_pinned.request_budget().question
        + c1_block_len
        + 50;

    let edited = apply_edits(&view, &base, &edits, tight);
    assert!(
        !edited.request_budget().sendable,
        "mandatory pin + closure exceeds tight budget"
    );
    for required in ["CL-CMP-0001", "XC-01", "CL-CMP-0002", "S-0001-v1"] {
        assert!(
            edited
                .items()
                .iter()
                .any(|i| i.unit == required && i.included),
            "pin closure member {required} must never be cut"
        );
    }
}

#[test]
fn unicode_budget_counts_scalars() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    // Multi-byte UTF-8 characters: each '§' is 2 bytes, each '—' / '…' is 3 bytes (1 scalar each).
    let question = "§ 9900.101 — withholding… §§§ — — — ………";
    assert!(question.len() > question.chars().count() + 15);

    let full = context_pack(
        &view,
        question,
        &Scope::Area("cmp".to_owned()),
        BUDGET,
        &Include::default(),
    );
    let exact_scalars = full.to_text().chars().count();
    let exact_bytes = full.to_text().len();
    assert!(exact_bytes > exact_scalars);
    assert_eq!(full.request_budget().total, exact_scalars);

    // Budget equal to exact_scalars (< exact_bytes) MUST be sendable with zero trimming!
    let at_scalars = context_pack(
        &view,
        question,
        &Scope::Area("cmp".to_owned()),
        exact_scalars,
        &Include::default(),
    );
    assert!(
        at_scalars.request_budget().sendable,
        "budget at exact Unicode scalar count ({exact_scalars} < {exact_bytes} bytes) must fit"
    );
    assert_eq!(at_scalars.request_budget().over_by, 0);
    assert_eq!(at_scalars.blocks().len(), full.blocks().len());
}

#[test]
fn whole_request_budget_includes_question_and_rules() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let long_question = "synthetic ".repeat(40);
    let budget = Budget::for_question(200, ASSISTANT_MD, &long_question);
    let pack = context_pack(
        &view,
        &long_question,
        &Scope::All,
        budget,
        &Include::default(),
    );

    let rb = pack.request_budget();
    assert!(
        rb.rules
            >= ASSISTANT_MD
                .lines()
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .count()
    );
    assert!(rb.question > long_question.chars().count());
    assert_eq!(rb.history, 0);
    assert_eq!(rb.total, pack.to_text().chars().count());
    assert!(
        !rb.sendable,
        "question + rules alone exceed 200 scalars so sendable must be false"
    );
    assert!(rb.over_by > 0);
}

#[test]
fn optional_hits_trim_before_pins() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let base = context_pack(&view, "synthetic", &Scope::All, BUDGET, &Include::default());
    let pin_id = "est-calc-01".to_owned();
    let edits = Edits {
        pinned: vec![pin_id.clone()],
        ..Edits::default()
    };
    let full = apply_edits(&view, &base, &edits, BUDGET);
    assert!(full.blocks().len() > 3);

    // Pick a budget that fits the pinned item + its closure, plus a couple of optional items,
    // but NOT the full set of optional hits.
    let pin_only = apply_edits(
        &view,
        &context_pack(&view, "synthetic", &Scope::All, BUDGET, &Include::default()),
        &Edits {
            pinned: vec![pin_id.clone()],
            removed: base.items().iter().map(|i| i.unit.clone()).collect(),
            ..Edits::default()
        },
        BUDGET,
    );
    let mid_budget = pin_only.request_budget().total + 350;
    assert!(mid_budget < full.request_budget().total);

    let trimmed = apply_edits(&view, &base, &edits, mid_budget);
    assert!(trimmed.request_budget().sendable);
    assert!(trimmed.request_budget().total <= mid_budget);
    assert!(
        trimmed
            .items()
            .iter()
            .any(|i| i.unit == pin_id && i.included && i.origin == Origin::Pinned),
        "pinned item must stay included"
    );
    assert!(
        !trimmed.left_out_budget().is_empty(),
        "optional hits must trim before pins"
    );
    assert!(
        !trimmed.left_out_budget().iter().any(|b| b.id == pin_id),
        "pinned item must never appear in left_out_budget"
    );
}

#[test]
fn budget_reservation_matches_final_serialization_and_json_escape_expansion() {
    let model = load_tree(mini_tree());
    let view = view_for(&model, &policy_default());
    let history = "User: earlier question\nAssistant: earlier answer";
    let repair = "Repair: fix citation format";
    let question = "Quote \"alpha\\beta\"\nnext";

    let budget = Budget::with_history(BUDGET, ASSISTANT_MD, question, history).with_repair(repair);
    let pack = context_pack(&view, question, &Scope::All, budget, &Include::default());
    let rb = pack.request_budget();

    let expected_total =
        scalar_count(&pack.to_text()) + scalar_count(history) + 2 + scalar_count(repair) + 2;
    assert_eq!(rb.total, expected_total);
    assert_eq!(rb.history, scalar_count(history) + 2);
    assert!(rb.rules >= budget.rules);
    assert_eq!(rb.question, budget.question);

    // JSON-escaped serialization expands quotes/backslashes/newlines; verify measuring the exact
    // JSON-serialized payload catches expansion above the raw string scalar count.
    let raw_text = pack.to_text();
    let json_serialized = serde_json::to_string(&raw_text).unwrap();
    let raw_scalars = scalar_count(&raw_text);
    let json_scalars = scalar_count(&json_serialized);
    assert!(
        json_scalars > raw_scalars,
        "JSON escaping expands quotes, backslashes and newlines"
    );
    // If a transport cap sits strictly between raw_scalars and json_scalars, checking the exact
    // serialized payload prevents bypass.
    let cap = raw_scalars + 1;
    assert!(json_scalars > cap);
}

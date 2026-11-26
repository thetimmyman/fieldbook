//! Core context pack tests (`handoff-context` steps 2, 5, 8, 10; controls `HG-01` and `HG-02`).
//!
//! Runs over `tests/fixtures/corpus-mini` plus in-memory synthetic `np-0007` records and
//! `tests/fixtures/policy/context.toml` (and a copy allowing `np-0007`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture and golden vector file"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::collections::BTreeSet;
use std::path::Path;

use fieldbook_core::assist::{
    Budget, ContextItem, ContextPack, Edits, Include, Scope, Why, apply_edits, context_pack,
    context_pack_for, export_units,
};
use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Model, Options, Tree, load};
use fieldbook_core::date::CivilDate;
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::labels::{LabelAllow, LabelId, LabelSet, OWNER_SUPPLIED, PUBLIC};
use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};
use fieldbook_core::strings;
use serde::Deserialize;

const POLICY_CONTEXT: &str = include_str!("../../../tests/fixtures/policy/context.toml");
const TODAY_RAW: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");
const ASSISTANT_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
const BUDGET: usize = 12_000;

const NP_PAGE_TITLE: &str = "Synthetic nda supplier pricing clause overview";
const MARKER_PAGE: &str = "zzctxpagenp7";
const MARKER_SOURCE: &str = "zzctxsourcenp7";
const MARKER_CLAIM: &str = "zzctxclaimnp7";
const MARKER_HELD: &str = "zzctxheldtext";

fn lid(s: &str) -> LabelId {
    LabelId::parse(s).unwrap()
}

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

fn csv_row(header: &str, cells: &[(&str, &str)]) -> String {
    header
        .split(',')
        .map(|col| {
            let value = cells
                .iter()
                .find(|(c, _)| *c == col)
                .map_or("", |(_, v)| *v);
            if value.contains(',') {
                format!("\"{value}\"")
            } else {
                value.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn concept_page(id: &str, title: &str, handling: &str, purpose: &str, cite: &str) -> String {
    format!(
        "+++\nid = \"{id}\"\ntitle = \"{title}\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"{handling}\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\n{purpose}\n\n## The rule\n\nSynthetic rule text {cite}.\n\n## How it works\n\nSynthetic mechanism.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSynthetic walk-through.\n\n## Never confuse\n\nSynthetic confusion.\n\n## Company calibration\n\nSynthetic calibration.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n"
    )
}

#[derive(Clone, Copy, Default)]
struct Plant {
    np_records: bool,
    contradicted_claim: bool,
    contradicted_conflict_claim: bool,
    /// The contradicted conflict claim cites a source no other planted claim cites.
    promoted_own_source: bool,
    /// A third claim conflicts with the contradicted conflict claim in a second conflict.
    promoted_third_claim: bool,
    /// The first-time opponent CL-CMP-0002 cites a source no other planted claim cites.
    opponent_own_source: bool,
    /// A fourth claim conflicts with the first-time opponent CL-CMP-0002 in a second conflict.
    opponent_third_claim: bool,
    quarantined_source: bool,
    extra_pages: usize,
}

fn fixture_tree(plant: Plant) -> Tree {
    let corpus =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini/corpus");
    let mut tree = Tree::new();
    insert_all(&mut tree, &corpus, "corpus");

    let sources = std::fs::read_to_string(corpus.join("registers/sources.csv")).unwrap();
    let sources_header = sources.lines().next().unwrap().to_owned();
    let mut sources_out: Vec<String> = sources.lines().map(str::to_owned).collect();

    let claims = std::fs::read_to_string(corpus.join("registers/claims.csv")).unwrap();
    let claims_header = claims.lines().next().unwrap().to_owned();
    let mut claims_out: Vec<String> = claims.lines().map(str::to_owned).collect();

    if plant.np_records {
        let page = concept_page(
            "cmp-np-canary",
            NP_PAGE_TITLE,
            "np-0007",
            &format!("Restricted synthetic page {MARKER_PAGE}."),
            "[@CL-CMP-0001]",
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-np-canary.md",
            page.into_bytes(),
        );
        sources_out.push(csv_row(
            &sources_header,
            &[
                ("source_id", "S-0006"),
                ("version", "1"),
                ("tag", "synthetic-np"),
                ("title", &format!("{NP_PAGE_TITLE} {MARKER_SOURCE}")),
                ("publisher", "Synthetic Board"),
                ("source_kind", "primary_law"),
                ("source_role", "primary_evidence"),
                ("authority_tier", "A"),
                ("licence", "Synthetic, no restrictions"),
                ("location", "metadata-only: made up for tests"),
                ("sha256", &"6".repeat(64)),
                ("as_of", "2026-02-01"),
                ("retrieved", "2026-02-01"),
                ("lifecycle", "active"),
                ("volatility", "V2"),
                ("handling", "np-0007"),
                ("note", "Restricted synthetic source note."),
                ("purpose", "Synthetic purpose for context tests"),
            ],
        ));
        claims_out.push(csv_row(
            &claims_header,
            &[
                ("claim_id", "CL-CMP-0009"),
                ("area", "cmp"),
                ("kind", "fact"),
                (
                    "statement",
                    &format!("Synthetic restricted claim {MARKER_CLAIM}."),
                ),
                ("as_of", "2026-02-01"),
                ("source_id", "S-0006"),
                ("version", "1"),
                ("locator_kind", "text_section"),
                ("locator", "sec. 1"),
                ("quote", "Synthetic restricted quote."),
                ("consequential", "false"),
                ("drafted_by", "codex"),
                ("verification", "unchecked"),
                ("applicability", "pending_company_confirmation"),
                ("adoption", "proposed"),
                ("data_origin", "observed"),
                ("volatility", "V2"),
            ],
        ));
    }

    if plant.contradicted_conflict_claim {
        for row in &mut claims_out {
            if row.starts_with("CL-CMP-0001,") {
                let next = row.replacen(",primary_source_supported,", ",contradicted,", 1);
                assert_ne!(next, *row, "the verification column text must have changed");
                *row = next;
            }
        }
    }

    if plant.promoted_own_source {
        sources_out.push(csv_row(
            &sources_header,
            &[
                ("source_id", "S-0009"),
                ("version", "1"),
                ("tag", "synthetic-promoted"),
                ("title", "Synthetic promoted guide, made up for tests"),
                ("publisher", "Synthetic Board"),
                ("source_kind", "primary_law"),
                ("source_role", "primary_evidence"),
                ("authority_tier", "A"),
                ("licence", "Synthetic, no restrictions"),
                ("location", "metadata-only: made up for tests"),
                ("sha256", &"9".repeat(64)),
                ("as_of", "2026-02-01"),
                ("retrieved", "2026-02-01"),
                ("lifecycle", "active"),
                ("volatility", "V2"),
                ("handling", "public"),
                ("note", "Synthetic source for promotion tests."),
                ("purpose", "Synthetic purpose for context tests"),
            ],
        ));
        let mut replaced = false;
        for row in &mut claims_out {
            if row.starts_with("CL-CMP-0001,") {
                let next = row.replacen(",S-0001,1,", ",S-0009,1,", 1);
                replaced = next != *row;
                *row = next;
            }
        }
        assert!(replaced, "the claim row must change its source");
    }

    if plant.promoted_third_claim {
        claims_out.push(csv_row(
            &claims_header,
            &[
                ("claim_id", "CL-CMP-0003"),
                ("area", "cmp"),
                ("kind", "fact"),
                ("statement", "Synthetic zeta marker is 7 units."),
                ("as_of", "2026-02-01"),
                ("source_id", "S-0001"),
                ("version", "1"),
                ("locator_kind", "text_section"),
                ("locator", "sec. 3"),
                ("quote", "Synthetic zeta marker."),
                ("consequential", "false"),
                ("drafted_by", "codex"),
                ("verification", "unchecked"),
                ("applicability", "pending_company_confirmation"),
                ("adoption", "proposed"),
                ("data_origin", "observed"),
                ("volatility", "V2"),
            ],
        ));
        let path = "corpus/registers/conflicts.csv";
        let conflicts = std::fs::read_to_string(corpus.join("registers/conflicts.csv")).unwrap();
        let header = conflicts.lines().next().unwrap().to_owned();
        let mut rows: Vec<String> = conflicts.lines().map(str::to_owned).collect();
        rows.push(csv_row(
            &header,
            &[
                ("conflict_id", "XC-02"),
                ("area", "cmp"),
                ("topic", "Synthetic zeta marker versus review interval"),
                (
                    "positions",
                    "Synthetic review runs every 30 days|Synthetic zeta marker is 7 units",
                ),
                ("status", "unresolved"),
                ("claim_ids", "CL-CMP-0001|CL-CMP-0003"),
                ("question_id", "Q-001"),
            ],
        ));
        tree.insert_bytes(path, format!("{}\n", rows.join("\n")).into_bytes());
    }

    if plant.opponent_own_source {
        sources_out.push(csv_row(
            &sources_header,
            &[
                ("source_id", "S-0010"),
                ("version", "1"),
                ("tag", "synthetic-opponent"),
                ("title", "Synthetic opponent guide, made up for tests"),
                ("publisher", "Synthetic Board"),
                ("source_kind", "primary_law"),
                ("source_role", "primary_evidence"),
                ("authority_tier", "A"),
                ("licence", "Synthetic, no restrictions"),
                ("location", "metadata-only: made up for tests"),
                ("sha256", &"a".repeat(64)),
                ("as_of", "2026-02-01"),
                ("retrieved", "2026-02-01"),
                ("lifecycle", "active"),
                ("volatility", "V2"),
                ("handling", "public"),
                ("note", "Synthetic source for first-time opponent tests."),
                ("purpose", "Synthetic purpose for context tests"),
            ],
        ));
        let mut replaced = false;
        for row in &mut claims_out {
            if row.starts_with("CL-CMP-0002,") {
                let next = row.replacen(",S-0001,1,", ",S-0010,1,", 1);
                replaced = next != *row;
                *row = next;
            }
        }
        assert!(replaced, "the opponent claim row must change its source");
    }

    if plant.opponent_third_claim {
        claims_out.push(csv_row(
            &claims_header,
            &[
                ("claim_id", "CL-CMP-0004"),
                ("area", "cmp"),
                ("kind", "fact"),
                ("statement", "Synthetic omega marker is 9 units."),
                ("as_of", "2026-02-01"),
                ("source_id", "S-0001"),
                ("version", "1"),
                ("locator_kind", "text_section"),
                ("locator", "sec. 4"),
                ("quote", "Synthetic omega marker."),
                ("consequential", "false"),
                ("drafted_by", "codex"),
                ("verification", "unchecked"),
                ("applicability", "pending_company_confirmation"),
                ("adoption", "proposed"),
                ("data_origin", "observed"),
                ("volatility", "V2"),
            ],
        ));
        let path = "corpus/registers/conflicts.csv";
        let conflicts = std::fs::read_to_string(corpus.join("registers/conflicts.csv")).unwrap();
        let header = conflicts.lines().next().unwrap().to_owned();
        let mut rows: Vec<String> = conflicts.lines().map(str::to_owned).collect();
        rows.push(csv_row(
            &header,
            &[
                ("conflict_id", "XC-03"),
                ("area", "cmp"),
                ("topic", "Synthetic omega marker versus withholding threshold"),
                (
                    "positions",
                    "Synthetic withholding threshold is 5 percent|Synthetic omega marker is 9 units",
                ),
                ("status", "unresolved"),
                ("claim_ids", "CL-CMP-0002|CL-CMP-0004"),
                ("question_id", "Q-001"),
            ],
        ));
        tree.insert_bytes(path, format!("{}\n", rows.join("\n")).into_bytes());
    }

    if plant.contradicted_claim {
        claims_out.push(csv_row(
            &claims_header,
            &[
                ("claim_id", "CL-EST-0009"),
                ("area", "est"),
                ("kind", "fact"),
                (
                    "statement",
                    "Synthetic contradicted zebra threshold is 99 percent.",
                ),
                ("as_of", "2026-02-01"),
                ("source_id", "S-0002"),
                ("version", "2"),
                ("locator_kind", "text_section"),
                ("locator", "sec. 9"),
                ("quote", "Synthetic contradicted quote."),
                ("consequential", "false"),
                ("drafted_by", "codex"),
                ("verification", "contradicted"),
                ("applicability", "pending_company_confirmation"),
                ("adoption", "proposed"),
                ("data_origin", "observed"),
                ("volatility", "V2"),
            ],
        ));
    }

    if plant.quarantined_source {
        sources_out.push(csv_row(
            &sources_header,
            &[
                ("source_id", "S-0008"),
                ("version", "1"),
                ("tag", "synthetic-held"),
                ("title", &format!("Synthetic held guide {MARKER_HELD}")),
                ("publisher", "Synthetic Board"),
                ("source_kind", "primary_law"),
                ("source_role", "primary_evidence"),
                ("authority_tier", "A"),
                ("licence", "Synthetic, no restrictions"),
                ("location", "metadata-only: made up for tests"),
                ("sha256", &"8".repeat(64)),
                ("as_of", "2026-02-01"),
                ("retrieved", "2026-02-01"),
                ("lifecycle", "quarantined"),
                ("volatility", "V2"),
                ("handling", "public"),
                ("note", "Synthetic held version for context tests."),
                ("purpose", "Synthetic held purpose"),
            ],
        ));
    }

    for idx in 1..=plant.extra_pages {
        let pid = format!("cmp-extra-{idx:02}");
        let page = concept_page(
            &pid,
            &format!("Synthetic extra compliance hit {idx:02} quorum"),
            "public",
            &format!("Synthetic quorum compliance overview number {idx:02}."),
            "[@CL-CMP-0001]",
        );
        tree.insert_bytes(
            format!("corpus/pages/compliance/business-systems/{pid}.md"),
            page.into_bytes(),
        );
    }

    tree.insert_bytes(
        "corpus/registers/sources.csv",
        format!("{}\n", sources_out.join("\n")).into_bytes(),
    );
    tree.insert_bytes(
        "corpus/registers/claims.csv",
        format!("{}\n", claims_out.join("\n")).into_bytes(),
    );
    tree
}

fn load_model(plant: Plant) -> Model {
    load(
        &Host::new(fixture_tree(plant), today()),
        &Options::new(false),
    )
    .0
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

fn view_for(model: &Model, eff: &EffectivePolicy, dest: &Dest) -> LabelledView {
    let Decision::Allowed(token) = check(eff, dest, None, None, now(), Shipped::Hours(1)) else {
        panic!("selection for {dest:?} must be allowed");
    };
    LabelledView::new(model, &token, &ReadableStores::corpus())
}

fn last_line(text: &str) -> &str {
    text.trim_end_matches('\n').lines().last().unwrap()
}

// -----------------------------------------------------------------------------
// Policy filtering, banners, labels, omission line
// -----------------------------------------------------------------------------

// control: HG-01
#[test]
fn a_record_outside_the_allowed_set_never_reaches_a_pack_or_the_export() {
    let model = load_model(Plant {
        np_records: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack_for(
        &view,
        &Dest::Handoff,
        Some(&eff),
        NP_PAGE_TITLE,
        &LabelSet::owner_supplied(),
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let text = pack.to_text();
    let exported = export_units(&view);

    for marker in [MARKER_PAGE, MARKER_SOURCE, MARKER_CLAIM] {
        assert!(!text.contains(marker), "marker `{marker}` leaked into pack");
        for u in &exported {
            assert!(
                !u.block.contains(marker) && !u.title.contains(marker),
                "marker `{marker}` leaked into export_units entry {}",
                u.id
            );
        }
    }
    assert!(!exported.iter().any(|u| u.id == "cmp-np-canary"));
    assert!(!exported.iter().any(|u| u.id == "S-0006-v1"));
    assert!(!exported.iter().any(|u| u.id == "CL-CMP-0009"));
    assert_eq!(last_line(&text), "Left out (policy): 3 records");
}

#[test]
fn pack_never_ranks_a_unit_outside_the_view() {
    let model = load_model(Plant {
        np_records: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack_for(
        &view,
        &Dest::Handoff,
        Some(&eff),
        NP_PAGE_TITLE,
        &LabelSet::owner_supplied(),
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let text = pack.to_text();

    assert!(
        !pack
            .items()
            .iter()
            .any(|i| i.unit.starts_with("cmp-np-canary")),
        "restricted page must not be ranked: {:?}",
        pack.items()
    );
    assert!(!text.contains("cmp-np-canary"));
    assert!(!text.contains(MARKER_PAGE));
    assert_eq!(last_line(&text), "Left out (policy): 3 records");
}

#[test]
fn a_record_inside_the_allowed_set_is_present_with_its_banner() {
    let model = load_model(Plant {
        np_records: true,
        ..Plant::default()
    });
    let eff = policy_with_np();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack_for(
        &view,
        &Dest::Handoff,
        Some(&eff),
        NP_PAGE_TITLE,
        &LabelSet::owner_supplied(),
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let text = pack.to_text();

    assert!(
        text.contains(MARKER_PAGE) || text.contains(MARKER_SOURCE),
        "allowed np-0007 record missing from pack:\n{text}"
    );
    assert!(
        text.contains("NP-0007: restricted to the named counterparty"),
        "expected np-0007 banner line in pack:\n{text}"
    );
    assert_eq!(last_line(&text), "Left out (policy): 0 records");
}

#[test]
fn the_labels_line_is_the_union_of_the_items_and_the_question() {
    let model = load_model(Plant {
        np_records: true,
        ..Plant::default()
    });

    // Public-only items + owner-supplied question -> `Labels: owner-supplied` (`public` drops beside it).
    let eff_pub = policy_default();
    let view_pub = view_for(&model, &eff_pub, &Dest::Handoff);
    let pack_pub = context_pack_for(
        &view_pub,
        &Dest::Handoff,
        Some(&eff_pub),
        "withholding",
        &LabelSet::owner_supplied(),
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    assert_eq!(pack_pub.labels(), &LabelSet::owner_supplied());
    assert!(pack_pub.to_text().contains("\nLabels: owner-supplied\n"));

    // Allowed np-0007 item + owner-supplied question -> `Labels: np-0007, owner-supplied`.
    let eff_np = policy_with_np();
    let view_np = view_for(&model, &eff_np, &Dest::Handoff);
    let pack_np = context_pack_for(
        &view_np,
        &Dest::Handoff,
        Some(&eff_np),
        NP_PAGE_TITLE,
        &LabelSet::owner_supplied(),
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    assert_eq!(
        pack_np.labels(),
        &LabelSet::from_ids([lid("np-0007"), OWNER_SUPPLIED])
    );
    assert!(
        pack_np
            .to_text()
            .contains("\nLabels: np-0007, owner-supplied\n")
    );
}

#[test]
fn the_pack_ends_with_the_left_out_line() {
    let model = load_model(Plant {
        np_records: true,
        ..Plant::default()
    });
    let eff_handoff = policy_default();
    let view_handoff = view_for(&model, &eff_handoff, &Dest::Handoff);
    let handoff_pack = context_pack_for(
        &view_handoff,
        &Dest::Handoff,
        Some(&eff_handoff),
        "withholding",
        &LabelSet::owner_supplied(),
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    assert_eq!(
        last_line(&handoff_pack.to_text()),
        "Left out (policy): 3 records"
    );

    // Non-count destinations (model / bridge) emit the fixed line with no digit.
    let policy_full_text = include_str!("../../../tests/fixtures/policy/policy.toml");
    let policy_full = Policy::parse(policy_full_text, None).unwrap();
    let mut owner = OwnerSettings::unset();
    owner.endpoints.insert(
        "approved-gateway".to_owned(),
        LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED]),
    );
    let eff_full = EffectivePolicy::new(&PolicyState::Loaded(policy_full), &owner);
    for dest in [
        Dest::Bridge,
        Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        },
    ] {
        let view = view_for(&model, &eff_full, &dest);
        let pack = context_pack_for(
            &view,
            &dest,
            Some(&eff_full),
            "withholding",
            &LabelSet::owner_supplied(),
            &Scope::All,
            BUDGET,
            &Include::default(),
        );
        let text = pack.to_text();
        let last = last_line(&text);
        assert_eq!(last, "Some records were omitted.");
        assert!(!last.chars().any(|c| c.is_ascii_digit()));
    }
}

// -----------------------------------------------------------------------------
// Selection, closure, conflicts, exclusions, scopes, determinism
// -----------------------------------------------------------------------------

#[test]
fn top_8_hits_then_closure_in_order() {
    let model = load_model(Plant {
        extra_pages: 12,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(&view, "quorum", &Scope::All, BUDGET, &Include::default());

    let hit_items: Vec<&ContextItem> = pack.items().iter().filter(|i| i.why == Why::Hit).collect();
    assert_eq!(hit_items.len(), 8, "at most top 8 hits: {hit_items:?}");

    // Closure items must follow the first hit that brought them and have unique unit ids.
    let mut seen = BTreeSet::new();
    for item in pack.items() {
        assert!(seen.insert(&item.unit), "duplicate unit {}", item.unit);
        if item.why == Why::Closure {
            let via = item.via.as_deref().expect("closure item must carry via");
            assert!(
                seen.contains(&via.to_owned()),
                "closure item {} appeared before its via {via}",
                item.unit
            );
        }
    }
}

#[test]
fn budget_drops_whole_blocks_and_lists_them() {
    let model = load_model(Plant::default());
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let full = context_pack(&view, "synthetic", &Scope::All, BUDGET, &Include::default());
    assert!(full.left_out_budget().is_empty());
    assert!(full.blocks().len() > 2);

    let small_budget = ASSISTANT_MD.chars().count() + 450;
    let small = context_pack(
        &view,
        "synthetic",
        &Scope::All,
        small_budget,
        &Include::default(),
    );
    assert!(small.blocks().len() < full.blocks().len());
    assert!(!small.left_out_budget().is_empty());
    assert!(small.request_budget().total <= small_budget);

    for b in small.blocks() {
        let orig = full.blocks().iter().find(|o| o.id == b.id).unwrap();
        assert_eq!(b.text, orig.text, "kept blocks stay whole");
    }
    let text = small.to_text();
    assert!(text.contains(strings::left_out_budget()));
    for dropped in small.left_out_budget() {
        assert!(text.contains(&format!("- {} {}", dropped.kind, dropped.id)));
        assert!(!text.contains(&format!("--- {} {} ---", dropped.kind, dropped.id)));
    }
}

#[test]
fn a_selected_claim_brings_its_conflict_and_opponent() {
    let model = load_model(Plant::default());
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        "CL-CMP-0001",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let included: Vec<&str> = pack
        .items()
        .iter()
        .filter(|i| i.included)
        .map(|i| i.unit.as_str())
        .collect();
    assert!(included.contains(&"CL-CMP-0001"), "{included:?}");
    assert!(included.contains(&"XC-01"), "{included:?}");
    assert!(included.contains(&"CL-CMP-0002"), "{included:?}");

    let opp_block = pack
        .blocks()
        .iter()
        .find(|b| b.id == "CL-CMP-0002")
        .unwrap();
    assert!(
        opp_block
            .text
            .contains("Status: contradicted (conflict XC-01)"),
        "{}",
        opp_block.text
    );
}

// control: HG-02
#[test]
fn a_conflict_dependency_reached_first_as_an_exclusion_is_kept_with_its_partner() {
    let model = load_model(Plant {
        contradicted_conflict_claim: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        "compliance reviews occur every 30 days withholding threshold",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let included: Vec<&str> = pack
        .items()
        .iter()
        .filter(|i| i.included)
        .map(|i| i.unit.as_str())
        .collect();
    // Whichever claim the walk reached first, the conflict and both claims come together.
    assert!(included.contains(&"XC-01"), "{included:?}");
    assert!(included.contains(&"CL-CMP-0001"), "{included:?}");
    assert!(included.contains(&"CL-CMP-0002"), "{included:?}");
}

// control: HG-02
#[test]
fn over_budget_drops_claim_with_its_conflict() {
    let model = load_model(Plant::default());
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let full = context_pack(
        &view,
        "CL-CMP-0001",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let c1_len = full
        .blocks()
        .iter()
        .find(|b| b.id == "CL-CMP-0001")
        .unwrap()
        .text
        .chars()
        .count();
    let xc_len = full
        .blocks()
        .iter()
        .find(|b| b.id == "XC-01")
        .unwrap()
        .text
        .chars()
        .count();
    // Large enough for framing + CL-CMP-0001 alone, too small for CL-CMP-0001 + XC-01 + CL-CMP-0002.
    let base = full.request_budget().rules + full.request_budget().question;
    let tight = base + c1_len + xc_len;
    let pack = context_pack(
        &view,
        "CL-CMP-0001",
        &Scope::All,
        tight,
        &Include::default(),
    );

    let kept_ids: Vec<&str> = pack.blocks().iter().map(|b| b.id.as_str()).collect();
    let dropped_ids: Vec<&str> = pack
        .left_out_budget()
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert!(!kept_ids.contains(&"CL-CMP-0001"), "kept={kept_ids:?}");
    assert!(!kept_ids.contains(&"XC-01"), "kept={kept_ids:?}");
    assert!(!kept_ids.contains(&"CL-CMP-0002"), "kept={kept_ids:?}");
    assert!(
        dropped_ids.contains(&"CL-CMP-0001"),
        "dropped={dropped_ids:?}"
    );
    assert!(dropped_ids.contains(&"XC-01"), "dropped={dropped_ids:?}");
    assert!(
        dropped_ids.contains(&"CL-CMP-0002"),
        "dropped={dropped_ids:?}"
    );
}

const PROMOTE_QUERY: &str = "compliance reviews occur every 30 days withholding threshold";

fn included_units(pack: &ContextPack) -> Vec<String> {
    pack.items()
        .iter()
        .filter(|i| i.included)
        .map(|i| i.unit.clone())
        .collect()
}

// control: HG-02
#[test]
fn a_promoted_conflict_claim_brings_its_own_source() {
    let model = load_model(Plant {
        contradicted_conflict_claim: true,
        promoted_own_source: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        PROMOTE_QUERY,
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let included = included_units(&pack);
    assert!(included.iter().any(|u| u == "CL-CMP-0001"), "{included:?}");
    assert!(included.iter().any(|u| u == "S-0009-v1"), "{included:?}");
}

// control: HG-02
#[test]
fn a_promoted_conflict_claim_brings_its_own_conflicts() {
    let model = load_model(Plant {
        contradicted_conflict_claim: true,
        promoted_third_claim: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        PROMOTE_QUERY,
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let included = included_units(&pack);
    for id in [
        "CL-CMP-0001",
        "CL-CMP-0002",
        "XC-01",
        "XC-02",
        "CL-CMP-0003",
    ] {
        assert!(included.iter().any(|u| u == id), "{id} in {included:?}");
    }
    let selected = pack
        .blocks()
        .iter()
        .find(|b| b.id == "CL-CMP-0002")
        .unwrap();
    assert!(
        !selected.text.contains("Status: contradicted (conflict"),
        "the selected claim keeps its own status: {}",
        selected.text
    );
    let promoted = pack
        .items()
        .iter()
        .find(|i| i.unit == "CL-CMP-0001")
        .unwrap();
    assert!(matches!(promoted.why, Why::Closure), "{promoted:?}");
    assert!(promoted.via.is_some(), "{promoted:?}");
}

// control: HG-02
#[test]
fn a_promoted_conflict_group_is_dropped_together_under_a_tight_budget() {
    let model = load_model(Plant {
        contradicted_conflict_claim: true,
        promoted_third_claim: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let full = context_pack(
        &view,
        PROMOTE_QUERY,
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let len_of = |id: &str| {
        full.blocks()
            .iter()
            .find(|b| b.id == id)
            .unwrap_or_else(|| panic!("{id} missing from the full pack"))
            .text
            .chars()
            .count()
    };
    // Sweep budgets across the range where the group no longer fits: at every size the five
    // connected units are all kept or all dropped, and at least one size drops them.
    let base = full.request_budget().rules + full.request_budget().question;
    let group = [
        "CL-CMP-0001",
        "CL-CMP-0002",
        "XC-01",
        "XC-02",
        "CL-CMP-0003",
    ];
    let total: usize = group.iter().map(|id| len_of(id)).sum();
    let full_kept: Vec<&str> = full.blocks().iter().map(|b| b.id.as_str()).collect();
    assert!(
        group.iter().all(|g| full_kept.contains(g)),
        "the group is kept whole when it fits: {full_kept:?}"
    );
    let mut dropped_somewhere = false;
    for tight in (base..base + total + 400).step_by(7) {
        let pack = context_pack(
            &view,
            PROMOTE_QUERY,
            &Scope::All,
            tight,
            &Include::default(),
        );
        let kept: Vec<&str> = pack.blocks().iter().map(|b| b.id.as_str()).collect();
        let kept_group = group.iter().filter(|g| kept.contains(g)).count();
        assert!(
            kept_group == 0 || kept_group == group.len(),
            "budget {tight}: the group must be kept or dropped together: {kept:?}"
        );
        if kept_group == 0 {
            dropped_somewhere = true;
            let left_out: Vec<&str> = pack
                .left_out_budget()
                .iter()
                .map(|b| b.id.as_str())
                .collect();
            for id in group {
                assert!(left_out.contains(&id), "budget {tight}: {id} lost");
            }
        }
    }
    assert!(dropped_somewhere, "no budget dropped the group");
}

// control: HG-02
#[test]
fn a_first_time_opponent_claim_brings_its_own_source() {
    let model = load_model(Plant {
        opponent_own_source: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        PROMOTE_QUERY,
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let included = included_units(&pack);
    // CL-CMP-0002 is reached first as a new opponent of CL-CMP-0001, not promoted from an
    // exclusion. It must still bring its own source version (HG-02).
    assert!(included.iter().any(|u| u == "CL-CMP-0002"), "{included:?}");
    assert!(included.iter().any(|u| u == "S-0010-v1"), "{included:?}");
}

// control: HG-02
#[test]
fn a_first_time_opponent_claim_brings_its_own_conflicts() {
    let model = load_model(Plant {
        opponent_third_claim: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        PROMOTE_QUERY,
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let included = included_units(&pack);
    for id in [
        "CL-CMP-0001",
        "CL-CMP-0002",
        "XC-01",
        "XC-03",
        "CL-CMP-0004",
    ] {
        assert!(included.iter().any(|u| u == id), "{id} in {included:?}");
    }
}

#[test]
fn tier_d_superseded_and_contradicted_are_left_out_unless_named() {
    let model = load_model(Plant {
        contradicted_claim: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);

    // 1. Tier-D source (`S-0003-v1`): excluded by default on keyword hit, included with --include tier-d or named id.
    let p_tier_d_default = context_pack(
        &view,
        "trade press bulletin",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let tier_d_item = p_tier_d_default
        .items()
        .iter()
        .find(|i| i.unit == "S-0003-v1")
        .expect("S-0003-v1 listed as considered");
    assert!(!tier_d_item.included, "Tier D must be excluded by default");

    let p_tier_d_inc = context_pack(
        &view,
        "trade press bulletin",
        &Scope::All,
        BUDGET,
        &Include {
            tier_d: true,
            ..Include::default()
        },
    );
    assert!(
        p_tier_d_inc
            .items()
            .iter()
            .any(|i| i.unit == "S-0003-v1" && i.included)
    );

    let p_tier_d_named = context_pack(
        &view,
        "Explain S-0003-v1",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let named_item = p_tier_d_named
        .items()
        .iter()
        .find(|i| i.unit == "S-0003-v1")
        .unwrap();
    assert!(named_item.included);
    assert_eq!(named_item.why, Why::Named);

    // 2. Superseded claim (`CL-EST-0002`): excluded by default, included with --include superseded or named id.
    let p_sup_default = context_pack(
        &view,
        "15 days prior guidance",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let sup_item = p_sup_default
        .items()
        .iter()
        .find(|i| i.unit == "CL-EST-0002")
        .expect("CL-EST-0002 listed");
    assert!(!sup_item.included, "superseded claim excluded by default");

    let p_sup_inc = context_pack(
        &view,
        "15 days prior guidance",
        &Scope::All,
        BUDGET,
        &Include {
            superseded: true,
            ..Include::default()
        },
    );
    assert!(
        p_sup_inc
            .items()
            .iter()
            .any(|i| i.unit == "CL-EST-0002" && i.included)
    );

    // 3. Contradicted claim (`CL-EST-0009`): excluded by default, included with --include contradicted or named id.
    let p_con_default = context_pack(
        &view,
        "zebra threshold",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let con_item = p_con_default
        .items()
        .iter()
        .find(|i| i.unit == "CL-EST-0009")
        .expect("CL-EST-0009 listed");
    assert!(!con_item.included, "contradicted claim excluded by default");

    let p_con_inc = context_pack(
        &view,
        "zebra threshold",
        &Scope::All,
        BUDGET,
        &Include {
            contradicted: true,
            ..Include::default()
        },
    );
    assert!(
        p_con_inc
            .items()
            .iter()
            .any(|i| i.unit == "CL-EST-0009" && i.included)
    );

    let p_con_named = context_pack(
        &view,
        "Explain CL-EST-0009",
        &Scope::All,
        BUDGET,
        &Include::default(),
    );
    let con_named = p_con_named
        .items()
        .iter()
        .find(|i| i.unit == "CL-EST-0009")
        .unwrap();
    assert!(con_named.included);
    assert_eq!(con_named.why, Why::Named);
}

#[test]
fn quarantined_is_never_listed() {
    let model = load_model(Plant {
        quarantined_source: true,
        ..Plant::default()
    });
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let all_inc = Include {
        tier_d: true,
        superseded: true,
        contradicted: true,
    };
    let pack = context_pack(&view, "S-0008-v1", &Scope::All, BUDGET, &all_inc);
    assert!(!pack.items().iter().any(|i| i.unit == "S-0008-v1"));
    assert!(!pack.to_text().contains(MARKER_HELD));
    assert!(!export_units(&view).iter().any(|u| u.id == "S-0008-v1"));
}

#[test]
fn why_and_via_are_set_for_each_item() {
    let model = load_model(Plant::default());
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        "rate window",
        &Scope::Page("cmp-system-review".to_owned()),
        BUDGET,
        &Include::default(),
    );

    assert_eq!(pack.items()[0].unit, "cmp-system-review");
    assert_eq!(pack.items()[0].why, Why::Scope);
    assert_eq!(pack.items()[0].via, None);

    for item in pack.items() {
        match item.why {
            Why::Closure => assert!(item.via.is_some(), "closure item {} needs via", item.unit),
            Why::Hit | Why::Scope | Why::Named => {
                assert!(item.via.is_none(), "non-closure item {} had via", item.unit);
            }
        }
    }
    assert!(pack.items().iter().any(|i| i.why == Why::Hit));
    assert!(pack.items().iter().any(|i| i.why == Why::Closure));
}

#[test]
fn area_scope_keeps_hits_in_the_area() {
    let model = load_model(Plant::default());
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let exported = export_units(&view);

    let pack = context_pack(
        &view,
        "synthetic",
        &Scope::Area("est".to_owned()),
        BUDGET,
        &Include::default(),
    );
    let hits: Vec<&ContextItem> = pack.items().iter().filter(|i| i.why == Why::Hit).collect();
    assert!(!hits.is_empty());
    for hit in hits {
        let u = exported.iter().find(|u| u.id == hit.unit).unwrap();
        assert_eq!(u.area, "est", "hit {} had area {}", u.id, u.area);
    }
}

#[test]
fn page_scope_puts_the_page_first() {
    let model = load_model(Plant::default());
    let eff = policy_default();
    let view = view_for(&model, &eff, &Dest::Handoff);
    let pack = context_pack(
        &view,
        "escalation",
        &Scope::Page("cmp-system-review".to_owned()),
        BUDGET,
        &Include::default(),
    );
    assert_eq!(pack.items()[0].unit, "cmp-system-review");
    assert_eq!(pack.items()[0].why, Why::Scope);
    assert_eq!(pack.items()[1].unit, "CL-CMP-0001");
    assert_eq!(pack.items()[1].why, Why::Closure);
    assert_eq!(pack.items()[1].via.as_deref(), Some("cmp-system-review"));
    assert!(
        !pack
            .items()
            .iter()
            .any(|i| i.unit.starts_with("cmp-system-review#")),
        "page's own sections must be left out of hits"
    );
}

#[test]
fn same_inputs_same_bytes() {
    let model1 = load_model(Plant::default());
    let model2 = load_model(Plant::default());
    let eff = policy_default();
    let v1 = view_for(&model1, &eff, &Dest::Handoff);
    let v2 = view_for(&model2, &eff, &Dest::Handoff);
    let p1 = context_pack(&v1, "withholding", &Scope::All, BUDGET, &Include::default());
    let p2 = context_pack(&v2, "withholding", &Scope::All, BUDGET, &Include::default());
    assert_eq!(p1.to_text().as_bytes(), p2.to_text().as_bytes());
    assert_eq!(p1.items(), p2.items());
    assert_eq!(p1.view_fingerprint(), p2.view_fingerprint());
}

// -----------------------------------------------------------------------------
// Fuzz / mutation safety of Scope::parse and Include::parse
// -----------------------------------------------------------------------------

#[test]
fn context_arguments_never_panic_on_random_and_mutated_bytes() {
    let mut r = rng::Rng::new(1);
    for _ in 0..200 {
        let len = (r.next_u64() % 97) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (r.next_u64() & 0xff) as u8).collect();
        let text = String::from_utf8_lossy(&bytes);
        let _ = Scope::parse(&text);
        let _ = Include::parse(&text);
    }

    for seed in ["area:cmp", "page:P-0001", "tier-d,superseded,contradicted"] {
        let orig = seed.as_bytes();
        for i in 0..orig.len() {
            let mut flipped = orig.to_vec();
            flipped[i] ^= 0x55;
            let text = String::from_utf8_lossy(&flipped);
            let _ = Scope::parse(&text);
            let _ = Include::parse(&text);

            let mut deleted = orig.to_vec();
            deleted.remove(i);
            let text = String::from_utf8_lossy(&deleted);
            let _ = Scope::parse(&text);
            let _ = Include::parse(&text);
        }
    }

    assert!(Scope::parse("").is_err());
    assert!(Include::parse("").is_err());
    assert_eq!(Scope::parse("area:cmp"), Ok(Scope::Area("cmp".to_owned())));
    assert_eq!(
        Scope::parse("page:P-0001"),
        Ok(Scope::Page("P-0001".to_owned()))
    );
    assert_eq!(
        Include::parse("tier-d,superseded,contradicted"),
        Ok(Include {
            tier_d: true,
            superseded: true,
            contradicted: true,
        })
    );
}

// -----------------------------------------------------------------------------
// Golden context cases (`tests/golden/context/cases.json`)
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenContextFile {
    kind: String,
    version: u32,
    cases: Vec<GoldenContextCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenInclude {
    tier_d: bool,
    superseded: bool,
    contradicted: bool,
    #[serde(default)]
    notes: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenContextCase {
    id: String,
    question: String,
    scope: Scope,
    budget: usize,
    include: GoldenInclude,
    edits: Edits,
    items: Vec<ContextItem>,
    text: String,
    #[serde(default)]
    note: Option<String>,
}

fn has_edits(e: &Edits) -> bool {
    !e.pinned.is_empty() || !e.removed.is_empty() || !e.added.is_empty() || !e.suggested.is_empty()
}

#[test]
fn goldens_match() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/context/cases.json");
    let raw = std::fs::read_to_string(&path).expect("read tests/golden/context/cases.json");
    let file: GoldenContextFile = serde_json::from_str(&raw).expect("parse context golden JSON");
    assert_eq!(file.kind, "context");
    assert_eq!(file.version, 1);
    assert!(file.cases.len() >= 6, "expected at least 6 golden cases");

    let model = load_model(Plant::default());
    let eff = EffectivePolicy::floor(false);
    let view = view_for(&model, &eff, &Dest::Handoff);

    let mut failures = Vec::new();
    for case in &file.cases {
        let _ = (&case.note, case.include.notes);
        let inc = Include {
            tier_d: case.include.tier_d,
            superseded: case.include.superseded,
            contradicted: case.include.contradicted,
        };
        let budget = Budget::for_question(case.budget, ASSISTANT_MD, &case.question);
        let base: ContextPack = context_pack(&view, &case.question, &case.scope, budget, &inc);
        let final_pack = if has_edits(&case.edits) {
            apply_edits(&view, &base, &case.edits, budget)
        } else {
            base
        };
        if final_pack.items() != case.items.as_slice() {
            failures.push(format!(
                "{}: items mismatch\n  actual:   {:?}\n  expected: {:?}",
                case.id,
                final_pack.items(),
                case.items
            ));
        }
        let actual_text = final_pack.to_text();
        if actual_text != case.text {
            failures.push(format!(
                "{}: text mismatch\n--- actual ---\n{actual_text}\n--- expected ---\n{}",
                case.id, case.text
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "context golden failures ({}):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

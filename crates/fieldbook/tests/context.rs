//! Binary integration tests for `fieldbook context` (`handoff-context` steps 6, 7, 9, 10, 13, 15, 16;
//! control `HG-03` and `AL-04`).

#![cfg(feature = "test-hooks")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "binary integration tests copy fixtures, write temporary policies, and inspect journals"
)]

mod support;

#[path = "support/policy.rs"]
mod policy_support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use fieldbook::context::{HANDOFF_REPLY_TARGET, deliver_pack};
use fieldbook::io::journal::{
    Recovery, read_valid_events_prefix, set_lock_timeout_for_test, write_sync_file,
};
use fieldbook::io::state;
use fieldbook::{Outcome, World};
use fieldbook_core::asked::{self, Via as AskedVia};
use fieldbook_core::audit::{
    DestKind, Event, EventPayload, LabelId, LabelSet, OperationIntent, OperationKey, RecordRef,
    Sha256, Verb, Via, Word, WriteFile, WriteSet, request_sha256,
};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{self, Decision, Delivery, Dest, Shipped, Transfer};
use fieldbook_core::labels::{ItemRef, OWNER_SUPPLIED, StoreId};
use fieldbook_core::policy::{EffectivePolicy, Policy};
use policy_support::write_policy;
use serde_json::Value;
use support::temp_copy;
use support::world::TestWorld;

const POLICY_CONTEXT: &str = include_str!("../../../tests/fixtures/policy/context.toml");
const POLICY_BAD: &str = include_str!("../../../tests/fixtures/policy/policy-bad.toml");
const CONTEXT_JSON_GOLDEN: &str = include_str!("../../../tests/golden/json/context.json");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_mini() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
}

fn lid(s: &str) -> LabelId {
    LabelId::parse(s).unwrap()
}

fn policy_with_np_text() -> String {
    POLICY_CONTEXT.replace(
        "allow = [\"public\", \"owner-supplied\"]",
        "allow = [\"public\", \"owner-supplied\", \"np-0007\"]",
    )
}

fn run_args(world: &TestWorld, args: &[&str]) -> Outcome {
    let argv: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    fieldbook::run(&argv, world)
}

fn events_after(state_dir: &Path, prev_seq: u64) -> Vec<Event> {
    read_valid_events_prefix(state_dir)
        .into_iter()
        .filter(|e| e.seq > prev_seq)
        .collect()
}

fn last_seq(state_dir: &Path) -> u64 {
    read_valid_events_prefix(state_dir)
        .last()
        .map_or(0, |e| e.seq)
}

fn list_files_rel(root: &Path) -> BTreeSet<String> {
    fn walk(base: &Path, cur: &Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(cur) else {
            return;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

// -----------------------------------------------------------------------------
// Core CLI execution, journal ordering, JSON report (steps 6, 7, 9, 10, 16)
// -----------------------------------------------------------------------------

#[test]
fn context_prints_the_pack_after_appending_ai_pack() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let state_dir = tmp.path().join(".fieldbook");

    // Step 16: capture pre-run journal sequence, then select the single subsequent `ai.pack`
    // and assert the later `ask` event separately with `asked_via = context`.
    let before_seq = last_seq(&state_dir);
    let out = run_args(&world, &["context", "synthetic", "--scope", "area:cmp"]);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.contains("Question: synthetic\n"));
    assert!(out.stdout.contains("cite as: [@"));
    assert!(out.stdout.ends_with("Left out (policy): 0 records\n"));
    assert!(out.stderr.contains("context: "));

    let delta = events_after(&state_dir, before_seq);
    let packs: Vec<&Event> = delta
        .iter()
        .filter(|e| matches!(e.payload, EventPayload::AiPack { .. }))
        .collect();
    assert_eq!(
        packs.len(),
        1,
        "expected exactly one ai.pack event: {delta:?}"
    );

    let pack_event = packs[0];
    let EventPayload::AiPack {
        prompt_sha256,
        context_ids,
        dest,
        labels,
        policy,
    } = &pack_event.payload
    else {
        unreachable!()
    };
    assert_eq!(prompt_sha256, &Sha256::digest(out.stdout.as_bytes()));
    assert!(!context_ids.is_empty());
    assert_eq!(*dest, DestKind::Handoff);
    assert_eq!(labels, &LabelSet::owner_supplied());
    assert!(!policy.as_str().is_empty());

    let asks: Vec<&Event> = delta
        .iter()
        .filter(|e| {
            matches!(
                e.payload,
                EventPayload::Ask {
                    asked_via: AskedVia::Context,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        asks.len(),
        1,
        "expected one ask event with asked_via=context"
    );
    assert!(
        asks[0].seq > pack_event.seq,
        "ask event must follow ai.pack"
    );
}

#[test]
fn context_json_matches_the_golden() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let out = run_args(
        &world,
        &["context", "synthetic", "--scope", "area:cmp", "--json"],
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert_eq!(out.stdout, CONTEXT_JSON_GOLDEN);
}

// control: HG-03
#[test]
fn a_question_with_a_marking_the_request_does_not_cover_is_fb0700_and_appends_one_refuse_event() {
    let tmp = temp_copy(&corpus_mini());
    let policy_tmp = tempfile::tempdir().unwrap();
    let policy_path = write_policy(policy_tmp.path(), "context.toml", POLICY_CONTEXT);
    let world = TestWorld::new(tmp.path());
    let state_dir = tmp.path().join(".fieldbook");

    // 1. Mapped marking (`NP-0007 NDA` -> `np-0007`), which `context.toml` handoff row does NOT allow.
    let before_seq = last_seq(&state_dir);
    let out = run_args(
        &world,
        &[
            "--policy",
            policy_path.to_str().unwrap(),
            "context",
            "Explain NP-0007 NDA payment terms",
        ],
    );
    assert_eq!(out.code, 2, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("FB0700"), "stderr={}", out.stderr);

    let delta = events_after(&state_dir, before_seq);
    let refuses: Vec<&Event> = delta
        .iter()
        .filter(|e| matches!(e.payload, EventPayload::Refuse { .. }))
        .collect();
    assert_eq!(
        refuses.len(),
        1,
        "expected exactly one refuse event: {delta:?}"
    );
    let EventPayload::Refuse {
        code, dest, labels, ..
    } = &refuses[0].payload
    else {
        unreachable!()
    };
    assert_eq!(*code, Code::Fb0700);
    assert_eq!(*dest, Some(DestKind::Handoff));
    assert_eq!(
        labels.as_ref(),
        Some(&LabelSet::from_ids([lid("np-0007"), OWNER_SUPPLIED]))
    );
    assert!(
        !delta.iter().any(|e| matches!(
            e.payload,
            EventPayload::AiPack { .. } | EventPayload::Ask { .. }
        )),
        "refused question must write no ai.pack and no ask event"
    );
    assert!(!state_dir.join("asked/asked.csv").exists());

    // 2. Unmapped built-in banner shape -> `marked` -> FB0700.
    let before_seq2 = last_seq(&state_dir);
    let banner = fieldbook_core::scan::marking::PATTERNS[0].text;
    let q2 = format!("Question carrying {banner} synthetic probe");
    let out2 = run_args(
        &world,
        &["--policy", policy_path.to_str().unwrap(), "context", &q2],
    );
    assert_eq!(out2.code, 2);
    assert!(out2.stdout.is_empty());
    assert!(out2.stderr.contains("FB0700"), "stderr={}", out2.stderr);
    let delta2 = events_after(&state_dir, before_seq2);
    let refuses2: Vec<&Event> = delta2
        .iter()
        .filter(|e| matches!(e.payload, EventPayload::Refuse { .. }))
        .collect();
    assert_eq!(refuses2.len(), 1);
}

// control: HG-03
#[test]
fn a_marking_whose_label_the_row_allows_is_sent_with_that_label() {
    let tmp = temp_copy(&corpus_mini());
    let policy_tmp = tempfile::tempdir().unwrap();
    let policy_path = write_policy(policy_tmp.path(), "context-np.toml", &policy_with_np_text());
    let world = TestWorld::new(tmp.path());
    let state_dir = tmp.path().join(".fieldbook");

    let before_seq = last_seq(&state_dir);
    let out = run_args(
        &world,
        &[
            "--policy",
            policy_path.to_str().unwrap(),
            "context",
            "Explain NP-0007 NDA withholding rules",
        ],
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.contains("Labels: np-0007, owner-supplied\n"),
        "stdout={}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("NP-0007: restricted to the named counterparty\n"),
        "banner missing:\n{}",
        out.stdout
    );

    let delta = events_after(&state_dir, before_seq);
    let pack = delta
        .iter()
        .find(|e| matches!(e.payload, EventPayload::AiPack { .. }))
        .expect("ai.pack event");
    let EventPayload::AiPack { labels, .. } = &pack.payload else {
        unreachable!()
    };
    assert_eq!(
        labels,
        &LabelSet::from_ids([lid("np-0007"), OWNER_SUPPLIED])
    );
}

#[test]
fn a_closed_policy_is_fb0413_and_appends_one_refuse_event() {
    let tmp = temp_copy(&corpus_mini());
    let policy_tmp = tempfile::tempdir().unwrap();
    let bad_path = write_policy(policy_tmp.path(), "policy-bad.toml", POLICY_BAD);
    let world = TestWorld::new(tmp.path());
    let state_dir = tmp.path().join(".fieldbook");

    let before_seq = last_seq(&state_dir);
    let out = run_args(
        &world,
        &[
            "--policy",
            bad_path.to_str().unwrap(),
            "context",
            "synthetic",
        ],
    );
    assert_eq!(out.code, 2, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("FB0413"), "stderr={}", out.stderr);

    let delta = events_after(&state_dir, before_seq);
    let op_events: Vec<&Event> = delta
        .iter()
        .filter(|e| {
            !matches!(
                e.payload,
                EventPayload::JournalOpen { .. }
                    | EventPayload::PolicyChange { .. }
                    | EventPayload::SettingsChange { .. }
            )
        })
        .collect();
    assert_eq!(
        op_events.len(),
        1,
        "expected exactly one operation refusal event beyond init/observations: {delta:?}"
    );
    let EventPayload::Refuse { code, dest, .. } = &op_events[0].payload else {
        panic!("expected Refuse event, got {:?}", op_events[0]);
    };
    assert_eq!(*code, Code::Fb0413);
    assert_eq!(*dest, Some(DestKind::Handoff));
    assert!(!state_dir.join("asked/asked.csv").exists());
}

#[test]
fn ai_pack_records_dest_labels_and_the_policy_hash() {
    let tmp = temp_copy(&corpus_mini());
    let policy_tmp = tempfile::tempdir().unwrap();
    let policy_path = write_policy(policy_tmp.path(), "context.toml", POLICY_CONTEXT);
    let parsed_policy = Policy::parse(POLICY_CONTEXT, None).unwrap();
    let expected_hash = parsed_policy.hash().to_owned();

    let world = TestWorld::new(tmp.path());
    let state_dir = tmp.path().join(".fieldbook");

    let before_seq = last_seq(&state_dir);
    let out = run_args(
        &world,
        &[
            "--policy",
            policy_path.to_str().unwrap(),
            "context",
            "synthetic",
        ],
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);

    let delta = events_after(&state_dir, before_seq);
    let pack = delta
        .iter()
        .find(|e| matches!(e.payload, EventPayload::AiPack { .. }))
        .expect("ai.pack event");
    let EventPayload::AiPack {
        dest,
        labels,
        policy,
        ..
    } = &pack.payload
    else {
        unreachable!()
    };
    assert_eq!(*dest, DestKind::Handoff);
    assert_eq!(labels, &LabelSet::owner_supplied());
    assert_eq!(policy.as_str(), expected_hash);
}

#[test]
fn pack_event_labels_are_the_union_of_the_included_items() {
    let tmp = temp_copy(&corpus_mini());
    // Plant an in-tree `np-0007` page alongside public pages in the temporary copy.
    let np_page = "+++\nid = \"cmp-np-union\"\ntitle = \"Synthetic np0007 union probe\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic np0007 union probe page.\n\n## The rule\n\nSynthetic rule [@CL-CMP-0001].\n\n## How it works\n\nSynthetic.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\n## Never confuse\n\nSynthetic.\n\n## Company calibration\n\nSynthetic.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n";
    std::fs::write(
        tmp.path()
            .join("corpus/pages/compliance/business-systems/cmp-np-union.md"),
        np_page,
    )
    .unwrap();

    let policy_tmp = tempfile::tempdir().unwrap();
    let policy_path = write_policy(policy_tmp.path(), "context-np.toml", &policy_with_np_text());
    let world = TestWorld::new(tmp.path());
    let state_dir = tmp.path().join(".fieldbook");

    let before_seq = last_seq(&state_dir);
    let out = run_args(
        &world,
        &[
            "--policy",
            policy_path.to_str().unwrap(),
            "context",
            "np0007 union probe",
            "--json",
        ],
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);

    let report: Value = serde_json::from_str(&out.stdout).expect("parse json report");
    let report_labels: Vec<String> = report["pack"]["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(report_labels, vec!["np-0007", "owner-supplied"]);

    let delta = events_after(&state_dir, before_seq);
    let pack = delta
        .iter()
        .find(|e| matches!(e.payload, EventPayload::AiPack { .. }))
        .expect("ai.pack event");
    let EventPayload::AiPack { labels, .. } = &pack.payload else {
        unreachable!()
    };
    let expected = LabelSet::from_ids([lid("np-0007"), OWNER_SUPPLIED]);
    assert_eq!(labels, &expected);
}

#[test]
fn a_journal_that_refuses_appends_prints_no_pack() {
    let tmp = temp_copy(&corpus_mini());
    let state_dir = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state_dir).unwrap();
    // Place a regular file where the `audit/` directory must live so journal open/append fails.
    std::fs::write(state_dir.join("audit"), b"blocker\n").unwrap();

    let world = TestWorld::with_state(tmp.path(), &state_dir);
    let out = run_args(&world, &["context", "synthetic"]);
    assert_eq!(out.code, 3, "stderr={}", out.stderr);
    assert!(
        out.stdout.is_empty(),
        "no pack may be printed when journal append fails:\n{}",
        out.stdout
    );
    assert!(out.stderr.contains("FB0903"), "stderr={}", out.stderr);
}

// -----------------------------------------------------------------------------
// Usage errors & budget override (steps 6 & 10)
// -----------------------------------------------------------------------------

#[test]
fn an_empty_question_is_a_usage_error() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    for q in ["", "   "] {
        let out = run_args(&world, &["context", q]);
        assert_eq!(out.code, 2, "q={q:?}");
        assert!(out.stdout.is_empty());
        assert!(!tmp.path().join(".fieldbook").exists());
    }
}

#[test]
fn an_unknown_scope_is_a_usage_error() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    for bad in ["area:zzz", "page:no-such-page", "bogus:cmp", "area:"] {
        let out = run_args(&world, &["context", "synthetic", "--scope", bad]);
        assert_eq!(out.code, 2, "scope={bad}");
        assert!(out.stdout.is_empty());
        assert!(!tmp.path().join(".fieldbook").exists());
    }
}

#[test]
fn an_unknown_include_word_is_a_usage_error() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    for bad in ["unknown", "tier-d,bogus", ""] {
        let out = run_args(&world, &["context", "synthetic", "--include", bad]);
        assert_eq!(out.code, 2, "include={bad}");
        assert!(out.stdout.is_empty());
        assert!(!tmp.path().join(".fieldbook").exists());
    }
}

#[test]
fn a_zero_or_non_numeric_budget_is_a_usage_error() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    for bad in ["0", "abc", "-5"] {
        let out = run_args(&world, &["context", "synthetic", "--budget", bad]);
        assert_eq!(out.code, 2, "budget={bad} stderr={}", out.stderr);
        assert!(out.stdout.is_empty());
        assert!(
            !tmp.path().join(".fieldbook").exists(),
            "bad --budget `{bad}` must write nothing"
        );
    }
}

#[test]
fn budget_flag_overrides_corpus_toml() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let full = run_args(&world, &["context", "synthetic", "--json"]);
    assert_eq!(full.code, 0, "stderr={}", full.stderr);
    let full_json: Value = serde_json::from_str(&full.stdout).unwrap();
    assert_eq!(full_json["pack"]["budget"], 12_000);

    let small = run_args(
        &world,
        &["context", "synthetic", "--budget", "900", "--json"],
    );
    assert_eq!(small.code, 0, "stderr={}", small.stderr);
    let small_json: Value = serde_json::from_str(&small.stdout).unwrap();
    assert_eq!(small_json["pack"]["budget"], 900);
    assert!(small_json["pack"]["chars"].as_u64().unwrap() <= 900);
    assert!(
        small_json["pack"]["chars"].as_u64().unwrap()
            < full_json["pack"]["chars"].as_u64().unwrap()
    );
}

#[test]
fn context_writes_only_the_journal_and_the_question_log() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let state_dir = tmp.path().join(".fieldbook");

    let before_seq = last_seq(&state_dir);
    let out = run_args(
        &world,
        &["context", "synthetic", "--scope", "area:cmp", "--json"],
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);

    let files = list_files_rel(&state_dir);
    for rel in &files {
        assert!(
            rel.starts_with("audit/")
                || rel == "asked/asked.csv"
                || rel == "lock"
                || rel == "store.lock"
                || rel == "store.uid",
            "unexpected file written in state folder: {rel} (all={files:?})"
        );
    }
    assert!(files.contains("asked/asked.csv"));
    assert!(files.iter().any(|f| f.starts_with("audit/journal")));

    // Row's labels equal the ai.pack labels and its inputs equal the included store-qualified items.
    let delta = events_after(&state_dir, before_seq);
    let pack_event = delta
        .iter()
        .find(|e| matches!(e.payload, EventPayload::AiPack { .. }))
        .expect("ai.pack");
    let EventPayload::AiPack {
        labels: pack_labels,
        ..
    } = &pack_event.payload
    else {
        unreachable!()
    };

    let report: Value = serde_json::from_str(&out.stdout).unwrap();
    let mut expected_inputs: Vec<ItemRef> = report["pack"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["included"].as_bool().unwrap())
        .map(|i| ItemRef::new(StoreId::Corpus, i["unit"].as_str().unwrap().to_owned()))
        .collect();
    expected_inputs.sort();
    expected_inputs.dedup();

    let asked_bytes = std::fs::read(state_dir.join("asked/asked.csv")).unwrap();
    let rows = asked::parse(&asked_bytes).expect("parse asked.csv");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].via, AskedVia::Context);
    assert_eq!(&rows[0].labels, pack_labels);
    assert_eq!(rows[0].inputs, expected_inputs);
}

// -----------------------------------------------------------------------------
// Delivery binding check (step 13)
// -----------------------------------------------------------------------------

#[test]
fn deliver_pack_refuses_selection_token_changed_bytes_and_changed_target() {
    let eff = EffectivePolicy::floor(false);
    let now = TestWorld::new(&corpus_mini()).now();

    // 1. Selection-only token is refused by deliver_pack.
    let Decision::Allowed(view_token) =
        egress::check(&eff, &Dest::Handoff, None, None, now, Shipped::Never)
    else {
        panic!("floor handoff selection allowed");
    };
    let payload = b"Synthetic pack text\n";
    assert_eq!(
        deliver_pack(&view_token, HANDOFF_REPLY_TARGET, payload),
        Err(Code::Fb0902)
    );

    // 2. Delivery-bound token succeeds for exact bytes + target, fails on changed bytes or changed target.
    let delivery = Delivery::new(vec![Transfer::reply(HANDOFF_REPLY_TARGET, payload)], None);
    let Decision::Allowed(bound) = egress::check(
        &eff,
        &Dest::Handoff,
        Some(&LabelSet::owner_supplied()),
        Some(&delivery),
        now,
        Shipped::Never,
    ) else {
        panic!("bound handoff token allowed");
    };
    assert_eq!(
        deliver_pack(&bound, HANDOFF_REPLY_TARGET, payload).unwrap(),
        "Synthetic pack text\n"
    );
    assert_eq!(
        deliver_pack(&bound, HANDOFF_REPLY_TARGET, b"Synthetic pack text!\n"),
        Err(Code::Fb0902)
    );
    assert_eq!(
        deliver_pack(&bound, "handoff:other-target", payload),
        Err(Code::Fb0902)
    );
}

// -----------------------------------------------------------------------------
// Coherent generation & crashed pending transaction refusal (step 15)
// -----------------------------------------------------------------------------

const NEW_CLAIM: &str = "CL-CMP-0099";
const NEW_QUESTION: &str = "Q-099";

struct TwoFileBatch {
    world: TestWorld,
    state_dir: PathBuf,
    intent: OperationIntent,
    ws: WriteSet,
    claims: (PathBuf, Vec<u8>),
    questions: (PathBuf, Vec<u8>),
}

fn two_file_batch(root: &Path) -> TwoFileBatch {
    let claims_rel = "corpus/registers/claims.csv";
    let questions_rel = "corpus/registers/questions.csv";
    let claims_path = root.join(claims_rel);
    let questions_path = root.join(questions_rel);
    let claims_before = std::fs::read(&claims_path).expect("read claims.csv");
    let questions_before = std::fs::read(&questions_path).expect("read questions.csv");

    let mut claims_after = String::from_utf8(claims_before.clone()).expect("utf8 claims");
    claims_after.push_str(
        "CL-CMP-0099,cmp,fact,Synthetic coherent generation probe alpha.,2026-02-01,S-0001,1,text_section,sec. 1,Synthetic probe alpha.,false,codex,unchecked,pending_company_confirmation,proposed,observed,V2,,,,,,,,,,,,,,,,,,Synthetic batch probe.\n",
    );
    let mut questions_after = String::from_utf8(questions_before.clone()).expect("utf8 questions");
    questions_after.push_str(
        "Q-099,cmp,Which synthetic coherent generation probe alpha applies?,,,,,,false,open,,,\n",
    );

    let claims_after_bytes = claims_after.into_bytes();
    let questions_after_bytes = questions_after.into_bytes();
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![
            WriteFile::new(
                claims_rel,
                Sha256::digest(&claims_after_bytes).as_str().to_owned(),
            ),
            WriteFile::new(
                questions_rel,
                Sha256::digest(&questions_after_bytes).as_str().to_owned(),
            ),
        ],
        affected: vec![
            RecordRef::parse(&format!("corpus/{NEW_CLAIM}")).expect("ref"),
            RecordRef::parse(&format!("corpus/{NEW_QUESTION}")).expect("ref"),
        ],
    };
    let state_dir = root.join(".fieldbook");
    let world = TestWorld::with_state(root, &state_dir);
    let intent = OperationIntent {
        schema: 1,
        key: OperationKey::new(
            Verb::parse("add").expect("verb"),
            Sha256::parse(&"61".repeat(32)).expect("sha"),
        ),
        request_sha256: request_sha256(&[("req", &[0x61])]).expect("request sha"),
        allocated: vec![RecordRef::parse(&format!("corpus/{NEW_CLAIM}")).expect("ref")],
        allocated_store_uid: None,
        planned_on: world.today(),
    };
    TwoFileBatch {
        world,
        state_dir,
        intent,
        ws,
        claims: (claims_path, claims_after_bytes),
        questions: (questions_path, questions_after_bytes),
    }
}

fn spawn_context(root: &Path, query: &str, timeout: Duration) -> mpsc::Receiver<Outcome> {
    let (tx, rx) = mpsc::channel();
    let root = root.to_path_buf();
    let query = query.to_owned();
    std::thread::spawn(move || {
        set_lock_timeout_for_test(Some(timeout));
        let world = TestWorld::new(&root);
        let out = run_args(&world, &["context", &query]);
        set_lock_timeout_for_test(None);
        let _ = tx.send(out);
    });
    rx
}

#[test]
fn context_waits_for_a_complete_generation() {
    // 1. Pause a two-file writer between files: a waiting `context` blocks until completion and
    //    sees both files, while a short-timeout `context` refuses with no mixed content.
    let tmp = temp_copy(&corpus_mini());
    let b = two_file_batch(tmp.path());
    let mut journal = state::open(&b.world, &b.state_dir, Via::Cli).expect("open journal");
    let lease = b.world.write_lease().expect("write lease");
    let token = journal.begin(&lease, &b.intent, &b.ws).expect("begin");
    write_sync_file(&b.claims.0, &b.claims.1).expect("write file 1");

    let waiting = spawn_context(
        tmp.path(),
        "coherent generation probe alpha",
        Duration::from_secs(20),
    );
    assert!(
        waiting.recv_timeout(Duration::from_millis(500)).is_err(),
        "context returned while the writer held the store between its two files"
    );

    let refused = spawn_context(
        tmp.path(),
        "coherent generation probe alpha",
        Duration::from_millis(80),
    )
    .recv_timeout(Duration::from_secs(15))
    .expect("short-timeout context finishes");
    assert_eq!(refused.code, 3, "stdout={}", refused.stdout);
    assert!(refused.stdout.is_empty());
    assert!(!refused.stderr.is_empty());

    write_sync_file(&b.questions.0, &b.questions.1).expect("write file 2");
    journal.end(token, Word::new_static("ok")).expect("end");
    drop(lease);

    let out = waiting
        .recv_timeout(Duration::from_secs(30))
        .expect("waiting context finishes after writer commits");
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.contains(NEW_CLAIM) && out.stdout.contains(NEW_QUESTION),
        "context must see both files of the complete generation:\n{}",
        out.stdout
    );

    // 2. A crashed pending transaction is refused until recovery.
    let tmp2 = temp_copy(&corpus_mini());
    let b2 = two_file_batch(tmp2.path());
    let mut j2 = state::open(&b2.world, &b2.state_dir, Via::Cli).expect("open journal 2");
    let lease2 = b2.world.write_lease().expect("write lease 2");
    let token2 = j2.begin(&lease2, &b2.intent, &b2.ws).expect("begin 2");
    write_sync_file(&b2.claims.0, &b2.claims.1).expect("write file 1 (crash)");
    drop(token2);
    drop(lease2);

    let crashed = run_args(&b2.world, &["context", "coherent generation probe alpha"]);
    assert_eq!(crashed.code, 3, "stdout={}", crashed.stdout);
    assert!(crashed.stdout.is_empty());
    assert!(
        crashed.stderr.contains("pending"),
        "stderr={}",
        crashed.stderr
    );

    // Recover and complete the batch; context then succeeds and sees both files.
    let lease3 = b2.world.write_lease().expect("lease for recovery");
    let recovery = j2
        .recover(
            &lease3,
            &b2.intent.key,
            b2.intent.request_sha256.clone(),
            &b2.ws,
        )
        .expect("recover");
    let Recovery::Resume { audited, .. } = recovery else {
        panic!("expected resumable pending transaction");
    };
    write_sync_file(&b2.questions.0, &b2.questions.1).expect("write file 2 (recovery)");
    j2.end(audited, Word::new_static("ok"))
        .expect("end recovery");
    drop(lease3);

    let recovered = run_args(&b2.world, &["context", "coherent generation probe alpha"]);
    assert_eq!(recovered.code, 0, "stderr={}", recovered.stderr);
    assert!(recovered.stdout.contains(NEW_CLAIM) && recovered.stdout.contains(NEW_QUESTION));
}

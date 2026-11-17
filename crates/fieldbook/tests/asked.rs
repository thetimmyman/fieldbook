//! Tests of the question log (`ask-log`): `search` records each question in
//! `asked/asked.csv` and the journal, and recording never stops a search.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "integration tests copy fixtures into temporary folders and assert on their files"
)]

mod support;

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::path::{Path, PathBuf};
use std::time::Instant;

use fieldbook::Outcome;
use fieldbook::World;
use fieldbook::asked::{Cited, Hits, Logged, log_question};
use fieldbook::io::asked as file;
use fieldbook::io::journal::{Journal, read_valid_events_prefix};
use fieldbook::io::state::{self, AskedMode, AskedSettings, read_asked_settings};
use fieldbook_core::asked::{
    self, AskedId, AskedRow, Outcome as AskedOutcome, RowState, Via as AskedVia,
};
use fieldbook_core::audit::{
    Count, Event, EventPayload, Id, LabelId, LabelSet, Now, Principal, SettingsGroup, Sha256, Via,
    Word, parse_settings_role_and_baseline,
};
use fieldbook_core::labels::{ItemRef, StoreId};
use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};
use rng::Rng;
use support::state::build_state;
use support::world::TestWorld;

// --- Helpers --------------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_mini() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
}

fn state_asked() -> PathBuf {
    repo_root().join("tests/fixtures/state-asked")
}

/// A temporary copy of `corpus-mini`, outside Git, with no state folder yet.
fn corpus() -> tempfile::TempDir {
    support::temp_copy(&corpus_mini())
}

fn run(world: &TestWorld, argv: &[&str]) -> Outcome {
    let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_owned()).collect();
    fieldbook::run(&argv, world)
}

fn search(world: &TestWorld, words: &[&str]) -> Outcome {
    let mut argv = vec!["search"];
    argv.extend_from_slice(words);
    run(world, &argv)
}

/// `search` in the repository's own fixture: refused before any state, so it prints exactly what
/// an unlogged search prints.
fn unlogged(words: &[&str]) -> Outcome {
    search(&TestWorld::new(&corpus_mini()), words)
}

fn rows(world: &TestWorld) -> Vec<AskedRow> {
    file::read_rows(world.state_dir())
        .expect("asked.csv parses")
        .unwrap_or_default()
}

fn log_bytes(world: &TestWorld) -> Option<Vec<u8>> {
    std::fs::read(file::path(world.state_dir())).ok()
}

fn events(world: &TestWorld) -> Vec<Event> {
    read_valid_events_prefix(world.state_dir())
}

fn ask_events(world: &TestWorld) -> Vec<Event> {
    events(world)
        .into_iter()
        .filter(|event| matches!(event.payload, EventPayload::Ask { .. }))
        .collect()
}

fn journal_text(world: &TestWorld) -> String {
    std::fs::read_to_string(world.state_dir().join("audit/journal.jsonl")).unwrap_or_default()
}

fn ask_lines(world: &TestWorld) -> Vec<String> {
    journal_text(world)
        .lines()
        .filter(|line| line.contains("\"kind\":\"ask\""))
        .map(str::to_owned)
        .collect()
}

fn now_at(text: &str) -> Now {
    Now::parse(text).expect("valid timestamp")
}

/// Moves the world's clock `seconds` forward.
fn advance(world: &TestWorld, seconds: u64) {
    let now = world.now();
    let days = u64::try_from(now.date.to_epoch_days()).expect("after the epoch");
    let epoch = days * 86_400 + u64::from(now.seconds);
    world.set_now(Now::from_unix_secs(epoch + seconds).expect("valid time"));
}

fn write_settings(world: &TestWorld, text: &str) {
    std::fs::create_dir_all(world.state_dir()).expect("state folder");
    std::fs::write(world.state_dir().join("settings.toml"), text).expect("write settings");
}

fn open(world: &TestWorld) -> Journal<'_> {
    state::open(world, world.state_dir(), Via::Cli).expect("journal opens")
}

fn corpus_ref(id: &str) -> ItemRef {
    ItemRef::new(StoreId::Corpus, id)
}

fn hits(count: u64, top_ids: &[&str]) -> Hits {
    Hits {
        count,
        top_ids: top_ids.iter().map(|id| (*id).to_owned()).collect(),
        fingerprint: "ab".repeat(32),
    }
}

/// Calls the logger directly with floor A, `owner-supplied` and `search`.
fn log_direct(
    world: &TestWorld,
    journal: &Journal<'_>,
    query: &str,
    inputs: &[ItemRef],
    cited: Cited,
) -> Logged {
    log_question(
        world,
        journal,
        &AskedSettings::default(),
        &EffectivePolicy::floor(false),
        AskedVia::Search,
        &LabelSet::owner_supplied(),
        inputs,
        query,
        &hits(inputs.len() as u64, &[]),
        cited,
    )
}

fn np_0007() -> LabelSet {
    LabelSet::from_ids([LabelId::parse("np-0007").expect("valid label id")])
}

/// A synthetic policy whose State store holds `np-0007`.
fn np_policy() -> EffectivePolicy {
    let text = r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Synthetic supplier terms"
banner = "NP-0007"
restricted = true

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
default = ["np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'
"#;
    let policy = Policy::parse(text, None).expect("parse policy");
    EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset())
}

fn stderr_lines(out: &Outcome) -> Vec<&str> {
    out.stderr.lines().collect()
}

fn assert_one_warning(out: &Outcome) {
    let lines = stderr_lines(out);
    assert_eq!(lines.len(), 1, "one warning line: {:?}", out.stderr);
    assert!(
        lines[0].starts_with("question not recorded: "),
        "{:?}",
        out.stderr
    );
}

#[cfg(unix)]
fn set_read_only(path: &Path, read_only: bool) {
    use std::os::unix::fs::PermissionsExt;
    let mode = match (path.is_dir(), read_only) {
        (true, true) => 0o555,
        (true, false) => 0o755,
        (false, true) => 0o444,
        (false, false) => 0o644,
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

#[cfg(not(unix))]
fn set_read_only(path: &Path, read_only: bool) {
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_readonly(read_only);
    std::fs::set_permissions(path, perms).expect("set permissions");
}

// --- The state-asked fixture ----------------------------------------------------------------

const STATE_ASKED_SETTINGS: &str = "role = \"lead\"\n\n[asked]\nmode = \"text\"\nkeep_days = 365\n";

#[expect(
    clippy::too_many_arguments,
    reason = "one argument per column a fixture row sets"
)]
fn fixture_row(
    n: u16,
    at: &str,
    via: AskedVia,
    question: &str,
    labels: LabelSet,
    inputs: Vec<ItemRef>,
    outcome: AskedOutcome,
    state: RowState,
) -> AskedRow {
    let top_ids: Vec<String> = inputs.iter().map(|item| item.id.clone()).collect();
    AskedRow {
        id: AskedId::new(n).expect("valid id"),
        at: now_at(at),
        via,
        question: question.to_owned(),
        labels,
        inputs,
        by: Principal::local(),
        tool_version: "0.0.0".to_owned(),
        fingerprint: "5a".repeat(32),
        result_count: top_ids.len() as u64,
        top_ids,
        outcome,
        state,
    }
}

/// The six synthetic rows of `tests/fixtures/state-asked/asked/asked.csv`: every outcome and
/// state, one row with `np-0007|owner-supplied` and one older than 365 days on 2026-03-01.
fn state_asked_rows() -> Vec<AskedRow> {
    let owner = LabelSet::owner_supplied();
    vec![
        fixture_row(
            1,
            "2025-01-15T09:00:00Z",
            AskedVia::Search,
            "synthetic retention schedule for drafts",
            owner.clone(),
            Vec::new(),
            AskedOutcome::NoResult,
            RowState::Promoted,
        ),
        fixture_row(
            2,
            "2026-02-10T10:15:00Z",
            AskedVia::Search,
            "synthetic system review",
            owner.clone(),
            vec![corpus_ref("cmp-system-review")],
            AskedOutcome::Answered,
            RowState::Open,
        ),
        fixture_row(
            3,
            "2026-02-12T11:30:00Z",
            AskedVia::Ask,
            "which synthetic cost rule applies to the supplier terms?",
            owner.union(&np_0007()),
            vec![
                corpus_ref("S-0001-v1"),
                ItemRef::new(StoreId::State, "P-0001"),
            ],
            AskedOutcome::CiteFailed,
            RowState::Open,
        ),
        fixture_row(
            4,
            "2026-02-14T14:00:00Z",
            AskedVia::Context,
            "synthetic rate window, \"quoted\"",
            owner.clone(),
            vec![corpus_ref("est-rate-window-brief")],
            AskedOutcome::Flagged,
            RowState::Promoted,
        ),
        fixture_row(
            5,
            "2026-02-20T08:45:00Z",
            AskedVia::Chat,
            "synthetic onboarding checklist",
            owner.clone(),
            vec![corpus_ref("cmp-onboarding-checklist")],
            AskedOutcome::Answered,
            RowState::Dismissed,
        ),
        fixture_row(
            6,
            "2026-02-27T16:20:00Z",
            AskedVia::Search,
            "synthetic withhold case",
            owner,
            vec![corpus_ref("cmp-withhold-case")],
            AskedOutcome::Answered,
            RowState::Open,
        ),
    ]
}

/// The journal events of the fixture: the `asked` settings change, then one `ask` event per row.
fn state_asked_events(rows: &[AskedRow]) -> Vec<EventPayload> {
    let (_, baseline) = parse_settings_role_and_baseline(Some(STATE_ASKED_SETTINGS.as_bytes()))
        .expect("valid fixture settings");
    let mut events = vec![EventPayload::SettingsChange {
        names: vec![SettingsGroup::Asked],
        baseline,
    }];
    events.extend(rows.iter().map(|row| EventPayload::Ask {
        asked: Some(Id::parse(&row.id.to_string()).expect("valid id")),
        asked_via: row.via,
        question_sha256: Sha256::digest(row.question.as_bytes()),
        result_count: Count::new(row.result_count),
        outcome: Word::new_static(row.outcome.as_str()),
        labels: row.labels.clone(),
    }));
    events
}

/// Writes the fixture's journal and question log into `dir` the way the tools write them.
fn build_state_asked(dir: &Path) {
    let rows = state_asked_rows();
    build_state(dir, &state_asked_events(&rows));
    std::fs::create_dir_all(dir.join("asked")).expect("asked folder");
    std::fs::write(dir.join(file::REL_PATH), asked::render(&rows)).expect("write asked.csv");
    std::fs::write(dir.join("settings.toml"), STATE_ASKED_SETTINGS).expect("write settings");
}

#[test]
fn state_asked_matches_the_writer() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let rebuilt = tmp.path().join("state-asked");
    build_state_asked(&rebuilt);
    let mut differ = Vec::new();
    for rel in ["audit/journal.jsonl", file::REL_PATH, "settings.toml"] {
        let want = std::fs::read(state_asked().join(rel)).unwrap_or_default();
        let got = std::fs::read(rebuilt.join(rel)).expect("rebuilt file");
        if want != got {
            differ.push(rel);
        }
    }
    if !differ.is_empty() {
        let keep = repo_root().join("target/state-rebuilt/state-asked");
        let _ = std::fs::remove_dir_all(&keep);
        support::copy_dir_recursive(&rebuilt, &keep).expect("keep the rebuilt fixture");
        panic!(
            "{differ:?} differ from the writer; the rebuilt folder is in {}",
            keep.display()
        );
    }
}

#[test]
fn state_asked_settings_parse() {
    let settings = read_asked_settings(&state_asked()).expect("fixture settings parse");
    assert_eq!(settings.mode, AskedMode::Text);
    assert_eq!(settings.keep_days, 365);
    let rows = file::read_rows(&state_asked())
        .expect("fixture log parses")
        .expect("fixture log exists");
    assert_eq!(rows, state_asked_rows());
}

#[test]
fn status_prints_the_question_line() {
    let tmp = corpus();
    support::copy_dir_recursive(&state_asked(), &tmp.path().join(".fieldbook"))
        .expect("copy state-asked");
    let world = TestWorld::new(tmp.path());
    let out = run(&world, &["status"]);
    assert!(
        out.stdout
            .lines()
            .any(|line| line.trim() == "6 questions asked, 3 misses, 2 promoted"),
        "{}{}",
        out.stdout,
        out.stderr
    );
}

// --- search records each question ---------------------------------------------------------

#[test]
fn search_logs_a_miss_and_a_hit() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());

    let miss = search(&world, &["zzzznotaword"]);
    let plain_miss = unlogged(&["zzzznotaword"]);
    assert_eq!(miss.code, 1);
    assert_eq!(miss.code, plain_miss.code);
    assert_eq!(miss.stdout, plain_miss.stdout);
    assert!(miss.stderr.is_empty(), "{}", miss.stderr);

    advance(&world, 120);
    let hit = search(&world, &["synthetic"]);
    let plain_hit = unlogged(&["synthetic"]);
    assert_eq!(hit.code, 0);
    assert_eq!(hit.stdout, plain_hit.stdout);
    assert!(hit.stderr.is_empty(), "{}", hit.stderr);

    let rows = rows(&world);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].question, "zzzznotaword");
    assert_eq!(rows[0].outcome, AskedOutcome::NoResult);
    assert_eq!(rows[0].result_count, 0);
    assert!(rows[0].top_ids.is_empty());
    assert!(rows[0].inputs.is_empty());

    assert_eq!(rows[1].outcome, AskedOutcome::Answered);
    assert!(rows[1].result_count > 0);
    assert!(!rows[1].top_ids.is_empty() && rows[1].top_ids.len() <= 5);
    assert!(
        hit.stdout.contains(&format!("1. {} ", rows[1].top_ids[0])),
        "the first top id is the first result printed"
    );
    let mut expected: Vec<ItemRef> = rows[1].top_ids.iter().map(|id| corpus_ref(id)).collect();
    expected.sort();
    assert_eq!(rows[1].inputs, expected);
    assert_eq!(rows[0].via, AskedVia::Search);

    // The event names the door in `asked_via` and the channel in the envelope.
    let asks = ask_events(&world);
    assert_eq!(asks.len(), 2);
    for event in &asks {
        assert_eq!(event.envelope.via, Via::Cli);
        let EventPayload::Ask { asked_via, .. } = &event.payload else {
            panic!("an ask event");
        };
        assert_eq!(*asked_via, AskedVia::Search);
    }
}

#[test]
fn bare_id_is_not_logged() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    for id in [
        "S-0001",
        "S-0001-v1",
        "CL-CMP-0001",
        "cmp-system-review",
        "A-0001",
    ] {
        let _ = search(&world, &[id]);
    }
    assert!(
        !world.state_dir().exists(),
        "a lookup opens no state folder"
    );
}

#[test]
fn repeat_is_one_row() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    advance(&world, 30);
    let again = search(&world, &["synthetic"]);
    assert!(again.stderr.is_empty(), "{}", again.stderr);
    assert_eq!(rows(&world).len(), 1);
    assert_eq!(ask_events(&world).len(), 1);

    advance(&world, 61);
    let _ = search(&world, &["synthetic"]);
    assert_eq!(rows(&world).len(), 2, "a minute later it counts again");
}

#[test]
fn repeat_with_different_inputs_or_labels_writes_a_row() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let journal = open(&world);
    let policy = np_policy();
    let settings = AskedSettings::default();
    let call = |labels: &LabelSet, inputs: &[ItemRef]| {
        log_question(
            &world,
            &journal,
            &settings,
            &policy,
            AskedVia::Ask,
            labels,
            inputs,
            "synthetic question",
            &hits(1, &["S-0001-v1"]),
            Cited::NotChecked,
        )
    };
    let owner = LabelSet::owner_supplied();
    assert!(call(&owner, &[corpus_ref("S-0001-v1")]).id.is_some());
    assert!(
        call(&owner, &[corpus_ref("S-0001-v1")]).id.is_none(),
        "a repeat"
    );
    assert!(
        call(&owner, &[corpus_ref("S-0002-v1")]).id.is_some(),
        "other inputs"
    );
    assert!(
        call(&owner, &[ItemRef::new(StoreId::State, "S-0002-v1")])
            .id
            .is_some(),
        "the same id in another store is another input"
    );
    let with_pack = owner.union(&np_0007());
    let logged = call(&with_pack, &[corpus_ref("S-0002-v1")]);
    assert!(logged.id.is_some(), "other labels: {:?}", logged.warning);
    assert_eq!(rows(&world).len(), 4);
}

#[test]
fn row_holds_version_and_fingerprint() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    let row = rows(&world).pop().expect("one row");
    assert_eq!(row.tool_version, env!("CARGO_PKG_VERSION"));

    // The fingerprint an edition built from the same Corpus records in `edition.toml`.
    let out = tempfile::tempdir().expect("out tempdir");
    let out_arg = out.path().to_string_lossy().into_owned();
    let built = run(&world, &["build", "--out", &out_arg]);
    assert_eq!(built.code, 0, "{}", built.stderr);
    let edition = fieldbook_core::edition::EditionInfo::parse(
        &std::fs::read_to_string(out.path().join("edition/edition.toml")).expect("edition.toml"),
    )
    .expect("edition.toml parses");
    assert_eq!(row.fingerprint, edition.fingerprint);
    let tree = world.read_tree().expect("read tree");
    assert_eq!(
        Ok(row.fingerprint.clone()),
        fieldbook::asked::fingerprint(&tree)
    );
}

#[test]
fn fingerprint_ignores_support_files() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    // A support file is not a search input: editing it leaves the recorded identity unchanged.
    let agents = tmp.path().join("corpus/AGENTS.md");
    let mut text = std::fs::read_to_string(&agents).expect("read AGENTS.md");
    text.push_str("\nOne more synthetic line.\n");
    std::fs::write(&agents, text).expect("write AGENTS.md");
    let _ = search(&world, &["cost"]);
    let logged = rows(&world);
    assert_eq!(logged.len(), 2, "{logged:?}");
    assert_eq!(logged[0].fingerprint, logged[1].fingerprint);
}

#[test]
fn row_records_labels_and_principal() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    let row = rows(&world).pop().expect("one row");
    assert_eq!(row.labels, LabelSet::owner_supplied());
    assert_eq!(row.labels.to_cell(), "owner-supplied");
    assert_eq!(row.by, Principal::local());
    assert_eq!(row.by.to_string(), "local");
}

#[test]
fn hash_mode_keeps_no_text() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    write_settings(&world, "[asked]\nmode = \"hash\"\n");
    let out = search(&world, &["synthetic", "review"]);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    let row = rows(&world).pop().expect("one row");
    assert_eq!(row.question, asked::hashed("synthetic review"));
    let text = String::from_utf8(log_bytes(&world).expect("log")).expect("utf-8");
    assert!(!text.contains("synthetic review"));
}

#[test]
fn off_mode_keeps_no_row_and_keeps_the_event() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    write_settings(&world, "[asked]\nmode = \"off\"\n");
    let out = search(&world, &["synthetic"]);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    assert!(log_bytes(&world).is_none(), "no row in mode off");

    let asks = ask_events(&world);
    assert_eq!(asks.len(), 1, "the event passes the journal validator");
    let EventPayload::Ask { asked, .. } = &asks[0].payload else {
        panic!("an ask event");
    };
    assert!(asked.is_none());
    let lines = ask_lines(&world);
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].contains("\"asked\":"), "{}", lines[0]);

    let verify = run(&world, &["audit", "verify"]);
    assert_eq!(verify.code, 0, "{}{}", verify.stdout, verify.stderr);

    // A repeat of the same question is not suppressed in mode off.
    let _ = search(&world, &["synthetic"]);
    assert_eq!(ask_events(&world).len(), 2);
}

#[test]
fn expiry_writes_a_purge_event() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    write_settings(&world, "[asked]\nkeep_days = 30\n");
    let _ = search(&world, &["synthetic"]);
    advance(&world, 31 * 86_400);
    let _ = search(&world, &["review"]);

    let rows = rows(&world);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].question, "review");

    let all = events(&world);
    let purge = all
        .iter()
        .position(|event| {
            matches!(&event.payload, EventPayload::Purge { what, count }
                if what.as_str() == "asked" && count.value() == 1)
        })
        .expect("one purge event");
    let last_ask = all
        .iter()
        .rposition(|event| matches!(event.payload, EventPayload::Ask { .. }))
        .expect("an ask event");
    assert!(purge < last_ask, "the purge comes before the new ask");
}

#[test]
fn event_holds_only_the_hash() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let question = "quokka marmalade xylophone";
    let _ = search(&world, &[question]);
    let lines = ask_lines(&world);
    assert_eq!(lines.len(), 1);
    let text = journal_text(&world);
    for word in question.split(' ') {
        assert!(!text.contains(word), "the journal holds `{word}`");
    }
    assert!(lines[0].contains(&format!(
        "\"question_sha256\":\"{}\"",
        fieldbook_core::hash::sha256_hex(question.as_bytes())
    )));
}

#[test]
fn event_matches_the_row() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    let row = rows(&world).pop().expect("one row");
    let event = ask_events(&world).pop().expect("one event");
    assert_eq!(event.envelope.via, Via::Cli);
    let EventPayload::Ask {
        asked,
        asked_via,
        question_sha256,
        result_count,
        outcome,
        labels,
    } = event.payload
    else {
        panic!("an ask event");
    };
    assert_eq!(
        asked.map(|id| id.as_str().to_owned()),
        Some(row.id.to_string())
    );
    assert_eq!(asked_via, row.via);
    assert_eq!(question_sha256, Sha256::digest(row.question.as_bytes()));
    assert_eq!(result_count.value(), row.result_count);
    assert_eq!(outcome.as_str(), row.outcome.as_str());
    assert_eq!(labels, row.labels.union(&LabelSet::public()));
    assert_eq!(event.time, row.at);
}

#[test]
fn broken_verdict_writes_cite_failed() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let journal = open(&world);
    let logged = log_direct(
        &world,
        &journal,
        "synthetic cost rule",
        &[corpus_ref("S-0001-v1")],
        Cited::Broken,
    );
    assert!(logged.warning.is_none(), "{:?}", logged.warning);
    let row = rows(&world).pop().expect("one row");
    assert_eq!(row.outcome, AskedOutcome::CiteFailed);
}

#[test]
fn set_outcome_rewrites_one_row_and_appends_no_event() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let journal = open(&world);
    let first = log_direct(
        &world,
        &journal,
        "first synthetic",
        &[corpus_ref("S-0001-v1")],
        Cited::NotChecked,
    )
    .id
    .expect("first row");
    advance(&world, 120);
    let _ = log_direct(
        &world,
        &journal,
        "second synthetic",
        &[corpus_ref("S-0002-v1")],
        Cited::NotChecked,
    );
    let before = rows(&world);
    let events_before = events(&world).len();

    file::set_outcome(&world, &journal, first, AskedOutcome::CiteFailed, None)
        .expect("set_outcome");
    let after = rows(&world);
    assert_eq!(after[0].outcome, AskedOutcome::CiteFailed);
    assert_eq!(after[1], before[1]);
    assert_eq!(events(&world).len(), events_before, "no journal event");

    let unknown = AskedId::new(99).expect("valid id");
    let err = file::set_outcome(&world, &journal, unknown, AskedOutcome::Flagged, None)
        .expect_err("unknown id");
    assert!(err.contains("A-0099"), "{err}");
}

#[test]
fn outcome_updates_keep_inputs() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let journal = open(&world);
    let inputs = [corpus_ref("S-0001-v1"), corpus_ref("cmp-system-review")];
    let id = log_direct(
        &world,
        &journal,
        "synthetic inputs",
        &inputs,
        Cited::NotChecked,
    )
    .id
    .expect("a row");
    let before = rows(&world).pop().expect("row");
    file::set_outcome(&world, &journal, id, AskedOutcome::Flagged, None).expect("set_outcome");
    let after = rows(&world).pop().expect("row");
    assert_eq!(after.outcome, AskedOutcome::Flagged);
    assert_eq!(after.inputs, before.inputs);
    assert_eq!(after.labels, before.labels);
    assert_eq!(after.inputs.len(), 2);
}

#[test]
fn a_packs_labels_are_added_to_the_row() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let journal = open(&world);
    let labels = LabelSet::owner_supplied().union(&np_0007());
    let logged = log_question(
        &world,
        &journal,
        &AskedSettings::default(),
        &np_policy(),
        AskedVia::Context,
        &labels,
        &[corpus_ref("S-0001-v1")],
        "synthetic pack question",
        &hits(1, &["S-0001-v1"]),
        Cited::NotChecked,
    );
    assert!(logged.warning.is_none(), "{:?}", logged.warning);
    let row = rows(&world).pop().expect("one row");
    assert_eq!(row.labels.to_cell(), "np-0007|owner-supplied");
    let event = ask_events(&world).pop().expect("one event");
    let EventPayload::Ask {
        labels: event_labels,
        ..
    } = event.payload
    else {
        panic!("an ask event");
    };
    assert!(event_labels.contains(&LabelId::parse("np-0007").expect("label")));
}

#[test]
fn marked_search_question_is_not_retained_under_floor_a() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let out = search(&world, &["FOUO", "synthetic"]);
    let plain = unlogged(&["FOUO", "synthetic"]);
    assert_eq!(out.code, plain.code);
    assert_eq!(out.stdout, plain.stdout);
    assert_one_warning(&out);
    assert!(out.stderr.contains("FB0412"), "{}", out.stderr);
    assert!(log_bytes(&world).is_none());
    assert!(!world.state_dir().exists(), "refused before any state");
}

// --- Failures never stop a search ---------------------------------------------------------

#[test]
fn unwritable_journal_does_not_stop_search() {
    // Through `search`: the journal cannot be opened for writing.
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    let journal_path = world.state_dir().join("audit/journal.jsonl");
    let before = log_bytes(&world);
    advance(&world, 120);
    set_read_only(&journal_path, true);
    let out = search(&world, &["zzzznotaword"]);
    set_read_only(&journal_path, false);
    let plain = unlogged(&["zzzznotaword"]);
    assert_eq!(out.code, plain.code);
    assert_eq!(out.stdout, plain.stdout);
    assert_one_warning(&out);
    assert_eq!(log_bytes(&world), before, "the log gains no row");

    // Directly: the journal was opened, and the `ask` append fails.
    let journal = open(&world);
    advance(&world, 120);
    set_read_only(&journal_path, true);
    let logged = log_direct(
        &world,
        &journal,
        "synthetic append fails",
        &[],
        Cited::NotChecked,
    );
    set_read_only(&journal_path, false);
    assert!(logged.id.is_none());
    assert!(logged.warning.is_some());
    assert_eq!(log_bytes(&world), before, "no row without its event");
}

#[test]
fn unwritable_log_does_not_stop_search() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    let asked_dir = world.state_dir().join("asked");
    let before = log_bytes(&world);
    advance(&world, 120);
    set_read_only(&file::path(world.state_dir()), true);
    set_read_only(&asked_dir, true);
    let out = search(&world, &["zzzznotaword"]);
    set_read_only(&asked_dir, false);
    set_read_only(&file::path(world.state_dir()), false);
    let plain = unlogged(&["zzzznotaword"]);
    assert_eq!(out.code, plain.code);
    assert_eq!(out.stdout, plain.stdout);
    assert_one_warning(&out);
    assert_eq!(log_bytes(&world), before);
}

#[test]
fn bad_settings_warn_once() {
    for text in [
        "[asked]\nmode = \"loud\"\n",
        "[asked]\nkeep_days = -1\n",
        "[asked]\ncolour = \"blue\"\n",
        "[asked\n",
    ] {
        let tmp = corpus();
        let world = TestWorld::new(tmp.path());
        write_settings(&world, text);
        let out = search(&world, &["synthetic"]);
        let plain = unlogged(&["synthetic"]);
        assert_eq!(out.code, plain.code, "{text}");
        assert_eq!(out.stdout, plain.stdout, "{text}");
        assert_one_warning(&out);
        assert!(log_bytes(&world).is_none(), "{text}: nothing logged");
        assert!(ask_events(&world).is_empty(), "{text}: no event");
    }
}

#[test]
fn question_with_a_line_break_warns() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let out = search(&world, &["synthetic\nreview"]);
    let plain = unlogged(&["synthetic\nreview"]);
    assert_eq!(out.code, plain.code);
    assert_eq!(out.stdout, plain.stdout);
    assert_one_warning(&out);
    assert!(!world.state_dir().exists(), "refused before any state");
}

#[test]
fn unparsable_log_is_left_alone() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    std::fs::create_dir_all(world.state_dir().join("asked")).expect("dir");
    let broken = b"id,when\nA-0001,yesterday\n";
    std::fs::write(file::path(world.state_dir()), broken).expect("write");
    let out = search(&world, &["synthetic"]);
    assert_one_warning(&out);
    assert!(out.stderr.contains("does not parse"), "{}", out.stderr);
    assert_eq!(log_bytes(&world).as_deref(), Some(&broken[..]));
    assert!(ask_events(&world).is_empty());
}

#[test]
fn ids_are_not_reused_after_expiry_or_a_deleted_log() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let _ = search(&world, &["synthetic"]);
    advance(&world, 120);
    let _ = search(&world, &["review"]);
    assert_eq!(
        rows(&world).last().map(|row| row.id.to_string()),
        Some("A-0002".to_owned())
    );

    std::fs::remove_file(file::path(world.state_dir())).expect("delete the log");
    advance(&world, 120);
    let _ = search(&world, &["window"]);
    assert_eq!(
        rows(&world)
            .iter()
            .map(|row| row.id.to_string())
            .collect::<Vec<_>>(),
        ["A-0003"]
    );

    advance(&world, 400 * 86_400);
    let _ = search(&world, &["checklist"]);
    assert_eq!(
        rows(&world)
            .iter()
            .map(|row| row.id.to_string())
            .collect::<Vec<_>>(),
        ["A-0004"]
    );
}

#[test]
fn asked_id_exhaustion_does_not_reuse_or_widen() {
    // From the rows: A-9998 gives A-9999, then nothing.
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    std::fs::create_dir_all(world.state_dir().join("asked")).expect("dir");
    let mut seed = state_asked_rows().swap_remove(5);
    seed.id = AskedId::new(9998).expect("valid");
    seed.at = world.now();
    std::fs::write(file::path(world.state_dir()), asked::render(&[seed])).expect("seed");

    advance(&world, 120);
    let out = search(&world, &["synthetic"]);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    assert_eq!(
        rows(&world).last().map(|row| row.id.to_string()),
        Some("A-9999".to_owned())
    );

    advance(&world, 120);
    let asks_before = ask_events(&world).len();
    let log_before = log_bytes(&world);
    let out = search(&world, &["review"]);
    let plain = unlogged(&["review"]);
    assert_eq!(out.code, plain.code);
    assert_eq!(out.stdout, plain.stdout);
    assert_eq!(out.stderr, "question not recorded: id space exhausted\n");
    assert_eq!(log_bytes(&world), log_before, "no row");
    assert_eq!(
        ask_events(&world).len(),
        asks_before,
        "no event claims an id"
    );
    let text = String::from_utf8(log_bytes(&world).expect("log")).expect("utf-8");
    assert!(!text.contains("A-10000") && !text.contains("A-0000"));

    // From the journal: the log is deleted, and the journal still ends at A-9999.
    std::fs::remove_file(file::path(world.state_dir())).expect("delete the log");
    advance(&world, 120);
    let out = search(&world, &["window"]);
    assert_eq!(out.stderr, "question not recorded: id space exhausted\n");
    assert!(
        log_bytes(&world).is_none(),
        "an empty log does not restart the ids"
    );

    // After expiry: the old rows go, and the ids still do not restart.
    std::fs::create_dir_all(world.state_dir().join("asked")).expect("dir");
    let mut old = state_asked_rows().swap_remove(5);
    old.id = AskedId::new(9999).expect("valid");
    old.at = now_at("2020-01-01T00:00:00Z");
    std::fs::write(file::path(world.state_dir()), asked::render(&[old])).expect("old row");
    advance(&world, 120);
    let out = search(&world, &["checklist"]);
    assert_eq!(out.stderr, "question not recorded: id space exhausted\n");
    assert!(
        rows(&world).is_empty(),
        "the expired row is gone and none took its place"
    );

    // Mode off still writes its id-free event.
    write_settings(&world, "[asked]\nmode = \"off\"\n");
    let asks_before = ask_events(&world).len();
    advance(&world, 120);
    let out = search(&world, &["onboarding"]);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    assert_eq!(ask_events(&world).len(), asks_before + 1);

    // Five digits never parse, and A-9999 is the last id.
    assert!(AskedId::new(10_000).is_none());
    assert!(AskedId::parse("A-10000").is_err());
}

// --- Round trips and parsers ----------------------------------------------------------------

#[test]
fn inputs_round_trip_across_stores() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let mut row = state_asked_rows().swap_remove(1);
    row.inputs = vec![
        ItemRef::new(StoreId::Corpus, "X-1"),
        ItemRef::new(StoreId::State, "X-1"),
        ItemRef::new(StoreId::Working, "odd \"id\", with\\ escapes"),
        ItemRef::new(StoreId::Overlay, "tab\there"),
    ];
    row.inputs.sort();
    let journal = open(&world);
    let lease = world.write_lease().expect("lease");
    let audited = journal.state_scope(&lease).expect("state scope");
    file::write_rows(&world, &audited, std::slice::from_ref(&row)).expect("write");
    drop(audited);
    drop(lease);
    let back = rows(&world);
    assert_eq!(back, vec![row.clone()]);
    assert_eq!(
        back[0].inputs.len(),
        4,
        "equal ids in different stores stay distinct"
    );
    let text = String::from_utf8(log_bytes(&world).expect("log")).expect("utf-8");
    assert_eq!(
        text.lines().count(),
        2,
        "one header and one single-line row"
    );
}

#[test]
fn missing_inputs_is_not_empty() {
    let mut text = asked::render(&state_asked_rows()[..1]);
    text = text.replacen(",[],", ",,", 1);
    let err = asked::parse(text.as_bytes()).expect_err("a missing inputs cell");
    assert!(err.message.contains("inputs"), "{err}");

    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    std::fs::create_dir_all(world.state_dir().join("asked")).expect("dir");
    std::fs::write(file::path(world.state_dir()), &text).expect("write");
    assert!(file::read_rows(world.state_dir()).is_err());
}

#[test]
fn asked_settings_never_panic_on_random_and_mutated_bytes() {
    let mut rng = Rng::new(0x0a5c_ed10);
    for _ in 0..200 {
        let len = (rng.next_u64() % 160) as usize;
        let bytes = rng.bytes(len);
        let _ = AskedSettings::parse(&String::from_utf8_lossy(&bytes));
    }
    let fixture = std::fs::read(state_asked().join("settings.toml")).expect("fixture settings");
    for i in 0..fixture.len() {
        for delta in [1u8, 0x20, 0x80] {
            let mut mutated = fixture.clone();
            mutated[i] = mutated[i].wrapping_add(delta);
            let _ = AskedSettings::parse(&String::from_utf8_lossy(&mutated));
        }
    }
}

#[test]
fn logging_adds_little() {
    let tmp = corpus();
    let world = TestWorld::new(tmp.path());
    let plain_world = TestWorld::new(&corpus_mini());
    let _ = search(&world, &["warm"]);
    let start = Instant::now();
    for n in 0..5 {
        advance(&world, 120);
        let _ = search(&world, &["synthetic", &n.to_string()]);
    }
    let logged = start.elapsed();
    let start = Instant::now();
    for n in 0..5 {
        let _ = search(&plain_world, &["synthetic", &n.to_string()]);
    }
    let unlogged = start.elapsed();
    println!("5 logged searches: {logged:?}; 5 unlogged searches: {unlogged:?}");
}

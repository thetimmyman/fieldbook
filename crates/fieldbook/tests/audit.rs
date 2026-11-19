//! Integration tests for the audit journal, `audit verify` and `audit show` (`audit-journal`).

#![allow(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "integration tests prepare fixture directories and print benchmark timings"
)]

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fieldbook::World;
use fieldbook::io::journal::{
    JournalError, set_fail_on_settings_change_append, set_lock_timeout_for_test, write_sync_file,
};
use fieldbook::io::state;
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    Count, Event, EventEnvelope, EventPayload, Kind, LabelSet, Now, OperationIntent, OperationKey,
    Principal, RecordRef, RefuseAbout, SettingsGroup, Sha256, StoreId, Verb, Via, Word, WriteFile,
    WriteSet, ZERO_HASH, completed_after_hash, request_sha256, split_hash_suffix,
};
use fieldbook_core::check::Code;
use serde_json::Value;

use support::state::{Plant, build_state, plant, state_mini_events};
use support::world::TestWorld;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_mini() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
}

fn state_mini() -> PathBuf {
    repo_root().join("tests/fixtures/state-mini")
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create_dir_all");
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let ty = entry.file_type().expect("file_type");
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), &dst_path).expect("copy file");
        }
    }
}

fn collect_relative_files(root: &Path, cur: &Path, out: &mut BTreeSet<String>) {
    if !cur.exists() {
        return;
    }
    for entry in std::fs::read_dir(cur).expect("read_dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            collect_relative_files(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .expect("strip_prefix")
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel);
        }
    }
}

fn compare_dirs_or_save(expected_dir: &Path, rebuilt_dir: &Path, save_rel: &str) -> Option<String> {
    let mut expected_files = BTreeSet::new();
    let mut rebuilt_files = BTreeSet::new();
    collect_relative_files(expected_dir, expected_dir, &mut expected_files);
    collect_relative_files(rebuilt_dir, rebuilt_dir, &mut rebuilt_files);

    let mut diffs = Vec::new();
    if expected_files != rebuilt_files {
        diffs.push(format!(
            "file set mismatch: expected {expected_files:?}, rebuilt {rebuilt_files:?}"
        ));
    } else {
        for rel in &expected_files {
            let exp_bytes = std::fs::read(expected_dir.join(rel)).expect("read expected");
            let reb_bytes = std::fs::read(rebuilt_dir.join(rel)).expect("read rebuilt");
            if exp_bytes != reb_bytes {
                diffs.push(format!("content differs in `{rel}`"));
            }
        }
    }

    if !diffs.is_empty() {
        let save_path = repo_root().join(save_rel);
        if save_path.exists() {
            let _ = std::fs::remove_dir_all(&save_path);
        }
        copy_dir_all(rebuilt_dir, &save_path);
        Some(format!(
            "fixture `{}` differs from writer output ({}); rebuilt folder saved at `{save_rel}`",
            expected_dir.display(),
            diffs.join("; ")
        ))
    } else {
        None
    }
}

fn sample_intent(verb_str: &str, subject_byte: u8, req_byte: u8) -> OperationIntent {
    let verb = Verb::parse(verb_str).expect("valid verb");
    let subject_sha256 = Sha256::parse(&format!("{subject_byte:02x}").repeat(32)).expect("sha256");
    let request_sha256 = request_sha256(&[("op", &[req_byte])]).expect("request_sha256");
    OperationIntent {
        schema: 1,
        key: OperationKey {
            verb,
            subject_sha256,
        },
        request_sha256,
        allocated: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("valid ref")],
        allocated_store_uid: None,
        planned_on: CivilDate::from_ymd(2026, 3, 1).expect("valid date"),
    }
}

fn sample_write_set() -> WriteSet {
    let content = b"synthetic content\n";
    let sha = Sha256::digest(content);
    WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/registers/claims.csv",
            sha.as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("valid ref")],
    }
}

fn run_args(list: &[&str], world: &dyn World) -> fieldbook::Outcome {
    let argv: Vec<String> = list.iter().map(|s| (*s).to_owned()).collect();
    fieldbook::run(&argv, world)
}

#[test]
fn state_mini_matches_the_writer() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let rebuilt = tmp.path().join("state-mini");
    build_state(&rebuilt, &state_mini_events());
    if let Some(msg) =
        compare_dirs_or_save(&state_mini(), &rebuilt, "target/state-rebuilt/state-mini")
    {
        panic!("{msg}");
    }
}

#[test]
fn state_known_bad_matches_the_writer() {
    let root = repo_root();
    let mut failures = Vec::new();
    for (plant_kind, code) in Plant::ALL {
        let tmp = tempfile::tempdir().expect("tempdir");
        let rebuilt = tmp.path().join(code);
        plant(&rebuilt, plant_kind);
        let expected = root.join("tests/fixtures/state-known-bad").join(code);
        if let Some(msg) = compare_dirs_or_save(
            &expected,
            &rebuilt,
            &format!("target/state-rebuilt/state-known-bad/{code}"),
        ) {
            failures.push(msg);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn open_creates_a_missing_state_folder() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    assert!(!state_dir.exists());

    let world = TestWorld::new(tmp.path());
    let _journal = state::open(&world, &state_dir, Via::Cli).expect("open creates state");
    assert!(state_dir.is_dir());
    assert!(state_dir.join("audit/journal.jsonl").is_file());
}

#[test]
fn read_only_inspection_creates_no_state() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    assert!(!state_dir.exists());

    let world = TestWorld::new(tmp.path());
    let out_verify = run_args(&["audit", "verify"], &world);
    assert_eq!(out_verify.code, 0);
    assert!(!state_dir.exists(), "audit verify must not create state");

    let out_show = run_args(&["audit", "show"], &world);
    assert_eq!(out_show.code, 0);
    assert!(!state_dir.exists(), "audit show must not create state");
}

#[test]
fn append_survives_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    {
        let mut journal = state::open(&world, &state_dir, Via::Cli).expect("first open");
        let lease = world.write_lease().expect("write lease");
        let intent = sample_intent("new claim", 1, 1);
        let ws = sample_write_set();
        let token = journal.begin(&lease, &intent, &ws).expect("begin");
        journal.end(token, Word::new_static("ok")).expect("end");
    }

    {
        let _journal = state::open(&world, &state_dir, Via::Cli).expect("second open");
    }

    let out = run_args(&["audit", "verify"], &world);
    assert_eq!(
        out.code, 0,
        "stdout: {}, stderr: {}",
        out.stdout, out.stderr
    );
    assert!(out.stdout.starts_with("journal ok: 5 events, head "));
}

#[test]
fn state_scope_appends_no_event() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let before = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");

    let lease = world.write_lease().expect("write lease");
    let token = journal.state_scope(&lease).expect("state_scope");
    assert!(token.is_state_only());
    drop(token);
    drop(lease);

    let after = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    assert_eq!(before, after, "state_scope must append no event");
}

#[test]
fn state_scope_retains_the_validated_lease() {
    set_lock_timeout_for_test(Some(Duration::from_millis(250)));
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let journal1 = state::open(&world, &state_dir, Via::Cli).expect("open 1");
    let journal2 = state::open(&world, &state_dir, Via::Cli).expect("open 2");
    let before = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");

    let lease = world.write_lease().expect("write lease");
    let token = journal1.state_scope(&lease).expect("state_scope");
    let after_acq = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    assert_eq!(before, after_acq, "state_scope acquisition adds no event");

    // While `token` is alive, a competing journal writer is refused with Busy.
    let err = journal2
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect_err("competing writer must be blocked while state_scope token is held");
    assert_eq!(err, JournalError::Busy);

    drop(token);

    // Once `token` is dropped, the competing writer can append.
    journal2
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect("append succeeds after state_scope token is dropped");
    set_lock_timeout_for_test(None);
}

#[test]
fn state_scope_errors_write_nothing() {
    set_lock_timeout_for_test(Some(Duration::from_millis(250)));
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal1 = state::open(&world, &state_dir, Via::Cli).expect("open 1");
    let journal2 = state::open(&world, &state_dir, Via::Cli).expect("open 2");

    // 1. Foreign lease is refused and writes nothing.
    let other_tmp = tempfile::tempdir().expect("other tempdir");
    copy_dir_all(&corpus_mini(), other_tmp.path());
    let other_world = TestWorld::new(other_tmp.path());
    let foreign_lease = other_world.write_lease().expect("foreign lease");
    let before = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    assert!(journal1.state_scope(&foreign_lease).is_err());
    drop(foreign_lease);

    // 2. Shared (read-only) lease is refused and writes nothing.
    let shared_lease = fieldbook::io::journal::acquire_read_lease(world.tree_root(), &state_dir)
        .expect("read lease");
    assert!(journal1.state_scope(&shared_lease).is_err());
    drop(shared_lease);
    assert_eq!(
        before,
        std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read")
    );

    // 3. Timeout when another transaction holds the journal lock: returns no token and writes nothing.
    let lease = world.write_lease().expect("write lease");
    let intent = sample_intent("add", 2, 2);
    let ws = sample_write_set();
    let active_tx = journal1.begin(&lease, &intent, &ws).expect("begin");
    let during_tx = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    assert_eq!(
        journal2.state_scope(&lease).unwrap_err(),
        JournalError::Busy
    );
    assert_eq!(
        during_tx,
        std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read")
    );
    journal1
        .end(active_tx, Word::new_static("ok"))
        .expect("end");

    // 4. Invalid journal head: returns BrokenChain and writes nothing.
    let journal_path = state_dir.join("audit/journal.jsonl");
    let mut broken = std::fs::read_to_string(&journal_path).expect("read");
    broken = broken.replacen("\"outcome\":\"ok\"", "\"outcome\":\"failed\"", 1);
    write_sync_file(&journal_path, broken.as_bytes()).expect("write broken");
    assert!(matches!(
        journal1.state_scope(&lease),
        Err(JournalError::BrokenChain(_))
    ));
    assert_eq!(
        broken,
        std::fs::read_to_string(&journal_path).expect("read")
    );
    set_lock_timeout_for_test(None);
}

#[test]
fn last_time_of_returns_the_newest_event_of_a_kind() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    journal.set_segment_limit(200);

    let t1 = Now::new(CivilDate::from_ymd(2026, 3, 1).expect("date"), 43_300).expect("t1");
    world.set_now(t1);
    journal
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect("refuse 1");

    let t2 = Now::new(CivilDate::from_ymd(2026, 3, 1).expect("date"), 45_000).expect("t2");
    world.set_now(t2);
    journal
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect("refuse 2 in second segment");

    assert!(state_dir.join("audit/journal-0002.jsonl").is_file());
    assert_eq!(journal.last_time_of(Kind::Refuse), Some(t2));
}

#[test]
fn last_time_of_is_none_with_no_event() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    assert_eq!(journal.last_time_of(Kind::AuditExport), None);
}

#[test]
fn torn_tail_is_moved_aside_and_recorded() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    plant(&state_dir, Plant::TornTail);

    let world = TestWorld::new(tmp.path());
    let _journal = state::open(&world, &state_dir, Via::Cli).expect("open recovers torn tail");

    let torn_file = state_dir.join("audit/journal.torn-12");
    assert!(torn_file.is_file(), "expected journal.torn-12 to exist");

    let out = run_args(&["audit", "verify", "--json"], &world);
    assert_eq!(out.code, 0);
    let parsed: Value = serde_json::from_str(&out.stdout).expect("json");
    assert_eq!(parsed["state"], "torn");
    assert_eq!(parsed["findings"][0]["code"], "FB0804");
}

#[test]
fn segment_rolls_at_the_limit() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    journal.set_segment_limit(250);

    for _ in 0..3 {
        journal
            .refuse(
                Some("add"),
                Code::Fb0410,
                Word::new_static("label"),
                &RefuseAbout::default(),
            )
            .expect("refuse");
    }

    assert!(state_dir.join("audit/journal.jsonl").is_file());
    assert!(state_dir.join("audit/journal-0002.jsonl").is_file());

    let out = run_args(&["audit", "verify"], &world);
    assert_eq!(
        out.code, 0,
        "multi-segment chain must verify clean: {}",
        out.stdout
    );
}

#[cfg(unix)]
#[test]
fn unwritable_journal_blocks_the_write() {
    use std::os::unix::fs::PermissionsExt as _;

    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("write lease");

    let journal_file = state_dir.join("audit/journal.jsonl");
    let orig_perms = std::fs::metadata(&journal_file)
        .expect("meta")
        .permissions();
    let mut ro_perms = orig_perms.clone();
    ro_perms.set_mode(0o400);
    std::fs::set_permissions(&journal_file, ro_perms).expect("set ro");

    let intent = sample_intent("new claim", 3, 3);
    let ws = sample_write_set();
    let err = journal
        .begin(&lease, &intent, &ws)
        .expect_err("unwritable journal must refuse begin");
    assert_eq!(err.exit_code(), 3);

    std::fs::set_permissions(&journal_file, orig_perms).expect("restore perms");
}

#[cfg(not(unix))]
#[test]
fn unwritable_journal_blocks_the_write() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("write lease");
    let journal_file = state_dir.join("audit/journal.jsonl");
    let mut perms = std::fs::metadata(&journal_file)
        .expect("meta")
        .permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&journal_file, perms.clone()).expect("set ro");

    let intent = sample_intent("new claim", 3, 3);
    let ws = sample_write_set();
    let err = journal
        .begin(&lease, &intent, &ws)
        .expect_err("unwritable journal must refuse begin");
    assert_eq!(err.exit_code(), 3);

    perms.set_readonly(false);
    std::fs::set_permissions(&journal_file, perms).expect("restore perms");
}

#[test]
fn broken_chain_refuses_the_append() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let journal_path = state_dir.join("audit/journal.jsonl");
    let raw = std::fs::read_to_string(&journal_path).expect("read");
    let tampered = raw.replacen("\"writer\":0", "\"writer\":1", 1);
    write_sync_file(&journal_path, tampered.as_bytes()).expect("write tampered");

    let err = journal
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect_err("broken chain must refuse append");
    assert!(matches!(err, JournalError::BrokenChain(_)));
    assert_eq!(err.exit_code(), 3);
}

#[test]
fn repair_moves_a_broken_chain_aside() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let fb0800_src = repo_root().join("tests/fixtures/state-known-bad/FB0800");
    copy_dir_all(&fb0800_src, &state_dir);
    let original_bytes =
        std::fs::read(state_dir.join("audit/journal.jsonl")).expect("read original FB0800");

    let world = TestWorld::new(tmp.path());
    let out_repair = run_args(&["audit", "verify", "--repair"], &world);
    assert_eq!(out_repair.code, 1);
    assert!(
        out_repair.stdout.contains("FB0804") && out_repair.stdout.contains("journal.broken-5"),
        "unexpected repair output: {}",
        out_repair.stdout
    );

    let broken_copy = state_dir.join("audit/journal.broken-5/journal.jsonl");
    assert!(broken_copy.is_file());
    assert_eq!(
        std::fs::read(&broken_copy).expect("read broken copy"),
        original_bytes
    );

    // Second event of new journal is journal.recover with reason = broken_chain and last_seq = 4.
    let new_journal = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    let new_lines: Vec<&str> = new_journal.lines().collect();
    assert_eq!(new_lines.len(), 2);
    let (ev2, _) = Event::parse_line(new_lines[1]).expect("parse recover line");
    assert_eq!(ev2.kind(), Kind::JournalRecover);
    let orig_lines: Vec<&str> = std::str::from_utf8(&original_bytes)
        .expect("utf8")
        .lines()
        .collect();
    let (_, expected_hash_4) = split_hash_suffix(orig_lines[3]).expect("line 4 hash");
    assert_eq!(
        ev2.payload,
        EventPayload::JournalRecover {
            reason: Word::new_static("broken_chain"),
            last_seq: Count::new(4),
            last_hash: expected_hash_4,
        }
    );

    // A second audit verify exits 0.
    let out_second = run_args(&["audit", "verify"], &world);
    assert_eq!(out_second.code, 0);
    assert!(out_second.stdout.starts_with("journal ok: 2 events, head "));

    // On a clean copy of state-mini, --repair writes nothing and exits 0.
    let clean_tmp = tempfile::tempdir().expect("clean tempdir");
    copy_dir_all(&corpus_mini(), clean_tmp.path());
    let clean_state = clean_tmp.path().join(".fieldbook");
    copy_dir_all(&state_mini(), &clean_state);
    let before_clean = std::fs::read(clean_state.join("audit/journal.jsonl")).expect("read clean");
    let clean_world = TestWorld::new(clean_tmp.path());
    let out_clean = run_args(&["audit", "verify", "--repair"], &clean_world);
    assert_eq!(out_clean.code, 0);
    assert!(out_clean.stdout.starts_with("journal ok: 12 events, head "));
    assert_eq!(
        before_clean,
        std::fs::read(clean_state.join("audit/journal.jsonl")).expect("read clean after")
    );
}

#[test]
fn idle_handles_do_not_hold_the_journal_lock() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let handle1 = state::open(&world, &state_dir, Via::Cli).expect("open 1");
    let handle2 = state::open(&world, &state_dir, Via::Cli).expect("open 2");

    handle1
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect("append on handle 1 while handle 2 is idle");
    handle2
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect("append on handle 2 while handle 1 is idle");
}

#[test]
fn second_writer_is_refused() {
    set_lock_timeout_for_test(Some(Duration::from_millis(250)));
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut j1 = state::open(&world, &state_dir, Via::Cli).expect("open 1");
    let mut j2 = state::open(&world, &state_dir, Via::Cli).expect("open 2");
    let lease = world.write_lease().expect("write lease");

    let intent1 = sample_intent("add", 4, 4);
    let ws1 = sample_write_set();
    let token1 = j1.begin(&lease, &intent1, &ws1).expect("begin 1");

    let intent2 = sample_intent("new claim", 5, 5);
    let ws2 = sample_write_set();
    let err = j2
        .begin(&lease, &intent2, &ws2)
        .expect_err("second concurrent write transaction must be refused");
    assert_eq!(err, JournalError::Busy);
    assert_eq!(err.exit_code(), 3);
    assert_eq!(
        err.to_string(),
        "the journal is in use by another fieldbook process"
    );

    j1.end(token1, Word::new_static("ok")).expect("end 1");
    set_lock_timeout_for_test(None);
}

#[test]
fn lock_waits_then_refuses() {
    set_lock_timeout_for_test(None);
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut j1 = state::open(&world, &state_dir, Via::Cli).expect("open 1");
    let lease = world.write_lease().expect("write lease");
    let intent = sample_intent("add", 6, 6);
    let ws = sample_write_set();
    let token1 = j1.begin(&lease, &intent, &ws).expect("begin 1");

    // 1. A competing writer started ~300 ms before the first releases succeeds.
    let state_clone = state_dir.clone();
    let root_clone = tmp.path().to_path_buf();
    let waiter = std::thread::spawn(move || {
        let w = TestWorld::new(&root_clone);
        let j2 = state::open(&w, &state_clone, Via::Cli)?;
        j2.refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
    });

    std::thread::sleep(Duration::from_millis(350));
    j1.end(token1, Word::new_static("ok")).expect("end 1");
    waiter
        .join()
        .expect("join waiter")
        .expect("competing writer that waited < 3s must succeed");

    // 2. A competing writer that waits longer than the timeout is refused with exit 3.
    set_lock_timeout_for_test(Some(Duration::from_millis(350)));
    let token2 = j1.begin(&lease, &intent, &ws).expect("begin 2");
    let j3 = state::open(&world, &state_dir, Via::Cli).expect_err("must time out");
    assert_eq!(j3, JournalError::Busy);
    assert_eq!(j3.exit_code(), 3);
    j1.end(token2, Word::new_static("ok")).expect("end 2");
    set_lock_timeout_for_test(None);
}

#[test]
fn transaction_appends_reuse_the_lease() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("write lease");
    let intent = sample_intent("add", 7, 7);
    let ws = sample_write_set();

    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    // Appending `refuse` or taking `state_scope` on the same `Journal` while `token` is active
    // reuses the active transaction's journal lock without deadlocking.
    journal
        .refuse(
            Some("add"),
            Code::Fb0410,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect("refuse inside active transaction reuses lease");
    let scope_token = journal
        .state_scope(&lease)
        .expect("state_scope inside active transaction reuses lease");
    drop(scope_token);
    journal.end(token, Word::new_static("ok")).expect("end");
}

#[test]
fn cancel_releases_journal_lease() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut j1 = state::open(&world, &state_dir, Via::Cli).expect("open 1");
    let j2 = state::open(&world, &state_dir, Via::Cli).expect("open 2");
    let lease = world.write_lease().expect("write lease");
    let intent = sample_intent("add", 8, 8);
    let ws = sample_write_set();

    let token = j1.begin(&lease, &intent, &ws).expect("begin");
    // Drop `token` without calling `end` (simulating cancellation).
    drop(token);

    // The journal lock is released immediately and the unmatched `write.begin` remains visible.
    j2.refuse(
        Some("add"),
        Code::Fb0410,
        Word::new_static("label"),
        &RefuseAbout::default(),
    )
    .expect("j2 can append after j1 transaction dropped");

    let out = run_args(&["audit", "verify", "--json"], &world);
    assert_eq!(out.code, 0);
    let parsed: Value = serde_json::from_str(&out.stdout).expect("json");
    assert_eq!(parsed["state"], "unfinished");
    assert_eq!(parsed["unfinished"], serde_json::json!([4]));
}

// control: AL-01
#[test]
fn each_code_is_raised_once() {
    let root = repo_root();
    let corpus = corpus_mini();

    for (code, folder, extra_args, expected_exit) in [
        ("FB0800", "state-known-bad/FB0800", &[][..], 1u8),
        ("FB0801", "state-known-bad/FB0801", &[][..], 1u8),
        ("FB0802", "state-mini", &["--expect-seq", "99"][..], 1u8),
        ("FB0803", "state-known-bad/FB0803", &[][..], 0u8),
        ("FB0804", "state-known-bad/FB0804", &[][..], 0u8),
        ("FB0805", "state-known-bad/FB0805", &[][..], 0u8),
    ] {
        let state_dir = root.join("tests/fixtures").join(folder);
        let world = TestWorld::with_state(&corpus, &state_dir);
        let mut argv = vec!["audit", "verify", "--json"];
        argv.extend_from_slice(extra_args);
        let out = run_args(&argv, &world);
        assert_eq!(out.code, expected_exit, "{code}: {}", out.stdout);
        let parsed: Value = serde_json::from_str(&out.stdout).expect("json");
        let findings = parsed["findings"].as_array().expect("findings array");
        assert_eq!(
            findings.len(),
            1,
            "expected exactly 1 finding for {code}, got {findings:?}"
        );
        assert_eq!(findings[0]["code"], code);
    }
}

// control: AL-01
#[test]
fn expect_head_and_seq() {
    let world = TestWorld::with_state(&corpus_mini(), &state_mini());
    let show_out = run_args(&["audit", "show", "--head", "--json"], &world);
    assert_eq!(show_out.code, 0);
    let show_json: Value = serde_json::from_str(&show_out.stdout).expect("json");
    let head_hash = show_json["head"]["hash"].as_str().expect("head hash");

    // Exact match passes.
    let ok_out = run_args(
        &[
            "audit",
            "verify",
            "--expect-seq",
            "12",
            "--expect-head",
            head_hash,
        ],
        &world,
    );
    assert_eq!(ok_out.code, 0);

    // Wrong seq fails with FB0802.
    let bad_seq = run_args(&["audit", "verify", "--expect-seq", "11"], &world);
    assert_eq!(bad_seq.code, 1);
    assert!(bad_seq.stdout.contains("FB0802"));

    // Wrong head fails with FB0802.
    let bad_head = run_args(&["audit", "verify", "--expect-head", ZERO_HASH], &world);
    assert_eq!(bad_head.code, 1);
    assert!(bad_head.stdout.contains("FB0802"));
}

#[test]
fn anchor_allows_a_valid_tail() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join("state");
    copy_dir_all(&state_mini(), &state_dir);

    let raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    let lines: Vec<&str> = raw.lines().collect();
    let (_, hash_6) = split_hash_suffix(lines[5]).expect("line 6 hash");

    let world = TestWorld::with_state(tmp.path(), &state_dir);
    let out = run_args(
        &[
            "audit",
            "verify",
            "--anchor-seq",
            "6",
            "--anchor-head",
            hash_6.as_str(),
        ],
        &world,
    );
    assert_eq!(
        out.code, 0,
        "valid tail after anchor must pass: {}",
        out.stdout
    );
}

#[test]
fn anchor_refuses_changed_missing_or_corrupt_chain() {
    let world = TestWorld::with_state(&corpus_mini(), &state_mini());

    // 1. Missing pair or invalid syntax exits 2 without creating state.
    let no_state_tmp = tempfile::tempdir().expect("tempdir");
    let no_state_dir = no_state_tmp.path().join("absent-state");
    let no_state_world = TestWorld::with_state(&corpus_mini(), &no_state_dir);
    assert_eq!(
        run_args(&["audit", "verify", "--anchor-seq", "1"], &no_state_world).code,
        2
    );
    assert_eq!(
        run_args(
            &["audit", "verify", "--anchor-head", ZERO_HASH],
            &no_state_world
        )
        .code,
        2
    );
    assert_eq!(
        run_args(
            &[
                "audit",
                "verify",
                "--anchor-seq",
                "0",
                "--anchor-head",
                ZERO_HASH
            ],
            &no_state_world
        )
        .code,
        2
    );
    assert_eq!(
        run_args(
            &[
                "audit",
                "verify",
                "--anchor-seq",
                "1",
                "--anchor-head",
                ZERO_HASH,
                "--repair"
            ],
            &no_state_world
        )
        .code,
        2
    );
    assert!(!no_state_dir.exists());

    // 2. Rehashed/mismatched prefix at seq 6 fails with FB0802 / exit 1.
    let mismatch = run_args(
        &[
            "audit",
            "verify",
            "--anchor-seq",
            "6",
            "--anchor-head",
            ZERO_HASH,
        ],
        &world,
    );
    assert_eq!(mismatch.code, 1);
    assert!(mismatch.stdout.contains("FB0802"));

    // 3. Missing anchor (seq 99 > 12) fails with FB0802 / exit 1.
    let missing = run_args(
        &[
            "audit",
            "verify",
            "--anchor-seq",
            "99",
            "--anchor-head",
            ZERO_HASH,
        ],
        &world,
    );
    assert_eq!(missing.code, 1);
    assert!(missing.stdout.contains("FB0802"));

    // 4. Corrupt tail after valid anchor at seq 2 retains its FB0800 chain error / exit 1.
    let fb0800_dir = repo_root().join("tests/fixtures/state-known-bad/FB0800");
    let raw = std::fs::read_to_string(fb0800_dir.join("audit/journal.jsonl")).expect("read");
    let lines: Vec<&str> = raw.lines().collect();
    let (_, hash_2) = split_hash_suffix(lines[1]).expect("line 2 hash");
    let corrupt_world = TestWorld::with_state(&corpus_mini(), &fb0800_dir);
    let corrupt_out = run_args(
        &[
            "audit",
            "verify",
            "--anchor-seq",
            "2",
            "--anchor-head",
            hash_2.as_str(),
        ],
        &corrupt_world,
    );
    assert_eq!(corrupt_out.code, 1);
    assert!(corrupt_out.stdout.contains("FB0800"));
}

#[test]
fn show_filters_by_kind_and_id() {
    let world = TestWorld::with_state(&corpus_mini(), &state_mini());

    let by_kind = run_args(
        &["audit", "show", "--kind", "write.begin", "--json"],
        &world,
    );
    assert_eq!(by_kind.code, 0);
    let kind_json: Value = serde_json::from_str(&by_kind.stdout).expect("json");
    assert_eq!(kind_json["shown"], 3);

    let by_id = run_args(&["audit", "show", "--id", "CL-CMP-0001", "--json"], &world);
    assert_eq!(by_id.code, 0);
    let id_json: Value = serde_json::from_str(&by_id.stdout).expect("json");
    assert_eq!(id_json["shown"], 2);
    let evs = id_json["events"].as_array().expect("events");
    assert_eq!(evs[0]["seq"], 5);
    assert_eq!(evs[1]["seq"], 6);

    let by_qual_id = run_args(
        &["audit", "show", "--id", "corpus/CL-CMP-0001", "--json"],
        &world,
    );
    assert_eq!(by_qual_id.code, 0);
    let qual_json: Value = serde_json::from_str(&by_qual_id.stdout).expect("json");
    assert_eq!(qual_json["shown"], 2);

    // Allowed empty affected list on a store-level configuration write does not match record IDs.
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let tmp_world = TestWorld::new(tmp.path());
    let mut journal = state::open(&tmp_world, &state_dir, Via::Cli).expect("open");
    let lease = tmp_world.write_lease().expect("lease");
    let mut intent = sample_intent("migrate", 9, 9);
    intent.allocated.clear();
    let cfg_ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/corpus.toml",
            Sha256::digest(b"schema_version = 1\n").as_str().to_owned(),
        )],
        affected: Vec::new(),
    };
    let token = journal
        .begin(&lease, &intent, &cfg_ws)
        .expect("begin empty affected");
    journal.end(token, Word::new_static("ok")).expect("end");

    let empty_id_out = run_args(
        &["audit", "show", "--id", "CL-CMP-0001", "--json"],
        &tmp_world,
    );
    let empty_id_json: Value = serde_json::from_str(&empty_id_out.stdout).expect("json");
    assert_eq!(empty_id_json["shown"], 0);
}

#[test]
fn show_filters_by_label() {
    let world = TestWorld::with_state(&corpus_mini(), &state_mini());

    let by_np = run_args(&["audit", "show", "--label", "np-0007", "--json"], &world);
    assert_eq!(by_np.code, 0);
    let np_json: Value = serde_json::from_str(&by_np.stdout).expect("json");
    assert_eq!(np_json["shown"], 1);
    assert_eq!(np_json["events"][0]["seq"], 8);
    assert_eq!(np_json["events"][0]["kind"], "refuse");
}

#[test]
fn envelope_is_on_every_event() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());
    let _journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let fresh_raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");

    let raw = std::fs::read_to_string(state_mini().join("audit/journal.jsonl")).expect("read");
    for line in fresh_raw.lines().chain(raw.lines()) {
        let (ev, obj) = Event::parse_line(line).expect("valid event");
        for key in [
            "seq", "time", "kind", "actor", "role", "store", "via", "hash",
        ] {
            assert!(
                obj.get(key).is_some(),
                "missing envelope key `{key}` on seq {}",
                ev.seq
            );
        }
    }
}

#[test]
fn write_begin_carries_verb_labels_and_file_count() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("lease");
    let intent = sample_intent("new claim", 10, 10);
    let ws = sample_write_set();
    let expected_sha = Sha256::parse(&ws.files[0].sha256).expect("sha");

    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    journal.end(token, Word::new_static("ok")).expect("end");
    assert!(!state_dir.join("pending.toml").exists());

    // After pending.toml cleanup, completed after-hash remains retrievable from the journal.
    assert_eq!(
        journal
            .completed_after_hash("corpus/registers/claims.csv")
            .expect("completed_after_hash"),
        Some(expected_sha)
    );

    // Changing the matching write.end outcome to `failed` removes completed-write provenance.
    let raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    let mut events: Vec<Event> = raw
        .lines()
        .map(|l| Event::parse_line(l).expect("parse").0)
        .collect();
    if let Some(last) = events.last_mut()
        && let EventPayload::WriteEnd { outcome, .. } = &mut last.payload
    {
        *outcome = Word::new_static("failed");
    }
    assert_eq!(
        completed_after_hash(&events, StoreId::Corpus, "corpus/registers/claims.csv"),
        None
    );
}

#[test]
fn write_events_carry_affected_records() {
    let raw = std::fs::read_to_string(state_mini().join("audit/journal.jsonl")).expect("read");
    let events: Vec<Event> = raw
        .lines()
        .map(|l| Event::parse_line(l).expect("parse").0)
        .collect();

    let begin_3 = &events[2];
    let end_4 = &events[3];
    let expected = vec![
        RecordRef::parse("corpus/S-0001").expect("ref"),
        RecordRef::parse("corpus/S-0001-v1").expect("ref"),
    ];
    match &begin_3.payload {
        EventPayload::WriteBegin { affected, .. } => assert_eq!(affected, &expected),
        other => panic!("expected WriteBegin, got {other:?}"),
    }
    match &end_4.payload {
        EventPayload::WriteEnd { affected, .. } => assert_eq!(affected, &expected),
        other => panic!("expected WriteEnd, got {other:?}"),
    }
}

#[test]
fn journal_open_records_writer_and_schema() {
    let raw = std::fs::read_to_string(state_mini().join("audit/journal.jsonl")).expect("read");
    let first = raw.lines().next().expect("line 1");
    let (ev, _) = Event::parse_line(first).expect("parse line 1");
    match ev.payload {
        EventPayload::JournalOpen {
            writer,
            event_schema,
            stores,
            ..
        } => {
            assert_eq!(writer.value(), 0);
            assert_eq!(event_schema.value(), 1);
            assert!(stores.is_empty());
        }
        other => panic!("expected JournalOpen, got {other:?}"),
    }
}

#[test]
fn principal_comes_from_the_world() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());
    let custom = Principal::parse("os:CORP\\alice").expect("valid principal");
    world.set_principal(custom.clone());

    let _journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    let first = raw.lines().next().expect("line 1");
    let (ev, _) = Event::parse_line(first).expect("parse");
    assert_eq!(ev.envelope.actor, custom);
}

#[test]
fn verify_runs_on_a_broken_journal() {
    let fb0800_dir = repo_root().join("tests/fixtures/state-known-bad/FB0800");
    let world = TestWorld::with_state(&corpus_mini(), &fb0800_dir);

    let out_verify = run_args(&["audit", "verify", "--json"], &world);
    assert_eq!(out_verify.code, 1);
    let v_json: Value = serde_json::from_str(&out_verify.stdout).expect("json");
    assert_eq!(v_json["state"], fieldbook_core::strings::cite_statuses()[0]);
    assert_eq!(v_json["events"], 12);

    let out_show = run_args(&["audit", "show", "--last", "3", "--json"], &world);
    assert_eq!(out_show.code, 0);
    let s_json: Value = serde_json::from_str(&out_show.stdout).expect("json");
    assert_eq!(s_json["count"], 12);
    assert_eq!(s_json["shown"], 3);
}

#[test]
fn settings_baseline_survives_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let settings_path = state_dir.join("settings.toml");
    let world = TestWorld::new(tmp.path());

    let count_settings_changes = |dir: &Path| -> Vec<Vec<SettingsGroup>> {
        let raw = std::fs::read_to_string(dir.join("audit/journal.jsonl")).expect("read");
        raw.lines()
            .filter_map(|l| {
                let (ev, _) = Event::parse_line(l).expect("parse");
                match ev.payload {
                    EventPayload::SettingsChange { names, .. } => Some(names),
                    _ => None,
                }
            })
            .collect()
    };

    // 1. Initial open with absent settings.toml records baseline once (names = []).
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 1");
    let changes = count_settings_changes(&state_dir);
    assert_eq!(changes, vec![Vec::<SettingsGroup>::new()]);

    // 2. Unchanged reopen with absent settings.toml records no extra settings.change.
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 2");
    assert_eq!(count_settings_changes(&state_dir).len(), 1);

    // 3. Empty file creation (present: false -> true, names = []) records settings.change.
    write_sync_file(&settings_path, b"").expect("write empty settings.toml");
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 3");
    let changes = count_settings_changes(&state_dir);
    assert_eq!(changes.len(), 2);
    assert!(changes[1].is_empty());

    // 4. Unchanged reopen of empty settings.toml records no extra settings.change.
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 4");
    assert_eq!(count_settings_changes(&state_dir).len(), 2);

    // 5. One changed group (`role = "lead"`) records `names = [Role]`.
    write_sync_file(&settings_path, b"role = \"lead\"\n").expect("write role");
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 5");
    let changes = count_settings_changes(&state_dir);
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[2], vec![SettingsGroup::Role]);

    // 6. Formatting-only change in settings.toml affects file_sha256 and emits `names = []`.
    write_sync_file(&settings_path, b"# comment\nrole   =   \"lead\"\n\n")
        .expect("write formatted role");
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 6");
    let changes = count_settings_changes(&state_dir);
    assert_eq!(changes.len(), 4);
    assert!(changes[3].is_empty());

    // 7. Unchanged reopen after formatting change emits no extra settings.change.
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 6b");
    assert_eq!(count_settings_changes(&state_dir).len(), 4);

    // 8. Other-key change (`custom_key = 42`) records `names = [Other]`.
    write_sync_file(&settings_path, b"role = \"lead\"\ncustom_key = 42\n")
        .expect("write other key");
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 7");
    let changes = count_settings_changes(&state_dir);
    assert_eq!(changes.len(), 5);
    assert_eq!(changes[4], vec![SettingsGroup::Other]);

    // 9. Removal of settings.toml records `names = [Other, Role]`.
    std::fs::remove_file(&settings_path).expect("remove settings.toml");
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 8");
    let changes = count_settings_changes(&state_dir);
    assert_eq!(changes.len(), 6);
    assert_eq!(changes[5], vec![SettingsGroup::Other, SettingsGroup::Role]);

    // 10. Another unchanged reopen records no extra settings.change.
    let _ = state::open(&world, &state_dir, Via::Cli).expect("open 9");
    assert_eq!(count_settings_changes(&state_dir).len(), 6);
}

#[test]
fn settings_baseline_append_failure_keeps_previous() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let settings_path = state_dir.join("settings.toml");
    let world = TestWorld::new(tmp.path());

    let _ = state::open(&world, &state_dir, Via::Cli).expect("initial open");
    write_sync_file(&settings_path, b"role = \"lead\"\n").expect("write role");

    // Inject failure when appending `settings.change`.
    set_fail_on_settings_change_append(true);
    let err = state::open(&world, &state_dir, Via::Cli)
        .expect_err("injected settings.change failure must fail open");
    assert_eq!(err.exit_code(), 3);

    // Next open sees the previous baseline and records `settings.change` for `role`.
    let _ = state::open(&world, &state_dir, Via::Cli).expect("recovery open");
    let raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    let last_line = raw.lines().last().expect("last line");
    let (ev, _) = Event::parse_line(last_line).expect("parse last line");
    match ev.payload {
        EventPayload::SettingsChange { names, .. } => {
            assert_eq!(names, vec![SettingsGroup::Role]);
        }
        other => panic!("expected SettingsChange on retry, got {other:?}"),
    }
}

#[test]
fn verify_100000_events() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let state_dir = tmp.path().join("state");
    let audit_dir = state_dir.join("audit");
    std::fs::create_dir_all(&audit_dir).expect("create audit dir");

    let env = EventEnvelope {
        actor: Principal::local(),
        role: fieldbook_core::audit::Role::Lead,
        store: StoreId::Corpus,
        via: Via::Cli,
    };
    let at = Now::new(CivilDate::from_ymd(2026, 3, 1).expect("date"), 43_200).expect("now");
    let payload = EventPayload::ChatSession {
        phase: Word::new_static("start"),
        calls: Count::new(0),
    };

    let mut buf = String::with_capacity(100_000 * 180);
    let mut prev = ZERO_HASH.to_owned();
    for seq in 1..=100_000u64 {
        let (line, hash) = payload.format_line(seq, at, &env, &prev);
        buf.push_str(&line);
        buf.push('\n');
        prev = hash.as_str().to_owned();
    }
    std::fs::write(audit_dir.join("journal.jsonl"), buf.as_bytes()).expect("write 100k");

    let world = TestWorld::with_state(&corpus_mini(), &state_dir);
    let start = Instant::now();
    let out = run_args(&["audit", "verify"], &world);
    let elapsed = start.elapsed();
    println!(
        "verify_100000_events: {:.3} s (target <= 2.0 s)",
        elapsed.as_secs_f64()
    );

    assert_eq!(out.code, 0);
    assert!(
        elapsed <= Duration::from_secs(20),
        "verify_100000_events took {:.3} s (ceiling 20 s)",
        elapsed.as_secs_f64()
    );
}

const APPEND_SAMPLES: usize = 100;

fn append_percentile(samples: &mut [Duration; APPEND_SAMPLES]) -> Duration {
    samples.sort_unstable();
    // The nearest-rank 95th percentile counts the first sample as rank one.
    samples[(samples.len() * 95).div_ceil(100) - 1]
}

#[test]
fn append_percentile_counts_slow_writes_at_the_boundary() {
    let fast = Duration::from_millis(10);
    let slow = Duration::from_millis(250);
    for (slow_count, expected) in [(5, fast), (6, slow)] {
        let mut samples = [fast; APPEND_SAMPLES];
        samples[..slow_count].fill(slow);
        assert_eq!(append_percentile(&mut samples), expected);
    }
}

#[test]
fn append_p95() {
    let tmp = if cfg!(unix) && Path::new("/dev/shm").is_dir() {
        tempfile::tempdir_in("/dev/shm").unwrap_or_else(|_| tempfile::tempdir().expect("tempdir"))
    } else {
        tempfile::tempdir().expect("tempdir")
    };
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());
    let journal = state::open(&world, &state_dir, Via::Cli).expect("open");

    let mut samples = [Duration::ZERO; APPEND_SAMPLES];
    for sample in &mut samples {
        let start = Instant::now();
        journal
            .refuse(
                Some("add"),
                Code::Fb0410,
                Word::new_static("label"),
                &RefuseAbout::default(),
            )
            .expect("append refuse");
        *sample = start.elapsed();
    }
    let p95 = append_percentile(&mut samples);
    println!(
        "append_p95: {:.2} ms (target <= 20.0 ms), sorted samples (microseconds): {:?}",
        p95.as_secs_f64() * 1000.0,
        samples.map(|d| d.as_micros())
    );
    assert!(
        p95 <= Duration::from_millis(200),
        "append_p95 was {:.2} ms (ceiling 200 ms)",
        p95.as_secs_f64() * 1000.0
    );
}

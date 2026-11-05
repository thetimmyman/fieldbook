//! Integration tests for coherent store reads, `StoreLease`, `state/pending.toml`,
//! and crash-recovery (`audit-journal`).

#![allow(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests coordinate threads and prepare temporary Corpus directories"
)]

mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use fieldbook::World;
use fieldbook::io::journal::{
    JournalError, Recovery, acquire_read_lease, set_fail_after_end_append,
    set_fail_before_begin_append, set_init_pause_hook, set_lock_timeout_for_test,
    set_no_state_read_hook, write_sync_file,
};
use fieldbook::io::state;
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    Event, EventPayload, Kind, LabelSet, Now, OperationIntent, OperationKey, PendingKey,
    PendingWrite, RecordRef, Sha256, StoreUid, Verb, Via, Word, WriteFile, WriteSet,
    request_sha256,
};

use support::world::TestWorld;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_mini() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
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

fn make_intent(verb_str: &str, subj_byte: u8, req_byte: u8) -> OperationIntent {
    let verb = Verb::parse(verb_str).expect("valid verb");
    let subject_sha256 = Sha256::parse(&format!("{subj_byte:02x}").repeat(32)).expect("sha256");
    let request_sha256 = request_sha256(&[("req", &[req_byte])]).expect("request_sha256");
    OperationIntent {
        schema: 1,
        key: OperationKey::new(verb, subject_sha256),
        request_sha256,
        allocated: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("valid ref")],
        allocated_store_uid: None,
        planned_on: CivilDate::from_ymd(2026, 3, 1).expect("valid date"),
    }
}

#[test]
fn journal_only_initialization_allows_a_later_read() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    // Initializing the journal without performing a store write leaves no pending descriptor
    // and allows subsequent coherent reads.
    let _journal = state::open(&world, &state_dir, Via::Cli).expect("open journal");
    assert!(!state_dir.join("pending.toml").exists());

    let tree = world
        .read_tree()
        .expect("read_tree after journal-only open");
    assert!(tree.get_bytes("corpus/corpus.toml").is_some());
}

#[test]
fn readers_never_observe_half_a_batch() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let questions_rel = "corpus/registers/questions.csv";
    let claims_path = tmp.path().join(claims_rel);
    let questions_path = tmp.path().join(questions_rel);

    let old_claims = std::fs::read(&claims_path).expect("read old claims");
    let old_questions = std::fs::read(&questions_path).expect("read old questions");

    let mut new_claims = old_claims.clone();
    new_claims.extend_from_slice(b"# batch-marker-1\n");
    let mut new_questions = old_questions.clone();
    new_questions.extend_from_slice(b"# batch-marker-2\n");

    let claims_after_sha = Sha256::digest(&new_claims);
    let questions_after_sha = Sha256::digest(&new_questions);

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![
            WriteFile::new(claims_rel, claims_after_sha.as_str().to_owned()),
            WriteFile::new(questions_rel, questions_after_sha.as_str().to_owned()),
        ],
        affected: vec![
            RecordRef::parse("corpus/CL-CMP-0001").expect("ref"),
            RecordRef::parse("corpus/Q-001").expect("ref"),
        ],
    };
    let intent = make_intent("add", 0x10, 0x10);

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("write lease");
    let token = journal.begin(&lease, &intent, &ws).expect("begin");

    // Write file 1 only; pause before writing file 2.
    write_sync_file(&claims_path, &new_claims).expect("write file 1");

    // Set a short lock timeout so a reader racing while the writer holds the exclusive store lease
    // refuses promptly rather than observing half-written files; a reader that runs after the writer
    // finishes observes both new files together.
    let root_mid = tmp.path().to_path_buf();
    let mid_reader = std::thread::spawn(move || {
        set_lock_timeout_for_test(Some(Duration::from_millis(80)));
        let r_world = TestWorld::new(&root_mid);
        let res = r_world.read_tree();
        set_lock_timeout_for_test(None);
        res
    });
    let mid_res = mid_reader.join().expect("join mid_reader");
    assert!(
        mid_res.is_err(),
        "reader racing mid-batch while write lease is held must not observe half a batch"
    );

    // Now write file 2, complete the transaction, and release the write lease.
    write_sync_file(&questions_path, &new_questions).expect("write file 2");
    journal.end(token, Word::new_static("ok")).expect("end");
    drop(lease);

    let after_tree = world.read_tree().expect("read_tree after batch");
    assert_eq!(
        after_tree.get_bytes(claims_rel),
        Some(new_claims.as_slice())
    );
    assert_eq!(
        after_tree.get_bytes(questions_rel),
        Some(new_questions.as_slice())
    );
}

#[test]
fn first_write_and_read_share_the_lock() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    assert!(!state_dir.exists());

    let claims_rel = "corpus/registers/claims.csv";
    let claims_path = tmp.path().join(claims_rel);
    let mut new_claims = std::fs::read(&claims_path).expect("read claims");
    new_claims.extend_from_slice(b"# concurrent-first-writer\n");
    let after_sha = Sha256::digest(&new_claims);

    let root_for_writer = tmp.path().to_path_buf();
    let claims_for_writer = claims_path.clone();
    let new_claims_for_writer = new_claims.clone();

    set_no_state_read_hook(Some(Arc::new(move |s_dir: &Path| {
        // Triggered once by the reader after it loaded the initial no-state snapshot and before
        // it re-checks `!state_dir.exists()`. Run the first writer to completion here.
        set_no_state_read_hook(None);
        let w_world = TestWorld::new(&root_for_writer);
        let mut journal = state::open(&w_world, s_dir, Via::Cli).expect("writer open");
        let lease = w_world.write_lease().expect("writer lease");
        let intent = make_intent("add", 0x20, 0x20);
        let ws = WriteSet {
            labels: LabelSet::public(),
            files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
            affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
        };
        let token = journal.begin(&lease, &intent, &ws).expect("begin");
        write_sync_file(&claims_for_writer, &new_claims_for_writer).expect("write claims");
        journal.end(token, Word::new_static("ok")).expect("end");
        drop(lease);
    })));

    let r_world = TestWorld::new(tmp.path());
    let tree = r_world
        .read_tree()
        .expect("reader discards pre-state snapshot and retries under store.lock");
    set_no_state_read_hook(None);

    assert_eq!(tree.get_bytes(claims_rel), Some(new_claims.as_slice()));
}

#[test]
fn pending_write_needs_recovery() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let claims_path = tmp.path().join(claims_rel);
    let mut new_claims = std::fs::read(&claims_path).expect("read");
    new_claims.extend_from_slice(b"# crashed-write\n");
    let after_sha = Sha256::digest(&new_claims);

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("write lease");
    let intent = make_intent("add", 0x30, 0x30);
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    write_sync_file(&claims_path, &new_claims).expect("write");

    // Simulate an interrupted process: drop token and lease without calling `end`.
    drop(token);
    drop(lease);
    assert!(state_dir.join("pending.toml").is_file());

    let err = world
        .read_tree()
        .expect_err("ordinary read_tree must refuse while pending.toml exists");
    assert!(
        err.to_string().contains("pending"),
        "unexpected error: {err}"
    );
}

#[test]
fn cancel_releases_read_lease() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());
    let _journal = state::open(&world, &state_dir, Via::Cli).expect("open");

    {
        let read_lease =
            acquire_read_lease(tmp.path(), &state_dir).expect("acquire shared read lease");
        let _tree = world
            .read_tree_under(&read_lease)
            .expect("read_tree_under shared lease");
        // Drop `read_lease` (simulating cancellation).
    }

    let write_lease = world
        .write_lease()
        .expect("write_lease succeeds immediately after read lease is dropped");
    drop(write_lease);
}

#[test]
fn read_under_write_lease_does_not_relock() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());
    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");

    let lease = world.write_lease().expect("exclusive write lease");
    let tree_before = world
        .read_tree_under(&lease)
        .expect("read_tree_under exclusive lease does not relock");
    assert!(tree_before.get_bytes("corpus/corpus.toml").is_some());

    // Even after `begin` writes `pending.toml`, `read_tree_under(&lease)` with the exclusive
    // lease succeeds for the recovery/transaction holder.
    let intent = make_intent("add", 0x40, 0x40);
    let claims_rel = "corpus/registers/claims.csv";
    let after_sha = Sha256::digest(b"updated\n");
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    let tree_during = world
        .read_tree_under(&lease)
        .expect("read_tree_under exclusive lease while pending");
    assert!(tree_during.get_bytes(claims_rel).is_some());
    journal
        .end(token, Word::new_static("failed"))
        .expect("end failed");
}

#[test]
fn foreign_lease_is_refused() {
    let tmp_a = tempfile::tempdir().expect("tempdir a");
    let tmp_b = tempfile::tempdir().expect("tempdir b");
    copy_dir_all(&corpus_mini(), tmp_a.path());
    copy_dir_all(&corpus_mini(), tmp_b.path());

    let state_a = tmp_a.path().join(".fieldbook");
    let state_b = tmp_b.path().join(".fieldbook");
    let world_a = TestWorld::new(tmp_a.path());
    let world_b = TestWorld::new(tmp_b.path());

    let mut journal_a = state::open(&world_a, &state_a, Via::Cli).expect("open a");
    let _journal_b = state::open(&world_b, &state_b, Via::Cli).expect("open b");

    let lease_b = world_b.write_lease().expect("lease b");
    assert!(world_a.read_tree_under(&lease_b).is_err());

    let intent = make_intent("add", 0x50, 0x50);
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/registers/claims.csv",
            Sha256::digest(b"x").as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };

    assert!(matches!(
        journal_a.begin(&lease_b, &intent, &ws),
        Err(JournalError::InvalidLease(_))
    ));
    assert!(matches!(
        journal_a.state_scope(&lease_b),
        Err(JournalError::InvalidLease(_))
    ));
    assert!(matches!(
        journal_a.pending_key(&lease_b),
        Err(JournalError::InvalidLease(_))
    ));
    assert!(matches!(
        journal_a.pending_intent(&lease_b, &intent.key, intent.request_sha256.clone()),
        Err(JournalError::InvalidLease(_))
    ));
    assert!(matches!(
        journal_a.recover(&lease_b, &intent.key, intent.request_sha256.clone(), &ws),
        Err(JournalError::InvalidLease(_))
    ));
}

#[test]
fn store_precedes_journal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("store lease first");
    let scope_token = journal
        .state_scope(&lease)
        .expect("journal lock after store lease succeeds");

    // Drop the store lease while still holding the journal lock (`scope_token`), then try to
    // acquire a store lease on the same thread: lock-ordering check must refuse.
    drop(lease);
    let err = world
        .write_lease()
        .expect_err("acquiring store.lock while holding journal lock must fail");
    assert!(
        err.to_string().contains("lock order violation"),
        "unexpected error: {err}"
    );
    let read_err = world
        .read_tree()
        .expect_err("read_tree while holding journal lock must fail");
    assert!(
        read_err.to_string().contains("lock order violation"),
        "unexpected error: {read_err}"
    );

    drop(scope_token);
    // Once the journal lock is released, acquiring the store lease succeeds again.
    let lease_again = world
        .write_lease()
        .expect("store lease after journal release");
    drop(lease_again);
    let _ = &mut journal;
}

#[test]
fn pending_intent_covers_every_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let questions_rel = "corpus/registers/questions.csv";
    let claims_path = tmp.path().join(claims_rel);
    let questions_path = tmp.path().join(questions_rel);

    let mut new_claims = std::fs::read(&claims_path).expect("read claims");
    new_claims.extend_from_slice(b"# new-row-already-written\n");
    let mut new_questions = std::fs::read(&questions_path).expect("read questions");
    new_questions.extend_from_slice(b"# second-file-not-written-yet\n");

    let claims_sha = Sha256::digest(&new_claims);
    let questions_sha = Sha256::digest(&new_questions);

    let intent = make_intent("new claim", 0x61, 0x61);
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![
            WriteFile::new(claims_rel, claims_sha.as_str().to_owned()),
            WriteFile::new(questions_rel, questions_sha.as_str().to_owned()),
        ],
        affected: vec![
            RecordRef::parse("corpus/CL-CMP-0001").expect("ref"),
            RecordRef::parse("corpus/Q-001").expect("ref"),
        ],
    };

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("lease");
    let token = journal.begin(&lease, &intent, &ws).expect("begin");

    // Write file 1 so its row is already on disk, leave file 2 untouched, and crash.
    write_sync_file(&claims_path, &new_claims).expect("write file 1");
    drop(token);
    drop(lease);

    // Advance the clock past midnight to the next day (`2026-03-02`).
    let next_day = CivilDate::from_ymd(2026, 3, 2).expect("valid date");
    world.set_now(Now::new(next_day, 3_600).expect("valid now"));

    let journal2 = state::open(&world, &state_dir, Via::Cli).expect("reopen");
    let lease2 = world.write_lease().expect("lease 2");
    let pending_op = journal2
        .pending_intent(&lease2, &intent.key, intent.request_sha256.clone())
        .expect("pending_intent")
        .expect("must find pending operation");

    // Both files (including the already-written first file) and the original allocation/date
    // remain intact in the stored plan.
    assert_eq!(pending_op.intent, intent);
    assert_eq!(
        pending_op.intent.planned_on,
        CivilDate::from_ymd(2026, 3, 1).expect("date")
    );
    assert_eq!(pending_op.writes.files.len(), 2);
    assert_eq!(pending_op.writes, ws);
}

#[test]
fn changed_recovery_plan_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let after_sha = Sha256::digest(b"planned-bytes\n");
    let intent = make_intent("add", 0x62, 0x62);
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("lease");
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    drop(token);

    let different_ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            claims_rel,
            Sha256::digest(b"different-bytes\n").as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };

    let err = journal
        .recover(
            &lease,
            &intent.key,
            intent.request_sha256.clone(),
            &different_ws,
        )
        .expect_err("changed recovery plan must be refused");
    assert!(matches!(err, JournalError::InvalidPending(_)));
}

#[test]
fn third_hash_during_recovery_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let claims_path = tmp.path().join(claims_rel);
    let after_sha = Sha256::digest(b"planned-after-bytes\n");
    let intent = make_intent("add", 0x63, 0x63);
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("lease");
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    drop(token);

    // Corrupt `claims.csv` on disk to a third hash (neither before_sha256 nor after_sha256).
    write_sync_file(&claims_path, b"unexpected-third-content\n").expect("write third content");

    let err = journal
        .recover(&lease, &intent.key, intent.request_sha256.clone(), &ws)
        .expect_err("third hash on disk must refuse recovery");
    assert!(matches!(err, JournalError::InvalidPending(_)));
}

#[test]
fn intent_faults_before_begin_and_after_end_recover() {
    // Subcase A: fault after `pending.toml` is written and before `write.begin` is appended.
    {
        let tmp = tempfile::tempdir().expect("tempdir a");
        copy_dir_all(&corpus_mini(), tmp.path());
        let state_dir = tmp.path().join(".fieldbook");
        let world = TestWorld::new(tmp.path());

        let claims_rel = "corpus/registers/claims.csv";
        let after_sha = Sha256::digest(b"after-bytes\n");
        let intent = make_intent("add", 0x64, 0x64);
        let ws = WriteSet {
            labels: LabelSet::public(),
            files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
            affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
        };

        let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
        let lease = world.write_lease().expect("lease");

        set_fail_before_begin_append(true);
        let begin_err = journal
            .begin(&lease, &intent, &ws)
            .expect_err("injected fault before write.begin");
        set_fail_before_begin_append(false);
        assert!(matches!(begin_err, JournalError::Io { .. }));
        assert!(state_dir.join("pending.toml").is_file());

        // Recovery sees no `write.begin` and unchanged before-hashes, discards `pending.toml`,
        // and returns `Recovery::None`.
        let rec = journal
            .recover(&lease, &intent.key, intent.request_sha256.clone(), &ws)
            .expect("recover pre-begin fault");
        assert!(matches!(rec, Recovery::None));
        assert!(!state_dir.join("pending.toml").exists());
        drop(lease);
        assert!(world.read_tree().is_ok());
    }

    // Subcase B: fault after `write.end` is appended and before `generation` / `pending.toml` cleanup.
    {
        let tmp = tempfile::tempdir().expect("tempdir b");
        copy_dir_all(&corpus_mini(), tmp.path());
        let state_dir = tmp.path().join(".fieldbook");
        let world = TestWorld::new(tmp.path());

        let claims_rel = "corpus/registers/claims.csv";
        let claims_path = tmp.path().join(claims_rel);
        let mut after_bytes = std::fs::read(&claims_path).expect("read claims");
        after_bytes.extend_from_slice(b"# post-end-fault-row\n");
        let after_sha = Sha256::digest(&after_bytes);

        let intent = make_intent("add", 0x65, 0x65);
        let ws = WriteSet {
            labels: LabelSet::public(),
            files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
            affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
        };

        let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
        let lease = world.write_lease().expect("lease");
        let token = journal.begin(&lease, &intent, &ws).expect("begin");
        write_sync_file(&claims_path, &after_bytes).expect("write after bytes");

        set_fail_after_end_append(true);
        let end_err = journal
            .end(token, Word::new_static("ok"))
            .expect_err("injected fault after write.end");
        set_fail_after_end_append(false);
        assert!(matches!(end_err, JournalError::Io { .. }));
        assert!(state_dir.join("pending.toml").is_file());

        // Recovery sees `write.end` already recorded and all live files at `after_sha256`;
        // finishes bookkeeping without writing a second `write.end`.
        let rec = journal
            .recover(&lease, &intent.key, intent.request_sha256.clone(), &ws)
            .expect("recover post-end fault");
        match rec {
            Recovery::Completed { intent: rec_intent } => assert_eq!(rec_intent, intent),
            other => panic!("expected Recovery::Completed, got {other:?}"),
        }
        assert!(!state_dir.join("pending.toml").exists());
        assert!(state_dir.join("generation").is_file());
        drop(lease);

        let raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
        let end_count = raw
            .lines()
            .filter(|l| Event::parse_line(l).expect("parse").0.kind() == Kind::WriteEnd)
            .count();
        assert_eq!(end_count, 1, "must not append a duplicate write.end");
        assert!(world.read_tree().is_ok());
    }
}

#[test]
fn changed_request_same_files_refuses_recovery() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let after_sha = Sha256::digest(b"same-output-bytes\n");
    let intent = make_intent("add", 0x71, 0x71);
    let other_req = request_sha256(&[("req", &[0x72])]).expect("other request_sha256");

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("lease");
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    drop(token);

    assert!(matches!(
        journal.pending_intent(&lease, &intent.key, other_req.clone()),
        Err(JournalError::InvalidPending(_))
    ));
    assert!(matches!(
        journal.recover(&lease, &intent.key, other_req, &ws),
        Err(JournalError::InvalidPending(_))
    ));
}

#[test]
fn recovery_keeps_original_allocations() {
    // Also verify StoreUid validation and RecordRef UUID rejection (Step 26).
    assert!(StoreUid::parse("not-a-uuid").is_err());
    assert!(StoreUid::parse("018F0000-0000-7000-8000-000000000001").is_err());
    let valid_uid =
        StoreUid::parse("018f0000-0000-7000-8000-000000000001").expect("valid lowercase uuid");
    assert!(RecordRef::parse("overlay/018f0000-0000-7000-8000-000000000001").is_err());

    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let questions_rel = "corpus/registers/questions.csv";
    let claims_path = tmp.path().join(claims_rel);
    let questions_path = tmp.path().join(questions_rel);

    let mut new_claims = std::fs::read(&claims_path).expect("read claims");
    new_claims.extend_from_slice(b"# allocated-claim\n");
    let mut new_questions = std::fs::read(&questions_path).expect("read questions");
    new_questions.extend_from_slice(b"# allocated-question\n");

    let mut intent = make_intent("new claim", 0x73, 0x73);
    intent.allocated = vec![
        RecordRef::parse("corpus/CL-CMP-0001").expect("ref"),
        RecordRef::parse("corpus/Q-001").expect("ref"),
    ];
    intent.allocated_store_uid = Some(valid_uid.clone());

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![
            WriteFile::new(claims_rel, Sha256::digest(&new_claims).as_str().to_owned()),
            WriteFile::new(
                questions_rel,
                Sha256::digest(&new_questions).as_str().to_owned(),
            ),
        ],
        affected: intent.allocated.clone(),
    };

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("lease");
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    let original_begin_seq = token.begin_seq().expect("begin_seq");

    // Write file 1 only and drop token to simulate interruption.
    write_sync_file(&claims_path, &new_claims).expect("write file 1");
    drop(token);

    // Advance clock to the next day.
    world.set_now(Now::new(CivilDate::from_ymd(2026, 3, 2).expect("date"), 100).expect("now"));

    let pending_op = journal
        .pending_intent(&lease, &intent.key, intent.request_sha256.clone())
        .expect("pending_intent")
        .expect("some pending");
    assert_eq!(pending_op.intent.allocated, intent.allocated);
    assert_eq!(pending_op.intent.allocated_store_uid, Some(valid_uid));
    assert_eq!(pending_op.intent.planned_on, intent.planned_on);

    let rec = journal
        .recover(
            &lease,
            &intent.key,
            intent.request_sha256.clone(),
            &pending_op.writes,
        )
        .expect("recover");
    let audited = match rec {
        Recovery::Resume {
            audited,
            intent: rec_intent,
        } => {
            assert_eq!(rec_intent, intent);
            assert_eq!(audited.begin_seq(), Some(original_begin_seq));
            audited
        }
        other => panic!("expected Recovery::Resume, got {other:?}"),
    };

    // Finish writing file 2 and close the original transaction.
    write_sync_file(&questions_path, &new_questions).expect("write file 2");
    journal
        .end(audited, Word::new_static("ok"))
        .expect("end resumed transaction");
    drop(lease);

    let raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    let events: Vec<Event> = raw
        .lines()
        .map(|l| Event::parse_line(l).expect("parse").0)
        .collect();
    let begins: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind() == Kind::WriteBegin)
        .collect();
    let ends: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind() == Kind::WriteEnd)
        .collect();
    assert_eq!(begins.len(), 1, "must not start a second write.begin");
    assert_eq!(ends.len(), 1, "must close with a single write.end");
    match &ends[0].payload {
        EventPayload::WriteEnd { affected, .. } => assert_eq!(affected, &intent.allocated),
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn all_after_recovery_closes_original_begin() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let claims_rel = "corpus/registers/claims.csv";
    let claims_path = tmp.path().join(claims_rel);
    let mut new_claims = std::fs::read(&claims_path).expect("read");
    new_claims.extend_from_slice(b"# all-after-written\n");
    let after_sha = Sha256::digest(&new_claims);

    let intent = make_intent("add", 0x74, 0x74);
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(claims_rel, after_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");
    let lease = world.write_lease().expect("lease");
    let token = journal.begin(&lease, &intent, &ws).expect("begin");

    // Write all planned files, then drop `token` before `journal.end`.
    write_sync_file(&claims_path, &new_claims).expect("write all planned files");
    drop(token);

    let rec = journal
        .recover(&lease, &intent.key, intent.request_sha256.clone(), &ws)
        .expect("recover all-after");
    match rec {
        Recovery::Completed { intent: rec_intent } => assert_eq!(rec_intent, intent),
        other => panic!("expected Recovery::Completed, got {other:?}"),
    }
    assert!(!state_dir.join("pending.toml").exists());
    assert!(state_dir.join("generation").is_file());
    drop(lease);

    let raw = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("read");
    let kinds: Vec<Kind> = raw
        .lines()
        .map(|l| Event::parse_line(l).expect("parse").0.kind())
        .collect();
    assert_eq!(kinds.iter().filter(|&&k| k == Kind::WriteBegin).count(), 1);
    assert_eq!(kinds.iter().filter(|&&k| k == Kind::WriteEnd).count(), 1);
    assert!(world.read_tree().is_ok());
}

#[test]
fn first_writer_initializes_before_store_lock() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    assert!(!state_dir.exists());

    let barrier = Arc::new(Barrier::new(2));
    let b_hook = Arc::clone(&barrier);
    let root_for_reader = tmp.path().to_path_buf();

    set_init_pause_hook(Some(Arc::new(move |s_dir: &Path| {
        set_init_pause_hook(None);
        // At this point `.fieldbook/.init` exists and `store.lock` has not been created yet.
        assert!(s_dir.join(".init").exists());
        assert!(!s_dir.join("store.lock").exists());
        b_hook.wait();
        std::thread::sleep(Duration::from_millis(60));
    })));

    let reader_handle = std::thread::spawn(move || {
        barrier.wait();
        let r_world = TestWorld::new(&root_for_reader);
        // Reader waits for `.init` to clear and never creates lock files itself.
        r_world.read_tree()
    });

    let w_world = TestWorld::new(tmp.path());
    let lease = w_world
        .write_lease()
        .expect("first writer initializes state and acquires store.lock");
    let mut journal =
        state::open(&w_world, &state_dir, Via::Cli).expect("open journal under store lease");
    let intent = make_intent("add", 0x81, 0x81);
    let claims_rel = "corpus/registers/claims.csv";
    let claims_path = tmp.path().join(claims_rel);
    let mut new_claims = std::fs::read(&claims_path).expect("read");
    new_claims.extend_from_slice(b"# first-writer-row\n");
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            claims_rel,
            Sha256::digest(&new_claims).as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    write_sync_file(&claims_path, &new_claims).expect("write");
    journal.end(token, Word::new_static("ok")).expect("end");
    drop(lease);

    let reader_tree = reader_handle
        .join()
        .expect("join reader")
        .expect("reader observes coherent tree");
    set_init_pause_hook(None);
    assert!(reader_tree.get_bytes(claims_rel).is_some());
}

#[test]
fn pending_key_requires_the_exclusive_matching_lease() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&corpus_mini(), tmp.path());
    let state_dir = tmp.path().join(".fieldbook");
    let world = TestWorld::new(tmp.path());

    let mut journal = state::open(&world, &state_dir, Via::Cli).expect("open");

    // 1. Shared read lease is refused.
    let shared_lease = acquire_read_lease(tmp.path(), &state_dir).expect("shared lease");
    assert!(matches!(
        journal.pending_key(&shared_lease),
        Err(JournalError::InvalidLease(_))
    ));
    drop(shared_lease);

    // 2. Exclusive matching lease with no `pending.toml` returns `Ok(None)`.
    let lease = world.write_lease().expect("exclusive lease");
    assert_eq!(journal.pending_key(&lease).expect("pending_key none"), None);

    // 3. Present valid descriptor returns `Ok(Some(PendingKey))`.
    let intent = make_intent("add", 0x91, 0x91);
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/registers/claims.csv",
            Sha256::digest(b"pending-key-test\n").as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/CL-CMP-0001").expect("ref")],
    };
    let token = journal.begin(&lease, &intent, &ws).expect("begin");
    drop(token);

    let pk = journal
        .pending_key(&lease)
        .expect("pending_key some")
        .expect("some pending key");
    assert_eq!(
        pk,
        PendingKey {
            key: intent.key.clone(),
            request_sha256: intent.request_sha256.clone(),
        }
    );

    // 4. Malformed `pending.toml` returns `Err(JournalError::InvalidPending(_))`.
    let pending_path = state_dir.join("pending.toml");
    let raw_valid = std::fs::read_to_string(&pending_path).expect("read valid pending.toml");
    let parsed_pw = PendingWrite::parse_toml(&raw_valid).expect("round-trip PendingWrite");
    assert_eq!(parsed_pw.intent, intent);

    write_sync_file(&pending_path, b"schema = 99\n").expect("write malformed pending.toml");
    assert!(matches!(
        journal.pending_key(&lease),
        Err(JournalError::InvalidPending(_))
    ));
}

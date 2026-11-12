//! Integration tests for the backup subsystem (foundation: place, settings, rename/remove, journal).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests create temporary fixture directories and assert test conditions"
)]

mod support;

use std::path::Path;
use std::time::Duration;

use fieldbook::World;
use fieldbook::io::Disk;
use fieldbook::io::atomic::{write_in_root, write_outside};
use fieldbook::io::dirs::{Removable, remove_outside, rename_dir_outside};
use fieldbook::io::journal::{
    Journal, JournalError, acquire_write_lease, set_lock_timeout_for_test,
};
use fieldbook::io::root::Roots;
use fieldbook::io::state::{BackupSettings, read_backup_settings};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    BackupVerified, Count, EventPayload, LabelSet, Now, RefuseAbout, Sha256, Via, Word,
};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{
    Decision, Delivery, Dest, Operation, Shipped, Target, Transfer, check, hash_tree_manifest,
};
use fieldbook_core::labels::PlaceFacts;
use fieldbook_core::policy::{EffectivePolicy, FloorKind, OwnerSettings, PolicyState};

use support::world::TestWorld;

fn floor_a_effective() -> EffectivePolicy {
    EffectivePolicy::new(&PolicyState::Floor(FloorKind::A), &OwnerSettings::unset())
}

fn default_now() -> Now {
    let d = CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("{e}"));
    Now::new(d, 43_200).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
#[cfg(not(windows))]
fn the_disk_place_answers_false_off_windows() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let roots = Roots::at_home(tmp.path(), &tmp.path().join("state"));
    let disk = Disk::new(roots);
    let facts = disk.place(tmp.path());
    assert_eq!(facts, PlaceFacts::local());
    assert!(!facts.sync_root);
    assert!(!facts.reparse);
    assert!(!facts.network);

    let sub = tmp.path().join("some/nested/dir");
    let sub_facts = disk.place(&sub);
    assert_eq!(sub_facts, PlaceFacts::local());
}

#[test]
fn the_double_returns_the_facts_a_test_sets() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let world = TestWorld::new(tmp.path());

    let custom_path = tmp.path().join("cloud/folder");
    let facts = PlaceFacts {
        reparse: false,
        sync_root: true,
        network: false,
    };
    world.set_place(&custom_path, facts);

    // Matches exact path
    assert_eq!(world.place(&custom_path), facts);

    // Any other path defaults to local()
    let other_path = tmp.path().join("other/local/path");
    assert_eq!(world.place(&other_path), PlaceFacts::local());

    // FaultyWorld forwards place
    let faulty = support::faulty::FaultyWorld::new(world, support::faulty::Fault::BrokenPrompt);
    assert_eq!(faulty.place(&custom_path), facts);
    assert_eq!(faulty.place(&other_path), PlaceFacts::local());
}

#[test]
fn backup_settings_parses_defaults_and_targets() {
    let empty_cfg = BackupSettings::parse("").expect("parse empty");
    assert_eq!(empty_cfg.default, None);
    assert_eq!(empty_cfg.warn_after_days, 7);
    assert_eq!(empty_cfg.keep_last, 7);
    assert_eq!(empty_cfg.keep_weekly, 4);
    assert!(empty_cfg.targets.is_empty());

    let toml = r#"
[general]
app = "fieldbook"

[backup]
default = "main"
warn_after_days = 14
keep_last = 10
keep_weekly = 8

[[backup.target]]
id = "main"
kind = "folder"
path = "/var/backups"
include = ["corpus", "state"]
company_storage = true

[[backup.target]]
id = "cloud"
kind = "synced"
path = "C:/Users/test/OneDrive/backups"
"#;
    let cfg = BackupSettings::parse(toml).expect("parse custom");
    assert_eq!(cfg.default.as_deref(), Some("main"));
    assert_eq!(cfg.warn_after_days, 14);
    assert_eq!(cfg.keep_last, 10);
    assert_eq!(cfg.keep_weekly, 8);
    assert_eq!(cfg.targets.len(), 2);

    assert_eq!(cfg.targets[0].id, "main");
    assert_eq!(cfg.targets[0].kind, "folder");
    assert_eq!(cfg.targets[0].path, Path::new("/var/backups"));
    assert_eq!(
        cfg.targets[0].include,
        Some(vec!["corpus".to_string(), "state".to_string()])
    );
    assert!(cfg.targets[0].company_storage);

    assert_eq!(cfg.targets[1].id, "cloud");
    assert_eq!(cfg.targets[1].kind, "synced");
    assert_eq!(
        cfg.targets[1].path,
        Path::new("C:/Users/test/OneDrive/backups")
    );
    assert_eq!(cfg.targets[1].include, None);
    assert!(!cfg.targets[1].company_storage);
}

#[test]
fn backup_settings_rejects_unknown_keys_with_fb0812() {
    let bad_backup = r#"
[backup]
extra_option = true
"#;
    let err = BackupSettings::parse(bad_backup).expect_err("unknown key in [backup]");
    assert_eq!(err.code(), Some(Code::Fb0812));
    assert_eq!(err.key(), Some("extra_option"));

    let bad_target = r#"
[[backup.target]]
id = "t1"
kind = "folder"
path = "/backups"
unknown_setting = 99
"#;
    let err = BackupSettings::parse(bad_target).expect_err("unknown key in [[backup.target]]");
    assert_eq!(err.code(), Some(Code::Fb0812));
    assert_eq!(err.key(), Some("unknown_setting"));
}

#[test]
fn read_backup_settings_reads_from_state_dir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let state_dir = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state_dir).expect("create state dir");

    let defaults = read_backup_settings(&state_dir).expect("read absent");
    assert_eq!(defaults.warn_after_days, 7);
    assert!(defaults.targets.is_empty());

    let settings_file = state_dir.join("settings.toml");
    std::fs::write(
        &settings_file,
        r#"
[backup]
warn_after_days = 21
"#,
    )
    .expect("write settings");

    let parsed = read_backup_settings(&state_dir).expect("read present");
    assert_eq!(parsed.warn_after_days, 21);
}

#[test]
fn rename_and_remove_outside_handles_snapshots_and_refuses_invalid_tokens() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    let state = tmp.path().join("state");
    let target = tmp.path().join("target");
    std::fs::create_dir_all(&corpus).expect("create corpus");
    std::fs::create_dir_all(&state).expect("create state");
    std::fs::create_dir_all(&target).expect("create target");

    // Initialize world and journal
    let world = TestWorld::new(&corpus);
    let mut journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    // Prepare a partial snapshot directory: target/fieldbook-backup-0001.partial
    let partial_dir = target.join("fieldbook-backup-0001.partial");
    let final_dir = target.join("fieldbook-backup-0001");
    std::fs::create_dir_all(&partial_dir).expect("create partial dir");
    let file_content = b"backup data file\n";
    std::fs::write(partial_dir.join("data.txt"), file_content).expect("write data");

    // Tree manifest
    let data_sha = Sha256::digest(file_content).to_string();
    let (manifest_sha, total_bytes) =
        hash_tree_manifest(&[("data.txt".to_string(), data_sha, file_content.len() as u64)]);

    // Construct valid rename transfer
    let rename_transfer = Transfer {
        target: Target::Path(partial_dir.clone()),
        operation: Operation::RenameTree {
            to: final_dir.clone(),
        },
        sha256: manifest_sha.clone(),
        bytes: total_bytes,
    };
    let delivery = Delivery {
        entries: vec![rename_transfer],
        markings: None,
    };
    let eff_policy = floor_a_effective();
    let Decision::Allowed(allowed_rename) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed for rename");
    };

    // Begin backup transaction to obtain Audited token
    let policy_sha = Sha256::parse(allowed_rename.policy_hash()).expect("policy sha");
    let _prep = journal
        .prepare_backup_begin(&LabelSet::public(), &policy_sha, 1)
        .expect("prepare backup");
    let audited = journal
        .begin_backup(&allowed_rename, &target, "folder", 1)
        .expect("begin backup");

    // Refusal: token with wrong destination
    let bad_transfer = Transfer {
        target: Target::Path(partial_dir.clone()),
        operation: Operation::RenameTree {
            to: target.join("wrong-dir"),
        },
        sha256: manifest_sha.clone(),
        bytes: total_bytes,
    };
    let bad_delivery = Delivery {
        entries: vec![bad_transfer],
        markings: None,
    };
    let Decision::Allowed(bad_allowed) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&bad_delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed for bad delivery");
    };
    assert!(
        rename_dir_outside(&audited, &bad_allowed, &partial_dir, &final_dir).is_err(),
        "mismatched token destination must be refused"
    );

    // Refusal: path escaping target root or inside store root
    let inside_store = corpus.join("fieldbook-backup-0001.partial");
    assert!(
        rename_dir_outside(&audited, &allowed_rename, &inside_store, &final_dir).is_err(),
        "rename from inside store root must be refused"
    );

    // Success: valid rename
    assert!(partial_dir.exists());
    assert!(!final_dir.exists());
    rename_dir_outside(&audited, &allowed_rename, &partial_dir, &final_dir)
        .expect("rename_dir_outside succeeds");
    assert!(!partial_dir.exists());
    assert!(final_dir.exists());
    assert_eq!(
        std::fs::read(final_dir.join("data.txt")).expect("read"),
        file_content
    );

    // Test remove_outside with Removable::PartialSnapshot
    let partial2 = target.join("fieldbook-backup-0002.partial");
    std::fs::create_dir_all(&partial2).expect("create partial2");
    std::fs::write(partial2.join("temp.txt"), b"temp").expect("write temp");
    let temp_sha = Sha256::digest(b"temp").to_string();
    let (rem_manifest_sha, rem_bytes) =
        hash_tree_manifest(&[("temp.txt".to_string(), temp_sha, 4)]);
    let remove_transfer = Transfer {
        target: Target::Path(partial2.clone()),
        operation: Operation::RemoveTree,
        sha256: rem_manifest_sha,
        bytes: rem_bytes,
    };
    let rem_delivery = Delivery {
        entries: vec![remove_transfer],
        markings: None,
    };
    let Decision::Allowed(allowed_remove) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&rem_delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed for remove");
    };

    remove_outside(
        &audited,
        &allowed_remove,
        &partial2,
        Removable::PartialSnapshot,
    )
    .expect("remove partial snapshot");
    assert!(!partial2.exists());

    // Test remove_outside with Removable::Snapshot (requires backup.toml)
    let snap3 = target.join("fieldbook-backup-0003");
    std::fs::create_dir_all(&snap3).expect("create snap3");
    std::fs::write(snap3.join("data.txt"), b"snapshot data").expect("write snap data");
    let snap_data_sha = Sha256::digest(b"snapshot data").to_string();
    let (snap_manifest_sha, snap_bytes) =
        hash_tree_manifest(&[("data.txt".to_string(), snap_data_sha.clone(), 13)]);

    let snap_transfer = Transfer {
        target: Target::Path(snap3.clone()),
        operation: Operation::RemoveTree,
        sha256: snap_manifest_sha,
        bytes: snap_bytes,
    };
    let snap_delivery = Delivery {
        entries: vec![snap_transfer],
        markings: None,
    };
    let Decision::Allowed(allowed_snap_rem) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&snap_delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    // Fails because backup.toml is missing!
    assert!(
        remove_outside(&audited, &allowed_snap_rem, &snap3, Removable::Snapshot).is_err(),
        "removing snapshot without backup.toml must be refused"
    );

    // Now write backup.toml and update manifest/transfer
    let backup_toml_bytes = b"backup_version = 1\n";
    std::fs::write(snap3.join("backup.toml"), backup_toml_bytes).expect("write backup.toml");
    let btoml_sha = Sha256::digest(backup_toml_bytes).to_string();
    let (full_manifest_sha, full_bytes) = hash_tree_manifest(&[
        (
            "backup.toml".to_string(),
            btoml_sha,
            backup_toml_bytes.len() as u64,
        ),
        ("data.txt".to_string(), snap_data_sha, 13),
    ]);
    let valid_snap_transfer = Transfer {
        target: Target::Path(snap3.clone()),
        operation: Operation::RemoveTree,
        sha256: full_manifest_sha,
        bytes: full_bytes,
    };
    let valid_delivery = Delivery {
        entries: vec![valid_snap_transfer],
        markings: None,
    };
    let Decision::Allowed(allowed_valid_snap) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&valid_delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    remove_outside(&audited, &allowed_valid_snap, &snap3, Removable::Snapshot)
        .expect("remove snapshot with backup.toml succeeds");
    assert!(!snap3.exists());

    // Target root itself is never removable
    assert!(remove_outside(&audited, &allowed_valid_snap, &target, Removable::Snapshot).is_err());
}

#[test]
fn journal_backup_begin_and_end_lifecycle() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).expect("create corpus");
    let target = tmp.path().join("target");
    std::fs::create_dir_all(&target).expect("create target");

    let world = TestWorld::new(&corpus);
    let state = world.state_dir().to_path_buf();
    std::fs::create_dir_all(&state).expect("create state");
    let mut journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    let eff_policy = floor_a_effective();
    let delivery = Delivery {
        entries: vec![Transfer::write_path(target.join("file.txt"), b"bytes")],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };
    let policy_sha = Sha256::parse(allowed.policy_hash()).expect("policy sha");

    // Prepare backup begin (seq 3 because open wrote journal.open + settings.change)
    let prep = journal
        .prepare_backup_begin(&LabelSet::public(), &policy_sha, 1)
        .expect("prepare backup");
    assert_eq!(prep.seq, 3);

    // Begin backup
    let audited = journal
        .begin_backup(&allowed, &target, "folder", 1)
        .expect("begin backup");
    assert_eq!(audited.begin_seq(), Some(3));

    // Verify written line in segment equals prep.bytes
    let seg_path = state.join("audit/journal.jsonl");
    let lines: Vec<String> = std::fs::read_to_string(&seg_path)
        .expect("read segment")
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[2], String::from_utf8_lossy(&prep.bytes).trim_end());

    // End backup
    let manifest_sha = Sha256::digest(b"manifest");
    let fingerprint = Sha256::digest(b"fingerprint");
    journal
        .end_backup(
            audited,
            Word::new_static("ok"),
            10,
            2048,
            manifest_sha,
            prep.seq,
            prep.head,
            fingerprint,
            Word::new_static("folder"),
        )
        .expect("end backup");

    // Chain stays valid
    let events = fieldbook::io::journal::read_valid_events_prefix(&state);
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].kind().as_str(), "journal.open");
    assert_eq!(events[1].kind().as_str(), "settings.change");
    assert_eq!(events[2].kind().as_str(), "backup.begin");
    assert_eq!(events[3].kind().as_str(), "backup.end");
}

#[test]
fn journal_prepare_backup_invalidated_by_changed_head() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).expect("create corpus");
    let target = tmp.path().join("target");
    std::fs::create_dir_all(&target).expect("create target");

    let world = TestWorld::new(&corpus);
    let state = world.state_dir().to_path_buf();
    std::fs::create_dir_all(&state).expect("create state");
    let mut journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    let eff_policy = floor_a_effective();
    let delivery = Delivery {
        entries: vec![Transfer::write_path(target.join("file.txt"), b"bytes")],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };
    let policy_sha = Sha256::parse(allowed.policy_hash()).expect("policy sha");

    // Prepare backup begin for snapshot 1
    let _prep = journal
        .prepare_backup_begin(&LabelSet::public(), &policy_sha, 1)
        .expect("prepare backup");

    // Intervene: append another event, changing the head!
    journal
        .refuse(
            Some("backup"),
            Code::Fb0415,
            Word::new_static("label"),
            &RefuseAbout::default(),
        )
        .expect("refuse event");

    // Try begin_backup with the stale preparation: must fail!
    let err = journal
        .begin_backup(&allowed, &target, "folder", 1)
        .expect_err("begin_backup must fail on changed head");
    assert!(matches!(err, JournalError::InvalidLease(_)));
}

#[test]
fn journal_open_backup_recovery_operates_on_broken_tail() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).expect("create corpus");
    let target = tmp.path().join("target");
    std::fs::create_dir_all(&target).expect("create target");

    let world = TestWorld::new(&corpus);
    let state = world.state_dir().to_path_buf();
    std::fs::create_dir_all(&state).expect("create state");
    let _journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    // Corrupt the journal segment by appending a line whose hash suffix does not match
    let seg_path = state.join("audit/journal.jsonl");
    let original_bytes = std::fs::read(&seg_path).expect("read segment");
    let bad_line = b"{\"seq\":3,\"at\":\"2026-03-01T12:00:00Z\",\"principal\":\"local\",\"role\":\"owner\",\"store\":\"state\",\"via\":\"cli\",\"kind\":\"refuse\",\"code\":\"FB0415\",\"reason\":\"label\",\"hash\":\"0000000000000000000000000000000000000000000000000000000000000000\"}\n";
    let mut corrupted = original_bytes.clone();
    corrupted.extend_from_slice(bad_line);
    std::fs::write(&seg_path, &corrupted).expect("write corrupted segment");

    // Normal Journal::open now fails because the last line's hash does not match
    assert!(Journal::open(&world, &state, Via::Cli).is_err());

    // Acquire store lease for recovery
    let lease = acquire_write_lease(world.tree_root(), &state).expect("acquire write lease");

    // open_backup_recovery succeeds even with broken tail!
    let recovery = Journal::open_backup_recovery(&world, &lease).expect("open_backup_recovery");

    // Setup delivery token
    let eff_policy = floor_a_effective();
    let delivery = Delivery {
        entries: vec![Transfer::write_path(
            target.join("recovered.txt"),
            b"content",
        )],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &eff_policy,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    // Approve delivery
    let audited = recovery.approve(&allowed).expect("recovery approve");
    assert!(audited.is_recovery_only());

    // Nothing was appended to the journal!
    let current_bytes = std::fs::read(&seg_path).expect("read current segment");
    assert_eq!(current_bytes, corrupted);

    // In-root write is refused
    let roots = Roots::at_home(&corpus, &state);
    assert!(
        write_in_root(&roots.state, Path::new("test.txt"), b"in-root", &audited).is_err(),
        "in-root write must be refused for recovery-only token"
    );

    // Outside write is accepted
    write_outside(
        &allowed,
        &target.join("recovered.txt"),
        b"content",
        &audited,
    )
    .expect("outside write succeeds for approved recovery");
    assert_eq!(
        std::fs::read(target.join("recovered.txt")).expect("read"),
        b"content"
    );
}

#[test]
fn journal_open_backup_recovery_refused_on_held_lock() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).expect("create corpus");

    let world = TestWorld::new(&corpus);
    let state = world.state_dir().to_path_buf();
    std::fs::create_dir_all(&state).expect("create state");

    let _journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    let lease = acquire_write_lease(world.tree_root(), &state).expect("acquire write lease");

    // Hold the journal lock exclusively in another handle
    let held_journal = Journal::open(&world, &state, Via::Cli).expect("open held journal");
    let state_scope_token = held_journal.state_scope(&lease).expect("state scope token");

    // Set a very short timeout for test
    set_lock_timeout_for_test(Some(Duration::from_millis(50)));

    let err = Journal::open_backup_recovery(&world, &lease)
        .expect_err("open_backup_recovery must fail when lock is held");
    assert!(matches!(err, JournalError::Busy));

    set_lock_timeout_for_test(None);
    drop(state_scope_token);
}

#[test]
fn journal_backup_verify_prune_and_queries() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    let state = tmp.path().join("state");
    std::fs::create_dir_all(&corpus).expect("create corpus");
    std::fs::create_dir_all(&state).expect("create state");

    let world = TestWorld::new(&corpus);
    let mut journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    let target_sha = Sha256::digest(b"/path/to/target");
    let manifest_sha1 = Sha256::digest(b"manifest-1");
    let manifest_sha2 = Sha256::digest(b"manifest-2");

    // Initially no verifications
    assert_eq!(
        journal.newest_successful_snapshot_verify(&target_sha, 1, &manifest_sha1),
        None
    );
    assert_eq!(
        journal.newest_snapshot_verify(&target_sha, 1, &manifest_sha1),
        None
    );
    assert_eq!(journal.newest_successful_snapshot_verify_any(), None);

    // Append a successful verify for snapshot 1
    journal
        .append_backup_verify(
            Word::new_static("ok"),
            BackupVerified::Snapshot {
                target_sha256: target_sha.clone(),
                snapshot: Count::new(1),
                manifest_sha256: manifest_sha1.clone(),
            },
        )
        .expect("append verify");

    let time_succ = journal
        .newest_successful_snapshot_verify(&target_sha, 1, &manifest_sha1)
        .expect("successful verify time");
    let (outcome, time_latest) = journal
        .newest_snapshot_verify(&target_sha, 1, &manifest_sha1)
        .expect("latest verify");
    assert_eq!(outcome.as_str(), "ok");
    assert_eq!(time_succ, time_latest);
    assert_eq!(
        journal.newest_successful_snapshot_verify_any(),
        Some(time_succ)
    );

    // Append a failed verify for the same snapshot 1
    journal
        .append_backup_verify(
            Word::new_static("failed"),
            BackupVerified::Snapshot {
                target_sha256: target_sha.clone(),
                snapshot: Count::new(1),
                manifest_sha256: manifest_sha1.clone(),
            },
        )
        .expect("append failed verify");

    // Latest verify now reports failed
    let (outcome_after_fail, _) = journal
        .newest_snapshot_verify(&target_sha, 1, &manifest_sha1)
        .expect("latest verify after fail");
    assert_eq!(outcome_after_fail.as_str(), "failed");

    // BUT newest_successful_snapshot_verify STILL reports the historical success! (Step 26)
    assert_eq!(
        journal.newest_successful_snapshot_verify(&target_sha, 1, &manifest_sha1),
        Some(time_succ)
    );

    // Mismatched manifest sha returns None
    assert_eq!(
        journal.newest_successful_snapshot_verify(&target_sha, 1, &manifest_sha2),
        None
    );

    // Prune helper
    journal
        .append_backup_prune(&[1, 2], Word::new_static("retention"))
        .expect("append prune");

    // RestoreEnd query
    assert_eq!(journal.newest_restore_end(), None);
    let _ = journal.append_payload(EventPayload::RestoreEnd {
        snapshot: Count::new(1),
        mode: Word::new_static("drill"),
        outcome: Word::new_static("ok"),
        code: None,
    });
    let drill_info = journal.newest_restore_end().expect("restore end info");
    assert_eq!(drill_info.snapshot, 1);
    assert_eq!(drill_info.mode.as_str(), "drill");
    assert_eq!(drill_info.outcome.as_str(), "ok");
    assert_eq!(drill_info.code, None);
}

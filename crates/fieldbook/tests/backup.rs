//! Integration tests for the backup subsystem (foundation: place, settings, rename/remove, journal).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "integration tests create temporary fixture directories and assert test conditions"
)]

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use fieldbook::World;
use fieldbook::backup::{BackupOptions, run_backup};
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
use fieldbook_core::backup::{BACKUP_TARGET_TEMPLATE, BackupToml, corpus_entries, manifest_sha256};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{
    Decision, Delivery, Dest, Operation, Shipped, Target, Transfer, check, hash_tree_manifest,
};
use fieldbook_core::labels::{OWNER_SUPPLIED, PlaceFacts};
use fieldbook_core::manifest;
use fieldbook_core::policy::{EffectivePolicy, FloorKind, OwnerSettings, Policy, PolicyState};

use support::faulty::{Fault, FaultKind, FaultyWorld};
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

struct TestFixture {
    _tree_dir: tempfile::TempDir,
    _state_dir: tempfile::TempDir,
    target_dir: tempfile::TempDir,
    world: TestWorld,
}

impl TestFixture {
    fn new() -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let corpus_mini = repo.join("tests/fixtures/corpus-mini");
        let state_mini = repo.join("tests/fixtures/state-mini");

        let tree_dir = support::temp_copy(&corpus_mini);
        let state_dir = support::temp_copy(&state_mini);
        let target_dir = tempfile::tempdir().expect("target tempdir");
        let target_path = target_dir.path().to_path_buf();

        let settings_path = state_dir.path().join("settings.toml");
        let settings_text = format!(
            "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
            target_path.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(&settings_path, settings_text).expect("write settings.toml");

        let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
        Self {
            _tree_dir: tree_dir,
            _state_dir: state_dir,
            target_dir,
            world,
        }
    }

    fn tree_root(&self) -> &Path {
        self.world.tree_root()
    }

    fn state_dir(&self) -> &Path {
        self.world.state_dir()
    }

    fn target_path(&self) -> &Path {
        self.target_dir.path()
    }
}

#[test]
fn backup_writes_a_verifiable_snapshot() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);
    assert!(outcome.stdout.contains("backup 1 written:"));

    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    assert!(snap_dir.is_dir(), "snapshot 1 directory must exist");
    assert!(snap_dir.join("backup.toml").is_file());
    assert!(snap_dir.join("SHA256SUMS").is_file());
    assert!(snap_dir.join("data").is_dir());
    assert!(fix.target_path().join("LATEST").is_file());
    assert!(
        !fix.target_path()
            .join("fieldbook-backup-0001.partial")
            .exists()
    );

    // Verify SHA256SUMS
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).expect("read SHA256SUMS");
    for line in sums_text.lines() {
        if line.is_empty() {
            continue;
        }
        let (expected_sha, rel) = line.split_once("  ").expect("two spaces in checksum line");
        let file_bytes = std::fs::read(snap_dir.join(rel)).expect("read copied file");
        let actual_sha = fieldbook_core::hash::sha256_hex(&file_bytes);
        assert_eq!(actual_sha, expected_sha, "checksum mismatch for {rel}");
    }

    // Verify fingerprint
    let toml_text =
        std::fs::read_to_string(snap_dir.join("backup.toml")).expect("read backup.toml");
    let info = BackupToml::parse(&toml_text).expect("parse backup.toml");
    let corpus_pairs = corpus_entries(&sums_text);
    let expected_fp = manifest::fingerprint(&corpus_pairs);
    assert_eq!(info.fingerprint, expected_fp);
}

#[test]
fn latest_is_written_last() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    // First backup succeeds
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome1.code, 0);

    let latest_before =
        std::fs::read_to_string(fix.target_path().join("LATEST")).expect("read LATEST");
    assert!(latest_before.starts_with("1\n"));

    // Plant a hand-made leftover partial folder
    let planted_partial = fix.target_path().join("fieldbook-backup-0002.partial");
    std::fs::create_dir_all(&planted_partial).expect("create planted partial");
    std::fs::write(planted_partial.join("leftover.txt"), b"junk").expect("write leftover");

    // Second backup fails on LATEST write using FaultyWorld
    let world2 = TestWorld::with_state(fix.tree_root(), fix.state_dir());
    let faulty = FaultyWorld::new(
        world2,
        Fault::FailOnName {
            name: "LATEST",
            kind: FaultKind::Io,
        },
    );
    let outcome2 = run_backup(&faulty, &eff, &options);
    assert_eq!(outcome2.code, 3);

    // Assert previous snapshot and LATEST untouched
    let latest_after =
        std::fs::read_to_string(fix.target_path().join("LATEST")).expect("read LATEST");
    assert_eq!(latest_after, latest_before);
    assert!(fix.target_path().join("fieldbook-backup-0001").is_dir());

    // Completed final snapshot 2 exists, its .partial is absent, and the planted partial is gone
    assert!(fix.target_path().join("fieldbook-backup-0002").is_dir());
    assert!(
        !fix.target_path()
            .join("fieldbook-backup-0002.partial")
            .exists()
    );
}

#[test]
fn manifest_is_made_from_the_copied_bytes() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).expect("read SHA256SUMS");
    let toml_text =
        std::fs::read_to_string(snap_dir.join("backup.toml")).expect("read backup.toml");
    let info = BackupToml::parse(&toml_text).expect("parse backup.toml");

    let computed_manifest_sha = manifest_sha256(&info, &sums_text).expect("manifest_sha256");
    assert_eq!(info.manifest_sha256, computed_manifest_sha.as_str());
}

// control: DR-02
#[test]
fn target_inside_the_corpus_is_fb0812() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let bad_target = fix.tree_root().join("corpus/sources");
    let options = BackupOptions {
        target: None,
        to: Some(bad_target),
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 1);
    assert!(outcome.stderr.contains("FB0812"));
}

#[test]
fn synced_path_warns_unless_declared() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    fix.world.set_place(
        fix.target_path(),
        PlaceFacts {
            sync_root: true,
            reparse: false,
            network: false,
        },
    );

    // 1. With kind = "folder", undeclared sync root warns FB0813 and exits 1
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome1.code, 1);
    assert!(outcome1.stderr.contains("FB0813"));

    // 2. Declaring kind = "synced" turns the warning into a notice and exits 0
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"synced-main\"\nkind = \"synced\"\npath = \"{}\"\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");
    let options2 = BackupOptions {
        target: Some("synced-main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome2 = run_backup(&fix.world, &eff, &options2);
    assert_eq!(
        outcome2.code, 0,
        "declaring kind=synced must succeed: {}",
        outcome2.stderr
    );
    assert!(!outcome2.stderr.contains("FB0813"));
}

#[test]
fn dry_run_writes_nothing_and_logs_nothing() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: true,
    };
    let journal_before =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);
    assert!(outcome.stdout.contains("would write snapshot 1:"));

    // Verify nothing written to target
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
    assert!(!fix.target_path().join("LATEST").exists());

    // Verify nothing appended to journal
    let journal_after =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert_eq!(journal_after, journal_before);
}

#[test]
fn backup_without_a_target_exits_2_with_the_template_on_standard_error() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    // Overwrite settings.toml without any targets
    std::fs::write(fix.state_dir().join("settings.toml"), "role = \"lead\"\n")
        .expect("write settings");

    let options = BackupOptions {
        target: None,
        to: None,
        dry_run: false,
    };
    let journal_before =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 2);
    assert!(outcome.stdout.is_empty(), "stdout must be empty");
    assert!(
        outcome
            .stderr
            .contains("no backup target; add this to settings.toml in the state folder:\n")
    );
    assert!(outcome.stderr.contains(BACKUP_TARGET_TEMPLATE));

    // No event appended to journal
    let journal_after =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert_eq!(journal_after, journal_before);
}

#[test]
fn backup_events_are_begin_end_verify_prune() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    // Set keep_last = 1 in settings.toml so third backup prunes first
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[backup]\nkeep_last = 1\nkeep_weekly = 0\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    // Run 1: writes snapshot 1, logs begin and end
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome1.code, 0);

    // Run 2: writes snapshot 2 (2 snapshots kept by rule)
    let outcome2 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome2.code, 0);

    // Run 3: writes snapshot 3, prunes snapshot 1, logs begin, end, and prune
    let outcome3 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome3.code, 0);

    let journal_text =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert!(journal_text.contains("\"kind\":\"backup.begin\""));
    assert!(journal_text.contains("\"kind\":\"backup.end\""));
    assert!(journal_text.contains("\"kind\":\"backup.prune\""));
}

#[test]
fn no_backup_without_a_journal() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let tree_dir = support::temp_copy(&corpus_mini);

    let state_file = tempfile::NamedTempFile::new().expect("named temp file");
    let target_dir = tempfile::tempdir().expect("target tempdir");

    let world = TestWorld::with_state(tree_dir.path(), state_file.path());
    let eff = floor_a_effective();

    let options = BackupOptions {
        target: None,
        to: Some(target_dir.path().to_path_buf()),
        dry_run: false,
    };
    let outcome = run_backup(&world, &eff, &options);
    assert_eq!(outcome.code, 3);
    assert!(!target_dir.path().join("fieldbook-backup-0001").exists());
}

#[test]
fn backup_runs_on_a_refused_journal() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Corrupt the hash of the last line of the journal with a valid-format 64-hex hash that does not match
    let journal_path = fix.state_dir().join("audit/journal.jsonl");
    let content = std::fs::read_to_string(&journal_path).expect("read journal");
    let mut lines: Vec<&str> = content.lines().collect();
    let last = lines.pop().expect("last line");
    let (prefix, _) = last.rsplit_once(',').expect("split comma");
    let corrupted_last = format!(
        "{prefix},\"hash\":\"0000000000000000000000000000000000000000000000000000000000000000\"}}"
    );
    lines.push(&corrupted_last);
    std::fs::write(&journal_path, format!("{}\n", lines.join("\n")))
        .expect("write corrupted journal");
    let journal_before = std::fs::read_to_string(&journal_path).expect("read journal");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 1, "recovery backup must exit 1 (findings)");

    // Snapshot and SHA256SUMS exist
    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    assert!(snap_dir.is_dir());
    assert!(snap_dir.join("SHA256SUMS").is_file());

    // backup.toml records journal_state = "broken"
    let toml_text =
        std::fs::read_to_string(snap_dir.join("backup.toml")).expect("read backup.toml");
    let info = BackupToml::parse(&toml_text).expect("parse backup.toml");
    assert_eq!(info.journal_state, "broken");

    // No event appended to journal
    let journal_after = std::fs::read_to_string(&journal_path).expect("read journal after");
    assert_eq!(journal_after, journal_before);
}

#[test]
fn backup_after_an_earlier_break_records_broken() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let fb0800_state = repo.join("tests/fixtures/state-known-bad/FB0800");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = support::temp_copy(&fb0800_state);
    let target_dir = tempfile::tempdir().expect("target tempdir");
    let target_path = target_dir.path().to_path_buf();

    let settings_path = state_dir.path().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_path.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&world, &eff, &options);
    assert_eq!(outcome.code, 1, "backup after earlier break must exit 1");

    let snap_dir = target_path.join("fieldbook-backup-0001");
    let toml_text =
        std::fs::read_to_string(snap_dir.join("backup.toml")).expect("read backup.toml");
    let info = BackupToml::parse(&toml_text).expect("parse backup.toml");
    assert_eq!(info.journal_state, "broken");

    // begin and end are appended
    let journal_text = std::fs::read_to_string(state_dir.path().join("audit/journal.jsonl"))
        .expect("read journal");
    assert!(journal_text.contains("\"kind\":\"backup.begin\""));
    assert!(journal_text.contains("\"kind\":\"backup.end\""));
}

#[test]
fn backup_is_refused_while_the_lock_is_held() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Acquire write lease / store lock
    let _lease = acquire_write_lease(fix.tree_root(), fix.state_dir()).expect("acquire lease");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 3);
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
}

#[test]
fn unlabelled_item_refuses_the_snapshot() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Plant a page with non-permitted label np-0007
    let bad_page = fix
        .tree_root()
        .join("corpus/pages/estimating/pricing/est-unlabelled.md");
    let content = "+++\nid = \"est-unlabelled\"\ntitle = \"Unlabelled page\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSecret.\n";
    std::fs::write(&bad_page, content).expect("write page");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 2, "FB0901 refusal exits 2");
    assert!(outcome.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());

    // Check refuse event in journal
    let journal_text =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert!(journal_text.contains("\"kind\":\"refuse\""));
    assert!(journal_text.contains("\"dest\":\"backup\""));
}

#[test]
fn a_stray_file_adds_unlabelled() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Plant a stray file in corpus
    let stray = fix.tree_root().join("corpus/stray.txt");
    std::fs::write(&stray, b"random unlabelled content").expect("write stray");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 2);
    assert!(outcome.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
}

#[test]
fn a_derived_folder_is_not_copied() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Plant files in derived folders
    let site_file = fix.tree_root().join("out/edition/index.html");
    std::fs::create_dir_all(site_file.parent().unwrap()).expect("create out");
    std::fs::write(&site_file, b"derived html").expect("write site");

    let drill_file = fix.state_dir().join("drill/drill.toml");
    std::fs::create_dir_all(drill_file.parent().unwrap()).expect("create drill");
    std::fs::write(&drill_file, b"derived drill").expect("write drill");

    let pages_file = fix.state_dir().join("pages/cached.md");
    std::fs::create_dir_all(pages_file.parent().unwrap()).expect("create state pages");
    std::fs::write(&pages_file, b"derived state page").expect("write state page");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).expect("read sums");
    assert!(!sums_text.contains("out/edition"));
    assert!(!sums_text.contains("state/drill"));
    assert!(!sums_text.contains("state/pages"));
    assert!(sums_text.contains("data/corpus/pages/"));
}

#[test]
fn backup_begin_carries_dest_labels_policy() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    let events = fieldbook::io::journal::read_valid_events_prefix(fix.state_dir());
    let begin_event = events
        .iter()
        .rev()
        .find(|e| matches!(e.payload, EventPayload::BackupBegin { .. }))
        .expect("must contain BackupBegin");

    if let EventPayload::BackupBegin {
        dest,
        labels,
        policy,
        snapshot,
    } = &begin_event.payload
    {
        assert_eq!(*dest, fieldbook_core::audit::DestKind::Backup);
        assert!(labels.contains(&OWNER_SUPPLIED));
        assert_eq!(snapshot.0, 1);
        assert!(!policy.as_str().is_empty());
    } else {
        panic!("expected BackupBegin");
    }
}

#[test]
fn backup_toml_records_labels_not_zone() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    let toml_path = fix.target_path().join("fieldbook-backup-0001/backup.toml");
    let text = std::fs::read_to_string(&toml_path).expect("read toml");
    assert!(text.contains("labels = ["));
    assert!(!text.contains("zone"));
    let info = BackupToml::parse(&text).expect("must parse");
    assert!(info.labels.contains(&"owner-supplied".to_string()));
}

#[test]
fn backup_and_restore_use_the_same_fingerprint_input() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).expect("read sums");
    let toml_text = std::fs::read_to_string(snap_dir.join("backup.toml")).expect("read toml");
    let info = BackupToml::parse(&toml_text).expect("parse toml");

    let entries = corpus_entries(&sums_text);
    let expected_fp = manifest::fingerprint(&entries);
    assert_eq!(info.fingerprint, expected_fp);

    let events = fieldbook::io::journal::read_valid_events_prefix(fix.state_dir());
    let end_event = events
        .iter()
        .find(|e| matches!(e.payload, EventPayload::BackupEnd { .. }))
        .expect("must contain BackupEnd");
    if let EventPayload::BackupEnd { fingerprint, .. } = &end_event.payload {
        assert_eq!(fingerprint.as_str(), expected_fp);
    } else {
        panic!("expected BackupEnd");
    }
}

#[test]
fn floor_allows_public_and_owner_supplied() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);
    assert!(fix.target_path().join("fieldbook-backup-0001").is_dir());
}

#[test]
fn target_outside_every_backup_row_is_fb0918() {
    let fix = TestFixture::new();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let policy_path = repo.join("tests/fixtures/policy/policy.toml");
    let policy_toml = std::fs::read_to_string(policy_path).expect("read policy");
    let policy = Policy::parse(&policy_toml, None).expect("parse policy");
    let state = PolicyState::Loaded(policy);
    let eff = EffectivePolicy::new(&state, &OwnerSettings::unset());

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome.code, 2,
        "target outside policy rows must exit 2 (FB0918)"
    );
    assert!(outcome.stderr.contains("FB0918"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());

    // Refuse event has dest=backup and reason=no-row
    let journal_text =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert!(journal_text.contains("\"kind\":\"refuse\""));
    assert!(journal_text.contains("\"reason\":\"no-row\""));
}

#[test]
fn journal_only_target_holds_no_content() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Set include = ["audit"]
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"audit-only\"\nkind = \"folder\"\npath = \"{}\"\ninclude = [\"audit\"]\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let options = BackupOptions {
        target: Some("audit-only".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).expect("read sums");
    assert!(sums_text.contains("data/state/audit/"));
    assert!(!sums_text.contains("data/corpus/"));
    assert!(!sums_text.contains("data/state/settings.toml"));
}

#[test]
#[ignore = "takes > 20s in unoptimized debug test profile"]
fn backup_200_mb() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Create 200 MB of synthetic public files (e.g. 10 files of 20 MB each)
    let chunk_size = 20 * 1024 * 1024;
    let chunk = vec![0x42u8; chunk_size];
    let sources_dir = fix.tree_root().join("corpus/sources/public");
    for i in 0..10 {
        std::fs::write(sources_dir.join(format!("large-{i}.bin")), &chunk)
            .expect("write 20MB file");
    }

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let start = std::time::Instant::now();
    let outcome = run_backup(&fix.world, &eff, &options);
    let elapsed = start.elapsed();
    println!("backup 200 MB took {elapsed:?}");

    assert_eq!(
        outcome.code, 0,
        "200 MB backup must succeed: {}",
        outcome.stderr
    );
    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    assert!(snap_dir.is_dir());
}

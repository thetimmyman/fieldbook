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
use fieldbook::backup::{
    BackupListArgs, BackupOptions, BackupVerifyArgs, backup_age, run_backup, run_list, run_verify,
};
use fieldbook::cli::AddArgs;
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
use fieldbook_core::report::{BackupList, LastBackupReport};

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

// control: DR-02
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

    // Run verify on snapshot 3: logs verify
    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(3),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let outcome_v = run_verify(&fix.world, &verify_args);
    assert_eq!(outcome_v.code, 0, "verify failed: {}", outcome_v.stderr);

    let journal_text =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert!(journal_text.contains("\"kind\":\"backup.begin\""));
    assert!(journal_text.contains("\"kind\":\"backup.end\""));
    assert!(journal_text.contains("\"kind\":\"backup.verify\""));
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

// control: ED-04
#[test]
fn verify_names_a_changed_byte() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    // Corrupt one byte in a copied file
    let corrupt_file = fix
        .target_path()
        .join("fieldbook-backup-0001/data/corpus/corpus.toml");
    let content = std::fs::read_to_string(&corrupt_file).expect("read corpus.toml");
    let corrupted = format!("{content}\n# corrupted");
    std::fs::write(&corrupt_file, corrupted).expect("write corrupted file");

    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let verify_outcome = run_verify(&fix.world, &verify_args);
    assert_eq!(verify_outcome.code, 1);
    assert!(verify_outcome.stderr.contains("FB0810"));
    assert!(verify_outcome.stderr.contains("data/corpus/corpus.toml"));

    // Check failed verify event logged
    let journal_text =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert!(journal_text.contains("\"outcome\":\"failed\""));
}

#[test]
fn verify_names_an_inconsistent_manifest() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    // Corrupt SHA256SUMS by deleting a line
    let sums_path = fix.target_path().join("fieldbook-backup-0001/SHA256SUMS");
    let sums_text = std::fs::read_to_string(&sums_path).expect("read sums");
    let mut lines: Vec<&str> = sums_text.lines().collect();
    lines.pop();
    let new_sums = lines.join("\n") + "\n";
    std::fs::write(&sums_path, new_sums).expect("write new sums");

    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let verify_outcome = run_verify(&fix.world, &verify_args);
    assert_eq!(verify_outcome.code, 1);
    assert!(verify_outcome.stderr.contains("FB0811"));
}

#[test]
fn verify_checks_the_journal_head() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    let toml_path = snap_dir.join("backup.toml");
    let sums_path = snap_dir.join("SHA256SUMS");

    let toml_text = std::fs::read_to_string(&toml_path).expect("read backup.toml");
    let mut info = BackupToml::parse(&toml_text).expect("parse backup.toml");

    // Change journal_head to mismatch actual journal at journal_seq
    let fake_head = "0000000000000000000000000000000000000000000000000000000000000000";
    info.journal_head = fake_head.to_string();

    let sums_text = std::fs::read_to_string(&sums_path).expect("read sums");
    let new_manifest_sha = manifest_sha256(&info, &sums_text).expect("calc manifest sha");
    info.manifest_sha256 = new_manifest_sha.as_str().to_string();

    let rendered = info.render();
    std::fs::write(&toml_path, rendered).expect("write backup.toml");

    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let verify_outcome = run_verify(&fix.world, &verify_args);
    assert_eq!(verify_outcome.code, 1);
    assert!(verify_outcome.stderr.contains("FB0802"));
}

#[test]
fn raw_copy_is_checked() {
    let fix = TestFixture::new();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let raw_temp = tempfile::tempdir().expect("raw tempdir");
    let raw_path = raw_temp.path();

    // Copy corpus and state into raw copy
    let corpus_dest = raw_path.join("corpus");
    let state_dest = raw_path.join("state");
    support::copy_dir_recursive(&corpus_mini.join("corpus"), &corpus_dest).expect("copy corpus");
    support::copy_dir_recursive(&state_mini, &state_dest).expect("copy state");

    // 1. Clean raw copy passes
    let verify_clean = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw_path.to_path_buf()),
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let outcome_clean = run_verify(&fix.world, &verify_clean);
    assert_eq!(
        outcome_clean.code, 0,
        "clean raw verify failed: {}",
        outcome_clean.stderr
    );
    assert!(outcome_clean.stdout.contains("raw copy consistent"));

    // Check journal logged BackupVerified::Raw
    let journal_text =
        std::fs::read_to_string(fix.state_dir().join("audit/journal.jsonl")).expect("read journal");
    assert!(journal_text.contains("\"path_sha256\""));

    // 2. Planted FB0211 (remove an original source file)
    let s1 = corpus_dest.join("sources/public/S-0001-v1.txt");
    std::fs::remove_file(&s1).expect("remove S-0001-v1.txt");
    let outcome_211 = run_verify(&fix.world, &verify_clean);
    assert_eq!(outcome_211.code, 1);
    assert!(outcome_211.stderr.contains("FB0211"));

    // Restore S-0001-v1.txt
    std::fs::copy(corpus_mini.join("corpus/sources/public/S-0001-v1.txt"), &s1).expect("restore");

    // 3. Planted FB0212 (corrupt an extraction file)
    let ext1 = corpus_dest.join("sources/text/S-0001-v1/body.txt");
    std::fs::write(&ext1, "corrupted extraction text\n").expect("corrupt extraction");
    let outcome_212 = run_verify(&fix.world, &verify_clean);
    assert_eq!(outcome_212.code, 1);
    assert!(outcome_212.stderr.contains("FB0212"));
}

#[test]
fn stale_backup_warns_fb0814() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Remove seeded backup.verify event from state-mini's journal
    let journal_path = fix.state_dir().join("audit/journal.jsonl");
    let journal_text = std::fs::read_to_string(&journal_path).expect("read journal");
    let lines: Vec<&str> = journal_text
        .lines()
        .filter(|line| !line.contains("\"kind\":\"backup.verify\""))
        .collect();
    std::fs::write(&journal_path, lines.join("\n") + "\n").expect("write journal");

    // Start at 2026-03-01
    let t0 = Now::new(CivilDate::from_ymd(2026, 3, 1).expect("date"), 3600).expect("now");
    fix.world.set_now(t0);

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_outcome = run_verify(&fix.world, &verify_args);
    assert_eq!(v_outcome.code, 0);

    // Advance clock by 10 days (> default warn_after_days = 7)
    let t1 = Now::new(CivilDate::from_ymd(2026, 3, 11).expect("date"), 3600).expect("now");
    fix.world.set_now(t1);

    // Run backup list: warns FB0814 on stderr and exits 1
    let list_args = BackupListArgs {
        target: Some("main".to_string()),
        in_dir: None,
        json: false,
    };
    let l_stale = run_list(&fix.world, &list_args);
    assert_eq!(l_stale.code, 1);
    assert!(l_stale.stderr.contains("FB0814"));

    // Run backup verify: warns FB0814 on stderr and exits 1 (judged before this run's event)
    let v_stale = run_verify(&fix.world, &verify_args);
    assert_eq!(v_stale.code, 1);
    assert!(v_stale.stderr.contains("FB0814"));
}

#[test]
fn none_yet_warns_fb0814_in_list() {
    let fix = TestFixture::new();

    // Remove seeded backup.verify event from state-mini's journal so no verify event exists
    let journal_path = fix.state_dir().join("audit/journal.jsonl");
    let journal_text = std::fs::read_to_string(&journal_path).expect("read journal");
    let lines: Vec<&str> = journal_text
        .lines()
        .filter(|line| !line.contains("\"kind\":\"backup.verify\""))
        .collect();
    std::fs::write(&journal_path, lines.join("\n") + "\n").expect("write journal");

    // Target is configured ("main"), but no backups or verify events exist yet
    let list_args = BackupListArgs {
        target: Some("main".to_string()),
        in_dir: None,
        json: false,
    };
    let outcome = run_list(&fix.world, &list_args);
    assert_eq!(outcome.code, 1);
    assert!(outcome.stdout.contains("Last verified backup: none yet"));
    assert!(outcome.stderr.contains("FB0814"));
}

#[test]
fn no_target_no_snapshot_prints_nothing() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = support::temp_copy(&state_mini);
    // Write empty settings (no backup target)
    std::fs::write(state_dir.path().join("settings.toml"), "role = \"lead\"\n")
        .expect("write settings");

    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
    let list_args = BackupListArgs {
        target: None,
        in_dir: None,
        json: false,
    };
    let outcome = run_list(&world, &list_args);
    assert_eq!(outcome.code, 0);
    assert!(outcome.stdout.is_empty(), "stdout was: {}", outcome.stdout);
    assert!(outcome.stderr.is_empty(), "stderr was: {}", outcome.stderr);
}

#[test]
fn verification_is_scoped_to_target_and_manifest() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Create a second target directory
    let target_b = tempfile::tempdir().expect("target b tempdir");
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n\n[[backup.target]]\nid = \"sec\"\nkind = \"folder\"\npath = \"{}\"\n",
        fix.target_path().to_string_lossy().replace('\\', "/"),
        target_b.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    // Take backup to target main (snapshot 1)
    let opt_main = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome1 = run_backup(&fix.world, &eff, &opt_main);
    assert_eq!(outcome1.code, 0);

    // Take backup to target sec (snapshot 1)
    let opt_sec = BackupOptions {
        target: Some("sec".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome2 = run_backup(&fix.world, &eff, &opt_sec);
    assert_eq!(outcome2.code, 0);

    // Verify snapshot 1 of main
    let v_main = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_out = run_verify(&fix.world, &v_main);
    assert_eq!(v_out.code, 0);

    // List main (json): snapshot 1 is verified
    let l_main = run_list(
        &fix.world,
        &BackupListArgs {
            target: Some("main".to_string()),
            in_dir: None,
            json: true,
        },
    );
    assert_eq!(l_main.code, 0);
    let report_main: BackupList = serde_json::from_str(&l_main.stdout).expect("parse json main");
    assert_eq!(report_main.snapshots.len(), 1);
    assert!(report_main.snapshots[0].verified);

    // List sec (json): snapshot 1 in sec is NOT verified because verification is scoped to target
    let l_sec = run_list(
        &fix.world,
        &BackupListArgs {
            target: Some("sec".to_string()),
            in_dir: None,
            json: true,
        },
    );
    let report_sec: BackupList = serde_json::from_str(&l_sec.stdout).expect("parse json sec");
    assert_eq!(report_sec.snapshots.len(), 1);
    assert!(!report_sec.snapshots[0].verified);

    // Now replace manifest in target main (modify backup.toml to have different manifest_sha256)
    let toml_path = fix.target_path().join("fieldbook-backup-0001/backup.toml");
    let mut toml =
        BackupToml::parse(&std::fs::read_to_string(&toml_path).expect("read toml")).expect("parse");
    toml.manifest_sha256 =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
    std::fs::write(&toml_path, toml.render()).expect("write toml");

    // List main again: snapshot 1 is now NOT verified because manifest_sha256 no longer matches
    let l_main_after = run_list(
        &fix.world,
        &BackupListArgs {
            target: Some("main".to_string()),
            in_dir: None,
            json: true,
        },
    );
    let report_after: BackupList =
        serde_json::from_str(&l_main_after.stdout).expect("parse json main after");
    assert!(!report_after.snapshots[0].verified);
}

#[test]
fn latest_verification_keeps_failure_after_success() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0);

    // 1. Verify successfully
    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v1 = run_verify(&fix.world, &verify_args);
    assert_eq!(v1.code, 0);

    // List and get initial verified_utc
    let l1 = run_list(
        &fix.world,
        &BackupListArgs {
            target: Some("main".to_string()),
            in_dir: None,
            json: true,
        },
    );
    let rep1: BackupList = serde_json::from_str(&l1.stdout).expect("parse json 1");
    assert!(rep1.snapshots[0].verified);
    let initial_verified_utc = rep1.snapshots[0].verified_utc.clone();
    assert!(initial_verified_utc.is_some());

    // 2. Corrupt a data byte without changing backup.toml identity
    let data_file = fix
        .target_path()
        .join("fieldbook-backup-0001/data/corpus/corpus.toml");
    let content = std::fs::read_to_string(&data_file).expect("read file");
    std::fs::write(&data_file, format!("{content}\n# corrupted")).expect("write corrupt");

    // Advance clock
    let t_fail = Now::new(CivilDate::from_ymd(2026, 3, 2).expect("date"), 3600).expect("now");
    fix.world.set_now(t_fail);

    // 3. Verify fails
    let v2 = run_verify(&fix.world, &verify_args);
    assert_eq!(v2.code, 1);
    assert!(v2.stderr.contains("FB0810"));

    // 4. List again: verified_utc remains from earlier success and latest_verification.outcome is failed
    let l2 = run_list(
        &fix.world,
        &BackupListArgs {
            target: Some("main".to_string()),
            in_dir: None,
            json: true,
        },
    );
    let rep2: BackupList = serde_json::from_str(&l2.stdout).expect("parse json 2");
    assert!(rep2.snapshots[0].verified);
    assert_eq!(rep2.snapshots[0].verified_utc, initial_verified_utc);
    assert_eq!(
        rep2.snapshots[0]
            .latest_verification
            .as_ref()
            .map(|v| v.outcome.as_str()),
        Some("failed")
    );
}

#[test]
fn wrote_store_ignores_build() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = support::temp_copy(&state_mini);
    let out_dir = tempfile::tempdir().expect("out dir");
    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());

    // 1. Build leaves wrote_store false
    let build_out = fieldbook::run(
        &[
            "build".to_string(),
            "--out".to_string(),
            out_dir.path().display().to_string(),
        ],
        &world,
    );
    assert_eq!(build_out.code, 0, "build stderr: {}", build_out.stderr);
    assert!(
        !fieldbook::io::journal::wrote_store(),
        "build must leave wrote_store() false"
    );

    // 2. Add leaves wrote_store true
    let input_path = tree_dir.path().join("input.txt");
    std::fs::write(&input_path, b"Synthetic public text for testing.\n").expect("write input");
    let add_out = fieldbook::run(
        &[
            "add".to_string(),
            input_path.display().to_string(),
            "--confirm-public".to_string(),
        ],
        &world,
    );
    assert_eq!(add_out.code, 0, "add stderr: {}", add_out.stderr);
    assert!(
        fieldbook::io::journal::wrote_store(),
        "add must leave wrote_store() true"
    );
}

#[test]
fn write_verb_prints_backup_reminder() {
    let fix = TestFixture::new();

    // 1. Fixed clock at Day 1 (2026-03-01)
    let t_day1 = Now::new(CivilDate::from_ymd(2026, 3, 1).expect("date"), 3600).expect("now");
    fix.world.set_now(t_day1);

    // Run backup and verify on day 1 so valid cache and journal event exist
    let b1 = fieldbook::run(&["backup".to_string()], &fix.world);
    assert_eq!(b1.code, 0, "backup stderr: {}", b1.stderr);
    let v1 = fieldbook::run(
        &[
            "backup".to_string(),
            "verify".to_string(),
            "--target".to_string(),
            "main".to_string(),
        ],
        &fix.world,
    );
    assert_eq!(v1.code, 0, "verify stderr: {}", v1.stderr);

    // Case A: 9 days old under a fixed clock (2026-03-10)
    let t_day10 = Now::new(CivilDate::from_ymd(2026, 3, 10).expect("date"), 3600).expect("now");
    fix.world.set_now(t_day10);

    let input1 = fix.tree_root().join("input1.txt");
    std::fs::write(&input1, b"Synthetic input 1\n").expect("write input 1");
    let add1 = fieldbook::run(
        &[
            "add".to_string(),
            input1.display().to_string(),
            "--confirm-public".to_string(),
        ],
        &fix.world,
    );
    assert_eq!(add1.code, 0);
    assert!(
        add1.stderr.contains(
            "fieldbook: last verified backup is 9 days old (warn after 7). Run: fieldbook backup\n"
        ),
        "expected 9-day reminder, got: {}",
        add1.stderr
    );

    // Case B: 2 days old (2026-03-03)
    let t_day3 = Now::new(CivilDate::from_ymd(2026, 3, 3).expect("date"), 3600).expect("now");
    fix.world.set_now(t_day3);

    let input2 = fix.tree_root().join("input2.txt");
    std::fs::write(&input2, b"Synthetic input 2\n").expect("write input 2");
    let add2 = fieldbook::run(
        &[
            "add".to_string(),
            input2.display().to_string(),
            "--confirm-public".to_string(),
        ],
        &fix.world,
    );
    assert_eq!(add2.code, 0);
    assert!(
        !add2.stderr.contains("Run: fieldbook backup"),
        "2-day-old backup should not warn, got: {}",
        add2.stderr
    );

    // Case C: No verified backup yet
    let fix_none = TestFixture::new();
    let input3 = fix_none.tree_root().join("input3.txt");
    std::fs::write(&input3, b"Synthetic input 3\n").expect("write input 3");
    let add3 = fieldbook::run(
        &[
            "add".to_string(),
            input3.display().to_string(),
            "--confirm-public".to_string(),
        ],
        &fix_none.world,
    );
    assert_eq!(add3.code, 0);
    assert!(
        add3.stderr.contains(
            "fieldbook: no verified backup yet (warn after 7 days). Run: fieldbook backup\n"
        ),
        "expected no verified backup reminder, got: {}",
        add3.stderr
    );
}

#[test]
fn read_verbs_print_no_reminder() {
    let fix = TestFixture::new();
    let out_dir = tempfile::tempdir().expect("out dir");

    // check
    let check_out = fieldbook::run(&["check".to_string()], &fix.world);
    assert_eq!(check_out.code, 0);
    assert!(
        !check_out.stderr.contains("Run: fieldbook backup"),
        "check must not print backup reminder"
    );

    // build
    let build_out = fieldbook::run(
        &[
            "build".to_string(),
            "--out".to_string(),
            out_dir.path().display().to_string(),
        ],
        &fix.world,
    );
    assert_eq!(build_out.code, 0);
    assert!(
        !build_out.stderr.contains("Run: fieldbook backup"),
        "build must not print backup reminder"
    );

    // search
    let search_out = fieldbook::run(&["search".to_string(), "corpus".to_string()], &fix.world);
    assert!(
        !search_out.stderr.contains("Run: fieldbook backup"),
        "search must not print backup reminder"
    );

    // audit show
    let audit_out = fieldbook::run(&["audit".to_string(), "show".to_string()], &fix.world);
    assert!(
        !audit_out.stderr.contains("Run: fieldbook backup"),
        "audit show must not print backup reminder"
    );
}

#[test]
fn backup_verify_writes_the_age_file() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // 1. Create snapshot 1
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let b_res = run_backup(&fix.world, &eff, &options);
    assert_eq!(b_res.code, 0, "backup stderr: {}", b_res.stderr);

    let cache_path = fix.state_dir().join("reports/last-backup.json");
    assert!(!cache_path.exists(), "cache must not exist before verify");

    // 2. Verify snapshot 1
    let v_res = run_verify(
        &fix.world,
        &BackupVerifyArgs {
            target: Some("main".to_string()),
            snapshot: None,
            in_dir: None,
            raw: None,
            support_catalogue: None,
            expect_support_sha256: None,
        },
    );
    assert_eq!(v_res.code, 0, "verify stderr: {}", v_res.stderr);
    assert!(cache_path.exists(), "cache must exist after verify");

    let cache_content = std::fs::read_to_string(&cache_path).expect("read cache");
    let rep: LastBackupReport =
        serde_json::from_str(&cache_content).expect("parse LastBackupReport");
    assert_eq!(rep.verification, "snapshot");
    assert_eq!(rep.snapshot, 1);
    let canon_target = fix.target_path().canonicalize().expect("canon");
    assert_eq!(rep.target_path, canon_target.to_string_lossy());
    let expected_target_sha =
        fieldbook_core::hash::sha256_hex(canon_target.to_string_lossy().as_bytes());
    assert_eq!(rep.target_sha256, expected_target_sha);
    assert!(!rep.manifest_sha256.is_empty());
    assert!(!rep.verified_utc.is_empty());

    let age = backup_age(fix.state_dir(), fix.world.today());
    assert_eq!(age, Some(0));
}

#[test]
fn one_off_verification_survives_reopen() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = support::temp_copy(&state_mini);
    let target_dir = tempfile::tempdir().expect("target tempdir");

    // Remove any backup targets from settings.toml
    let settings_path = state_dir.path().join("settings.toml");
    std::fs::write(&settings_path, "role = \"lead\"\n").expect("write settings");

    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());

    // One-off backup to DIR
    let b_out = fieldbook::run(
        &[
            "backup".to_string(),
            "--to".to_string(),
            target_dir.path().display().to_string(),
        ],
        &world,
    );
    assert_eq!(b_out.code, 0, "backup stderr: {}", b_out.stderr);

    // Verify with --in DIR
    let v_out = fieldbook::run(
        &[
            "backup".to_string(),
            "verify".to_string(),
            "--in".to_string(),
            target_dir.path().display().to_string(),
        ],
        &world,
    );
    assert_eq!(v_out.code, 0, "verify stderr: {}", v_out.stderr);

    // Reopen and check age
    let age = backup_age(state_dir.path(), world.today());
    assert_eq!(age, Some(0));
}

#[test]
fn substituted_cache_path_invalidates_age() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    assert_eq!(run_backup(&fix.world, &eff, &options).code, 0);
    assert_eq!(
        run_verify(
            &fix.world,
            &BackupVerifyArgs {
                target: Some("main".to_string()),
                snapshot: None,
                in_dir: None,
                raw: None,
                support_catalogue: None,
                expect_support_sha256: None,
            },
        )
        .code,
        0
    );

    let cache_path = fix.state_dir().join("reports/last-backup.json");
    let orig_text = std::fs::read_to_string(&cache_path).expect("read cache");
    let rep: LastBackupReport = serde_json::from_str(&orig_text).expect("parse");

    // Case A: Substitute target_path but not target_sha256
    let mut modified = rep.clone();
    modified.target_path = "/nonexistent/different/path".to_string();
    std::fs::write(&cache_path, serde_json::to_string(&modified).expect("json")).expect("write");
    assert_eq!(backup_age(fix.state_dir(), fix.world.today()), None);

    // Case B: Substitute target_path AND target_sha256 to a nonexistent path
    let other_dir = tempfile::tempdir().expect("tempdir");
    let other_canon = other_dir.path().canonicalize().expect("canon");
    let other_str = other_canon.to_string_lossy().into_owned();
    modified.target_path = other_str.clone();
    modified.target_sha256 = fieldbook_core::hash::sha256_hex(other_str.as_bytes());
    std::fs::write(&cache_path, serde_json::to_string(&modified).expect("json")).expect("write");
    assert_eq!(backup_age(fix.state_dir(), fix.world.today()), None);

    // Case C: Corrupted manifest_sha256
    let mut modified_manifest = rep.clone();
    modified_manifest.manifest_sha256 =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
    std::fs::write(
        &cache_path,
        serde_json::to_string(&modified_manifest).expect("json"),
    )
    .expect("write");
    assert_eq!(backup_age(fix.state_dir(), fix.world.today()), None);

    // Case D: Non-existent snapshot number
    let mut modified_snapshot = rep;
    modified_snapshot.snapshot = 999;
    std::fs::write(
        &cache_path,
        serde_json::to_string(&modified_snapshot).expect("json"),
    )
    .expect("write");
    assert_eq!(backup_age(fix.state_dir(), fix.world.today()), None);
}

#[test]
fn subsequent_backup_keeps_labelled_cache() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Backup 1 and verify 1
    assert_eq!(run_backup(&fix.world, &eff, &options).code, 0);
    assert_eq!(
        run_verify(
            &fix.world,
            &BackupVerifyArgs {
                target: Some("main".to_string()),
                snapshot: None,
                in_dir: None,
                raw: None,
                support_catalogue: None,
                expect_support_sha256: None,
            },
        )
        .code,
        0
    );

    let cache_path = fix.state_dir().join("reports/last-backup.json");
    let cache_text = std::fs::read_to_string(&cache_path).expect("read cache");

    // Backup 2 includes last-backup.json
    assert_eq!(run_backup(&fix.world, &eff, &options).code, 0);

    let snap2_cache = fix
        .target_path()
        .join("fieldbook-backup-0002/data/state/reports/last-backup.json");
    assert!(
        snap2_cache.is_file(),
        "snapshot 2 must contain last-backup.json"
    );
    let snap2_text = std::fs::read_to_string(&snap2_cache).expect("read snap2 cache");
    assert_eq!(
        snap2_text, cache_text,
        "snapshot 2 must have identical cache bytes"
    );

    // Verify snapshot 2
    assert_eq!(
        run_verify(
            &fix.world,
            &BackupVerifyArgs {
                target: Some("main".to_string()),
                snapshot: Some(2),
                in_dir: None,
                raw: None,
                support_catalogue: None,
                expect_support_sha256: None,
            },
        )
        .code,
        0
    );
}

#[test]
fn malformed_unknown_field_cache_refusal() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // 1. Initial backup and verify
    assert_eq!(run_backup(&fix.world, &eff, &options).code, 0);
    assert_eq!(
        run_verify(
            &fix.world,
            &BackupVerifyArgs {
                target: Some("main".to_string()),
                snapshot: None,
                in_dir: None,
                raw: None,
                support_catalogue: None,
                expect_support_sha256: None,
            },
        )
        .code,
        0
    );

    let cache_path = fix.state_dir().join("reports/last-backup.json");
    let orig = std::fs::read_to_string(&cache_path).expect("read cache");

    // Inject unknown field
    let mut val: serde_json::Value = serde_json::from_str(&orig).expect("json");
    val["unknown_extra_field"] = serde_json::json!("disallowed");
    std::fs::write(&cache_path, serde_json::to_string(&val).expect("to_string")).expect("write");

    // Age returns None
    assert_eq!(backup_age(fix.state_dir(), fix.world.today()), None);

    // Backup refuses because malformed cache is unlabelled
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 2, "FB0901 refusal on unlabelled item");
    assert!(outcome.stderr.contains("FB0901"));

    // Syntax error
    std::fs::write(&cache_path, b"not valid json {{{").expect("write invalid");
    assert_eq!(backup_age(fix.state_dir(), fix.world.today()), None);
    let outcome2 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome2.code, 2);
    assert!(outcome2.stderr.contains("FB0901"));
}

#[test]
fn refused_filename_report_obeys_backup_labels() {
    let fix = TestFixture::new();
    let target_str = fix
        .target_path()
        .canonicalize()
        .expect("canon")
        .to_string_lossy()
        .replace('\\', "/");

    let policy_toml_allow_np = format!(
        r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Supplier terms"
banner = "NP-0007"
restricted = true

[[marking]]
match = "canary-file"
labels = ["np-0007"]

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'

[[destination]]
kind = "backup"
allow = ["public", "owner-supplied", "np-0007"]
paths = ["{target_str}"]
"#
    );
    let policy_allow = Policy::parse(&policy_toml_allow_np, None).expect("parse policy");

    let policy_toml_deny_np = format!(
        r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Supplier terms"
banner = "NP-0007"
restricted = true

[[marking]]
match = "canary-file"
labels = ["np-0007"]

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'

[[destination]]
kind = "backup"
allow = ["public", "owner-supplied"]
paths = ["{target_str}"]
"#
    );
    let policy_deny = Policy::parse(&policy_toml_deny_np, None).expect("parse policy");

    // 1. Run real add of a refused file whose filename has a canary
    let canary_input = fix.tree_root().join("canary-file-secret.txt");
    std::fs::write(&canary_input, b"synthetic plain data\n").expect("write canary");

    let add_out = fieldbook::add::run_with_policy(
        &AddArgs {
            file: canary_input.clone(),
            source: None,
            metadata_only: false,
            confirm_public: true,
            recursive: false,
            json: false,
        },
        &fix.world,
        &policy_allow,
    );
    assert_eq!(add_out.code, 1, "marked file refused by corpus must exit 1");
    let last_add_path = fix.state_dir().join("reports/last-add.json");
    assert!(
        last_add_path.is_file(),
        "last-add.json must be saved in state"
    );
    let last_add_text = std::fs::read_to_string(&last_add_path).expect("read last-add");
    assert!(last_add_text.contains("canary-file-secret.txt"));

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // 2. Real backup with target lacking mapped company label receives no canary and no snapshot
    let eff_deny = EffectivePolicy::new(&PolicyState::Loaded(policy_deny), &OwnerSettings::unset());
    let b_deny = run_backup(&fix.world, &eff_deny, &options);
    assert_eq!(b_deny.code, 2, "lacking np-0007 must refuse FB0901");
    assert!(b_deny.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());

    // 3. Real backup with target permitting np-0007 and owner-supplied receives the intact report
    let eff_allow =
        EffectivePolicy::new(&PolicyState::Loaded(policy_allow), &OwnerSettings::unset());
    let b_allow = run_backup(&fix.world, &eff_allow, &options);
    assert_eq!(b_allow.code, 0, "backup stderr: {}", b_allow.stderr);
    let snap1_report = fix
        .target_path()
        .join("fieldbook-backup-0001/data/state/reports/last-add.json");
    assert!(snap1_report.is_file());
    let snap1_text = std::fs::read_to_string(&snap1_report).expect("read snap1 last-add");
    assert_eq!(snap1_text, last_add_text);

    // 4. Missing metadata (empty inputs/labels) contributes unlabelled
    std::fs::write(
        &last_add_path,
        b"{\"report_version\":1,\"dir\":\"inbox\",\"counts\":{\"added\":0,\"present\":0,\"refused\":1},\"files\":[],\"labels\":[],\"inputs\":[]}\n",
    )
    .expect("write bad");
    let b_missing = run_backup(&fix.world, &eff_allow, &options);
    assert_eq!(b_missing.code, 2);
    assert!(b_missing.stderr.contains("FB0901"));

    // 5. State admission refusal: Floor A does not admit marked to state
    let cui_input = fix.tree_root().join("cui-marked.txt");
    std::fs::write(
        &cui_input,
        b"CONTROLLED UNCLASSIFIED INFORMATION\nSynthetic marked text.\n",
    )
    .expect("write cui");
    let floor_policy = Policy::floor(FloorKind::A);
    let add_floor = fieldbook::add::run_with_policy(
        &AddArgs {
            file: cui_input,
            source: None,
            metadata_only: false,
            confirm_public: true,
            recursive: false,
            json: false,
        },
        &fix.world,
        &floor_policy,
    );
    assert!(
        add_floor.stderr.contains("report not saved"),
        "stderr must warn report not saved when state denies: {}",
        add_floor.stderr
    );
}

#[test]
fn schema_invalid_repair_archive_obeys_backup_labels() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // 1. Synthetic FB0801 journal with invalid free-text canary
    let journal_path = fix.state_dir().join("audit/journal.jsonl");
    let mut journal_content = std::fs::read_to_string(&journal_path).expect("read journal");
    journal_content.push_str("{\"seq\":99,\"canary_secret_field\":\"forbidden_free_text\"}\n");
    std::fs::write(&journal_path, journal_content).expect("write corrupt journal");

    // Repair through real audit handler
    let repair_out = fieldbook::run(
        &[
            "audit".to_string(),
            "verify".to_string(),
            "--repair".to_string(),
        ],
        &fix.world,
    );
    assert_eq!(repair_out.code, 1);
    assert!(repair_out.stdout.contains("journal.broken"));

    // Real backup: unchanged repair bytes contribute unlabelled and a target lacking that label receives no snapshot
    let b_corrupt = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        b_corrupt.code, 2,
        "invalid repair archive must refuse FB0901"
    );
    assert!(b_corrupt.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());

    // 2. Positive case: schema-valid journal with only a chain-hash failure (FB0800 fixture)
    let fix_valid = TestFixture::new();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fb0800_src = repo.join("tests/fixtures/state-known-bad/FB0800");
    for entry in std::fs::read_dir(fb0800_src.join("audit")).expect("read dir") {
        let entry = entry.expect("entry");
        std::fs::copy(
            entry.path(),
            fix_valid.state_dir().join("audit").join(entry.file_name()),
        )
        .expect("copy audit file");
    }

    let repair_valid = fieldbook::run(
        &[
            "audit".to_string(),
            "verify".to_string(),
            "--repair".to_string(),
        ],
        &fix_valid.world,
    );
    assert_eq!(repair_valid.code, 1);
    assert!(repair_valid.stdout.contains("journal.broken"));

    let b_valid = run_backup(&fix_valid.world, &eff, &options);
    assert_eq!(
        b_valid.code, 0,
        "schema-valid repair archive must succeed: {}",
        b_valid.stderr
    );
    assert!(
        fix_valid
            .target_path()
            .join("fieldbook-backup-0001")
            .is_dir()
    );
}

#[test]
fn backup_settings_require_owner_supplied() {
    let fix = TestFixture::new();
    let target_str = fix
        .target_path()
        .canonicalize()
        .expect("canon")
        .to_string_lossy()
        .replace('\\', "/");

    // Policy whose backup destination allows public, but excludes owner-supplied
    let policy_toml = format!(
        r#"
policy_schema = 1
revision = 1

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied"]

[[destination]]
kind = "backup"
allow = ["public"]
paths = ["{target_str}"]
"#
    );
    let policy = Policy::parse(&policy_toml, None).expect("parse policy");
    let eff = EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset());

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome.code, 2,
        "target excluding owner-supplied must refuse FB0901"
    );
    assert!(outcome.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
}

#[test]
fn backup_settings_preserve_authorized_bytes() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let original_bytes =
        std::fs::read(fix.state_dir().join("settings.toml")).expect("read settings");
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup stderr: {}", outcome.stderr);

    let copied_bytes = std::fs::read(
        fix.target_path()
            .join("fieldbook-backup-0001/data/state/settings.toml"),
    )
    .expect("read copied settings");
    assert_eq!(copied_bytes, original_bytes);
}

#[test]
fn backup_settings_keep_comment_markings() {
    let fix = TestFixture::new();
    let target_str = fix
        .target_path()
        .canonicalize()
        .expect("canon")
        .to_string_lossy()
        .replace('\\', "/");

    // Add comment with marking to settings.toml
    let settings_file = fix.state_dir().join("settings.toml");
    let mut content = std::fs::read_to_string(&settings_file).expect("read");
    content.push_str("\n# NP-0007 NDA confidential note in comment\n");
    std::fs::write(&settings_file, &content).expect("write settings");

    let policy_toml_no_np = format!(
        r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Supplier terms"
banner = "NP-0007"
restricted = true

[[marking]]
match = "NP-0007 NDA"
labels = ["np-0007"]

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'

[[destination]]
kind = "backup"
allow = ["public", "owner-supplied"]
paths = ["{target_str}"]
"#
    );
    let policy_no_np = Policy::parse(&policy_toml_no_np, None).expect("parse policy");
    let eff_no = EffectivePolicy::new(&PolicyState::Loaded(policy_no_np), &OwnerSettings::unset());

    let policy_toml_with_np = format!(
        r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Supplier terms"
banner = "NP-0007"
restricted = true

[[marking]]
match = "NP-0007 NDA"
labels = ["np-0007"]

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "np-0007"]
readers = 'EXAMPLE\Fieldbook-Readers'

[[destination]]
kind = "backup"
allow = ["public", "owner-supplied", "np-0007"]
paths = ["{target_str}"]
"#
    );
    let policy_with_np = Policy::parse(&policy_toml_with_np, None).expect("parse policy");
    let eff_with = EffectivePolicy::new(
        &PolicyState::Loaded(policy_with_np),
        &OwnerSettings::unset(),
    );

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Refused when destination lacks np-0007
    let out_refuse = run_backup(&fix.world, &eff_no, &options);
    assert_eq!(out_refuse.code, 2);
    assert!(out_refuse.stderr.contains("FB0901"));

    // Permitted when destination allows np-0007
    let out_ok = run_backup(&fix.world, &eff_with, &options);
    assert_eq!(out_ok.code, 0, "backup stderr: {}", out_ok.stderr);
    let snap_settings = std::fs::read_to_string(
        fix.target_path()
            .join("fieldbook-backup-0001/data/state/settings.toml"),
    )
    .expect("read snap settings");
    assert!(snap_settings.contains("# NP-0007 NDA confidential note in comment"));
}

#[test]
fn backup_settings_include_loaded_state_default() {
    let fix = TestFixture::new();
    let target_str = fix
        .target_path()
        .canonicalize()
        .expect("canon")
        .to_string_lossy()
        .replace('\\', "/");

    let policy_toml = format!(
        r#"
policy_schema = 1
revision = 1

[[label]]
id = "bid-0003"
title = "Bid Cost"
banner = "BID-0003"

[[store]]
id = "state"
holds = ["public", "unlabelled", "owner-supplied", "bid-0003"]
default = ["bid-0003"]

[[destination]]
kind = "backup"
allow = ["public", "owner-supplied"]
paths = ["{target_str}"]
"#
    );
    let policy = Policy::parse(&policy_toml, None).expect("parse policy");
    let eff = EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset());

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Target allowing public + owner-supplied but excluding state default bid-0003 refuses
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome.code, 2,
        "target lacking state default must refuse FB0901"
    );
    assert!(outcome.stderr.contains("FB0901"));

    // Floor A has no loaded policy state default: only owner-supplied is required
    let eff_floor = floor_a_effective();
    let outcome_floor = run_backup(&fix.world, &eff_floor, &options);
    assert_eq!(
        outcome_floor.code, 0,
        "floor A has no loaded state default: {}",
        outcome_floor.stderr
    );
}

#[test]
fn unknown_or_text_bearing_control_shape_is_unlabelled() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Case A: Synthetic text in an unknown TOML sidecar
    let unknown_toml = fix.state_dir().join("handoff/H-0001.toml");
    std::fs::create_dir_all(unknown_toml.parent().unwrap()).expect("create dir");
    std::fs::write(
        &unknown_toml,
        b"canary = \"free-text canary in unknown toml\"\n",
    )
    .expect("write toml");

    let out_a = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        out_a.code, 2,
        "unknown TOML sidecar must be unlabelled and refuse FB0901"
    );
    assert!(out_a.stderr.contains("FB0901"));
    std::fs::remove_file(&unknown_toml).expect("remove file");

    // Case B: Extra field in known control schema (pending.toml)
    let pending_toml = fix.state_dir().join("pending.toml");
    std::fs::write(
        &pending_toml,
        b"verb = \"add\"\nextra_unrecognized_field = \"unexpected\"\n",
    )
    .expect("write pending");

    let out_b = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        out_b.code, 2,
        "extra field in known control must be unlabelled and refuse FB0901"
    );
    assert!(out_b.stderr.contains("FB0901"));
}

#[test]
fn known_reports_and_derived_pages_add_no_unlabelled() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // 1. Initial backup and verify creates valid last-backup.json
    assert_eq!(run_backup(&fix.world, &eff, &options).code, 0);
    assert_eq!(
        run_verify(
            &fix.world,
            &BackupVerifyArgs {
                target: Some("main".to_string()),
                snapshot: None,
                in_dir: None,
                raw: None,
                support_catalogue: None,
                expect_support_sha256: None,
            },
        )
        .code,
        0
    );

    // 2. Add derived folders
    let site_file = fix.tree_root().join("out/edition/index.html");
    std::fs::create_dir_all(site_file.parent().unwrap()).expect("create out");
    std::fs::write(&site_file, b"derived html content\n").expect("write site");

    // Subsequent backup succeeds under Floor A (which excludes unlabelled)
    let out = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        out.code, 0,
        "known reports and derived pages must not add unlabelled: {}",
        out.stderr
    );
}

#[test]
fn an_unknown_handoff_file_adds_unlabelled() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let handoff_file = fix.state_dir().join("handoff/H-0001.txt");
    std::fs::create_dir_all(handoff_file.parent().unwrap()).expect("create dir");
    std::fs::write(&handoff_file, b"Handoff text payload\n").expect("write handoff");

    let out = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        out.code, 2,
        "unknown handoff file must add unlabelled and refuse FB0901"
    );
    assert!(out.stderr.contains("FB0901"));
}

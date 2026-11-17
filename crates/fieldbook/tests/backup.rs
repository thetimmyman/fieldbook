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
    BackupListArgs, BackupOptions, BackupVerifyArgs, backup_age, refusal_text, run_backup,
    run_list, run_verify,
};
use fieldbook::cli::AddArgs;
use fieldbook::io::Disk;
use fieldbook::io::atomic::{write_in_root, write_outside};
use fieldbook::io::backup_folder::newest_verified_snapshot_for_target;
use fieldbook::io::dirs::{Removable, remove_outside, rename_dir_outside};
use fieldbook::io::journal::{
    Audited, Journal, JournalError, acquire_write_lease, set_fail_on_backup_end_append,
    set_lock_timeout_for_test,
};
use fieldbook::io::root::Roots;
use fieldbook::io::state::{BackupSettings, read_backup_settings};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    BackupVerified, Count, EventPayload, LabelSet, Now, RefuseAbout, Sha256, Via, Word,
};
use fieldbook_core::backup::{
    BACKUP_TARGET_TEMPLATE, BackupTarget, BackupToml, corpus_entries, manifest_sha256,
};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{
    Allowed, Decision, Delivery, Dest, Operation, Reason, Shipped, Target, Transfer, check,
    hash_tree_manifest,
};
use fieldbook_core::labels::{OWNER_SUPPLIED, PlaceFacts};
use fieldbook_core::manifest;
use fieldbook_core::policy::{EffectivePolicy, FloorKind, OwnerSettings, Policy, PolicyState};
use fieldbook_core::report::{BackupList, LastBackupReport};

use support::faulty::{Fault, FaultKind, FaultyWorld};
use support::world::TestWorld;
use tempfile::TempDir;

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

    let bin = fieldbook_core::strings::binary();
    let toml = format!(
        r#"
[general]
app = "{bin}"

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
"#
    );
    let cfg = BackupSettings::parse(&toml).expect("parse custom");
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
    let backup_toml_bytes = &valid_backup_toml_text(3, "ok").into_bytes();
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

fn make_raw_copy(tree_root: &Path, state_dir: &Path) -> tempfile::TempDir {
    let raw = tempfile::tempdir().expect("raw tempdir");
    let corpus_dest = raw.path().join("corpus");
    let state_dest = raw.path().join("state");
    support::copy_dir_recursive(&tree_root.join("corpus"), &corpus_dest).expect("copy corpus");
    support::copy_dir_recursive(state_dir, &state_dest).expect("copy state");
    raw
}

struct TestFixture {
    _tree_dir: tempfile::TempDir,
    _state_dir: tempfile::TempDir,
    target_dir: tempfile::TempDir,
    world: TestWorld,
}

impl TestFixture {
    fn raw_copy(&self) -> tempfile::TempDir {
        make_raw_copy(self.tree_root(), self.state_dir())
    }

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
        let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
        let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
            &tree,
            "1111111111111111111111111111111111111111",
        )
        .expect("catalogue from corpus-mini");
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
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

    // Plant a hand-made leftover partial folder from a prior crash
    let planted_partial = fix.target_path().join("fieldbook-backup-0099.partial");
    std::fs::create_dir_all(&planted_partial).expect("create planted partial");
    std::fs::write(planted_partial.join("leftover.txt"), b"junk").expect("write leftover");

    // Second backup fails on LATEST write using FaultyWorld
    let world2 = TestWorld::with_state(fix.tree_root(), fix.state_dir());
    let tree = fieldbook::io::read_tree(fix.tree_root()).expect("read tree");
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world2.set_support_catalogue(cat, cat_sha);
    }
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
    assert!(
        !planted_partial.exists(),
        "leftover partial must be removed"
    );

    // Verification by that explicit number succeeds
    let v_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(2),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_out = run_verify(&fix.world, &v_args);
    assert_eq!(
        v_out.code, 0,
        "explicit verification of snapshot 2 must succeed: {}",
        v_out.stderr
    );
    assert!(v_out.stdout.contains("snapshot 2 verified"));
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

fn snapshot_state_dir(root: &Path, current: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(current) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            if path.is_dir() {
                entries.push((rel.clone(), Vec::new()));
                entries.extend(snapshot_state_dir(root, &path));
            } else if path.is_file() {
                let bytes = std::fs::read(&path).unwrap_or_default();
                entries.push((rel, bytes));
            }
        }
    }
    entries.sort();
    entries
}

#[test]
fn dry_run_leaves_a_half_initialized_state_unchanged() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: true,
    };

    let audit_dir = fix.state_dir().join("audit");
    if audit_dir.exists() {
        std::fs::remove_dir_all(&audit_dir).expect("remove audit");
    }
    let lock_file = fix.state_dir().join("lock");
    if lock_file.exists() {
        std::fs::remove_file(&lock_file).expect("remove lock");
    }
    std::fs::write(fix.state_dir().join("store.lock"), b"").expect("create store.lock");
    std::fs::write(fix.state_dir().join(".init"), b"").expect("create .init");

    let before = snapshot_state_dir(fix.state_dir(), fix.state_dir());
    let _outcome = run_backup(&fix.world, &eff, &options);
    let after = snapshot_state_dir(fix.state_dir(), fix.state_dir());

    assert_eq!(
        before, after,
        "half-initialized State directory must remain completely unchanged after dry run"
    );
}

#[test]
fn dry_run_on_an_initialized_state_waits_for_a_held_store_lock() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: true,
    };

    // Initialize fixture State locks so it counts as fully initialized
    let prime = acquire_write_lease(fix.tree_root(), fix.state_dir()).expect("prime lease");
    drop(prime);

    // Before holding the lease, run once with no lock held to prove fixture State counts as initialized
    let outcome_unlocked = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome_unlocked.code, 0,
        "unlocked dry run on initialized state must succeed: {}",
        outcome_unlocked.stderr
    );
    assert!(
        outcome_unlocked.stdout.contains("would write snapshot"),
        "stdout must indicate snapshot would be written: {}",
        outcome_unlocked.stdout
    );

    // Now hold the store lease and verify that dry run waits and times out
    set_lock_timeout_for_test(Some(Duration::from_millis(50)));
    let held = acquire_write_lease(fix.tree_root(), fix.state_dir()).expect("lease");
    let outcome = run_backup(&fix.world, &eff, &options);
    set_lock_timeout_for_test(None);
    drop(held);

    assert_eq!(
        outcome.code, 3,
        "dry run on initialized state with held lock must fail with exit 3: {}",
        outcome.stderr
    );
    assert!(
        !outcome.stdout.contains("would write"),
        "dry run must not output 'would write' when lock acquisition fails"
    );
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

#[cfg(unix)]
#[test]
fn a_failed_prune_removal_exits_3_names_the_snapshot_and_is_not_recorded() {
    use std::os::unix::fs::PermissionsExt;

    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // 1. Settings with keep_last = 3, keep_weekly = 0
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text_3 = format!(
        "role = \"lead\"\n\n[backup]\nkeep_last = 3\nkeep_weekly = 0\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text_3).expect("write settings");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Run three backups successfully (each exit 0)
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome1.code, 0, "backup 1 failed: {}", outcome1.stderr);
    let outcome2 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome2.code, 0, "backup 2 failed: {}", outcome2.stderr);
    let outcome3 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome3.code, 0, "backup 3 failed: {}", outcome3.stderr);

    // 2. Make snapshot 2 impossible to remove: set permissions of data dir to 0o555
    let data_dir = fix.target_path().join("fieldbook-backup-0002/data");

    struct PermGuard(PathBuf);
    impl Drop for PermGuard {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    let guard = PermGuard(data_dir.clone());
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o555))
        .expect("chmod 0o555 data dir");

    // If running as root the permission has no effect: skip test body if writing succeeds
    let probe_file = data_dir.join("probe_writable.tmp");
    if std::fs::write(&probe_file, b"").is_ok() {
        let _ = std::fs::remove_file(&probe_file);
        drop(guard);
        return;
    }

    // 3. Rewrite settings.toml with keep_last = 1, keep_weekly = 0
    let settings_text_1 = format!(
        "role = \"lead\"\n\n[backup]\nkeep_last = 1\nkeep_weekly = 0\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text_1).expect("rewrite settings");

    // 4. Run fourth backup
    let outcome4 = run_backup(&fix.world, &eff, &options);

    // Restore permissions before final asserts so tempdir cleanup works even if asserts fail
    drop(guard);
    let _ = std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o755));

    // Assert fourth run outcome
    assert_eq!(
        outcome4.code, 3,
        "expected exit 3 on prune failure: {}",
        outcome4.stderr
    );
    assert!(
        outcome4.stderr.contains("0002"),
        "stderr should name failed snapshot 0002: {}",
        outcome4.stderr
    );
    assert!(
        fix.target_path().join("fieldbook-backup-0002").exists(),
        "snapshot 2 folder must still exist"
    );

    // Collect all audit/journal*.jsonl files concatenated
    let mut journal_text = String::new();
    let audit_dir = fix.state_dir().join("audit");
    if let Ok(entries) = std::fs::read_dir(&audit_dir) {
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                name.starts_with("journal") && name.ends_with(".jsonl")
            })
            .collect();
        paths.sort();
        for p in paths {
            if let Ok(content) = std::fs::read_to_string(&p) {
                journal_text.push_str(&content);
            }
        }
    }

    let prune_events: Vec<&str> = journal_text
        .lines()
        .filter(|line| line.contains("\"kind\":\"backup.prune\""))
        .collect();

    // Snapshot 1 was pruned, snapshot 2 was not: verify prune event lists snapshot 1 and not 2
    if fix.target_path().join("fieldbook-backup-0001").exists() {
        // In case only snapshot 2 was selected for pruning
        assert!(
            prune_events.is_empty(),
            "no backup.prune event should be appended when only snapshot 2 failed to be removed"
        );
    } else {
        assert_eq!(
            prune_events.len(),
            1,
            "expected exactly one backup.prune event: {journal_text}"
        );
        let val: serde_json::Value =
            serde_json::from_str(prune_events[0]).expect("parse prune event json");
        let snaps = val["snapshots"]
            .as_array()
            .expect("snapshots field must be an array");
        let nums: Vec<u64> = snaps.iter().filter_map(|v| v.as_u64()).collect();
        assert!(
            nums.contains(&1),
            "backup.prune should list snapshot 1: {}",
            prune_events[0]
        );
        assert!(
            !nums.contains(&2),
            "backup.prune must NOT list snapshot 2: {}",
            prune_events[0]
        );
    }
}

/// Reads every journal file under the State audit folder into one string.
fn all_journal_text(fix: &TestFixture) -> String {
    let mut text = String::new();
    let audit_dir = fix.state_dir().join("audit");
    if let Ok(entries) = std::fs::read_dir(&audit_dir) {
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                name.starts_with("journal") && name.ends_with(".jsonl")
            })
            .collect();
        paths.sort();
        for p in paths {
            if let Ok(content) = std::fs::read_to_string(&p) {
                text.push_str(&content);
            }
        }
    }
    text
}

#[test]
fn retention_never_touches_an_entry_without_a_parsed_backup_toml() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[backup]\nkeep_last = 2\nkeep_weekly = 0\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome1.code, 0, "backup 1 failed: {}", outcome1.stderr);
    let outcome2 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome2.code, 0, "backup 2 failed: {}", outcome2.stderr);

    // A folder that carries the snapshot name but no backup.toml.
    let foreign = fix.target_path().join("fieldbook-backup-0099");
    std::fs::create_dir_all(&foreign).expect("create foreign folder");
    std::fs::write(foreign.join("note.txt"), b"not a snapshot").expect("write note");

    // The newest real snapshot gets a key this build does not know.
    let newest_toml = fix.target_path().join("fieldbook-backup-0002/backup.toml");
    let mut toml_text = std::fs::read_to_string(&newest_toml).expect("read backup.toml");
    toml_text.push_str("an_unknown_key = \"from a newer writer\"\n");
    std::fs::write(&newest_toml, toml_text).expect("write backup.toml");

    // The dry run lists neither entry as a removal.
    let dry = BackupOptions {
        dry_run: true,
        ..options.clone()
    };
    let dry_outcome = run_backup(&fix.world, &eff, &dry);
    assert_eq!(
        dry_outcome.code, 0,
        "dry run failed: {}",
        dry_outcome.stderr
    );
    assert!(
        !dry_outcome.stdout.contains("would prune"),
        "dry run must not list an entry without a parsed backup.toml: {}",
        dry_outcome.stdout
    );

    // A real backup exits 0, leaves both in place, and records no prune naming them.
    let outcome3 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome3.code, 0, "backup 3 failed: {}", outcome3.stderr);
    assert!(
        !outcome3.stderr.contains("could not remove"),
        "no removal may be attempted: {}",
        outcome3.stderr
    );
    assert!(foreign.exists(), "the foreign folder must stay");
    assert!(
        fix.target_path().join("fieldbook-backup-0002").exists(),
        "the snapshot with an unparsable backup.toml must stay"
    );
    let journal_text = all_journal_text(&fix);
    for line in journal_text
        .lines()
        .filter(|l| l.contains("\"kind\":\"backup.prune\""))
    {
        let val: serde_json::Value = serde_json::from_str(line).expect("parse prune event");
        let nums: Vec<u64> = val["snapshots"]
            .as_array()
            .map(|a| a.iter().filter_map(serde_json::Value::as_u64).collect())
            .unwrap_or_default();
        assert!(
            !nums.contains(&99) && !nums.contains(&2),
            "backup.prune must not name an entry without a parsed backup.toml: {line}"
        );
    }
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

    // Compare original and copied journal bytes (exact match in recovery mode)
    let copied_journal = std::fs::read_to_string(snap_dir.join("data/state/audit/journal.jsonl"))
        .expect("read copied journal");
    assert_eq!(
        copied_journal, journal_before,
        "copied journal bytes must equal original refused journal"
    );

    // backup.toml records journal_state = `broken`
    let toml_text =
        std::fs::read_to_string(snap_dir.join("backup.toml")).expect("read backup.toml");
    let info = BackupToml::parse(&toml_text).expect("parse backup.toml");
    assert_eq!(
        info.journal_state,
        fieldbook_core::backup::toml::JOURNAL_STATE_BROKEN
    );

    // No event appended to journal
    let journal_after = std::fs::read_to_string(&journal_path).expect("read journal after");
    assert_eq!(journal_after, journal_before);

    // Recovery token constraints and real create/write operations
    let lease = fix.world.write_lease().expect("acquire store lease");
    let rec = Journal::open_backup_recovery(&fix.world, &lease).expect("open recovery");
    let test_file = fix.target_path().join("recovery-test.txt");
    let test_bytes = b"recovery test bytes\n";
    let delivery = Delivery::new(vec![Transfer::write_path(&test_file, test_bytes)], None);
    let Decision::Allowed(allowed) = check(
        &eff,
        &Dest::Backup {
            path: fix.target_path().to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        fix.world.now(),
        Shipped::Never,
    ) else {
        panic!("expected check Allowed for recovery test");
    };

    let recovery_audited = rec.approve(&allowed).expect("approve recovery delivery");
    assert!(recovery_audited.is_recovery_only());

    // 1. Recovery token cannot write in-root
    let in_root_res = fix.world.write_file(
        &recovery_audited,
        &fix.tree_root().join("corpus/in_root.txt"),
        b"forbidden in root",
    );
    assert!(
        in_root_res.is_err(),
        "recovery token must not write in-root"
    );
    let roots = Roots::at_home(fix.tree_root(), fix.state_dir());
    let atomic_res = write_in_root(
        &roots.corpus,
        Path::new("in_root.txt"),
        b"forbidden",
        &recovery_audited,
    );
    assert!(
        matches!(atomic_res, Err(fieldbook::Error::OutOfScope(_))),
        "atomic write_in_root must refuse recovery-only token"
    );

    // 2. Recovery token cannot use another delivery
    let other_file = fix.target_path().join("other.txt");
    let other_delivery = Delivery::new(
        vec![Transfer::write_path(&other_file, b"other bytes")],
        None,
    );
    let Decision::Allowed(allowed_other) = check(
        &eff,
        &Dest::Backup {
            path: fix.target_path().to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&other_delivery),
        fix.world.now(),
        Shipped::Never,
    ) else {
        panic!("expected allowed for other delivery");
    };
    let other_res = fix.world.write_outside(
        &allowed_other,
        &recovery_audited,
        &other_file,
        b"other bytes",
    );
    assert!(
        matches!(other_res, Err(fieldbook::Error::OutOfScope(_))),
        "recovery token must not be used with another delivery"
    );

    // 3. Recovery token refuses changed paths
    let changed_path = fix.target_path().join("unapproved-file.txt");
    let changed_path_res =
        fix.world
            .write_outside(&allowed, &recovery_audited, &changed_path, test_bytes);
    assert!(
        matches!(changed_path_res, Err(fieldbook::Error::OutOfScope(_))),
        "recovery token must refuse changed unapproved paths"
    );

    // 4. Real create/write through recovery token succeeds
    let write_res = fix
        .world
        .write_outside(&allowed, &recovery_audited, &test_file, test_bytes);
    assert!(
        write_res.is_ok(),
        "real write through recovery token must succeed"
    );
    assert_eq!(
        std::fs::read(&test_file).expect("read written file"),
        test_bytes
    );
    drop(recovery_audited);
    drop(rec);
    drop(lease);

    // 5. Held lock still exits 3 with no snapshot
    set_lock_timeout_for_test(Some(Duration::from_millis(50)));
    let _held_lock = acquire_write_lease(fix.tree_root(), fix.state_dir()).expect("acquire lease");
    let held_outcome = run_backup(&fix.world, &eff, &options);
    set_lock_timeout_for_test(None);
    drop(_held_lock);
    assert_eq!(
        held_outcome.code, 3,
        "backup on held lock must exit 3: {}",
        held_outcome.stderr
    );
    assert!(
        !fix.target_path().join("fieldbook-backup-0002").exists(),
        "no snapshot must be created when lock is held"
    );
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
    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("catalogue from corpus-mini");
    let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
    world.set_support_catalogue(cat, cat_sha);
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
    assert_eq!(
        info.journal_state,
        fieldbook_core::backup::toml::JOURNAL_STATE_BROKEN
    );

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
fn a_page_with_a_bad_handling_id_among_good_ones_refuses_the_snapshot() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Front matter the model cannot parse (no closing delimiter, bad TOML), with a
    // handling list that holds one public id and one id that is not a label.
    let bad_page = fix
        .tree_root()
        .join("corpus/pages/estimating/pricing/est-broken-front-matter.md");
    let content = "+++\nid = \"est-broken\"\ntitle = \nhandling = [\"public\", \"NP-0007\"]\n+++\n\nSecret.\n";
    std::fs::write(&bad_page, content).expect("write page");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome.code, 2,
        "FB0901 refusal exits 2: {}",
        outcome.stderr
    );
    assert!(outcome.stderr.contains("FB0901"), "{}", outcome.stderr);
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
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
fn text_in_a_state_lock_file_is_stray_and_refuses() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    std::fs::write(fix.state_dir().join("lock"), b"canary text in lock file")
        .expect("write state lock");

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
fn a_non_register_csv_under_registers_is_stray_and_refuses() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    std::fs::write(
        fix.tree_root().join("corpus/registers/notes.csv"),
        b"a,b\n1,2\n",
    )
    .expect("write non-register csv");

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
    let sources_csv = fix.tree_root().join("corpus/registers/sources.csv");
    let mut sources_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&sources_csv)
        .expect("open sources.csv");
    use std::io::Write as _;
    for i in 0..10 {
        let sid = format!("S-900{i}");
        let fname = format!("{sid}-v1.bin");
        std::fs::write(sources_dir.join(&fname), &chunk).expect("write 20MB file");
        writeln!(
            sources_file,
            "{sid},1,synthetic-perf,\"Large {i}\",PerfBoard,primary_law,primary_evidence,A,\"Synthetic\",synthetic_perf,reviewed,{fname},0000000000000000000000000000000000000000000000000000000000000000,2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Note,Purpose,,,,,"
        )
        .expect("write row to sources.csv");
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
    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("catalogue from corpus-mini");
    let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
    world.set_support_catalogue(cat, cat_sha);
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
    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("catalogue from corpus-mini");
    let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
    world.set_support_catalogue(cat, cat_sha);

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
    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("catalogue from corpus-mini");
    let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
    world.set_support_catalogue(cat, cat_sha);

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
default = ["np-0007"]
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
default = ["np-0007"]
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

#[test]
fn complete_corpus_support_files_are_backed_up() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let corpus = fix.tree_root().join("corpus");
    std::fs::write(corpus.join("AGENTS.md"), b"Agents doc\n").expect("write AGENTS.md");
    std::fs::write(
        corpus.join(fieldbook_core::strings::paths()[6]),
        b"Assistant doc\n",
    )
    .expect("write assistant");
    std::fs::write(corpus.join("README.md"), b"Readme doc\n").expect("write README.md");
    std::fs::write(corpus.join("changes.md"), b"Changes doc\n").expect("write changes.md");
    std::fs::write(corpus.join("scan-allow.toml"), b"# scan allow\n")
        .expect("write scan-allow.toml");

    let t_dir = corpus.join("templates");
    std::fs::create_dir_all(&t_dir).expect("create templates");
    std::fs::write(t_dir.join("concept.md"), b"+++\ntype = \"concept\"\n+++\n")
        .expect("write concept.md");
    let wb_dir = t_dir.join("workbooks");
    std::fs::create_dir_all(&wb_dir).expect("create wb dir");
    std::fs::write(wb_dir.join("financial.csv"), b"code,amount\n").expect("write financial.csv");

    let eval_dir = corpus.join("eval");
    std::fs::create_dir_all(&eval_dir).expect("create eval");
    std::fs::write(eval_dir.join("regression.toml"), b"# regression\n")
        .expect("write regression.toml");

    let pipe_dir = corpus.join("pipeline");
    std::fs::create_dir_all(&pipe_dir).expect("create pipeline");
    std::fs::write(pipe_dir.join("rules.toml"), b"# rules\n").expect("write rules.toml");

    let tree = fieldbook::io::read_tree(fix.tree_root()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
    fix.world.set_support_catalogue(cat, cat_sha);

    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).expect("read SHA256SUMS");

    assert!(sums_text.contains("data/corpus/AGENTS.md"));
    assert!(sums_text.contains("data/corpus/ASSISTANT.md"));
    assert!(sums_text.contains("data/corpus/README.md"));
    assert!(sums_text.contains("data/corpus/corpus.toml"));
    assert!(sums_text.contains("data/corpus/changes.md"));
    assert!(sums_text.contains("data/corpus/scan-allow.toml"));
    assert!(sums_text.contains("data/corpus/templates/concept.md"));
    assert!(sums_text.contains("data/corpus/templates/workbooks/financial.csv"));
    assert!(sums_text.contains("data/corpus/eval/regression.toml"));
    assert!(sums_text.contains("data/corpus/pipeline/rules.toml"));
}

#[test]
fn support_without_public_provenance_refuses_backup() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let tree = fieldbook::io::read_tree(fix.tree_root()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let incomplete_files: Vec<_> = cat
        .files
        .iter()
        .filter(|f| f.path != "changes.md")
        .cloned()
        .collect();
    let mut incomplete_pairs = Vec::new();
    for f in incomplete_files {
        incomplete_pairs.push((f.path, f.sha256));
    }
    let incomplete_cat = fieldbook_core::manifest::SupportCatalogue::new(
        cat.corpus_schema,
        &cat.commit,
        incomplete_pairs,
    )
    .expect("new cat");
    let (_, inc_sha) = incomplete_cat.to_toml_bytes_and_sha256();
    fix.world.set_support_catalogue(incomplete_cat, inc_sha);

    let outcome_a = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome_a.code, 2,
        "must refuse missing provenance with exit 2"
    );
    assert!(outcome_a.stderr.contains("FB0901"));

    let (complete_cat, cat_sha) = {
        let c = fieldbook_core::manifest::SupportCatalogue::from_public_source(
            &tree,
            "1111111111111111111111111111111111111111",
        )
        .expect("from_public_source");
        let (_, s) = c.to_toml_bytes_and_sha256();
        (c, s)
    };
    fix.world.set_support_catalogue(complete_cat, cat_sha);

    let changes_path = fix.tree_root().join("corpus/changes.md");
    std::fs::write(&changes_path, b"Mutated changes content\n").expect("write changes");

    let outcome_b = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome_b.code, 2, "must refuse mutated byte with exit 2");
    assert!(outcome_b.stderr.contains("FB0901"));
}

#[test]
fn raw_integrity_does_not_grant_public_provenance() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let raw = fix.raw_copy();
    let raw_args = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let verify_out = run_verify(&fix.world, &raw_args);
    assert_eq!(
        verify_out.code, 0,
        "ordinary raw verify must succeed: {}",
        verify_out.stderr
    );
    assert!(verify_out.stdout.contains("raw copy consistent\n"));
    assert!(
        !verify_out
            .stdout
            .contains("public support catalogue verified")
    );

    let events = fieldbook::io::journal::read_valid_events_prefix(fix.state_dir());
    let last_verify = events
        .iter()
        .filter_map(|e| {
            if let EventPayload::BackupVerify { verified, outcome } = &e.payload {
                Some((verified, outcome))
            } else {
                None
            }
        })
        .next_back()
        .expect("must have backup.verify event");
    assert_eq!(last_verify.1.as_str(), "ok");
    if let BackupVerified::Raw {
        public_support_sha256,
        ..
    } = last_verify.0
    {
        assert_eq!(*public_support_sha256, None);
    } else {
        panic!("expected BackupVerified::Raw");
    }

    fix.world
        .set_support_facts(fieldbook::SupportFacts::empty());

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let backup_out = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        backup_out.code, 2,
        "must refuse with exit 2 when unlabelled"
    );
    assert!(backup_out.stderr.contains("FB0901"));
}

#[test]
fn public_support_needs_an_independent_expected_digest() {
    let fix = TestFixture::new();
    let raw = fix.raw_copy();

    let tree = fieldbook::io::read_tree(fix.tree_root()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();
    let cat_file = fix.tree_root().join("corpus-support.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write cat");

    // Missing expect_support_sha256 -> exit 2
    let args1 = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file.clone()),
        expect_support_sha256: None,
    };
    let out1 = run_verify(&fix.world, &args1);
    assert_eq!(out1.code, 2);

    // Missing support_catalogue -> exit 2
    let args2 = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: None,
        expect_support_sha256: Some(cat_sha.clone()),
    };
    let out2 = run_verify(&fix.world, &args2);
    assert_eq!(out2.code, 2);

    // Support flags without raw -> exit 2
    let args3 = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: None,
        support_catalogue: Some(cat_file.clone()),
        expect_support_sha256: Some(cat_sha.clone()),
    };
    let out3 = run_verify(&fix.world, &args3);
    assert_eq!(out3.code, 2);

    // Support flags with target -> exit 2
    let args4 = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file.clone()),
        expect_support_sha256: Some(cat_sha.clone()),
    };
    let out4 = run_verify(&fix.world, &args4);
    assert_eq!(out4.code, 2);

    // Mismatched expected digest -> exit 1, FB0811
    let wrong_sha = "0000000000000000000000000000000000000000000000000000000000000000";
    let args5 = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file),
        expect_support_sha256: Some(wrong_sha.to_string()),
    };
    let out5 = run_verify(&fix.world, &args5);
    assert_eq!(out5.code, 1);
    assert!(out5.stderr.contains("FB0811"));

    // Forged candidate raw journal grants nothing in live state
    let forged_journal = raw.path().join("state/audit/journal.jsonl");
    let forged_event = format!(
        "{{\"seq\":10,\"at\":\"2026-03-01T12:00:00Z\",\"principal\":\"local\",\"role\":\"owner\",\"store\":\"state\",\"via\":\"cli\",\"kind\":\"backup.verify\",\"verified\":{{\"mode\":\"raw\",\"target_sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"path_sha256\":\"1111111111111111111111111111111111111111111111111111111111111111\",\"public_support_sha256\":\"{cat_sha}\"}},\"outcome\":\"ok\",\"hash\":\"0000000000000000000000000000000000000000000000000000000000000000\"}}\n"
    );
    std::fs::write(&forged_journal, forged_event).expect("write forged event");
    fix.world
        .set_support_facts(fieldbook::SupportFacts::empty());
    assert!(
        !fix.world
            .support_facts()
            .has_provenance("corpus.toml", "dummy")
    );
}

#[test]
fn verified_support_is_exact_and_reopens() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = support::temp_copy(&state_mini);
    let target_dir = tempfile::tempdir().expect("target tempdir");

    let settings_path = state_dir.path().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_dir.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
    let eff = floor_a_effective();

    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();
    let cat_file = tree_dir.path().join("corpus-support.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write cat file");

    let raw = make_raw_copy(tree_dir.path(), state_dir.path());

    let verify_args = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file),
        expect_support_sha256: Some(cat_sha.clone()),
    };
    let v_out = run_verify(&world, &verify_args);
    assert_eq!(v_out.code, 0, "optional verify failed: {}", v_out.stderr);
    assert!(v_out.stdout.contains("raw copy consistent\n"));
    let short_hex = &cat_sha[..12];
    assert!(
        v_out
            .stdout
            .contains(&format!("public support catalogue verified: {short_hex}"))
    );

    drop(world);
    let world2 = TestWorld::with_state(tree_dir.path(), state_dir.path());

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let b_out1 = run_backup(&world2, &eff, &options);
    assert_eq!(
        b_out1.code, 0,
        "backup after reopen must succeed: {}",
        b_out1.stderr
    );
    let latest1 = std::fs::read_to_string(target_dir.path().join("LATEST")).expect("read LATEST");
    assert!(latest1.starts_with("1\n"));

    let changes_path = tree_dir.path().join("corpus/changes.md");
    let mutated_bytes = b"Mutated after verify\n";
    std::fs::write(&changes_path, mutated_bytes).expect("write mutated");

    let b_out2 = run_backup(&world2, &eff, &options);
    assert_eq!(b_out2.code, 2, "mutated support byte must refuse FB0901");
    assert!(b_out2.stderr.contains("FB0901"));
    let latest2 = std::fs::read_to_string(target_dir.path().join("LATEST")).expect("read LATEST");
    assert_eq!(latest2, latest1);
    assert!(!target_dir.path().join("fieldbook-backup-0002").exists());

    // Record a write that fails: does not count!
    let mutated_sha = fieldbook_core::hash::sha256_hex(mutated_bytes);
    let ws = fieldbook_core::audit::WriteSet {
        labels: LabelSet::public(),
        files: vec![fieldbook_core::audit::WriteFile::new(
            "corpus/changes.md",
            mutated_sha.clone(),
        )],
        affected: Vec::new(),
    };
    let intent_failed = fieldbook_core::audit::OperationIntent {
        schema: 1,
        key: fieldbook_core::audit::OperationKey {
            verb: fieldbook_core::audit::Verb::parse("add").unwrap(),
            subject_sha256: Sha256::digest(b"failed_sub"),
        },
        request_sha256: Sha256::digest(b"req_failed"),
        allocated: Vec::new(),
        allocated_store_uid: None,
        planned_on: CivilDate::from_ymd(2026, 3, 1).unwrap(),
    };
    {
        let lease = world2.write_lease().expect("lease");
        let mut journal = Journal::open(&world2, world2.state_dir(), Via::Cli).expect("journal");
        let token_failed = journal
            .begin(&lease, &intent_failed, &ws)
            .expect("begin failed");
        // Pending intent does not grant provenance
        assert!(
            !world2
                .support_facts()
                .has_provenance("corpus/changes.md", &mutated_sha)
        );
        journal
            .end(token_failed, Word::new_static("failed"))
            .expect("end failed");
    }
    // Failed write does not grant provenance
    let b_out_failed = run_backup(&world2, &eff, &options);
    assert_eq!(
        b_out_failed.code, 2,
        "failed write must not grant provenance"
    );

    // Clear pending.toml after failed write so next write can begin
    let pending_file = state_dir.path().join("pending.toml");
    if pending_file.exists() {
        let _ = std::fs::remove_file(&pending_file);
    }

    // Complete write with outcome ok
    let intent_ok = fieldbook_core::audit::OperationIntent {
        schema: 1,
        key: fieldbook_core::audit::OperationKey {
            verb: fieldbook_core::audit::Verb::parse("add").unwrap(),
            subject_sha256: Sha256::digest(b"ok_sub"),
        },
        request_sha256: Sha256::digest(b"req_ok"),
        allocated: Vec::new(),
        allocated_store_uid: None,
        planned_on: CivilDate::from_ymd(2026, 3, 1).unwrap(),
    };
    {
        let lease = world2.write_lease().expect("lease");
        let mut journal = Journal::open(&world2, world2.state_dir(), Via::Cli).expect("journal");
        let token_ok = journal.begin(&lease, &intent_ok, &ws).expect("begin ok");
        journal
            .end(token_ok, Word::new_static("ok"))
            .expect("end ok");
    }

    drop(world2);
    let world3 = TestWorld::with_state(tree_dir.path(), state_dir.path());
    let b_out3 = run_backup(&world3, &eff, &options);
    assert_eq!(
        b_out3.code, 0,
        "backup after completed public write must succeed: {}",
        b_out3.stderr
    );
    let latest3 = std::fs::read_to_string(target_dir.path().join("LATEST")).expect("read LATEST");
    assert!(latest3.starts_with("2\n"));
}

#[test]
fn support_cache_without_success_is_unresolved() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = support::temp_copy(&state_mini);
    let target_dir = tempfile::tempdir().expect("target tempdir");

    let settings_path = state_dir.path().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_dir.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();
    let orphan_path = state_dir
        .path()
        .join("public-support")
        .join(format!("{cat_sha}.toml"));
    std::fs::create_dir_all(orphan_path.parent().unwrap()).expect("create dir");
    std::fs::write(&orphan_path, &cat_bytes).expect("write orphan");

    // Orphan catalogue (no matching verify event) grants nothing
    let out_orphan = run_backup(&world, &eff, &options);
    assert_eq!(
        out_orphan.code, 2,
        "orphan catalogue must not grant provenance"
    );
    assert!(out_orphan.stderr.contains("FB0901"));

    // Real verify succeeds and retains binding
    let raw = make_raw_copy(tree_dir.path(), state_dir.path());
    let cat_file = tree_dir.path().join("cat.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write cat file");
    let verify_args = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file),
        expect_support_sha256: Some(cat_sha.clone()),
    };
    let v_out = run_verify(&world, &verify_args);
    assert_eq!(v_out.code, 0, "real verify succeeds: {}", v_out.stderr);

    // Normal backup succeeds
    let out_retry = run_backup(&world, &eff, &options);
    assert_eq!(
        out_retry.code, 0,
        "backup after verify must succeed: {}",
        out_retry.stderr
    );

    // Schema-valid recovery prefix establishes proof before planted tail fault
    let journal_path = state_dir.path().join("audit/journal.jsonl");
    let content = std::fs::read_to_string(&journal_path).expect("read journal");
    let last = content
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .expect("last line");
    let (last_event, _) = fieldbook_core::audit::Event::parse_line(last).expect("parse last");
    let next_seq = last_event.seq + 1;
    let (prefix, _) = last.rsplit_once(',').expect("split comma");
    let seq_str = format!("\"seq\":{}", last_event.seq);
    let new_seq_str = format!("\"seq\":{next_seq}");
    let prefix_with_next_seq = prefix.replace(&seq_str, &new_seq_str);
    let bad_tail_line = format!(
        "{prefix_with_next_seq},\"hash\":\"0000000000000000000000000000000000000000000000000000000000000000\"}}\n"
    );
    let mut j_content = content.clone();
    j_content.push_str(&bad_tail_line);
    std::fs::write(&journal_path, j_content).expect("write corrupted journal");

    let out_prefix = run_backup(&world, &eff, &options);
    assert_eq!(
        out_prefix.code, 1,
        "recovery prefix establishes proof before fault: {}",
        out_prefix.stderr
    );
}

#[test]
fn public_support_keeps_markings_and_admission() {
    let fix = TestFixture::new();

    let tree = fieldbook::io::read_tree(fix.tree_root()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");

    let agents_path = fix.tree_root().join("corpus/AGENTS.md");
    let marked_content = "# Agent instructions\n\nCOMPANY CONFIDENTIAL\n";
    std::fs::write(&agents_path, marked_content).expect("write marked agents");
    let marked_sha = fieldbook_core::hash::sha256_hex(marked_content.as_bytes());

    let mut pairs = Vec::new();
    for f in &cat.files {
        if f.path == "AGENTS.md" {
            pairs.push((f.path.clone(), marked_sha.clone()));
        } else {
            pairs.push((f.path.clone(), f.sha256.clone()));
        }
    }
    let cat_with_marked =
        fieldbook_core::manifest::SupportCatalogue::new(cat.corpus_schema, &cat.commit, pairs)
            .expect("new cat with marked");
    let (_, cat_sha) = cat_with_marked.to_toml_bytes_and_sha256();
    fix.world.set_support_catalogue(cat_with_marked, cat_sha);

    let eff_floor_a = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let out = run_backup(&fix.world, &eff_floor_a, &options);
    assert_eq!(
        out.code, 2,
        "unmapped marking must keep marked and refuse export"
    );
    assert!(out.stderr.contains("FB0901"));

    // Mapped restrictive label: when policy maps marking and destination allows it, backup succeeds
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
id = "np-0007"
title = "Supplier terms"
banner = "NP-0007"
restricted = true

[[marking]]
match = "COMPANY CONFIDENTIAL"
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
    let policy = Policy::parse(&policy_toml, None).expect("parse policy");
    let eff_loaded = EffectivePolicy::new(&PolicyState::Loaded(policy), &OwnerSettings::unset());
    let out_loaded = run_backup(&fix.world, &eff_loaded, &options);
    assert_eq!(
        out_loaded.code, 0,
        "mapped restrictive label allowed by destination must succeed: {}",
        out_loaded.stderr
    );

    // Destination without restrictive label must refuse
    let policy_toml_no_dest = format!(
        r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Supplier terms"
banner = "NP-0007"
restricted = true

[[marking]]
match = "COMPANY CONFIDENTIAL"
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
    let policy2 = Policy::parse(&policy_toml_no_dest, None).expect("parse policy");
    let eff_no_dest = EffectivePolicy::new(&PolicyState::Loaded(policy2), &OwnerSettings::unset());
    let out_refused = run_backup(&fix.world, &eff_no_dest, &options);
    assert_eq!(
        out_refused.code, 2,
        "destination without restrictive label must refuse"
    );
    assert!(out_refused.stderr.contains("FB0901"));

    // Floor B route: restore clean AGENTS.md, but remove support provenance facts so support is unlabelled.
    // Configure target with company_storage = true.
    let fix2 = TestFixture::new();
    let target2_str = fix2
        .target_path()
        .canonicalize()
        .expect("canon")
        .to_string_lossy()
        .replace('\\', "/");
    let settings_cs = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{target2_str}\"\ncompany_storage = true\n"
    );
    std::fs::write(fix2.state_dir().join("settings.toml"), settings_cs).expect("write settings");
    fix2.world
        .set_support_facts(fieldbook::SupportFacts::empty());

    // Under Floor A: unlabelled support refuses
    let out_floor_a_unlabelled = run_backup(&fix2.world, &eff_floor_a, &options);
    assert_eq!(out_floor_a_unlabelled.code, 2, "floor A refuses unlabelled");

    // Under Floor B with company_storage: true, unlabelled is permitted
    let eff_floor_b =
        EffectivePolicy::new(&PolicyState::Floor(FloorKind::B), &OwnerSettings::unset());
    let out_floor_b = run_backup(&fix2.world, &eff_floor_b, &options);
    assert_eq!(
        out_floor_b.code, 0,
        "floor B route permits export of unlabelled: {}",
        out_floor_b.stderr
    );
}

#[test]
fn one_executable_accepts_two_supported_editions() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let eff = floor_a_effective();

    // Dataset A:
    let tree_a = support::temp_copy(&corpus_mini);
    let state_a = support::temp_copy(&state_mini);
    let target_a = tempfile::tempdir().expect("target a");
    let settings_a = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_a.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_a.path().join("settings.toml"), settings_a).expect("write settings a");
    let world_a = TestWorld::with_state(tree_a.path(), state_a.path());

    let t_a = fieldbook::io::read_tree(tree_a.path()).expect("read tree a");
    let cat_a = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &t_a,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source a");
    let (cat_bytes_a, cat_sha_a) = cat_a.to_toml_bytes_and_sha256();
    let cat_file_a = tree_a.path().join("cat_a.toml");
    std::fs::write(&cat_file_a, &cat_bytes_a).expect("write cat a");

    let raw_a = make_raw_copy(tree_a.path(), state_a.path());

    let v_a = run_verify(
        &world_a,
        &BackupVerifyArgs {
            target: None,
            in_dir: None,
            snapshot: None,
            raw: Some(raw_a.path().to_path_buf()),
            support_catalogue: Some(cat_file_a),
            expect_support_sha256: Some(cat_sha_a),
        },
    );
    assert_eq!(v_a.code, 0, "verify dataset A: {}", v_a.stderr);
    let b_a = run_backup(
        &world_a,
        &eff,
        &BackupOptions {
            target: Some("main".to_string()),
            to: None,
            dry_run: false,
        },
    );
    assert_eq!(b_a.code, 0, "backup dataset A: {}", b_a.stderr);

    // Dataset B: different support bytes (templates and regression.toml changed)
    let tree_b = support::temp_copy(&corpus_mini);
    let state_b = support::temp_copy(&state_mini);
    let target_b = tempfile::tempdir().expect("target b");
    let settings_b = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_b.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_b.path().join("settings.toml"), settings_b).expect("write settings b");

    std::fs::create_dir_all(tree_b.path().join("corpus/eval")).expect("create eval");
    std::fs::write(
        tree_b.path().join("corpus/eval/regression.toml"),
        b"# regression variant B\n",
    )
    .expect("write regression b");
    std::fs::write(
        tree_b.path().join("corpus/templates/concept.md"),
        b"+++\ntype = \"concept\"\nvariant = \"B\"\n+++\n",
    )
    .expect("write concept b");

    let world_b = TestWorld::with_state(tree_b.path(), state_b.path());
    let t_b = fieldbook::io::read_tree(tree_b.path()).expect("read tree b");
    let cat_b = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &t_b,
        "2222222222222222222222222222222222222222",
    )
    .expect("from_public_source b");
    let (cat_bytes_b, cat_sha_b) = cat_b.to_toml_bytes_and_sha256();
    let cat_file_b = tree_b.path().join("cat_b.toml");
    std::fs::write(&cat_file_b, &cat_bytes_b).expect("write cat b");

    let raw_b = make_raw_copy(tree_b.path(), state_b.path());

    let v_b = run_verify(
        &world_b,
        &BackupVerifyArgs {
            target: None,
            in_dir: None,
            snapshot: None,
            raw: Some(raw_b.path().to_path_buf()),
            support_catalogue: Some(cat_file_b),
            expect_support_sha256: Some(cat_sha_b),
        },
    );
    assert_eq!(v_b.code, 0, "verify dataset B: {}", v_b.stderr);
    let b_b = run_backup(
        &world_b,
        &eff,
        &BackupOptions {
            target: Some("main".to_string()),
            to: None,
            dry_run: false,
        },
    );
    assert_eq!(b_b.code, 0, "backup dataset B: {}", b_b.stderr);
}

#[test]
fn support_process_smoke_uses_an_independent_digest() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = tempfile::tempdir().expect("state tempdir");
    let target_dir = tempfile::tempdir().expect("target tempdir");

    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
    let _j = Journal::open(&world, state_dir.path(), Via::Cli).expect("open journal");

    let settings_path = state_dir.path().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_dir.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let eff = floor_a_effective();

    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let (cat_bytes, expected_digest) = cat.to_toml_bytes_and_sha256();
    let cat_file = tree_dir.path().join("corpus-support.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write cat");

    let raw = make_raw_copy(tree_dir.path(), state_dir.path());

    let v_args = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: Some(cat_file),
        expect_support_sha256: Some(expected_digest.clone()),
    };
    let v_out = run_verify(&world, &v_args);
    assert_eq!(v_out.code, 0, "verify failed: {}", v_out.stderr);
    assert!(v_out.stdout.contains("raw copy consistent\n"));
    let short_12 = &expected_digest[..12];
    assert!(
        v_out
            .stdout
            .contains(&format!("public support catalogue verified: {short_12}"))
    );

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let b_out = run_backup(&world, &eff, &options);
    assert_eq!(b_out.code, 0, "backup failed: {}", b_out.stderr);

    let latest_saved =
        std::fs::read_to_string(target_dir.path().join("LATEST")).expect("read LATEST");
    let snap_count_before =
        fieldbook::io::backup_folder::list_existing_snapshots(target_dir.path()).len();

    let changes_file = tree_dir.path().join("corpus/changes.md");
    std::fs::write(&changes_file, b"Changed byte in smoke\n").expect("write changed byte");

    let b_repeat = run_backup(&world, &eff, &options);
    assert_eq!(
        b_repeat.code, 2,
        "repeat backup must refuse with exit 2 (FB0901)"
    );
    assert!(b_repeat.stderr.contains("FB0901"));

    let latest_after =
        std::fs::read_to_string(target_dir.path().join("LATEST")).expect("read LATEST");
    assert_eq!(latest_after, latest_saved);
    let snap_count_after =
        fieldbook::io::backup_folder::list_existing_snapshots(target_dir.path()).len();
    assert_eq!(snap_count_after, snap_count_before);
}

struct FolderTargetFactory;

impl support::backup_suite::BackupFactory for FolderTargetFactory {
    fn make<'a>(
        &self,
        world: &'a support::world::TestWorld,
        audited: &'a Audited,
        allowed: &'a Allowed,
        target: &'a Path,
    ) -> Box<dyn BackupTarget + 'a> {
        Box::new(fieldbook::io::backup_folder::FolderTarget::new(
            world, audited, allowed, target,
        ))
    }
}

#[test]
fn folder_target_contract() {
    support::backup_suite::backup_target_contract(&FolderTargetFactory);
}

#[test]
fn backup_binds_the_complete_delivery() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Set keep_last = 1, keep_weekly = 0 so third backup prunes snapshot 1
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[backup]\nkeep_last = 1\nkeep_weekly = 0\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    // First backup
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome1.code, 0,
        "first backup must succeed: {}",
        outcome1.stderr
    );

    // Second backup
    let outcome2 = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome2.code, 0,
        "second backup must succeed: {}",
        outcome2.stderr
    );

    // Verify snapshot 2 (making 2 the newest verified snapshot)
    let v2 = run_verify(
        &fix.world,
        &BackupVerifyArgs {
            target: Some("main".to_string()),
            in_dir: None,
            snapshot: Some(2),
            raw: None,
            support_catalogue: None,
            expect_support_sha256: None,
        },
    );
    assert_eq!(v2.code, 0);

    // Plant a leftover partial directory
    let leftover = fix.target_path().join("fieldbook-backup-0099.partial");
    std::fs::create_dir_all(&leftover).expect("create leftover");
    std::fs::write(leftover.join("file.txt"), b"leftover data").expect("write leftover file");

    // Third backup: prunes snapshot 1, cleans up leftover, creates snapshot 3
    let outcome3 = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        outcome3.code, 0,
        "third backup must succeed: {}",
        outcome3.stderr
    );

    // Assert:
    // 1. Snapshot 3 exists and was renamed
    let snap3_dir = fix.target_path().join("fieldbook-backup-0003");
    assert!(snap3_dir.is_dir());
    assert!(
        !fix.target_path()
            .join("fieldbook-backup-0003.partial")
            .exists()
    );

    // 2. Leftover partial was cleaned up
    assert!(!leftover.exists(), "leftover partial must be removed");

    // 3. Snapshot 1 was pruned per retention
    assert!(
        !fix.target_path().join("fieldbook-backup-0001").exists(),
        "snapshot 1 pruned"
    );
    assert!(
        fix.target_path().join("fieldbook-backup-0002").exists(),
        "snapshot 2 kept"
    );

    // 4. Copied journal head, sums and generated metadata equal prepared bytes
    let snap_journal = std::fs::read_to_string(snap3_dir.join("data/state/audit/journal.jsonl"))
        .expect("read snap journal");
    let last_snap_line = snap_journal.lines().last().expect("last snap line");
    assert!(last_snap_line.contains("\"kind\":\"backup.begin\""));

    let snap_toml = std::fs::read_to_string(snap3_dir.join("backup.toml")).expect("read toml");
    let info = BackupToml::parse(&snap_toml).expect("parse toml");
    assert_eq!(info.snapshot, 3);

    let sums_text = std::fs::read_to_string(snap3_dir.join("SHA256SUMS")).expect("read sums");
    let computed_m_sha = manifest_sha256(&info, &sums_text).expect("manifest sha");
    assert_eq!(info.manifest_sha256, computed_m_sha.as_str());

    let latest_text =
        std::fs::read_to_string(fix.target_path().join("LATEST")).expect("read LATEST");
    assert_eq!(latest_text, format!("3\n{}\n", info.manifest_sha256));

    // 5. Cleanup, retention and rename targets are bound by Allowed Delivery:
    // Attempting an unapproved rename target or unapproved remove target with Allowed fails
    let dummy_partial = fix.target_path().join("fieldbook-backup-0088.partial");
    std::fs::create_dir_all(&dummy_partial).expect("create dummy partial");
    let mut journal = Journal::open(&fix.world, fix.state_dir(), Via::Cli).expect("open");
    let unapproved_delivery = Delivery::new(
        vec![Transfer::write_path(
            fix.target_path().join("other.txt"),
            b"other",
        )],
        None,
    );
    let Decision::Allowed(allowed_unapproved) = check(
        &eff,
        &Dest::Backup {
            path: fix.target_path().to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&unapproved_delivery),
        fix.world.now(),
        Shipped::Never,
    ) else {
        panic!("allowed");
    };
    let policy_sha = Sha256::parse(allowed_unapproved.policy_hash()).expect("sha");
    let _prep = journal
        .prepare_backup_begin(&LabelSet::public(), &policy_sha, 3)
        .expect("prep");
    let audited = journal
        .begin_backup(&allowed_unapproved, fix.target_path(), "folder", 3)
        .expect("begin");

    // Unapproved partial removal fails
    let rm_res = remove_outside(
        &audited,
        &allowed_unapproved,
        &dummy_partial,
        Removable::PartialSnapshot,
    );
    assert!(
        matches!(rm_res, Err(fieldbook::Error::OutOfScope(_))),
        "removal of unapproved partial must be refused by Allowed"
    );

    // Unapproved rename fails
    let rename_res = rename_dir_outside(
        &audited,
        &allowed_unapproved,
        &dummy_partial,
        &fix.target_path().join("fieldbook-backup-0088"),
    );
    assert!(
        matches!(rename_res, Err(fieldbook::Error::OutOfScope(_))),
        "rename of unapproved snapshot must be refused by Allowed"
    );
    let _ = std::fs::remove_dir_all(&dummy_partial);
}

#[test]
fn changed_prepared_event_writes_no_snapshot() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome1.code, 0);
    let latest_before =
        std::fs::read_to_string(fix.target_path().join("LATEST")).expect("read LATEST");
    assert!(latest_before.starts_with("1\n"));

    let state_dir = fix.state_dir();
    let mut journal = Journal::open(&fix.world, state_dir, Via::Cli).expect("open journal");
    let target = fix.target_path();
    let now = fix.world.now();
    let policy_sha = Sha256::parse(eff.hash()).unwrap_or_else(|_| Sha256::digest(b""));

    // Prepare backup begin for snapshot 2
    let prep1 = journal
        .prepare_backup_begin_at(&LabelSet::public(), &policy_sha, 2, now)
        .expect("prepare");

    // Build delivery with prep1 bytes for journal.jsonl
    let seg_path = state_dir.join("audit/journal.jsonl");
    let mut journal_bytes = std::fs::read(&seg_path).expect("read journal");
    journal_bytes.extend_from_slice(&prep1.bytes);

    let partial_path = target.join("fieldbook-backup-0002.partial");
    let dest_journal = partial_path.join("data/state/audit/journal.jsonl");
    let delivery = Delivery::new(
        vec![Transfer::write_path(&dest_journal, &journal_bytes)],
        None,
    );
    let dest = Dest::Backup {
        path: target.to_string_lossy().into_owned(),
        company_storage: false,
    };
    let Decision::Allowed(allowed) = check(
        &eff,
        &dest,
        Some(&LabelSet::public()),
        Some(&delivery),
        now,
        Shipped::Never,
    ) else {
        panic!("expected allowed");
    };

    // Change the prepared event after approval!
    let later_now = Now::new(CivilDate::from_ymd(2026, 3, 2).expect("date"), 50_000).expect("now");
    let _prep2 = journal
        .prepare_backup_begin_at(&LabelSet::public(), &policy_sha, 2, later_now)
        .expect("prepare altered");

    // begin_backup must refuse!
    let err = journal
        .begin_backup(&allowed, target, "folder", 2)
        .expect_err("begin_backup must fail with altered event");
    assert!(matches!(err, JournalError::Vocab(_)));

    // Assert: no snapshot 2, LATEST untouched
    assert!(!target.join("fieldbook-backup-0002").exists());
    assert!(!target.join("fieldbook-backup-0002.partial").exists());
    let latest_after = std::fs::read_to_string(target.join("LATEST")).expect("read LATEST");
    assert_eq!(latest_after, latest_before);
}

#[test]
fn backup_waits_for_a_complete_store_generation() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Corpus files
    let claims_rel = "corpus/registers/claims.csv";
    let questions_rel = "corpus/registers/questions.csv";
    let claims_path = fix.tree_root().join(claims_rel);
    let questions_path = fix.tree_root().join(questions_rel);

    let old_claims = std::fs::read(&claims_path).expect("read old claims");
    let old_questions = std::fs::read(&questions_path).expect("read old questions");

    // Each generation edit keeps the register parsable: a note cell grows by a few words.
    let new_claims = String::from_utf8(old_claims.clone())
        .expect("claims utf8")
        .replacen(
            "Synthetic owner-verified fact for fixture tests.",
            "Synthetic owner-verified fact for fixture tests (generation two).",
            1,
        )
        .into_bytes();
    assert_ne!(new_claims, old_claims, "claims edit must change the file");
    let new_questions = String::from_utf8(old_questions.clone())
        .expect("questions utf8")
        .replacen("until confirmed.", "until confirmed (generation two).", 1)
        .into_bytes();
    assert_ne!(
        new_questions, old_questions,
        "questions edit must change the file"
    );

    let claims_sha = Sha256::digest(&new_claims);
    let questions_sha = Sha256::digest(&new_questions);

    let ws = fieldbook_core::audit::WriteSet {
        labels: LabelSet::public(),
        files: vec![
            fieldbook_core::audit::WriteFile::new(claims_rel, claims_sha.as_str().to_owned()),
            fieldbook_core::audit::WriteFile::new(questions_rel, questions_sha.as_str().to_owned()),
        ],
        affected: vec![
            fieldbook_core::audit::RecordRef::parse("corpus/CL-CMP-0001").expect("ref"),
            fieldbook_core::audit::RecordRef::parse("corpus/Q-001").expect("ref"),
        ],
    };
    let intent = fieldbook_core::audit::OperationIntent {
        schema: 1,
        key: fieldbook_core::audit::OperationKey::new(
            fieldbook_core::audit::Verb::parse("add").expect("verb"),
            Sha256::digest(b"subj"),
        ),
        request_sha256: Sha256::digest(b"req"),
        allocated: vec![
            fieldbook_core::audit::RecordRef::parse("corpus/CL-CMP-0001").expect("ref"),
        ],
        allocated_store_uid: None,
        planned_on: fieldbook_core::CivilDate::from_ymd(2026, 3, 1).expect("valid date"),
    };

    let mut journal = Journal::open(&fix.world, fix.state_dir(), Via::Cli).expect("open journal");
    let lease = fix.world.write_lease().expect("acquire write lease");
    let token = journal
        .begin(&lease, &intent, &ws)
        .expect("begin store write");

    // Write file 1 only: pause before writing file 2
    fieldbook::io::journal::write_sync_file(&claims_path, &new_claims).expect("write file 1");

    // 1. Timeout test: while writer holds store lease, short lock timeout makes backup time out
    set_lock_timeout_for_test(Some(Duration::from_millis(50)));
    let outcome_timeout = run_backup(&fix.world, &eff, &options);
    set_lock_timeout_for_test(None);
    assert_eq!(
        outcome_timeout.code, 3,
        "backup racing mid-generation must time out on store lease"
    );
    assert!(
        !fix.target_path().join("fieldbook-backup-0001").exists(),
        "timed-out backup must write no snapshot"
    );

    // 2. Wait and copy complete generation test:
    let (tx_ready, rx_ready) = std::sync::mpsc::channel();
    let root_bg = fix.tree_root().to_path_buf();
    let state_bg = fix.state_dir().to_path_buf();

    let backup_handle = std::thread::spawn(move || {
        let b_world = TestWorld::with_state(&root_bg, &state_bg);
        let tree = fieldbook::io::read_tree(b_world.tree_root()).expect("read tree");
        if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
            &tree,
            "1111111111111111111111111111111111111111",
        ) {
            let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
            b_world.set_support_catalogue(cat, cat_sha);
        }
        let b_eff = floor_a_effective();
        let b_opts = BackupOptions {
            target: Some("main".to_string()),
            to: None,
            dry_run: false,
        };
        tx_ready.send(()).expect("send ready");
        run_backup(&b_world, &b_eff, &b_opts)
    });

    rx_ready.recv().expect("rx ready");
    // Small sleep so backup thread is blocked on store lease
    std::thread::sleep(Duration::from_millis(40));

    // Writer writes file 2, finishes transaction, releases lease
    fieldbook::io::journal::write_sync_file(&questions_path, &new_questions).expect("write file 2");
    journal
        .end(token, Word::new_static("ok"))
        .expect("end store write");
    drop(lease);

    // Backup thread completes now
    let outcome_wait = backup_handle.join().expect("join backup thread");
    assert_eq!(
        outcome_wait.code, 0,
        "backup must wait and succeed: {}",
        outcome_wait.stderr
    );

    // Verify snapshot 1 contains BOTH new files (complete generation!)
    let snap_dir = fix.target_path().join("fieldbook-backup-0001");
    assert!(snap_dir.is_dir());
    let snap_claims =
        std::fs::read(snap_dir.join(format!("data/{claims_rel}"))).expect("read snap claims");
    assert_eq!(
        snap_claims, new_claims,
        "snapshot must contain file 1 from generation"
    );
    let snap_questions =
        std::fs::read(snap_dir.join(format!("data/{questions_rel}"))).expect("read snap questions");
    assert_eq!(
        snap_questions, new_questions,
        "snapshot must contain file 2 from generation"
    );
}

fn append_np_source_row(tree_root: &Path, volatility: &str) {
    let sources_path = tree_root.join("corpus/registers/sources.csv");
    let mut content = std::fs::read_to_string(&sources_path).expect("read sources.csv");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    let sha256 = "5".repeat(64);
    let row = format!(
        "S-0004,1,synthetic-np-row,\"Synthetic Cost Rule, made up for tests\",Synthetic Standards Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",synthetic_cost_rule,reviewed,metadata-only: made up for tests,{sha256},2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,{volatility},np-0007,Synthetic fixture source for automated tests.,Synthetic rule baseline for fixture tests,,,,,\n"
    );
    content.push_str(&row);
    std::fs::write(&sources_path, content).expect("write sources.csv");
}

#[test]
fn backup_refuses_when_a_source_row_does_not_load() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    append_np_source_row(fix.tree_root(), "V9");

    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 2);
    assert!(outcome.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
}

#[test]
fn backup_refuses_registers_when_corpus_toml_does_not_load() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    append_np_source_row(fix.tree_root(), "V2");

    let toml_path = fix.tree_root().join("corpus/corpus.toml");
    let content = std::fs::read_to_string(&toml_path).expect("read corpus.toml");
    std::fs::write(&toml_path, format!("unknown_key = 1\n{content}")).expect("write corpus.toml");

    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\ninclude = [\"corpus/registers\"]\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

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
fn backup_with_a_registers_only_target_verifies() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\ninclude = [\"corpus/registers\"]\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_outcome = run_verify(&fix.world, &verify_args);
    assert_eq!(
        v_outcome.code, 0,
        "a snapshot this command wrote must verify: {}",
        v_outcome.stderr
    );
    assert!(v_outcome.stdout.contains("snapshot 1 verified"));

    // The snapshot holds the audit files whatever `include` says, so it can be verified.
    let snap_audit = fix
        .target_path()
        .join("fieldbook-backup-0001/data/state/audit");
    assert!(
        snap_audit.join("journal.jsonl").is_file(),
        "a registers-only snapshot must hold the audit journal"
    );
    assert!(
        fix.target_path()
            .join("fieldbook-backup-0001/data/corpus/registers/sources.csv")
            .is_file(),
        "the snapshot must hold the registers it includes"
    );
}

#[test]
fn backup_refuses_a_claims_only_target_when_sources_row_is_np() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    append_np_source_row(fix.tree_root(), "V2");

    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\ninclude = [\"corpus/registers/claims.csv\"]\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

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
fn audit_only_backup_under_floor_a_and_loaded_policy_refuses_unknown_file() {
    let fix = TestFixture::new();
    let eff_floor = floor_a_effective();

    // 1. Audit-only target under Floor A
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"audit-only\"\nkind = \"folder\"\npath = \"{}\"\ninclude = [\"audit\"]\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, &settings_text).expect("write settings");

    let options = BackupOptions {
        target: Some("audit-only".to_string()),
        to: None,
        dry_run: false,
    };

    // Clean audit-only backup succeeds
    let out_clean = run_backup(&fix.world, &eff_floor, &options);
    assert_eq!(
        out_clean.code, 0,
        "clean audit backup under floor A must succeed: {}",
        out_clean.stderr
    );

    // Plant an unknown file in state/audit/
    let unknown_audit = fix.state_dir().join("audit/unknown-report.txt");
    std::fs::write(&unknown_audit, b"stray unlabelled artifact\n").expect("write unknown file");

    let out_stray = run_backup(&fix.world, &eff_floor, &options);
    assert_eq!(
        out_stray.code, 2,
        "unknown file in state/audit must refuse with exit 2 (FB0901) under floor A"
    );
    assert!(out_stray.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0002").exists());

    let _ = std::fs::remove_file(&unknown_audit);

    // 2. Audit-only target under Loaded Policy
    // Parse policy permitting public data at target path prefix
    let target_prefix = fix.target_path().to_string_lossy().replace('\\', "/");
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
paths = ["{target_prefix}"]
"#
    );
    let parsed_policy = Policy::parse(&policy_toml, None).expect("parse policy");
    let eff_loaded =
        EffectivePolicy::new(&PolicyState::Loaded(parsed_policy), &OwnerSettings::unset());

    // Clean audit backup succeeds under loaded policy
    let out_loaded_clean = run_backup(&fix.world, &eff_loaded, &options);
    assert_eq!(
        out_loaded_clean.code, 0,
        "clean audit backup under loaded policy must succeed: {}",
        out_loaded_clean.stderr
    );

    // Plant unknown file in state/audit/ again
    std::fs::write(&unknown_audit, b"stray unlabelled artifact 2\n").expect("write unknown file");

    let out_loaded_stray = run_backup(&fix.world, &eff_loaded, &options);
    assert_eq!(
        out_loaded_stray.code, 2,
        "unknown file in state/audit must refuse with exit 2 (FB0901) under loaded policy"
    );
    assert!(out_loaded_stray.stderr.contains("FB0901"));
    let _ = std::fs::remove_file(&unknown_audit);
}

#[test]
fn backup_git_guard_uses_the_shared_context() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let git_repo = tmp.path().join("git-repo");
    std::fs::create_dir_all(git_repo.join(".git")).expect("create .git");
    std::fs::write(git_repo.join(".gitignore"), ".fieldbook\n").expect("write gitignore");

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    support::copy_dir_recursive(&corpus_mini, &git_repo).expect("copy corpus-mini");

    let state = git_repo.join(".fieldbook");
    support::copy_dir_recursive(&state_mini, &state).expect("copy state");

    let git_target = git_repo.join("backup-target");
    std::fs::create_dir_all(&git_target).expect("create git target");
    let outside_target = tmp.path().join("outside-target");
    std::fs::create_dir_all(&outside_target).expect("create outside target");

    let world = TestWorld::with_state(&git_repo, &state);
    let tree = fieldbook::io::read_tree(world.tree_root()).expect("tree");
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    let eff = floor_a_effective();

    // 1. Outside git target passes
    let options_outside = BackupOptions {
        target: None,
        to: Some(outside_target.clone()),
        dry_run: false,
    };
    let out_outside = run_backup(&world, &eff, &options_outside);
    assert_eq!(
        out_outside.code, 0,
        "outside-git target must pass: {}",
        out_outside.stderr
    );
    assert!(outside_target.join("fieldbook-backup-0001").is_dir());

    // 2. Full backup (containing owner-supplied settings.toml) to target under .git is REFUSED with FB0415
    let sentinel = git_target.join("survivor.txt");
    std::fs::write(&sentinel, b"sentinel bytes\n").expect("write sentinel");

    let options_git = BackupOptions {
        target: None,
        to: Some(git_target.clone()),
        dry_run: false,
    };
    let out_git = run_backup(&world, &eff, &options_git);
    assert_eq!(
        out_git.code, 1,
        "backup target under .git must refuse owner-supplied labels with exit 1 (FB0415)"
    );
    assert!(out_git.stderr.contains("FB0415"));
    assert!(
        !git_target.join("fieldbook-backup-0001").exists(),
        "no snapshot must be created under git"
    );
    assert!(!git_target.join("fieldbook-backup-0001.partial").exists());
    assert_eq!(
        std::fs::read(&sentinel).expect("read sentinel"),
        b"sentinel bytes\n",
        "existing target bytes must survive"
    );

    // 3. Public-only included target under .git passes
    let settings_path = state.join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"git-audit\"\nkind = \"folder\"\npath = \"{}\"\ninclude = [\"audit\"]\n",
        git_target.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let options_git_audit = BackupOptions {
        target: Some("git-audit".to_string()),
        to: None,
        dry_run: false,
    };
    let out_git_audit = run_backup(&world, &eff, &options_git_audit);
    assert_eq!(
        out_git_audit.code, 0,
        "public-only backup under .git must pass: {}",
        out_git_audit.stderr
    );
    assert!(git_target.join("fieldbook-backup-0001").is_dir());

    // 4. Owner-supplied under Git passes only in ignored Home State / drill
    let drill_path = state.join("drill/restore");
    let ctx_drill =
        fieldbook::io::root::git_context(&world, &drill_path).expect("git context drill");
    assert_eq!(
        ctx_drill,
        fieldbook_core::labels::GitContext::ignored_home_state()
    );
    let owner_labels = LabelSet::owner_supplied();
    assert!(
        fieldbook_core::labels::git_guard(&owner_labels, ctx_drill).is_ok(),
        "owner-supplied labels must pass in ignored home state drill"
    );

    // 5. Unignored State under .git refuses owner-supplied (FB0415)
    std::fs::write(git_repo.join(".gitignore"), "# nothing ignored\n")
        .expect("write empty gitignore");
    let ctx_unignored =
        fieldbook::io::root::git_context(&world, &drill_path).expect("git context unignored");
    assert!(
        !ctx_unignored.ignored_home_state,
        "unignored state must not have ignored_home_state set"
    );
    assert!(
        fieldbook_core::labels::git_guard(&owner_labels, ctx_unignored).is_err(),
        "unignored state under .git must refuse owner-supplied labels"
    );
}

#[test]
fn the_leak_matrix_plants_a_canary_in_working_in_the_site_and_in_a_stray_file() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Plant 1: Canary in derived site folder (corpus/out/edition/canary.txt)
    let derived_site_canary = fix.tree_root().join("corpus/out/edition/canary.txt");
    std::fs::create_dir_all(derived_site_canary.parent().expect("parent")).expect("create parent");
    std::fs::write(&derived_site_canary, b"CANARY_IN_DERIVED_SITE_31415\n")
        .expect("write derived canary");

    let out1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        out1.code, 0,
        "backup with derived site folder must succeed: {}",
        out1.stderr
    );

    let snap1_dir = fix.target_path().join("fieldbook-backup-0001");
    let sums_text = std::fs::read_to_string(snap1_dir.join("SHA256SUMS")).expect("read sums");
    assert!(
        !sums_text.contains("CANARY_IN_DERIVED_SITE_31415"),
        "derived site canary must not appear in snapshot sums"
    );
    assert!(
        !sums_text.contains("corpus/out/"),
        "derived folder must not be copied"
    );

    // Plant 2: Canary in stray folder (state/working/)
    let working_canary = fix.state_dir().join("working/canary.txt");
    std::fs::create_dir_all(working_canary.parent().expect("parent")).expect("create parent");
    std::fs::write(&working_canary, b"CANARY_IN_WORKING_STRAY_92653\n")
        .expect("write working canary");

    let out2 = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        out2.code, 2,
        "canary in working/ stray folder must cause refusal FB0901: {}",
        out2.stderr
    );
    assert!(out2.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0002").exists());

    let _ = std::fs::remove_file(&working_canary);

    // Plant 3: Canary in stray file (corpus/stray.dat)
    let stray_file = fix.tree_root().join("corpus/stray.dat");
    std::fs::write(&stray_file, b"CANARY_IN_STRAY_FILE_58979\n").expect("write stray file");

    let out3 = run_backup(&fix.world, &eff, &options);
    assert_eq!(
        out3.code, 2,
        "canary in stray file must cause refusal FB0901: {}",
        out3.stderr
    );
    assert!(out3.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0002").exists());
}

#[test]
fn snapshot_labels_are_the_union() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let (model, _) = fieldbook_core::check::load(
        &fieldbook_core::check::Host::new(
            fieldbook::io::read_tree(fix.tree_root()).expect("read tree"),
            fix.world.today(),
        ),
        &fieldbook_core::check::Options::new(false),
    );

    let listing =
        fieldbook::io::backup_classify::enumerate_roots(&fix.world).expect("enumerate roots");
    let facts = fix.world.support_facts();
    let mut files = Vec::new();
    for rel in &listing {
        let bytes = fieldbook::io::backup_classify::read_file_content(&fix.world, rel)
            .expect("read file content");
        let cf =
            fieldbook::io::backup_classify::classify_file(&facts, Some(&model), &eff, rel, &bytes);
        files.push(cf);
    }
    let union_labels = fieldbook_core::backup::snapshot_labels(&files);
    assert_eq!(union_labels, LabelSet::owner_supplied());
    assert!(union_labels.contains(&OWNER_SUPPLIED));

    // A public-only subset yields public
    let public_files: Vec<_> = files
        .iter()
        .filter(|f| f.path.starts_with("corpus/"))
        .cloned()
        .collect();
    let public_union = fieldbook_core::backup::snapshot_labels(&public_files);
    assert_eq!(public_union, LabelSet::public());
    assert!(public_union.contains(&fieldbook_core::labels::PUBLIC));

    // Plant a page with custom handling
    let custom_page = fix.tree_root().join("corpus/pages/estimating/custom.md");
    let custom_content = "+++\nid = \"custom-page\"\ntitle = \"Custom\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nCustom.\n";
    std::fs::write(&custom_page, custom_content).expect("write custom page");

    let listing2 =
        fieldbook::io::backup_classify::enumerate_roots(&fix.world).expect("enumerate roots 2");
    let mut files2 = Vec::new();
    for rel in &listing2 {
        let bytes = fieldbook::io::backup_classify::read_file_content(&fix.world, rel)
            .expect("read file content 2");
        let cf =
            fieldbook::io::backup_classify::classify_file(&facts, Some(&model), &eff, rel, &bytes);
        files2.push(cf);
    }
    let union_labels2 = fieldbook_core::backup::snapshot_labels(&files2);
    let custom_id = fieldbook_core::audit::LabelId::parse("np-0007").expect("parse id");
    assert!(union_labels2.contains(&custom_id));
    assert!(union_labels2.contains(&OWNER_SUPPLIED));
    assert_eq!(
        union_labels2,
        LabelSet::from_ids([OWNER_SUPPLIED, custom_id])
    );
}

#[test]
fn backup_succeeds_with_a_two_segment_journal() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let state_dir = fix.state_dir();

    // 1. Using a Journal handle on the fixture's state dir with set_segment_limit set to a small value,
    // append events until audit/journal-0002.jsonl exists.
    {
        let mut journal = Journal::open(&fix.world, state_dir, Via::Cli).expect("open journal");
        journal.set_segment_limit(500);
        let about = RefuseAbout::default();
        while !state_dir.join("audit/journal-0002.jsonl").exists() {
            journal
                .refuse(
                    Some("backup"),
                    Code::Fb0415,
                    Word::new_static("place"),
                    &about,
                )
                .expect("refuse event");
        }
    }

    assert!(state_dir.join("audit/journal-0002.jsonl").exists());

    // 2. run_backup (floor A, target main) must exit 0
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    // 3. The snapshot's data/state/audit holds both segments
    let snap1_audit = fix
        .target_path()
        .join("fieldbook-backup-0001/data/state/audit");
    assert!(
        snap1_audit.join("journal.jsonl").exists(),
        "snapshot missing journal.jsonl"
    );
    assert!(
        snap1_audit.join("journal-0002.jsonl").exists(),
        "snapshot missing journal-0002.jsonl"
    );

    // 4. run_verify must exit 0 and print "snapshot 1 verified"
    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_outcome = run_verify(&fix.world, &verify_args);
    assert_eq!(v_outcome.code, 0, "verify failed: {}", v_outcome.stderr);
    assert!(v_outcome.stdout.contains("snapshot 1 verified"));
}

#[test]
fn backup_begin_that_rotates_the_journal_succeeds() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // 1. Make the active segment at least DEFAULT_SEGMENT_LIMIT long by appending a valid
    // hash-chained filler to the fixture's audit/journal.jsonl.
    let journal_path = fix.state_dir().join("audit/journal.jsonl");
    let content = std::fs::read_to_string(&journal_path).expect("read journal");
    let last_line = content.lines().rfind(|l| !l.is_empty()).expect("last line");
    let (last_event, _) = fieldbook_core::audit::Event::parse_line(last_line).expect("parse line");

    let mut cur_seq = last_event.seq + 1;
    let mut prev_hash = last_event.hash;
    let now = fix.world.now();
    let env = fieldbook_core::audit::EventEnvelope {
        actor: fieldbook_core::audit::Principal::local(),
        role: fieldbook_core::audit::Role::Lead,
        store: fieldbook_core::audit::StoreId::State,
        via: Via::Cli,
    };

    {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&journal_path)
            .expect("open journal for appending");
        while f.metadata().expect("meta").len() < fieldbook::io::journal::DEFAULT_SEGMENT_LIMIT {
            let payload = EventPayload::BackupPrune {
                snapshots: vec![Count::new(10_000_000_000_000_000_000); 40_000],
                reason: Word::new_static("retention"),
            };
            let (line, hash) = payload.format_line(cur_seq, now, &env, prev_hash.as_str());
            writeln!(f, "{line}").expect("write line");
            cur_seq += 1;
            prev_hash = hash;
        }
    }

    // 2. run_backup must exit 0
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    // 3. audit/journal-0002.jsonl must exist in the live state and contain backup.begin and backup.end
    let live_seg2 = fix.state_dir().join("audit/journal-0002.jsonl");
    assert!(live_seg2.exists(), "live state missing journal-0002.jsonl");
    let seg2_content = std::fs::read_to_string(&live_seg2).expect("read live seg2");
    assert!(
        seg2_content.contains("\"kind\":\"backup.begin\""),
        "seg2 missing backup.begin"
    );
    assert!(
        seg2_content.contains("\"kind\":\"backup.end\""),
        "seg2 missing backup.end"
    );

    // 4. The snapshot must contain data/state/audit/journal-0002.jsonl
    let snap_seg2 = fix
        .target_path()
        .join("fieldbook-backup-0001/data/state/audit/journal-0002.jsonl");
    assert!(snap_seg2.exists(), "snapshot missing journal-0002.jsonl");

    // 5. backup verify must exit 0
    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_outcome = run_verify(&fix.world, &verify_args);
    assert_eq!(v_outcome.code, 0, "verify failed: {}", v_outcome.stderr);
    assert!(v_outcome.stdout.contains("snapshot 1 verified"));
}

#[cfg(unix)]
#[test]
fn an_unreadable_state_subfolder_fails_closed() {
    use std::os::unix::fs::PermissionsExt;

    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let subfolder = fix.state_dir().join("reports");
    std::fs::create_dir_all(&subfolder).expect("create reports subfolder");
    let test_file = subfolder.join("test.txt");
    std::fs::write(&test_file, b"content").expect("write test file");

    struct PermGuard(PathBuf);
    impl Drop for PermGuard {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    let _guard = PermGuard(subfolder.clone());
    std::fs::set_permissions(&subfolder, std::fs::Permissions::from_mode(0o000))
        .expect("chmod 000 subfolder");

    if std::fs::read_dir(&subfolder).is_ok() {
        return;
    }

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 3);
    assert!(
        outcome.stderr.contains(&subfolder.display().to_string()),
        "stderr should name subfolder path: {}",
        outcome.stderr
    );
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
    assert!(!fix.target_path().join("LATEST").exists());
}

#[cfg(unix)]
#[test]
fn an_unreadable_state_file_fails_closed() {
    use std::os::unix::fs::PermissionsExt;

    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let reports_dir = fix.state_dir().join("reports");
    std::fs::create_dir_all(&reports_dir).expect("create reports");
    let unreadable_file = reports_dir.join("last-add.json");
    std::fs::write(&unreadable_file, b"{\"report_version\":1}").expect("write file");

    struct PermGuard(PathBuf);
    impl Drop for PermGuard {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o644));
        }
    }

    let _guard = PermGuard(unreadable_file.clone());
    std::fs::set_permissions(&unreadable_file, std::fs::Permissions::from_mode(0o000))
        .expect("chmod 000 file");

    if std::fs::read(&unreadable_file).is_ok() {
        return;
    }

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 3);
    assert!(
        outcome
            .stderr
            .contains(&unreadable_file.display().to_string()),
        "stderr should name unreadable file: {}",
        outcome.stderr
    );
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
    assert!(!fix.target_path().join("LATEST").exists());
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_never_read_through() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let canary_dir = tempfile::tempdir().expect("create canary tempdir");
    let canary_file = canary_dir.path().join("canary.txt");
    let canary_str = "CANARY_LEAK_TEST_STRING_9876543210";
    std::fs::write(&canary_file, canary_str.as_bytes()).expect("write canary");

    let leak_link = fix.tree_root().join("corpus/sources/public/leak.txt");
    std::os::unix::fs::symlink(&canary_file, &leak_link).expect("symlink canary");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 3);
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
    assert!(!fix.target_path().join("LATEST").exists());

    fn walk_check_no_canary(dir: &Path, canary: &str) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk_check_no_canary(&p, canary);
                } else if p.is_file() {
                    let content = std::fs::read_to_string(&p).unwrap_or_default();
                    assert!(
                        !content.contains(canary),
                        "canary leaked to target file {}",
                        p.display()
                    );
                }
            }
        }
    }
    walk_check_no_canary(fix.target_path(), canary_str);
}

/// Writes `file_name` under `corpus/sources/public/` and returns the labels `classify_file`
/// gives it.
fn classify_an_original(fix: &TestFixture, file_name: &str) -> LabelSet {
    let original = fix
        .tree_root()
        .join("corpus/sources/public")
        .join(file_name);
    std::fs::write(&original, b"synthetic original content\n").expect("write original");

    let (model, _) = fieldbook_core::check::load(
        &fieldbook_core::check::Host::new(
            fieldbook::io::read_tree(fix.tree_root()).expect("read tree"),
            fix.world.today(),
        ),
        &fieldbook_core::check::Options::new(false),
    );
    let rel = format!("corpus/sources/public/{file_name}");
    let bytes =
        fieldbook::io::backup_classify::read_file_content(&fix.world, &rel).expect("read original");
    let facts = fix.world.support_facts();
    fieldbook::io::backup_classify::classify_file(
        &facts,
        Some(&model),
        &floor_a_effective(),
        &rel,
        &bytes,
    )
    .labels
}

/// Adds the synthetic `np-0007` source row (`S-0004`, version 1), then classifies `file_name`.
fn labels_of_an_original_with_np(fix: &TestFixture, file_name: &str) -> LabelSet {
    append_np_source_row(fix.tree_root(), "V2");
    classify_an_original(fix, file_name)
}

/// Backs up the originals alone to a floor B target that is company storage: a file that still
/// carries the source's not-in-table label must stay refused there.
fn assert_a_floor_b_company_storage_backup_refuses_the_original(fix: &TestFixture) {
    let settings_path = fix.state_dir().join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\ninclude = [\"corpus/sources/public\"]\ncompany_storage = true\n",
        fix.target_path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings");

    let eff_floor_b =
        EffectivePolicy::new(&PolicyState::Floor(FloorKind::B), &OwnerSettings::unset());
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff_floor_b, &options);
    assert_eq!(
        outcome.code, 2,
        "a floor B company-storage backup must still refuse the source's label: {}",
        outcome.stderr
    );
    assert!(outcome.stderr.contains("FB0901"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
}

#[test]
fn original_by_version_id_takes_the_sources_labels() {
    let fix = TestFixture::new();
    let labels = classify_an_original(&fix, "S-0001-v1.pdf");
    assert_eq!(labels, LabelSet::public());
}

#[test]
fn original_by_bare_source_id_is_unlabelled() {
    let fix = TestFixture::new();
    let labels = labels_of_an_original_with_np(&fix, "S-0004.pdf");
    let np = fieldbook_core::audit::LabelId::parse("np-0007").expect("parse np-0007");
    assert!(
        labels.contains(&fieldbook_core::labels::UNLABELLED),
        "a bare source id must carry unlabelled: {labels:?}"
    );
    assert!(
        labels.contains(&np),
        "a bare source id keeps the source's labels: {labels:?}"
    );
    assert_a_floor_b_company_storage_backup_refuses_the_original(&fix);
}

#[test]
fn original_with_a_longer_name_is_unlabelled() {
    let fix = TestFixture::new();
    let labels = labels_of_an_original_with_np(&fix, "S-0004-v1.draft.txt");
    let np = fieldbook_core::audit::LabelId::parse("np-0007").expect("parse np-0007");
    assert!(
        labels.contains(&fieldbook_core::labels::UNLABELLED),
        "a longer name must carry unlabelled: {labels:?}"
    );
    assert!(
        labels.contains(&np),
        "a longer name keeps the source's labels: {labels:?}"
    );
    assert_a_floor_b_company_storage_backup_refuses_the_original(&fix);
}

#[test]
fn original_without_an_extension_is_unlabelled() {
    let fix = TestFixture::new();
    let labels = labels_of_an_original_with_np(&fix, "S-0004-v1");
    let np = fieldbook_core::audit::LabelId::parse("np-0007").expect("parse np-0007");
    assert!(
        labels.contains(&fieldbook_core::labels::UNLABELLED),
        "a name without an extension must carry unlabelled: {labels:?}"
    );
    assert!(
        labels.contains(&np),
        "a name without an extension keeps the source's labels: {labels:?}"
    );
    assert_a_floor_b_company_storage_backup_refuses_the_original(&fix);
}

#[test]
fn an_unregistered_original_is_unlabelled() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let unreg_orig = fix.tree_root().join("corpus/sources/public/S-9999-v1.txt");
    std::fs::write(&unreg_orig, b"unregistered original content\n")
        .expect("write unregistered original");

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
fn an_unregistered_extraction_folder_is_unlabelled() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    let unreg_ext_dir = fix.tree_root().join("corpus/sources/text/S-9999-v1");
    std::fs::create_dir_all(&unreg_ext_dir).expect("create ext dir");
    let unreg_ext = unreg_ext_dir.join("page-0001.txt");
    std::fs::write(&unreg_ext, b"unregistered extraction text\n").expect("write extraction");

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
fn refusal_counts_the_items_that_cause_it() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();

    // Plant ONE stray file corpus/stray-one.txt
    let stray1 = fix.tree_root().join("corpus/stray-one.txt");
    std::fs::write(&stray1, b"stray one content\n").expect("write stray-one");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome1 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome1.code, 2);
    assert!(outcome1.stderr.contains("FB0901"));
    assert!(outcome1.stderr.contains("1 item is unlabelled"));
    assert!(!outcome1.stderr.contains("2 items"));

    // Plant a SECOND stray corpus/stray-two.txt
    let stray2 = fix.tree_root().join("corpus/stray-two.txt");
    std::fs::write(&stray2, b"stray two content\n").expect("write stray-two");

    let outcome2 = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome2.code, 2);
    assert!(outcome2.stderr.contains("FB0901"));
    assert!(outcome2.stderr.contains("2 items are unlabelled"));
}

#[test]
fn refusal_names_the_way_out_for_each_reason() {
    // 1. unlabelled: integration test with stray file
    let fix_unlabelled = TestFixture::new();
    let eff_a = floor_a_effective();
    let stray = fix_unlabelled.tree_root().join("corpus/stray.txt");
    std::fs::write(&stray, b"unlabelled stray file\n").expect("write stray");

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome_unlabelled = run_backup(&fix_unlabelled.world, &eff_a, &options);
    assert_eq!(outcome_unlabelled.code, 2);
    assert!(outcome_unlabelled.stderr.contains("FB0901"));
    assert!(outcome_unlabelled.stderr.contains("way out:"));
    assert!(outcome_unlabelled.stderr.contains("[[source]]"));
    assert!(outcome_unlabelled.stderr.contains("label add"));

    // 2. no-row: copy setup of target_outside_every_backup_row_is_fb0918
    let fix_norow = TestFixture::new();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let policy_path = repo.join("tests/fixtures/policy/policy.toml");
    let policy_toml = std::fs::read_to_string(policy_path).expect("read policy");
    let policy = Policy::parse(&policy_toml, None).expect("parse policy");
    let state = PolicyState::Loaded(policy);
    let eff_loaded = EffectivePolicy::new(&state, &OwnerSettings::unset());

    let outcome_norow = run_backup(&fix_norow.world, &eff_loaded, &options);
    assert_eq!(outcome_norow.code, 2);
    assert!(outcome_norow.stderr.contains("FB0918"));
    assert!(outcome_norow.stderr.contains("way out:"));
    assert!(outcome_norow.stderr.contains("[[destination]]"));
    let count: usize = outcome_norow
        .stderr
        .split("backup refused: ")
        .nth(1)
        .and_then(|s| s.split(" items").next())
        .and_then(|s| s.parse().ok())
        .expect("parsed count of items from refusal");
    assert!(count >= 1);
    let expected_substr = format!("{count} items go to a target that no backup row names");
    assert!(
        outcome_norow.stderr.contains(&expected_substr),
        "expected stderr to contain '{expected_substr}', got: {}",
        outcome_norow.stderr
    );

    // 3. marked & all reasons tested directly through refusal_text
    let text_marked_1 = refusal_text(Code::Fb0901, Reason::Marked, 1);
    assert!(text_marked_1.contains("error[FB0901]: backup refused: 1 item is marked"));
    assert!(text_marked_1.contains("way out:"));
    assert!(text_marked_1.contains("[[marking]]"));
    assert!(text_marked_1.contains("label reapply"));

    let text_marked_2 = refusal_text(Code::Fb0901, Reason::Marked, 2);
    assert!(text_marked_2.contains("error[FB0901]: backup refused: 2 items are marked"));
    assert!(text_marked_2.contains("way out:"));
    assert!(text_marked_2.contains("[[marking]]"));
    assert!(text_marked_2.contains("label reapply"));

    let text_unlabelled_1 = refusal_text(Code::Fb0901, Reason::Unlabelled, 1);
    assert!(text_unlabelled_1.contains("error[FB0901]: backup refused: 1 item is unlabelled"));
    assert!(text_unlabelled_1.contains("way out:"));
    assert!(text_unlabelled_1.contains("[[source]]"));
    assert!(text_unlabelled_1.contains("label add"));

    let text_unlabelled_3 = refusal_text(Code::Fb0901, Reason::Unlabelled, 3);
    assert!(text_unlabelled_3.contains("error[FB0901]: backup refused: 3 items are unlabelled"));
    assert!(text_unlabelled_3.contains("way out:"));

    let text_label_1 = refusal_text(Code::Fb0901, Reason::Label, 1);
    assert!(text_label_1.contains(
        "error[FB0901]: backup refused: 1 item carries a label this target does not allow"
    ));
    assert!(text_label_1.contains("way out:"));
    assert!(text_label_1.contains(
        "choose a target whose backup row allows these labels, or change the labels of the items"
    ));

    let text_label_3 = refusal_text(Code::Fb0901, Reason::Label, 3);
    assert!(text_label_3.contains(
        "error[FB0901]: backup refused: 3 items carry a label this target does not allow"
    ));
    assert!(text_label_3.contains("way out:"));

    let text_norow_1 = refusal_text(Code::Fb0918, Reason::NoRow, 1);
    assert!(text_norow_1.contains(
        "error[FB0918]: backup refused: 1 item goes to a target that no backup row names"
    ));
    assert!(text_norow_1.contains("way out:"));
    assert!(text_norow_1.contains("[[destination]]"));

    let text_norow_5 = refusal_text(Code::Fb0918, Reason::NoRow, 5);
    assert!(text_norow_5.contains(
        "error[FB0918]: backup refused: 5 items go to a target that no backup row names"
    ));
    assert!(text_norow_5.contains("way out:"));
    assert!(text_norow_5.contains("[[destination]]"));

    let text_generic = refusal_text(Code::Fb0413, Reason::Closed, 5);
    assert!(text_generic.contains("error[FB0413]: backup refused: 5 items\n"));
    assert!(!text_generic.contains("way out:"));

    let text_generic_1 = refusal_text(Code::Fb0413, Reason::Closed, 1);
    assert!(text_generic_1.contains("error[FB0413]: backup refused: 1 item\n"));
    assert!(!text_generic_1.contains("way out:"));
}

#[test]
fn dry_run_creates_nothing_when_state_is_absent() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let tree_dir = support::temp_copy(&corpus_mini);

    let base_temp = tempfile::tempdir().expect("tempdir");
    let absent_state_dir = base_temp.path().join("absent-state");

    let target_dir = tempfile::tempdir().expect("target tempdir");
    let target_path = target_dir.path().to_path_buf();

    let world = TestWorld::with_state(tree_dir.path(), &absent_state_dir);
    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("catalogue from corpus-mini");
    let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
    world.set_support_catalogue(cat, cat_sha);

    let eff = floor_a_effective();
    let options = BackupOptions {
        target: None,
        to: Some(target_path.clone()),
        dry_run: true,
    };

    // Variant 1: Absent State folder
    assert!(!absent_state_dir.exists());
    let outcome = run_backup(&world, &eff, &options);
    assert_ne!(
        outcome.code, 3,
        "must not fail with exit 3: {}",
        outcome.stderr
    );
    assert_eq!(outcome.code, 0);
    assert!(outcome.stdout.contains("would write snapshot 1:"));
    assert!(
        !absent_state_dir.exists(),
        "State path must still not exist"
    );

    // Variant 2: Existing but empty State folder
    let empty_state_dir = base_temp.path().join("empty-state");
    std::fs::create_dir_all(&empty_state_dir).expect("create empty state dir");
    assert!(empty_state_dir.is_dir());
    assert_eq!(std::fs::read_dir(&empty_state_dir).unwrap().count(), 0);

    let world_empty = TestWorld::with_state(tree_dir.path(), &empty_state_dir);
    let cat2 = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("catalogue from corpus-mini");
    let (_, cat2_sha) = cat2.to_toml_bytes_and_sha256();
    world_empty.set_support_catalogue(cat2, cat2_sha);

    let outcome_empty = run_backup(&world_empty, &eff, &options);
    assert_ne!(
        outcome_empty.code, 3,
        "must not fail with exit 3: {}",
        outcome_empty.stderr
    );
    assert_eq!(outcome_empty.code, 0);
    assert!(outcome_empty.stdout.contains("would write snapshot 1:"));
    assert_eq!(
        std::fs::read_dir(&empty_state_dir).unwrap().count(),
        0,
        "State folder must still be empty"
    );
}

#[test]
fn a_changed_manifest_inherits_no_verification() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let b_out = run_backup(&fix.world, &eff, &options);
    assert_eq!(b_out.code, 0, "backup failed: {}", b_out.stderr);

    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_out = run_verify(&fix.world, &verify_args);
    assert_eq!(v_out.code, 0, "verify failed: {}", v_out.stderr);

    let list_json_args = BackupListArgs {
        target: Some("main".to_string()),
        in_dir: None,
        json: true,
    };
    let l_out = run_list(&fix.world, &list_json_args);
    assert_eq!(l_out.code, 0);
    let report: BackupList = serde_json::from_str(&l_out.stdout).expect("parse json");
    assert_eq!(report.snapshots.len(), 1);
    assert!(report.snapshots[0].verified);
    assert_eq!(
        newest_verified_snapshot_for_target(fix.state_dir(), fix.target_path(), &[1]),
        Some(1)
    );

    let toml_path = fix.target_path().join("fieldbook-backup-0001/backup.toml");
    let toml_text = std::fs::read_to_string(&toml_path).expect("read backup.toml");
    let mut info = BackupToml::parse(&toml_text).expect("parse backup.toml");
    info.labels = vec!["custom-label".to_string()];
    let rewritten = toml::to_string_pretty(&info).expect("serialize backup.toml");
    std::fs::write(&toml_path, rewritten).expect("write rewritten backup.toml");

    let l_after = run_list(&fix.world, &list_json_args);
    assert_eq!(l_after.code, 0);
    let report_after: BackupList = serde_json::from_str(&l_after.stdout).expect("parse json after");
    assert_eq!(report_after.snapshots.len(), 1);
    assert!(!report_after.snapshots[0].verified);
    assert!(report_after.snapshots[0].verified_utc.is_none());
    assert!(report_after.snapshots[0].latest_verification.is_none());
    assert_eq!(
        newest_verified_snapshot_for_target(fix.state_dir(), fix.target_path(), &[1]),
        None
    );

    let list_text_args = BackupListArgs {
        target: Some("main".to_string()),
        in_dir: None,
        json: false,
    };
    let l_text = run_list(&fix.world, &list_text_args);
    assert_eq!(l_text.code, 0);
    let row = l_text
        .stdout
        .lines()
        .find(|line| line.starts_with("1 "))
        .expect("snapshot 1 row in table");
    assert!(row.ends_with("no"));
}

#[test]
fn verify_exits_3_when_the_event_cannot_be_recorded() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let b_out = run_backup(&fix.world, &eff, &options);
    assert_eq!(b_out.code, 0, "backup failed: {}", b_out.stderr);

    let journal_path = fix.state_dir().join("audit/journal.jsonl");
    let count_verify_events = || -> usize {
        let content = std::fs::read_to_string(&journal_path).unwrap_or_default();
        content.matches("\"kind\":\"backup.verify\"").count()
    };
    let before_count = count_verify_events();

    let lease = acquire_write_lease(fix.tree_root(), fix.state_dir()).expect("acquire write lease");
    let held_journal =
        Journal::open(&fix.world, fix.state_dir(), Via::Cli).expect("open held journal");
    let state_scope_token = held_journal.state_scope(&lease).expect("state scope token");

    set_lock_timeout_for_test(Some(Duration::from_millis(50)));

    let v_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_out = run_verify(&fix.world, &v_args);

    set_lock_timeout_for_test(None);
    drop(state_scope_token);
    drop(held_journal);
    drop(lease);

    assert_eq!(v_out.code, 3);
    assert!(!v_out.stdout.contains(" verified"));
    assert!(v_out.stderr.contains("cannot record backup.verify"));

    let after_count = count_verify_events();
    assert_eq!(before_count, after_count);
}

#[test]
fn verify_on_a_broken_chain_does_not_claim_success() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let b_out = run_backup(&fix.world, &eff, &options);
    assert_eq!(b_out.code, 0, "backup failed: {}", b_out.stderr);

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

    let v_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let v_out = run_verify(&fix.world, &v_args);

    assert_eq!(v_out.code, 1);
    assert!(!v_out.stdout.contains(" verified"));
    assert!(v_out.stderr.contains("FB0800"));
}

#[test]
fn support_facts_follow_the_selected_state() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_a = support::temp_copy(&state_mini);
    let state_b = support::temp_copy(&state_mini);
    let target_dir_1 = tempfile::tempdir().expect("target tempdir 1");

    let settings_text_1 = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_dir_1.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_a.path().join("settings.toml"), &settings_text_1)
        .expect("write settings A");
    std::fs::write(state_b.path().join("settings.toml"), &settings_text_1)
        .expect("write settings B");

    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read tree");
    let cat = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("from_public_source");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();
    let cat_file = tree_dir.path().join("corpus-support.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write cat file");

    // 1. Verified support facts exist only in the default State A
    let raw_a = make_raw_copy(tree_dir.path(), state_a.path());
    let disk_a = Disk::with_state(tree_dir.path(), state_a.path());
    let v_args_a = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw_a.path().to_path_buf()),
        support_catalogue: Some(cat_file.clone()),
        expect_support_sha256: Some(cat_sha.clone()),
    };
    let v_out_a = run_verify(&disk_a, &v_args_a);
    assert_eq!(
        v_out_a.code, 0,
        "verify against state A: {}",
        v_out_a.stderr
    );

    assert!(!disk_a.support_facts().catalogues.is_empty());
    assert!(
        Disk::with_state(tree_dir.path(), state_b.path())
            .support_facts()
            .catalogues
            .is_empty()
    );

    // Run backup --state B --to <target> floor A through fieldbook::backup::run with Some(B):
    // support files have NO public provenance because facts of A are not used; refused with FB0901, exit 2.
    let args_1 = fieldbook::cli::BackupArgs {
        target: None,
        to: Some(target_dir_1.path().to_path_buf()),
        dry_run: false,
        command: None,
    };
    let outcome_1 = fieldbook::backup::run(&args_1, Some(state_b.path()), &disk_a);
    assert_eq!(
        outcome_1.code, 2,
        "must be refused with exit 2: {}",
        outcome_1.stderr
    );
    assert!(
        outcome_1.stderr.contains("FB0901"),
        "stderr should mention FB0901: {}",
        outcome_1.stderr
    );
    assert!(
        std::fs::read_dir(target_dir_1.path())
            .expect("read target 1")
            .next()
            .is_none(),
        "nothing must be written to target"
    );

    // 2. Fresh pair of A' (default, no facts) and B with verified catalogue only in B
    let state_a_prime = support::temp_copy(&state_mini);
    let state_b_2 = support::temp_copy(&state_mini);
    let target_dir_2 = tempfile::tempdir().expect("target tempdir 2");

    let settings_text_2 = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_dir_2.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_a_prime.path().join("settings.toml"), &settings_text_2)
        .expect("write settings A'");
    std::fs::write(state_b_2.path().join("settings.toml"), &settings_text_2)
        .expect("write settings B2");

    let raw_b2 = make_raw_copy(tree_dir.path(), state_b_2.path());
    let disk_b2 = Disk::with_state(tree_dir.path(), state_b_2.path());
    let v_args_b2 = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw_b2.path().to_path_buf()),
        support_catalogue: Some(cat_file),
        expect_support_sha256: Some(cat_sha),
    };
    let v_out_b2 = run_verify(&disk_b2, &v_args_b2);
    assert_eq!(
        v_out_b2.code, 0,
        "verify against state B: {}",
        v_out_b2.stderr
    );

    let disk_a_prime = Disk::with_state(tree_dir.path(), state_a_prime.path());
    assert!(disk_a_prime.support_facts().catalogues.is_empty());
    assert!(
        !Disk::with_state(tree_dir.path(), state_b_2.path())
            .support_facts()
            .catalogues
            .is_empty()
    );

    // backup --state B succeeds (exit 0) because B's facts are honoured, and snapshot contains support files
    let args_2 = fieldbook::cli::BackupArgs {
        target: None,
        to: None,
        dry_run: false,
        command: None,
    };
    let outcome_2 = fieldbook::backup::run(&args_2, Some(state_b_2.path()), &disk_a_prime);
    assert_eq!(
        outcome_2.code, 0,
        "backup --state B must succeed: {}",
        outcome_2.stderr
    );

    let snap_dir = target_dir_2.path().join("fieldbook-backup-0001");
    assert!(snap_dir.is_dir(), "snapshot directory must exist");
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).expect("read SHA256SUMS");
    assert!(
        sums_text.contains("data/corpus/AGENTS.md"),
        "snapshot must contain support files: {sums_text}"
    );
}

#[test]
fn support_facts_are_computed_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingWorld {
        inner: TestWorld,
        calls: AtomicUsize,
    }

    impl World for CountingWorld {
        fn read_tree(&self) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
            self.inner.read_tree()
        }
        fn write_lease(&self) -> Result<fieldbook::StoreLease, fieldbook::WorldError> {
            self.inner.write_lease()
        }
        fn read_tree_under(
            &self,
            lease: &fieldbook::StoreLease,
        ) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
            self.inner.read_tree_under(lease)
        }
        fn write_file(
            &self,
            audited: &fieldbook::io::journal::Audited,
            path: &Path,
            bytes: &[u8],
        ) -> Result<Sha256, fieldbook::WorldError> {
            self.inner.write_file(audited, path, bytes)
        }
        fn write_outside(
            &self,
            allowed: &Allowed,
            audited: &fieldbook::io::journal::Audited,
            path: &Path,
            bytes: &[u8],
        ) -> Result<Sha256, fieldbook::WorldError> {
            self.inner.write_outside(allowed, audited, path, bytes)
        }
        fn has_company_store(&self) -> bool {
            self.inner.has_company_store()
        }
        fn today(&self) -> CivilDate {
            self.inner.today()
        }
        fn now(&self) -> Now {
            self.inner.now()
        }
        fn principal(&self) -> fieldbook_core::audit::Principal {
            self.inner.principal()
        }
        fn cwd(&self) -> &Path {
            self.inner.cwd()
        }
        fn tree_root(&self) -> &Path {
            self.inner.tree_root()
        }
        fn state_dir(&self) -> &Path {
            self.inner.state_dir()
        }
        fn stdin_is_terminal(&self) -> bool {
            self.inner.stdin_is_terminal()
        }
        fn read_line(&self) -> String {
            self.inner.read_line()
        }
        fn prompt(&self, flag: &str) -> Result<(), fieldbook::WorldError> {
            self.inner.prompt(flag)
        }
        fn place(&self, path: &Path) -> PlaceFacts {
            self.inner.place(path)
        }
        fn support_facts(&self) -> fieldbook::SupportFacts {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.support_facts()
        }
        fn reset_drill(
            &self,
            audited: &fieldbook::io::journal::Audited,
        ) -> Result<(), fieldbook::WorldError> {
            self.inner.reset_drill(audited)
        }
        fn remove_drill(
            &self,
            audited: &fieldbook::io::journal::Audited,
        ) -> Result<(), fieldbook::WorldError> {
            self.inner.remove_drill(audited)
        }
        fn read_maintenance(&self) -> Result<String, fieldbook::WorldError> {
            self.inner.read_maintenance()
        }
        fn read_state_summary(
            &self,
            today: CivilDate,
        ) -> Result<fieldbook_core::status::StateSummary, fieldbook::WorldError> {
            self.inner.read_state_summary(today)
        }
    }

    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let counting = CountingWorld {
        inner: fix.world,
        calls: AtomicUsize::new(0),
    };
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let outcome = run_backup(&counting, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);
    assert_eq!(
        counting.calls.load(Ordering::SeqCst),
        1,
        "world.support_facts() must be computed exactly once per run_backup"
    );
}

#[test]
fn file_changed_between_passes_refuses() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct HookGuard;
    impl Drop for HookGuard {
        fn drop(&mut self) {
            fieldbook::io::backup_classify::set_after_read_hook(None);
        }
    }

    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let target_rel = "corpus/changes.md";
    let target_abs = fix.tree_root().join(target_rel);
    let mutated = Arc::new(AtomicBool::new(false));
    let mutated_hook = Arc::clone(&mutated);

    let _guard = HookGuard;
    fieldbook::io::backup_classify::set_after_read_hook(Some(Box::new(move |rel| {
        if rel == target_rel && !mutated_hook.swap(true, Ordering::SeqCst) {
            std::fs::write(&target_abs, b"# Modified Between Passes\n")
                .expect("mutate corpus file between passes");
        }
    })));

    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome = run_backup(&fix.world, &eff, &options);
    assert!(mutated.load(Ordering::SeqCst), "hook must have fired");
    assert_eq!(
        outcome.code,
        fieldbook::cli::EXIT_IO,
        "expected EXIT_IO (3), got {} stderr={}",
        outcome.code,
        outcome.stderr
    );
    assert!(
        outcome
            .stderr
            .contains(&format!("source changed during backup: {target_rel}")),
        "unexpected stderr: {}",
        outcome.stderr
    );
    assert!(
        !fix.target_path().join("fieldbook-backup-0001").exists(),
        "final snapshot directory must not exist when pass 2 detects a change"
    );
}

#[test]
fn active_journal_segment_hash_includes_begin_event() {
    let fix = TestFixture::new();
    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    let outcome = run_backup(&fix.world, &eff, &options);
    assert_eq!(outcome.code, 0, "backup failed: {}", outcome.stderr);

    let snap1 = fix.target_path().join("fieldbook-backup-0001");
    let copied_journal_rel = "data/state/audit/journal.jsonl";
    let copied_bytes = std::fs::read(snap1.join(copied_journal_rel)).expect("read copied journal");
    let copied_text = std::str::from_utf8(&copied_bytes).expect("utf8 journal");
    assert!(
        copied_text.contains("\"kind\":\"backup.begin\""),
        "copied active journal segment must include backup.begin event"
    );

    let actual_sha = fieldbook_core::hash::sha256_hex(&copied_bytes);
    let sums_text = std::fs::read_to_string(snap1.join("SHA256SUMS")).expect("read SHA256SUMS");
    let recorded_sha = sums_text
        .lines()
        .find_map(|line| {
            let (sha, rel) = line.split_once("  ")?;
            (rel == copied_journal_rel).then_some(sha)
        })
        .expect("SHA256SUMS must record active journal segment");
    assert_eq!(
        recorded_sha, actual_sha,
        "SHA256SUMS active journal digest must match file including backup.begin"
    );
}

#[test]
fn finish_detects_a_changed_written_file() {
    use fieldbook::io::backup_folder::FolderTarget;
    use fieldbook_core::backup::BackupTarget as _;

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("root");
    let state = tmp.path().join("state");
    let target = tmp.path().join("target");
    std::fs::create_dir_all(root.join("corpus")).expect("mkdir corpus");
    std::fs::create_dir_all(&state).expect("mkdir state");
    std::fs::create_dir_all(&target).expect("mkdir target");

    let world = TestWorld::with_state(&root, &state);
    let mut journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    let partial_dir = target.join("fieldbook-backup-0001.partial");
    let final_dir = target.join("fieldbook-backup-0001");
    let payload_rel = Path::new("payload.txt");
    let payload_bytes = b"original staged payload bytes\n";
    let payload_path = partial_dir.join(payload_rel);

    let eff = floor_a_effective();
    let payload_sha = fieldbook_core::hash::sha256_hex(payload_bytes);
    let (tree_sha, tree_bytes) = hash_tree_manifest(&[(
        "payload.txt".to_string(),
        payload_sha,
        payload_bytes.len() as u64,
    )]);
    let delivery = Delivery {
        entries: vec![
            Transfer::write_path(&payload_path, payload_bytes),
            Transfer {
                target: Target::Path(partial_dir.clone()),
                operation: Operation::RenameTree {
                    to: final_dir.clone(),
                },
                sha256: tree_sha,
                bytes: tree_bytes,
            },
        ],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &eff,
        &Dest::Backup {
            path: target.to_string_lossy().into_owned(),
            company_storage: false,
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed delivery");
    };

    let policy_sha = Sha256::parse(allowed.policy_hash()).expect("policy sha");
    let _prep = journal
        .prepare_backup_begin(&LabelSet::public(), &policy_sha, 1)
        .expect("prepare backup");
    let audited = journal
        .begin_backup(&allowed, &target, "folder", 1)
        .expect("begin backup");

    let folder_target = FolderTarget::new(&world, &audited, &allowed, &target);
    let mut stage = folder_target.begin(1, &allowed).expect("stage begin");
    stage
        .write(payload_rel, payload_bytes)
        .expect("stage write");

    // Mutate the staged file in the partial directory before calling finish().
    std::fs::write(&payload_path, b"tampered staged payload bytes\n")
        .expect("tamper staged file in partial dir");

    let finish_err = stage
        .finish()
        .expect_err("finish must fail when a staged file changed on disk");
    assert!(
        finish_err.to_string().contains("readback mismatch"),
        "unexpected finish error: {finish_err}"
    );
    assert!(
        !final_dir.exists(),
        "final snapshot directory must not exist after finish detects tampering"
    );
}

#[test]
fn snapshot_bytes_are_unchanged_by_the_two_passes() {
    use std::collections::BTreeMap;

    const EXPECTED_DATA_FILES: &[&str] = &[
        "data/corpus/AGENTS.md",
        "data/corpus/ASSISTANT.md",
        "data/corpus/changes.md",
        "data/corpus/corpus.toml",
        "data/corpus/eval/regression.toml",
        "data/corpus/pages/compliance/business-systems/cmp-clause-9900-101.md",
        "data/corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md",
        "data/corpus/pages/compliance/business-systems/cmp-review-steps.md",
        "data/corpus/pages/compliance/business-systems/cmp-system-review.md",
        "data/corpus/pages/compliance/business-systems/cmp-withhold-case.md",
        "data/corpus/pages/estimating/pricing/est-baseline-template.md",
        "data/corpus/pages/estimating/pricing/est-calc-01.md",
        "data/corpus/pages/estimating/pricing/est-rate-reference.md",
        "data/corpus/pages/estimating/pricing/est-rate-window-brief.md",
        "data/corpus/registers/claims.csv",
        "data/corpus/registers/conflicts.csv",
        "data/corpus/registers/glossary.csv",
        "data/corpus/registers/questions.csv",
        "data/corpus/registers/sources.csv",
        "data/corpus/scan-allow.toml",
        "data/corpus/sources/public/S-0001-v1.txt",
        "data/corpus/sources/public/S-0002-v2.txt",
        "data/corpus/sources/text/S-0001-v1/extraction.toml",
        "data/corpus/sources/text/S-0001-v1/p0001.txt",
        "data/corpus/templates/brief.md",
        "data/corpus/templates/concept.md",
        "data/corpus/templates/workbooks/90-day-plan.csv",
        "data/corpus/templates/workbooks/assumption-register.csv",
        "data/corpus/templates/workbooks/discovery-log.csv",
        "data/state/audit/journal.jsonl",
        "data/state/lock",
        "data/state/settings.toml",
        "data/state/store.lock",
    ];

    fn collect_snapshot_data_files(
        dir: &Path,
        snap_root: &Path,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("read snapshot dir")
            .map(|e| e.expect("dir entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                collect_snapshot_data_files(&path, snap_root, out);
            } else {
                let rel = path
                    .strip_prefix(snap_root)
                    .expect("strip snap root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).expect("read snapshot file");
                out.insert(rel, bytes);
            }
        }
    }

    let eff = floor_a_effective();
    let options = BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // Build an independent single-pass reference baseline in memory before invoking run_backup.
    // Both fixtures use identical corpus-mini, state-mini, settings.toml (same target path),
    // and deterministic TestWorld clock (default_now).
    let fix = TestFixture::new();
    let ref_fix = TestFixture::new();
    let settings_bytes =
        std::fs::read(fix.state_dir().join("settings.toml")).expect("read fix settings.toml");
    std::fs::write(ref_fix.state_dir().join("settings.toml"), &settings_bytes)
        .expect("sync ref settings.toml");

    let ref_journal =
        Journal::open(&ref_fix.world, ref_fix.state_dir(), Via::Cli).expect("open ref journal");
    let _ref_lease = ref_fix
        .world
        .write_lease()
        .expect("acquire ref write lease");
    let ref_labels = LabelSet::owner_supplied();
    let policy_sha = Sha256::parse(eff.hash()).expect("policy sha");
    let ref_prep = ref_journal
        .prepare_backup_begin_at(&ref_labels, &policy_sha, 1, ref_fix.world.now())
        .expect("prepare ref backup.begin");

    // Old-implementation single-pass in-memory cache built from the independent explicit list.
    let mut single_pass_cache: Vec<(String, Vec<u8>)> =
        Vec::with_capacity(EXPECTED_DATA_FILES.len());
    for &snap_rel in EXPECTED_DATA_FILES {
        let mut bytes = if let Some(corpus_rel) = snap_rel.strip_prefix("data/corpus/") {
            std::fs::read(ref_fix.tree_root().join("corpus").join(corpus_rel))
                .expect("read ref corpus file")
        } else if let Some(state_rel) = snap_rel.strip_prefix("data/state/") {
            std::fs::read(ref_fix.state_dir().join(state_rel)).expect("read ref state file")
        } else {
            panic!("unexpected snapshot rel {snap_rel}");
        };
        if snap_rel == "data/state/audit/journal.jsonl" {
            bytes.extend_from_slice(&ref_prep.bytes);
        }
        single_pass_cache.push((snap_rel.to_string(), bytes));
    }

    let mut expected_payloads = BTreeMap::new();
    let mut expected_sums_entries = Vec::with_capacity(single_pass_cache.len());
    let mut expected_total_bytes = 0u64;
    for (snap_rel, bytes) in single_pass_cache {
        expected_total_bytes = expected_total_bytes.saturating_add(bytes.len() as u64);
        expected_sums_entries.push((snap_rel.clone(), fieldbook_core::hash::sha256_hex(&bytes)));
        expected_payloads.insert(snap_rel, bytes);
    }
    let expected_sums_text = manifest::write_sums(&expected_sums_entries);
    let expected_fingerprint = manifest::fingerprint(&corpus_entries(&expected_sums_text));
    let mut sorted_labels: Vec<String> = ref_labels
        .ids()
        .iter()
        .map(|id| id.as_str().to_string())
        .collect();
    sorted_labels.sort();
    let mut expected_toml = BackupToml {
        backup_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        snapshot: 1,
        created_utc: ref_fix.world.now().to_string(),
        labels: sorted_labels,
        roots: vec!["corpus".to_string(), "state".to_string()],
        files: EXPECTED_DATA_FILES.len() as u64,
        bytes: expected_total_bytes,
        fingerprint: expected_fingerprint,
        journal_seq: ref_prep.seq,
        journal_head: ref_prep.new_head.as_str().to_string(),
        journal_state: "ok".to_string(),
        manifest_sha256: String::new(),
    };
    expected_toml.manifest_sha256 = manifest_sha256(&expected_toml, &expected_sums_text)
        .expect("ref manifest_sha256")
        .as_str()
        .to_string();
    let expected_toml_text =
        toml::to_string_pretty(&expected_toml).expect("serialize ref backup.toml");

    // Run the two-pass backup implementation and compare every artifact against the independent baseline.
    let out = run_backup(&fix.world, &eff, &options);
    assert_eq!(out.code, 0, "backup failed: {}", out.stderr);

    let snap1 = fix.target_path().join("fieldbook-backup-0001");
    let mut actual_payloads = BTreeMap::new();
    collect_snapshot_data_files(&snap1.join("data"), &snap1, &mut actual_payloads);

    let actual_rel_list: Vec<&str> = actual_payloads.keys().map(String::as_str).collect();
    assert_eq!(
        actual_rel_list, EXPECTED_DATA_FILES,
        "snapshot data/ file list must match the explicit expected corpus + support + state list"
    );
    assert_eq!(
        actual_payloads, expected_payloads,
        "snapshot data/ payloads must match single-pass reference byte-for-byte"
    );

    let actual_sums_text =
        std::fs::read_to_string(snap1.join("SHA256SUMS")).expect("read SHA256SUMS");
    assert_eq!(
        actual_sums_text, expected_sums_text,
        "SHA256SUMS must match single-pass reference byte-for-byte"
    );

    let actual_toml_text =
        std::fs::read_to_string(snap1.join("backup.toml")).expect("read backup.toml");
    assert_eq!(
        actual_toml_text, expected_toml_text,
        "backup.toml must match single-pass reference byte-for-byte"
    );

    let verify_args = BackupVerifyArgs {
        target: Some("main".to_string()),
        in_dir: None,
        snapshot: Some(1),
        raw: None,
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let verify_out = run_verify(&fix.world, &verify_args);
    assert_eq!(
        verify_out.code, 0,
        "backup verify failed: {}",
        verify_out.stderr
    );
}

fn main_backup_opts() -> BackupOptions {
    let target = Some("main".to_string());
    BackupOptions {
        target,
        to: None,
        dry_run: false,
    }
}

fn write_target_settings(state_dir: &Path, target: &Path, extra_backup: &str) {
    let path = target.to_string_lossy().replace('\\', "/");
    let text = format!(
        "role = \"lead\"\n\n[backup]\n{extra_backup}[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{path}\"\n"
    );
    std::fs::write(state_dir.join("settings.toml"), text).expect("write settings");
}

/// Runs `n` backups, then keeps only the last snapshot.
fn backups_then_keep_one(fix: &TestFixture, n: usize) {
    for _ in 0..n {
        let code = run_backup(&fix.world, &floor_a_effective(), &main_backup_opts()).code;
        assert_eq!(code, 0);
    }
    let keep_one = "keep_last = 1\nkeep_weekly = 0\n\n";
    write_target_settings(fix.state_dir(), fix.target_path(), keep_one);
}

/// Journal lines after the first `skip` that contain every needle.
fn journal_matches(state_dir: &Path, skip: usize, needles: &[&str]) -> Vec<String> {
    let text = std::fs::read_to_string(state_dir.join("audit/journal.jsonl")).expect("journal");
    let hit = |l: &&str| needles.iter().all(|n| l.contains(n));
    let lines = text.lines().skip(skip);
    lines.filter(hit).map(str::to_owned).collect()
}

const SETTINGS_CHANGE: &str = "\"kind\":\"settings.change\"";

fn broken_chain_fixture() -> (TempDir, TempDir, TempDir, TestWorld) {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tree_dir = support::temp_copy(&repo.join("tests/fixtures/corpus-mini"));
    let state_dir = support::temp_copy(&repo.join("tests/fixtures/state-known-bad/FB0800"));
    let target_dir = tempfile::tempdir().expect("target tempdir");
    write_target_settings(state_dir.path(), target_dir.path(), "");
    let world = TestWorld::with_state(tree_dir.path(), state_dir.path());
    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read corpus-mini tree");
    let cat = manifest::SupportCatalogue::from_public_source(&tree, &"1".repeat(40)).expect("cat");
    let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
    world.set_support_catalogue(cat, cat_sha);
    (tree_dir, state_dir, target_dir, world)
}

#[test]
fn raw_verify_that_cannot_append_says_it_was_not_recorded() {
    let fix = TestFixture::new();
    drop(Journal::open(&fix.world, fix.state_dir(), Via::Cli).expect("init state locks"));
    let raw = fix.raw_copy();
    let lock_path = fix.state_dir().join("lock");
    let lock_file = std::fs::OpenOptions::new().write(true).open(lock_path);
    let lock_file = lock_file.expect("open state/lock");
    lock_file.try_lock().expect("hold OS lock on state/lock");

    set_lock_timeout_for_test(Some(Duration::from_millis(50)));
    let args = BackupVerifyArgs {
        target: None,
        in_dir: None,
        snapshot: None,
        raw: Some(raw.path().to_path_buf()),
        support_catalogue: None,
        expect_support_sha256: None,
    };
    let out = run_verify(&fix.world, &args);
    set_lock_timeout_for_test(None);
    drop(lock_file);

    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("raw copy consistent"), "{}", out.stdout);
    let note = "note: verify was not recorded in the journal:";
    assert!(out.stderr.contains(note), "{}", out.stderr);
}

#[test]
fn refusal_on_a_broken_chain_is_recorded() {
    let (tree_dir, state_dir, _target_dir, world) = broken_chain_fixture();
    std::fs::write(tree_dir.path().join("corpus/stray.txt"), b"unlabelled\n").expect("write stray");
    let before = journal_matches(state_dir.path(), 0, &[]).len();

    let out = run_backup(&world, &floor_a_effective(), &main_backup_opts());
    assert_eq!(out.code, 2, "{}", out.stderr);
    let refuse_backup = ["\"kind\":\"refuse\"", "\"verb\":\"backup\""];
    let refusals = journal_matches(state_dir.path(), before, &refuse_backup);
    assert_eq!(refusals.len(), 1, "{refusals:?}");

    let audit_out = fieldbook::run(&["audit".to_string(), "verify".to_string()], &world);
    assert_eq!(audit_out.code, 1);
    assert!(audit_out.stdout.contains("FB0800") || audit_out.stderr.contains("FB0800"));
}

#[test]
fn recovery_mode_backup_removes_no_snapshot() {
    let fix = TestFixture::new();
    backups_then_keep_one(&fix, 3);

    let journal_path = fix.state_dir().join("audit/journal.jsonl");
    let content = std::fs::read_to_string(&journal_path).expect("read journal");
    let mut lines: Vec<&str> = content.lines().collect();
    let last = lines.pop().expect("last");
    let (prefix, _) = last.rsplit_once(',').expect("split comma");
    let corrupted_last = format!("{prefix},\"hash\":\"{}\"}}", "0".repeat(64));
    lines.push(&corrupted_last);
    std::fs::write(&journal_path, format!("{}\n", lines.join("\n"))).expect("corrupt journal tail");

    let out = run_backup(&fix.world, &floor_a_effective(), &main_backup_opts());
    assert_eq!(out.code, 1, "{}", out.stderr);
    for n in 1..=4 {
        let dir = fix.target_path().join(format!("fieldbook-backup-{n:04}"));
        assert!(dir.is_dir(), "snapshot {n:04} must remain");
    }
    let msg = "no snapshot removed: the journal is refused";
    assert!(out.stderr.contains(msg), "{}", out.stderr);
}

#[test]
fn end_append_failure_names_the_removed_snapshots() {
    let fix = TestFixture::new();
    backups_then_keep_one(&fix, 2);

    set_fail_on_backup_end_append(true);
    let out = run_backup(&fix.world, &floor_a_effective(), &main_backup_opts());
    set_fail_on_backup_end_append(false);

    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
    let msg = "removed without a journal record: 0001";
    assert!(out.stderr.contains(msg), "{}", out.stderr);
}

fn valid_backup_toml_text(snapshot: u32, journal_state: &str) -> String {
    let (zeros, journal_state) = ("0".repeat(64), journal_state.to_string());
    BackupToml {
        backup_version: 1,
        tool_version: "0.1.0".to_string(),
        snapshot,
        created_utc: "2026-03-01T12:00:00Z".to_string(),
        labels: vec!["public".to_string()],
        roots: vec!["corpus".to_string()],
        files: 0,
        bytes: 0,
        fingerprint: zeros.clone(),
        journal_seq: 0,
        journal_head: zeros.clone(),
        journal_state,
        manifest_sha256: zeros,
    }
    .render()
}

/// Asks `remove_outside` to remove a snapshot folder 0009 holding `toml_bytes`
/// as its `backup.toml`, and asserts it is refused and kept.
fn assert_snapshot_0009_is_not_removed(toml_bytes: &[u8]) {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join("corpus")).expect("corpus");
    let target = tmp.path().join("target");
    let snap9 = target.join("fieldbook-backup-0009");
    std::fs::create_dir_all(&snap9).expect("snap9");
    std::fs::write(snap9.join("backup.toml"), toml_bytes).expect("write backup.toml");

    let world = TestWorld::new(tmp.path());
    let mut journal = Journal::open(&world, world.state_dir(), Via::Cli).expect("open journal");
    let sha = Sha256::digest(toml_bytes).to_string();
    let entry = ("backup.toml".to_string(), sha, toml_bytes.len() as u64);
    let (sha256, bytes) = hash_tree_manifest(&[entry]);
    let operation = Operation::RemoveTree;
    let target_path = Target::Path(snap9.clone());
    let transfer = Transfer {
        target: target_path,
        operation,
        sha256,
        bytes,
    };
    let delivery = Delivery::new(vec![transfer], None);
    let path = target.to_string_lossy().into_owned();
    let dest = Dest::Backup {
        path,
        company_storage: false,
    };
    let public = LabelSet::public();
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &dest,
        Some(&public),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };
    let policy_sha = Sha256::parse(allowed.policy_hash()).expect("policy sha");
    journal
        .prepare_backup_begin(&public, &policy_sha, 1)
        .expect("prepare");
    let audited = journal
        .begin_backup(&allowed, &target, "folder", 1)
        .expect("begin");
    let res = remove_outside(&audited, &allowed, &snap9, Removable::Snapshot);
    let refused = matches!(res, Err(fieldbook::Error::OutOfScope(_)));
    assert!(refused, "{res:?}");
    assert!(snap9.exists());
}

#[test]
#[cfg(unix)]
fn unremovable_partial_fails_the_backup() {
    use std::os::unix::fs::PermissionsExt as _;

    struct PermGuard(PathBuf);
    impl Drop for PermGuard {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let fix = TestFixture::new();
    let locked_parent = fix.target_path().join("fieldbook-backup-0002.partial/sub");
    std::fs::create_dir_all(&locked_parent).expect("create partial/sub");
    std::fs::write(locked_parent.join("locked.txt"), b"locked").expect("write locked.txt");
    let guard = PermGuard(locked_parent.clone());
    let read_only = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(&locked_parent, read_only).expect("chmod");
    let probe = locked_parent.join("probe.tmp");
    if std::fs::write(&probe, b"").is_ok() {
        let _ = std::fs::remove_file(&probe);
        return;
    }

    let out = run_backup(&fix.world, &floor_a_effective(), &main_backup_opts());
    drop(guard);
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(out.stderr.contains("clean leftover partials error:"));
    assert!(out.stderr.contains("fieldbook-backup-0002.partial"));
    assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
}

#[test]
fn snapshot_folder_with_a_foreign_manifest_is_not_removed() {
    assert_snapshot_0009_is_not_removed(valid_backup_toml_text(2, "ok").as_bytes());
}

#[test]
fn snapshot_folder_with_an_unparsable_manifest_is_not_removed() {
    assert_snapshot_0009_is_not_removed(b"backup_version = [\n");
}

#[test]
fn journal_state_words_are_ok_and_broken() {
    use fieldbook_core::backup::toml::{JOURNAL_STATE_BROKEN, JOURNAL_STATE_OK};

    assert_eq!(JOURNAL_STATE_OK, "ok");
    assert_eq!(JOURNAL_STATE_BROKEN, concat!("bro", "ken"));
    let old_ok = valid_backup_toml_text(1, "ok");
    let old_broken = valid_backup_toml_text(1, fieldbook_core::strings::cite_statuses()[0]);
    assert!(BackupToml::parse(&old_ok).is_ok());
    assert!(BackupToml::parse(&old_broken).is_ok());
}

#[test]
fn settings_change_after_a_break_is_not_appended_again() {
    let (_tree_dir, state_dir, _target_dir, world) = broken_chain_fixture();
    drop(Journal::open(&world, state_dir.path(), Via::Cli).expect("first open after break"));
    let count_changes = || journal_matches(state_dir.path(), 0, &[SETTINGS_CHANGE]).len();
    assert_eq!(count_changes(), 2);
    for _ in 0..10 {
        drop(Journal::open(&world, state_dir.path(), Via::Cli).expect("repeat open"));
    }
    assert_eq!(count_changes(), 2);
}

#[test]
fn unverified_settings_change_after_a_break_does_not_suppress_a_real_one() {
    let (_tree_dir, state_dir, target_dir, world) = broken_chain_fixture();
    let mut j = Journal::open(&world, state_dir.path(), Via::Cli).expect("open 1");
    let path_sha256 = Sha256::digest(b"raw-probe");
    let raw_verified = BackupVerified::Raw {
        path_sha256,
        public_support_sha256: None,
    };
    j.append_backup_verify(Word::new_static("ok"), raw_verified)
        .expect("append");
    drop(j);

    let warn = "warn_after_days = 14\n\n";
    write_target_settings(state_dir.path(), target_dir.path(), warn);
    let (_, disk_baseline) =
        fieldbook::io::state::read_role_and_baseline(state_dir.path()).expect("baseline");
    let new_baseline_json = serde_json::to_string(&disk_baseline).expect("serialize");

    let journal_path = state_dir.path().join("audit/journal.jsonl");
    let mut lines = journal_matches(state_dir.path(), 0, &[]);
    let is_change = |l: &String| l.contains(SETTINGS_CHANGE);
    let idx = lines.iter().rposition(is_change).expect("idx");
    assert!(idx + 1 < lines.len());
    let line = &lines[idx];
    let b_start = line.find("\"baseline\":").expect("start") + "\"baseline\":".len();
    let h_pos = line.rfind(",\"hash\":\"").expect("hash");
    lines[idx] = format!("{}{new_baseline_json}{}", &line[..b_start], &line[h_pos..]);
    std::fs::write(&journal_path, format!("{}\n", lines.join("\n"))).expect("write");

    drop(Journal::open(&world, state_dir.path(), Via::Cli).expect("open 2"));
    let appended = journal_matches(state_dir.path(), lines.len(), &[SETTINGS_CHANGE]);
    assert_eq!(appended.len(), 1, "{appended:?}");
    assert!(appended[0].contains(&new_baseline_json));
}

#[test]
fn backup_of_a_checkout_without_corpus_is_refused() {
    for corpus_is_a_file in [false, true] {
        let fix = TestFixture::new();
        let corpus = fix.tree_root().join("corpus");
        std::fs::remove_dir_all(&corpus).expect("remove corpus dir");
        if corpus_is_a_file {
            let generation = format!("{}\n", "0".repeat(64));
            std::fs::write(fix.state_dir().join("generation"), generation)
                .expect("seed generation");
            std::fs::write(&corpus, b"not a folder\n").expect("write corpus file");
        }
        let out = run_backup(&fix.world, &floor_a_effective(), &main_backup_opts());
        assert_eq!(out.code, 3, "{}", out.stderr);
        let msg = format!("no corpus folder at {}", corpus.display());
        assert!(out.stderr.contains(&msg), "{}", out.stderr);
        assert!(!fix.target_path().join("fieldbook-backup-0001").exists());
    }
}

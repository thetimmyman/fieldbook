//! The backup verb: create, verify, or list backup snapshots (`AD-17`, `backup-snapshot`).
//!
//! Implements snapshot creation, verification preflight, egress policy gating,
//! and retention management according to card steps 4, 5, 7, 14, and 30.

#![expect(
    clippy::disallowed_methods,
    reason = "backup verify and list inspect snapshot directories and read manifests"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use fieldbook_core::CivilDate;
use fieldbook_core::audit::kind::BackupVerified;
use fieldbook_core::audit::{
    Count, DestKind, Event, EventPayload, LabelSet, Now, RefuseAbout, Sha256, Via, Word,
};
use fieldbook_core::backup::{
    BACKUP_TARGET_TEMPLATE, BackupFileKind, BackupTarget, BackupToml, CopiedFile, Number,
    SnapshotInfo, corpus_entries, manifest_sha256, path_budget, plan_backup, plan_retention,
    snapshot_labels, target_place,
};
use fieldbook_core::check::{Code, Host, Model, Options, load};
use fieldbook_core::egress::{
    Decision, Delivery, Dest, Operation, Reason, Shipped, Target, Transfer, check,
    hash_tree_manifest,
};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::{self, StoreId, git_guard, markings_in};
use fieldbook_core::manifest;
use fieldbook_core::policy::{EffectivePolicy, PolicyState, path_matches_prefix};
use fieldbook_core::report::{
    BackupLatestVerification, BackupList, BackupListDrill, BackupListFinding, BackupListSnapshot,
    BackupListTarget, LastBackupReport,
};

use crate::cli::{BackupArgs, BackupSubcommand, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE};
pub use crate::cli::{BackupListArgs, BackupVerifyArgs};
use crate::io::backup_classify::{classify_file, enumerate_roots, read_file_content};
use crate::io::backup_folder::{
    FolderTarget, list_existing_snapshots, list_partial_removals,
    newest_verified_snapshot_for_target, read_snapshot_infos, tree_remove_transfer,
};
use crate::io::journal::{
    Journal, JournalError, PreparedBackupBegin, is_journal_chain_clean, newest_restore_end_events,
    newest_snapshot_verify_events, newest_successful_snapshot_verify_any_events,
    newest_successful_snapshot_verify_events, read_valid_events_prefix,
};
use crate::io::root::git_context;
use crate::io::state;
pub(crate) use crate::io::state::{BackupSettings, BackupTargetConfig, read_backup_settings};
use crate::{Outcome, StateOverrideWorld, World};

/// Options for snapshot creation.
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Target identifier from `settings.toml`.
    pub target: Option<String>,
    /// One-off destination directory.
    pub to: Option<PathBuf>,
    /// Perform checks without writing or appending.
    pub dry_run: bool,
}

/// Dispatches `fieldbook backup` from parsed CLI arguments.
pub fn run(args: &BackupArgs, state_flag: Option<&Path>, world: &dyn World) -> Outcome {
    if let Some(state_dir) = state_flag {
        let resolved = state::resolve(world.tree_root(), world.cwd(), Some(state_dir));
        let overridden = StateOverrideWorld {
            inner: world,
            state_dir: resolved,
        };
        run_with_world(args, &overridden)
    } else {
        run_with_world(args, world)
    }
}

fn days_elapsed(from: CivilDate, to: CivilDate) -> u32 {
    u32::try_from(from.days_between(to).max(0)).unwrap_or(u32::MAX)
}

fn parse_last_backup_cache(json: &str) -> Option<LastBackupReport> {
    let report: LastBackupReport = serde_json::from_str(json).ok()?;
    if report.verification != "snapshot" || report.labels.is_empty() {
        return None;
    }
    Some(report)
}

/// Returns the age in whole days since the newest verified numbered snapshot, or `None`
/// if no verified snapshot exists or verification evidence fails validation (`backup-snapshot` step 10, 21).
#[must_use]
pub fn backup_age(state: &Path, today: CivilDate) -> Option<u32> {
    let report_path = state.join("reports/last-backup.json");
    let content = std::fs::read_to_string(&report_path).ok()?;
    let report = parse_last_backup_cache(&content)?;

    let verified_now = Now::parse(&report.verified_utc).ok()?;

    // 1. Recanonicalize target_path and check target_sha256
    let canonical = Path::new(&report.target_path).canonicalize().ok()?;
    let canonical_str = canonical.to_string_lossy();
    let target_sha256 = Sha256::digest(canonical_str.as_bytes());
    if target_sha256.as_str() != report.target_sha256 {
        return None;
    }

    // 2. Reopen selected snapshot directory and verify manifest_sha256
    let snap_dir = canonical.join(format!("fieldbook-backup-{:04}", report.snapshot));
    if !snap_dir.is_dir() {
        return None;
    }
    let toml_text = std::fs::read_to_string(snap_dir.join("backup.toml")).ok()?;
    let info = BackupToml::parse(&toml_text).ok()?;
    if info.snapshot != report.snapshot {
        return None;
    }
    let sums_text = std::fs::read_to_string(snap_dir.join("SHA256SUMS")).ok()?;
    let computed_manifest = manifest_sha256(&info, &sums_text).ok()?;
    if computed_manifest.as_str() != report.manifest_sha256 {
        return None;
    }

    // 3. Match the successful backup.verify event in the journal including verified_utc
    let events = read_valid_events_prefix(state);
    let matched_event = events.iter().any(|e| {
        if e.time.to_string() != report.verified_utc {
            return false;
        }
        matches!(
            &e.payload,
            EventPayload::BackupVerify {
                outcome,
                verified: BackupVerified::Snapshot {
                    target_sha256,
                    snapshot,
                    manifest_sha256,
                },
            } if outcome.as_str() == "ok"
                && target_sha256.as_str() == report.target_sha256
                && snapshot.value() == u64::from(report.snapshot)
                && manifest_sha256.as_str() == report.manifest_sha256
        )
    });

    if !matched_event {
        return None;
    }

    Some(days_elapsed(verified_now.date, today))
}

/// Computes the optional reminder message displayed on stderr after write verbs.
///
/// Returns `None` only when a verified backup exists and is not older than `warn_after_days`.
#[must_use]
pub fn reminder(state: &Path, today: CivilDate, warn_after_days: u32) -> Option<String> {
    match backup_age(state, today) {
        None => Some(format!(
            "fieldbook: no verified backup yet (warn after {warn_after_days} days). Run: fieldbook backup"
        )),
        Some(age) if age > warn_after_days => Some(format!(
            "fieldbook: last verified backup is {age} days old (warn after {warn_after_days}). Run: fieldbook backup"
        )),
        Some(_) => None,
    }
}

fn run_with_world(args: &BackupArgs, world: &dyn World) -> Outcome {
    if let Some(sub) = &args.command {
        match sub {
            BackupSubcommand::Verify(verify_args) => {
                return run_verify(world, verify_args);
            }
            BackupSubcommand::List(list_args) => {
                return run_list(world, list_args);
            }
        }
    }

    let options = BackupOptions {
        target: args.target.clone(),
        to: args.to.clone(),
        dry_run: args.dry_run,
    };

    let eff = EffectivePolicy::floor(world.has_company_store());
    run_backup(world, &eff, &options)
}

/// Formats the refusal error message and way-out guidance for egress refusals (`AD-17`, `TASK R6`).
#[must_use]
pub fn refusal_text(code: Code, reason: Reason, count: usize) -> String {
    let (detail, way_out) = match reason {
        Reason::Unlabelled => (
            if count == 1 {
                "1 item is unlabelled".to_string()
            } else {
                format!("{count} items are unlabelled")
            },
            Some(
                "give each item a real label: a [[source]] default in the policy sets one when an item is taken in, and `fieldbook label add` sets one by hand",
            ),
        ),
        Reason::Marked => (
            if count == 1 {
                "1 item is marked".to_string()
            } else {
                format!("{count} items are marked")
            },
            Some(
                "add a [[marking]] rule to the policy that maps the marking to a label, then run `fieldbook label reapply` (`label add` leaves the marking in place)",
            ),
        ),
        Reason::Label => (
            if count == 1 {
                "1 item carries a label this target does not allow".to_string()
            } else {
                format!("{count} items carry a label this target does not allow")
            },
            Some(
                "choose a target whose backup row allows these labels, or change the labels of the items",
            ),
        ),
        Reason::NoRow => (
            if count == 1 {
                "1 item goes to a target that no backup row names".to_string()
            } else {
                format!("{count} items go to a target that no backup row names")
            },
            Some("add a [[destination]] row with kind = \"backup\" whose paths name the target"),
        ),
        _ => (
            if count == 1 {
                "1 item".to_string()
            } else {
                format!("{count} items")
            },
            None,
        ),
    };

    if let Some(way) = way_out {
        format!("error[{code}]: backup refused: {detail}\n  way out: {way}\n")
    } else {
        format!("error[{code}]: backup refused: {detail}\n")
    }
}

/// Returns whether the State directory is fully initialized.
///
/// Mirrors the early return check of `init_state_locks` in `crate::io::journal`.
fn state_is_initialized(state_dir: &Path) -> bool {
    let audit_dir = state_dir.join("audit");
    let journal_lock = state_dir.join("lock");
    let store_lock = state_dir.join("store.lock");
    let init_marker = state_dir.join(".init");

    audit_dir.is_dir() && journal_lock.is_file() && store_lock.is_file() && !init_marker.exists()
}

/// Executes snapshot creation under the given effective policy.
pub fn run_backup(world: &dyn World, eff: &EffectivePolicy, options: &BackupOptions) -> Outcome {
    // State path cannot be a file
    if world.state_dir().is_file() {
        return Outcome {
            stdout: String::new(),
            stderr: format!("state path is a file: {}\n", world.state_dir().display()),
            code: EXIT_IO,
        };
    }

    // 1. Read BackupSettings from settings.toml
    let settings = match read_backup_settings(world.state_dir()) {
        Ok(s) => s,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("error[FB0812]: {err}\n"),
                code: EXIT_FINDINGS,
            };
        }
    };

    let events = read_valid_events_prefix(world.state_dir());
    let newest_succ = newest_successful_snapshot_verify_any_events(&events);
    let mut warn_stale = String::new();
    if let Some(t) = newest_succ {
        let age_days = days_elapsed(t.date, world.today());
        if age_days > settings.warn_after_days {
            warn_stale.push_str(&format!(
                "warning[FB0814]: last verified backup is {age_days} days old (warn after {})\n",
                settings.warn_after_days
            ));
        }
    }

    // Resolve target configuration
    let target_cfg = if let Some(to_dir) = &options.to {
        BackupTargetConfig {
            id: "one-off".to_owned(),
            kind: "folder".to_owned(),
            path: to_dir.clone(),
            include: None,
            company_storage: false,
        }
    } else if let Some(target_id) = &options.target {
        let Some(found) = settings.targets.iter().find(|t| &t.id == target_id) else {
            return Outcome {
                stdout: String::new(),
                stderr: format!("target `{target_id}` not found in settings.toml\n"),
                code: EXIT_USAGE,
            };
        };
        found.clone()
    } else if let Some(def_id) = &settings.default {
        let Some(found) = settings.targets.iter().find(|t| &t.id == def_id) else {
            return Outcome {
                stdout: String::new(),
                stderr: format!("default target `{def_id}` not found in settings.toml\n"),
                code: EXIT_USAGE,
            };
        };
        found.clone()
    } else if let Some(first) = settings.targets.first() {
        first.clone()
    } else {
        return Outcome {
            stdout: String::new(),
            stderr: format!(
                "no backup target; add this to settings.toml in the state folder:\n{}",
                BACKUP_TARGET_TEMPLATE
            ),
            code: EXIT_USAGE,
        };
    };

    let target_path = target_cfg.path.clone();
    let target_str = target_path.to_string_lossy().into_owned();

    // 2. Refuse a target inside the Corpus or state folder (FB0812)
    let corpus_root = world.tree_root().join("corpus");
    let state_root = world.state_dir().to_path_buf();
    if is_inside_or_equal(&target_path, &corpus_root)
        || is_inside_or_equal(&target_path, &state_root)
    {
        return Outcome {
            stdout: String::new(),
            stderr: format!(
                "error[FB0812]: backup target lies inside a folder being backed up: {}\n",
                target_path.display()
            ),
            code: EXIT_FINDINGS,
        };
    }

    // 3. Acquire exclusive store lease (skipped for dry runs when State is absent or uninitialized)
    let lease = if options.dry_run && !state_is_initialized(world.state_dir()) {
        None
    } else {
        match world.write_lease() {
            Ok(l) => Some(l),
            Err(err) => {
                if matches!(
                    std::fs::symlink_metadata(&corpus_root),
                    Ok(ref m) if m.file_type().is_file()
                ) {
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("error: no corpus folder at {}\n", corpus_root.display()),
                        code: EXIT_IO,
                    };
                }
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot acquire store lease: {err}\n"),
                    code: EXIT_IO,
                };
            }
        }
    };

    // 4. Open the Journal or Recovery mode (skip opening for dry-run to write/append nothing)
    let mut journal_opt = None;
    let mut recovery_opt = None;
    let mut is_broken_chain = false;

    if !options.dry_run {
        match state::open(world, world.state_dir(), Via::Cli) {
            Ok(j) => {
                if !j.is_clean() {
                    is_broken_chain = true;
                }
                journal_opt = Some(j);
            }
            Err(JournalError::BrokenChain(_)) => {
                is_broken_chain = true;
                let Some(lease_ref) = &lease else {
                    return Outcome {
                        stdout: String::new(),
                        stderr: "cannot open recovery mode without a store lease\n".to_string(),
                        code: EXIT_IO,
                    };
                };
                match Journal::open_backup_recovery(world, lease_ref) {
                    Ok(rec) => recovery_opt = Some(rec),
                    Err(err) => {
                        return Outcome {
                            stdout: String::new(),
                            stderr: format!("cannot open recovery mode: {err}\n"),
                            code: EXIT_IO,
                        };
                    }
                }
            }
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot open journal: {err}\n"),
                    code: EXIT_IO,
                };
            }
        }
    }

    let mut notices = String::new();

    // 5. Target place check
    let facts = world.place(&target_path);
    let declared = eff.state().policy().is_some_and(|p| {
        p.destinations().iter().any(|d| {
            d.kind == DestKind::Backup
                && d.paths.as_ref().is_some_and(|paths| {
                    paths
                        .iter()
                        .any(|pfx| path_matches_prefix(&target_str, pfx))
                })
        })
    });

    if let Some(finding) =
        target_place(world.has_company_store(), &target_cfg.kind, facts, declared)
    {
        match finding {
            fieldbook_core::backup::PlaceFinding::Notice => {
                notices
                    .push_str("notice: target is a synced path (`kind = \"synced\"` declared)\n");
            }
            fieldbook_core::backup::PlaceFinding::Warning(code) => {
                return Outcome {
                    stdout: notices,
                    stderr: format!(
                        "{}: warning[{code}]: target is a sync root; declare kind = \"synced\" in settings.toml to acknowledge\n",
                        target_path.display()
                    ),
                    code: EXIT_FINDINGS,
                };
            }
            fieldbook_core::backup::PlaceFinding::Refusal(refusal) => {
                if let Some(j) = &journal_opt {
                    let about = RefuseAbout {
                        dest: Some(DestKind::Backup),
                        ..RefuseAbout::default()
                    };
                    let _ = j.refuse_backup(refusal.code, Word::new_static("place"), &about);
                }
                return Outcome {
                    stdout: notices,
                    stderr: format!("error[{}]: target place refusal\n", refusal.code),
                    code: EXIT_FINDINGS,
                };
            }
        }
    }

    // 6. Enumerate files in roots (corpus and state), skipping derived folders
    let include_rules = target_cfg.include.as_deref().unwrap_or(&[]);
    let listing = match enumerate_roots(world) {
        Ok(l) => l,
        Err(err) => {
            return Outcome {
                stdout: notices,
                stderr: format!("error: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    // Determine next snapshot number
    let existing_snapshots = list_existing_snapshots(&target_path);
    let next_number = existing_snapshots.last().copied().unwrap_or(0) + 1;

    let mut plan = plan_backup(&listing, include_rules, next_number);

    // Check path budget
    let longest_rel = plan
        .files
        .iter()
        .max_by_key(|f| f.len())
        .map(String::as_str)
        .unwrap_or("");
    if let Err(err) = path_budget(&target_str, longest_rel) {
        return Outcome {
            stdout: notices,
            stderr: format!("error: {err}\n"),
            code: EXIT_FINDINGS,
        };
    }

    // 7. Load model and support provenance once for label resolution
    let (model, _) = load_model_if_available(world);
    let support_facts = world.support_facts();

    // Pass 1: classify each file and record only its digest and byte length
    let mut copied_files = Vec::with_capacity(plan.files.len());
    let mut recorded_files: Vec<(String, String, u64)> = Vec::with_capacity(plan.files.len());

    for file_rel in &plan.files {
        let bytes = match read_file_content(world, file_rel) {
            Ok(b) => b,
            Err(err) => {
                return Outcome {
                    stdout: notices,
                    stderr: format!("error: {err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let cf = classify_file(&support_facts, model.as_ref(), eff, file_rel, &bytes);
        copied_files.push(cf);
        recorded_files.push((file_rel.clone(), sha256_hex(&bytes), bytes.len() as u64));
    }

    // Calculate aggregated snapshot labels
    let labels = snapshot_labels(&copied_files);

    // 8. Git context check (step 30)
    let git_ctx = match git_context(world, &target_path) {
        Ok(c) => c,
        Err(err) => {
            return Outcome {
                stdout: notices,
                stderr: format!("cannot resolve git context: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    if let Err(refusal) = git_guard(&labels, git_ctx) {
        if let Some(j) = &journal_opt {
            let about = RefuseAbout {
                labels: Some(labels.clone()),
                dest: Some(DestKind::Backup),
                ..RefuseAbout::default()
            };
            let _ = j.refuse_backup(refusal.code, Word::new_static("git"), &about);
        }
        return Outcome {
            stdout: notices,
            stderr: format!(
                "error[{}]: backup target under git refuses non-public labels\n",
                refusal.code
            ),
            code: EXIT_FINDINGS,
        };
    }

    let now = world.now();
    let shipped = Shipped::Never;

    // 9. Prepare journal event and plan Delivery
    let mut prepared_begin: Option<PreparedBackupBegin> = None;
    let (journal_seq, journal_head) = if !options.dry_run
        && let Some(j) = &journal_opt
    {
        let policy_hash = Sha256::parse(eff.hash()).unwrap_or_else(|_| Sha256::digest(b""));
        let prep = match j.prepare_backup_begin_at(&labels, &policy_hash, next_number, now) {
            Ok(p) => p,
            Err(err) => {
                return Outcome {
                    stdout: notices,
                    stderr: format!("cannot prepare backup.begin: {err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let seq = prep.seq;
        let head = prep.new_head.clone();
        prepared_begin = Some(prep);
        (seq, head.as_str().to_string())
    } else {
        // Recovery mode or dry-run: use last chaining sequence and head from valid prefix
        let events = read_valid_events_prefix(world.state_dir());
        if let Some(last) = events.last() {
            (last.seq, last.hash.as_str().to_string())
        } else {
            (0, fieldbook_core::audit::ZERO_HASH.to_string())
        }
    };

    if let Some(prep) = &prepared_begin {
        let segment_rel = format!("state/audit/{}", prep.segment);
        if !plan_backup(&[&segment_rel], include_rules, next_number)
            .files
            .is_empty()
            && !plan.files.contains(&segment_rel)
        {
            if let Err(err) = path_budget(&target_str, &segment_rel) {
                return Outcome {
                    stdout: notices,
                    stderr: format!("error: {err}\n"),
                    code: EXIT_FINDINGS,
                };
            }
            if !plan.roots.iter().any(|r| r == "state") {
                plan.roots.push("state".to_string());
                plan.roots.sort();
            }
            let idx = match plan.files.binary_search(&segment_rel) {
                Ok(i) | Err(i) => i,
            };
            plan.files.insert(idx, segment_rel.clone());
            recorded_files.insert(
                idx,
                (
                    segment_rel.clone(),
                    sha256_hex(&prep.bytes),
                    prep.bytes.len() as u64,
                ),
            );
            copied_files.insert(
                idx,
                CopiedFile::new(
                    segment_rel,
                    BackupFileKind::TextFreeControl,
                    LabelSet::public(),
                ),
            );
        } else if let Some((_, expected_sha, expected_len)) = recorded_files
            .iter_mut()
            .find(|(rel, _, _)| rel == &segment_rel)
        {
            let mut seg_bytes = match read_file_content(world, &segment_rel) {
                Ok(b) => b,
                Err(err) => {
                    return Outcome {
                        stdout: notices,
                        stderr: format!("error: {err}\n"),
                        code: EXIT_IO,
                    };
                }
            };
            if seg_bytes.len() as u64 != *expected_len || sha256_hex(&seg_bytes) != *expected_sha {
                return Outcome {
                    stdout: notices,
                    stderr: format!("source changed during backup: {segment_rel}\n"),
                    code: EXIT_IO,
                };
            }
            seg_bytes.extend_from_slice(&prep.bytes);
            *expected_sha = sha256_hex(&seg_bytes);
            *expected_len = seg_bytes.len() as u64;
        }
    }

    let partial_dir_name = format!("fieldbook-backup-{next_number:04}.partial");
    let final_dir_name = format!("fieldbook-backup-{next_number:04}");
    let partial_path = target_path.join(&partial_dir_name);
    let final_path = target_path.join(&final_dir_name);

    // Build files delivery and SHA256SUMS from pass-1 digests without retaining file bytes
    let mut delivery_transfers = Vec::with_capacity(recorded_files.len());
    let mut sums_entries = Vec::with_capacity(recorded_files.len());
    let mut tree_entries = Vec::with_capacity(recorded_files.len());
    let mut total_bytes = 0u64;

    for (file_rel, sha, len) in &recorded_files {
        total_bytes = total_bytes.saturating_add(*len);

        let snap_rel = format!("data/{file_rel}");
        let full_out_path = partial_path.join(&snap_rel);

        delivery_transfers.push(Transfer {
            target: Target::Path(full_out_path),
            operation: Operation::Write,
            sha256: sha.clone(),
            bytes: *len,
        });
        sums_entries.push((snap_rel.clone(), sha.clone()));
        tree_entries.push((snap_rel, sha.clone(), *len));
    }

    // SHA256SUMS
    let sums_text = manifest::write_sums(&sums_entries);
    let sums_bytes = sums_text.as_bytes();
    let sums_sha = sha256_hex(sums_bytes);
    let sums_len = sums_bytes.len() as u64;

    delivery_transfers.push(Transfer::write_path(
        partial_path.join("SHA256SUMS"),
        sums_bytes,
    ));
    tree_entries.push(("SHA256SUMS".to_string(), sums_sha, sums_len));

    // backup.toml
    let corpus_pairs = corpus_entries(&sums_text);
    let fingerprint = manifest::fingerprint(&corpus_pairs);

    let mut sorted_label_strings: Vec<String> = labels
        .ids()
        .iter()
        .map(|id| id.as_str().to_string())
        .collect();
    sorted_label_strings.sort();

    let mut toml_info = BackupToml {
        backup_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        snapshot: next_number,
        created_utc: now.to_string(),
        labels: sorted_label_strings,
        roots: plan.roots.clone(),
        files: plan.files.len() as u64,
        bytes: total_bytes,
        fingerprint: fingerprint.clone(),
        journal_seq,
        journal_head: journal_head.clone(),
        journal_state: if is_broken_chain {
            fieldbook_core::backup::toml::JOURNAL_STATE_BROKEN.to_string()
        } else {
            fieldbook_core::backup::toml::JOURNAL_STATE_OK.to_string()
        },
        manifest_sha256: String::new(),
    };

    let manifest_sha = match manifest_sha256(&toml_info, &sums_text) {
        Ok(s) => s.as_str().to_string(),
        Err(err) => {
            return Outcome {
                stdout: notices,
                stderr: format!("manifest sha computation error: {err}\n"),
                code: EXIT_FINDINGS,
            };
        }
    };
    toml_info.manifest_sha256 = manifest_sha.clone();

    let backup_toml_text = match toml::to_string_pretty(&toml_info) {
        Ok(t) => t,
        Err(err) => {
            return Outcome {
                stdout: notices,
                stderr: format!("backup.toml serialize error: {err}\n"),
                code: EXIT_FINDINGS,
            };
        }
    };
    let toml_bytes = backup_toml_text.as_bytes();
    let toml_sha = sha256_hex(toml_bytes);
    let toml_len = toml_bytes.len() as u64;

    delivery_transfers.push(Transfer::write_path(
        partial_path.join("backup.toml"),
        toml_bytes,
    ));
    tree_entries.push(("backup.toml".to_string(), toml_sha, toml_len));

    // Tree rename transfer
    let (tree_manifest_sha, tree_total_bytes) = hash_tree_manifest(&tree_entries);
    delivery_transfers.push(Transfer {
        target: Target::Path(partial_path.clone()),
        operation: Operation::RenameTree {
            to: final_path.clone(),
        },
        sha256: tree_manifest_sha,
        bytes: tree_total_bytes,
    });

    // LATEST pointer transfer
    let latest_content = format!("{next_number}\n{manifest_sha}\n");
    delivery_transfers.push(Transfer::write_path(
        target_path.join("LATEST"),
        latest_content.as_bytes(),
    ));

    // Leftover .partial removals in target
    delivery_transfers.extend(list_partial_removals(&target_path));

    // Retention removals
    let mut snapshot_infos = read_snapshot_infos(&target_path, &existing_snapshots);
    snapshot_infos.push(SnapshotInfo::new(next_number, now.to_string(), true));

    let newest_verified =
        newest_verified_snapshot_for_target(world.state_dir(), &target_path, &existing_snapshots);
    let retained_snapshots = plan_retention(
        &snapshot_infos,
        newest_verified,
        settings.keep_last,
        settings.keep_weekly,
    );
    // Only an entry whose backup.toml was read and parsed is a snapshot: retention
    // neither keeps nor removes a foreign folder or an unparsable one.
    let mut pruned_snapshots: Vec<Number> = snapshot_infos
        .iter()
        .filter(|info| info.has_backup_toml && info.number != next_number)
        .map(|info| info.number)
        .filter(|n| !retained_snapshots.contains(n))
        .collect();
    if recovery_opt.is_some() {
        pruned_snapshots.clear();
    }

    for num in &pruned_snapshots {
        let prune_dir = target_path.join(format!("fieldbook-backup-{num:04}"));
        if let Ok(Some(t)) = tree_remove_transfer(&prune_dir) {
            delivery_transfers.push(t);
        }
    }

    let delivery = Delivery::new(delivery_transfers, None);

    // 10. Check egress policy
    let dest = Dest::Backup {
        path: target_str.clone(),
        company_storage: target_cfg.company_storage,
    };

    let allowed = match check(eff, &dest, Some(&labels), Some(&delivery), now, shipped) {
        Decision::Allowed(a) => a,
        Decision::Denied(denial) => {
            if !options.dry_run
                && let Some(j) = &journal_opt
            {
                let about = RefuseAbout {
                    labels: (!denial.labels.is_empty())
                        .then(|| LabelSet::from_ids(denial.labels.clone())),
                    dest: Some(DestKind::Backup),
                    ..RefuseAbout::default()
                };
                let _ = j.refuse_backup(denial.code, denial.reason.to_word(), &about);
            }

            let probe_count = copied_files
                .iter()
                .filter(|file| {
                    matches!(
                        check(eff, &dest, Some(&file.labels), None, now, shipped),
                        Decision::Denied(d) if d.code == denial.code && d.reason == denial.reason
                    )
                })
                .count();
            let count = if probe_count == 0 {
                copied_files.len()
            } else {
                probe_count
            };
            let exit_code = if denial.code == Code::Fb0901 || denial.code == Code::Fb0918 {
                EXIT_USAGE
            } else {
                EXIT_FINDINGS
            };
            return Outcome {
                stdout: notices,
                stderr: refusal_text(denial.code, denial.reason, count),
                code: exit_code,
            };
        }
    };

    // 11. If dry-run: report and exit 0 without writing
    if options.dry_run {
        let prune_msg = if pruned_snapshots.is_empty() {
            String::new()
        } else {
            let nums = pruned_snapshots
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("; would prune: {nums}")
        };
        return Outcome {
            stdout: format!(
                "{notices}would write snapshot {next_number}: {} files, {total_bytes} bytes{prune_msg}\n",
                plan.files.len()
            ),
            stderr: String::new(),
            code: EXIT_OK,
        };
    }

    // 12. Write snapshot execution
    let audited = if let Some(mut j) = journal_opt.take() {
        match j.begin_backup(&allowed, &target_path, &target_cfg.kind, next_number) {
            Ok(a) => {
                journal_opt = Some(j);
                a
            }
            Err(err) => {
                return Outcome {
                    stdout: notices,
                    stderr: format!("cannot begin backup transaction: {err}\n"),
                    code: EXIT_IO,
                };
            }
        }
    } else if let Some(rec) = &recovery_opt {
        match rec.approve(&allowed) {
            Ok(a) => a,
            Err(err) => {
                return Outcome {
                    stdout: notices,
                    stderr: format!("cannot approve recovery backup: {err}\n"),
                    code: EXIT_IO,
                };
            }
        }
    } else {
        unreachable!();
    };

    let folder_target = FolderTarget::new(world, &audited, &allowed, &target_path);

    // Clean leftover .partial directories first
    if let Err(err) = folder_target.clean_leftover_partials() {
        return Outcome {
            stdout: notices,
            stderr: format!("clean leftover partials error: {err}\n"),
            code: EXIT_IO,
        };
    }

    // Write files through SnapshotStage
    let mut stage = match folder_target.begin(next_number, &allowed) {
        Ok(s) => s,
        Err(err) => {
            return Outcome {
                stdout: notices,
                stderr: format!("target begin error: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    // Pass 2: re-read files one at a time, verify against pass-1 digest, and stage
    for (file_rel, expected_sha, expected_len) in &recorded_files {
        let bytes = match read_file_content(world, file_rel) {
            Ok(b) => b,
            Err(err) => {
                return Outcome {
                    stdout: notices,
                    stderr: format!("error: {err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        if bytes.len() as u64 != *expected_len || sha256_hex(&bytes) != *expected_sha {
            return Outcome {
                stdout: notices,
                stderr: format!("source changed during backup: {file_rel}\n"),
                code: EXIT_IO,
            };
        }

        let snap_rel = PathBuf::from(format!("data/{file_rel}"));
        if let Err(err) = stage.write(&snap_rel, &bytes) {
            return Outcome {
                stdout: notices,
                stderr: format!("write error for {file_rel}: {err}\n"),
                code: EXIT_IO,
            };
        }
    }

    // Write SHA256SUMS
    if let Err(err) = stage.write(Path::new("SHA256SUMS"), sums_text.as_bytes()) {
        return Outcome {
            stdout: notices,
            stderr: format!("write SHA256SUMS error: {err}\n"),
            code: EXIT_IO,
        };
    }

    // Write backup.toml last inside partial
    if let Err(err) = stage.write(Path::new("backup.toml"), backup_toml_text.as_bytes()) {
        return Outcome {
            stdout: notices,
            stderr: format!("write backup.toml error: {err}\n"),
            code: EXIT_IO,
        };
    }

    // Finish stage: verify readback, rename partial to final, write LATEST
    if let Err(err) = stage.finish() {
        return Outcome {
            stdout: notices,
            stderr: format!("finish snapshot error: {err}\n"),
            code: EXIT_IO,
        };
    }

    // 13. Retention pruning on disk before closing audited scope
    let mut removed: Vec<Number> = Vec::new();
    let mut prune_problems: Vec<String> = Vec::new();
    for num in &pruned_snapshots {
        match folder_target.remove(*num) {
            Ok(()) => removed.push(*num),
            Err(err) => prune_problems.push(format!("could not remove snapshot {num:04}: {err}")),
        }
    }

    // 14. End journal transaction if not recovery mode
    let mut stderr = String::new();
    if let Some(mut j) = journal_opt {
        let target_word = match Word::from_closed(
            &target_cfg.kind,
            fieldbook_core::audit::vocab::BACKUP_TARGET_KINDS,
        ) {
            Ok(w) => w,
            Err(_) => Word::new_static("folder"),
        };
        let fp_sha = Sha256::parse(&fingerprint).unwrap_or_else(|_| Sha256::digest(b""));
        let manifest_sha256_parsed =
            Sha256::parse(&manifest_sha).unwrap_or_else(|_| Sha256::digest(b""));
        let head_sha = Sha256::parse(&journal_head).unwrap_or_else(|_| Sha256::digest(b""));

        if let Err(err) = j.end_backup(
            audited,
            Word::new_static("ok"),
            plan.files.len() as u64,
            total_bytes,
            manifest_sha256_parsed,
            journal_seq,
            head_sha,
            fp_sha,
            target_word,
        ) {
            let mut err_text = format!("end_backup error: {err}\n");
            if !removed.is_empty() {
                let list = removed
                    .iter()
                    .map(|n| format!("{n:04}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                err_text.push_str(&format!("removed without a journal record: {list}\n"));
            }
            for problem in &prune_problems {
                err_text.push_str(problem);
                err_text.push('\n');
            }
            return Outcome {
                stdout: notices,
                stderr: err_text,
                code: EXIT_IO,
            };
        }

        if !removed.is_empty()
            && let Err(err) = j.append_backup_prune(&removed, Word::new_static("retention"))
        {
            prune_problems.push(format!("backup.prune journal append failed: {err}"));
        }
    } else {
        // Recovery mode: refusal stderr notification
        stderr.push_str("refusing to append to journal: hash chain broken\n");
        stderr.push_str("no snapshot removed: the journal is refused\n");
    }

    if !warn_stale.is_empty() {
        stderr.insert_str(0, &warn_stale);
    }

    let mut exit_code = if is_broken_chain {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };

    if !prune_problems.is_empty() {
        for problem in &prune_problems {
            stderr.push_str(problem);
            stderr.push('\n');
        }
        exit_code = EXIT_IO;
    }

    Outcome {
        stdout: format!(
            "{notices}backup {next_number} written: {} files, {total_bytes} bytes\n",
            plan.files.len()
        ),
        stderr,
        code: exit_code,
    }
}

fn is_inside_or_equal(target: &Path, root: &Path) -> bool {
    if let Ok(canon_t) = target.canonicalize()
        && let Ok(canon_r) = root.canonicalize()
    {
        canon_t.starts_with(&canon_r)
    } else {
        target.starts_with(root)
    }
}

fn load_model_if_available(
    world: &dyn World,
) -> (Option<Model>, Option<fieldbook_core::check::Report>) {
    let Ok(tree) = crate::io::read_tree(world.tree_root()) else {
        return (None, None);
    };
    let host = Host::new(tree, world.today());
    let (model, report) = load(&host, &Options::new(false));
    (Some(model), Some(report))
}

/// Executes backup verification for numbered snapshots or raw copies under floor policy.
pub fn run_verify(world: &dyn World, args: &BackupVerifyArgs) -> Outcome {
    let eff = EffectivePolicy::floor(world.has_company_store());
    run_verify_with_policy(world, &eff, args)
}

/// Executes backup verification for numbered snapshots or raw copies under `eff`.
pub fn run_verify_with_policy(
    world: &dyn World,
    eff: &EffectivePolicy,
    args: &BackupVerifyArgs,
) -> Outcome {
    // 1. Enforce support catalogue flags
    if args.support_catalogue.is_some() || args.expect_support_sha256.is_some() {
        if args.snapshot.is_some() || args.target.is_some() || args.in_dir.is_some() {
            return Outcome {
                stdout: String::new(),
                stderr:
                    "error: support catalogue flags cannot be used with numbered snapshot flags\n"
                        .to_string(),
                code: EXIT_USAGE,
            };
        }
        if args.raw.is_none() {
            return Outcome {
                stdout: String::new(),
                stderr: "error: support catalogue flags require --raw\n".to_string(),
                code: EXIT_USAGE,
            };
        }
        if args.support_catalogue.is_none() || args.expect_support_sha256.is_none() {
            return Outcome {
                stdout: String::new(),
                stderr:
                    "error: --support-catalogue and --expect-support-sha256 must be provided together\n"
                        .to_string(),
                code: EXIT_USAGE,
            };
        }
        let Some(raw_path) = args.raw.as_deref() else {
            return Outcome {
                stdout: String::new(),
                stderr: "error: --support-catalogue and --expect-support-sha256 require --raw\n"
                    .to_string(),
                code: EXIT_USAGE,
            };
        };
        return run_verify_raw(
            world,
            eff,
            raw_path,
            args.support_catalogue.as_deref(),
            args.expect_support_sha256.as_deref(),
        );
    }

    // 2. Raw copy verification
    if let Some(raw_path) = &args.raw {
        if args.snapshot.is_some() || args.target.is_some() || args.in_dir.is_some() {
            return Outcome {
                stdout: String::new(),
                stderr: "error: --raw cannot be combined with numbered snapshot flags\n"
                    .to_string(),
                code: EXIT_USAGE,
            };
        }
        return run_verify_raw(world, eff, raw_path, None, None);
    }

    // 3. Numbered snapshot verification
    if world.state_dir().is_file() {
        return Outcome {
            stdout: String::new(),
            stderr: format!("state path is a file: {}\n", world.state_dir().display()),
            code: EXIT_IO,
        };
    }

    let settings = match read_backup_settings(world.state_dir()) {
        Ok(s) => s,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("error[FB0812]: {err}\n"),
                code: EXIT_FINDINGS,
            };
        }
    };

    let events = read_valid_events_prefix(world.state_dir());
    let newest_succ = newest_successful_snapshot_verify_any_events(&events);
    let mut warn_stale = String::new();
    if let Some(t) = newest_succ {
        let age_days = days_elapsed(t.date, world.today());
        if age_days > settings.warn_after_days {
            warn_stale.push_str(&format!(
                "warning[FB0814]: last verified backup is {age_days} days old (warn after {})\n",
                settings.warn_after_days
            ));
        }
    }

    let target_path = if let Some(in_dir) = &args.in_dir {
        in_dir.clone()
    } else if let Some(target_id) = &args.target {
        let Some(found) = settings.targets.iter().find(|t| &t.id == target_id) else {
            return Outcome {
                stdout: String::new(),
                stderr: format!("target `{target_id}` not found in settings.toml\n"),
                code: EXIT_USAGE,
            };
        };
        found.path.clone()
    } else if let Some(def_id) = &settings.default {
        let Some(found) = settings.targets.iter().find(|t| &t.id == def_id) else {
            return Outcome {
                stdout: String::new(),
                stderr: format!("default target `{def_id}` not found in settings.toml\n"),
                code: EXIT_USAGE,
            };
        };
        found.path.clone()
    } else if let Some(first) = settings.targets.first() {
        first.path.clone()
    } else {
        return Outcome {
            stdout: String::new(),
            stderr: format!(
                "no backup target; add this to settings.toml in the state folder:\n{}",
                BACKUP_TARGET_TEMPLATE
            ),
            code: EXIT_USAGE,
        };
    };

    run_verify_numbered(world, eff, args, &target_path, warn_stale)
}

fn run_verify_raw(
    world: &dyn World,
    eff: &EffectivePolicy,
    raw_path: &Path,
    support_catalogue: Option<&Path>,
    expect_support_sha256: Option<&str>,
) -> Outcome {
    if !raw_path.is_dir() {
        return Outcome {
            stdout: String::new(),
            stderr: format!(
                "raw path not found or not a directory: {}\n",
                raw_path.display()
            ),
            code: EXIT_IO,
        };
    }

    let has_data_corpus = raw_path.join("data/corpus").is_dir();
    let has_corpus = raw_path.join("corpus").is_dir();
    if support_catalogue.is_some()
        && ((has_data_corpus && has_corpus) || (!has_data_corpus && !has_corpus))
    {
        return Outcome {
            stdout: String::new(),
            stderr: "error[FB0811]: raw copy public corpus root is zero or ambiguous\n".to_string(),
            code: EXIT_FINDINGS,
        };
    }

    let corpus_root = if has_data_corpus {
        raw_path.join("data")
    } else {
        raw_path.to_path_buf()
    };

    let state_dir = if raw_path.join("data/state/audit").is_dir() {
        raw_path.join("data/state")
    } else if raw_path.join("state/audit").is_dir() {
        raw_path.join("state")
    } else if raw_path.join(".fieldbook/audit").is_dir() {
        raw_path.join(".fieldbook")
    } else if raw_path.join("audit").is_dir() {
        raw_path.to_path_buf()
    } else {
        return Outcome {
            stdout: String::new(),
            stderr: format!(
                "raw copy state/audit directory not found in {}\n",
                raw_path.display()
            ),
            code: EXIT_IO,
        };
    };

    let lease = match world.write_lease() {
        Ok(l) => l,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot acquire store lease: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let mut errors = String::new();
    if !is_journal_chain_clean(&state_dir) {
        errors.push_str("error[FB0800]: journal hash chain is broken\n");
    }

    let tree = match crate::io::read_tree(&corpus_root) {
        Ok(t) => t,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot read raw corpus tree: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let host = Host::new(tree.clone(), world.today());
    let (_model, report) = load(&host, &Options::new(false));

    for diag in &report.diagnostics {
        if diag.code == Code::Fb0211 || diag.code == Code::Fb0212 {
            errors.push_str(&format!(
                "{}:{}:{}: error[{}]: {}\n",
                diag.path, diag.line, diag.col, diag.code, diag.message
            ));
        }
    }

    if !errors.is_empty() {
        return Outcome {
            stdout: String::new(),
            stderr: errors,
            code: EXIT_FINDINGS,
        };
    }

    let raw_canonical = raw_path
        .canonicalize()
        .unwrap_or_else(|_| raw_path.to_path_buf());
    let raw_sha256 = Sha256::digest(raw_canonical.to_string_lossy().as_bytes());

    if support_catalogue.is_none() {
        let verified = BackupVerified::Raw {
            path_sha256: raw_sha256,
            public_support_sha256: None,
        };
        let mut stderr = String::new();
        if let Err(err) = Journal::open(world, world.state_dir(), Via::Cli)
            .and_then(|mut j| j.append_backup_verify(Word::new_static("ok"), verified))
        {
            stderr.push_str(&format!(
                "note: verify was not recorded in the journal: {err}\n"
            ));
        }
        return Outcome {
            stdout: "raw copy consistent\n".to_string(),
            stderr,
            code: EXIT_OK,
        };
    }

    let (Some(cat_file), Some(expected_hex)) = (support_catalogue, expect_support_sha256) else {
        unreachable!("already checked flags are paired");
    };

    if !manifest::is_lower_hex(expected_hex, 64) {
        return Outcome {
            stdout: String::new(),
            stderr: format!("error[FB0811]: expected support sha256 is invalid: {expected_hex}\n"),
            code: EXIT_FINDINGS,
        };
    }

    let cat_bytes = match std::fs::read(cat_file) {
        Ok(b) => b,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot read support catalogue: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let cat = match manifest::verify_support_catalogue_bytes(&cat_bytes, expected_hex) {
        Ok(c) => c,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("error[FB0811]: support catalogue verification failed: {err}\n"),
                code: EXIT_FINDINGS,
            };
        }
    };

    if let Err(err) = cat.verify_present_support_files(&tree) {
        let code_str = match &err {
            manifest::SupportCatalogueError::FileBindingMismatch { reason, .. }
                if reason.starts_with("sha256 mismatch") =>
            {
                "FB0810"
            }
            _ => "FB0811",
        };
        return Outcome {
            stdout: String::new(),
            stderr: format!("error[{code_str}]: {err}\n"),
            code: EXIT_FINDINGS,
        };
    }

    let Ok(cat_text) = std::str::from_utf8(&cat_bytes) else {
        return Outcome {
            stdout: String::new(),
            stderr: "error[FB0811]: support catalogue is not valid UTF-8\n".to_string(),
            code: EXIT_FINDINGS,
        };
    };
    let rules = eff.marking_rules();
    let mut cat_labels = LabelSet::public();
    if let Some(mark_set) = markings_in(cat_text, &rules) {
        cat_labels = cat_labels.union(&mark_set);
    }
    let state_rule = eff.store_rule(&StoreId::State);
    if labels::admit(&cat_labels, &state_rule).is_err() {
        return Outcome {
            stdout: String::new(),
            stderr: "error[FB0811]: support catalogue labels not admitted to state\n".to_string(),
            code: EXIT_FINDINGS,
        };
    }
    let dest_rel = format!("public-support/{expected_hex}.toml");
    let dest_path = world.state_dir().join(&dest_rel);
    let git_ok = git_context(world, &dest_path)
        .map(|ctx| labels::git_guard(&cat_labels, ctx).is_ok())
        .unwrap_or(false);
    if !git_ok {
        return Outcome {
            stdout: String::new(),
            stderr: "error[FB0811]: git guard refuses support catalogue retention\n".to_string(),
            code: EXIT_FINDINGS,
        };
    }

    let mut journal = match Journal::open(world, world.state_dir(), Via::Cli) {
        Ok(j) => j,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot open journal: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    if dest_path.is_file() {
        let existing_bytes = match std::fs::read(&dest_path) {
            Ok(b) => b,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot read existing retained catalogue: {err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        if existing_bytes != cat_bytes {
            return Outcome {
                stdout: String::new(),
                stderr: "error[FB0811]: existing retained catalogue has differing bytes\n"
                    .to_string(),
                code: EXIT_FINDINGS,
            };
        }
    } else {
        let state_audited = match journal.state_scope(&lease) {
            Ok(t) => t,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot acquire state scope: {err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        if let Err(err) = world.write_file(&state_audited, &dest_path, &cat_bytes) {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot retain public support catalogue: {err}\n"),
                code: EXIT_IO,
            };
        }
    }

    let Ok(support_sha) = Sha256::parse(expected_hex) else {
        return Outcome {
            stdout: String::new(),
            stderr: format!("error[FB0811]: invalid support sha256: {expected_hex}\n"),
            code: EXIT_FINDINGS,
        };
    };
    let verified = BackupVerified::Raw {
        path_sha256: raw_sha256,
        public_support_sha256: Some(support_sha),
    };
    if let Err(err) = journal.append_backup_verify(Word::new_static("ok"), verified) {
        return Outcome {
            stdout: String::new(),
            stderr: format!("cannot append backup.verify event: {err}\n"),
            code: EXIT_IO,
        };
    }

    let short = &expected_hex[..12];
    Outcome {
        stdout: format!("raw copy consistent\npublic support catalogue verified: {short}\n"),
        stderr: String::new(),
        code: EXIT_OK,
    }
}

/// Returns the folder name for a numbered snapshot, e.g. `fieldbook-backup-0001`.
pub(crate) fn snapshot_dir_name(n: u32) -> String {
    format!("fieldbook-backup-{n:04}")
}

/// A snapshot folder checked against its own backup.toml and SHA256SUMS; nothing is journaled.
#[derive(Debug, Clone)]
pub(crate) struct SnapshotCheck {
    pub info: BackupToml,
    pub sums_text: String,
    pub snap_dir: PathBuf,
    pub snap_num: u32,
    pub canonical_target: PathBuf,
    pub target_sha256: Sha256,
    pub findings: Vec<String>,
    pub has_fb0811: bool,
    pub has_fb0810: bool,
    pub has_fb0802: bool,
}

impl SnapshotCheck {
    /// True when any of the three failure kinds was found.
    pub fn failed(&self) -> bool {
        self.has_fb0811 || self.has_fb0810 || self.has_fb0802
    }

    /// The first failing code in the order FB0810, FB0811, FB0802, as the text "FB0810" etc.; None when clean.
    pub fn first_code(&self) -> Option<&'static str> {
        if self.has_fb0810 {
            Some("FB0810")
        } else if self.has_fb0811 {
            Some("FB0811")
        } else if self.has_fb0802 {
            Some("FB0802")
        } else {
            None
        }
    }
}

pub(crate) fn check_snapshot(
    target_path: &Path,
    snapshot: Option<u64>,
) -> Result<SnapshotCheck, Outcome> {
    if !target_path.is_dir() {
        return Err(Outcome {
            stdout: String::new(),
            stderr: format!("target directory not found: {}\n", target_path.display()),
            code: EXIT_IO,
        });
    }

    let snap_num = if let Some(n) = snapshot {
        u32::try_from(n).unwrap_or(0)
    } else {
        let latest_file = target_path.join("LATEST");
        if let Ok(content) = std::fs::read_to_string(&latest_file)
            && let Some(first_line) = content.lines().next()
            && let Ok(n) = first_line.trim().parse::<u32>()
        {
            n
        } else {
            let existing = list_existing_snapshots(target_path);
            if let Some(&last) = existing.last() {
                last
            } else {
                return Err(Outcome {
                    stdout: String::new(),
                    stderr: format!("no snapshots found in {}\n", target_path.display()),
                    code: EXIT_IO,
                });
            }
        }
    };

    let snap_dir = target_path.join(snapshot_dir_name(snap_num));
    if !snap_dir.is_dir() {
        return Err(Outcome {
            stdout: String::new(),
            stderr: format!(
                "snapshot {snap_num} directory not found in {}\n",
                target_path.display()
            ),
            code: EXIT_IO,
        });
    }

    let toml_path = snap_dir.join("backup.toml");
    let toml_text = match std::fs::read_to_string(&toml_path) {
        Ok(t) => t,
        Err(err) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("cannot read {}: {err}\n", toml_path.display()),
                code: EXIT_IO,
            });
        }
    };
    let info = match BackupToml::parse(&toml_text) {
        Ok(i) => i,
        Err(err) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!(
                    "error[FB0811]: cannot parse {}: {err}\n",
                    toml_path.display()
                ),
                code: EXIT_FINDINGS,
            });
        }
    };

    let sums_path = snap_dir.join("SHA256SUMS");
    let sums_text = match std::fs::read_to_string(&sums_path) {
        Ok(t) => t,
        Err(err) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("cannot read {}: {err}\n", sums_path.display()),
                code: EXIT_IO,
            });
        }
    };

    let canonical_target = target_path
        .canonicalize()
        .unwrap_or_else(|_| target_path.to_path_buf());
    let target_sha256 = Sha256::digest(canonical_target.to_string_lossy().as_bytes());

    let mut finding_errors = Vec::new();
    let mut has_fb0811 = false;
    let mut has_fb0810 = false;
    let mut has_fb0802 = false;

    // 1. Computed manifest_sha256
    let computed_manifest = fieldbook_core::backup::manifest_sha256(&info, &sums_text);
    if computed_manifest.as_ref().map(|s| s.as_str()) != Ok(info.manifest_sha256.as_str()) {
        finding_errors.push("error[FB0811]: manifest_sha256 mismatch in backup.toml\n".to_string());
        has_fb0811 = true;
    }

    // 2. Parse SHA256SUMS lines
    let mut sums_entries: Vec<(String, String)> = Vec::new();
    for line in sums_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((sha, rel)) = trimmed.split_once("  ") {
            sums_entries.push((rel.to_string(), sha.to_string()));
        } else {
            finding_errors.push(format!(
                "error[FB0811]: malformed line in SHA256SUMS: {trimmed}\n"
            ));
            has_fb0811 = true;
        }
    }

    // 3. File count check
    if sums_entries.len() as u64 != info.files {
        finding_errors.push(format!(
            "error[FB0811]: file count in backup.toml ({}) does not match SHA256SUMS ({})\n",
            info.files,
            sums_entries.len()
        ));
        has_fb0811 = true;
    }

    // 4. Collect all files under data/ on disk
    let data_dir = snap_dir.join("data");
    let mut files_on_disk = Vec::new();
    if data_dir.is_dir() {
        collect_files_recursive(&data_dir, Path::new("data"), &mut files_on_disk);
    }
    let disk_file_set: std::collections::BTreeSet<String> = files_on_disk.into_iter().collect();
    let manifest_file_set: std::collections::BTreeSet<String> =
        sums_entries.iter().map(|(rel, _)| rel.clone()).collect();

    // Check for unlisted files under data/
    for f in &disk_file_set {
        if !manifest_file_set.contains(f) {
            finding_errors.push(format!("error[FB0811]: unlisted file in snapshot: {f}\n"));
            has_fb0811 = true;
        }
    }

    // 5. Re-hash every file in SHA256SUMS (missing or different bytes -> FB0810)
    for (rel, expected_sha) in &sums_entries {
        let full_path = snap_dir.join(rel);
        match std::fs::read(&full_path) {
            Ok(bytes) => {
                let actual_sha = fieldbook_core::hash::sha256_hex(&bytes);
                if actual_sha != *expected_sha {
                    finding_errors.push(format!("{rel}: error[FB0810]: checksum mismatch\n"));
                    has_fb0810 = true;
                }
            }
            Err(_) => {
                finding_errors.push(format!("{rel}: error[FB0810]: file missing\n"));
                has_fb0810 = true;
            }
        }
    }

    // 6. Check snapshot's journal reaches journal_head at journal_seq (FB0802)
    let snap_state_dir = snap_dir.join("data/state");
    let reaches_head =
        check_snapshot_journal(&snap_state_dir, info.journal_seq, &info.journal_head);
    if !reaches_head {
        finding_errors.push(format!(
            "error[FB0802]: snapshot journal does not reach {} at seq {}\n",
            info.journal_head, info.journal_seq
        ));
        has_fb0802 = true;
    }

    Ok(SnapshotCheck {
        info,
        sums_text,
        snap_dir,
        snap_num,
        canonical_target,
        target_sha256,
        findings: finding_errors,
        has_fb0811,
        has_fb0810,
        has_fb0802,
    })
}

fn run_verify_numbered(
    world: &dyn World,
    eff: &EffectivePolicy,
    args: &BackupVerifyArgs,
    target_path: &Path,
    warn_stale: String,
) -> Outcome {
    let check = match check_snapshot(target_path, args.snapshot) {
        Ok(c) => c,
        Err(early) => return early,
    };

    let manifest_sha256_val = match Sha256::parse(&check.info.manifest_sha256) {
        Ok(s) => s,
        Err(_) => Sha256::digest(b""),
    };
    let verified_id = BackupVerified::Snapshot {
        target_sha256: check.target_sha256.clone(),
        snapshot: Count::new(u64::from(check.info.snapshot)),
        manifest_sha256: manifest_sha256_val,
    };

    let has_failure = check.failed();
    let _ = (check.first_code(), &check.sums_text, &check.snap_dir);
    let outcome_word = if has_failure {
        Word::new_static("failed")
    } else {
        Word::new_static("ok")
    };

    let mut findings_text = warn_stale;
    for err_line in &check.findings {
        findings_text.push_str(err_line);
    }

    let record_res = (|| -> Result<(Journal, Now), JournalError> {
        let mut j = Journal::open(world, world.state_dir(), Via::Cli)?;
        let now = j.append_backup_verify(outcome_word, verified_id)?;
        Ok((j, now))
    })();

    let (journal, now) = match record_res {
        Ok(pair) => pair,
        Err(JournalError::BrokenChain(_)) => {
            findings_text.push_str(
                "error[FB0800]: journal hash chain is broken; backup.verify not recorded\n",
            );
            return Outcome {
                stdout: String::new(),
                stderr: findings_text,
                code: EXIT_FINDINGS,
            };
        }
        Err(err) => {
            findings_text.push_str(&format!("error: cannot record backup.verify: {err}\n"));
            return Outcome {
                stdout: String::new(),
                stderr: findings_text,
                code: EXIT_IO,
            };
        }
    };

    let mut save_warn = None;
    if !has_failure {
        let mut cache_labels = LabelSet::owner_supplied();
        let rules = eff.marking_rules();
        let target_str = target_path.to_string_lossy();
        if let Some(marks) = markings_in(&target_str, &rules) {
            cache_labels = cache_labels.union(&marks);
        }
        if let PolicyState::Loaded(p) = eff.state() {
            let state_rule = p.store_rule(&StoreId::State);
            cache_labels = cache_labels.union(&state_rule.default);
        }

        let state_rule = eff.store_rule(&StoreId::State);
        let admit_ok = labels::admit(&cache_labels, &state_rule).is_ok();
        let cache_path = world.state_dir().join("reports/last-backup.json");
        let git_ok = crate::io::root::git_context(world, &cache_path)
            .map(|ctx| labels::git_guard(&cache_labels, ctx).is_ok())
            .unwrap_or(false);

        if admit_ok && git_ok {
            match world.write_lease() {
                Ok(lease) => match journal.state_scope(&lease) {
                    Ok(state_audited) => {
                        let report = LastBackupReport {
                            verification: "snapshot".to_string(),
                            target_path: check.canonical_target.to_string_lossy().into_owned(),
                            labels: cache_labels.ids().to_vec(),
                            target_sha256: check.target_sha256.to_string(),
                            snapshot: check.snap_num,
                            manifest_sha256: check.info.manifest_sha256.clone(),
                            verified_utc: now.to_string(),
                        };
                        let mut json = serde_json::to_string_pretty(&report).unwrap_or_default();
                        json.push('\n');
                        if world
                            .write_file(&state_audited, &cache_path, json.as_bytes())
                            .is_err()
                        {
                            save_warn = Some("warning: could not write reports/last-backup.json\n");
                        }
                    }
                    Err(_) => {
                        save_warn =
                            Some("warning: cannot acquire state scope to save last-backup.json\n");
                    }
                },
                Err(_) => {
                    save_warn =
                        Some("warning: cannot acquire store lease to save last-backup.json\n");
                }
            }
        }
    }

    if has_failure {
        Outcome {
            stdout: String::new(),
            stderr: findings_text,
            code: EXIT_FINDINGS,
        }
    } else {
        let exit_code = if !findings_text.is_empty() {
            EXIT_FINDINGS
        } else {
            EXIT_OK
        };
        if let Some(w) = save_warn {
            findings_text.push_str(w);
        }
        Outcome {
            stdout: format!("snapshot {} verified\n", check.snap_num),
            stderr: findings_text,
            code: exit_code,
        }
    }
}

fn check_snapshot_journal(snap_state_dir: &Path, expected_seq: u64, expected_head: &str) -> bool {
    let audit_dir = snap_state_dir.join("audit");
    let Ok(entries) = std::fs::read_dir(&audit_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in content.lines() {
                if line.is_empty() {
                    continue;
                }
                if let Ok((event, _)) = Event::parse_line(line)
                    && event.seq == expected_seq
                    && event.hash.as_str() == expected_head
                {
                    return true;
                }
            }
        }
    }
    false
}

fn collect_files_recursive(dir: &Path, rel_prefix: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut sorted_entries: Vec<_> = entries.filter_map(Result::ok).collect();
    sorted_entries.sort_by_key(|e| e.file_name());
    for entry in sorted_entries {
        let path = entry.path();
        let name = entry.file_name();
        let rel = rel_prefix.join(name);
        if path.is_dir() {
            collect_files_recursive(&path, &rel, out);
        } else if path.is_file() {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Lists snapshots in target or one-off directory.
pub fn run_list(world: &dyn World, args: &BackupListArgs) -> Outcome {
    if world.state_dir().is_file() {
        return Outcome {
            stdout: String::new(),
            stderr: format!("state path is a file: {}\n", world.state_dir().display()),
            code: EXIT_IO,
        };
    }

    let mut findings = Vec::new();

    let settings = match read_backup_settings(world.state_dir()) {
        Ok(s) => s,
        Err(err) => {
            if args.json {
                findings.push(BackupListFinding {
                    code: "FB0812".to_string(),
                    severity: "error".to_string(),
                });
                BackupSettings {
                    default: None,
                    warn_after_days: 7,
                    keep_last: 7,
                    keep_weekly: 4,
                    targets: Vec::new(),
                }
            } else {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error[FB0812]: {err}\n"),
                    code: EXIT_FINDINGS,
                };
            }
        }
    };

    let target_cfg = if let Some(in_dir) = &args.in_dir {
        Some(BackupTargetConfig {
            id: "one-off".to_owned(),
            kind: "folder".to_owned(),
            path: in_dir.clone(),
            include: None,
            company_storage: false,
        })
    } else if let Some(target_id) = &args.target {
        let Some(found) = settings.targets.iter().find(|t| &t.id == target_id) else {
            return Outcome {
                stdout: String::new(),
                stderr: format!("target `{target_id}` not found in settings.toml\n"),
                code: EXIT_USAGE,
            };
        };
        Some(found.clone())
    } else if let Some(def_id) = &settings.default {
        let Some(found) = settings.targets.iter().find(|t| &t.id == def_id) else {
            return Outcome {
                stdout: String::new(),
                stderr: format!("default target `{def_id}` not found in settings.toml\n"),
                code: EXIT_USAGE,
            };
        };
        Some(found.clone())
    } else {
        settings.targets.first().cloned()
    };

    // Check place of target if present
    if let Some(tc) = &target_cfg {
        let corpus_root = world.tree_root().join("corpus");
        let state_root = world.state_dir().to_path_buf();
        if is_inside_or_equal(&tc.path, &corpus_root) || is_inside_or_equal(&tc.path, &state_root) {
            findings.push(BackupListFinding {
                code: "FB0812".to_string(),
                severity: "error".to_string(),
            });
        }
        let facts = world.place(&tc.path);
        if facts.sync_root && tc.kind != "synced" {
            findings.push(BackupListFinding {
                code: "FB0813".to_string(),
                severity: "warning".to_string(),
            });
        }
    }

    let existing_snapshots = if let Some(tc) = &target_cfg {
        if tc.path.is_dir() {
            list_existing_snapshots(&tc.path)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let has_target_or_snapshot = target_cfg.is_some() || !existing_snapshots.is_empty();

    // If no target and no snapshots, print nothing in text mode and exit 0
    if !has_target_or_snapshot && !args.json {
        return Outcome {
            stdout: String::new(),
            stderr: String::new(),
            code: EXIT_OK,
        };
    }

    let events = read_valid_events_prefix(world.state_dir());
    let newest_succ_any = newest_successful_snapshot_verify_any_events(&events);

    let mut warn_stale = String::new();
    if has_target_or_snapshot {
        if newest_succ_any.is_none() {
            findings.push(BackupListFinding {
                code: "FB0814".to_string(),
                severity: "warning".to_string(),
            });
            warn_stale = "warning[FB0814]: Last verified backup: none yet\n".to_string();
        } else if let Some(t) = newest_succ_any {
            let age_days = days_elapsed(t.date, world.today());
            if age_days > settings.warn_after_days {
                findings.push(BackupListFinding {
                    code: "FB0814".to_string(),
                    severity: "warning".to_string(),
                });
                warn_stale = format!(
                    "warning[FB0814]: last verified backup is {age_days} days old (warn after {})\n",
                    settings.warn_after_days
                );
            }
        }
    }

    let drill = newest_restore_end_events(&events).map(|info| BackupListDrill {
        mode: info.mode.as_str().to_string(),
        outcome: if info.outcome.as_str() == "ok" {
            "ok".to_string()
        } else if let Some(c) = info.code {
            c.to_string()
        } else {
            info.outcome.as_str().to_string()
        },
        utc: info.time.to_string(),
    });

    let mut list_snapshots = Vec::new();
    if let Some(tc) = &target_cfg {
        let canonical_target = tc.path.canonicalize().unwrap_or_else(|_| tc.path.clone());
        let target_sha256 = Sha256::digest(canonical_target.to_string_lossy().as_bytes());

        for &num in &existing_snapshots {
            let snap_dir = tc.path.join(format!("fieldbook-backup-{num:04}"));
            let toml_path = snap_dir.join("backup.toml");
            let Ok(text) = std::fs::read_to_string(&toml_path) else {
                continue;
            };
            let Ok(info) = BackupToml::parse(&text) else {
                continue;
            };

            let sums_path = snap_dir.join("SHA256SUMS");
            let manifest_matches = std::fs::read_to_string(&sums_path)
                .ok()
                .and_then(|sums| manifest_sha256(&info, &sums).ok())
                .is_some_and(|computed| computed.as_str() == info.manifest_sha256);

            let (last_succ, latest_verify) = if manifest_matches {
                let manifest_sha256_val = match Sha256::parse(&info.manifest_sha256) {
                    Ok(s) => s,
                    Err(_) => Sha256::digest(b""),
                };

                let last_succ = newest_successful_snapshot_verify_events(
                    &events,
                    &target_sha256,
                    num,
                    &manifest_sha256_val,
                );
                let latest_verify = newest_snapshot_verify_events(
                    &events,
                    &target_sha256,
                    num,
                    &manifest_sha256_val,
                );
                (last_succ, latest_verify)
            } else {
                (None, None)
            };

            list_snapshots.push(BackupListSnapshot {
                snapshot: info.snapshot,
                backup_version: info.backup_version,
                tool_version: info.tool_version,
                created_utc: info.created_utc,
                labels: info.labels,
                roots: info.roots,
                files: info.files,
                bytes: info.bytes,
                fingerprint: info.fingerprint,
                journal_seq: info.journal_seq,
                journal_head: info.journal_head,
                journal_state: info.journal_state,
                manifest_sha256: info.manifest_sha256,
                verified: last_succ.is_some(),
                verified_utc: last_succ.map(|t| t.to_string()),
                latest_verification: latest_verify.map(|(outcome, time)| {
                    BackupLatestVerification {
                        outcome: outcome.as_str().to_string(),
                        utc: time.to_string(),
                    }
                }),
            });
        }
    }

    let exit_code = if findings.is_empty() {
        EXIT_OK
    } else {
        EXIT_FINDINGS
    };

    if args.json {
        let report = BackupList {
            report_version: 1,
            verb: "backup list".to_string(),
            target: target_cfg.as_ref().map(|tc| BackupListTarget {
                id: tc.id.clone(),
                kind: tc.kind.clone(),
                path: tc.path.to_string_lossy().to_string(),
                company_storage: tc.company_storage,
            }),
            warn_after_days: settings.warn_after_days,
            keep_last: settings.keep_last,
            keep_weekly: settings.keep_weekly,
            snapshots: list_snapshots,
            verified_utc: newest_succ_any.map(|t| t.to_string()),
            drill,
            findings,
            exit: exit_code,
        };

        return Outcome {
            stdout: render_backup_list_json(&report),
            stderr: String::new(),
            code: exit_code,
        };
    }

    // Text table mode
    let mut stdout = String::new();
    if !list_snapshots.is_empty() {
        let _ = writeln!(
            stdout,
            "{:<8}  {:<20}  {:<7}  {:<5}  {:<7}  {:<7}  Verified",
            "Snapshot", "Created (UTC)", "Age (d)", "Files", "Bytes", "Journal"
        );
        for s in &list_snapshots {
            let verified_str = if s.verified { "yes" } else { "no" };
            let age_days = if s.created_utc.len() >= 10 {
                match CivilDate::parse(&s.created_utc[..10]) {
                    Ok(d) => days_elapsed(d, world.today()),
                    Err(_) => 0,
                }
            } else {
                0
            };
            let _ = writeln!(
                stdout,
                "{:<8}  {:<20}  {:<7}  {:<5}  {:<7}  {:<7}  {}",
                s.snapshot,
                s.created_utc,
                age_days,
                s.files,
                s.bytes,
                s.journal_state,
                verified_str
            );
        }
    }

    if newest_succ_any.is_none() && has_target_or_snapshot {
        stdout.push_str("Last verified backup: none yet\n");
    }

    let mut stderr = String::new();
    if !warn_stale.is_empty() {
        stderr.push_str(&warn_stale);
    }
    if let Some(tc) = &target_cfg {
        let corpus_root = world.tree_root().join("corpus");
        let state_root = world.state_dir().to_path_buf();
        if is_inside_or_equal(&tc.path, &corpus_root) || is_inside_or_equal(&tc.path, &state_root) {
            stderr.push_str(&format!(
                "{}: error[FB0812]: target inside corpus or state\n",
                tc.path.display()
            ));
        }
        let facts = world.place(&tc.path);
        if facts.sync_root && tc.kind != "synced" {
            stderr.push_str(&format!(
                "{}: warning[FB0813]: target is a sync root; declare kind = \"synced\" in settings.toml to acknowledge\n",
                tc.path.display()
            ));
        }
    }

    Outcome {
        stdout,
        stderr,
        code: exit_code,
    }
}

/// Formats a [`BackupList`] report as pretty-printed JSON with a trailing newline.
#[must_use]
pub fn render_backup_list_json(report: &BackupList) -> String {
    let mut json = serde_json::to_string_pretty(report).unwrap_or_default();
    json.push('\n');
    json
}

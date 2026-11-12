//! The backup verb: create, verify, or list backup snapshots (`AD-17`, `backup-snapshot`).
//!
//! Implements snapshot creation, verification preflight, egress policy gating,
//! and retention management according to card steps 4, 5, 7, 14, and 30.

use std::path::{Path, PathBuf};

use fieldbook_core::audit::{DestKind, LabelSet, RefuseAbout, Sha256, Via, Word};
use fieldbook_core::backup::{
    BACKUP_TARGET_TEMPLATE, BackupTarget, BackupToml, Number, SnapshotInfo, corpus_entries,
    manifest_sha256, path_budget, plan_backup, plan_retention, snapshot_labels, target_place,
};
use fieldbook_core::check::{Code, Host, Model, Options, load};
use fieldbook_core::egress::{
    Decision, Delivery, Dest, Operation, Reason, Shipped, Target, Transfer, check,
    hash_tree_manifest,
};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::{MARKED, UNLABELLED, git_guard};
use fieldbook_core::manifest;
use fieldbook_core::policy::{EffectivePolicy, path_matches_prefix};

use crate::cli::{BackupArgs, BackupSubcommand, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE};
use crate::io::backup_classify::{classify_file, enumerate_roots, read_file_content};
use crate::io::backup_folder::{
    FolderTarget, list_existing_snapshots, list_partial_removals,
    newest_verified_snapshot_for_target, read_snapshot_infos, tree_remove_transfer,
};
use crate::io::journal::{Journal, JournalError, PreparedBackupBegin, read_valid_events_prefix};
use crate::io::root::git_context;
use crate::io::state::{self, BackupTargetConfig, read_backup_settings};
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

fn run_with_world(args: &BackupArgs, world: &dyn World) -> Outcome {
    if let Some(sub) = &args.command {
        match sub {
            BackupSubcommand::Verify(_) | BackupSubcommand::List(_) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: "not built yet\n".to_string(),
                    code: EXIT_IO,
                };
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

    // 3. Acquire exclusive store lease
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
                match Journal::open_backup_recovery(world, &lease) {
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
                    let _ = j.refuse(
                        Some("backup"),
                        refusal.code,
                        Word::new_static("place"),
                        &about,
                    );
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
    let listing = enumerate_roots(world);

    // Determine next snapshot number
    let existing_snapshots = list_existing_snapshots(&target_path);
    let next_number = existing_snapshots.last().copied().unwrap_or(0) + 1;

    let plan = plan_backup(&listing, include_rules, next_number);

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

    // 7. Load model for label resolution
    let (model, _) = load_model_if_available(world);

    // Classify files and resolve labels
    let mut copied_files = Vec::new();
    let mut file_bytes_cache = Vec::new();

    for file_rel in &plan.files {
        let bytes = read_file_content(world, file_rel);
        let cf = classify_file(world, model.as_ref(), eff, file_rel, &bytes);
        copied_files.push(cf);
        file_bytes_cache.push((file_rel.clone(), bytes));
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
            let _ = j.refuse(
                Some("backup"),
                refusal.code,
                Word::new_static("git"),
                &about,
            );
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

    let partial_dir_name = format!("fieldbook-backup-{next_number:04}.partial");
    let final_dir_name = format!("fieldbook-backup-{next_number:04}");
    let partial_path = target_path.join(&partial_dir_name);
    let final_path = target_path.join(&final_dir_name);

    // Build files delivery and SHA256SUMS
    let mut delivery_transfers = Vec::new();
    let mut sums_entries = Vec::new();
    let mut tree_entries = Vec::new();
    let mut total_bytes = 0u64;

    for (file_rel, mut bytes) in file_bytes_cache {
        // If this is the active journal file and we prepared begin, append prepared bytes
        if let Some(prep) = &prepared_begin
            && file_rel == "state/audit/journal.jsonl"
        {
            bytes.extend_from_slice(&prep.bytes);
        }

        let sha = sha256_hex(&bytes);
        let len = bytes.len() as u64;
        total_bytes = total_bytes.saturating_add(len);

        let snap_rel = format!("data/{file_rel}");
        let full_out_path = partial_path.join(&snap_rel);

        delivery_transfers.push(Transfer::write_path(full_out_path, &bytes));
        sums_entries.push((snap_rel.clone(), sha.clone()));
        tree_entries.push((snap_rel, sha, len));
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
            "broken".to_string()
        } else {
            "ok".to_string()
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
    let pruned_snapshots: Vec<Number> = existing_snapshots
        .iter()
        .copied()
        .filter(|n| !retained_snapshots.contains(n))
        .collect();

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
                let _ = j.refuse(Some("backup"), denial.code, denial.reason.to_word(), &about);
            }

            let causing_count = copied_files
                .iter()
                .filter(|f| {
                    if denial.labels.is_empty() {
                        true
                    } else {
                        f.labels.ids().iter().any(|id| denial.labels.contains(id))
                            || (denial.reason == Reason::Unlabelled
                                && f.labels.contains(&UNLABELLED))
                            || (denial.reason == Reason::Marked && f.labels.contains(&MARKED))
                    }
                })
                .count();
            let count = if causing_count == 0 { 1 } else { causing_count };
            let exit_code = if denial.code == Code::Fb0901 || denial.code == Code::Fb0918 {
                EXIT_USAGE
            } else {
                EXIT_FINDINGS
            };
            return Outcome {
                stdout: notices,
                stderr: format!("error[{}]: backup refused: {} items\n", denial.code, count),
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

    // Copy files one at a time
    for file_rel in &plan.files {
        let bytes = read_file_content(world, file_rel);

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
    for num in &pruned_snapshots {
        let _ = folder_target.remove(*num);
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
            return Outcome {
                stdout: notices,
                stderr: format!("end_backup error: {err}\n"),
                code: EXIT_IO,
            };
        }

        if !pruned_snapshots.is_empty() {
            let _ = j.append_backup_prune(&pruned_snapshots, Word::new_static("retention"));
        }
    } else {
        // Recovery mode: refusal stderr notification
        stderr.push_str("refusing to append to journal: hash chain broken\n");
    }

    let exit_code = if is_broken_chain {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };

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

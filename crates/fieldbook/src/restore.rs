//! The restore verb: restore a backup snapshot or run the restore drill (`AD-16`, `restore-and-drill`).

#![expect(
    clippy::disallowed_methods,
    reason = "restore inspects destination directory and reads snapshot files with std::fs"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use fieldbook_core::CivilDate;
use fieldbook_core::audit::{DestKind, LabelId, LabelSet, RefuseAbout, StoreId, Via, Word};
use fieldbook_core::backup::{
    BACKUP_TARGET_TEMPLATE, DRILL_STEPS, DestinationFacts, PlannedFile, RestoreShell,
    corpus_entries, plan_restore, rename_help,
};
use fieldbook_core::check::{Code, Host, Options};
use fieldbook_core::egress::{
    Decision, Delivery, Dest, Operation, Shipped, Target, Transfer, hash_tree_manifest,
};
use fieldbook_core::manifest;
use fieldbook_core::policy::EffectivePolicy;

use crate::cli::{
    AuditArgs, AuditSubcommand, AuditVerifyArgs, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE,
    RestoreArgs,
};
use crate::io::backup_folder::write_restore_to;
use crate::io::journal::{JournalError, RestoreExportRequest};
use crate::io::root::resolve_canonical_with_tail;
use crate::io::state;
use crate::{Outcome, StateOverrideWorld, World};

/// Signature of the test hook called after snapshot files are copied into the drill folder.
pub type AfterCopyHook = Arc<dyn Fn(&Path) + Send + Sync>;

#[derive(Default)]
struct TestHooks {
    after_copy: Vec<(std::thread::ThreadId, AfterCopyHook)>,
}

fn test_hooks() -> &'static Mutex<TestHooks> {
    static HOOKS: OnceLock<Mutex<TestHooks>> = OnceLock::new();
    HOOKS.get_or_init(|| Mutex::new(TestHooks::default()))
}

/// Set or clear a test hook called after the copy and before drill verification.
pub fn set_after_copy_hook_for_test(hook: Option<AfterCopyHook>) {
    let tid = std::thread::current().id();
    if let Ok(mut guard) = test_hooks().lock() {
        guard.after_copy.retain(|(t, _)| *t != tid);
        if let Some(h) = hook {
            guard.after_copy.push((tid, h));
        }
    }
}

fn run_after_copy_hook(path: &Path) {
    let tid = std::thread::current().id();
    let hook = test_hooks().lock().ok().and_then(|g| {
        g.after_copy
            .iter()
            .find(|(t, _)| *t == tid)
            .map(|(_, h)| h.clone())
    });
    if let Some(h) = hook {
        h(path);
    }
}

/// Options for snapshot restore and restore drill.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Snapshot number to restore.
    pub snapshot: u32,
    /// Restore destination directory for `--to`.
    pub to: Option<PathBuf>,
    /// Whether to run the restore drill into `<state>/drill`.
    pub drill: bool,
    /// Target identifier from `settings.toml`.
    pub target: Option<String>,
    /// Read snapshot from this folder instead of settings.
    pub from: Option<PathBuf>,
}

/// Failure in one of the verification steps of a drill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrillFailure {
    /// Step name in `DRILL_STEPS` that failed.
    pub step: &'static str,
    /// Detailed diagnostic of the failure.
    pub detail: String,
    /// Verification lines that succeeded before the failure occurred.
    pub ok_lines: Vec<String>,
}

/// Runs drill verification steps 3 to 5 (check, journal, fingerprint) in order.
fn drill_verify(
    world: &dyn World,
    check: &crate::backup::SnapshotCheck,
    drill_dir: &Path,
) -> Result<Vec<String>, DrillFailure> {
    let mut ok_lines = Vec::new();

    // Step 3: check
    let tree = match crate::io::tree::read_tree(drill_dir) {
        Ok(t) => t,
        Err(err) => {
            return Err(DrillFailure {
                step: DRILL_STEPS[2],
                detail: err.to_string(),
                ok_lines,
            });
        }
    };

    let date_str = if check.info.created_utc.len() >= 10 {
        &check.info.created_utc[..10]
    } else {
        &check.info.created_utc
    };
    let date = match CivilDate::parse(date_str) {
        Ok(d) => d,
        Err(err) => {
            return Err(DrillFailure {
                step: DRILL_STEPS[2],
                detail: format!("cannot parse created_utc date `{date_str}`: {err}"),
                ok_lines,
            });
        }
    };

    let host = Host::new(tree, date);
    let options = Options::new(true);
    let report = fieldbook_core::check::run(&host, &options);
    if !report.is_ok() {
        let rendered = report.render_text();
        let first_line = rendered.lines().next().unwrap_or("").to_string();
        let count = report.diagnostics.len();
        let detail = if count > 1 {
            format!("{first_line} ({count} findings)")
        } else {
            first_line
        };
        return Err(DrillFailure {
            step: DRILL_STEPS[2],
            detail,
            ok_lines,
        });
    }
    ok_lines.push("ok: check --strict clean".to_string());

    // Step 4: journal
    let audit_args = AuditArgs {
        command: AuditSubcommand::Verify(AuditVerifyArgs {
            expect_seq: Some(check.info.journal_seq),
            expect_head: Some(check.info.journal_head.clone()),
            anchor_seq: None,
            anchor_head: None,
            repair: false,
            json: false,
        }),
    };
    let state_dir = drill_dir.join("state");
    let audit_outcome = crate::audit::run(&audit_args, Some(&state_dir), world);
    if audit_outcome.code != EXIT_OK {
        let all_lines: Vec<&str> = audit_outcome
            .stdout
            .lines()
            .chain(audit_outcome.stderr.lines())
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        let detail = all_lines
            .iter()
            .find(|l| l.contains("FB0"))
            .copied()
            .or_else(|| all_lines.first().copied())
            .unwrap_or("journal verification failed")
            .to_string();
        return Err(DrillFailure {
            step: DRILL_STEPS[3],
            detail,
            ok_lines,
        });
    }
    let short_head = if check.info.journal_head.len() >= 12 {
        &check.info.journal_head[..12]
    } else {
        &check.info.journal_head
    };
    ok_lines.push(format!(
        "ok: journal ok (seq {}, head {short_head})",
        check.info.journal_seq
    ));

    // Step 5: fingerprint
    let corpus_pairs = corpus_entries(&check.sums_text);
    let recomputed = manifest::fingerprint(&corpus_pairs);
    if recomputed != check.info.fingerprint {
        let snap_short = if check.info.fingerprint.len() >= 12 {
            &check.info.fingerprint[..12]
        } else {
            &check.info.fingerprint
        };
        let recomputed_short = if recomputed.len() >= 12 {
            &recomputed[..12]
        } else {
            &recomputed
        };
        return Err(DrillFailure {
            step: DRILL_STEPS[4],
            detail: format!(
                "fingerprint mismatch: snapshot says {snap_short} but the restored files give {recomputed_short}"
            ),
            ok_lines,
        });
    }
    let short_fp = if check.info.fingerprint.len() >= 12 {
        &check.info.fingerprint[..12]
    } else {
        &check.info.fingerprint
    };
    ok_lines.push(format!("ok: fingerprint {short_fp} matches"));

    Ok(ok_lines)
}

/// Dispatches `fieldbook restore` from parsed CLI arguments.
pub fn run(args: &RestoreArgs, state_flag: Option<&Path>, world: &dyn World) -> Outcome {
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

fn run_with_world(args: &RestoreArgs, world: &dyn World) -> Outcome {
    let eff = EffectivePolicy::floor(world.has_company_store());
    let options = RestoreOptions {
        snapshot: args.snapshot,
        to: args.to.clone(),
        drill: args.drill,
        target: args.target.clone(),
        from: args.from.clone(),
    };
    run_restore(world, &eff, &options)
}

/// Executes snapshot restore or drill under the given effective policy.
pub fn run_restore(world: &dyn World, eff: &EffectivePolicy, options: &RestoreOptions) -> Outcome {
    if options.to.is_none() && !options.drill {
        return Outcome {
            stdout: String::new(),
            stderr: "error: restore needs exactly one of --to DIR or --drill\n".to_string(),
            code: EXIT_USAGE,
        };
    }
    if options.to.is_some() && options.drill {
        return Outcome {
            stdout: String::new(),
            stderr: "error: restore needs exactly one of --to DIR or --drill\n".to_string(),
            code: EXIT_USAGE,
        };
    }

    if world.state_dir().is_file() {
        return Outcome {
            stdout: String::new(),
            stderr: format!("state path is a file: {}\n", world.state_dir().display()),
            code: EXIT_IO,
        };
    }

    // Locate the snapshot folder: --from DIR or resolve target from settings
    let target_dir = if let Some(from_dir) = &options.from {
        from_dir.clone()
    } else {
        let settings = match crate::backup::read_backup_settings(world.state_dir()) {
            Ok(s) => s,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error[FB0812]: {err}\n"),
                    code: EXIT_FINDINGS,
                };
            }
        };

        if let Some(target_id) = &options.target {
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
        }
    };

    let check = match crate::backup::check_snapshot(&target_dir, Some(u64::from(options.snapshot)))
    {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    if check.failed() {
        let findings_text = check.findings.join("");
        if options.drill {
            return Outcome {
                stdout: String::new(),
                stderr: findings_text,
                code: EXIT_FINDINGS,
            };
        }
        let first = check.first_code().unwrap_or("FB0811");
        return Outcome {
            stdout: String::new(),
            stderr: format!(
                "error[FB0815]: snapshot {} does not verify ({first})\n{findings_text}",
                options.snapshot
            ),
            code: EXIT_FINDINGS,
        };
    }

    let mut journal = match state::open(world, world.state_dir(), Via::Cli) {
        Ok(j) => j,
        Err(JournalError::BrokenChain(_)) => {
            return Outcome {
                stdout: String::new(),
                stderr: "error[FB0800]: journal hash chain is broken\n".to_string(),
                code: EXIT_FINDINGS,
            };
        }
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot open state journal: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let mut label_ids = Vec::with_capacity(check.info.labels.len());
    for raw in &check.info.labels {
        match LabelId::parse(raw) {
            Ok(id) => label_ids.push(id),
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error[FB0811]: cannot parse label `{raw}`: {err}\n"),
                    code: EXIT_FINDINGS,
                };
            }
        }
    }
    let labels = LabelSet::from_ids(label_ids);

    let mode = if options.drill { "drill" } else { "real" };
    let canonical_tree_root = match resolve_canonical_with_tail(world.cwd(), world.tree_root()) {
        Ok(p) => p,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot resolve tree root: {err}\n"),
                code: EXIT_IO,
            };
        }
    };
    let canonical_state_dir = match resolve_canonical_with_tail(world.cwd(), world.state_dir()) {
        Ok(p) => p,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot resolve state dir: {err}\n"),
                code: EXIT_IO,
            };
        }
    };
    let canonical_corpus_dir = canonical_tree_root.join("corpus");

    let (destination, canon_dest, plan, facts) = if options.drill {
        let destination = world.state_dir().join("drill");
        let canon_dest = match resolve_canonical_with_tail(world.cwd(), &destination) {
            Ok(p) => p,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot resolve drill destination: {err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let facts = DestinationFacts {
            exists: false,
            empty: true,
        };
        let plan = match plan_restore(&check.info, &canon_dest, facts, &[]) {
            Ok(p) => p,
            Err(r) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error[FB0815]: {}\n", r.detail),
                    code: EXIT_FINDINGS,
                };
            }
        };
        (destination, canon_dest, plan, facts)
    } else {
        let Some(to_dir) = &options.to else {
            return Outcome {
                stdout: String::new(),
                stderr: "error: restore needs exactly one of --to DIR or --drill\n".to_string(),
                code: EXIT_USAGE,
            };
        };
        let canon_dest = match resolve_canonical_with_tail(world.cwd(), to_dir) {
            Ok(p) => p,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot resolve destination: {err}\n"),
                    code: EXIT_IO,
                };
            }
        };
        let (exists, empty) = if let Ok(meta) = std::fs::symlink_metadata(&canon_dest) {
            if meta.is_dir() {
                let is_empty = match std::fs::read_dir(&canon_dest) {
                    Ok(mut rd) => rd.next().is_none(),
                    Err(_) => false,
                };
                (true, is_empty)
            } else {
                (true, false)
            }
        } else {
            (false, true)
        };
        let facts = DestinationFacts { exists, empty };

        let drill_dir = canonical_state_dir.join("drill");
        let strictly_inside_drill = canon_dest.starts_with(&drill_dir) && canon_dest != drill_dir;
        let live_roots = if strictly_inside_drill {
            vec![canonical_corpus_dir.clone()]
        } else {
            vec![canonical_corpus_dir.clone(), canonical_state_dir.clone()]
        };

        let plan = match plan_restore(&check.info, &canon_dest, facts, &live_roots) {
            Ok(p) => p,
            Err(r) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("error[FB0815]: {}\n", r.detail),
                    code: EXIT_FINDINGS,
                };
            }
        };
        (to_dir.clone(), canon_dest, plan, facts)
    };

    let ctx = match crate::io::root::git_context(world, &destination) {
        Ok(c) => c,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("error: cannot check git context: {err}\n"),
                code: EXIT_IO,
            };
        }
    };
    if let Err(_refusal) = fieldbook_core::labels::git_guard(&labels, ctx) {
        let about = RefuseAbout {
            labels: Some(labels.clone()),
            ..RefuseAbout::default()
        };
        if let Err(err) = journal.refuse(
            Some("restore"),
            Code::Fb0415,
            Word::new_static("git"),
            &about,
        ) {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot append refuse event: {err}\n"),
                code: EXIT_IO,
            };
        }
        return Outcome {
            stdout: String::new(),
            stderr: format!(
                "error[FB0415]: destination `{}` under git refuses non-public labels\n",
                destination.display()
            ),
            code: EXIT_FINDINGS,
        };
    }

    let is_state_write = options.drill || canon_dest.starts_with(&canonical_state_dir);

    // Build planned files list from SHA256SUMS
    let mut planned_files = Vec::new();
    let mut sums_pairs = Vec::new();
    for line in check.sums_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((sha, rel)) = trimmed.split_once("  ") else {
            continue;
        };
        let Some(stripped) = rel.strip_prefix("data/") else {
            continue;
        };
        let snap_file = check.snap_dir.join(rel);
        let bytes_len = match std::fs::metadata(&snap_file) {
            Ok(m) => m.len(),
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot read {}: {err}\n", snap_file.display()),
                    code: EXIT_IO,
                };
            }
        };
        planned_files.push(PlannedFile {
            path: canon_dest.join(stripped),
            sha256: sha.to_string(),
            bytes: bytes_len,
        });
        sums_pairs.push((sha.to_string(), rel.to_string(), stripped.to_string()));
    }

    let allowed_token = if is_state_write {
        let state_rule = eff.store_rule(&StoreId::State);
        if let Err(refusal) = fieldbook_core::labels::admit(&labels, &state_rule) {
            let about = RefuseAbout {
                labels: Some(labels.clone()),
                ..RefuseAbout::default()
            };
            if let Err(err) = journal.refuse(
                Some("restore"),
                refusal.code,
                Word::new_static("store"),
                &about,
            ) {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot append refuse event: {err}\n"),
                    code: EXIT_IO,
                };
            }
            return Outcome {
                stdout: String::new(),
                stderr: format!(
                    "error[{}]: store `state` refuses labels\n",
                    refusal.code.id()
                ),
                code: EXIT_FINDINGS,
            };
        }
        None
    } else {
        let create_destination = !facts.exists;
        let mut delivery_transfers =
            Vec::with_capacity(planned_files.len() + usize::from(create_destination));
        if create_destination {
            let (dir_sha, dir_bytes) = hash_tree_manifest(&[]);
            delivery_transfers.push(Transfer {
                target: Target::Path(canon_dest.clone()),
                operation: Operation::CreateDir,
                sha256: dir_sha,
                bytes: dir_bytes,
            });
        }
        for file in &planned_files {
            delivery_transfers.push(Transfer {
                target: Target::Path(file.path.clone()),
                operation: Operation::Write,
                sha256: file.sha256.clone(),
                bytes: file.bytes,
            });
        }
        let delivery = Delivery::new(delivery_transfers, None);
        let dest = Dest::Export {
            path: canon_dest.to_string_lossy().into_owned(),
        };
        let now = world.now();
        let shipped = Shipped::Never;
        match fieldbook_core::egress::check(
            eff,
            &dest,
            Some(&labels),
            Some(&delivery),
            now,
            shipped,
        ) {
            Decision::Allowed(a) => Some(a),
            Decision::Denied(denial) => {
                let about = RefuseAbout {
                    labels: (!denial.labels.is_empty())
                        .then(|| LabelSet::from_ids(denial.labels.clone())),
                    dest: Some(DestKind::Export),
                    ..RefuseAbout::default()
                };
                let _ = journal.refuse(
                    Some("restore"),
                    denial.code,
                    denial.reason.to_word(),
                    &about,
                );
                let count = 1;
                let exit_code = if denial.code == Code::Fb0901 || denial.code == Code::Fb0918 {
                    EXIT_USAGE
                } else {
                    EXIT_FINDINGS
                };
                return Outcome {
                    stdout: String::new(),
                    stderr: crate::backup::refusal_text(denial.code, denial.reason, count),
                    code: exit_code,
                };
            }
        }
    };

    let _lease = match world.write_lease() {
        Ok(l) => l,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot acquire store lease: {err}\n"),
                code: EXIT_IO,
            };
        }
    };

    let audited = if let Some(allowed) = &allowed_token {
        let req = RestoreExportRequest {
            snapshot: options.snapshot,
            mode,
            labels: &labels,
            plan: &plan,
            files: &planned_files,
            create_destination: !facts.exists,
        };
        match journal.begin_restore_export(&req, allowed) {
            Ok(tok) => tok,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot begin restore export: {err}\n"),
                    code: EXIT_IO,
                };
            }
        }
    } else {
        match journal.begin_restore(options.snapshot, mode, &labels) {
            Ok(tok) => tok,
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("cannot begin restore: {err}\n"),
                    code: EXIT_IO,
                };
            }
        }
    };

    if options.drill
        && let Err(err) = world.reset_drill(&audited)
    {
        let _ = world.remove_drill(&audited);
        let _ = journal.end_restore(audited, options.snapshot, mode, "failed", None);
        return Outcome {
            stdout: String::new(),
            stderr: format!("cannot reset drill folder: {err}\n"),
            code: EXIT_IO,
        };
    }

    if let Some(allowed) = &allowed_token {
        let mut export_payloads = Vec::with_capacity(sums_pairs.len());
        for (sha, rel, stripped) in &sums_pairs {
            let snap_file = check.snap_dir.join(rel);
            let bytes = match std::fs::read(&snap_file) {
                Ok(b) => b,
                Err(err) => {
                    let _ = journal.end_restore(audited, options.snapshot, mode, "failed", None);
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("cannot read {}: {err}\n", snap_file.display()),
                        code: EXIT_IO,
                    };
                }
            };
            let actual_sha = fieldbook_core::hash::sha256_hex(&bytes);
            if actual_sha != *sha {
                let _ = journal.end_restore(
                    audited,
                    options.snapshot,
                    mode,
                    "failed",
                    Some(Code::Fb0815),
                );
                return Outcome {
                    stdout: String::new(),
                    stderr: format!(
                        "error[FB0815]: snapshot {} does not verify (FB0810)\n{rel}: error[FB0810]: checksum mismatch\n",
                        options.snapshot
                    ),
                    code: EXIT_FINDINGS,
                };
            }
            export_payloads.push((canon_dest.join(stripped), bytes, sha.clone()));
        }
        let file_refs: Vec<(&Path, &[u8], &str)> = export_payloads
            .iter()
            .map(|(p, b, s)| (p.as_path(), b.as_slice(), s.as_str()))
            .collect();
        if let Err(err) = write_restore_to(
            world,
            &audited,
            allowed,
            &canon_dest,
            !facts.exists,
            &file_refs,
        ) {
            let _ = journal.end_restore(audited, options.snapshot, mode, "failed", None);
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot write restore export: {err}\n"),
                code: EXIT_IO,
            };
        }
    } else {
        // In-root (including --drill)
        for (sha, rel, stripped) in &sums_pairs {
            let snap_file = check.snap_dir.join(rel);
            let bytes = match std::fs::read(&snap_file) {
                Ok(b) => b,
                Err(err) => {
                    if options.drill {
                        let _ = world.remove_drill(&audited);
                    }
                    let _ = journal.end_restore(audited, options.snapshot, mode, "failed", None);
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("cannot read {}: {err}\n", snap_file.display()),
                        code: EXIT_IO,
                    };
                }
            };
            let actual_sha = fieldbook_core::hash::sha256_hex(&bytes);
            if actual_sha != *sha {
                if options.drill {
                    let _ = world.remove_drill(&audited);
                    let _ = journal.end_restore(
                        audited,
                        options.snapshot,
                        mode,
                        "failed",
                        Some(Code::Fb0810),
                    );
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("{rel}: error[FB0810]: checksum mismatch\n"),
                        code: EXIT_FINDINGS,
                    };
                }
                let _ = journal.end_restore(
                    audited,
                    options.snapshot,
                    mode,
                    "failed",
                    Some(Code::Fb0815),
                );
                return Outcome {
                    stdout: String::new(),
                    stderr: format!(
                        "error[FB0815]: snapshot {} does not verify (FB0810)\n{rel}: error[FB0810]: checksum mismatch\n",
                        options.snapshot
                    ),
                    code: EXIT_FINDINGS,
                };
            }
            let write_path = destination.join(stripped);
            match world.write_file(&audited, &write_path, &bytes) {
                Ok(written_sha) => {
                    if written_sha.as_str() != sha {
                        if options.drill {
                            let _ = world.remove_drill(&audited);
                        }
                        let _ =
                            journal.end_restore(audited, options.snapshot, mode, "failed", None);
                        return Outcome {
                            stdout: String::new(),
                            stderr: format!(
                                "written digest mismatch for {}\n",
                                write_path.display()
                            ),
                            code: EXIT_IO,
                        };
                    }
                }
                Err(err) => {
                    if options.drill {
                        let _ = world.remove_drill(&audited);
                    }
                    let _ = journal.end_restore(audited, options.snapshot, mode, "failed", None);
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("cannot write {}: {err}\n", write_path.display()),
                        code: EXIT_IO,
                    };
                }
            }
        }
    }

    // Post-copy: drill vs --to
    if options.drill {
        run_after_copy_hook(&destination);
        let verify_res = drill_verify(world, &check, &destination);
        let remove_res = world.remove_drill(&audited);
        match verify_res {
            Ok(extra_lines) => {
                if let Err(err) = remove_res {
                    let _ = journal.end_restore(audited, options.snapshot, "drill", "failed", None);
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("cannot remove drill folder: {err}\n"),
                        code: EXIT_IO,
                    };
                }
                if let Err(err) =
                    journal.end_restore(audited, options.snapshot, "drill", "ok", None)
                {
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!("cannot end restore: {err}\n"),
                        code: EXIT_IO,
                    };
                }
                let mut stdout = format!(
                    "ok: snapshot {} verified\nok: restored {} files, {} bytes into drill folder\n",
                    options.snapshot, check.info.files, check.info.bytes
                );
                for line in &extra_lines {
                    stdout.push_str(line);
                    if !line.ends_with('\n') {
                        stdout.push('\n');
                    }
                }
                let short_fp = if check.info.fingerprint.len() >= 12 {
                    &check.info.fingerprint[..12]
                } else {
                    &check.info.fingerprint
                };
                stdout.push_str(&format!(
                    "drill passed: check clean, journal ok, fingerprint {short_fp} matches\n"
                ));
                Outcome {
                    stdout,
                    stderr: String::new(),
                    code: EXIT_OK,
                }
            }
            Err(failure) => {
                let _ = journal.end_restore(
                    audited,
                    options.snapshot,
                    "drill",
                    "failed",
                    Some(Code::Fb0816),
                );
                let mut stdout = format!(
                    "ok: snapshot {} verified\nok: restored {} files, {} bytes into drill folder\n",
                    options.snapshot, check.info.files, check.info.bytes
                );
                for line in &failure.ok_lines {
                    stdout.push_str(line);
                    if !line.ends_with('\n') {
                        stdout.push('\n');
                    }
                }
                Outcome {
                    stdout,
                    stderr: format!(
                        "error[FB0816]: drill step {} failed: {}\n",
                        failure.step, failure.detail
                    ),
                    code: EXIT_FINDINGS,
                }
            }
        }
    } else {
        if let Err(err) = journal.end_restore(audited, options.snapshot, "real", "ok", None) {
            return Outcome {
                stdout: String::new(),
                stderr: format!("cannot end restore: {err}\n"),
                code: EXIT_IO,
            };
        }
        let live = [
            ("corpus".to_string(), world.tree_root().join("corpus")),
            ("state".to_string(), world.state_dir().to_path_buf()),
        ];
        let sh_lines = rename_help(&plan, &live, RestoreShell::Sh);
        let pwsh_lines = rename_help(&plan, &live, RestoreShell::PowerShell);

        let mut stdout = format!(
            "restored snapshot {} into {} ({} files, {} bytes)\n\nTo use the restored folders, run these commands yourself; fieldbook never writes over live data.\nsh:\n",
            options.snapshot,
            destination.display(),
            check.info.files,
            check.info.bytes
        );
        for line in &sh_lines {
            stdout.push_str("  ");
            stdout.push_str(line);
            stdout.push('\n');
        }
        stdout.push_str("PowerShell:\n");
        for line in &pwsh_lines {
            stdout.push_str("  ");
            stdout.push_str(line);
            stdout.push('\n');
        }
        Outcome {
            stdout,
            stderr: String::new(),
            code: EXIT_OK,
        }
    }
}

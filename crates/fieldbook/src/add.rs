//! `fieldbook add`: store a public original under `corpus/sources/public/` and append its
//! version row to `corpus/registers/sources.csv` (`add-new-and-edition`, `add-folder-mode`,
//! AD-17, ADR 0004).

#![expect(
    clippy::disallowed_methods,
    reason = "add folder traversal reads directory entries and symlink metadata"
)]

use std::path::{Path, PathBuf};

use fieldbook_core::add::{
    AddError, AddOptions, AddOutcome, AddPlan, check_type, needs_extraction, plan_add,
    plan_add_with_allocation, source_exists,
};
use fieldbook_core::audit::{
    Id, OperationIntent, OperationKey, PendingOperation, RecordRef, RefuseAbout, Sha256, StoreId,
    Verb, Via, Word, WriteSet, normalize_affected, request_sha256,
};
use fieldbook_core::check::{self, Code, Host, Model};
use fieldbook_core::ids::{SourceId, VersionId};
use fieldbook_core::labels::{self, LabelContext, LabelSet, PublicAct, Refusal, StoreRule};
use fieldbook_core::policy::{FloorKind, Policy};
use fieldbook_core::report::{AddFileReport, AddFileResult};

use crate::cli::{AddArgs, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE};
use crate::io::{Journal, Recovery};
use crate::report::{
    FileReportEntry, build_add_report, render_add_json, render_add_text, save_last_add_report,
};
use crate::{Outcome, StoreLease, World};

/// Build the [`LabelContext`] for a run of `add` or `new` (`add-new-and-edition` step 3).
///
/// This is the single caller of [`PublicAct::typed`]:
/// - When `confirm_public` is `true`, calls `PublicAct::typed(world.has_company_store(), true)`
///   and wraps it in [`LabelContext::act`].
/// - When `confirm_public` is `false`, returns [`LabelContext::none`].
///
/// # Errors
///
/// Returns [`Refusal`] (`FB0822`, reason `label`) when `confirm_public` is `true` and
/// `world.has_company_store()` is `true`.
pub fn label_context(world: &dyn World, confirm_public: bool) -> Result<LabelContext, Refusal> {
    if confirm_public {
        let act = PublicAct::typed(world.has_company_store(), true)?;
        Ok(LabelContext::act(act))
    } else {
        Ok(LabelContext::none())
    }
}

/// Centralized refusal reason mapping at the shared `add` boundary (`add-new-and-edition` step 13):
/// `FB0410 -> unlabelled`, `FB0400 -> store`, `FB0822 -> label`, `FB0821 -> store`.
#[must_use]
pub const fn add_refuse_reason(code: Code) -> Word {
    match code {
        Code::Fb0410 => Word::new_static("unlabelled"),
        Code::Fb0822 => Word::new_static("label"),
        _ => Word::new_static("store"),
    }
}

/// Append one `refuse` event for `add` using [`add_refuse_reason`] and only the hashes and labels
/// already known at the point of refusal.
///
/// # Errors
///
/// Returns [`crate::io::JournalError`] if the event cannot be appended.
pub fn record_add_refusal(
    journal: &Journal<'_>,
    code: Code,
    file_sha256: Option<Sha256>,
    name_sha256: Option<Sha256>,
    known_labels: Option<LabelSet>,
) -> Result<(), crate::io::JournalError> {
    let reason = add_refuse_reason(code);
    let about = RefuseAbout {
        sha256: file_sha256,
        name_sha256,
        labels: known_labels,
        dest: None,
        system: None,
        stream: None,
    };
    journal.refuse(Some("add"), code, reason, &about)
}

/// Compute the `OperationKey` for a single-file `add` from the verb, destination store, and
/// input content hash (`add-new-and-edition` step 10).
#[must_use]
pub fn add_operation_key(store: StoreId, content_sha256: &Sha256) -> OperationKey {
    let subject = request_sha256(&[
        ("verb", b"add"),
        ("store", store.as_str().as_bytes()),
        ("sha256", content_sha256.as_str().as_bytes()),
    ])
    .unwrap_or_else(|_| unreachable!());
    let verb = Verb::parse("add").unwrap_or_else(|_| unreachable!());
    OperationKey::new(verb, subject)
}

/// Compute the normalized `request_sha256` for `add` over the canonical input-path hash,
/// filename, input content hash, `--source` option, `--metadata-only`, and `--confirm-public`
/// (`add-new-and-edition` step 10).
#[must_use]
pub fn add_request_sha256(
    input_path_sha256: &Sha256,
    file_name: &str,
    content_sha256: &Sha256,
    source: Option<&SourceId>,
    metadata_only: bool,
    confirm_public: bool,
) -> Sha256 {
    let source_bytes = source.map_or(&b""[..], |s| s.as_str().as_bytes());
    let metadata_bytes: &[u8] = if metadata_only { b"1" } else { b"0" };
    let confirm_bytes: &[u8] = if confirm_public { b"1" } else { b"0" };
    request_sha256(&[
        ("input_path_sha256", input_path_sha256.as_str().as_bytes()),
        ("file_name", file_name.as_bytes()),
        ("sha256", content_sha256.as_str().as_bytes()),
        ("source", source_bytes),
        ("metadata_only", metadata_bytes),
        ("confirm_public", confirm_bytes),
    ])
    .unwrap_or_else(|_| unreachable!())
}

/// Run `fieldbook add` under the active floor policy for `world`.
#[must_use]
pub fn run(args: &AddArgs, world: &dyn World) -> Outcome {
    let policy = Policy::floor(FloorKind::for_machine(world.has_company_store()));
    run_with_policy(args, world, &policy)
}

/// Run `fieldbook add` with an explicit [`Policy`] (used by tests exercising custom marking or
/// state-store rules).
#[must_use]
pub fn run_with_policy(args: &AddArgs, world: &dyn World, policy: &Policy) -> Outcome {
    let Some(file_name) = args.file.file_name().and_then(|n| n.to_str()) else {
        return Outcome {
            stdout: String::new(),
            stderr: format!("invalid file name: {}\n", args.file.display()),
            code: EXIT_USAGE,
        };
    };
    if file_name.is_empty() {
        return Outcome {
            stdout: String::new(),
            stderr: "file name must not be empty\n".to_owned(),
            code: EXIT_USAGE,
        };
    }

    let primary_path = if args.file.is_absolute() {
        args.file.clone()
    } else {
        world.cwd().join(&args.file)
    };
    let (input_path, meta) = match std::fs::symlink_metadata(&primary_path) {
        Ok(m) => (primary_path, m),
        Err(primary_err) => {
            if !args.file.is_absolute() && primary_err.kind() == std::io::ErrorKind::NotFound {
                let state_fallback = world.state_dir().join(&args.file);
                if let Ok(m) = std::fs::symlink_metadata(&state_fallback) {
                    (state_fallback, m)
                } else {
                    return io_err(crate::WorldError::new(
                        &primary_path,
                        primary_err.to_string(),
                    ));
                }
            } else {
                return io_err(crate::WorldError::new(
                    &primary_path,
                    primary_err.to_string(),
                ));
            }
        }
    };

    if meta.is_dir() {
        if args.source.is_some() {
            return Outcome {
                stdout: String::new(),
                stderr: "--source cannot be used when adding a folder\n".to_owned(),
                code: EXIT_USAGE,
            };
        }
        run_folder(args, world, policy, &input_path)
    } else {
        run_single_file(args, world, policy, &input_path, file_name)
    }
}

struct OneSuccess {
    entry: FileReportEntry,
    refusal_diag: Option<String>,
    wrote_store: bool,
}

#[expect(
    clippy::too_many_arguments,
    reason = "single-file and folder modes share the complete per-file pipeline"
)]
fn add_one(
    args: &AddArgs,
    world: &dyn World,
    policy: &Policy,
    label_ctx: &Result<LabelContext, Refusal>,
    journal: &mut Journal<'_>,
    lease: &StoreLease,
    cached_model: Option<&Model>,
    input_path: &Path,
    file_name: &str,
    diag_name: &str,
    single_file_recovery: bool,
) -> Result<OneSuccess, Outcome> {
    let file_bytes = crate::io::read_file(input_path).map_err(io_err)?;
    let byte_len = file_bytes.len() as u64;
    let file_sha = Sha256::digest(&file_bytes);
    let name_sha = Sha256::digest(file_name.as_bytes());
    let input_path_sha = crate::io::root::input_path_sha256(input_path).map_err(io_err)?;

    let first_bytes = &file_bytes[..file_bytes.len().min(4)];
    if let Err(type_refusal) = check_type(file_name, first_bytes, StoreId::Corpus) {
        if let Err(err) = record_add_refusal(
            journal,
            type_refusal.code,
            Some(file_sha.clone()),
            Some(name_sha),
            None,
        ) {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            });
        }
        return Ok(OneSuccess {
            entry: FileReportEntry {
                file: AddFileReport {
                    name: file_name.to_owned(),
                    result: AddFileResult::Refused,
                    id: None,
                    sha256: file_sha.as_str().to_owned(),
                    bytes: byte_len,
                    needs_extraction: false,
                    code: Some(type_refusal.code.id().to_owned()),
                },
                observed_labels: None,
            },
            refusal_diag: Some(format!(
                "{diag_name}:1:1: error[{}]: executable or disallowed file type\n",
                type_refusal.code.id()
            )),
            wrote_store: false,
        });
    }

    let context = match label_ctx {
        Ok(ctx) => ctx,
        Err(refusal) => {
            if let Err(err) = record_add_refusal(
                journal,
                refusal.code,
                Some(file_sha.clone()),
                Some(name_sha),
                Some(refusal.labels.clone()),
            ) {
                return Err(Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: err.exit_code(),
                });
            }
            return Ok(OneSuccess {
                entry: FileReportEntry {
                    file: AddFileReport {
                        name: file_name.to_owned(),
                        result: AddFileResult::Refused,
                        id: None,
                        sha256: file_sha.as_str().to_owned(),
                        bytes: byte_len,
                        needs_extraction: false,
                        code: Some(refusal.code.id().to_owned()),
                    },
                    observed_labels: None,
                },
                refusal_diag: Some(format_refusal_diag(diag_name, refusal)),
                wrote_store: false,
            });
        }
    };

    let text = String::from_utf8_lossy(&file_bytes);
    let labels = labels::for_text(&text, &policy.marking_rules(), context);

    if let Err(refusal) = labels::admit(&labels, &StoreRule::corpus()) {
        if let Err(err) = record_add_refusal(
            journal,
            refusal.code,
            Some(file_sha.clone()),
            Some(name_sha),
            Some(refusal.labels.clone()),
        ) {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            });
        }
        return Ok(OneSuccess {
            entry: FileReportEntry {
                file: AddFileReport {
                    name: file_name.to_owned(),
                    result: AddFileResult::Refused,
                    id: None,
                    sha256: file_sha.as_str().to_owned(),
                    bytes: byte_len,
                    needs_extraction: false,
                    code: Some(refusal.code.id().to_owned()),
                },
                observed_labels: Some(refusal.labels.clone()),
            },
            refusal_diag: Some(format_refusal_diag(diag_name, &refusal)),
            wrote_store: false,
        });
    }

    let git_ctx = crate::io::root::git_context(world, world.tree_root()).map_err(io_err)?;
    if let Err(refusal) = labels::git_guard(&labels, git_ctx) {
        if let Err(err) = record_add_refusal(
            journal,
            refusal.code,
            Some(file_sha.clone()),
            Some(name_sha),
            Some(refusal.labels.clone()),
        ) {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            });
        }
        return Ok(OneSuccess {
            entry: FileReportEntry {
                file: AddFileReport {
                    name: file_name.to_owned(),
                    result: AddFileResult::Refused,
                    id: None,
                    sha256: file_sha.as_str().to_owned(),
                    bytes: byte_len,
                    needs_extraction: false,
                    code: Some(refusal.code.id().to_owned()),
                },
                observed_labels: Some(refusal.labels.clone()),
            },
            refusal_diag: Some(format_refusal_diag(diag_name, &refusal)),
            wrote_store: false,
        });
    }

    let owned_model;
    let model = if let Some(m) = cached_model {
        m
    } else {
        let tree = world.read_tree_under(lease).map_err(io_err)?;
        owned_model = check::load(&Host::new(tree, world.today()), &check::Options::new(false)).0;
        &owned_model
    };

    if let Some(ref src_id) = args.source
        && !source_exists(model, src_id)
    {
        return Err(Outcome {
            stdout: String::new(),
            stderr: format!("unknown source `{src_id}` in sources.csv\n"),
            code: EXIT_USAGE,
        });
    }

    let add_options = AddOptions {
        source: args.source.clone(),
        metadata_only: args.metadata_only,
    };

    let op_key = add_operation_key(StoreId::Corpus, &file_sha);
    let req_sha = add_request_sha256(
        &input_path_sha,
        file_name,
        &file_sha,
        args.source.as_ref(),
        args.metadata_only,
        args.confirm_public,
    );

    if single_file_recovery {
        let pending_op = match journal.pending_intent(lease, &op_key, req_sha.clone()) {
            Ok(op) => op,
            Err(err) => {
                return Err(Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: err.exit_code(),
                });
            }
        };
        if let Some(pending) = pending_op {
            let version_id = recover_pending_add(
                world,
                journal,
                lease,
                model,
                file_name,
                &file_bytes,
                &file_sha,
                &add_options,
                &labels,
                &op_key,
                req_sha,
                &pending,
            )?;
            return Ok(OneSuccess {
                entry: FileReportEntry {
                    file: AddFileReport {
                        name: file_name.to_owned(),
                        result: AddFileResult::Added,
                        id: Some(version_id.to_string()),
                        sha256: file_sha.as_str().to_owned(),
                        bytes: byte_len,
                        needs_extraction: needs_extraction(file_name),
                        code: None,
                    },
                    observed_labels: Some(labels),
                },
                refusal_diag: None,
                wrote_store: true,
            });
        }
    }

    let today = world.today();
    let outcome = match plan_add(
        model,
        file_name,
        file_sha.as_str(),
        &add_options,
        None,
        today,
    ) {
        Ok(outcome) => outcome,
        Err(AddError::Exhausted(id_err)) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{} id space exhausted\n", id_err.form()),
                code: EXIT_USAGE,
            });
        }
        Err(AddError::UnknownSource(src_id)) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("unknown source `{src_id}` in sources.csv\n"),
                code: EXIT_USAGE,
            });
        }
        Err(err) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            });
        }
    };

    match outcome {
        AddOutcome::Present(version_id) => Ok(OneSuccess {
            entry: FileReportEntry {
                file: AddFileReport {
                    name: file_name.to_owned(),
                    result: AddFileResult::Present,
                    id: Some(version_id.to_string()),
                    sha256: file_sha.as_str().to_owned(),
                    bytes: byte_len,
                    needs_extraction: needs_extraction(file_name),
                    code: None,
                },
                observed_labels: Some(labels),
            },
            refusal_diag: None,
            wrote_store: false,
        }),
        AddOutcome::Planned(plan) => {
            execute_planned_add(
                world,
                journal,
                lease,
                model.tree(),
                &file_bytes,
                &add_options,
                &labels,
                op_key,
                req_sha,
                today,
                &plan,
            )?;
            Ok(OneSuccess {
                entry: FileReportEntry {
                    file: AddFileReport {
                        name: file_name.to_owned(),
                        result: AddFileResult::Added,
                        id: Some(plan.ids.version_id.to_string()),
                        sha256: file_sha.as_str().to_owned(),
                        bytes: byte_len,
                        needs_extraction: needs_extraction(file_name),
                        code: None,
                    },
                    observed_labels: Some(labels),
                },
                refusal_diag: None,
                wrote_store: true,
            })
        }
    }
}

fn run_single_file(
    args: &AddArgs,
    world: &dyn World,
    policy: &Policy,
    input_path: &Path,
    file_name: &str,
) -> Outcome {
    let display_file = args.file.display().to_string();
    let label_ctx = label_context(world, args.confirm_public);

    let lease = match world.write_lease() {
        Ok(lease) => lease,
        Err(err) => return io_err(err),
    };

    let mut journal = match crate::io::state::open(world, world.state_dir(), Via::Cli) {
        Ok(journal) => journal,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };

    let step = match add_one(
        args,
        world,
        policy,
        &label_ctx,
        &mut journal,
        &lease,
        None,
        input_path,
        file_name,
        &display_file,
        true,
    ) {
        Ok(step) => step,
        Err(err_outcome) => return err_outcome,
    };

    let exit = match step.entry.file.result {
        AddFileResult::Added | AddFileResult::Present => EXIT_OK,
        AddFileResult::Refused => EXIT_FINDINGS,
    };

    let parent_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
    let raw_parent = args.file.parent().unwrap_or_else(|| Path::new("."));
    let dir_display = corpus_relative_dir(parent_dir, raw_parent, world);
    let entries = [step.entry];
    let report = build_add_report(dir_display, &entries, StoreId::Corpus, policy, exit);
    let saved = save_last_add_report(world, &journal, &lease, policy, &report);

    let mut stderr = step.refusal_diag.unwrap_or_default();
    if !saved {
        stderr.push_str("report not saved\n");
    }

    let stdout = if args.json {
        render_add_json(&report)
    } else {
        let file = &entries[0].file;
        match file.result {
            AddFileResult::Added => {
                format!(
                    "added {} {display_file}\n",
                    file.id.as_deref().unwrap_or("")
                )
            }
            AddFileResult::Present => {
                format!(
                    "present {} {display_file}\n",
                    file.id.as_deref().unwrap_or("")
                )
            }
            AddFileResult::Refused => format!("refused {display_file}\n"),
        }
    };

    Outcome {
        stdout,
        stderr,
        code: exit,
    }
}

#[derive(Debug, Clone)]
struct FolderCandidate {
    rel_name: String,
    disk_path: PathBuf,
}

fn enumerate_folder_candidates(
    folder_path: &Path,
    recursive: bool,
) -> Result<Vec<FolderCandidate>, crate::WorldError> {
    let mut out = Vec::new();
    enumerate_folder_inner(folder_path, "", recursive, &mut out)?;
    out.sort_by(|a, b| a.rel_name.as_bytes().cmp(b.rel_name.as_bytes()));
    Ok(out)
}

fn enumerate_folder_inner(
    dir_path: &Path,
    prefix: &str,
    recursive: bool,
    out: &mut Vec<FolderCandidate>,
) -> Result<(), crate::WorldError> {
    let read_dir = std::fs::read_dir(dir_path)
        .map_err(|err| crate::WorldError::new(dir_path, err.to_string()))?;
    for entry_res in read_dir {
        let entry = entry_res.map_err(|err| crate::WorldError::new(dir_path, err.to_string()))?;
        let entry_path = entry.path();
        let meta = std::fs::symlink_metadata(&entry_path)
            .map_err(|err| crate::WorldError::new(&entry_path, err.to_string()))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel_name = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if ft.is_dir() {
            if recursive {
                enumerate_folder_inner(&entry_path, &rel_name, recursive, out)?;
            }
        } else if ft.is_file() {
            out.push(FolderCandidate {
                rel_name,
                disk_path: entry_path,
            });
        }
    }
    Ok(())
}

fn corpus_relative_dir(target_dir: &Path, raw_arg: &Path, world: &dyn World) -> String {
    let rel = if let Ok(stripped) = target_dir.strip_prefix(world.tree_root()) {
        stripped
    } else if let Ok(stripped) = target_dir.strip_prefix(world.cwd()) {
        stripped
    } else if raw_arg.is_relative() {
        raw_arg
    } else {
        target_dir
            .file_name()
            .map_or_else(|| Path::new("."), Path::new)
    };
    let s = rel.to_string_lossy().replace('\\', "/");
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        ".".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn run_folder(args: &AddArgs, world: &dyn World, policy: &Policy, folder_path: &Path) -> Outcome {
    let dir_display = corpus_relative_dir(folder_path, &args.file, world);
    let label_ctx = label_context(world, args.confirm_public);

    let lease = match world.write_lease() {
        Ok(lease) => lease,
        Err(err) => return io_err(err),
    };

    let mut journal = match crate::io::state::open(world, world.state_dir(), Via::Cli) {
        Ok(journal) => journal,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };

    let candidates = match enumerate_folder_candidates(folder_path, args.recursive) {
        Ok(cands) => cands,
        Err(err) => return io_err(err),
    };

    let mut recovered_candidate: Option<(usize, FileReportEntry)> = None;
    let pending_key_opt = match journal.pending_key(&lease) {
        Ok(pk) => pk,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };

    if let Some(pending_key) = pending_key_opt {
        if pending_key.key.verb.as_str() != "add" {
            return Outcome {
                stdout: String::new(),
                stderr: format!(
                    "pending write belongs to verb `{}`, not `add`\n",
                    pending_key.key.verb
                ),
                code: EXIT_IO,
            };
        }

        let mut matches = Vec::new();
        for (idx, cand) in candidates.iter().enumerate() {
            let Ok(bytes) = crate::io::read_file(&cand.disk_path) else {
                continue;
            };
            let Ok(input_path_sha) = crate::io::root::input_path_sha256(&cand.disk_path) else {
                continue;
            };
            let file_sha = Sha256::digest(&bytes);
            let op_key = add_operation_key(StoreId::Corpus, &file_sha);
            let req_sha = add_request_sha256(
                &input_path_sha,
                &cand.rel_name,
                &file_sha,
                None,
                args.metadata_only,
                args.confirm_public,
            );
            if op_key == pending_key.key && req_sha == pending_key.request_sha256 {
                matches.push((idx, bytes, file_sha, op_key, req_sha));
            }
        }

        if matches.len() != 1 {
            return Outcome {
                stdout: String::new(),
                stderr:
                    "pending add write does not match a unique candidate in the target folder\n"
                        .to_owned(),
                code: EXIT_IO,
            };
        }

        let (match_idx, file_bytes, file_sha, op_key, req_sha) = matches.remove(0);
        let cand = &candidates[match_idx];
        let first_bytes = &file_bytes[..file_bytes.len().min(4)];
        if check_type(&cand.rel_name, first_bytes, StoreId::Corpus).is_err() {
            return Outcome {
                stdout: String::new(),
                stderr: "pending candidate failed file type check during recovery\n".to_owned(),
                code: EXIT_IO,
            };
        }
        let Ok(ref context) = label_ctx else {
            return Outcome {
                stdout: String::new(),
                stderr: "pending candidate failed label context check during recovery\n".to_owned(),
                code: EXIT_IO,
            };
        };
        let text = String::from_utf8_lossy(&file_bytes);
        let labels = labels::for_text(&text, &policy.marking_rules(), context);
        if labels::admit(&labels, &StoreRule::corpus()).is_err() {
            return Outcome {
                stdout: String::new(),
                stderr: "pending candidate failed store admission during recovery\n".to_owned(),
                code: EXIT_IO,
            };
        }
        let git_ctx = match crate::io::root::git_context(world, world.tree_root()) {
            Ok(ctx) => ctx,
            Err(err) => return io_err(err),
        };
        if labels::git_guard(&labels, git_ctx).is_err() {
            return Outcome {
                stdout: String::new(),
                stderr: "pending candidate failed git guard during recovery\n".to_owned(),
                code: EXIT_IO,
            };
        }

        let tree = match world.read_tree_under(&lease) {
            Ok(tree) => tree,
            Err(err) => return io_err(err),
        };
        let (model, _) = check::load(&Host::new(tree, world.today()), &check::Options::new(false));
        let add_options = AddOptions {
            source: None,
            metadata_only: args.metadata_only,
        };
        let pending_op = match journal.pending_intent(&lease, &op_key, req_sha.clone()) {
            Ok(Some(op)) => op,
            Ok(None) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: "pending write disappeared before recovery\n".to_owned(),
                    code: EXIT_IO,
                };
            }
            Err(err) => {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: err.exit_code(),
                };
            }
        };

        let version_id = match recover_pending_add(
            world,
            &mut journal,
            &lease,
            &model,
            &cand.rel_name,
            &file_bytes,
            &file_sha,
            &add_options,
            &labels,
            &op_key,
            req_sha,
            &pending_op,
        ) {
            Ok(ver_id) => ver_id,
            Err(err_outcome) => return err_outcome,
        };

        recovered_candidate = Some((
            match_idx,
            FileReportEntry {
                file: AddFileReport {
                    name: cand.rel_name.clone(),
                    result: AddFileResult::Added,
                    id: Some(version_id.to_string()),
                    sha256: file_sha.as_str().to_owned(),
                    bytes: file_bytes.len() as u64,
                    needs_extraction: needs_extraction(&cand.rel_name),
                    code: None,
                },
                observed_labels: Some(labels),
            },
        ));
    }

    let initial_tree = match world.read_tree_under(&lease) {
        Ok(tree) => tree,
        Err(err) => return io_err(err),
    };
    let mut model = check::load(
        &Host::new(initial_tree, world.today()),
        &check::Options::new(false),
    )
    .0;

    let mut entries: Vec<FileReportEntry> = Vec::with_capacity(candidates.len());
    let mut refusal_stderr = String::new();

    for (idx, cand) in candidates.iter().enumerate() {
        if let Some((rec_idx, ref rec_entry)) = recovered_candidate
            && rec_idx == idx
        {
            entries.push(rec_entry.clone());
            continue;
        }

        match add_one(
            args,
            world,
            policy,
            &label_ctx,
            &mut journal,
            &lease,
            Some(&model),
            &cand.disk_path,
            &cand.rel_name,
            &cand.rel_name,
            false,
        ) {
            Ok(step) => {
                if let Some(diag) = step.refusal_diag {
                    refusal_stderr.push_str(&diag);
                }
                if step.wrote_store {
                    let refreshed_tree = match world.read_tree_under(&lease) {
                        Ok(t) => t,
                        Err(err) => {
                            entries.push(step.entry);
                            let mut err_out = io_err(err);
                            err_out.stderr = format!("{refusal_stderr}{}", err_out.stderr);
                            return partial_folder_io_outcome(
                                args,
                                world,
                                policy,
                                &journal,
                                &lease,
                                dir_display,
                                &entries,
                                err_out,
                            );
                        }
                    };
                    model = check::load(
                        &Host::new(refreshed_tree, world.today()),
                        &check::Options::new(false),
                    )
                    .0;
                }
                entries.push(step.entry);
            }
            Err(mut err_outcome) => {
                if err_outcome.code == EXIT_IO {
                    err_outcome.stderr = format!("{refusal_stderr}{}", err_outcome.stderr);
                    return partial_folder_io_outcome(
                        args,
                        world,
                        policy,
                        &journal,
                        &lease,
                        dir_display,
                        &entries,
                        err_outcome,
                    );
                }
                return err_outcome;
            }
        }
    }

    let exit = if entries
        .iter()
        .any(|e| e.file.result == AddFileResult::Refused)
    {
        EXIT_FINDINGS
    } else {
        EXIT_OK
    };

    let report = build_add_report(dir_display, &entries, StoreId::Corpus, policy, exit);
    let saved = save_last_add_report(world, &journal, &lease, policy, &report);

    let mut stderr = refusal_stderr;
    if !saved {
        stderr.push_str("report not saved\n");
    }

    let stdout = if args.json {
        render_add_json(&report)
    } else {
        render_add_text(&report)
    };

    Outcome {
        stdout,
        stderr,
        code: exit,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "partial folder failure persists completed entries and formats JSON when requested"
)]
fn partial_folder_io_outcome(
    args: &AddArgs,
    world: &dyn World,
    policy: &Policy,
    journal: &Journal<'_>,
    lease: &StoreLease,
    dir_display: String,
    entries: &[FileReportEntry],
    err_outcome: Outcome,
) -> Outcome {
    let report = build_add_report(dir_display, entries, StoreId::Corpus, policy, EXIT_IO);
    let saved = save_last_add_report(world, journal, lease, policy, &report);

    let mut stderr = err_outcome.stderr;
    if !saved {
        stderr.push_str("report not saved\n");
    }

    let stdout = if args.json {
        render_add_json(&report)
    } else {
        err_outcome.stdout
    };

    Outcome {
        stdout,
        stderr,
        code: EXIT_IO,
    }
}

fn build_affected_refs(plan: &AddPlan) -> Vec<RecordRef> {
    let src_id = Id::parse(plan.ids.source_id.as_str()).unwrap_or_else(|_| unreachable!());
    let ver_id = Id::parse(plan.ids.version_id.as_str()).unwrap_or_else(|_| unreachable!());
    normalize_affected(&[
        RecordRef::new(StoreId::Corpus, src_id),
        RecordRef::new(StoreId::Corpus, ver_id),
    ])
}

fn parse_stored_allocations(allocated: &[RecordRef]) -> Option<(SourceId, VersionId)> {
    let mut source_id = None;
    let mut version_id = None;
    for r in allocated {
        if r.store() != StoreId::Corpus {
            return None;
        }
        let raw = r.id().as_str();
        if let Ok(ver) = VersionId::parse(raw) {
            version_id = Some(ver);
        } else if let Ok(src) = SourceId::parse(raw) {
            source_id = Some(src);
        }
    }
    match (source_id, version_id) {
        (Some(s), Some(v)) if v.source() == s => Some((s, v)),
        (None, Some(v)) => Some((v.source(), v)),
        _ => None,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "recovery coordinates world, journal, lease, model, input bytes, and pending operation"
)]
fn recover_pending_add(
    world: &dyn World,
    journal: &mut Journal<'_>,
    lease: &StoreLease,
    model: &fieldbook_core::check::Model,
    file_name: &str,
    file_bytes: &[u8],
    file_sha: &Sha256,
    add_options: &AddOptions,
    labels: &LabelSet,
    op_key: &OperationKey,
    req_sha: Sha256,
    pending: &PendingOperation,
) -> Result<VersionId, Outcome> {
    let Some((alloc_source, alloc_version)) = parse_stored_allocations(&pending.intent.allocated)
    else {
        return Err(Outcome {
            stdout: String::new(),
            stderr: "pending write descriptor is missing valid source/version allocations\n"
                .to_owned(),
            code: EXIT_IO,
        });
    };

    let plan = match plan_add_with_allocation(
        model,
        file_name,
        file_sha.as_str(),
        add_options,
        None,
        &alloc_source,
        &alloc_version,
        pending.intent.planned_on,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            });
        }
    };

    let affected = build_affected_refs(&plan);
    let write_set = WriteSet {
        labels: labels.clone(),
        files: plan.files.clone(),
        affected,
    };

    let recovery = match journal.recover(lease, op_key, req_sha.clone(), &write_set) {
        Ok(rec) => rec,
        Err(err) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            });
        }
    };

    match recovery {
        Recovery::Completed { .. } => Ok(plan.ids.version_id),
        Recovery::Resume { audited, .. } => {
            if !add_options.metadata_only
                && let Some(orig_wf) = plan.files.first()
            {
                let orig_disk = world.tree_root().join(&orig_wf.path);
                let already_written = crate::io::read_file(&orig_disk)
                    .ok()
                    .is_some_and(|b| &Sha256::digest(&b) == file_sha);
                if !already_written {
                    world
                        .write_file(&audited, &orig_disk, file_bytes)
                        .map_err(io_err)?;
                }
            }

            let reg_disk = world.tree_root().join("corpus/registers/sources.csv");
            world
                .write_file(&audited, &reg_disk, &plan.sources_csv)
                .map_err(io_err)?;

            if let Err(err) = journal.end(audited, Word::new_static("ok")) {
                return Err(Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: err.exit_code(),
                });
            }

            Ok(plan.ids.version_id)
        }
        Recovery::None => {
            execute_planned_add(
                world,
                journal,
                lease,
                model.tree(),
                file_bytes,
                add_options,
                labels,
                op_key.clone(),
                req_sha,
                pending.intent.planned_on,
                &plan,
            )?;
            Ok(plan.ids.version_id)
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "executes a validated AddPlan under the held store lease and journal transaction"
)]
fn execute_planned_add(
    world: &dyn World,
    journal: &mut Journal<'_>,
    lease: &StoreLease,
    tree: &fieldbook_core::check::Tree,
    file_bytes: &[u8],
    add_options: &AddOptions,
    labels: &LabelSet,
    op_key: OperationKey,
    req_sha: Sha256,
    planned_on: fieldbook_core::CivilDate,
    plan: &AddPlan,
) -> Result<(), Outcome> {
    if !add_options.metadata_only
        && let Some(orig_wf) = plan.files.first()
    {
        let prefix = format!("corpus/sources/public/{}.", plan.ids.version_id);
        for (path, entry) in tree.iter() {
            let is_direct_public_file = path
                .strip_prefix("corpus/sources/public/")
                .is_some_and(|rest| !rest.contains('/'));
            if is_direct_public_file && (path == orig_wf.path || path.starts_with(&prefix)) {
                let matches_plan = path == orig_wf.path
                    && match entry {
                        fieldbook_core::check::Entry::Original { sha256, .. } => {
                            sha256 == &orig_wf.sha256
                        }
                        fieldbook_core::check::Entry::Bytes(bytes) => {
                            Sha256::digest(bytes).as_str() == orig_wf.sha256
                        }
                    };
                if !matches_plan {
                    return Err(Outcome {
                        stdout: String::new(),
                        stderr: format!(
                            "{path}: existing original for {} has conflicting content; refusing to overwrite\n",
                            plan.ids.version_id
                        ),
                        code: EXIT_IO,
                    });
                }
            }
        }
    }

    let affected = build_affected_refs(plan);
    let intent = OperationIntent {
        schema: 1,
        key: op_key,
        request_sha256: req_sha,
        allocated: affected.clone(),
        allocated_store_uid: None,
        planned_on,
    };
    let write_set = WriteSet {
        labels: labels.clone(),
        files: plan.files.clone(),
        affected,
    };

    let audited = match journal.begin(lease, &intent, &write_set) {
        Ok(token) => token,
        Err(err) => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            });
        }
    };

    if !add_options.metadata_only
        && let Some(orig_wf) = plan.files.first()
    {
        let orig_disk = world.tree_root().join(&orig_wf.path);
        world
            .write_file(&audited, &orig_disk, file_bytes)
            .map_err(io_err)?;
    }

    let reg_disk = world.tree_root().join("corpus/registers/sources.csv");
    world
        .write_file(&audited, &reg_disk, &plan.sources_csv)
        .map_err(io_err)?;

    if let Err(err) = journal.end(audited, Word::new_static("ok")) {
        return Err(Outcome {
            stdout: String::new(),
            stderr: format!("{err}\n"),
            code: err.exit_code(),
        });
    }

    Ok(())
}

fn format_refusal_diag(display_file: &str, refusal: &Refusal) -> String {
    let reason = add_refuse_reason(refusal.code);
    format!(
        "{display_file}:1:1: error[{}]: store `{}` refused labels `{}` ({reason})\n",
        refusal.code.id(),
        StoreId::Corpus,
        refusal.labels.to_cell(),
    )
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "match arms and Result::map_err pass owned WorldError into io_err"
)]
fn io_err(err: crate::WorldError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_IO,
    }
}

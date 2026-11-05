//! `fieldbook add`: store a public original under `corpus/sources/public/` and append its
//! version row to `corpus/registers/sources.csv` (`add-new-and-edition`, AD-17, ADR 0004).

use fieldbook_core::add::{
    AddError, AddOptions, AddOutcome, AddPlan, plan_add, plan_add_with_allocation, source_exists,
};
use fieldbook_core::audit::{
    Id, OperationIntent, OperationKey, PendingOperation, RecordRef, RefuseAbout, Sha256, StoreId,
    Verb, Via, Word, WriteSet, normalize_affected, request_sha256,
};
use fieldbook_core::check::{self, Code, Host};
use fieldbook_core::ids::{SourceId, VersionId};
use fieldbook_core::labels::{
    self, LabelContext, LabelSet, MarkingRules, PublicAct, Refusal, StoreRule,
};

use crate::cli::{AddArgs, EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE};
use crate::io::{Journal, Recovery};
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

/// Run `fieldbook add`.
#[must_use]
pub fn run(args: &AddArgs, world: &dyn World) -> Outcome {
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

    let input_path = if args.file.is_absolute() {
        args.file.clone()
    } else {
        world.cwd().join(&args.file)
    };
    let display_file = args.file.display().to_string();

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

    let file_bytes = match crate::io::read_file(&input_path) {
        Ok(bytes) => bytes,
        Err(err) => return io_err(err),
    };
    let file_sha = Sha256::digest(&file_bytes);
    let name_sha = Sha256::digest(file_name.as_bytes());
    let input_path_sha = match crate::io::root::input_path_sha256(&input_path) {
        Ok(sha) => sha,
        Err(err) => return io_err(err),
    };

    let context = match label_context(world, args.confirm_public) {
        Ok(ctx) => ctx,
        Err(refusal) => {
            if let Err(err) = record_add_refusal(
                &journal,
                refusal.code,
                Some(file_sha),
                Some(name_sha),
                Some(refusal.labels.clone()),
            ) {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: err.exit_code(),
                };
            }
            return refused_outcome(&display_file, &refusal);
        }
    };

    let text = String::from_utf8_lossy(&file_bytes);
    let labels = labels::for_text(&text, &MarkingRules::builtin(), &context);

    if let Err(refusal) = labels::admit(&labels, &StoreRule::corpus()) {
        if let Err(err) = record_add_refusal(
            &journal,
            refusal.code,
            Some(file_sha),
            Some(name_sha),
            Some(refusal.labels.clone()),
        ) {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
        return refused_outcome(&display_file, &refusal);
    }

    let git_ctx = match crate::io::root::git_context(world, world.tree_root()) {
        Ok(ctx) => ctx,
        Err(err) => return io_err(err),
    };
    if let Err(refusal) = labels::git_guard(&labels, git_ctx) {
        if let Err(err) = record_add_refusal(
            &journal,
            refusal.code,
            Some(file_sha),
            Some(name_sha),
            Some(refusal.labels.clone()),
        ) {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
        return refused_outcome(&display_file, &refusal);
    }

    let tree = match world.read_tree_under(&lease) {
        Ok(tree) => tree,
        Err(err) => return io_err(err),
    };
    let today = world.today();
    let host = Host::new(tree, today);
    let (model, _) = check::load(&host, &check::Options::new(false));

    if let Some(ref src_id) = args.source
        && !source_exists(&model, src_id)
    {
        return Outcome {
            stdout: String::new(),
            stderr: format!("unknown source `{src_id}` in sources.csv\n"),
            code: EXIT_USAGE,
        };
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

    let pending_op = match journal.pending_intent(&lease, &op_key, req_sha.clone()) {
        Ok(op) => op,
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };

    if let Some(pending) = pending_op {
        return recover_pending_add(
            world,
            &mut journal,
            &lease,
            &model,
            file_name,
            &display_file,
            &file_bytes,
            &file_sha,
            &add_options,
            &labels,
            &op_key,
            req_sha,
            &pending,
        );
    }

    let outcome = match plan_add(
        &model,
        file_name,
        file_sha.as_str(),
        &add_options,
        None,
        today,
    ) {
        Ok(outcome) => outcome,
        Err(AddError::Exhausted(id_err)) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{} id space exhausted\n", id_err.form()),
                code: EXIT_USAGE,
            };
        }
        Err(AddError::UnknownSource(src_id)) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("unknown source `{src_id}` in sources.csv\n"),
                code: EXIT_USAGE,
            };
        }
        Err(err) => {
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
        }
    };

    match outcome {
        AddOutcome::Present(version_id) => Outcome {
            stdout: format!("present {version_id} {display_file}\n"),
            stderr: String::new(),
            code: EXIT_OK,
        },
        AddOutcome::Planned(plan) => execute_planned_add(
            world,
            &mut journal,
            &lease,
            model.tree(),
            &display_file,
            &file_bytes,
            &add_options,
            &labels,
            op_key,
            req_sha,
            today,
            &plan,
        ),
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
    display_file: &str,
    file_bytes: &[u8],
    file_sha: &Sha256,
    add_options: &AddOptions,
    labels: &LabelSet,
    op_key: &OperationKey,
    req_sha: Sha256,
    pending: &PendingOperation,
) -> Outcome {
    let Some((alloc_source, alloc_version)) = parse_stored_allocations(&pending.intent.allocated)
    else {
        return Outcome {
            stdout: String::new(),
            stderr: "pending write descriptor is missing valid source/version allocations\n"
                .to_owned(),
            code: EXIT_IO,
        };
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
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: EXIT_IO,
            };
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
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };

    match recovery {
        Recovery::Completed { .. } => Outcome {
            stdout: format!("added {} {display_file}\n", plan.ids.version_id),
            stderr: String::new(),
            code: EXIT_OK,
        },
        Recovery::Resume { audited, .. } => {
            if !add_options.metadata_only
                && let Some(orig_wf) = plan.files.first()
            {
                let orig_disk = world.tree_root().join(&orig_wf.path);
                let already_written = crate::io::read_file(&orig_disk)
                    .ok()
                    .is_some_and(|b| &Sha256::digest(&b) == file_sha);
                if !already_written
                    && let Err(err) = world.write_file(&audited, &orig_disk, file_bytes)
                {
                    return io_err(err);
                }
            }

            let reg_disk = world.tree_root().join("corpus/registers/sources.csv");
            if let Err(err) = world.write_file(&audited, &reg_disk, &plan.sources_csv) {
                return io_err(err);
            }

            if let Err(err) = journal.end(audited, Word::new_static("ok")) {
                return Outcome {
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                    code: err.exit_code(),
                };
            }

            Outcome {
                stdout: format!("added {} {display_file}\n", plan.ids.version_id),
                stderr: String::new(),
                code: EXIT_OK,
            }
        }
        Recovery::None => execute_planned_add(
            world,
            journal,
            lease,
            model.tree(),
            display_file,
            file_bytes,
            add_options,
            labels,
            op_key.clone(),
            req_sha,
            pending.intent.planned_on,
            &plan,
        ),
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
    display_file: &str,
    file_bytes: &[u8],
    add_options: &AddOptions,
    labels: &LabelSet,
    op_key: OperationKey,
    req_sha: Sha256,
    planned_on: fieldbook_core::CivilDate,
    plan: &AddPlan,
) -> Outcome {
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
                    return Outcome {
                        stdout: String::new(),
                        stderr: format!(
                            "{path}: existing original for {} has conflicting content; refusing to overwrite\n",
                            plan.ids.version_id
                        ),
                        code: EXIT_IO,
                    };
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
            return Outcome {
                stdout: String::new(),
                stderr: format!("{err}\n"),
                code: err.exit_code(),
            };
        }
    };

    if !add_options.metadata_only
        && let Some(orig_wf) = plan.files.first()
    {
        let orig_disk = world.tree_root().join(&orig_wf.path);
        if let Err(err) = world.write_file(&audited, &orig_disk, file_bytes) {
            return io_err(err);
        }
    }

    let reg_disk = world.tree_root().join("corpus/registers/sources.csv");
    if let Err(err) = world.write_file(&audited, &reg_disk, &plan.sources_csv) {
        return io_err(err);
    }

    if let Err(err) = journal.end(audited, Word::new_static("ok")) {
        return Outcome {
            stdout: String::new(),
            stderr: format!("{err}\n"),
            code: err.exit_code(),
        };
    }

    Outcome {
        stdout: format!("added {} {display_file}\n", plan.ids.version_id),
        stderr: String::new(),
        code: EXIT_OK,
    }
}

fn refused_outcome(display_file: &str, refusal: &Refusal) -> Outcome {
    let reason = add_refuse_reason(refusal.code);
    Outcome {
        stdout: format!("refused {display_file}\n"),
        stderr: format!(
            "{display_file}:1:1: error[{}]: store `{}` refused labels `{}` ({reason})\n",
            refusal.code.id(),
            StoreId::Corpus,
            refusal.labels.to_cell(),
        ),
        code: EXIT_FINDINGS,
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "match arms pass owned WorldError into io_err"
)]
fn io_err(err: crate::WorldError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_IO,
    }
}

//! `fieldbook fmt`: normalize registers and page bodies under `corpus/`.

use fieldbook_core::audit::{
    OperationIntent, OperationKey, PendingOperation, Sha256, StoreId, Verb, Via, Word, WriteFile,
    WriteSet, request_sha256,
};
use fieldbook_core::check::{Entry, Tree};
use fieldbook_core::labels::LabelSet;
use fieldbook_core::schema::{Register, page, table};

use crate::cli::{EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE, FmtArgs};
use crate::io::{Journal, JournalError, Recovery};
use crate::{Outcome, StoreLease, World, WorldError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Register(Register),
    Page,
}

fn classify_target(path: &str) -> Option<TargetKind> {
    if let Some(name) = path.strip_prefix("corpus/registers/") {
        if !name.contains('/') && name.ends_with(".csv") {
            return Register::ALL
                .iter()
                .copied()
                .find(|reg| reg.file_name() == name)
                .map(TargetKind::Register);
        }
        return None;
    }
    if let Some(rest) = path.strip_prefix("corpus/pages/")
        && !rest.is_empty()
        && std::path::Path::new(rest)
            .extension()
            .is_some_and(|ext| ext == "md")
    {
        return Some(TargetKind::Page);
    }
    None
}

fn normalize_target(path: &str, kind: TargetKind, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match kind {
        TargetKind::Register(reg) => table::normalize(reg, bytes)
            .map(String::into_bytes)
            .map_err(|err| {
                format!(
                    "{path}:{}:{}: error: {}\n",
                    err.line(),
                    err.col(),
                    err.message()
                )
            }),
        TargetKind::Page => page::normalize(bytes)
            .map(String::into_bytes)
            .map_err(|err| {
                format!(
                    "{path}:{}:{}: error: {}\n",
                    err.line(),
                    err.col(),
                    err.message()
                )
            }),
    }
}

struct ChangedFile {
    path: String,
    seen: Sha256,
    after: Sha256,
    bytes: Vec<u8>,
}

struct EvaluatedTree {
    total: usize,
    changed: Vec<ChangedFile>,
    errors: String,
}

fn evaluate_tree(tree: &Tree) -> EvaluatedTree {
    let mut total = 0usize;
    let mut changed = Vec::new();
    let mut errors = String::new();

    for (path, entry) in tree.iter() {
        let Some(kind) = classify_target(path) else {
            continue;
        };
        let Entry::Bytes(bytes) = entry else {
            continue;
        };
        total += 1;
        match normalize_target(path, kind, bytes) {
            Ok(norm) => {
                if norm.as_slice() != bytes.as_slice() {
                    let seen = Sha256::digest(bytes);
                    let after = Sha256::digest(&norm);
                    changed.push(ChangedFile {
                        path: path.to_owned(),
                        seen,
                        after,
                        bytes: norm,
                    });
                }
            }
            Err(diag) => errors.push_str(&diag),
        }
    }

    EvaluatedTree {
        total,
        changed,
        errors,
    }
}

fn operation_identity() -> Result<(OperationKey, Sha256), Outcome> {
    let verb = Verb::parse("fmt").map_err(usage_err)?;
    let store_bytes = StoreId::Corpus.as_str().as_bytes();
    let subject = request_sha256(&[("verb", b"fmt"), ("store", store_bytes)]).map_err(usage_err)?;
    let req = request_sha256(&[
        ("verb", b"fmt"),
        ("store", store_bytes),
        ("mode", b"write"),
        ("paths", b"corpus/pages/**/*.md\ncorpus/registers/*.csv"),
    ])
    .map_err(usage_err)?;
    Ok((OperationKey::new(verb, subject), req))
}

struct ReconstructedFile {
    path: String,
    live_sha: Sha256,
    after_sha: Sha256,
    bytes: Vec<u8>,
}

fn try_recover_pending(
    world: &dyn World,
    journal: &mut Journal<'_>,
    lease: &StoreLease,
    tree: &Tree,
    key: &OperationKey,
    req_sha: &Sha256,
    pending: &PendingOperation,
) -> Result<Option<Outcome>, Outcome> {
    let mut items = Vec::with_capacity(pending.writes.files.len());
    let mut write_files = Vec::with_capacity(pending.writes.files.len());

    for wf in &pending.writes.files {
        let path = wf.path.as_str();
        let Some(kind) = classify_target(path) else {
            return Err(journal_err(&JournalError::InvalidPending(format!(
                "recorded pending path `{path}` is not a fmt target"
            ))));
        };
        let Some(Entry::Bytes(live_bytes)) = tree.get(path) else {
            return Err(journal_err(&JournalError::InvalidPending(format!(
                "recorded pending path `{path}` is missing from tree"
            ))));
        };
        let live_sha = Sha256::digest(live_bytes);
        let norm_bytes = normalize_target(path, kind, live_bytes).map_err(|diag| {
            journal_err(&JournalError::InvalidPending(format!(
                "recorded pending path `{path}` failed to normalize during recovery: {}",
                diag.trim()
            )))
        })?;
        let after_sha = Sha256::digest(&norm_bytes);
        write_files.push(WriteFile::new(path, after_sha.as_str()));
        items.push(ReconstructedFile {
            path: path.to_owned(),
            live_sha,
            after_sha,
            bytes: norm_bytes,
        });
    }

    let writes = WriteSet {
        labels: LabelSet::public(),
        files: write_files,
        affected: Vec::new(),
    };

    let recovery = journal
        .recover(lease, key, req_sha.clone(), &writes)
        .map_err(|err| journal_err(&err))?;

    match recovery {
        Recovery::None => Ok(None),
        Recovery::Completed { .. } => {
            let stdout = formatted_lines(items.iter().map(|i| i.path.as_str()));
            Ok(Some(Outcome {
                stdout,
                stderr: String::new(),
                code: EXIT_OK,
            }))
        }
        Recovery::Resume { audited, .. } => {
            for item in &items {
                if item.live_sha != item.after_sha {
                    world
                        .write_file(&audited, &world.tree_root().join(&item.path), &item.bytes)
                        .map_err(io_err)?;
                }
            }
            journal
                .end(audited, Word::new_static("ok"))
                .map_err(|err| journal_err(&err))?;
            let stdout = formatted_lines(items.iter().map(|i| i.path.as_str()));
            Ok(Some(Outcome {
                stdout,
                stderr: String::new(),
                code: EXIT_OK,
            }))
        }
    }
}

/// Outcome of a run that rewrites no target: normal form, or the accumulated faults when a target
/// cannot be normalized. Neither case writes or appends.
fn no_change_outcome(eval: &EvaluatedTree) -> Outcome {
    if eval.errors.is_empty() {
        Outcome {
            stdout: format!("fmt: {} files in normal form\n", eval.total),
            stderr: String::new(),
            code: EXIT_OK,
        }
    } else {
        Outcome {
            stdout: String::new(),
            stderr: eval.errors.clone(),
            code: EXIT_FINDINGS,
        }
    }
}

fn formatted_lines<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for path in paths {
        out.push_str("formatted ");
        out.push_str(path);
        out.push('\n');
    }
    out
}

#[must_use]
pub(crate) fn run(args: &FmtArgs, world: &dyn World) -> Outcome {
    if args.check {
        let tree = match world.read_tree() {
            Ok(tree) => tree,
            Err(err) => return io_err(err),
        };
        let eval = evaluate_tree(&tree);
        if eval.changed.is_empty() && eval.errors.is_empty() {
            return Outcome {
                stdout: format!("fmt: {} files in normal form\n", eval.total),
                stderr: String::new(),
                code: EXIT_OK,
            };
        }
        let mut stdout = String::new();
        for file in &eval.changed {
            stdout.push_str("would format ");
            stdout.push_str(&file.path);
            stdout.push('\n');
        }
        return Outcome {
            stdout,
            stderr: eval.errors,
            code: EXIT_FINDINGS,
        };
    }

    let (key, req_sha) = match operation_identity() {
        Ok(pair) => pair,
        Err(out) => return out,
    };

    let lease = match world.write_lease() {
        Ok(lease) => lease,
        Err(err) => return io_err(err),
    };
    let tree = match world.read_tree_under(&lease) {
        Ok(tree) => tree,
        Err(err) => return io_err(err),
    };
    let eval = evaluate_tree(&tree);

    // A run that writes nothing must also append nothing, and opening the journal appends
    // `journal.open`. The lease already carries the generation state, so open the journal only
    // when this run can write: a pending write to finish, or ordinary changes to record.
    if !lease.is_pending() && eval.changed.is_empty() {
        return no_change_outcome(&eval);
    }

    let mut journal = match crate::io::state::open(world, world.state_dir(), Via::Cli) {
        Ok(journal) => journal,
        Err(err) => return journal_err(&err),
    };

    // Recover matching pending `fmt` intent before collecting ordinary changes or returning no-change.
    match journal.pending_intent(&lease, &key, req_sha.clone()) {
        Ok(Some(pending)) => {
            match try_recover_pending(world, &mut journal, &lease, &tree, &key, &req_sha, &pending)
            {
                Ok(None) => {}
                // A finished recovery still owes the faults this run already collected from
                // targets outside the pending write set: report them and exit with findings.
                Ok(Some(mut outcome)) => {
                    if !eval.errors.is_empty() {
                        outcome.stderr.push_str(&eval.errors);
                        outcome.code = EXIT_FINDINGS;
                    }
                    return outcome;
                }
                Err(outcome) => return outcome,
            }
        }
        Ok(None) => {}
        Err(err) => return journal_err(&err),
    }

    if eval.changed.is_empty() {
        return no_change_outcome(&eval);
    }

    let files: Vec<WriteFile> = eval
        .changed
        .iter()
        .map(|c| WriteFile::new(c.path.as_str(), c.after.as_str()))
        .collect();
    let expected: Vec<(&str, Option<Sha256>)> = eval
        .changed
        .iter()
        .map(|c| (c.path.as_str(), Some(c.seen.clone())))
        .collect();
    let writes = WriteSet {
        labels: LabelSet::public(),
        files,
        affected: Vec::new(),
    };
    let intent = OperationIntent {
        schema: 1,
        key,
        request_sha256: req_sha,
        allocated: Vec::new(),
        allocated_store_uid: None,
        planned_on: world.today(),
    };

    let audited = match journal.begin_expecting(&lease, &intent, &writes, &expected) {
        Ok(audited) => audited,
        Err(err) => return journal_err(&err),
    };
    for file in &eval.changed {
        if let Err(err) =
            world.write_file(&audited, &world.tree_root().join(&file.path), &file.bytes)
        {
            return io_err(err);
        }
    }
    if let Err(err) = journal.end(audited, Word::new_static("ok")) {
        return journal_err(&err);
    }

    let stdout = formatted_lines(eval.changed.iter().map(|c| c.path.as_str()));
    let code = if eval.errors.is_empty() {
        EXIT_OK
    } else {
        EXIT_FINDINGS
    };
    Outcome {
        stdout,
        stderr: eval.errors,
        code,
    }
}

fn usage_err(err: impl std::fmt::Display) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_USAGE,
    }
}

fn journal_err(err: &JournalError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: err.exit_code(),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Result::map_err passes the owned WorldError"
)]
fn io_err(err: WorldError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_IO,
    }
}

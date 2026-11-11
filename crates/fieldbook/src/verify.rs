//! `fieldbook verify`: record that the owner (or lead) verified a claim, at a terminal.
//!
//! **Responsibility.** Advances the verification of one claim in `claims.csv` to
//! `primary_source_supported`, stamping the specified date and verifying role.
//!
//! **Fixed order.**
//! A. Refuse immediately if standard input is not a terminal, before reading anything and
//!    before opening the journal, so non-interactive runs write nothing.
//! B. Open the journal and read the caller's role; refuse `Role::Agent` before reading or
//!    showing any claim.
//! C. Parse the claim identifier, construct the store record reference, operation key, and
//!    request hash.
//! D. Under a short exclusive write lease, inspect pending writes in the state directory.
//!    If an identical verify operation was already written to disk (after-state), finish
//!    recovery immediately without re-verifying and without prompting. The write lease is
//!    explicitly dropped at the end of this step: no lease and no journal lock is held while
//!    the person is prompted.
//! E. Plan the verification under the loaded model, evaluating eligibility and promotion rules.
//! F. Prompt the person at the terminal with the claim summary and confirmation question,
//!    requiring an interactive `yes` answer.
//! G. After confirmation, acquire a write lease, re-read the tree and role, verify the claim
//!    evidence is unchanged, and construct the fresh write set and intent.
//! H. Commit the verification: recover an interrupted run if retrying before-state, or begin
//!    and write `claims.csv`, followed by `write.end`.
//! I. Print the one-line confirmation to standard output.

use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    Id, OperationIntent, OperationKey, RecordRef, Role, Sha256, StoreId, Verb, Via, Word,
    WriteFile, WriteSet, normalize_affected, request_sha256,
};
use fieldbook_core::check::{self, Host, Model};
use fieldbook_core::labels::LabelSet;
use fieldbook_core::verify::{Refusal, STATUS, VerifyPlan, role_message, terminal_message};

use crate::cli::{EXIT_FINDINGS, EXIT_IO, EXIT_OK, EXIT_USAGE, VerifyArgs};
use crate::io::{Journal, JournalError, Recovery};
use crate::{Outcome, World, WorldError};

/// The path of the claims register file within the Corpus.
const CLAIMS_PATH: &str = "corpus/registers/claims.csv";

/// Run `fieldbook verify`: interactive terminal verification of a claim.
#[must_use]
pub fn run(args: &VerifyArgs, world: &dyn World) -> Outcome {
    // A. Refuse non-terminal standard input before touching state or journal.
    if !world.stdin_is_terminal() {
        return Outcome {
            stdout: String::new(),
            stderr: format!("{}\n", terminal_message()),
            code: EXIT_USAGE,
        };
    }

    // B. Open the journal and read the role; refuse Role::Agent before reading Corpus.
    let mut journal = match crate::io::state::open(world, world.state_dir(), Via::Cli) {
        Ok(j) => j,
        Err(err) => return journal_err(&err),
    };
    let (role, _) = match crate::io::state::read_role_and_baseline(world.state_dir()) {
        Ok(rb) => rb,
        Err(err) => return journal_err(&err),
    };
    if role == Role::Agent {
        return Outcome {
            stdout: String::new(),
            stderr: format!("{}\n", role_message()),
            code: EXIT_USAGE,
        };
    }

    // C. The claim reference, operation key and request hash.
    let Ok(id) = Id::parse(&args.claim) else {
        return findings_err(Refusal::UnknownClaim(args.claim.clone()));
    };
    let claim_ref = RecordRef::new(StoreId::Corpus, id.clone());
    let key = match operation_key(&claim_ref) {
        Ok(k) => k,
        Err(out) => return out,
    };

    // D. Pending inspection BEFORE any eligibility or already-verified refusal.
    let model = match inspect_pending(world, &mut journal, args, role, &id, &claim_ref, &key) {
        Ok(m) => m,
        Err(outcome) => return outcome,
    };

    // E. Plan verification under the loaded model.
    let plan = match fieldbook_core::verify::plan(&model, &args.claim, args.on, role) {
        Ok(p) => p,
        Err(refusal) => return findings_err(refusal),
    };

    // F. Interactive prompt at the terminal without holding any lease or journal lock.
    if let Err(err) = world.prompt(&format!(
        "{}{}",
        plan.shown.summary(),
        plan.shown.question()
    )) {
        return io_err(err);
    }
    let answer = world.read_line();
    if answer.trim() != "yes" {
        return Outcome {
            stdout: String::new(),
            stderr: "not verified; nothing was written\n".to_owned(),
            code: EXIT_FINDINGS,
        };
    }

    // G & H. Commit the verification under a fresh lease.
    if let Err(out) = commit_verification(world, &mut journal, args, role, &claim_ref, &key, &plan)
    {
        return out;
    }

    // I. Success line.
    success_outcome(&id, args.on, role)
}

/// The operation key for verifying a claim.
fn operation_key(claim_ref: &RecordRef) -> Result<OperationKey, Outcome> {
    let claim_str = claim_ref.to_string();
    let subject = request_sha256(&[
        ("verb", b"verify"),
        ("store", StoreId::Corpus.as_str().as_bytes()),
        ("claim", claim_str.as_bytes()),
    ])
    .map_err(|e| usage_err(e.to_string()))?;
    let verb = Verb::parse("verify").map_err(|e| usage_err(e.to_string()))?;
    Ok(OperationKey::new(verb, subject))
}

/// Normalized request hash identifying this verification request.
fn request_hash(
    claim_ref: &RecordRef,
    role: Role,
    on: CivilDate,
    evidence: &Sha256,
    confirmation: &Sha256,
) -> Result<Sha256, Outcome> {
    let claim_str = claim_ref.to_string();
    let on_str = on.to_string();
    let entries: [(&str, &[u8]); 8] = [
        ("verb", b"verify"),
        ("store", b"corpus"),
        ("claim", claim_str.as_bytes()),
        ("status", STATUS.as_bytes()),
        ("role", role.as_str().as_bytes()),
        ("on", on_str.as_bytes()),
        ("evidence_sha256", evidence.as_str().as_bytes()),
        ("confirmation_sha256", confirmation.as_str().as_bytes()),
    ];
    request_sha256(&entries).map_err(|e| usage_err(e.to_string()))
}

/// Build the [`WriteSet`] containing the updated `claims.csv` bytes.
fn make_writes(path: &str, bytes: &[u8], claim_ref: &RecordRef) -> WriteSet {
    WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            path.to_owned(),
            Sha256::digest(bytes).as_str(),
        )],
        affected: normalize_affected(std::slice::from_ref(claim_ref)),
    }
}

/// Step D: Inspect pending writes before planning or prompting.
///
/// `Ok` carries the model to plan under. `Err` carries the finished outcome: a refusal, or
/// the success of a completed recovery, in which case nothing more is to be done.
fn inspect_pending(
    world: &dyn World,
    journal: &mut Journal<'_>,
    args: &VerifyArgs,
    role: Role,
    id: &Id,
    claim_ref: &RecordRef,
    key: &OperationKey,
) -> Result<Model, Outcome> {
    let lease = world.write_lease().map_err(io_err)?;
    let tree = world.read_tree_under(&lease).map_err(io_err)?;
    let (model, _) = check::load(
        &Host::new(tree.clone(), args.on),
        &check::Options::new(false),
    );

    let shown =
        fieldbook_core::verify::shown(&model, &args.claim, args.on).map_err(findings_err)?;
    let evidence =
        fieldbook_core::verify::evidence_sha256(&model, &args.claim).map_err(findings_err)?;
    let request = request_hash(
        claim_ref,
        role,
        args.on,
        &evidence,
        &shown.confirmation_sha256(),
    )?;

    let pending = journal.pending_key(&lease).map_err(|e| journal_err(&e))?;
    match pending {
        None => {}
        Some(found) if &found.key != key => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: "another write is pending in the state folder; finish it with its own command first\n".to_owned(),
                code: EXIT_FINDINGS,
            });
        }
        Some(found) if found.request_sha256 != request => {
            return Err(Outcome {
                stdout: String::new(),
                stderr: "an interrupted verify of this claim is pending with a different request (date, role or evidence changed); run the same command as before\n".to_owned(),
                code: EXIT_FINDINGS,
            });
        }
        Some(_) => {
            if let Some(op) = journal
                .pending_intent(&lease, key, request.clone())
                .map_err(|e| journal_err(&e))?
            {
                let is_after_state = !op.writes.files.is_empty()
                    && op.writes.files.iter().all(|file| {
                        tree.get_bytes(&file.path)
                            .is_some_and(|b| Sha256::digest(b).as_str() == file.sha256.as_str())
                    });
                if is_after_state {
                    let bytes =
                        fieldbook_core::verify::after_bytes(&model, &args.claim, args.on, role)
                            .map_err(findings_err)?;
                    let writes = make_writes(CLAIMS_PATH, &bytes, claim_ref);
                    let recovery = journal
                        .recover(&lease, key, request.clone(), &writes)
                        .map_err(|e| journal_err(&e))?;
                    match recovery {
                        Recovery::Completed { .. } => {
                            return Err(success_outcome(id, args.on, role));
                        }
                        Recovery::Resume { audited, .. } => {
                            world
                                .write_file(&audited, &world.tree_root().join(CLAIMS_PATH), &bytes)
                                .map_err(io_err)?;
                            journal
                                .end(audited, Word::new_static("ok"))
                                .map_err(|e| journal_err(&e))?;
                            return Err(success_outcome(id, args.on, role));
                        }
                        Recovery::None => {}
                    }
                }
            }
        }
    }

    drop(lease);
    Ok(model)
}

/// Steps G & H: Acquire fresh lease, re-verify evidence, and commit changes.
fn commit_verification(
    world: &dyn World,
    journal: &mut Journal<'_>,
    args: &VerifyArgs,
    role: Role,
    claim_ref: &RecordRef,
    key: &OperationKey,
    plan: &VerifyPlan,
) -> Result<(), Outcome> {
    let lease = world.write_lease().map_err(io_err)?;
    let fresh_tree = world.read_tree_under(&lease).map_err(io_err)?;
    let (fresh_role, _) =
        crate::io::state::read_role_and_baseline(world.state_dir()).map_err(|e| journal_err(&e))?;
    let (fresh_model, _) = check::load(
        &Host::new(fresh_tree.clone(), args.on),
        &check::Options::new(false),
    );

    if fresh_role != role {
        return Err(Outcome {
            stdout: String::new(),
            stderr: "claim changed; run verify again\n".to_owned(),
            code: EXIT_FINDINGS,
        });
    }

    let Ok(fresh_plan) =
        fieldbook_core::verify::plan(&fresh_model, &args.claim, args.on, fresh_role)
    else {
        return Err(Outcome {
            stdout: String::new(),
            stderr: "claim changed; run verify again\n".to_owned(),
            code: EXIT_FINDINGS,
        });
    };
    if fresh_plan.evidence_sha256 != plan.evidence_sha256 {
        return Err(Outcome {
            stdout: String::new(),
            stderr: "claim changed; run verify again\n".to_owned(),
            code: EXIT_FINDINGS,
        });
    }

    let fresh_request = request_hash(
        claim_ref,
        fresh_role,
        args.on,
        &fresh_plan.evidence_sha256,
        &fresh_plan.shown.confirmation_sha256(),
    )?;

    let writes = make_writes(&fresh_plan.path, &fresh_plan.bytes, claim_ref);

    let intent = OperationIntent {
        schema: 1,
        key: key.clone(),
        request_sha256: fresh_request.clone(),
        allocated: Vec::new(),
        allocated_store_uid: None,
        planned_on: args.on,
    };

    let target = world.tree_root().join(&fresh_plan.path);
    let pending = journal.pending_key(&lease).map_err(|e| journal_err(&e))?;
    let is_matching_pending = pending
        .as_ref()
        .is_some_and(|found| found.key == *key && found.request_sha256 == fresh_request);

    let mut need_begin = true;
    if is_matching_pending {
        let recovery = journal
            .recover(&lease, key, fresh_request.clone(), &writes)
            .map_err(|e| journal_err(&e))?;
        match recovery {
            Recovery::Completed { .. } => {
                need_begin = false;
            }
            Recovery::Resume { audited, .. } => {
                world
                    .write_file(&audited, &target, &fresh_plan.bytes)
                    .map_err(io_err)?;
                journal
                    .end(audited, Word::new_static("ok"))
                    .map_err(|e| journal_err(&e))?;
                need_begin = false;
            }
            Recovery::None => {}
        }
    }

    if need_begin {
        let Some(live_bytes) = fresh_tree.get_bytes(&fresh_plan.path) else {
            return Err(findings_err(Refusal::MissingRegister));
        };
        let audited = journal
            .begin_expecting(
                &lease,
                &intent,
                &writes,
                &[(fresh_plan.path.as_str(), Some(Sha256::digest(live_bytes)))],
            )
            .map_err(|e| journal_err(&e))?;
        world
            .write_file(&audited, &target, &fresh_plan.bytes)
            .map_err(io_err)?;
        journal
            .end(audited, Word::new_static("ok"))
            .map_err(|e| journal_err(&e))?;
    }

    Ok(())
}

/// Success outcome with the standard single-line confirmation.
fn success_outcome(id: &Id, on: CivilDate, role: Role) -> Outcome {
    Outcome {
        stdout: format!("verified {id} on {on} ({})\n", role.as_str()),
        stderr: String::new(),
        code: EXIT_OK,
    }
}

/// Format an error finding for standard error with exit code 1.
fn findings_err(err: impl std::fmt::Display) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_FINDINGS,
    }
}

/// Format a usage error for standard error with exit code 2.
fn usage_err(err: impl std::fmt::Display) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: EXIT_USAGE,
    }
}

/// Map a [`JournalError`] to its exit code and message.
fn journal_err(err: &JournalError) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{err}\n"),
        code: err.exit_code(),
    }
}

/// Map an environmental [`WorldError`] to exit code 3.
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

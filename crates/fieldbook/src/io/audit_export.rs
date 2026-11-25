//! Audit journal export to the policy's sink (`audit-export`, ADR 0004 §12).
//!
//! The export writes the journal's own lines and a manifest to a folder outside the data
//! root, through the `&Allowed` door for the fixed `audit` destination. It is text-free: the
//! lines carry typed ids, hashes, counts and words, so the request is labelled `public`.

use std::fmt;
use std::path::{Path, PathBuf};

use fieldbook_core::audit::export::{ExportError, plan_export};
use fieldbook_core::audit::{
    Count, DestKind, EventPayload, Kind, LabelSet, RefuseAbout, Sha256, StoreUid, Word,
};
use fieldbook_core::check::Code;
use fieldbook_core::egress::{Decision, Delivery, Dest, Reason, Transfer, check, shipped};
use fieldbook_core::labels::{self, PlaceRole, Placement};

use crate::World;
use crate::io::journal::{Journal, JournalError};
use crate::policy::PolicyCtx;

/// The data-root folders an export target must stay outside of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The Corpus root.
    pub tree_root: PathBuf,
    /// The state folder.
    pub state_dir: PathBuf,
}

impl Layout {
    /// Build the layout from the world's roots.
    #[must_use]
    pub fn of(world: &dyn World) -> Self {
        Self {
            tree_root: world.tree_root().to_path_buf(),
            state_dir: world.state_dir().to_path_buf(),
        }
    }
}

/// A refusal raised by `export`. It carries the journal's closed reason word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Diagnostic code (`FB0806`).
    pub code: Code,
    /// Journal `refuse` reason word (`no-row`, `store`, or `mismatch`).
    pub reason: &'static str,
    /// Plain explanation shown after `error[FB0806]:`.
    pub message: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.id(), self.message)
    }
}

impl std::error::Error for Refusal {}

/// What one successful `audit export` wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    /// `false` when the journal held nothing at or after `--since`.
    pub written: bool,
    /// Number of exported journal lines.
    pub events: usize,
    /// First exported sequence number.
    pub first_seq: u64,
    /// Last exported sequence number.
    pub last_seq: u64,
    /// Chain hash of the last exported line.
    pub head: Sha256,
    /// File name of the exported journal segment.
    pub file: String,
    /// File name of the manifest.
    pub manifest: String,
    /// The sink folder the files were written to.
    pub path: PathBuf,
    /// The journal's last sequence number when nothing was new.
    pub journal_end: u64,
}

const NO_SINK: &str =
    "the policy has no [audit] sink; IT names the folder in the policy file (IT-35)";

fn refusal(journal: &Journal<'_>, reason: &'static str, message: &str) -> Refusal {
    let word = Word::from_closed(reason, fieldbook_core::audit::kind::REFUSE_REASONS)
        .unwrap_or_else(|_| Word::new_static("store"));
    let _ = journal.refuse(
        Some("audit export"),
        Code::Fb0806,
        word,
        &RefuseAbout {
            dest: Some(DestKind::Audit),
            labels: Some(LabelSet::public()),
            ..RefuseAbout::default()
        },
    );
    Refusal {
        code: Code::Fb0806,
        reason,
        message: message.to_owned(),
    }
}

fn canonical(path: &Path) -> Option<PathBuf> {
    crate::io::root::resolve_outside_path(path).ok()
}

/// Whether one folder is the other or lies below it, in either direction. Both are resolved by
/// the same resolver as the target, so a Windows verbatim prefix cannot make them differ.
fn overlaps(a: &Path, b: &Path) -> bool {
    let a = canonical(a).unwrap_or_else(|| a.to_path_buf());
    let b = canonical(b).unwrap_or_else(|| b.to_path_buf());
    a.starts_with(&b) || b.starts_with(&a)
}

fn journal_refusal(journal: &Journal<'_>, err: &JournalError) -> Refusal {
    refusal(
        journal,
        "store",
        &format!("the journal cannot be read: {err}"),
    )
}

/// Export the journal's lines and a manifest to the policy's sink.
///
/// `to` must lie at or below the sink, outside the data root, and must be an existing folder.
/// The default `since` is one after the last `audit.export` event's `to_seq`, or 1.
///
/// # Errors
///
/// Returns [`Refusal`] (`FB0806`) when the policy names no sink, the target is unusable, or a
/// file already exists at the planned name with other bytes.
pub fn export(
    world: &dyn World,
    layout: &Layout,
    policy: &PolicyCtx,
    journal: &mut Journal<'_>,
    to: Option<&Path>,
    since: Option<u64>,
) -> Result<ExportReport, Refusal> {
    let now = world.now();
    let Some(sink) = policy.audit_sink() else {
        return Err(refusal(journal, "no-row", NO_SINK));
    };
    let Some(sink_canon) = canonical(&sink) else {
        return Err(refusal(
            journal,
            "store",
            "the audit sink folder cannot be resolved",
        ));
    };
    if !sink_canon.is_dir() {
        return Err(refusal(journal, "store", "the audit sink is not a folder"));
    }

    let target = match to {
        Some(path) => path.to_path_buf(),
        None => sink.clone(),
    };
    let Some(target_canon) = canonical(&target) else {
        return Err(refusal(
            journal,
            "store",
            "the export target folder cannot be resolved",
        ));
    };
    if !target_canon.is_dir() {
        return Err(refusal(
            journal,
            "store",
            "the export target is not a folder",
        ));
    }
    if !target_canon.starts_with(&sink_canon) {
        return Err(refusal(
            journal,
            "no-row",
            "the export target lies outside the policy's audit sink",
        ));
    }
    // The target may be neither inside the data root nor an ancestor of it (a sink that holds the
    // data root would otherwise export into the data it describes).
    if overlaps(&target_canon, &layout.tree_root) || overlaps(&target_canon, &layout.state_dir) {
        return Err(refusal(
            journal,
            "store",
            "the export target lies inside, or holds, the data root",
        ));
    }
    let git = crate::io::root::git_context(world, &target_canon)
        .map_err(|_| refusal(journal, "store", "the export target cannot be inspected"))?;
    if labels::placement(Placement {
        laptop: false,
        under_git: git.under_git,
        role: PlaceRole::ExportTarget,
        facts: world.place(&target_canon),
        declared: false,
    })
    .is_err()
    {
        return Err(refusal(
            journal,
            "store",
            "the export target is a Git work tree, a sync root, or a reparse point",
        ));
    }

    let lines = journal
        .lines()
        .map_err(|err| journal_refusal(journal, &err))?;
    let Some(first_line) = lines.first() else {
        return Ok(ExportReport {
            written: false,
            events: 0,
            first_seq: 0,
            last_seq: 0,
            head: Sha256::zero(),
            file: String::new(),
            manifest: String::new(),
            path: target_canon,
            journal_end: 0,
        });
    };
    let chain_id = first_line.hash.clone();
    let store_uid = store_uid_of(journal);
    let effective_since = since.unwrap_or_else(|| {
        journal
            .last_export_to_seq()
            .map_or(1, |seq| seq.saturating_add(1))
    });

    let planned = match plan_export(
        &lines,
        chain_id,
        journal.writer(),
        &store_uid,
        effective_since,
        policy.hash.clone(),
        env!("CARGO_PKG_VERSION"),
    ) {
        Ok(planned) => planned,
        Err(ExportError::NothingNew { last_seq }) => {
            return Ok(ExportReport {
                written: false,
                events: 0,
                first_seq: 0,
                last_seq: 0,
                head: Sha256::zero(),
                file: String::new(),
                manifest: String::new(),
                path: target_canon,
                journal_end: last_seq,
            });
        }
        Err(ExportError::OutOfRange { .. }) => {
            return Err(refusal(journal, "no-row", "--since must be at least 1"));
        }
    };

    let manifest_text = planned.manifest_text();
    let journal_path = target_canon.join(&planned.file_name);
    let manifest_path = target_canon.join(&planned.manifest_name);

    for (path, expected) in [
        (&journal_path, planned.bytes.as_slice()),
        (&manifest_path, manifest_text.as_bytes()),
    ] {
        match world.read_file(path) {
            Ok(Some(existing)) if !existing.is_empty() && existing != expected => {
                return Err(refusal(
                    journal,
                    "mismatch",
                    "a file at the export target already holds other bytes",
                ));
            }
            Ok(_) => {}
            Err(err) => {
                return Err(refusal(
                    journal,
                    "store",
                    &format!("the export target cannot be read: {err}"),
                ));
            }
        }
    }

    let effective = policy.effective_policy(world);
    let age = shipped(journal.last_time_of(Kind::AuditExport), now);
    let public = LabelSet::public();
    if check(&effective, &Dest::Audit, Some(&public), None, now, age).is_denied() {
        return Err(refusal(
            journal,
            "label",
            "the audit destination refused the export",
        ));
    }
    let delivery = Delivery::new(
        vec![
            Transfer::write_path(&journal_path, &planned.bytes),
            Transfer::write_path(&manifest_path, manifest_text.as_bytes()),
        ],
        None,
    );
    let allowed = match check(
        &effective,
        &Dest::Audit,
        Some(&public),
        Some(&delivery),
        now,
        age,
    ) {
        Decision::Allowed(allowed) => allowed,
        Decision::Denied(denial) => {
            let reason = match denial.reason {
                Reason::NoRow | Reason::Expired => "no-row",
                Reason::Mismatch => "mismatch",
                _ => "label",
            };
            return Err(refusal(
                journal,
                reason,
                "the audit destination refused the export",
            ));
        }
    };

    let write_set = fieldbook_core::audit::WriteSet {
        labels: public.clone(),
        files: vec![
            fieldbook_core::audit::WriteFile::new(
                planned.file_name.clone(),
                Sha256::digest(&planned.bytes).as_str().to_owned(),
            ),
            fieldbook_core::audit::WriteFile::new(
                planned.manifest_name.clone(),
                Sha256::digest(manifest_text.as_bytes()).as_str().to_owned(),
            ),
        ],
        affected: Vec::new(),
    };
    let audited = journal
        .begin_export("audit export", &write_set, &allowed, &target_canon)
        .map_err(|err| refusal(journal, "store", &format!("the export cannot begin: {err}")))?;

    for (path, bytes) in [
        (&journal_path, planned.bytes.as_slice()),
        (&manifest_path, manifest_text.as_bytes()),
    ] {
        match crate::io::write_export(world, &allowed, &audited, path, bytes) {
            Ok(_) => {}
            Err(crate::io::WriteExportError::Conflict) => {
                let _ = journal.end(audited, Word::new_static("failed"));
                return Err(refusal(
                    journal,
                    "mismatch",
                    "a file at the export target already holds other bytes",
                ));
            }
            Err(crate::io::WriteExportError::Io(err)) => {
                let _ = journal.end(audited, Word::new_static("failed"));
                return Err(refusal(
                    journal,
                    "store",
                    &format!("the export cannot be written: {err}"),
                ));
            }
        }
    }
    journal
        .end(audited, Word::new_static("ok"))
        .map_err(|err| refusal(journal, "store", &format!("the export cannot end: {err}")))?;

    let store_uid_typed = StoreUid::parse(&store_uid).unwrap_or_else(|_| StoreUid::nil());
    journal
        .append_payload(EventPayload::AuditExport {
            from_seq: Count::new(planned.first_seq),
            to_seq: Count::new(planned.last_seq),
            head: planned.head.clone(),
            policy: policy.hash.clone(),
            store_uid: store_uid_typed,
        })
        .map_err(|err| {
            refusal(
                journal,
                "store",
                &format!("the export cannot be recorded: {err}"),
            )
        })?;

    Ok(ExportReport {
        written: true,
        events: planned.events(),
        first_seq: planned.first_seq,
        last_seq: planned.last_seq,
        head: planned.head.clone(),
        file: planned.file_name.clone(),
        manifest: planned.manifest_name.clone(),
        path: target_canon,
        journal_end: planned.last_seq,
    })
}

/// The first entry of this run's `journal.open` `stores` list, or the nil UUID.
#[must_use]
pub fn store_uid_of(journal: &Journal<'_>) -> String {
    journal.stores().first().map_or_else(
        || fieldbook_core::audit::NIL_UUID.to_owned(),
        |uid| uid.as_str().to_owned(),
    )
}

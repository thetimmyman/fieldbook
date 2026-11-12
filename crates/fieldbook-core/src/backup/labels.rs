//! Provenance labels for backup snapshots.
//!
//! **Responsibility.** Represent copied files with their kinds and effective labels,
//! and aggregate snapshot labels by exclusion.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

use std::collections::BTreeSet;

use crate::labels::{LabelSet, UNLABELLED};

/// Classification of a file copied into a backup snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackupFileKind {
    /// Validated pending write file.
    PendingWrite,
    /// Corpus or overlay page.
    Page,
    /// Source version metadata/content.
    SourceVersion,
    /// Business or company record.
    Record,
    /// Original file under sources.
    Original,
    /// Extracted text file.
    ExtractionText,
    /// Queue proposal.
    QueueProposal,
    /// Question log line.
    QuestionLogLine,
    /// Call record.
    CallRecord,
    /// Handoff pack.
    HandoffPack,
    /// Label ledger.
    LabelLedger,
    /// Validated known text-free control file (journal, locks, sync cursors, backup metadata).
    TextFreeControl,
    /// Text-bearing public Corpus support file.
    CorpusSupport,
    /// Text-bearing last backup cache.
    LastBackupCache,
    /// Text-bearing last add report.
    LastAddReport,
    /// Text-bearing settings file.
    Settings,
    /// Unclassified or stray file.
    Stray,
}

/// A file scheduled for inclusion in a backup snapshot, with its kind and resolved labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedFile {
    /// Relative path within the store layout.
    pub path: String,
    /// Classified kind of the file.
    pub kind: BackupFileKind,
    /// Resolved effective labels for this file.
    pub labels: LabelSet,
}

impl CopiedFile {
    /// Creates a new [`CopiedFile`].
    #[must_use]
    pub fn new(path: impl Into<String>, kind: BackupFileKind, labels: LabelSet) -> Self {
        Self {
            path: path.into(),
            kind,
            labels,
        }
    }
}

/// Computes the aggregated [`LabelSet`] for a collection of snapshot files by exclusion.
///
/// Rules:
/// - An empty selection or a selection containing only [`BackupFileKind::TextFreeControl`]
///   yields [`LabelSet::public()`].
/// - [`BackupFileKind::TextFreeControl`] files contribute no labels to a mixed selection.
/// - [`BackupFileKind::Stray`] files add `unlabelled`.
/// - Known files that should declare labels and do not (or have `unlabelled`) add `unlabelled`.
/// - Otherwise, all file labels are unioned.
#[must_use]
pub fn snapshot_labels(files: &[CopiedFile]) -> LabelSet {
    if files.is_empty() {
        return LabelSet::public();
    }

    let all_controls = files
        .iter()
        .all(|f| f.kind == BackupFileKind::TextFreeControl);
    if all_controls {
        return LabelSet::public();
    }

    let mut ids = BTreeSet::new();

    for file in files {
        match file.kind {
            BackupFileKind::TextFreeControl => {
                // Text-free control files add nothing to the union
            }
            BackupFileKind::Stray => {
                ids.insert(UNLABELLED.clone());
            }
            _ => {
                if file.labels.contains(&UNLABELLED) || file.labels.ids().is_empty() {
                    ids.insert(UNLABELLED.clone());
                } else {
                    for id in file.labels.ids() {
                        ids.insert(id.clone());
                    }
                }
            }
        }
    }

    if ids.is_empty() {
        LabelSet::unlabelled()
    } else {
        LabelSet::from_ids(ids)
    }
}

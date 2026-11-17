//! Provenance labels for backup snapshots.
//!
//! **Responsibility.** Represent copied files with their kinds and effective labels,
//! and aggregate snapshot labels by exclusion.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

use std::collections::BTreeSet;

use crate::labels::{LabelGraph, LabelSet, MarkingRules, UNLABELLED, markings_in};

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
/// - A file with an empty label set adds `unlabelled`.
/// - Otherwise every id of a file's resolved set is unioned, `unlabelled` and `marked` included.
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
                if file.labels.ids().is_empty() {
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

/// The labels a backup copy of the question log (`state/asked/asked.csv`) carries: for every
/// row, its own labels, any marking `rules` find in its question, and the current effective
/// labels of its inputs in `graph`. A log that does not parse, or a row with inputs and no
/// `graph` to resolve them in, adds `unlabelled`; a log with no rows is `owner-supplied`.
#[must_use]
pub fn question_log_labels(
    bytes: &[u8],
    graph: Option<&dyn LabelGraph>,
    rules: &MarkingRules,
) -> LabelSet {
    let Ok(rows) = crate::asked::parse(bytes) else {
        return LabelSet::unlabelled();
    };
    let mut labels = LabelSet::owner_supplied();
    for row in &rows {
        labels = labels.union(&row.labels);
        if let Some(found) = markings_in(&row.question, rules) {
            labels = labels.union(&found);
        }
        match graph {
            Some(graph) => labels = crate::asked::resolve(&labels, &row.inputs, graph),
            None if !row.inputs.is_empty() => labels = labels.union(&LabelSet::unlabelled()),
            None => {}
        }
    }
    labels
}

//! Backup domain types, plans, traits, labels, and retention rules.
//!
//! **Responsibility.** Pure planning of backup operations, parsing and serialization of
//! `backup.toml`, trait definitions for backup targets and staging, provenance label aggregation,
//! and retention planning without a clock or environment.
//!
//! **Rules.** Pure: no I/O, no clock, no environment, no platform-specific calls.

use std::fmt;

pub mod labels;
pub mod plan;
pub mod restore;
pub mod retention;
pub mod target;
pub mod toml;

pub use labels::{BackupFileKind, CopiedFile, snapshot_labels};
pub use plan::{BackupPlan, PlaceFinding, path_budget, plan_backup, target_place};
pub use restore::{
    DRILL_STEPS, DestinationFacts, PlannedRoot, RestorePlan, RestoreRefusal, RestoreRefusalKind,
    RestoreShell, plan_restore, rename_help,
};
pub use retention::{SnapshotInfo, plan_retention};
pub use target::{BackupTarget, SnapshotStage};
pub use toml::{BackupToml, manifest_sha256};

/// Template configuration snippet to add to `settings.toml` when no target is configured.
pub const BACKUP_TARGET_TEMPLATE: &str = "[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"<a folder outside the Corpus and the state folder>\"\n";

/// Identifier number of a snapshot (1-based integer).
pub type Number = u32;

/// Extracts `(corpus/<path>, sha256)` pairs from lines in `SHA256SUMS` text that refer to `data/corpus/`.
#[must_use]
pub fn corpus_entries(sums: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for line in sums.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((sha, rest)) = trimmed.split_once(' ') else {
            continue;
        };
        let sha = sha.trim();
        let path = rest.trim_start().trim_start_matches('*');
        if let Some(corpus_subpath) = path.strip_prefix("data/corpus/") {
            entries.push((format!("corpus/{corpus_subpath}"), sha.to_owned()));
        } else if let Some(corpus_subpath) = path.strip_prefix("corpus/") {
            entries.push((format!("corpus/{corpus_subpath}"), sha.to_owned()));
        }
    }
    entries
}

/// Errors arising during backup planning, parsing, or staging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupError {
    /// TOML parsing or formatting error.
    Toml(String),
    /// An unknown key was encountered in `backup.toml`.
    UnknownKey(String),
    /// Invalid `backup_version` (expected `1`).
    InvalidVersion(u32),
    /// A value in `backup.toml` is invalid.
    InvalidValue {
        /// Name of the key with the invalid value.
        key: &'static str,
        /// Explanation of why the value is invalid.
        reason: String,
    },
    /// A list field in manifest identity is not sorted or contains duplicates.
    UnsortedList {
        /// Name of the field.
        field: &'static str,
        /// Offending member value.
        member: String,
    },
    /// Checksum text is noncanonical.
    NonCanonicalSums(String),
    /// The target path exceeds the 260-character budget.
    PathBudgetExceeded {
        /// Length of the longest staged path.
        staged_len: usize,
        /// Maximum allowed path length (`260`).
        max: usize,
        /// Target path configured.
        target: String,
        /// Shortest working root.
        shortest_root: String,
        /// Maximum allowed target root length.
        max_root_len: usize,
    },
    /// Snapshot number is already occupied in target.
    OccupiedSnapshot(Number),
    /// Target or staging operation failed.
    Target(String),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(msg) => write!(f, "backup.toml error: {msg}"),
            Self::UnknownKey(key) => write!(f, "unknown key `{key}` in backup.toml"),
            Self::InvalidVersion(v) => write!(f, "unsupported backup_version `{v}`, expected 1"),
            Self::InvalidValue { key, reason } => write!(f, "invalid value for `{key}`: {reason}"),
            Self::UnsortedList { field, member } => {
                write!(
                    f,
                    "list `{field}` is not strictly sorted: duplicate or out of order at `{member}`"
                )
            }
            Self::NonCanonicalSums(reason) => write!(f, "noncanonical checksum text: {reason}"),
            Self::PathBudgetExceeded {
                staged_len,
                max,
                target,
                shortest_root,
                max_root_len,
            } => {
                write!(
                    f,
                    "target `{target}` with longest staged path length {staged_len} exceeds {max}-character budget; shortest working root is `{shortest_root}` (max root length {max_root_len})"
                )
            }
            Self::OccupiedSnapshot(n) => write!(f, "snapshot {n} is already occupied"),
            Self::Target(msg) => write!(f, "target error: {msg}"),
        }
    }
}

impl std::error::Error for BackupError {}

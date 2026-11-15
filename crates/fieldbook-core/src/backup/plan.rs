//! Backup planning, target place rules, and target path budget validation.
//!
//! **Responsibility.** Plan files to copy into a backup snapshot, filter derived folders,
//! validate folder placement facts against policy, and enforce the 260-character path budget.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

use std::collections::BTreeSet;

use crate::check::Code;
use crate::labels::{LabelSet, PlaceFacts, PlaceRole, Placement, Refusal, RefuseReason, placement};

use super::{BackupError, Number};

/// Maximum allowed length for staged backup paths on all platforms (260 characters).
pub const MAX_PATH_BUDGET: usize = 260;

/// Result of planning a backup snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPlan {
    /// Number of the snapshot being planned.
    pub snapshot: Number,
    /// Sorted, deduplicated root names covered by this backup (e.g. `["corpus", "state"]`).
    pub roots: Vec<String>,
    /// File paths relative to the store layout in byte order.
    pub files: Vec<String>,
}

fn is_derived(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let norm = p.trim_start_matches('/');
    norm.starts_with("state/pages/")
        || norm == "state/pages"
        || norm.starts_with("state/drill/")
        || norm == "state/drill"
        || norm.starts_with("state/site/")
        || norm == "state/site"
        || norm.starts_with("site/")
        || norm == "site"
        || norm.starts_with("state/index/")
        || norm == "state/index"
        || norm.starts_with("index/")
        || norm == "index"
}

fn matches_include<I: AsRef<str>>(path: &str, include: &[I]) -> bool {
    if include.is_empty() {
        return true;
    }
    let p = path.replace('\\', "/");
    let norm = p.trim_start_matches('/');
    for inc in include {
        let rule = inc.as_ref();
        if rule == "all" {
            return true;
        }
        if rule == "audit"
            && (norm.starts_with("state/audit/")
                || norm == "state/audit"
                || norm.starts_with("audit/")
                || norm == "audit")
        {
            return true;
        }
        if norm.starts_with(rule) {
            let next_byte = norm.as_bytes().get(rule.len());
            if next_byte.is_none() || next_byte == Some(&b'/') {
                return true;
            }
        }
        let state_prefix = format!("state/{rule}");
        if norm.starts_with(&state_prefix) {
            let next_byte = norm.as_bytes().get(state_prefix.len());
            if next_byte.is_none() || next_byte == Some(&b'/') {
                return true;
            }
        }
    }
    false
}

/// True for the audit journal folder, which a snapshot always holds whatever `include` says.
fn is_audit_journal_path(path: &str) -> bool {
    path.starts_with("state/audit/")
}

/// Plans the files to copy for a snapshot in byte order of path, excluding derived folders.
#[must_use]
pub fn plan_backup<S: AsRef<str>, I: AsRef<str>>(
    listing: &[S],
    include: &[I],
    next_number: Number,
) -> BackupPlan {
    let mut files = Vec::new();
    let mut roots_set = BTreeSet::new();

    for item in listing {
        let raw = item.as_ref();
        let normalized = raw.replace('\\', "/");
        let path = normalized.trim_start_matches('/');

        if is_derived(path) {
            continue;
        }
        if !matches_include(path, include) && !is_audit_journal_path(path) {
            continue;
        }

        files.push(path.to_owned());

        if let Some((root, _)) = path.split_once('/') {
            roots_set.insert(root.to_owned());
        } else if !path.is_empty() {
            roots_set.insert(path.to_owned());
        }
    }

    files.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let roots = roots_set.into_iter().collect();

    BackupPlan {
        snapshot: next_number,
        roots,
        files,
    }
}

/// Outcome of testing a target folder's location against placement rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceFinding {
    /// A one-line informational notice (e.g. `kind = "synced"` at home).
    Notice,
    /// A warning finding (e.g. `FB0813` for an undeclared synced path at home).
    Warning(Code),
    /// A refusal finding (e.g. `FB0415` for an undeclared synced path on laptop).
    Refusal(Refusal),
}

impl PlaceFinding {
    /// Returns `true` if this finding is an informational notice.
    #[must_use]
    pub fn is_notice(&self) -> bool {
        matches!(self, Self::Notice)
    }

    /// Returns `true` if this finding is a warning.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Warning(_))
    }

    /// Returns `true` if this finding is a refusal error.
    #[must_use]
    pub fn is_refusal(&self) -> bool {
        matches!(self, Self::Refusal(_))
    }

    /// Returns the diagnostic code associated with this finding, if any.
    #[must_use]
    pub fn code(&self) -> Option<Code> {
        match self {
            Self::Notice => None,
            Self::Warning(code) => Some(*code),
            Self::Refusal(refusal) => Some(refusal.code),
        }
    }
}

/// Judges operating system facts about a target folder using placement rules.
///
/// Implements the six cells of step 5:
/// - Home, Plain folder: none
/// - Home, IT-approved location: none
/// - Home, Synced path: `FB0813` warning, or one-line notice if `kind == "synced"`
/// - Laptop, Plain folder: none
/// - Laptop, IT-approved location: none
/// - Laptop, Synced path: `FB0415` refusal unless declared (then none); `kind = "synced"` does not lift it.
#[must_use]
pub fn target_place(
    laptop: bool,
    kind: impl AsRef<str>,
    facts: PlaceFacts,
    declared: bool,
) -> Option<PlaceFinding> {
    if laptop {
        let is_synced_path = facts.sync_root || facts.reparse;
        if is_synced_path {
            if declared {
                None
            } else {
                let refusal = placement(Placement {
                    laptop: true,
                    under_git: false,
                    role: PlaceRole::BackupTarget,
                    facts,
                    declared: false,
                })
                .err()
                .unwrap_or_else(|| Refusal {
                    code: Code::Fb0415,
                    reason: RefuseReason::Place,
                    labels: LabelSet::unlabelled(),
                });
                Some(PlaceFinding::Refusal(refusal))
            }
        } else {
            None
        }
    } else if facts.sync_root {
        if kind.as_ref() == "synced" {
            Some(PlaceFinding::Notice)
        } else {
            Some(PlaceFinding::Warning(Code::Fb0813))
        }
    } else {
        None
    }
}

/// Checks that the longest staged backup path does not exceed the 260-character budget.
///
/// # Errors
///
/// Returns [`BackupError::PathBudgetExceeded`] if the staged path length exceeds 260 characters,
/// naming the shortest working root.
pub fn path_budget(target: &str, longest_relative: &str) -> Result<(), BackupError> {
    let target_norm = target.trim_end_matches(['/', '\\']);
    let longest_norm = longest_relative.trim_start_matches(['/', '\\']);

    // Staged path: <target>/fieldbook-backup-0001.partial/data/<longest_relative>
    // Length: target_norm + "/" (1) + "fieldbook-backup-0001.partial/data/" (35) + longest_norm
    let staged_len = if target_norm.is_empty() {
        35 + longest_norm.len()
    } else {
        target_norm.len() + 1 + 35 + longest_norm.len()
    };

    if staged_len <= MAX_PATH_BUDGET {
        return Ok(());
    }

    let overhead = 36 + longest_norm.len();
    let max_root_len = MAX_PATH_BUDGET.saturating_sub(overhead);

    let shortest_root = if target.starts_with('/') {
        "/".to_owned()
    } else if target.len() >= 3
        && target.as_bytes()[1] == b':'
        && (target.as_bytes()[2] == b'/' || target.as_bytes()[2] == b'\\')
    {
        target[..3].to_owned()
    } else {
        ".".to_owned()
    };

    Err(BackupError::PathBudgetExceeded {
        staged_len,
        max: MAX_PATH_BUDGET,
        target: target.to_owned(),
        shortest_root,
        max_root_len,
    })
}

//! Snapshot retention planning.
//!
//! **Responsibility.** Determine which snapshots to retain based on snapshot count,
//! calendar weeks, newest verified snapshot, and minimum retention threshold.
//!
//! **Rules.** Pure: no I/O, no clock, no environment.

use std::collections::{BTreeMap, BTreeSet};

use super::{Number, toml::parse_canonical_utc};

/// Metadata about a snapshot candidate inspected during retention planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    /// Number of the snapshot.
    pub number: Number,
    /// UTC timestamp of creation in canonical ISO 8601 format (`YYYY-MM-DDTHH:MM:SSZ`).
    pub created_utc: String,
    /// Whether this snapshot contains a valid `backup.toml`.
    pub has_backup_toml: bool,
}

impl SnapshotInfo {
    /// Creates a new [`SnapshotInfo`].
    #[must_use]
    pub fn new(number: Number, created_utc: impl Into<String>, has_backup_toml: bool) -> Self {
        Self {
            number,
            created_utc: created_utc.into(),
            has_backup_toml,
        }
    }
}

/// Plans which snapshot numbers to keep according to retention rules.
///
/// Rules:
/// - Only entries carrying their own `backup.toml` are considered.
/// - The newest `keep_last` snapshots are kept.
/// - The newest snapshot of each of the last `keep_weekly` calendar weeks is kept.
/// - The newest verified snapshot (`newest_verified`) is never removed.
/// - Never fewer than 2 snapshots are kept if at least 2 exist.
#[must_use]
pub fn plan_retention(
    snapshots: &[SnapshotInfo],
    newest_verified: Option<Number>,
    keep_last: usize,
    keep_weekly: usize,
) -> Vec<Number> {
    let mut valid: Vec<&SnapshotInfo> = snapshots.iter().filter(|s| s.has_backup_toml).collect();

    if valid.is_empty() {
        return Vec::new();
    }

    valid.sort_by_key(|s| s.number);

    let mut kept = BTreeSet::new();

    // Rule 1: The newest keep_last snapshots
    for s in valid.iter().rev().take(keep_last) {
        kept.insert(s.number);
    }

    // Rule 2: The newest snapshot of each of the last keep_weekly calendar weeks
    let mut weeks = BTreeMap::new();
    for s in &valid {
        if let Some((date, _)) = parse_canonical_utc(&s.created_utc) {
            let epoch_days = date.to_epoch_days();
            // Monday-aligned week index: 1970-01-01 was Thursday, so +3 aligns Thursday to 3, Mon to 0.
            let week_idx = (epoch_days + 3).div_euclid(7);
            // Since `valid` is sorted by number ascending, this overwrites with the latest snapshot of each week
            weeks.insert(week_idx, s.number);
        }
    }

    for &num in weeks.values().rev().take(keep_weekly) {
        kept.insert(num);
    }

    // Rule 3: Never remove the newest verified snapshot
    if let Some(nv) = newest_verified
        && valid.iter().any(|s| s.number == nv)
    {
        kept.insert(nv);
    }

    // Rule 4: Never fewer than 2 if 2 exist
    if valid.len() >= 2 && kept.len() < 2 {
        for s in valid.iter().rev() {
            kept.insert(s.number);
            if kept.len() >= 2 {
                break;
            }
        }
    }

    kept.into_iter().collect()
}

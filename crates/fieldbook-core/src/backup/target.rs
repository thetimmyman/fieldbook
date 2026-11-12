//! Traits for backup target destinations and snapshot staging.
//!
//! **Responsibility.** Abstract interface for writing and reading snapshots without coupling
//! to concrete filesystem or network implementations.
//!
//! **Rules.** Pure: traits mention no binary I/O types. Only [`Path`], standard slices,
//! and [`Allowed`] tokens are accepted.

use std::path::Path;

use crate::egress::Allowed;

use super::{BackupError, Number};

/// Port for a backup destination target (e.g. `folder@1`).
pub trait BackupTarget {
    /// Lists the existing completed snapshot numbers in the target.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if the target cannot be listed.
    fn list(&self) -> Result<Vec<Number>, BackupError>;

    /// Reads a file from a snapshot in the target.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if the snapshot or file cannot be read.
    fn read(&self, snapshot: Number, rel: &Path) -> Result<Vec<u8>, BackupError>;

    /// Removes a snapshot from the target during retention pruning.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if removal fails.
    fn remove(&self, snapshot: Number) -> Result<(), BackupError>;

    /// Begins writing a new snapshot in the target, returning a stage handle.
    ///
    /// Refuses an occupied snapshot number and retains the delivery [`Allowed`] token.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if the snapshot number is already occupied or the stage cannot begin.
    fn begin<'a>(
        &'a self,
        snapshot: Number,
        allowed: &'a Allowed,
    ) -> Result<Box<dyn SnapshotStage + 'a>, BackupError>;
}

/// Active staging handle for copying files into a snapshot.
pub trait SnapshotStage {
    /// Writes a single file's bytes into the snapshot staging directory.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if the file cannot be written or is unapproved.
    fn write(&mut self, rel: &Path, bytes: &[u8]) -> Result<(), BackupError>;

    /// Completes the snapshot: syncs files, renames `.partial` to final, and updates `LATEST`.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if syncing, rename, or `LATEST` update fails.
    fn finish(self: Box<Self>) -> Result<(), BackupError>;
}

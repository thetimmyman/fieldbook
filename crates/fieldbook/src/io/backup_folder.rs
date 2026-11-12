//! Folder-based backup target implementation (`folder@1`).
//!
//! Implements [`BackupTarget`] and [`SnapshotStage`] according to card steps 2, 5, and 29.

#![expect(
    clippy::disallowed_methods,
    reason = "io::backup_folder performs controlled directory enumeration and target removal"
)]

use std::path::{Path, PathBuf};

use fieldbook_core::audit::{BackupVerified, Count, EventPayload, Sha256};
use fieldbook_core::backup::{
    BackupError, BackupTarget, BackupToml, Number, SnapshotInfo, SnapshotStage,
};
use fieldbook_core::egress::{Allowed, Operation, Target, Transfer, hash_tree_manifest};

use crate::World;
use crate::io::dirs::{self, Removable};
use crate::io::journal::{Audited, read_valid_events_prefix};

/// Destination target implementing `folder@1`.
pub struct FolderTarget<'a> {
    world: &'a dyn World,
    audited: &'a Audited,
    allowed: &'a Allowed,
    target: PathBuf,
}

impl<'a> FolderTarget<'a> {
    /// Creates a new [`FolderTarget`].
    #[must_use]
    pub fn new(
        world: &'a dyn World,
        audited: &'a Audited,
        allowed: &'a Allowed,
        target: impl Into<PathBuf>,
    ) -> Self {
        Self {
            world,
            audited,
            allowed,
            target: target.into(),
        }
    }

    /// Removes leftover `.partial` directories from prior crashes.
    ///
    /// # Errors
    ///
    /// Returns [`BackupError`] if removal fails.
    pub fn clean_leftover_partials(&self) -> Result<(), BackupError> {
        let Ok(read_dir) = std::fs::read_dir(&self.target) else {
            return Ok(());
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                && file_name.starts_with("fieldbook-backup-")
                && file_name.ends_with(".partial")
            {
                let _ = dirs::remove_outside(
                    self.audited,
                    self.allowed,
                    &path,
                    Removable::PartialSnapshot,
                );
            }
        }
        Ok(())
    }
}

impl BackupTarget for FolderTarget<'_> {
    fn list(&self) -> Result<Vec<Number>, BackupError> {
        let mut numbers = Vec::new();
        let read_dir = match std::fs::read_dir(&self.target) {
            Ok(rd) => rd,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(BackupError::Target(err.to_string())),
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("fieldbook-backup-") || name.ends_with(".partial") {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() || !meta.is_dir() {
                continue;
            }
            if !path.join("backup.toml").is_file() {
                continue;
            }
            let Some(num_str) = name.strip_prefix("fieldbook-backup-") else {
                continue;
            };
            if let Ok(num) = num_str.parse::<Number>() {
                numbers.push(num);
            }
        }

        numbers.sort_unstable();
        numbers.dedup();
        Ok(numbers)
    }

    fn read(&self, snapshot: Number, rel: &Path) -> Result<Vec<u8>, BackupError> {
        let snap_dir = self.target.join(format!("fieldbook-backup-{snapshot:04}"));
        let full_path = snap_dir.join(rel);
        std::fs::read(&full_path).map_err(|err| BackupError::Target(err.to_string()))
    }

    fn remove(&self, snapshot: Number) -> Result<(), BackupError> {
        let snap_dir = self.target.join(format!("fieldbook-backup-{snapshot:04}"));
        dirs::remove_outside(self.audited, self.allowed, &snap_dir, Removable::Snapshot)
            .map_err(|err| BackupError::Target(err.to_string()))
    }

    fn begin<'b>(
        &'b self,
        snapshot: Number,
        allowed: &'b Allowed,
    ) -> Result<Box<dyn SnapshotStage + 'b>, BackupError> {
        let final_dir = self.target.join(format!("fieldbook-backup-{snapshot:04}"));
        if final_dir.exists() {
            return Err(BackupError::OccupiedSnapshot(snapshot));
        }

        let partial_dir = self
            .target
            .join(format!("fieldbook-backup-{snapshot:04}.partial"));
        if partial_dir.exists() {
            dirs::remove_outside(
                self.audited,
                allowed,
                &partial_dir,
                Removable::PartialSnapshot,
            )
            .map_err(|err| BackupError::Target(err.to_string()))?;
        }
        if partial_dir.exists() {
            return Err(BackupError::Target(format!(
                "partial snapshot already occupied: {snapshot}"
            )));
        }

        Ok(Box::new(FolderSnapshotStage {
            world: self.world,
            audited: self.audited,
            allowed,
            target_dir: self.target.clone(),
            snapshot,
            partial_dir,
            final_dir,
            manifest_sha256: None,
            written_files: Vec::new(),
        }))
    }
}

struct FolderSnapshotStage<'b> {
    world: &'b dyn World,
    audited: &'b Audited,
    allowed: &'b Allowed,
    target_dir: PathBuf,
    snapshot: Number,
    partial_dir: PathBuf,
    final_dir: PathBuf,
    manifest_sha256: Option<String>,
    written_files: Vec<(PathBuf, Vec<u8>)>,
}

impl SnapshotStage for FolderSnapshotStage<'_> {
    fn write(&mut self, rel: &Path, bytes: &[u8]) -> Result<(), BackupError> {
        let full_path = self.partial_dir.join(rel);
        self.world
            .write_outside(self.allowed, self.audited, &full_path, bytes)
            .map_err(|err| BackupError::Target(err.to_string()))?;

        if rel == Path::new("backup.toml")
            && let Ok(info) = BackupToml::parse(&String::from_utf8_lossy(bytes))
        {
            self.manifest_sha256 = Some(info.manifest_sha256);
        }

        self.written_files.push((rel.to_path_buf(), bytes.to_vec()));
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), BackupError> {
        // 1. Re-read and compare written files
        for (rel, expected_bytes) in &self.written_files {
            let full_path = self.partial_dir.join(rel);
            let read_bytes =
                std::fs::read(&full_path).map_err(|err| BackupError::Target(err.to_string()))?;
            if read_bytes != *expected_bytes {
                return Err(BackupError::Target(format!(
                    "readback mismatch for {}",
                    rel.display()
                )));
            }
        }

        // 2. Rename partial to final snapshot directory
        dirs::rename_dir_outside(
            self.audited,
            self.allowed,
            &self.partial_dir,
            &self.final_dir,
        )
        .map_err(|err| BackupError::Target(err.to_string()))?;

        // 3. Write LATEST pointer atomically
        let manifest_sha = self.manifest_sha256.unwrap_or_default();
        let latest_content = format!("{}\n{}\n", self.snapshot, manifest_sha);
        let latest_path = self.target_dir.join("LATEST");
        self.world
            .write_outside(
                self.allowed,
                self.audited,
                &latest_path,
                latest_content.as_bytes(),
            )
            .map_err(|err| BackupError::Target(err.to_string()))?;

        Ok(())
    }
}

/// Lists all completed snapshot numbers in the given target directory.
#[must_use]
pub fn list_existing_snapshots(target: &Path) -> Vec<Number> {
    let mut numbers = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(target) else {
        return numbers;
    };
    for entry in read_dir.flatten() {
        let p = entry.path();
        if let Some(name) = p.file_name().and_then(|n| n.to_str())
            && name.starts_with("fieldbook-backup-")
            && !name.ends_with(".partial")
            && let Some(num_str) = name.strip_prefix("fieldbook-backup-")
            && let Ok(num) = num_str.parse::<Number>()
        {
            numbers.push(num);
        }
    }
    numbers.sort_unstable();
    numbers
}

/// Constructs a tree removal [`Transfer`] for a directory if it exists.
pub fn tree_remove_transfer(dir: &Path) -> Result<Option<Transfer>, std::io::Error> {
    if !dir.exists() {
        return Ok(None);
    }
    let mut entries = Vec::new();
    collect_tree_entries(dir, dir, &mut entries)?;
    let (sha, bytes) = hash_tree_manifest(&entries);
    Ok(Some(Transfer {
        target: Target::Path(dir.to_path_buf()),
        operation: Operation::RemoveTree,
        sha256: sha,
        bytes,
    }))
}

fn collect_tree_entries(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, String, u64)>,
) -> Result<(), std::io::Error> {
    if current.is_file() {
        let rel = current
            .strip_prefix(root)
            .map_err(|e| std::io::Error::other(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(current)?;
        let sha = Sha256::digest(&bytes).to_string();
        let len = bytes.len() as u64;
        out.push((rel, sha, len));
    } else if current.is_dir() {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            collect_tree_entries(root, &entry.path(), out)?;
        }
    }
    Ok(())
}

/// Collects partial snapshot removals for delivery planning.
#[must_use]
pub fn list_partial_removals(target_path: &Path) -> Vec<Transfer> {
    let mut transfers = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(target_path) {
        for entry in read_dir.flatten() {
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str())
                && name.starts_with("fieldbook-backup-")
                && name.ends_with(".partial")
                && let Ok(Some(t)) = tree_remove_transfer(&p)
            {
                transfers.push(t);
            }
        }
    }
    transfers
}

/// Reads metadata for existing snapshots to plan retention.
#[must_use]
pub fn read_snapshot_infos(target_path: &Path, existing_snapshots: &[Number]) -> Vec<SnapshotInfo> {
    let mut snapshot_infos = Vec::new();
    for &num in existing_snapshots {
        let toml_path = target_path.join(format!("fieldbook-backup-{num:04}/backup.toml"));
        if let Ok(text) = std::fs::read_to_string(&toml_path)
            && let Ok(info) = BackupToml::parse(&text)
        {
            snapshot_infos.push(SnapshotInfo::new(num, info.created_utc, true));
        } else {
            snapshot_infos.push(SnapshotInfo::new(num, String::new(), false));
        }
    }
    snapshot_infos
}

/// Finds the newest verified snapshot number recorded in the state journal for the target.
#[must_use]
pub fn newest_verified_snapshot_for_target(
    state_dir: &Path,
    target_path: &Path,
    existing_snapshots: &[Number],
) -> Option<Number> {
    let canonical = target_path
        .canonicalize()
        .unwrap_or_else(|_| target_path.to_path_buf());
    let canonical_str = canonical.to_string_lossy();
    let target_sha256 = Sha256::digest(canonical_str.as_bytes());

    let events = read_valid_events_prefix(state_dir);
    for &num in existing_snapshots.iter().rev() {
        let toml_path = target_path.join(format!("fieldbook-backup-{num:04}/backup.toml"));
        let Ok(text) = std::fs::read_to_string(&toml_path) else {
            continue;
        };
        let Ok(info) = BackupToml::parse(&text) else {
            continue;
        };
        let count_num = Count::new(u64::from(num));
        for e in events.iter().rev() {
            if let EventPayload::BackupVerify {
                outcome,
                verified:
                    BackupVerified::Snapshot {
                        target_sha256: t,
                        snapshot: s,
                        manifest_sha256: m,
                    },
            } = &e.payload
                && outcome.as_str() == "ok"
                && t == &target_sha256
                && s == &count_num
                && m.as_str() == info.manifest_sha256
            {
                return Some(num);
            }
        }
    }
    None
}

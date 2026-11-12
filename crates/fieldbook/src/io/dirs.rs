//! Controlled directory checks and removal outside the data roots (`build-v0` step 9).
//!
//! **Responsibility.** Verify that build output roots (`edition`, `originals`) inside `--out DIR`
//! were created by `fieldbook build` (via the `.fieldbook-out` parent marker) before replacing
//! them, and provide the single authorized directory-tree removal helper [`remove_outside`]
//! outside `io::journal`.

#![expect(
    clippy::disallowed_methods,
    reason = "io::dirs is the single authorized directory-removal seam outside io::journal"
)]

use std::path::{Component, Path, PathBuf};

use fieldbook_core::audit::Sha256;
use fieldbook_core::egress::{Allowed, Operation, Target, Transfer, hash_tree_manifest};
use fieldbook_core::strings;

use crate::io::{Audited, Error};

/// Name of the parent marker file written in `--out DIR` by `fieldbook build`.
pub const MARKER_FILE: &str = ".fieldbook-out";

/// Exact bytes required in [`MARKER_FILE`] (`"fieldbook build output\n"`).
#[must_use]
pub fn marker_bytes() -> Vec<u8> {
    format!("{} build output\n", strings::binary()).into_bytes()
}

/// Format the refusal diagnostic when an existing build root was not created by `fieldbook build`.
#[must_use]
pub fn foreign_root_message(root: &Path) -> String {
    format!(
        "refusing to replace {}: it was not made by {} build; remove it yourself or choose another --out",
        root.display(),
        strings::binary()
    )
}

/// Managed subdirectory under `--out DIR` owned by `fieldbook build`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildRoot {
    /// `DIR/edition`
    Edition,
    /// `DIR/originals`
    Originals,
}

impl BuildRoot {
    /// Directory name component under `--out DIR`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Edition => "edition",
            Self::Originals => "originals",
        }
    }

    /// Match a directory name component to a [`BuildRoot`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "edition" => Some(Self::Edition),
            "originals" => Some(Self::Originals),
            _ => None,
        }
    }
}

/// Kind of directory tree that [`remove_outside`] may remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removable {
    /// A managed build output subdirectory (`edition` or `originals`).
    BuildRoot(BuildRoot),
    /// A partial backup snapshot directory (reserved for `backup`).
    PartialSnapshot,
    /// A completed backup snapshot directory (reserved for `backup`).
    Snapshot,
}

/// Check that every existing root in `roots` under `dir` is a regular directory accompanied
/// by a valid regular `.fieldbook-out` marker file in `dir`.
///
/// An absent root is valid even when `dir` or `.fieldbook-out` does not exist yet.
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] naming the offending root if an existing root is a symlink,
/// is not a directory, escapes `dir`, or lacks a valid regular `.fieldbook-out` marker in `dir`.
/// Returns [`Error::Io`] on underlying filesystem errors.
pub fn check_build_roots(dir: &Path, roots: &[BuildRoot]) -> Result<(), Error> {
    if dir.as_os_str().is_empty()
        || dir
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(Error::OutOfScope(dir.to_path_buf()));
    }
    let resolved_dir = crate::io::root::resolve_outside_path(dir)?;
    if let Ok(dir_meta) = std::fs::symlink_metadata(&resolved_dir)
        && (dir_meta.file_type().is_symlink() || !dir_meta.is_dir())
    {
        return Err(Error::OutOfScope(dir.to_path_buf()));
    }

    for &br in roots {
        let raw_root = dir.join(br.as_str());
        let resolved_root = resolved_dir.join(br.as_str());
        match std::fs::symlink_metadata(&resolved_root) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(Error::new(&raw_root, err.to_string())),
            Ok(root_meta) => {
                if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
                    return Err(Error::OutOfScope(raw_root));
                }
                let check_resolved = crate::io::root::resolve_outside_path(&raw_root)
                    .map_err(|_| Error::OutOfScope(raw_root.clone()))?;
                if check_resolved != resolved_root
                    || check_resolved.parent() != Some(resolved_dir.as_path())
                    || check_resolved.file_name().and_then(|n| n.to_str()) != Some(br.as_str())
                {
                    return Err(Error::OutOfScope(raw_root));
                }
                if !has_valid_marker(&resolved_dir)? {
                    return Err(Error::OutOfScope(raw_root));
                }
            }
        }
    }
    Ok(())
}

fn has_valid_marker(resolved_dir: &Path) -> Result<bool, Error> {
    let marker_path = resolved_dir.join(MARKER_FILE);
    let meta = match std::fs::symlink_metadata(&marker_path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(Error::new(&marker_path, err.to_string())),
        Ok(m) => m,
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Ok(false);
    }
    if crate::io::root::resolve_outside_path(&marker_path).is_err() {
        return Ok(false);
    }
    let bytes = std::fs::read(&marker_path).map_err(|e| Error::new(&marker_path, e.to_string()))?;
    Ok(bytes == marker_bytes())
}

/// Compute the [`Operation::RemoveTree`] [`Transfer`] for `path` if `path` currently exists.
///
/// Walks `path` recursively (rejecting symlinks with [`Error::OutOfScope`]) and hashes its
/// relative file manifest via [`hash_tree_manifest`].
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] if `path` or any entry inside it is a symlink or escapes scope,
/// or [`Error::Io`] on filesystem read failure.
pub fn tree_remove_transfer(path: &Path) -> Result<Option<Transfer>, Error> {
    let resolved = crate::io::root::resolve_outside_path(path)?;
    match std::fs::symlink_metadata(&resolved) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::new(path, err.to_string())),
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(Error::OutOfScope(path.to_path_buf()));
            }
            let (sha256, bytes) = compute_dir_manifest(&resolved, &resolved)?;
            Ok(Some(Transfer {
                target: Target::Path(resolved),
                operation: Operation::RemoveTree,
                sha256,
                bytes,
            }))
        }
    }
}

fn compute_dir_manifest(root: &Path, current: &Path) -> Result<(String, u64), Error> {
    let mut entries = Vec::new();
    collect_manifest_entries(root, current, &mut entries)?;
    Ok(hash_tree_manifest(&entries))
}

fn collect_manifest_entries(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, String, u64)>,
) -> Result<(), Error> {
    let read_dir =
        std::fs::read_dir(current).map_err(|err| Error::new(current, err.to_string()))?;
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|err| Error::new(current, err.to_string()))?;
        children.push(entry.path());
    }
    children.sort();

    for child in children {
        let meta =
            std::fs::symlink_metadata(&child).map_err(|err| Error::new(&child, err.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(Error::OutOfScope(child));
        }
        let resolved_child = crate::io::root::resolve_outside_path(&child)?;
        if resolved_child != child || !resolved_child.starts_with(root) {
            return Err(Error::OutOfScope(child));
        }
        if meta.is_dir() {
            collect_manifest_entries(root, &child, out)?;
        } else if meta.is_file() {
            let bytes = std::fs::read(&child).map_err(|err| Error::new(&child, err.to_string()))?;
            let rel = child
                .strip_prefix(root)
                .map_err(|_| Error::OutOfScope(child.clone()))?;
            let rel_slash = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let len_u64 = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            out.push((
                rel_slash,
                Sha256::digest(&bytes).as_str().to_owned(),
                len_u64,
            ));
        } else {
            return Err(Error::OutOfScope(child));
        }
    }
    Ok(())
}

fn is_partial_snapshot_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("fieldbook-backup-") else {
        return false;
    };
    let Some(num_str) = rest.strip_suffix(".partial") else {
        return false;
    };
    !num_str.is_empty() && num_str.bytes().all(|b| b.is_ascii_digit())
}

fn is_snapshot_name(name: &str) -> bool {
    let Some(num_str) = name.strip_prefix("fieldbook-backup-") else {
        return false;
    };
    !num_str.is_empty() && num_str.bytes().all(|b| b.is_ascii_digit())
}

/// Remove a directory tree outside the data roots under `audited` and `allowed`.
///
/// Initially only [`Removable::BuildRoot`] operations are valid; `backup` adds its snapshot cases.
/// Repeats [`check_build_roots`] on the parent folder for build roots, recomputes the tree manifest of
/// `path`, and verifies that `allowed` binds [`Operation::RemoveTree`] for `path` with the exact manifest
/// SHA-256 and byte count before calling `std::fs::remove_dir_all`.
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] if `audited` is state-only or inactive, if `what` does not match,
/// if [`check_build_roots`] fails, or if `allowed` does not permit the removal; returns [`Error::Io`]
/// if `remove_dir_all` fails.
pub fn remove_outside(
    audited: &Audited,
    allowed: &Allowed,
    path: &Path,
    what: Removable,
) -> Result<(), Error> {
    if audited.is_state_only() || !audited.has_active_lease() {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    if audited.is_recovery_only() && !audited.is_approved_recovery(allowed) {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Err(Error::OutOfScope(path.to_path_buf()));
    };
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::OutOfScope(path.to_path_buf()))?;

    match what {
        Removable::BuildRoot(br) => {
            if file_name != br.as_str() {
                return Err(Error::OutOfScope(path.to_path_buf()));
            }
            check_build_roots(parent, &[br])?;
        }
        Removable::PartialSnapshot => {
            if !is_partial_snapshot_name(file_name) {
                return Err(Error::OutOfScope(path.to_path_buf()));
            }
        }
        Removable::Snapshot => {
            if !is_snapshot_name(file_name) {
                return Err(Error::OutOfScope(path.to_path_buf()));
            }
        }
    }

    let resolved = crate::io::root::resolve_outside_path(path)?;
    if what == Removable::Snapshot {
        let backup_toml = resolved.join("backup.toml");
        let Ok(meta) = std::fs::symlink_metadata(&backup_toml) else {
            return Err(Error::OutOfScope(path.to_path_buf()));
        };
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(Error::OutOfScope(path.to_path_buf()));
        }
    }

    let (manifest_sha, total_bytes) = compute_dir_manifest(&resolved, &resolved)?;
    let permitted = allowed.permits_manifest(
        &Target::Path(resolved.clone()),
        &Operation::RemoveTree,
        &manifest_sha,
        total_bytes,
    ) || allowed.transfers().unwrap_or(&[]).iter().any(|t| {
        if let Target::Path(bound_path) = &t.target
            && let Ok(bound_resolved) = crate::io::root::resolve_outside_path(bound_path)
            && bound_resolved == resolved
        {
            allowed.permits_manifest(
                &t.target,
                &Operation::RemoveTree,
                &manifest_sha,
                total_bytes,
            )
        } else {
            false
        }
    });
    if !permitted {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }

    std::fs::remove_dir_all(&resolved).map_err(|err| Error::new(path, err.to_string()))
}

/// Rename a directory tree outside the data roots under `audited` and `allowed`.
///
/// Verifies that `audited` holds an active lease, `from` and `to` resolve outside the data
/// roots, `from` exists as a regular directory (not a symlink), and `allowed` binds
/// [`Operation::RenameTree`] with `target` matching `from`, destination matching `to`,
/// and exact manifest SHA-256 and total bytes before renaming via `std::fs::rename`.
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] if `audited` is state-only or inactive, if `from` or `to`
/// escape scope, or if `allowed` does not permit the rename; returns [`Error::Io`] on
/// underlying filesystem failure.
pub fn rename_dir_outside(
    audited: &Audited,
    allowed: &Allowed,
    from: &Path,
    to: &Path,
) -> Result<(), Error> {
    if audited.is_state_only() || !audited.has_active_lease() {
        return Err(Error::OutOfScope(from.to_path_buf()));
    }
    if audited.is_recovery_only() && !audited.is_approved_recovery(allowed) {
        return Err(Error::OutOfScope(from.to_path_buf()));
    }
    if from.as_os_str().is_empty() || to.as_os_str().is_empty() {
        return Err(Error::OutOfScope(from.to_path_buf()));
    }
    let Some(_) = from.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Err(Error::OutOfScope(from.to_path_buf()));
    };
    let Some(_) = to.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Err(Error::OutOfScope(to.to_path_buf()));
    };

    let resolved_from = crate::io::root::resolve_outside_path(from)?;
    let resolved_to = crate::io::root::resolve_outside_path(to)?;

    let meta_from = match std::fs::symlink_metadata(&resolved_from) {
        Ok(m) => m,
        Err(err) => return Err(Error::new(from, err.to_string())),
    };
    if meta_from.file_type().is_symlink() || !meta_from.is_dir() {
        return Err(Error::OutOfScope(from.to_path_buf()));
    }

    let (manifest_sha, total_bytes) = compute_dir_manifest(&resolved_from, &resolved_from)?;

    let op = Operation::RenameTree {
        to: resolved_to.clone(),
    };
    let permitted = allowed.permits_manifest(
        &Target::Path(resolved_from.clone()),
        &op,
        &manifest_sha,
        total_bytes,
    ) || allowed.transfers().unwrap_or(&[]).iter().any(|t| {
        if let Target::Path(bound_from) = &t.target
            && let Operation::RenameTree { to: bound_to } = &t.operation
            && let Ok(bound_resolved_from) = crate::io::root::resolve_outside_path(bound_from)
            && let Ok(bound_resolved_to) = crate::io::root::resolve_outside_path(bound_to)
            && bound_resolved_from == resolved_from
            && bound_resolved_to == resolved_to
        {
            allowed.permits_manifest(&t.target, &t.operation, &manifest_sha, total_bytes)
        } else {
            false
        }
    });

    if !permitted {
        return Err(Error::OutOfScope(from.to_path_buf()));
    }

    std::fs::rename(&resolved_from, &resolved_to).map_err(|err| Error::new(from, err.to_string()))
}

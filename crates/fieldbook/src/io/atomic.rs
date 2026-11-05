//! Atomic file writer for `fieldbook` (`add-new-and-edition`, `core-policy`).
//!
//! **Responsibility.** Write a file via a temporary file in the same directory, flush and
//! sync it, and rename it into place, retrying on sharing violations and cleaning up the
//! temporary file on any failure. Every function in this module requires an [`Audited`] token,
//! and the module exposes exactly two public doors: [`write_in_root`] and [`write_outside`].

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "io::atomic is the single authorized file writer outside io::journal"
)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use fieldbook_core::audit::{Sha256, StoreId};
use fieldbook_core::egress::{Allowed, Operation, Target};

use crate::io::root::RootPath;
use crate::io::{Audited, Error};

static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(1);

const IN_USE_RETRIES: usize = 3;
const IN_USE_RETRY_DELAY: Duration = Duration::from_millis(200);

/// Write `bytes` to `rel` inside `root` atomically under `audited`.
///
/// Resolves existing ancestors and remaining components against the canonical `root` before
/// creating any directory or temporary file. Refuses with [`Error::OutOfScope`] if `audited`
/// is state-only and `root.store() != StoreId::State`, if `audited` holds no active lease, or
/// if `rel` escapes `root` via `..`, a symlink, or a Windows junction.
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] on scope or containment violations, [`Error::InUse`] if the
/// target remains locked after retries, or [`Error::Io`] on any other filesystem failure.
pub fn write_in_root(
    root: &RootPath,
    rel: &Path,
    bytes: &[u8],
    audited: &Audited,
) -> Result<Sha256, Error> {
    let raw_path = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        root.path().join(rel)
    };
    if !audited.has_active_lease() {
        return Err(Error::OutOfScope(raw_path));
    }
    if audited.is_state_only() && root.store() != StoreId::State {
        return Err(Error::OutOfScope(raw_path));
    }
    let resolved = crate::io::root::resolve_in_root(root, rel)?;
    crate::io::journal::run_write_pause_hook(&resolved);
    write_atomic(&resolved, bytes)
}

/// Write `bytes` to `path` outside the data roots atomically under `allowed` and `audited`.
///
/// Verifies that `audited` is not state-only and holds an active lease, canonicalizes existing
/// ancestors of `path`, and verifies that `allowed` permits the exact canonical path,
/// [`Operation::Write`], byte count, and SHA-256 digest before creating any directory or
/// temporary file.
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] if `audited` is state-only or inactive, or if `allowed` does
/// not bind the exact target, operation, length, and digest; returns [`Error::InUse`] or
/// [`Error::Io`] on filesystem failure.
pub fn write_outside(
    allowed: &Allowed,
    path: &Path,
    bytes: &[u8],
    audited: &Audited,
) -> Result<Sha256, Error> {
    if audited.is_state_only() || !audited.has_active_lease() {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    let resolved = crate::io::root::resolve_outside_path(path)?;
    let permitted =
        allowed.permits_transfer(&Target::Path(resolved.clone()), &Operation::Write, bytes)
            || allowed.transfers().unwrap_or(&[]).iter().any(|t| {
                if let Target::Path(bound_path) = &t.target
                    && let Ok(bound_resolved) = crate::io::root::resolve_outside_path(bound_path)
                    && bound_resolved == resolved
                {
                    allowed.permits_transfer(&t.target, &Operation::Write, bytes)
                } else {
                    false
                }
            });
    if !permitted {
        return Err(Error::OutOfScope(path.to_path_buf()));
    }
    crate::io::journal::run_write_pause_hook(&resolved);
    write_atomic(&resolved, bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<Sha256, Error> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|err| Error::new(parent, err.to_string()))?;
    }

    let file_name = path
        .file_name()
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    let seq = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp_path = parent.join(format!(".{file_name}.tmp-{pid}-{seq}"));

    let write_res = (|| -> Result<(), std::io::Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_data()?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(Error::new(path, err.to_string()));
    }

    if let Err(first_err) = std::fs::rename(&tmp_path, path) {
        let target_is_dir = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
        let is_sharing_or_access = !target_is_dir
            && (first_err.kind() == std::io::ErrorKind::PermissionDenied
                || matches!(first_err.raw_os_error(), Some(5 | 32 | 33)));

        if is_sharing_or_access {
            let mut last_err = first_err;
            for _ in 0..IN_USE_RETRIES {
                std::thread::sleep(IN_USE_RETRY_DELAY);
                match std::fs::rename(&tmp_path, path) {
                    Ok(()) => return Ok(Sha256::digest(bytes)),
                    Err(err) => last_err = err,
                }
            }
            let _ = std::fs::remove_file(&tmp_path);
            let still_in_use = last_err.kind() == std::io::ErrorKind::PermissionDenied
                || matches!(last_err.raw_os_error(), Some(5 | 32 | 33));
            if still_in_use {
                return Err(Error::InUse(path.to_path_buf()));
            }
            return Err(Error::new(path, last_err.to_string()));
        }

        let _ = std::fs::remove_file(&tmp_path);
        return Err(Error::new(path, first_err.to_string()));
    }

    Ok(Sha256::digest(bytes))
}

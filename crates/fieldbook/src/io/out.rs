//! Managed output folder replacement for `fieldbook build` (`build-v0` step 4).
//!
//! **Responsibility.** Replace one managed build root (`DIR/edition` or `DIR/originals`)
//! under an [`Allowed`] delivery token and an [`Audited`] journal token, repeating the
//! `.fieldbook-out` ownership check before removing an existing root via [`dirs::remove_outside`]
//! and writing every file through [`atomic::write_outside`].

use std::collections::BTreeMap;
use std::path::{Component, Path};

use fieldbook_core::egress::Allowed;

use crate::io::dirs::{self, BuildRoot, Removable};
use crate::io::{Audited, Error, atomic};

/// Replace `root` (`DIR/edition` or `DIR/originals`) with `files` under `allowed` and `audited`.
///
/// Repeats [`dirs::check_build_roots`] on `root`'s parent directory, removes `root` via
/// [`dirs::remove_outside`] if it exists, and writes every `(rel, bytes)` entry in `files`
/// through [`atomic::write_outside`]. Never removes `root`'s parent directory (`DIR`).
///
/// # Errors
///
/// Returns [`Error::OutOfScope`] if `root` is not a valid [`BuildRoot`], if any relative file
/// path escapes `root`, if [`dirs::check_build_roots`] refuses a foreign root, or if `allowed`
/// or `audited` rejects the removal or any file write; returns [`Error::Io`] or [`Error::InUse`]
/// on filesystem failure.
pub fn replace(
    root: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    allowed: &Allowed,
    audited: &Audited,
) -> Result<(), Error> {
    if audited.is_state_only() || !audited.has_active_lease() {
        return Err(Error::OutOfScope(root.to_path_buf()));
    }
    let Some(root_name) = root.file_name().and_then(|n| n.to_str()) else {
        return Err(Error::OutOfScope(root.to_path_buf()));
    };
    let Some(build_root) = BuildRoot::from_name(root_name) else {
        return Err(Error::OutOfScope(root.to_path_buf()));
    };
    let Some(parent) = root.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Err(Error::OutOfScope(root.to_path_buf()));
    };

    dirs::check_build_roots(parent, &[build_root])?;

    for (rel, bytes) in files {
        if !is_valid_relative_output_path(rel) {
            return Err(Error::OutOfScope(root.join(rel)));
        }
        let target = root.join(rel);
        let _ = atomic::check_outside_write_binding(allowed, &target, bytes)?;
    }

    let resolved_root = crate::io::root::resolve_outside_path(root)?;
    if resolved_root.symlink_metadata().is_ok() {
        dirs::remove_outside(audited, allowed, root, Removable::BuildRoot(build_root))?;
    }

    for (rel, bytes) in files {
        let target = root.join(rel);
        atomic::write_outside(allowed, &target, bytes, audited)?;
    }

    Ok(())
}

fn is_valid_relative_output_path(rel: &str) -> bool {
    if rel.is_empty() || rel.starts_with('/') || rel.contains('\\') {
        return false;
    }
    Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

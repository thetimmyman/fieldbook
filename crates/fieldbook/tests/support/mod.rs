//! Shared test support for `fieldbook` integration tests (`audit-journal`, `add-new-and-edition`).

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "shared test support modules are selectively used across test binaries"
)]

use std::path::Path;

pub mod faulty;
pub mod state;
pub mod world;

/// Recursively copy `fixture` into a new [`tempfile::TempDir`].
#[must_use]
pub fn temp_copy(fixture: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temporary fixture directory");
    copy_dir_recursive(fixture, dir.path()).expect("copy fixture directory into tempdir");
    dir
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

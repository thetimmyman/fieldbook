//! Read-only loader for `settings.toml` in the state folder (`ai-config-and-keys` step 5).
//!
//! Never creates the state folder or writes to disk.

#![expect(
    clippy::disallowed_methods,
    reason = "io::ai_settings reads settings.toml from the resolved state folder"
)]

use std::path::Path;

use crate::io::WorldError;

/// Reads `<state_dir>/settings.toml` as UTF-8 text, returning `Ok(None)` when either the state
/// folder or `settings.toml` does not exist.
///
/// # Errors
///
/// Returns [`WorldError`] if `settings.toml` exists but cannot be read or is not valid UTF-8.
pub fn read_settings_text(state_dir: &Path) -> Result<Option<String>, WorldError> {
    let path = state_dir.join("settings.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(WorldError::new(&path, err.to_string())),
    }
}

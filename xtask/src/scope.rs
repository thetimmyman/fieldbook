//! `scope BASE HEAD`: a pull request that deletes a card changes only the paths the card lists (`docs-checks`).

use std::path::Path;

/// `scope BASE HEAD`.
pub fn command(_root: &Path, _base: &str, _head: &str) -> Result<bool, String> {
    Err("not written yet".to_owned())
}

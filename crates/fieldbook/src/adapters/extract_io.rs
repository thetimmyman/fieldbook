//! Staged extraction replay adapter (`staged@1`) over [`World::read_file`].
//!
//! Reads `staging/<sha256>/extraction.toml` and `pNNNN.txt` files strictly through [`World`],
//! never calling `std::fs` directly.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fieldbook_core::extraction::{ExtractError, Extraction, Extractor};
use fieldbook_core::pipeline::VerifiedExtraction;

use crate::World;
use crate::io::versions::staged_v1;

/// Extractor adapter that replays and verifies pre-staged text under `<state>/staging/<sha256>/`.
pub struct Staged<'w> {
    world: &'w dyn World,
    state_dir: PathBuf,
}

impl<'w> Staged<'w> {
    /// Creates a [`Staged`] adapter bound to `world` and `world.state_dir()`.
    #[must_use]
    pub fn new(world: &'w dyn World) -> Self {
        Self {
            world,
            state_dir: world.state_dir().to_path_buf(),
        }
    }

    /// Creates a [`Staged`] adapter reading staging folders under an explicit `state_dir`.
    #[must_use]
    pub fn with_state_dir(world: &'w dyn World, state_dir: impl Into<PathBuf>) -> Self {
        Self {
            world,
            state_dir: state_dir.into(),
        }
    }

    /// Loads and verifies `staging/<sha256>/` for `bytes`, returning a complete [`VerifiedExtraction`].
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`ExtractError::Missing`] with `"staging folder"` when `extraction.toml` is missing or any
    ///   staged file cannot be read through [`World`];
    /// - [`ExtractError::Missing`] with `"matching staging folder"` when `extraction.toml` cannot
    ///   parse, `original_sha256` / page count / thin pages / page hashes differ, or a page file is
    ///   absent.
    pub fn verify(&self, bytes: &[u8]) -> Result<VerifiedExtraction, ExtractError> {
        let orig_sha = fieldbook_core::hash::sha256_hex(bytes);
        let staging_dir = self.state_dir.join(staged_v1::staging_rel_dir(&orig_sha));
        let toml_path = staging_dir.join("extraction.toml");

        let toml_bytes = self
            .world
            .read_file(&toml_path)
            .map_err(|_| ExtractError::Missing("staging folder"))?
            .ok_or(ExtractError::Missing("staging folder"))?;

        let (verified_sha, stems) = staged_v1::expected_page_stems(bytes, Some(&toml_bytes))?;
        let mut page_files = BTreeMap::new();
        for stem in stems {
            let page_path = staging_dir.join(format!("{stem}.txt"));
            let page_bytes = self
                .world
                .read_file(&page_path)
                .map_err(|_| ExtractError::Missing("staging folder"))?
                .ok_or(ExtractError::Missing("matching staging folder"))?;
            page_files.insert(stem, page_bytes);
        }

        VerifiedExtraction::verify(&verified_sha, &toml_bytes, page_files)
    }
}

impl Extractor for Staged<'_> {
    fn id(&self) -> &'static str {
        "staged@1"
    }

    fn accepts(&self, _ext: &str, _head: &[u8]) -> bool {
        true
    }

    fn extract(&self, bytes: &[u8], _ext: &str) -> Result<Extraction, ExtractError> {
        self.verify(bytes).map(|v| v.extraction)
    }
}

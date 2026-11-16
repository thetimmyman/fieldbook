//! Retained verification kernel for the Extractor `staged@1` adapter (published compatibility binding).
//!
//! **Responsibility.** Frozen pure verification logic of `staged@1`: resolves the original
//! SHA-256 staging directory (`staging/<sha256>`), checks that staged `extraction.toml` and page
//! files are present and bound to `original_bytes`, and delegates full record/page verification to
//! [`VerifiedExtraction::verify`].
//!
//! **Rules.** Pure: no I/O. Any change is a new `staged_v2` file.

use std::collections::BTreeMap;

use fieldbook_core::extraction::{ExtractError, ExtractionToml, page_stem};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::pipeline::VerifiedExtraction;

/// Canonical relative directory path under `<state>` holding staged extraction artifacts for `orig_sha`.
#[must_use]
pub fn staging_rel_dir(orig_sha: &str) -> String {
    format!("staging/{orig_sha}")
}

/// Validates staged `extraction.toml` header fields (`original_sha256` and page count) and
/// returns the ordered page stems (`p0001`, ...) expected for `original_bytes`.
///
/// # Errors
///
/// Returns [`ExtractError::Missing`] when `toml_bytes` is `None` (`"staging folder"`), or when the
/// header fails parsing, mismatches `original_bytes`'s SHA-256, or declares invalid page keys
/// (`"matching staging folder"`).
pub fn expected_page_stems(
    original_bytes: &[u8],
    toml_bytes: Option<&[u8]>,
) -> Result<(String, Vec<String>), ExtractError> {
    let orig_sha = sha256_hex(original_bytes);
    let raw = toml_bytes.ok_or(ExtractError::Missing("staging folder"))?;
    let toml_text =
        std::str::from_utf8(raw).map_err(|_| ExtractError::Missing("matching staging folder"))?;
    let record = ExtractionToml::parse(toml_text)
        .map_err(|_| ExtractError::Missing("matching staging folder"))?;
    let expected_pages = usize::try_from(record.page_count)
        .map_err(|_| ExtractError::Missing("matching staging folder"))?;
    if record.original_sha256 != orig_sha || record.pages.len() != expected_pages {
        return Err(ExtractError::Missing("matching staging folder"));
    }

    let mut stems = Vec::with_capacity(expected_pages);
    for num in 1..=record.page_count {
        let stem = page_stem(num);
        if !record.pages.contains_key(&stem) {
            return Err(ExtractError::Missing("matching staging folder"));
        }
        stems.push(stem);
    }

    Ok((orig_sha, stems))
}

/// Verifies pre-loaded staged `extraction.toml` bytes and `page_files` against `original_bytes`.
///
/// # Errors
///
/// Returns [`ExtractError::Missing`] when staged metadata is absent, malformed, or does not match
/// `original_bytes` or `page_files`.
pub fn verify_staged(
    original_bytes: &[u8],
    toml_bytes: Option<&[u8]>,
    page_files: BTreeMap<String, Vec<u8>>,
) -> Result<VerifiedExtraction, ExtractError> {
    let raw_toml = toml_bytes.ok_or(ExtractError::Missing("staging folder"))?;
    let (orig_sha, _) = expected_page_stems(original_bytes, Some(raw_toml))?;
    VerifiedExtraction::verify(&orig_sha, raw_toml, page_files)
}

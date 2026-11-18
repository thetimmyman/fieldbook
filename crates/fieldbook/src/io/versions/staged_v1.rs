//! Retained verification kernel for the Extractor `staged@1` adapter (published compatibility binding).
//!
//! **Responsibility.** Frozen pure verification behavior of `staged@1`: resolves the original
//! SHA-256 staging directory (`staging/<sha256>`), validates `extraction.toml` header, page count,
//! page digests, UTF-8 page bodies, thin-page set, and method schema keys against `original_bytes`.
//! A change of contract requires a new `staged_v2` file; this file and its archived vectors stay
//! executable.
//!
//! **Rules.** Pure: no I/O.

use std::collections::BTreeMap;

use fieldbook_core::extraction::{
    ExtractError, ExtractionMethod, ExtractionToml, PageText, page_stem, thin_page_numbers,
};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::pipeline::VerifiedExtraction;

/// Adapter identifier retained by this kernel (`staged@1`).
pub const ADAPTER_ID: &str = "staged@1";

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

/// Executes the frozen `staged@1` verification contract over pre-loaded staged `extraction.toml`
/// bytes and `page_files` against `original_bytes`.
///
/// # Errors
///
/// Returns [`ExtractError::Missing`] when staged metadata is absent, malformed, or does not match
/// `original_bytes` or `page_files`.
pub fn verify_staged(
    original_bytes: &[u8],
    toml_bytes: Option<&[u8]>,
    mut page_files: BTreeMap<String, Vec<u8>>,
) -> Result<VerifiedExtraction, ExtractError> {
    let raw_toml = toml_bytes.ok_or(ExtractError::Missing("staging folder"))?;
    let (orig_sha, stems) = expected_page_stems(original_bytes, Some(raw_toml))?;
    let toml_text = std::str::from_utf8(raw_toml)
        .map_err(|_| ExtractError::Missing("matching staging folder"))?;
    let record = ExtractionToml::parse(toml_text)
        .map_err(|_| ExtractError::Missing("matching staging folder"))?;

    if page_files.len() != stems.len() {
        return Err(ExtractError::Missing("matching staging folder"));
    }

    let mut pages = Vec::with_capacity(stems.len());
    let mut verified_files = BTreeMap::new();
    for (idx, stem) in stems.into_iter().enumerate() {
        let expected_hash = record
            .pages
            .get(&stem)
            .ok_or(ExtractError::Missing("matching staging folder"))?;
        let bytes = page_files
            .remove(&stem)
            .ok_or(ExtractError::Missing("matching staging folder"))?;
        if sha256_hex(&bytes) != *expected_hash {
            return Err(ExtractError::Missing("matching staging folder"));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| ExtractError::Missing("matching staging folder"))?
            .to_owned();
        let num =
            u32::try_from(idx + 1).map_err(|_| ExtractError::Missing("matching staging folder"))?;
        pages.push(PageText { number: num, text });
        verified_files.insert(stem, bytes);
    }

    if thin_page_numbers(&pages) != record.thin_pages {
        return Err(ExtractError::Missing("matching staging folder"));
    }

    if let ExtractionMethod::Known(method) = &record.method {
        if record.method_data.is_some() && !method.keys.contains(&"method_data") {
            return Err(ExtractError::Missing("matching staging folder"));
        }
        if record
            .extra
            .keys()
            .any(|k| !method.keys.contains(&k.as_str()))
        {
            return Err(ExtractError::Missing("matching staging folder"));
        }
    }

    VerifiedExtraction::verify(&orig_sha, raw_toml, verified_files)
}

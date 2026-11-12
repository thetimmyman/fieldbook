//! Retained implementation of the `plain@1` extractor (published compatibility binding).
//!
//! **Responsibility.** The frozen byte-to-text behavior of `plain@1`: UTF-8 text is one page.
//! A change of these bytes is a new `plain@2` in its own file; this file and its archived vectors
//! stay executable.
//!
//! **Rules.** Pure: no I/O. Input original size is capped at [`MAX_ORIGINAL_BYTES`].

use crate::extraction::{ExtractError, Extraction, PageText};

/// The v1 size cap, frozen; a change is a new version.
pub const MAX_ORIGINAL_BYTES: u64 = 33_554_432;

/// Canonical extraction method name (`plain`).
pub const METHOD: &str = "plain";

/// Extract the single page of text from plain-text original bytes (`plain@1`).
///
/// # Errors
///
/// [`ExtractError::TooLarge`] over the size cap and [`ExtractError::NotText`] for bytes that are
/// not UTF-8.
pub fn extract(bytes: &[u8]) -> Result<Extraction, ExtractError> {
    let len = u64::try_from(bytes.len()).map_err(|_| ExtractError::TooLarge { bytes: u64::MAX })?;
    if len > MAX_ORIGINAL_BYTES {
        return Err(ExtractError::TooLarge { bytes: len });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ExtractError::NotText)?;
    Ok(Extraction {
        method: METHOD,
        pages: vec![PageText {
            number: 1,
            text: text.to_owned(),
        }],
    })
}

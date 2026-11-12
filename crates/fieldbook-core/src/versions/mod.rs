//! Retained, dedicated version implementations for published compatibility bindings.
//!
//! **Responsibility.** Frozen copies of historical adapters and extractors that cannot
//! drift when current implementations evolve. The compatibility runner dispatches
//! historical vectors through these retained versions.
//!
//! **Rules.** Pure: no I/O.

pub mod html_strip_v1;
pub mod plain_v1;
pub mod policy_readers_v1;

use crate::extraction::{ExtractError, Extraction};

/// Retained extractor function signature for published version dispatch.
pub type ExtractorFn = fn(&[u8]) -> Result<Extraction, ExtractError>;

/// Resolves an extractor identifier to its retained function implementation.
///
/// Maps `"plain@1"` and `"html-strip@1"` to their published version implementations.
/// All other identifiers return `None`.
#[must_use]
pub fn extractor_v1(id: &str) -> Option<ExtractorFn> {
    match id {
        "plain@1" => Some(plain_v1::extract),
        "html-strip@1" => Some(html_strip_v1::extract),
        _ => None,
    }
}

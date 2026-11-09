//! Edition metadata for the site: `edition.toml` and the header's edition and date (`build-about`).
//!
//! The value comes from `fieldbook_core::edition`; this module adds no second fingerprint,
//! metadata schema or released predicate. A build with no date writes no edition number.

pub use fieldbook_core::edition::{EditionCounts, EditionIds, EditionInfo};

use crate::meta::RenderContext;

/// Site-relative output path of the edition manifest.
pub const OUTPUT_PATH: &str = "edition.toml";

/// The exact bytes of `edition.toml` for `ctx`.
#[must_use]
pub fn toml_bytes(ctx: &RenderContext) -> Vec<u8> {
    ctx.edition_info().to_toml().into_bytes()
}

/// The build date (`YYYY-MM-DD`) of a released edition, or `None` for an unreleased build.
#[must_use]
pub fn built_date(info: &EditionInfo) -> Option<&str> {
    (!info.built.is_empty()).then_some(info.built.as_str())
}

/// The `edition NNN` text that precedes the date in the header.
#[must_use]
pub fn header_label(info: &EditionInfo) -> String {
    format!("edition {:03}", info.number)
}

//! Shipped-original availability for the citation renderer (`build-source-pages`).
//!
//! **Responsibility.** Tell a citation which shipped original, if any, its version has, so a
//! `p.` or `pp.` citation of a PDF can link to the original at its first page. The availability
//! is built from the same selection ([`crate::source::shipped_originals`]) that copies the
//! originals and names them for `lint`, so a link and a file cannot disagree.
//!
//! **Rules.** Pure: no I/O. A held version, a metadata-only version and a version with no stored
//! original have no entry. The `originals/` folder is not a `Site` key and no planned path
//! names it, so a link built here never depends on the site's path list.

use std::collections::BTreeMap;

use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::Locator;

use crate::frame::root_prefix_for_path;
use crate::source::shipped_originals;

/// Which shipped original each source version has, by version id (`S-0042-v2`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OriginalAvailability(BTreeMap<String, String>);

impl OriginalAvailability {
    /// No version has a shipped original (the input of the earlier `build-page` renderer).
    #[must_use]
    pub const fn empty() -> Self {
        Self(BTreeMap::new())
    }

    /// The availability the build ships for `view`: one entry per stored original that is
    /// copied, keyed by version id, valued by shipped name.
    #[must_use]
    pub fn from_view(view: &LabelledView) -> Self {
        Self(
            shipped_originals(view)
                .iter()
                .map(|o| (o.version_id(), o.shipped_name()))
                .collect(),
        )
    }

    /// Returns `self` with `shipped_name` recorded as the original of `version_id`.
    #[must_use]
    pub fn with(mut self, version_id: &str, shipped_name: &str) -> Self {
        self.0
            .insert(version_id.to_owned(), shipped_name.to_owned());
        self
    }

    /// The shipped name of the original of `version_id`, if it has one.
    #[must_use]
    pub fn shipped_name(&self, version_id: &str) -> Option<&str> {
        self.0.get(version_id).map(String::as_str)
    }

    /// Returns `true` when no version has a shipped original.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The first page a `p.` or `pp.` locator names (`p. 14` is 14, `pp. 14-15` is 14), or `None`
/// for every other locator kind and for text that is not a valid locator.
#[must_use]
pub fn first_pdf_page(locator: &str) -> Option<u32> {
    match Locator::parse(locator).ok()? {
        Locator::Page(page) => Some(page),
        Locator::PageRange { start, .. } => Some(start),
        _ => None,
    }
}

/// The link of a located citation of `version_id` from the page at `from_path`:
/// `../../originals/<shipped name>#page=<first page>` (from a page one folder below the
/// edition root) when the locator is a `p.` or `pp.` locator and the shipped original is a PDF,
/// otherwise `None`, and the citation keeps its `s/` link.
#[must_use]
pub fn located_original_href(
    from_path: &str,
    version_id: &str,
    locator: Option<&str>,
    availability: &OriginalAvailability,
) -> Option<String> {
    let page = first_pdf_page(locator?)?;
    let name = availability.shipped_name(version_id)?;
    let is_pdf = std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));
    is_pdf.then(|| {
        format!(
            "{}../originals/{name}#page={page}",
            root_prefix_for_path(from_path)
        )
    })
}

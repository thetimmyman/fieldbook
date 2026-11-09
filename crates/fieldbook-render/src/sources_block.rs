//! Part 7 (`Sources`) model builder (`sources_block.rs`).
//!
//! **Responsibility.** Build one numbered `Sources` row per [`CitationMarker`] in exact
//! marker reading order (`1..=K`), handling both claim citations (`[@CL-...]`) and direct
//! source-version citations (`[@S-nnnn-v<k> ...]`) without inventing claims, verification
//! statuses, or quotes on direct source rows.

use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::ClaimId;
use fieldbook_core::schema::{ClaimKind, Verification};

use crate::cite::OriginalAvailability;
use crate::inline::{CitationMarker, CitationTarget, find_claim, find_source};
use crate::links::{escape_html, render_citation_chip_with, render_claim_mark};
use crate::site::PlannedPaths;

/// 12×12 stroke check icon (`HANDOFF.md` §7).
pub const ICON_CHECK: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M3.5 8.5 6.5 11.5 12.5 4.5\"/></svg>";

/// 12×12 stroke clock icon (`HANDOFF.md` §7).
pub const ICON_CLOCK: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><circle cx=\"8\" cy=\"8\" r=\"5.5\"/><path d=\"M8 5v3.5l2.5 1.5\"/></svg>";

/// 12×12 stroke alert triangle icon (`HANDOFF.md` §7).
pub const ICON_ALERT: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M8 2.5 14 13H2L8 2.5Z\"/><path d=\"M8 6.5v3\"/><circle cx=\"8\" cy=\"11.2\" r=\"0.6\"/></svg>";

/// 12×12 stroke cross icon (`HANDOFF.md` §7).
pub const ICON_X: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M4.5 4.5 11.5 11.5M11.5 4.5 4.5 11.5\"/></svg>";

/// 12×12 stroke pin icon (`HANDOFF.md` §7).
pub const ICON_PIN: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><circle cx=\"8\" cy=\"6\" r=\"2.5\"/><path d=\"M8 8.5v5\"/></svg>";

/// Return `(variant_class, display_label, icon_svg)` for a claim's [`Verification`] state
/// as specified in `HANDOFF.md` §7.
#[must_use]
pub const fn verification_presentation(
    ver: Verification,
) -> (&'static str, &'static str, &'static str) {
    match ver {
        Verification::PrimarySourceSupported => ("ok", "Primary-source supported", ICON_CHECK),
        Verification::Unchecked => ("warn", "Unchecked", ICON_CLOCK),
        Verification::Stale => ("warn", "Stale", ICON_CLOCK),
        Verification::Unresolved => ("warn", "Unresolved", ICON_ALERT),
        Verification::Contradicted => ("bad", "Contradicted", ICON_X),
    }
}

/// Render a verification `StatusChip` (plus `Assumption` chip when `kind == ClaimKind::Assumption`).
#[must_use]
pub fn render_verification_chips(ver: Verification, kind: ClaimKind) -> String {
    let (variant, label, icon) = verification_presentation(ver);
    let mut out = format!(
        "<span class=\"chip {variant}\">{icon}<span>{}</span></span>",
        escape_html(label)
    );
    if kind == ClaimKind::Assumption {
        out.push_str(&format!(
            " <span class=\"chip info\">{ICON_PIN}<span>Assumption</span></span>"
        ));
    }
    out
}

/// A rendered claim row inside Part 7 (`Sources`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimSourceRow {
    /// Claim identifier (`CL-...`).
    pub claim_id: String,
    /// Rendered `ClaimMark` HTML (`<a>` if the claim's part page `claims/<AREA>-<part>.html` is planned, `<span>` otherwise).
    pub claim_mark_html: String,
    /// Rendered verification `StatusChip` HTML.
    pub status_chip_html: String,
    /// Rendered `CitationChip` HTML (source version + locator, or `No source yet`).
    pub citation_chip_html: String,
    /// Claim statement text.
    pub statement: String,
    /// Optional verbatim quote from the claim row.
    pub quote: Option<String>,
    /// Optional verified date (`YYYY-MM-DD`).
    pub verified_on: Option<String>,
    /// Optional role that verified the claim (`owner`, `lead`).
    pub verified_by_role: Option<String>,
}

/// A rendered direct source-version row inside Part 7 (`Sources`).
///
/// Carries the `CitationChip` and available source metadata from `LabelledView::sources`,
/// without inventing a claim, verification status, or quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectVersionSourceRow {
    /// Rendered `CitationChip` HTML (`S-nnnn-v<k>` and optional locator).
    pub citation_chip_html: String,
    /// Source title from `sources.csv` when present in the view.
    pub title: Option<String>,
    /// Source publisher from `sources.csv` when present in the view.
    pub publisher: Option<String>,
    /// Source `as_of` date from `sources.csv` when present in the view.
    pub as_of: Option<String>,
    /// Source `published` date from `sources.csv` when present in the view.
    pub published: Option<String>,
}

/// Variant of a numbered row in Part 7 (`Sources`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRowKind {
    /// Row backed by a claim citation (`[@CL-...]`).
    Claim(ClaimSourceRow),
    /// Row backed by a direct source-version citation (`[@S-nnnn-v<k> ...]`).
    Version(DirectVersionSourceRow),
}

/// One numbered `<li id="cite-{number}">` entry in Part 7 (`Sources`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberedSourceRow {
    /// 1-based marker number matching `<a class="cite-n" href="#cite-{number}">`.
    pub number: usize,
    /// Claim or direct source-version row payload.
    pub kind: SourceRowKind,
}

/// Build the ordered list of [`NumberedSourceRow`]s for Part 7 (`Sources`) from `markers`.
///
/// `availability` says which source versions have a shipped original, so a `p.` or `pp.`
/// citation of a PDF links to the original at its first page.
#[must_use]
pub fn build_sources_rows(
    from_path: &str,
    markers: &[CitationMarker],
    view: &LabelledView,
    planned: &PlannedPaths,
    availability: &OriginalAvailability,
) -> Vec<NumberedSourceRow> {
    let mut rows = Vec::with_capacity(markers.len());
    for marker in markers {
        let kind = match &marker.target {
            CitationTarget::Claim(claim_id) => SourceRowKind::Claim(build_claim_row(
                from_path,
                claim_id,
                view,
                planned,
                availability,
            )),
            CitationTarget::Version {
                version_id,
                locator,
            } => {
                let source_row = find_source(view, &version_id.source(), version_id.version());
                let citation_chip_html = render_citation_chip_with(
                    from_path,
                    version_id.as_str(),
                    locator.as_deref(),
                    source_row,
                    planned,
                    availability,
                );
                SourceRowKind::Version(DirectVersionSourceRow {
                    citation_chip_html,
                    title: source_row.map(|s| s.title.clone()),
                    publisher: source_row.map(|s| s.publisher.clone()),
                    as_of: source_row.map(|s| s.as_of.to_string()),
                    published: source_row
                        .and_then(|s| s.published.as_ref().map(ToString::to_string)),
                })
            }
        };
        rows.push(NumberedSourceRow {
            number: marker.number,
            kind,
        });
    }
    rows
}

/// Builds the citation chip of a claim row from the claim's own source version and locator.
pub(crate) fn claim_citation_chip(
    from_path: &str,
    claim: Option<&fieldbook_core::schema::ClaimRow>,
    view: &LabelledView,
    planned: &PlannedPaths,
    availability: &OriginalAvailability,
) -> String {
    match claim.and_then(|c| c.source_id.as_ref().zip(c.version)) {
        Some((sid, ver_num)) => {
            let vid = format!("{sid}-v{ver_num}");
            let source_row = find_source(view, sid, ver_num);
            render_citation_chip_with(
                from_path,
                &vid,
                claim.and_then(|c| c.locator.as_deref()),
                source_row,
                planned,
                availability,
            )
        }
        None => "<span class=\"cite neutral\">No source yet</span>".to_owned(),
    }
}

fn build_claim_row(
    from_path: &str,
    claim_id: &ClaimId,
    view: &LabelledView,
    planned: &PlannedPaths,
    availability: &OriginalAvailability,
) -> ClaimSourceRow {
    let claim = find_claim(view, claim_id);

    let ver = claim.map_or(Verification::Unchecked, |c| c.verification);
    let claim_kind = claim.map_or(ClaimKind::Fact, |c| c.kind);
    let (variant, ver_label, icon_svg) = verification_presentation(ver);
    let claim_mark_html =
        render_claim_mark(from_path, claim_id, variant, ver_label, icon_svg, planned);
    let status_chip_html = render_verification_chips(ver, claim_kind);

    let citation_chip_html = claim_citation_chip(from_path, claim, view, planned, availability);

    ClaimSourceRow {
        claim_id: claim_id.to_string(),
        claim_mark_html,
        status_chip_html,
        citation_chip_html,
        statement: claim.map_or_else(String::new, |c| c.statement.clone()),
        quote: claim.and_then(|c| c.quote.clone()),
        verified_on: claim.and_then(|c| c.verified_on.map(|d| d.to_string())),
        verified_by_role: claim.and_then(|c| c.verified_by_role.map(|r| r.as_str().to_owned())),
    }
}

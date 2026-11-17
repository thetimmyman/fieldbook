//! Source pages (`s/<version-id>.html`) and the shipped-original selection (`build-source-pages`).
//!
//! **Responsibility.** Decide, in one pure place, which source versions get an `s/` page and
//! which stored original each one ships as. The same selection feeds the page links, the
//! `OriginalNames` given to `lint` and the copy made by `build`, so a link and a file cannot
//! disagree. Then render each page as the `SourceDrawer` laid out as a page: the register row,
//! the original, the claims and pages that cite the version, and an `Unavailable` panel where
//! the extract and its locators do not exist yet.
//!
//! **Rules.** Pure: no I/O. A held (`quarantined`) version has no page, no shipped original and
//! no link. A metadata-only version has a page and no original.

use std::collections::{BTreeMap, BTreeSet};

use askama::Template;
use fieldbook_core::check::quotes::{read_extraction, read_pages};
use fieldbook_core::check::{Entry, Located};
use fieldbook_core::cite::KeyKind;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::extraction::{methods, section_anchor};
use fieldbook_core::ids::VersionId;
use fieldbook_core::quote;
use fieldbook_core::schema::{Lifecycle, Register, SourceRow};
use fieldbook_core::strings::EXTRACTION_SUPPLIED;

use crate::cite::{OriginalAvailability, located_original_href};
use crate::frame::render_page as render_frame_page;
use crate::frame::{Crumb, FrameInput, StoreContext, root_prefix_for_path};
use crate::inline::find_claim;
use crate::links::{
    escape_html, render_citation_chip_with, render_claim_chip, render_page_chip,
    render_version_chip,
};
use crate::lint::OriginalNames;
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId, format_not_available};
use crate::site::PlannedPaths;
use crate::sources_block::{ICON_CLOCK, render_verification_chips};

/// Folder of stored public originals, relative to the repository root.
pub const PUBLIC_ORIGINALS_PREFIX: &str = "corpus/sources/public/";

/// Prefix of a `location` that records no stored bytes.
const METADATA_ONLY_PREFIX: &str = "metadata-only:";

/// Extensions whose originals ship with `.txt` appended, so Edge shows them as text and runs
/// nothing (Renderable originals, control ED-02).
const RENDERABLE_EXTENSIONS: [&str; 4] = ["html", "htm", "svg", "xhtml"];

/// 12×12 stroke lock icon for the metadata-only variants (`HANDOFF.md` section 11).
pub const ICON_LOCK: &str = "<svg class=\"ic\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><rect x=\"5\" y=\"10.5\" width=\"14\" height=\"10\" rx=\"1.5\"/><path d=\"M8 10.5V8a4 4 0 0 1 8 0v2.5\"/></svg>";

/// 16×16 external link icon for official publisher links on `s/` pages (`HANDOFF.md` section 5 `LocatorView`).
pub const ICON_EXT: &str = "<svg class=\"ic\" viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M9 3h4v4M13 3L7 9M12 9v3a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3\"/></svg>";

/// One official publisher rule for `s/` location and section-locator links (`source-location-links`).
pub struct PublisherRule {
    /// The URL host to match (ASCII case-insensitive), such as `"www.ecfr.gov"`.
    pub host: &'static str,
    /// The short host label shown on section-locator links, such as `"ecfr.gov"`.
    pub display_host: &'static str,
    /// Turns the version's `location` URL and a section locator (`§`, `sec.`, `para.`) into a
    /// section URL on the publisher's site, or `None` when the locator is not a section locator.
    pub section_url: fn(location: &str, locator: &str) -> Option<String>,
}

/// The fixed publisher rule table for `s/` pages (`HANDOFF.md` section 5 `LocatorView`, `WB-02`).
pub const PUBLISHERS: &[PublisherRule] = &[
    PublisherRule {
        host: "www.ecfr.gov",
        display_host: "ecfr.gov",
        section_url: ecfr_section_url,
    },
    PublisherRule {
        host: "ecfr.gov",
        display_host: "ecfr.gov",
        section_url: ecfr_section_url,
    },
];

fn https_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains(':') {
        return None;
    }
    Some(authority)
}

fn find_publisher(location: &str) -> Option<&'static PublisherRule> {
    if location.starts_with(METADATA_ONLY_PREFIX) {
        return None;
    }
    let host = https_host(location)?;
    PUBLISHERS
        .iter()
        .find(|rule| rule.host.eq_ignore_ascii_case(host))
}

/// Returns `true` when `url` is an `https:` address whose host is in [`PUBLISHERS`].
#[must_use]
pub fn is_listed_publisher_url(url: &str) -> bool {
    find_publisher(url).is_some()
}

fn parse_section_locator_number(locator: &str) -> Option<&str> {
    let trimmed = locator.trim();
    let rest = if let Some(after) = trimmed.strip_prefix('§') {
        after
    } else if trimmed
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("sec."))
    {
        trimmed.get(4..)?
    } else if trimmed
        .get(..5)
        .is_some_and(|p| p.eq_ignore_ascii_case("para."))
    {
        trimmed.get(5..)?
    } else {
        return None;
    };
    let section = rest.trim().split('(').next().unwrap_or("").trim();
    if section.is_empty()
        || !section
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return None;
    }
    Some(section)
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn is_title_segment(s: &str) -> bool {
    s.strip_prefix("title-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
}

/// Builds the canonical eCFR section address from a register location: the date (or `current`)
/// and `title-N` come from the location; any other shape gives no link.
fn ecfr_section_url(location: &str, locator: &str) -> Option<String> {
    let section = parse_section_locator_number(locator)?;
    let after_scheme = location.strip_prefix("https://")?;
    let path_start = after_scheme.find('/')?;
    let path = after_scheme[path_start..]
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    let segments: Vec<&str> = path.split('/').skip(1).collect();
    let (when, title) = match segments.as_slice() {
        ["api", "renderer", "v1", "content", "enhanced", date, title] if is_iso_date(date) => {
            (format!("on/{date}"), *title)
        }
        ["current", title, ..] => ("current".to_owned(), *title),
        ["on", date, title, ..] if is_iso_date(date) => (format!("on/{date}"), *title),
        _ => return None,
    };
    if !is_title_segment(title) {
        return None;
    }
    Some(format!(
        "https://www.ecfr.gov/{when}/{title}/section-{section}"
    ))
}

/// Returns the name an original ships under: `<version-id>.<ext>`, with `.txt` appended exactly
/// when the last dot segment of `ext` is one of the extensions in Renderable originals (`html`,
/// `htm`, `svg`, `xhtml`, any case), so `x.html` ships as text like `html` does. Every other
/// extension keeps its name. `ext` has no leading dot.
#[must_use]
pub fn shipped_name(version_id: &str, ext: &str) -> String {
    let last = ext.rsplit('.').next().unwrap_or(ext);
    if RENDERABLE_EXTENSIONS
        .iter()
        .any(|renderable| last.eq_ignore_ascii_case(renderable))
    {
        format!("{version_id}.{ext}.txt")
    } else {
        format!("{version_id}.{ext}")
    }
}

/// Returns `true` when `row` is a held version (`lifecycle` of `quarantined`).
#[must_use]
pub fn is_held(row: &SourceRow) -> bool {
    row.lifecycle == Lifecycle::Quarantined
}

fn is_metadata_only(row: &SourceRow) -> bool {
    row.location.starts_with(METADATA_ONLY_PREFIX)
}

fn version_of(row: &SourceRow) -> Option<VersionId> {
    VersionId::parse(&format!("{}-v{}", row.source_id, row.version)).ok()
}

/// The held version ids of `view`, sorted. `build` prints one `held <version-id>` line for each.
#[must_use]
pub fn held_versions(view: &LabelledView) -> Vec<VersionId> {
    let mut out: Vec<VersionId> = view
        .sources()
        .iter()
        .filter(|s| is_held(&s.value))
        .filter_map(|s| version_of(&s.value))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The source version rows that get an `s/` page: every row that is not held.
#[must_use]
pub fn page_versions(view: &LabelledView) -> Vec<VersionId> {
    let mut out: Vec<VersionId> = view
        .sources()
        .iter()
        .filter(|s| !is_held(&s.value))
        .filter_map(|s| version_of(&s.value))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// One stored original that ships: its stored file, its shipped name and its row hash.
#[derive(Debug, Clone, Copy)]
pub struct StoredOriginal<'a> {
    /// The source version the original belongs to.
    pub version: &'a SourceRow,
    /// File name under `corpus/sources/public/` (`S-0042-v2.pdf`).
    pub file_name: &'a str,
    /// The tree entry for that file: its hash, and its bytes when the tree keeps them.
    pub entry: &'a Entry,
}

impl StoredOriginal<'_> {
    /// The version id (`S-0042-v2`).
    #[must_use]
    pub fn version_id(&self) -> String {
        format!("{}-v{}", self.version.source_id, self.version.version)
    }

    /// The file extension after the version id (`pdf`, `html`, `tar.gz`).
    #[must_use]
    pub fn extension(&self) -> &str {
        self.file_name.split_once('.').map_or("", |(_, ext)| ext)
    }

    /// The name under which the original ships in `originals/`.
    #[must_use]
    pub fn shipped_name(&self) -> String {
        shipped_name(&self.version_id(), self.extension())
    }

    /// The path of the stored file relative to the repository root.
    #[must_use]
    pub fn stored_path(&self) -> String {
        format!("{PUBLIC_ORIGINALS_PREFIX}{}", self.file_name)
    }

    /// The `sha256` the version's row records.
    #[must_use]
    pub fn row_sha256(&self) -> &str {
        &self.version.sha256
    }
}

/// Selects the stored originals that ship: for each version row that is not held and not
/// metadata-only, the file under `corpus/sources/public/` named for that version.
///
/// This one selection links a page to an original, names the originals for `lint`, and tells
/// `build` which files to copy. It reads only the tree of `view`, so a label the view left out
/// never reaches it.
#[must_use]
pub fn shipped_originals(view: &LabelledView) -> Vec<StoredOriginal<'_>> {
    let mut files: BTreeMap<VersionId, Vec<(&str, &Entry)>> = BTreeMap::new();
    for (path, entry) in view.tree().iter() {
        let Some(file_name) = path.strip_prefix(PUBLIC_ORIGINALS_PREFIX) else {
            continue;
        };
        if file_name.contains('/') {
            continue;
        }
        let Some((stem, ext)) = file_name.split_once('.') else {
            continue;
        };
        if ext.is_empty() {
            continue;
        }
        if let Ok(vid) = VersionId::parse(stem) {
            files.entry(vid).or_default().push((file_name, entry));
        }
    }

    let mut out = Vec::new();
    for located in view.sources() {
        let row = &located.value;
        if is_held(row) || is_metadata_only(row) {
            continue;
        }
        let Some(vid) = version_of(row) else {
            continue;
        };
        let Some(candidates) = files.get(&vid) else {
            continue;
        };
        let chosen = candidates
            .iter()
            .find(|(name, _)| *name == row.location)
            .or_else(|| candidates.first());
        if let Some(&(file_name, entry)) = chosen {
            out.push(StoredOriginal {
                version: row,
                file_name,
                entry,
            });
        }
    }
    out.sort_by_key(StoredOriginal::shipped_name);
    out
}

/// The shipped file names `lint` accepts as `originals/` targets: one per [`shipped_originals`].
#[must_use]
pub fn original_names(view: &LabelledView) -> OriginalNames {
    OriginalNames(
        shipped_originals(view)
            .iter()
            .map(StoredOriginal::shipped_name)
            .collect(),
    )
}

/// The `s/` page path of a version.
#[must_use]
pub fn page_path(version_id: &str) -> String {
    format!("s/{version_id}.html")
}

/// The path from a page under `s/` to a shipped original in the sibling `originals/` folder.
fn original_href(from_path: &str, shipped: &str) -> String {
    format!("{}../originals/{shipped}", root_prefix_for_path(from_path))
}

/// Inputs and text of the `Unavailable` panel (`parts/unavailable.html`).
#[derive(Template)]
#[template(path = "parts/unavailable.html")]
struct UnavailableTemplate<'a> {
    line: &'a str,
    lock_icon: &'a str,
}

/// Renders the `Unavailable` panel: a dashed panel with a lock and the frozen
/// `not_available` line for `thing` and `until`. `build-charts` reuses it.
#[must_use]
pub fn render_unavailable(thing: &str, until: &str) -> String {
    let line = format_not_available(thing, until);
    UnavailableTemplate {
        line: &line,
        lock_icon: ICON_LOCK,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("unavailable.html is infallible: {e}"))
}

/// Inputs and text of the `Supplied` provenance note (`parts/supplied-note.html`).
#[derive(Template)]
#[template(path = "parts/supplied-note.html")]
struct SuppliedNoteTemplate<'a> {
    text: &'a str,
}

/// Renders the note on a source page whose text was supplied by hand.
#[must_use]
pub fn render_supplied_note() -> String {
    SuppliedNoteTemplate {
        text: EXTRACTION_SUPPLIED,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("supplied-note.html is infallible: {e}"))
}

struct LocLineView {
    anchors: Vec<String>,
    line_no: usize,
    text_html: String,
}

struct LocPageView {
    page_num: u32,
    lines: Vec<LocLineView>,
}

#[derive(Template)]
#[template(path = "parts/locator-view.html")]
struct LocatorViewTemplate<'a> {
    version_id: &'a str,
    pages_sub: &'a str,
    thin_note: Option<String>,
    pages: Vec<LocPageView>,
}

struct LineInfo<'a> {
    line_no: usize,
    display_text: &'a str,
    start_byte: usize,
    end_byte: usize,
}

fn page_lines(text: &str) -> Vec<LineInfo<'_>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for (line_no, segment) in (1..).zip(text.split_inclusive('\n')) {
        let raw = segment.strip_suffix('\n').unwrap_or(segment);
        let disp = raw.strip_suffix('\r').unwrap_or(raw);
        lines.push(LineInfo {
            line_no,
            display_text: disp,
            start_byte: offset,
            end_byte: offset + disp.len(),
        });
        offset += segment.len();
    }
    lines
}

fn match_section_prefix(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix('§') {
        return Some(rest);
    }
    if line
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("section "))
    {
        return line.get(8..);
    }
    if line
        .get(..5)
        .is_some_and(|p| p.eq_ignore_ascii_case("sec. "))
    {
        return line.get(5..);
    }
    if line
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("sec "))
    {
        return line.get(4..);
    }
    None
}

fn line_matches_section_label(line: &str, label: &str) -> bool {
    let trimmed = line.trim();
    let Some(rest) = match_section_prefix(trimmed) else {
        return false;
    };
    let Some(token) = rest.split_whitespace().next() else {
        return false;
    };
    let token_clean = token.trim_end_matches(['.', ':', ',']);
    token_clean == label
}

fn render_line(
    line: &LineInfo<'_>,
    anchors: &[String],
    merged_ranges: &[std::ops::Range<usize>],
) -> LocLineView {
    let mut line_marks: Vec<std::ops::Range<usize>> = Vec::new();
    for r in merged_ranges {
        let mark_start = r.start.max(line.start_byte).min(line.end_byte);
        let mark_end = r.end.max(line.start_byte).min(line.end_byte);
        if mark_start < mark_end {
            let rel_start = mark_start - line.start_byte;
            let rel_end = mark_end - line.start_byte;
            if line.display_text.is_char_boundary(rel_start)
                && line.display_text.is_char_boundary(rel_end)
            {
                line_marks.push(rel_start..rel_end);
            }
        }
    }

    let mut text_html = String::new();
    let mut cursor = 0;
    for m in line_marks {
        if cursor < m.start
            && let Some(unmarked) = line.display_text.get(cursor..m.start)
        {
            text_html.push_str(&escape_html(unmarked));
        }
        if let Some(marked) = line.display_text.get(m.start..m.end) {
            text_html.push_str("<mark>");
            text_html.push_str(&escape_html(marked));
            text_html.push_str("</mark>");
        }
        cursor = m.end;
    }
    if cursor < line.display_text.len()
        && let Some(rest) = line.display_text.get(cursor..)
    {
        text_html.push_str(&escape_html(rest));
    }

    LocLineView {
        anchors: anchors.to_vec(),
        line_no: line.line_no,
        text_html,
    }
}

fn render_locator_view(
    vid_str: &str,
    record: &fieldbook_core::extraction::ExtractionToml,
    pages: &[&str],
    merged_passages: &BTreeMap<u32, Vec<std::ops::Range<usize>>>,
) -> String {
    let pages_sub = if record.page_count == 1 {
        "1 page".to_owned()
    } else {
        format!("{} pages", record.page_count)
    };

    let thin_note = if !record.thin_pages.is_empty() {
        let pages_list = record
            .thin_pages
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        if record.thin_pages.len() == 1 {
            Some(format!(
                "1 page has little or no text; read the original. Page {pages_list}."
            ))
        } else {
            Some(format!(
                "{} pages have little or no text; read the original. Pages {pages_list}.",
                record.thin_pages.len()
            ))
        }
    } else {
        None
    };

    let mut all_page_lines: Vec<Vec<LineInfo<'_>>> = Vec::with_capacity(pages.len());
    for &p_text in pages {
        all_page_lines.push(page_lines(p_text));
    }

    let mut line_anchors: BTreeMap<(u32, usize), Vec<String>> = BTreeMap::new();
    let mut emitted_anchors: BTreeSet<String> = BTreeSet::new();

    for (label, loc_pages) in &record.locators {
        let anchor_id = section_anchor(label);
        if !emitted_anchors.insert(anchor_id.clone()) {
            continue;
        }
        let first_page = loc_pages.first;
        if first_page < 1 {
            continue;
        }
        let Ok(page_idx) = usize::try_from(first_page.saturating_sub(1)) else {
            continue;
        };
        let Some(page_lines_for_first) = all_page_lines.get(page_idx) else {
            continue;
        };
        if page_lines_for_first.is_empty() {
            continue;
        }

        let mut matched_line_no = None;
        for line in page_lines_for_first {
            if line_matches_section_label(line.display_text, label) {
                matched_line_no = Some(line.line_no);
                break;
            }
        }

        let target_line_no = matched_line_no.unwrap_or(1);
        line_anchors
            .entry((first_page, target_line_no))
            .or_default()
            .push(anchor_id);
    }

    let mut loc_pages = Vec::with_capacity(all_page_lines.len());
    for (page_idx, plines) in all_page_lines.iter().enumerate() {
        let page_num = u32::try_from(page_idx + 1).unwrap_or(u32::MAX);
        let page_merged = merged_passages
            .get(&page_num)
            .map_or(&[][..], |v| v.as_slice());
        let mut rendered_lines = Vec::with_capacity(plines.len());
        for line in plines {
            let anchors = line_anchors
                .get(&(page_num, line.line_no))
                .map_or(&[][..], |v| v.as_slice());
            rendered_lines.push(render_line(line, anchors, page_merged));
        }
        loc_pages.push(LocPageView {
            page_num,
            lines: rendered_lines,
        });
    }

    LocatorViewTemplate {
        version_id: vid_str,
        pages_sub: &pages_sub,
        thin_note,
        pages: loc_pages,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("locator-view.html is infallible: {e}"))
}

/// One claim that cites the version.
struct CitingClaimView {
    claim_chip_html: String,
    status_chips_html: String,
    /// The `CitationChip` of the claim's own version and locator.
    citation_chip_html: String,
    /// `Effective <date>`, `No effective date` or `vN superseded`.
    effective: String,
    statement: String,
    excerpt_html: Option<String>,
    verified_on: Option<String>,
    verified_role: Option<String>,
}

/// One page that cites the version.
struct CitingPageView {
    chip_html: String,
    title: String,
    /// The `CitationChip` of each direct citation of the version on that page that has a locator.
    locator_chips_html: Vec<String>,
}

/// One sibling version of the same source.
struct SiblingView {
    chip_html: String,
    is_current: bool,
    is_superseded: bool,
}

/// External publisher link shown beside a section locator in the Original panel.
struct PublisherLocatorLink {
    href: String,
    host: &'static str,
}

/// One distinct locator that cites the version, shown in the Original panel.
struct OriginalLocatorView {
    text: String,
    original_href: Option<String>,
    publisher_link: Option<PublisherLocatorLink>,
}

/// One non-empty cell of the register row.
struct RowCellView {
    name: &'static str,
    html: String,
    ext_href: Option<String>,
    is_mono: bool,
}

struct SourcePageView {
    version_id: String,
    version: u32,
    title: String,
    publisher: String,
    is_metadata_only: bool,
    is_superseded: bool,
    superseded_by_html: Vec<String>,
    original_html: Option<String>,
    /// The distinct locators that cite the version, shown beside the link to the original.
    original_locators: Vec<OriginalLocatorView>,
    cells: Vec<RowCellView>,
    siblings: Vec<SiblingView>,
    claims: Vec<CitingClaimView>,
    pages: Vec<CitingPageView>,
    excerpt_html: String,
    locator_view_html: String,
    supplied_note_html: String,
    answer_meta_html: String,
    lock_icon: &'static str,
    clock_icon: &'static str,
    ext_icon: &'static str,
}

#[derive(Template)]
#[template(path = "source.html")]
struct SourceTemplate<'a> {
    v: &'a SourcePageView,
}

fn find_located<'a>(view: &'a LabelledView, vid: &VersionId) -> Option<&'a Located<SourceRow>> {
    view.sources()
        .iter()
        .find(|s| version_of(&s.value).as_ref() == Some(vid))
}

/// Versions of the same source that are not held, sorted by version number.
fn sibling_versions(view: &LabelledView, row: &SourceRow) -> Vec<VersionId> {
    let mut versions: Vec<(u32, VersionId)> = view
        .sources()
        .iter()
        .filter(|s| s.value.source_id == row.source_id && !is_held(&s.value))
        .filter_map(|s| version_of(&s.value).map(|v| (s.value.version, v)))
        .collect();
    versions.sort();
    versions.dedup();
    versions.into_iter().map(|(_, v)| v).collect()
}

/// Versions that supersede `vid` (another row's `supersedes` names it), never a held one.
fn superseded_by(view: &LabelledView, vid: &VersionId) -> Vec<VersionId> {
    let mut out: Vec<VersionId> = view
        .sources()
        .iter()
        .filter(|s| !is_held(&s.value) && s.value.supersedes.contains(vid))
        .filter_map(|s| version_of(&s.value))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn claim_cites(view: &LabelledView, claim_id: &fieldbook_core::ids::ClaimId, vid: &str) -> bool {
    find_claim(view, claim_id)
        .and_then(|c| c.source_id.as_ref().zip(c.version))
        .is_some_and(|(sid, ver)| format!("{sid}-v{ver}") == vid)
}

fn build_view(
    located: &Located<SourceRow>,
    view: &LabelledView,
    planned: &PlannedPaths,
    avail: &OriginalAvailability,
) -> Option<SourcePageView> {
    let row = &located.value;
    let vid = version_of(row)?;
    let vid_str = vid.as_str().to_owned();
    let from_path = page_path(&vid_str);
    let metadata_only = is_metadata_only(row);

    let superseders = superseded_by(view, &vid);
    let superseded_by_html = superseders
        .iter()
        .map(|v| {
            render_version_chip(
                &from_path,
                v,
                find_located(view, v).map(|l| &l.value),
                planned,
            )
        })
        .collect();

    let original_html = avail.shipped_name(&vid_str).map(|name| {
        format!(
            "<a class=\"cite\" href=\"{}\"><span>{}</span></a>",
            escape_html(&original_href(&from_path, name)),
            escape_html(name)
        )
    });

    let columns = Register::Sources.columns();
    let cells = view
        .register_rows(Register::Sources)
        .iter()
        .find(|r| r.line == located.line)
        .map(|raw| {
            columns
                .iter()
                .zip(raw.cells.iter())
                .filter(|(_, text)| !text.is_empty())
                .map(|(col, text)| row_cell(col.name, text, &from_path, view, planned))
                // A cell that names only versions the view leaves out shows nothing, and is
                // left out like an empty cell, so the page is the same with or without them.
                .filter(|cell| !cell.html.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let siblings = {
        let all = sibling_versions(view, row);
        if all.len() > 1 {
            all.iter()
                .map(|v| SiblingView {
                    chip_html: render_version_chip(
                        &from_path,
                        v,
                        find_located(view, v).map(|l| &l.value),
                        planned,
                    ),
                    is_current: *v == vid,
                    is_superseded: !superseded_by(view, v).is_empty(),
                })
                .collect()
        } else {
            Vec::new()
        }
    };

    let source_row = Some(row);
    let is_superseded = !superseders.is_empty();

    let extraction = read_extraction(view.tree(), &vid);
    let extracted_pages = extraction
        .as_ref()
        .and_then(|rec| read_pages(view.tree(), &vid, 1..=rec.page_count));
    let extracted_info = extraction.as_ref().zip(extracted_pages.as_ref());

    let mut page_passages: BTreeMap<u32, Vec<std::ops::Range<usize>>> = BTreeMap::new();
    let mut cited_locators: BTreeSet<String> = BTreeSet::new();
    let claims = view
        .claims()
        .iter()
        .filter(|c| {
            c.value
                .source_id
                .as_ref()
                .zip(c.value.version)
                .is_some_and(|(sid, ver)| format!("{sid}-v{ver}") == vid_str)
        })
        .map(|c| {
            let claim = &c.value;
            let locator_text = claim.locator.as_deref().filter(|l| !l.is_empty());
            if let Some(loc) = locator_text {
                cited_locators.insert(loc.to_owned());
            }
            let mut excerpt_html = None;
            if let Some((record, pages)) = extracted_info
                && let (Some(loc), Some(quote)) = (
                    locator_text,
                    claim.quote.as_deref().filter(|q| !q.is_empty()),
                )
                && let Ok(Some(range)) = quote::locator_resolves(record, loc)
                && let Some(span) = quote::find(quote, pages, range)
            {
                page_passages
                    .entry(span.page)
                    .or_default()
                    .push(span.start..span.end);
                if let Ok(p_idx) = usize::try_from(span.page.saturating_sub(1))
                    && let Some(p_text) = pages.get(p_idx)
                    && let Some(matched_raw) = p_text.get(span.start..span.end)
                {
                    let collapsed = matched_raw.split_whitespace().collect::<Vec<_>>().join(" ");
                    let char_count = collapsed.chars().count();
                    let passage = if char_count > 150 {
                        let prefix: String = collapsed.chars().take(150).collect();
                        format!("{prefix}…")
                    } else {
                        collapsed
                    };
                    excerpt_html = Some(format!(
                        "<blockquote class=\"excerpt\"><mark>{}</mark></blockquote>",
                        escape_html(&passage)
                    ));
                }
            }
            let effective = if is_superseded {
                format!("v{} superseded", row.version)
            } else {
                claim.effective_from.map_or_else(
                    || "No effective date".to_owned(),
                    |d| format!("Effective {d}"),
                )
            };
            CitingClaimView {
                claim_chip_html: render_claim_chip(&from_path, &claim.claim_id, planned),
                status_chips_html: render_verification_chips(claim.verification, claim.kind),
                citation_chip_html: render_citation_chip_with(
                    &from_path,
                    &vid_str,
                    locator_text,
                    source_row,
                    planned,
                    avail,
                ),
                effective,
                statement: claim.statement.clone(),
                excerpt_html,
                verified_on: claim.verified_on.map(|d| d.to_string()),
                verified_role: claim
                    .verified_on
                    .and(claim.verified_by_role)
                    .map(|role| role.as_str().to_owned()),
            }
        })
        .collect::<Vec<_>>();

    let mut cited_pages = BTreeSet::new();
    for page in view.pages() {
        let cites = view.citations(&page.id).iter().any(|c| match c.key_kind {
            KeyKind::Version => c.key == vid_str,
            KeyKind::Claim => fieldbook_core::ids::ClaimId::parse(&c.key)
                .is_ok_and(|cid| claim_cites(view, &cid, &vid_str)),
            _ => false,
        });
        if cites {
            cited_pages.insert(page.id.clone());
        }
    }
    let pages = cited_pages
        .iter()
        .filter_map(|pid| {
            view.pages().iter().find(|p| p.id == *pid).map(|p| {
                let mut seen = BTreeSet::new();
                let locator_chips_html = view
                    .citations(&p.id)
                    .iter()
                    .filter(|c| c.key_kind == KeyKind::Version && c.key == vid_str)
                    .filter_map(|c| c.locator.as_ref().map(|l| l.text.as_str()))
                    .filter(|loc| seen.insert((*loc).to_owned()))
                    .map(|loc| {
                        cited_locators.insert(loc.to_owned());
                        render_citation_chip_with(
                            &from_path,
                            &vid_str,
                            Some(loc),
                            source_row,
                            planned,
                            avail,
                        )
                    })
                    .collect();
                CitingPageView {
                    chip_html: render_page_chip(&from_path, &p.id, planned),
                    title: p.meta.title.clone(),
                    locator_chips_html,
                }
            })
        })
        .collect();

    let publisher_rule = find_publisher(&row.location);
    let original_locators = if original_html.is_some() {
        cited_locators
            .iter()
            .map(|loc| {
                let original_href = located_original_href(&from_path, &vid_str, Some(loc), avail);
                let publisher_link = publisher_rule.and_then(|rule| {
                    (rule.section_url)(&row.location, loc).map(|href| PublisherLocatorLink {
                        href,
                        host: rule.display_host,
                    })
                });
                OriginalLocatorView {
                    text: loc.clone(),
                    original_href,
                    publisher_link,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut merged_passages: BTreeMap<u32, Vec<std::ops::Range<usize>>> = BTreeMap::new();
    for (page_num, mut ranges) in page_passages {
        ranges.sort_unstable_by_key(|r| (r.start, r.end));
        let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
        for r in ranges {
            if let Some(last) = merged.last_mut() {
                if r.start <= last.end {
                    last.end = last.end.max(r.end);
                } else {
                    merged.push(r);
                }
            } else {
                merged.push(r);
            }
        }
        merged_passages.insert(page_num, merged);
    }

    // A metadata-only version has no stored bytes to extract, so its banner stands in for the
    // excerpt and the locator view (HANDOFF section 5 `Excerpt`, section 11). An extracted
    // version shows the locator view and suppresses the page-level excerpt panel.
    let (excerpt_html, locator_view_html, supplied_note_html) =
        if let Some((record, pages)) = extracted_info {
            let supplied_note = if record.method.name() == methods::SUPPLIED {
                render_supplied_note()
            } else {
                String::new()
            };
            (
                String::new(),
                render_locator_view(&vid_str, record, pages, &merged_passages),
                supplied_note,
            )
        } else if metadata_only {
            (String::new(), String::new(), String::new())
        } else {
            (
                render_unavailable("Excerpt", "this version is extracted"),
                render_unavailable("Locator view", "this version is extracted"),
                String::new(),
            )
        };
    let answer_meta_html = if claims.is_empty() {
        render_unavailable("Answer metadata", "a claim cites this version")
    } else {
        String::new()
    };

    Some(SourcePageView {
        version_id: vid_str,
        version: row.version,
        title: row.title.clone(),
        publisher: row.publisher.clone(),
        is_metadata_only: metadata_only,
        is_superseded: !superseders.is_empty(),
        superseded_by_html,
        original_html,
        original_locators,
        cells,
        siblings,
        claims,
        pages,
        excerpt_html,
        locator_view_html,
        supplied_note_html,
        answer_meta_html,
        lock_icon: ICON_LOCK,
        clock_icon: ICON_CLOCK,
        ext_icon: ICON_EXT,
    })
}

fn row_cell(
    name: &'static str,
    text: &str,
    from_path: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RowCellView {
    match name {
        "supersedes" => {
            let html = text
                .split('|')
                .map(str::trim)
                .filter_map(|id| VersionId::parse(id).ok())
                .filter_map(|v| {
                    let kept = find_located(view, &v)?;
                    (!is_held(&kept.value))
                        .then(|| render_version_chip(from_path, &v, Some(&kept.value), planned))
                })
                .collect::<Vec<_>>()
                .join(" ");
            RowCellView {
                name,
                html,
                ext_href: None,
                is_mono: false,
            }
        }
        "location" => RowCellView {
            name,
            html: escape_html(text),
            ext_href: find_publisher(text).map(|_| text.to_owned()),
            is_mono: true,
        },
        "sha256" | "source_id" | "version" => RowCellView {
            name,
            html: escape_html(text),
            ext_href: None,
            is_mono: true,
        },
        _ => RowCellView {
            name,
            html: escape_html(text),
            ext_href: None,
            is_mono: false,
        },
    }
}

/// Render the inner `<main>` content (`templates/source.html`) for `located`.
///
/// Returns `None` for a row whose version id does not parse.
#[must_use]
pub fn render_main(
    located: &Located<SourceRow>,
    view: &LabelledView,
    planned: &PlannedPaths,
    avail: &OriginalAvailability,
) -> Option<String> {
    let model = build_view(located, view, planned, avail)?;
    Some(
        SourceTemplate { v: &model }
            .render()
            .unwrap_or_else(|e| unreachable!("source.html is infallible: {e}")),
    )
}

/// Render the complete `s/<version-id>.html` document for the version row `vid`, wrapped in the
/// global site frame. Returns `None` when `vid` has no row in `view` or its row is held.
#[must_use]
pub fn render_page(
    vid: &VersionId,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
    avail: &OriginalAvailability,
) -> Option<String> {
    let located = find_located(view, vid)?;
    if is_held(&located.value) {
        return None;
    }
    let main_html = render_main(located, view, planned, avail)?;
    let path = page_path(vid.as_str());
    let edition = ctx.edition_info();
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let library = match SectionId::Library.first_path() {
        Some(first) => Crumb::linked("Library", first, planned),
        None => Crumb::current("Library"),
    };
    let sources = Crumb::linked("Sources register", "sources.html", planned);
    let crumbs = [library, sources, Crumb::current(vid.as_str())];
    Some(render_frame_page(&FrameInput {
        output_path: &path,
        title: vid.as_str(),
        page_area: None,
        edition,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    }))
}

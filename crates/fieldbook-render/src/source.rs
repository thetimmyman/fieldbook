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
use fieldbook_core::check::{Entry, Located};
use fieldbook_core::cite::KeyKind;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::VersionId;
use fieldbook_core::schema::{Lifecycle, Register, SourceRow};

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

/// One claim that cites the version.
struct CitingClaimView {
    claim_chip_html: String,
    status_chips_html: String,
    /// The `CitationChip` of the claim's own version and locator.
    citation_chip_html: String,
    /// `Effective <date>`, `No effective date` or `vN superseded`.
    effective: String,
    statement: String,
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

/// One non-empty cell of the register row.
struct RowCellView {
    name: &'static str,
    html: String,
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
    original_locators_html: Vec<String>,
    cells: Vec<RowCellView>,
    siblings: Vec<SiblingView>,
    claims: Vec<CitingClaimView>,
    pages: Vec<CitingPageView>,
    excerpt_html: String,
    locator_view_html: String,
    answer_meta_html: String,
    lock_icon: &'static str,
    clock_icon: &'static str,
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

    let original_locators_html = if original_html.is_some() {
        cited_locators
            .iter()
            .map(|loc| {
                let loc_esc = escape_html(loc);
                match located_original_href(&from_path, &vid_str, Some(loc), avail) {
                    Some(href) => format!(
                        "<a class=\"loc\" href=\"{}\">{loc_esc}</a>",
                        escape_html(&href)
                    ),
                    None => format!("<span class=\"loc\">{loc_esc}</span>"),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // A metadata-only version has no stored bytes to extract, so its banner stands in for the
    // excerpt and the locator view (HANDOFF section 5 `Excerpt`, section 11).
    let (excerpt_html, locator_view_html) = if metadata_only {
        (String::new(), String::new())
    } else {
        (
            render_unavailable("Excerpt", "this version is extracted"),
            render_unavailable("Locator view", "this version is extracted"),
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
        original_locators_html,
        cells,
        siblings,
        claims,
        pages,
        excerpt_html,
        locator_view_html,
        answer_meta_html,
        lock_icon: ICON_LOCK,
        clock_icon: ICON_CLOCK,
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
                is_mono: false,
            }
        }
        "sha256" | "location" | "source_id" | "version" => RowCellView {
            name,
            html: escape_html(text),
            is_mono: true,
        },
        _ => RowCellView {
            name,
            html: escape_html(text),
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

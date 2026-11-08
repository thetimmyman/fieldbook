//! The search page, `search.html`, and its script, `assets/search.js` (`search-js`,
//! `docs/reference/search.md#the-site-index`).
//!
//! **Responsibility.** Render the static page that `search.js` fills: the search input, the
//! result list, the fixed lines for an index that did not load or carries another format version,
//! the "Latest pages" list for an empty query, and the line that stands in for search when
//! scripts are blocked, with its link to `ids.html`. The script ranks in the browser; this module
//! ranks nothing.
//!
//! **Rules.** Pure: no I/O. The script is compiled in. The latest pages come from the docs of the
//! edition index, so the page shows the values `search.js` would show for the same units.

use askama::Template;
use fieldbook_core::search::{Index, unit_href, utf16_cmp};

use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::ids;
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::site::PlannedPaths;

/// Site-relative output path of the search page.
pub const OUTPUT_PATH: &str = "search.html";

/// Site-relative output path of the search script.
pub const SCRIPT_PATH: &str = "assets/search.js";

/// The search script, a classic script that stays within 10 KB (`FB0605`, `WB-03`).
pub const SCRIPT: &str = include_str!("../assets/search.js");

/// Page title for `search.html`.
pub const PAGE_TITLE: &str = "Search";

/// How many page leads the empty query lists (HANDOFF section 10).
pub const LATEST_PAGES: usize = 6;

/// One row of the "Latest pages" list, in the form `search.js` renders a result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestRow {
    /// The page id.
    pub id: String,
    /// `page` or `clause`.
    pub kind: String,
    /// The page title.
    pub title: String,
    /// The page's link from the edition root.
    pub href: String,
    /// The lead's snippet.
    pub snippet: String,
    /// The metadata line: kind, status and the dates, joined with ` · `.
    pub meta: String,
}

/// The view model of `search.html`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchView {
    /// The `<h1>`.
    pub title: &'static str,
    /// The link to the ID list.
    pub ids_href: &'static str,
    /// The text of that link.
    pub ids_label: &'static str,
    /// The `src` of the search script.
    pub script_src: &'static str,
    /// The page leads with the latest `as_of`, latest first.
    pub latest: Vec<LatestRow>,
}

#[derive(Template)]
#[template(path = "search.html")]
struct SearchTemplate<'a> {
    view: &'a SearchView,
}

/// The metadata line of a result row, as `search.js` writes it.
fn meta_line(
    kind: &str,
    status: &str,
    as_of: &str,
    effective_from: &str,
    verified_on: &str,
) -> String {
    let mut parts = vec![kind.to_owned()];
    if !status.is_empty() {
        parts.push(status.to_owned());
    }
    for (label, value) in [
        ("as of ", as_of),
        ("effective ", effective_from),
        ("verified ", verified_on),
    ] {
        if !value.is_empty() {
            parts.push(format!("{label}{value}"));
        }
    }
    parts.join(" \u{b7} ")
}

/// The page leads (kind `page` or `clause`, no parent) with the latest `as_of` as written, latest
/// first, ties in unit order, which is natural id order. A lead with no `as_of` is left out.
#[must_use]
pub fn latest_pages(index: &Index) -> Vec<LatestRow> {
    let mut leads: Vec<_> = index
        .docs
        .iter()
        .filter(|d| {
            matches!(d.kind.as_str(), "page" | "clause")
                && d.parent.is_empty()
                && !d.as_of.is_empty()
        })
        .collect();
    leads.sort_by(|a, b| utf16_cmp(&b.as_of, &a.as_of));
    leads
        .into_iter()
        .take(LATEST_PAGES)
        .filter_map(|d| {
            Some(LatestRow {
                href: unit_href(&d.kind, &d.id)?,
                id: d.id.clone(),
                kind: d.kind.clone(),
                title: d.title.clone(),
                snippet: d.snippet.clone(),
                meta: meta_line(
                    &d.kind,
                    &d.status,
                    &d.as_of,
                    &d.effective_from,
                    &d.verified_on,
                ),
            })
        })
        .collect()
}

/// Builds the view of `search.html` over the edition `index`.
#[must_use]
pub fn build_view(index: &Index) -> SearchView {
    SearchView {
        title: PAGE_TITLE,
        ids_href: ids::OUTPUT_PATH,
        ids_label: ids::PAGE_TITLE,
        script_src: SCRIPT_PATH,
        latest: latest_pages(index),
    }
}

/// Renders the inner `<main>` content of `search.html`.
#[must_use]
pub fn render_main(index: &Index) -> String {
    let view = build_view(index);
    SearchTemplate { view: &view }
        .render()
        .unwrap_or_else(|e| unreachable!("search.html is infallible: {e}"))
}

/// Renders the complete `search.html` inside the global site frame.
#[must_use]
pub fn render_page(
    index: &Index,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_main(index);
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let library = match SectionId::Library.first_path() {
        Some(first) => Crumb::linked("Library", first, planned),
        None => Crumb::current("Library"),
    };
    let crumbs = [library, Crumb::current(PAGE_TITLE)];
    render_frame_page(&FrameInput {
        output_path: OUTPUT_PATH,
        title: PAGE_TITLE,
        page_area: None,
        edition: ctx.edition_info(),
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

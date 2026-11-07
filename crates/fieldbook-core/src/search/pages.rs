//! The page view: the ranked unit list grouped for a reader
//! (`docs/reference/search.md#the-page-view`).
//!
//! **Responsibility.** Turn the full [`Results`] of [`query`](super::query) into three lists: exact
//! rows, one row per page, and every other hit. It reads the ranking and never changes it: a list
//! keeps rank order, and a page takes the place of its first hit.
//!
//! **Rules.** Pure: no I/O. A map here is only ever looked up, never iterated, so no output depends
//! on a map's order. Rust is the specification of this view for the JavaScript mirror, which
//! `tests/golden/search/page-view.json` holds to it.

use std::collections::{BTreeMap, BTreeSet};

use super::index::{DocEntry, Index};
use super::query::{Hit, Results};

/// How many rows of each list a surface shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewLimits {
    /// At most this many page rows.
    pub pages: usize,
    /// At most this many other rows after page rows.
    pub other: usize,
    /// At most this many other rows when the view has no page row.
    pub other_alone: usize,
}

impl ViewLimits {
    /// The limits of `fieldbook search`.
    pub const CLI: Self = Self {
        pages: 10,
        other: 5,
        other_alone: 10,
    };
}

/// One page of the view: its best-ranked unit stands for all its matching units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRow<'a> {
    /// The page id: the unit id before any `#`.
    pub page: &'a str,
    /// Whose display values the row shows: the page lead, or the best unit when `has_lead` is
    /// false.
    pub display: &'a DocEntry,
    /// `false` when the index holds no lead unit for this page.
    pub has_lead: bool,
    /// The best-ranked unit of the page.
    pub best: &'a Hit,
    /// The display values of `best`, which give the row its snippet.
    pub best_doc: &'a DocEntry,
    /// How many units of the page matched the query.
    pub matched_units: usize,
}

impl PageRow<'_> {
    /// `true` when the best unit is a section of the page rather than its lead.
    #[must_use]
    pub fn best_is_section(&self) -> bool {
        self.best.id != self.page
    }
}

/// The whole view, with no limit applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageView<'a> {
    /// Exact-class hits, in query order and ungrouped.
    pub exact: Vec<&'a Hit>,
    /// One row per remaining page, in the rank order of each page's best unit.
    pub pages: Vec<PageRow<'a>>,
    /// Every remaining hit whose kind is not `page` or `clause`, in rank order.
    pub other: Vec<&'a Hit>,
    /// Page units left out because an exact row already stands for their page.
    pub absorbed: usize,
    /// Every unit that matched: `exact`, the units of `pages`, `absorbed` and `other` add up to it.
    pub total: usize,
}

/// The part of a [`PageView`] that a surface shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShownRows<'v, 'a> {
    /// Every exact row: a limit never cuts one, so every version of a source id is listed.
    pub exact: &'v [&'a Hit],
    /// The first page rows.
    pub pages: &'v [PageRow<'a>],
    /// The first other rows.
    pub other: &'v [&'a Hit],
}

impl PageView<'_> {
    /// The rows that `limits` leaves. Exact rows are never cut, and other rows get the larger
    /// allowance when the view has no page row.
    #[must_use]
    pub fn shown(&self, limits: ViewLimits) -> ShownRows<'_, '_> {
        let other = if self.pages.is_empty() {
            limits.other_alone
        } else {
            limits.other
        };
        ShownRows {
            exact: &self.exact,
            pages: &self.pages[..limits.pages.min(self.pages.len())],
            other: &self.other[..other.min(self.other.len())],
        }
    }

    /// `true` when `shown` holds fewer rows than this view.
    #[must_use]
    pub fn is_cut_by(&self, shown: &ShownRows<'_, '_>) -> bool {
        shown.pages.len() < self.pages.len() || shown.other.len() < self.other.len()
    }
}

/// `true` for the kinds that group into a page row.
fn groups_into_a_page(kind: &str) -> bool {
    matches!(kind, "page" | "clause")
}

/// The page a unit id belongs to: the part before any `#`.
fn page_of(id: &str) -> &str {
    id.split_once('#').map_or(id, |(page, _)| page)
}

/// Builds the view from `results`, the ranked list that [`query`](super::query) gave over `index`
/// with a limit of every unit. A shorter list gives the view of that list.
#[must_use]
pub fn page_view<'a>(index: &'a Index, results: &'a Results) -> PageView<'a> {
    let exact: Vec<&Hit> = results.hits.iter().filter(|hit| hit.exact).collect();
    let seen: BTreeSet<&str> = exact
        .iter()
        .filter(|hit| groups_into_a_page(&hit.kind))
        .map(|hit| page_of(&hit.id))
        .collect();
    let leads: BTreeMap<&str, &DocEntry> = index
        .docs
        .iter()
        .filter(|doc| groups_into_a_page(&doc.kind) && !doc.id.contains('#'))
        .map(|doc| (doc.id.as_str(), doc))
        .collect();

    let mut pages: Vec<PageRow<'a>> = Vec::new();
    let mut row_of: BTreeMap<&str, usize> = BTreeMap::new();
    let mut other = Vec::new();
    let mut absorbed = 0;
    for hit in results.hits.iter().filter(|hit| !hit.exact) {
        if !groups_into_a_page(&hit.kind) {
            other.push(hit);
            continue;
        }
        let page = page_of(&hit.id);
        if seen.contains(page) {
            absorbed += 1;
        } else if let Some(&row) = row_of.get(page) {
            if let Some(row) = pages.get_mut(row) {
                row.matched_units += 1;
            }
        } else if let Some(best_doc) = index.docs.get(hit.unit) {
            let lead = leads.get(page).copied();
            row_of.insert(page, pages.len());
            pages.push(PageRow {
                page,
                display: lead.unwrap_or(best_doc),
                has_lead: lead.is_some(),
                best: hit,
                best_doc,
                matched_units: 1,
            });
        }
    }

    PageView {
        exact,
        pages,
        other,
        absorbed,
        total: results.total,
    }
}

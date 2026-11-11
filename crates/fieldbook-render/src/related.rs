//! Rail lists for reader pages: Related and Linked from.
//!
//! Computes and renders the `Related` and `Linked from` rail sections
//! for reader pages. Does not render other rail sections such as
//! `On this page`, `Read first`, `Depends on`, or `Cited claims`.

use std::collections::{BTreeMap, BTreeSet};

use fieldbook_core::check::PageEntry;
use fieldbook_core::check::markdown::body_links;
use fieldbook_core::egress::LabelledView;

use crate::inline::{find_page, first_sentence_of_purpose, parse_md_link_target};
use crate::links::{escape_html, render_page_link};
use crate::site::PlannedPaths;

/// Inbound link index mapping target page IDs to sorted inbound page IDs.
#[derive(Debug, Clone, Default)]
pub struct LinkIndex {
    inbound: BTreeMap<String, Vec<String>>,
}

impl LinkIndex {
    /// Build an index of inbound page links across all pages in `view`.
    #[must_use]
    pub fn new(view: &LabelledView) -> Self {
        let mut raw: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for page in view.pages() {
            let source_id = page.id.as_str();

            for target_pid in &page.meta.links {
                let target_id = target_pid.as_str();
                if target_id != source_id && find_page(view, target_id).is_some() {
                    raw.entry(target_id.to_owned())
                        .or_default()
                        .insert(source_id.to_owned());
                }
            }

            for bl in body_links(&page.body, 1) {
                if let Some((target_stem, _frag)) = parse_md_link_target(&bl.target)
                    && target_stem != source_id
                    && find_page(view, target_stem).is_some()
                {
                    raw.entry(target_stem.to_owned())
                        .or_default()
                        .insert(source_id.to_owned());
                }
            }
        }

        let mut inbound = BTreeMap::new();
        for (target_id, sources) in raw {
            let mut list: Vec<String> = sources.into_iter().collect();
            list.sort_by_cached_key(|id| {
                let title_lower =
                    find_page(view, id).map_or_else(String::new, |p| p.meta.title.to_lowercase());
                (title_lower, id.clone())
            });
            inbound.insert(target_id, list);
        }

        Self { inbound }
    }

    /// Return the slice of inbound page IDs linking to `page_id`.
    #[must_use]
    pub fn linked_from(&self, page_id: &str) -> &[String] {
        self.inbound.get(page_id).map_or(&[], Vec::as_slice)
    }
}

const RAIL_LIST_CAP: usize = 10;

fn render_rows<S: AsRef<str>>(
    ids: &[S],
    from_path: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> String {
    let mut out = String::new();
    for id in ids {
        let id_str = id.as_ref();
        if let Some(target_page) = find_page(view, id_str) {
            let title = &target_page.meta.title;
            let pv_body = first_sentence_of_purpose(target_page);
            let plink = render_page_link(
                from_path,
                id_str,
                None,
                &escape_html(title),
                Some(title.as_str()),
                Some(pv_body.as_str()),
                planned,
            );
            out.push_str("<li>");
            out.push_str(&plink);
            out.push_str("</li>");
        }
    }
    out
}

fn render_section<S: AsRef<str>>(
    label: &str,
    empty_line: &str,
    ids: &[S],
    from_path: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> String {
    let label_escaped = escape_html(label);
    if ids.is_empty() {
        let empty_escaped = escape_html(empty_line);
        return format!(
            "<section class=\"rail-sec rail-list\" aria-label=\"{label_escaped}\"><div class=\"rail-head\">{label_escaped}</div><p class=\"rail-empty\">{empty_escaped}</p></section>"
        );
    }

    let count = ids.len();
    let cap = count.min(RAIL_LIST_CAP);
    let (first, rest) = ids.split_at(cap);

    let first_items = render_rows(first, from_path, view, planned);
    let fold = if rest.is_empty() {
        String::new()
    } else {
        let rest_items = render_rows(rest, from_path, view, planned);
        format!(
            "<details class=\"fold\"><summary><b>{} more</b></summary><ul>{rest_items}</ul></details>",
            rest.len()
        )
    };

    format!(
        "<section class=\"rail-sec rail-list\" aria-label=\"{label_escaped}\"><div class=\"rail-head\">{label_escaped} <span class=\"rail-n\">({count})</span></div><ul>{first_items}</ul>{fold}</section>"
    )
}

/// Render the Related and Linked from rail lists for `page`.
#[must_use]
pub fn render_rail_lists(
    page: &PageEntry,
    view: &LabelledView,
    planned: &PlannedPaths,
    links: &LinkIndex,
) -> String {
    let from_path = format!("p/{}.html", page.id);

    let mut seen = BTreeSet::new();
    let mut related_ids = Vec::new();
    for pid in &page.meta.links {
        let id = pid.as_str();
        if find_page(view, id).is_some() && seen.insert(id) {
            related_ids.push(id);
        }
    }

    let related_html = render_section(
        "Related",
        "No related pages listed.",
        &related_ids,
        &from_path,
        view,
        planned,
    );

    let linked_from_ids = links.linked_from(page.id.as_str());
    let linked_from_html = render_section(
        "Linked from",
        "No page links here.",
        linked_from_ids,
        &from_path,
        view,
        planned,
    );

    format!("{related_html}\n{linked_from_html}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_index_default_empty() {
        let idx = LinkIndex::default();
        assert!(idx.linked_from("any-page").is_empty());
    }
}

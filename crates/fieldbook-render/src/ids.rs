//! ID index and list page renderers (`ids.html`, `ids/<slug>.html`, `ids/claims-<AREA>-<part>.html`).
//!
//! **Responsibility.** Render `ids.html` as an index of the seven identifier groups
//! (pages, sources, versions, claims, questions, conflicts, terms), and render dedicated
//! list pages under `ids/`:
//! - Group pages: `ids/pages.html`, `ids/sources.html`, `ids/versions.html`,
//!   `ids/questions.html`, `ids/conflicts.html`, `ids/terms.html`.
//! - Claims part pages: `ids/claims-<AREA>-<part>.html` (e.g. `ids/claims-CMP-1.html`).
//!
//! Identifier chips across all list pages resolve relative to `ids/` via [`LIST_BASE_PATH`].

use std::collections::BTreeMap;

use askama::Template;
use fieldbook_core::check::PageEntry;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::{SourceId, VersionId, claim_part};
use fieldbook_core::schema::{
    ClaimRow, ConflictRow, GlossaryRow, Lifecycle, QuestionRow, SourceRow,
};
use fieldbook_core::strings;

use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::links::{
    href_if_planned, render_claim_chip, render_conflict_chip, render_page_chip,
    render_question_chip, render_source_id_chip, render_term_chip, render_version_chip,
};
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::register;
use crate::site::PlannedPaths;

/// Site-relative output path for the ID index.
pub const OUTPUT_PATH: &str = "ids.html";

/// Page title for `ids.html`.
pub const PAGE_TITLE: &str = "All IDs";

/// Chips of every page under ids/ are relative to this directory.
const LIST_BASE_PATH: &str = "ids/list.html";

/// Slugs of the six group list pages in deterministic order.
const SIX_GROUP_SLUGS: [&str; 6] = [
    "pages",
    "sources",
    "versions",
    "questions",
    "conflicts",
    "terms",
];

/// One entry inside an [`IdGroupView`] or [`IdListPage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdItemView {
    /// Canonical identifier (`cmp-system-review`, `S-0002`, `S-0002-v2`, `CL-CMP-0001`, ...).
    pub id: String,
    /// Rendered mono ID chip HTML from [`crate::links`].
    pub chip_html: String,
    /// Title, statement, question, topic, or glossary label.
    pub label: String,
}

/// One group of identifiers in [`IdsView`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdGroupView {
    /// Stable group key (`"pages"`, `"sources"`, `"versions"`, `"claims"`, `"questions"`, `"conflicts"`, `"terms"`).
    pub slug: &'static str,
    /// Group heading.
    pub title: &'static str,
    /// Sorted entries in this group.
    pub items: Vec<IdItemView>,
}

/// Complete view model for identifier groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsView {
    /// Page `<h1>` and `<title>`.
    pub title: &'static str,
    /// Subtitle under `<h1>`.
    pub subtitle: &'static str,
    /// Total count of IDs across all seven groups.
    pub total_count: usize,
    /// The seven ID groups in order.
    pub groups: Vec<IdGroupView>,
}

/// One identifier list page under `ids/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdListPage {
    /// Site-relative output path (e.g. `"ids/pages.html"`, `"ids/claims-CMP-1.html"`).
    pub path: String,
    /// Full document and `<h1>` title (e.g. `"All IDs: Pages"`).
    pub title: String,
    /// Current breadcrumb label (e.g. `"Pages"` or `"<area name>, part <ordinal> of <of>"`).
    pub crumb: String,
    /// Stable group slug (`"pages"`, `"sources"`, `"versions"`, `"claims"`, `"questions"`, `"conflicts"`, `"terms"`).
    pub slug: &'static str,
    /// Subtitle under `<h1>`.
    pub subtitle: String,
    /// Sorted entries displayed in this list page.
    pub items: Vec<IdItemView>,
}

/// One claims part entry rendered on the ID index page (`ids.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsIndexClaimPartView {
    /// Site-relative link to the claims part page if planned, or `None`.
    pub href: Option<String>,
    /// Range label for the chip (e.g. `"CL-CMP-0001 to CL-CMP-0500"`).
    pub range_label: String,
    /// Part description label (e.g. `"<area name>, part <ordinal> of <of>, <count> claims"`).
    pub label: String,
}

/// One identifier group panel summary on the ID index page (`ids.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsIndexGroupView {
    /// Stable group key (`"pages"`, `"sources"`, `"versions"`, `"claims"`, `"questions"`, `"conflicts"`, `"terms"`).
    pub slug: &'static str,
    /// Group heading.
    pub title: &'static str,
    /// Total count label (`"None recorded"` or decimal string).
    pub count_label: String,
    /// Href to the list page if planned and not claims, or `None`.
    pub href: Option<String>,
    /// Claims part page entries when `slug == "claims"`.
    pub claim_parts: Vec<IdsIndexClaimPartView>,
}

/// View model for the groups index on `ids.html`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsIndexView {
    /// The seven identifier groups in display order.
    pub groups: Vec<IdsIndexGroupView>,
}

/// View model for an identifier list page under `ids/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsListView {
    /// Stable group slug (`"pages"`, `"sources"`, `"versions"`, `"claims"`, `"questions"`, `"conflicts"`, `"terms"`).
    pub slug: &'static str,
    /// Heading shown inside the panel.
    pub panel_title: String,
    /// Sorted entries displayed in this list.
    pub items: Vec<IdItemView>,
}

/// Complete view model passed to `templates/ids.html`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsPageView {
    /// Output file name displayed in the eyebrow chip (`"ids.html"`, `"ids/pages.html"`, ...).
    pub file_name: String,
    /// Page `<h1>` and `<title>`.
    pub title: String,
    /// Subtitle under `<h1>`.
    pub subtitle: String,
    /// Count chip label in the header eyebrow (`"<N> IDs"` or `"None recorded"`).
    pub count_chip: String,
    /// Index view model when rendering `ids.html`.
    pub index: Option<IdsIndexView>,
    /// List view model when rendering a list page under `ids/`.
    pub list: Option<IdsListView>,
}

#[derive(Template)]
#[template(path = "ids.html")]
struct IdsTemplate<'a> {
    view: &'a IdsPageView,
}

/// Build the [`IdsView`] from the permitted [`LabelledView`].
#[must_use]
pub fn build_ids_view(view: &LabelledView, planned: &PlannedPaths) -> IdsView {
    let from_path = LIST_BASE_PATH;

    // 1. Pages
    let mut pages: Vec<&PageEntry> = view.pages().iter().collect();
    pages.sort_by(|a, b| a.id.cmp(&b.id));
    let page_items: Vec<IdItemView> = pages
        .into_iter()
        .map(|p| IdItemView {
            id: p.id.as_str().to_owned(),
            chip_html: render_page_chip(from_path, &p.id, planned),
            label: p.meta.title.clone(),
        })
        .collect();

    // 2. Sources (distinct SourceId, preferring the first non-quarantined version's title in numeric version order)
    let mut source_versions: Vec<&SourceRow> = view.sources().iter().map(|l| &l.value).collect();
    source_versions.sort_by(|a, b| {
        a.source_id
            .cmp(&b.source_id)
            .then_with(|| a.version.cmp(&b.version))
    });
    let mut distinct_sources: BTreeMap<SourceId, &SourceRow> = BTreeMap::new();
    for row in &source_versions {
        distinct_sources
            .entry(row.source_id.clone())
            .and_modify(|existing| {
                if existing.lifecycle == Lifecycle::Quarantined
                    && row.lifecycle != Lifecycle::Quarantined
                {
                    *existing = row;
                }
            })
            .or_insert(row);
    }
    let source_items: Vec<IdItemView> = distinct_sources
        .into_iter()
        .map(|(sid, row)| IdItemView {
            id: sid.as_str().to_owned(),
            chip_html: render_source_id_chip(from_path, &sid, view, planned),
            label: row.title.clone(),
        })
        .collect();

    // 3. Source versions
    let version_items: Vec<IdItemView> = source_versions
        .into_iter()
        .map(|row| {
            let vid_str = format!("{}-v{}", row.source_id, row.version);
            let vid = VersionId::parse(&vid_str)
                .unwrap_or_else(|e| unreachable!("valid SourceRow produces valid VersionId: {e}"));
            IdItemView {
                id: vid_str,
                chip_html: render_version_chip(from_path, &vid, Some(row), planned),
                label: row.title.clone(),
            }
        })
        .collect();

    // 4. Claims
    let mut claims: Vec<&ClaimRow> = view.claims().iter().map(|l| &l.value).collect();
    claims.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    let claim_items: Vec<IdItemView> = claims
        .into_iter()
        .map(|c| IdItemView {
            id: c.claim_id.as_str().to_owned(),
            chip_html: render_claim_chip(from_path, &c.claim_id, planned),
            label: c.statement.clone(),
        })
        .collect();

    // 5. Questions
    let mut questions: Vec<&QuestionRow> = view.questions().iter().map(|l| &l.value).collect();
    questions.sort_by(|a, b| a.question_id.cmp(&b.question_id));
    let question_items: Vec<IdItemView> = questions
        .into_iter()
        .map(|q| IdItemView {
            id: q.question_id.as_str().to_owned(),
            chip_html: render_question_chip(from_path, &q.question_id, Some(&q.question), planned),
            label: q.question.clone(),
        })
        .collect();

    // 6. Conflicts
    let mut conflicts: Vec<&ConflictRow> = view.conflicts().iter().map(|l| &l.value).collect();
    conflicts.sort_by_key(|c| (c.conflict_id.number(), c.conflict_id.as_str().to_owned()));
    let conflict_items: Vec<IdItemView> = conflicts
        .into_iter()
        .map(|xc| IdItemView {
            id: xc.conflict_id.as_str().to_owned(),
            chip_html: render_conflict_chip(from_path, &xc.conflict_id, Some(&xc.topic), planned),
            label: xc.topic.clone(),
        })
        .collect();

    // 7. Terms
    let mut terms: Vec<&GlossaryRow> = view.glossary().iter().map(|l| &l.value).collect();
    terms.sort_by(|a, b| a.term_id.cmp(&b.term_id));
    let term_items: Vec<IdItemView> = terms
        .into_iter()
        .map(|t| IdItemView {
            id: t.term_id.as_str().to_owned(),
            chip_html: render_term_chip(from_path, &t.term_id, planned),
            label: t.label.clone(),
        })
        .collect();

    let groups = vec![
        IdGroupView {
            slug: "pages",
            title: "Pages",
            items: page_items,
        },
        IdGroupView {
            slug: "sources",
            title: strings::parts()[6],
            items: source_items,
        },
        IdGroupView {
            slug: "versions",
            title: "Source versions",
            items: version_items,
        },
        IdGroupView {
            slug: "claims",
            title: "Claims",
            items: claim_items,
        },
        IdGroupView {
            slug: "questions",
            title: "Questions",
            items: question_items,
        },
        IdGroupView {
            slug: "conflicts",
            title: "Conflicts",
            items: conflict_items,
        },
        IdGroupView {
            slug: "terms",
            title: "Glossary terms",
            items: term_items,
        },
    ];

    let total_count = groups.iter().map(|g| g.items.len()).sum();

    IdsView {
        title: PAGE_TITLE,
        subtitle: "Every permitted page, source, version, claim, question, conflict, and glossary term identifier in this edition.",
        total_count,
        groups,
    }
}

/// Returns the site-relative output routes of every identifier list page.
///
/// Order is deterministic: the six group pages first (`ids/pages.html`, `ids/sources.html`,
/// `ids/versions.html`, `ids/questions.html`, `ids/conflicts.html`, `ids/terms.html`),
/// followed by the claims part pages (`ids/claims-<AREA>-<part>.html`).
/// Does not include `ids.html`.
#[must_use]
pub fn page_paths(view: &LabelledView) -> Vec<String> {
    let mut paths = Vec::with_capacity(SIX_GROUP_SLUGS.len());
    for slug in SIX_GROUP_SLUGS {
        paths.push(format!("ids/{slug}.html"));
    }
    for part in register::claims_parts(view) {
        paths.push(format!("ids/claims-{}-{}.html", part.area_code, part.part));
    }
    paths
}

fn list_pages_from_ids_view(ids_view: &IdsView, view: &LabelledView) -> Vec<IdListPage> {
    let mut pages = Vec::new();

    // 1. Six group pages in order
    for slug in SIX_GROUP_SLUGS {
        let group = ids_view
            .groups
            .iter()
            .find(|g| g.slug == slug)
            .unwrap_or_else(|| unreachable!("group '{slug}' must exist in IdsView"));

        let path = format!("ids/{slug}.html");
        let title = format!("All IDs: {}", group.title);
        let crumb = group.title.to_owned();
        let subtitle = format!(
            "All {} in this edition, each with its link.",
            group.title.to_ascii_lowercase()
        );

        pages.push(IdListPage {
            path,
            title,
            crumb,
            slug: group.slug,
            subtitle,
            items: group.items.clone(),
        });
    }

    // 2. Claims part pages
    let claims_group = ids_view
        .groups
        .iter()
        .find(|g| g.slug == "claims")
        .unwrap_or_else(|| unreachable!("claims group must exist in IdsView"));

    let parts = register::claims_parts(view);
    let mut claims_by_part: BTreeMap<(String, u32), Vec<IdItemView>> = BTreeMap::new();
    for item in &claims_group.items {
        if let Some((area, part)) = claim_part(&item.id) {
            claims_by_part
                .entry((area, part))
                .or_default()
                .push(item.clone());
        }
    }

    for part in parts {
        let path = format!("ids/claims-{}-{}.html", part.area_code, part.part);
        let title = format!(
            "All IDs: Claims, {}, part {} of {}",
            part.area_name, part.ordinal, part.of
        );
        let crumb = format!("{}, part {} of {}", part.area_name, part.ordinal, part.of);
        let subtitle = format!(
            "The {} claims from {} to {} in this edition, each with its link.",
            part.count, part.first_id, part.last_id
        );
        let items = claims_by_part
            .remove(&(part.area_code.clone(), part.part))
            .unwrap_or_default();

        pages.push(IdListPage {
            path,
            title,
            crumb,
            slug: "claims",
            subtitle,
            items,
        });
    }

    pages
}

/// Builds the list page models for all identifier list pages under `ids/`.
///
/// Returned in the exact same order as [`page_paths`]: the six group pages first,
/// followed by the claims part pages.
#[must_use]
pub fn list_pages(view: &LabelledView, planned: &PlannedPaths) -> Vec<IdListPage> {
    let ids_view = build_ids_view(view, planned);
    list_pages_from_ids_view(&ids_view, view)
}

fn build_ids_index_view(
    ids_view: &IdsView,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> IdsIndexView {
    let parts = register::claims_parts(view);
    let mut groups = Vec::with_capacity(ids_view.groups.len());

    for group in &ids_view.groups {
        let count_label = if group.items.is_empty() {
            "None recorded".to_owned()
        } else {
            group.items.len().to_string()
        };

        if group.slug == "claims" {
            let claim_parts = parts
                .iter()
                .map(|p| {
                    let route = format!("ids/claims-{}-{}.html", p.area_code, p.part);
                    let href = href_if_planned(OUTPUT_PATH, &route, planned);
                    let range_label = format!("{} to {}", p.first_id, p.last_id);
                    let label = if p.count == 1 {
                        format!("{}, part {} of {}, 1 claim", p.area_name, p.ordinal, p.of)
                    } else {
                        format!(
                            "{}, part {} of {}, {} claims",
                            p.area_name, p.ordinal, p.of, p.count
                        )
                    };
                    IdsIndexClaimPartView {
                        href,
                        range_label,
                        label,
                    }
                })
                .collect();

            groups.push(IdsIndexGroupView {
                slug: group.slug,
                title: group.title,
                count_label,
                href: None,
                claim_parts,
            });
        } else {
            let group_path = format!("ids/{}.html", group.slug);
            let href = href_if_planned(OUTPUT_PATH, &group_path, planned);
            groups.push(IdsIndexGroupView {
                slug: group.slug,
                title: group.title,
                count_label,
                href,
                claim_parts: Vec::new(),
            });
        }
    }

    IdsIndexView { groups }
}

fn render_index_main(ids_view: &IdsView, view: &LabelledView, planned: &PlannedPaths) -> String {
    let count_chip = if ids_view.total_count == 0 {
        "None recorded".to_owned()
    } else {
        format!("{} IDs", ids_view.total_count)
    };
    let index_view = build_ids_index_view(ids_view, view, planned);
    let page_view = IdsPageView {
        file_name: OUTPUT_PATH.to_owned(),
        title: ids_view.title.to_owned(),
        subtitle: ids_view.subtitle.to_owned(),
        count_chip,
        index: Some(index_view),
        list: None,
    };
    let tpl = IdsTemplate { view: &page_view };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("ids.html is infallible: {e}"))
}

/// Render the inner `<main>` content (`templates/ids.html`) for the ID index page.
#[must_use]
pub fn render_main(view: &LabelledView, planned: &PlannedPaths) -> String {
    let ids_view = build_ids_view(view, planned);
    render_index_main(&ids_view, view, planned)
}

fn render_page_from_ids_view(
    ids_view: &IdsView,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_index_main(ids_view, view, planned);
    let edition = ctx.edition_info();
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let lib_crumb = match SectionId::Library.first_path() {
        Some(first) => Crumb::linked("Library", first, planned),
        None => Crumb::current("Library"),
    };
    let crumbs = [lib_crumb, Crumb::current(PAGE_TITLE)];
    render_frame_page(&FrameInput {
        output_path: OUTPUT_PATH,
        title: PAGE_TITLE,
        page_area: None,
        edition,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

/// Render the complete `ids.html` document wrapped in the global site frame.
#[must_use]
pub fn render_page(
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let ids_view = build_ids_view(view, planned);
    render_page_from_ids_view(&ids_view, view, ctx, nav, planned)
}

/// Render the inner `<main>` content (`templates/ids.html`) for an identifier list page.
#[must_use]
pub fn render_list_main(page: &IdListPage) -> String {
    let count_chip = if page.items.is_empty() {
        "None recorded".to_owned()
    } else {
        format!("{} IDs", page.items.len())
    };
    let panel_title = if page.slug == "claims" {
        page.title.clone()
    } else {
        page.crumb.clone()
    };
    let page_view = IdsPageView {
        file_name: page.path.clone(),
        title: page.title.clone(),
        subtitle: page.subtitle.clone(),
        count_chip,
        index: None,
        list: Some(IdsListView {
            slug: page.slug,
            panel_title,
            items: page.items.clone(),
        }),
    };
    let tpl = IdsTemplate { view: &page_view };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("ids.html is infallible: {e}"))
}

/// Render a complete identifier list page document wrapped in the global site frame.
#[must_use]
pub fn render_list_page(
    page: &IdListPage,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_list_main(page);
    let edition = ctx.edition_info();
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let lib_crumb = match SectionId::Library.first_path() {
        Some(first) => Crumb::linked("Library", first, planned),
        None => Crumb::current("Library"),
    };
    let all_ids_crumb = Crumb::linked(PAGE_TITLE, OUTPUT_PATH, planned);
    let current_crumb = Crumb::current(&page.crumb);
    let crumbs = [lib_crumb, all_ids_crumb, current_crumb];
    render_frame_page(&FrameInput {
        output_path: &page.path,
        title: &page.title,
        page_area: None,
        edition,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

/// Render `ids.html` and all identifier list pages wrapped in the global site frame.
///
/// Returns pairs of `(output_path, html)` in deterministic order with `ids.html` first.
/// Identifier groups are built once and shared across all emitted pages.
#[must_use]
pub fn render_all(
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> Vec<(String, String)> {
    let ids_view = build_ids_view(view, planned);
    let pages = list_pages_from_ids_view(&ids_view, view);
    let index_html = render_page_from_ids_view(&ids_view, view, ctx, nav, planned);

    let mut all = Vec::with_capacity(1 + pages.len());
    all.push((OUTPUT_PATH.to_owned(), index_html));

    for page in &pages {
        all.push((page.path.clone(), render_list_page(page, ctx, nav, planned)));
    }

    all
}

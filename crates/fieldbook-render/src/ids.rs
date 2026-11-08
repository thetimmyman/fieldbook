//! ID index renderer (`ids.html`) listing every permitted identifier in [`LabelledView`] (`build-registers`).
//!
//! **Responsibility.** List every page, source (`S-nnnn`), source version (`S-nnnn-v<k>`),
//! claim (`CL-...`), question (`Q-...`), conflict (`XC-...`), and glossary term (`<term_id>`)
//! present in [`LabelledView`], sorted within each group, each with its title or label and
//! its link from [`crate::links`].

use std::collections::BTreeMap;

use askama::Template;
use fieldbook_core::check::PageEntry;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::{SourceId, VersionId};
use fieldbook_core::schema::{
    ClaimRow, ConflictRow, GlossaryRow, Lifecycle, QuestionRow, SourceRow,
};
use fieldbook_core::strings;

use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::links::{
    render_claim_chip, render_conflict_chip, render_page_chip, render_question_chip,
    render_source_id_chip, render_term_chip, render_version_chip,
};
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::site::PlannedPaths;

/// Site-relative output path for the ID index.
pub const OUTPUT_PATH: &str = "ids.html";

/// Page title for `ids.html`.
pub const PAGE_TITLE: &str = "All IDs";

/// One entry inside an [`IdGroupView`] on `ids.html`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdItemView {
    /// Canonical identifier (`cmp-system-review`, `S-0002`, `S-0002-v2`, `CL-CMP-0001`, ...).
    pub id: String,
    /// Rendered mono ID chip HTML from [`crate::links`].
    pub chip_html: String,
    /// Title, statement, question, topic, or glossary label.
    pub label: String,
}

/// One group of identifiers on `ids.html`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdGroupView {
    /// Stable group key (`"pages"`, `"sources"`, `"versions"`, `"claims"`, `"questions"`, `"conflicts"`, `"terms"`).
    pub slug: &'static str,
    /// Group heading.
    pub title: &'static str,
    /// Sorted entries in this group.
    pub items: Vec<IdItemView>,
}

/// Complete view model for `ids.html`.
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

#[derive(Template)]
#[template(path = "ids.html")]
struct IdsTemplate<'a> {
    view: &'a IdsView,
}

/// Build the [`IdsView`] from the permitted [`LabelledView`].
#[must_use]
pub fn build_ids_view(view: &LabelledView, planned: &PlannedPaths) -> IdsView {
    let from_path = OUTPUT_PATH;

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

/// Render the inner `<main>` content (`templates/ids.html`).
#[must_use]
pub fn render_main(view: &LabelledView, planned: &PlannedPaths) -> String {
    let ids_view = build_ids_view(view, planned);
    let tpl = IdsTemplate { view: &ids_view };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("ids.html is infallible: {e}"))
}

/// Render the complete `ids.html` document wrapped in the global site frame.
#[must_use]
pub fn render_page(
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_main(view, planned);
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

//! Area pages `a/<area>.html`, one per `[[area]]` of `corpus.toml` (`build-home-and-areas`).
//!
//! **Responsibility.** `WorkspaceHeader`, then every section of the default workspace in HANDOFF
//! order under an in-page jump list. A section whose object has not landed shows `Unavailable`
//! with the release that brings it; a landed section with no rows shows its empty-state line.
//! Every count, link and line comes from the permitted [`LabelledView`].

use askama::Template;
use fieldbook_core::check::PageEntry;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::corpus_toml::AreaDef;
use fieldbook_core::schema::vocab::PageType;

use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::home::{coverage_of, purpose_line, render_three_up};
use crate::links::{escape_html, href_if_planned, render_page_link, render_question_chip};
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::site::PlannedPaths;
use crate::source::render_unavailable;
use crate::svg::{coverage_bar, coverage_legend};

/// The output path of the area with `code`.
#[must_use]
pub fn page_path(code: &str) -> String {
    format!("a/{}.html", code.to_ascii_lowercase())
}

/// The output paths of every area page, in `corpus.toml` order.
#[must_use]
pub fn page_paths(view: &LabelledView) -> Vec<String> {
    view.corpus().map_or_else(Vec::new, |corpus| {
        corpus.area.iter().map(|a| page_path(&a.code)).collect()
    })
}

struct JumpLink {
    id: &'static str,
    label: &'static str,
    count: Option<usize>,
}

struct PageRow {
    id: String,
    link_html: String,
    why: String,
    subarea: String,
    page_type: &'static str,
    stage: u8,
    as_of: String,
}

struct QuestionView {
    chip_html: String,
    question: String,
    owner_role: Option<String>,
}

#[derive(Template)]
#[template(path = "area.html")]
struct AreaTemplate {
    code: String,
    name: String,
    summary: String,
    jump: Vec<JumpLink>,
    three_up_html: String,
    bar_html: String,
    coverage: String,
    legend_html: String,
    processes_html: String,
    procedures_html: String,
    templates: Vec<PageRow>,
    templates_html: String,
    module_html: String,
    pages: Vec<PageRow>,
    sources_href: Option<String>,
    learn_href: Option<String>,
    history_html: String,
    questions: Vec<QuestionView>,
}

fn page_row(page: &PageEntry, from_path: &str, planned: &PlannedPaths) -> PageRow {
    let why = purpose_line(page);
    PageRow {
        id: page.id.as_str().to_owned(),
        link_html: render_page_link(
            from_path,
            page.id.as_str(),
            None,
            &escape_html(&page.meta.title),
            Some(&page.meta.title),
            Some(&why),
            planned,
        ),
        why,
        subarea: page.meta.subarea.clone(),
        page_type: page.meta.r#type.as_str(),
        stage: page.meta.stage,
        as_of: page.meta.as_of.to_string(),
    }
}

/// The pages of `area` by subarea (in `corpus.toml` order) and stage.
fn area_pages<'a>(view: &'a LabelledView, area: &AreaDef) -> Vec<&'a PageEntry> {
    let mut pages: Vec<&PageEntry> = view
        .pages()
        .iter()
        .filter(|p| p.meta.area.as_str().eq_ignore_ascii_case(&area.code))
        .collect();
    pages.sort_by_key(|p| {
        let subarea = area
            .subareas
            .iter()
            .position(|s| *s == p.meta.subarea)
            .unwrap_or(usize::MAX);
        (subarea, p.meta.stage, p.id.clone())
    });
    pages
}

/// The learn path starts at the first stage 1 page (HANDOFF section 2).
fn learn_href(view: &LabelledView, from_path: &str, planned: &PlannedPaths) -> Option<String> {
    let first = view
        .pages()
        .iter()
        .filter(|p| p.meta.stage == 1)
        .map(|p| p.id.as_str())
        .min()?;
    href_if_planned(from_path, &format!("p/{first}.html"), planned)
}

fn render_main(area: &AreaDef, view: &LabelledView, planned: &PlannedPaths) -> String {
    let from_path = page_path(&area.code);
    let counts = coverage_of(view, Some(&area.code));
    let in_area = area_pages(view, area);
    let pages: Vec<PageRow> = in_area
        .iter()
        .map(|p| page_row(p, &from_path, planned))
        .collect();
    let templates: Vec<PageRow> = in_area
        .iter()
        .filter(|p| p.meta.r#type == PageType::Template)
        .map(|p| page_row(p, &from_path, planned))
        .collect();
    let questions: Vec<QuestionView> = view
        .questions()
        .iter()
        .map(|q| &q.value)
        .filter(|q| q.area.as_str().eq_ignore_ascii_case(&area.code))
        .map(|q| QuestionView {
            chip_html: render_question_chip(&from_path, &q.question_id, Some(&q.question), planned),
            question: q.question.clone(),
            owner_role: q.owner_role.clone(),
        })
        .collect();
    let jump = vec![
        JumpLink {
            id: "overview",
            label: "Overview",
            count: None,
        },
        JumpLink {
            id: "processes",
            label: "Processes",
            count: None,
        },
        JumpLink {
            id: "procedures",
            label: "Procedures",
            count: None,
        },
        JumpLink {
            id: "templates",
            label: "Templates",
            count: (!templates.is_empty()).then_some(templates.len()),
        },
        JumpLink {
            id: "module",
            label: "Module",
            count: None,
        },
        JumpLink {
            id: "library-pages",
            label: "Library pages",
            count: (!pages.is_empty()).then_some(pages.len()),
        },
        JumpLink {
            id: "history",
            label: "History",
            count: None,
        },
        JumpLink {
            id: "questions",
            label: "Questions",
            count: (!questions.is_empty()).then_some(questions.len()),
        },
    ];
    AreaTemplate {
        code: area.code.to_ascii_uppercase(),
        name: area.name.clone(),
        summary: area.summary.clone(),
        jump,
        three_up_html: render_three_up(&from_path, view, planned, Some(&area.code), "h3"),
        bar_html: coverage_bar(&counts),
        coverage: counts.summary(),
        legend_html: if counts.total() == 0 {
            String::new()
        } else {
            coverage_legend(Some(&counts))
        },
        processes_html: render_unavailable("The process list", "Release 3"),
        procedures_html: render_unavailable("The procedure list", "Release 3"),
        templates,
        templates_html: render_unavailable("The template list", "E1"),
        module_html: render_unavailable("The module card", "Release 3"),
        pages,
        sources_href: href_if_planned(&from_path, "sources.html", planned),
        learn_href: learn_href(view, &from_path, planned),
        history_html: render_unavailable("History", "a later release"),
        questions,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("area.html is infallible: {e}"))
}

/// Renders the complete `a/<area>.html` document of `area` in the global frame.
#[must_use]
pub fn render_page(
    area: &AreaDef,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_main(area, view, planned);
    let path = page_path(&area.code);
    let built_date = match ctx {
        RenderContext::Public(edition) => {
            (!edition.built.is_empty()).then_some(edition.built.as_str())
        }
    };
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let areas = match SectionId::Areas.first_path() {
        Some(first) => Crumb::linked("Areas", first, planned),
        None => Crumb::current("Areas"),
    };
    let crumbs = [areas, Crumb::current(area.name.as_str())];
    render_frame_page(&FrameInput {
        output_path: &path,
        title: &area.name,
        page_area: Some(&area.code),
        built_date,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

//! Area pages `a/<area>.html`, one per `[[area]]` of `corpus.toml` (`build-home-and-areas`).
//!
//! **Responsibility.** `WorkspaceHeader`, then every section of the default workspace in HANDOFF
//! order under an in-page jump list. A section whose object has not landed shows `Unavailable`
//! with the release that brings it; a landed section with no rows shows its empty-state line.
//! Every count, link and line comes from the permitted [`LabelledView`]. The Overview holds the
//! area's charts (`build-charts`), each a ChartPanel or an `Unavailable` panel.
//!
//! A view page `a/<code>.html`, one per `[[view]]` (`views-and-day-n`), uses the same template:
//! its sections are the design's for that view, each list holds only the pages of the view's
//! filter, and a table the Corpus has no data for is an `Unavailable` panel, never rows.

use askama::Template;
use std::collections::BTreeSet;

use fieldbook_core::check::PageEntry;
use fieldbook_core::cite::KeyKind;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::SourceId;
use fieldbook_core::schema::corpus_toml::{AreaDef, ViewDef};
use fieldbook_core::schema::vocab::{PageType, QuestionStatus};
use fieldbook_core::strings;

use crate::charts::area_charts;
use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::home::{coverage_of, render_three_up};
use crate::inline::first_sentence_of_purpose;
use crate::links::{
    escape_html, href_if_planned, render_page_link, render_question_chip, render_source_id_chip,
};
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

/// The output paths of every view page, in `corpus.toml` order.
#[must_use]
pub fn view_page_paths(view: &LabelledView) -> Vec<String> {
    view.corpus().map_or_else(Vec::new, |corpus| {
        corpus.view.iter().map(|v| page_path(&v.code)).collect()
    })
}

/// The release the view sections wait for: the Corpus has no format for their data yet.
const LATER: &str = "a later release";

const HISTORY_EMPTY: &str =
    "<p class=\"muted\">Nothing recorded yet. History fills as pages and registers change.</p>";

/// What one workspace section holds.
enum Body {
    /// The area Overview: three-up row, charts and claim coverage.
    Overview,
    /// A pre-rendered panel or line (`Unavailable`, or an empty-state line).
    Html(String),
    /// The template pages, or the `Unavailable` template list when there are none.
    Templates,
    /// The page table.
    Library,
    /// The open questions.
    Questions,
    /// The sources the pages cite.
    Sources,
    /// The reference and concept pages of a view.
    Profiles,
}

/// One `<section class="ws-section">` and its jump link.
struct Section {
    id: &'static str,
    label: &'static str,
    count: Option<usize>,
    body: Body,
}

impl Section {
    const fn new(id: &'static str, label: &'static str, body: Body) -> Self {
        Self {
            id,
            label,
            count: None,
            body,
        }
    }

    fn counted(id: &'static str, label: &'static str, rows: usize, body: Body) -> Self {
        Self {
            id,
            label,
            count: (rows > 0).then_some(rows),
            body,
        }
    }

    fn unavailable(id: &'static str, label: &'static str, thing: &str, until: &str) -> Self {
        Self::new(id, label, Body::Html(render_unavailable(thing, until)))
    }
}

struct SourceView {
    chip_html: String,
    title: String,
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
    eyebrow: String,
    name: String,
    summary: Option<String>,
    note: Option<String>,
    sections: Vec<Section>,
    three_up_html: String,
    charts_html: String,
    bar_html: String,
    coverage: String,
    legend_html: String,
    templates: Vec<PageRow>,
    templates_html: String,
    caption: String,
    pages: Vec<PageRow>,
    sources_href: Option<String>,
    learn_href: Option<String>,
    questions: Vec<QuestionView>,
    sources: Vec<SourceView>,
    profiles: Vec<PageRow>,
}

fn page_row(page: &PageEntry, from_path: &str, planned: &PlannedPaths) -> PageRow {
    let why = first_sentence_of_purpose(page);
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

/// The pages of the view `def`: those whose area and subarea equal its filter, by stage then id.
fn view_pages<'a>(view: &'a LabelledView, def: &ViewDef) -> Vec<&'a PageEntry> {
    let mut pages: Vec<&PageEntry> = view
        .pages()
        .iter()
        .filter(|p| {
            p.meta.area.as_str().eq_ignore_ascii_case(&def.filter.area)
                && p.meta.subarea.eq_ignore_ascii_case(&def.filter.subarea)
        })
        .collect();
    pages.sort_by_key(|p| (p.meta.stage, p.id.clone()));
    pages
}

/// The sources that `pages` cite, directly or through a claim, in source id order.
fn cited_sources(
    view: &LabelledView,
    pages: &[&PageEntry],
    from_path: &str,
    planned: &PlannedPaths,
) -> Vec<SourceView> {
    let mut ids: BTreeSet<SourceId> = BTreeSet::new();
    for page in pages {
        for cite in view.citations(&page.id) {
            let source = match cite.key_kind {
                KeyKind::Version => cite
                    .key
                    .rsplit_once("-v")
                    .and_then(|(source, _)| SourceId::parse(source).ok()),
                KeyKind::Claim => view
                    .claims()
                    .iter()
                    .find(|c| c.value.claim_id.as_str() == cite.key)
                    .and_then(|c| c.value.source_id.clone()),
                KeyKind::Question | KeyKind::Conflict | KeyKind::Note => None,
            };
            ids.extend(source);
        }
    }
    ids.iter()
        .filter_map(|id| {
            let row = view.sources().iter().find(|s| s.value.source_id == *id)?;
            Some(SourceView {
                chip_html: render_source_id_chip(from_path, id, view, planned),
                title: row.value.title.clone(),
            })
        })
        .collect()
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

fn template_rows(pages: &[&PageEntry], from_path: &str, planned: &PlannedPaths) -> Vec<PageRow> {
    pages
        .iter()
        .filter(|p| p.meta.r#type == PageType::Template)
        .map(|p| page_row(p, from_path, planned))
        .collect()
}

fn question_view(
    q: &fieldbook_core::schema::questions::QuestionRow,
    from_path: &str,
    planned: &PlannedPaths,
) -> QuestionView {
    QuestionView {
        chip_html: render_question_chip(from_path, &q.question_id, Some(&q.question), planned),
        question: q.question.clone(),
        owner_role: q.owner_role.clone(),
    }
}

/// The Processes, Procedures, Module and History sections, the same on an area and a view.
fn processes() -> Section {
    Section::unavailable("processes", "Processes", "The process list", "Release 3")
}

fn procedures() -> Section {
    Section::unavailable(
        "procedures",
        "Procedures",
        "The procedure list",
        "Release 3",
    )
}

fn module() -> Section {
    Section::unavailable("module", "Module", "The module card", "Release 3")
}

fn history() -> Section {
    Section::new("history", "History", Body::Html(HISTORY_EMPTY.to_owned()))
}

fn render_main(area: &AreaDef, view: &LabelledView, planned: &PlannedPaths) -> String {
    let from_path = page_path(&area.code);
    let counts = coverage_of(view, Some(&area.code));
    let in_area = area_pages(view, area);
    let pages: Vec<PageRow> = in_area
        .iter()
        .map(|p| page_row(p, &from_path, planned))
        .collect();
    let templates = template_rows(&in_area, &from_path, planned);
    let questions: Vec<QuestionView> = view
        .questions()
        .iter()
        .map(|q| &q.value)
        .filter(|q| {
            q.status == QuestionStatus::Open && q.area.as_str().eq_ignore_ascii_case(&area.code)
        })
        .map(|q| question_view(q, &from_path, planned))
        .collect();
    let sections = vec![
        Section::new("overview", "Overview", Body::Overview),
        processes(),
        procedures(),
        Section::counted("templates", "Templates", templates.len(), Body::Templates),
        module(),
        Section::counted("library-pages", "Library pages", pages.len(), Body::Library),
        history(),
        Section::counted("questions", "Questions", questions.len(), Body::Questions),
    ];
    let code = area.code.to_ascii_uppercase();
    AreaTemplate {
        eyebrow: format!("{code} · workspace"),
        code,
        name: area.name.clone(),
        summary: Some(area.summary.clone()),
        note: None,
        sections,
        three_up_html: render_three_up(&from_path, view, planned, Some(&area.code), "h3"),
        charts_html: area_charts(&area.code),
        bar_html: coverage_bar(&counts),
        coverage: counts.summary(),
        legend_html: if counts.total() == 0 {
            String::new()
        } else {
            coverage_legend(Some(&counts))
        },
        templates,
        templates_html: render_unavailable("The template list", "E1"),
        caption: format!("Pages in {} by subarea and stage", area.name),
        pages,
        sources_href: href_if_planned(&from_path, "sources.html", planned),
        learn_href: learn_href(view, &from_path, planned),
        questions,
        sources: Vec::new(),
        profiles: Vec::new(),
    }
    .render()
    .unwrap_or_else(|e| unreachable!("area.html is infallible: {e}"))
}

/// The summary line of the area a view filters, if `corpus.toml` holds that area.
fn view_summary(view: &LabelledView, def: &ViewDef) -> Option<String> {
    view.corpus()?
        .area
        .iter()
        .find(|a| a.code.eq_ignore_ascii_case(&def.filter.area))
        .map(|a| a.summary.clone())
}

fn render_view_main(def: &ViewDef, view: &LabelledView, planned: &PlannedPaths) -> String {
    let from_path = page_path(&def.code);
    let own = view_pages(view, def);
    let pages: Vec<PageRow> = own
        .iter()
        .map(|p| page_row(p, &from_path, planned))
        .collect();
    let templates = template_rows(&own, &from_path, planned);
    let profiles: Vec<PageRow> = own
        .iter()
        .filter(|p| matches!(p.meta.r#type, PageType::Reference | PageType::Concept))
        .map(|p| page_row(p, &from_path, planned))
        .collect();
    let questions: Vec<QuestionView> = view
        .questions()
        .iter()
        .map(|q| &q.value)
        .filter(|q| {
            q.status == QuestionStatus::Open
                && own
                    .iter()
                    .any(|p| p.meta.questions.contains(&q.question_id))
        })
        .map(|q| question_view(q, &from_path, planned))
        .collect();
    let sources = cited_sources(view, &own, &from_path, planned);

    let templates_section =
        Section::counted("templates", "Templates", templates.len(), Body::Templates);
    let library = Section::counted("library-pages", "Library pages", pages.len(), Body::Library);
    let questions_section =
        Section::counted("questions", "Questions", questions.len(), Body::Questions);
    let code = def.code.to_ascii_uppercase();
    let sections = match code.as_str() {
        "CPY" => vec![
            Section::unavailable("background", "Background", "The company background", LATER),
            Section::unavailable(
                "organization",
                "Organization",
                "The organization chart",
                LATER,
            ),
            processes(),
            procedures(),
            templates_section,
            Section::unavailable("systems", "Systems", "The systems list", LATER),
            history(),
            questions_section,
        ],
        "CPT" => vec![
            Section::unavailable("compare", "Compare", "The comparison matrix", LATER),
            Section::counted("profiles", "Profiles", profiles.len(), Body::Profiles),
            Section::unavailable("moves", "Moves", "The moves list", LATER),
            Section::unavailable("watch-process", "Watch process", "The watch process", LATER),
            templates_section,
            library,
            Section::counted("sources", strings::parts()[6], sources.len(), Body::Sources),
            questions_section,
        ],
        _ => {
            let table = if code == "PGM" {
                "The programs table"
            } else {
                "The overview table"
            };
            vec![
                Section::unavailable("overview", "Overview", table, LATER),
                processes(),
                procedures(),
                templates_section,
                module(),
                library,
                history(),
                questions_section,
            ]
        }
    };
    let area_code = def.filter.area.to_ascii_uppercase();
    AreaTemplate {
        eyebrow: format!("View over {area_code}"),
        code,
        name: def.name.clone(),
        summary: view_summary(view, def),
        note: Some(format!(
            "{area_code} pages in subarea {}",
            def.filter.subarea
        )),
        sections,
        three_up_html: String::new(),
        charts_html: String::new(),
        bar_html: String::new(),
        coverage: String::new(),
        legend_html: String::new(),
        templates,
        templates_html: render_unavailable("The template list", "E1"),
        caption: format!("Pages in {} by stage", def.name),
        pages,
        sources_href: href_if_planned(&from_path, "sources.html", planned),
        learn_href: learn_href(view, &from_path, planned),
        questions,
        sources,
        profiles,
    }
    .render()
    .unwrap_or_else(|e| unreachable!("area.html is infallible: {e}"))
}

/// Wraps `main_html` in the global frame at `path`, with crumbs `Areas / <name>`.
fn render_in_frame(
    path: &str,
    name: &str,
    area_code: &str,
    main_html: &str,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let edition = ctx.edition_info();
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let areas = match SectionId::Areas.first_path() {
        Some(first) => Crumb::linked("Areas", first, planned),
        None => Crumb::current("Areas"),
    };
    let crumbs = [areas, Crumb::current(name)];
    render_frame_page(&FrameInput {
        output_path: path,
        title: name,
        page_area: Some(area_code),
        edition,
        store: &store,
        crumbs: &crumbs,
        main_html,
        nav,
    })
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
    render_in_frame(&path, &area.name, &area.code, &main_html, ctx, nav, planned)
}

/// Renders the complete `a/<code>.html` document of the view `def` in the global frame.
#[must_use]
pub fn render_view_page(
    def: &ViewDef,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_view_main(def, view, planned);
    let path = page_path(&def.code);
    render_in_frame(
        &path,
        &def.name,
        &def.filter.area,
        &main_html,
        ctx,
        nav,
        planned,
    )
}

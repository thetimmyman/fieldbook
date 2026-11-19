//! Area pages `a/<area>.html`, one per `[[area]]` of `corpus.toml` (`build-home-and-areas`).
//!
//! **Responsibility.** `WorkspaceHeader`, then every built section of the default workspace in
//! HANDOFF order under an in-page jump list. A section whose object has not landed takes no place
//! in that order and has no jump link: its landing line, with the release that brings it, sits in
//! one closed "Not built yet" disclosure after the last built section. A landed section with no
//! rows is built and shows its empty-state line. Every count, link and line comes from the
//! permitted [`LabelledView`]. The Overview holds the area's charts (`build-charts`), each a
//! ChartPanel or an `Unavailable` panel.
//!
//! A view page `a/<code>.html`, one per `[[view]]` (`views-and-day-n`), uses the same template:
//! its sections are the design's for that view, each list holds only the pages of the view's
//! filter, and a table the Corpus has no data for is a landing line in the disclosure, never rows.

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
use crate::nav::{Navigation, SectionId, format_not_available};
use crate::site::PlannedPaths;
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

/// What one built workspace section holds.
enum Body {
    /// The area Overview: three-up row, charts and claim coverage.
    Overview,
    /// A pre-rendered empty-state line.
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

/// One built `<section class="ws-section">` and its jump link.
struct Section {
    id: &'static str,
    label: &'static str,
    count: Option<usize>,
    body: Body,
}

/// A section whose object has not landed: its id and tab label, and its landing line ("<Thing> is
/// not available until <release or trigger>."). It has no place in the section sequence and no
/// jump link; the "Not built yet" disclosure holds it.
struct Unbuilt {
    id: &'static str,
    label: &'static str,
    line: String,
}

/// One entry of a workspace in table order: a section that is built, or one that is not.
enum Entry {
    Built(Section),
    Unbuilt(Unbuilt),
}

impl Entry {
    const fn built(id: &'static str, label: &'static str, body: Body) -> Self {
        Self::Built(Section {
            id,
            label,
            count: None,
            body,
        })
    }

    fn counted(id: &'static str, label: &'static str, rows: usize, body: Body) -> Self {
        Self::Built(Section {
            id,
            label,
            count: (rows > 0).then_some(rows),
            body,
        })
    }

    fn unbuilt(id: &'static str, label: &'static str, thing: &str, until: &str) -> Self {
        Self::Unbuilt(Unbuilt {
            id,
            label,
            line: format_not_available(thing, until),
        })
    }
}

/// Splits a workspace into its built sections and its unbuilt ones, each in table order.
fn split(entries: Vec<Entry>) -> (Vec<Section>, Vec<Unbuilt>) {
    let mut built = Vec::new();
    let mut unbuilt = Vec::new();
    for entry in entries {
        match entry {
            Entry::Built(section) => built.push(section),
            Entry::Unbuilt(section) => unbuilt.push(section),
        }
    }
    (built, unbuilt)
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
    unbuilt: Vec<Unbuilt>,
    three_up_html: String,
    charts_html: String,
    bar_html: String,
    coverage: String,
    legend_html: String,
    templates: Vec<PageRow>,
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
fn processes() -> Entry {
    Entry::unbuilt("processes", "Processes", "The process list", "Release 3")
}

fn procedures() -> Entry {
    Entry::unbuilt(
        "procedures",
        "Procedures",
        "The procedure list",
        "Release 3",
    )
}

fn module() -> Entry {
    Entry::unbuilt("module", "Module", "The module card", "Release 3")
}

fn history() -> Entry {
    Entry::built("history", "History", Body::Html(HISTORY_EMPTY.to_owned()))
}

/// The Templates section: built when the workspace holds `rows` template pages, unbuilt until the
/// template list lands (E1) when it holds none.
fn templates_entry(rows: usize) -> Entry {
    if rows == 0 {
        Entry::unbuilt("templates", "Templates", "The template list", "E1")
    } else {
        Entry::counted("templates", "Templates", rows, Body::Templates)
    }
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
    let (sections, unbuilt) = split(vec![
        Entry::built("overview", "Overview", Body::Overview),
        processes(),
        procedures(),
        templates_entry(templates.len()),
        module(),
        Entry::counted("library-pages", "Library pages", pages.len(), Body::Library),
        history(),
        Entry::counted("questions", "Questions", questions.len(), Body::Questions),
    ]);
    let code = area.code.to_ascii_uppercase();
    AreaTemplate {
        eyebrow: format!("{code} · workspace"),
        code,
        name: area.name.clone(),
        summary: Some(area.summary.clone()),
        note: None,
        sections,
        unbuilt,
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

    let templates_section = templates_entry(templates.len());
    let library = Entry::counted("library-pages", "Library pages", pages.len(), Body::Library);
    let questions_section =
        Entry::counted("questions", "Questions", questions.len(), Body::Questions);
    let code = def.code.to_ascii_uppercase();
    let entries = match code.as_str() {
        "CPY" => vec![
            Entry::unbuilt("background", "Background", "The company background", LATER),
            Entry::unbuilt(
                "organization",
                "Organization",
                "The organization chart",
                LATER,
            ),
            processes(),
            procedures(),
            templates_section,
            Entry::unbuilt("systems", "Systems", "The systems list", LATER),
            history(),
            questions_section,
        ],
        "CPT" => vec![
            Entry::unbuilt("compare", "Compare", "The comparison matrix", LATER),
            Entry::counted("profiles", "Profiles", profiles.len(), Body::Profiles),
            Entry::unbuilt("moves", "Moves", "The moves list", LATER),
            Entry::unbuilt("watch-process", "Watch process", "The watch process", LATER),
            templates_section,
            library,
            Entry::counted("sources", strings::parts()[6], sources.len(), Body::Sources),
            questions_section,
        ],
        _ => {
            let table = if code == "PGM" {
                "The programs table"
            } else {
                "The overview table"
            };
            vec![
                Entry::unbuilt("overview", "Overview", table, LATER),
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
    let (sections, unbuilt) = split(entries);
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
        unbuilt,
        three_up_html: String::new(),
        charts_html: String::new(),
        bar_html: String::new(),
        coverage: String::new(),
        legend_html: String::new(),
        templates,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The workspace template over `entries`: no header text, panels or rows of its own.
    fn render_entries(entries: Vec<Entry>) -> String {
        let (sections, unbuilt) = split(entries);
        AreaTemplate {
            code: "TST".to_owned(),
            eyebrow: String::new(),
            name: "Synthetic".to_owned(),
            summary: None,
            note: None,
            sections,
            unbuilt,
            three_up_html: String::new(),
            charts_html: String::new(),
            bar_html: String::new(),
            coverage: String::new(),
            legend_html: String::new(),
            templates: Vec::new(),
            caption: String::new(),
            pages: Vec::new(),
            sources_href: None,
            learn_href: None,
            questions: Vec::new(),
            sources: Vec::new(),
            profiles: Vec::new(),
        }
        .render()
        .unwrap_or_else(|e| unreachable!("area.html is infallible: {e}"))
    }

    fn table(processes: Entry) -> Vec<Entry> {
        vec![
            Entry::built("overview", "Overview", Body::Overview),
            processes,
            history(),
            Entry::counted("questions", "Questions", 0, Body::Questions),
        ]
    }

    fn section_ids(html: &str) -> Vec<&str> {
        html.split("<section class=\"ws-section\" id=\"")
            .skip(1)
            .map(|chunk| chunk.split('"').next().unwrap_or_default())
            .collect()
    }

    #[test]
    fn a_workspace_with_nothing_unbuilt_has_no_disclosure_and_no_jump_entry() {
        let landed = Entry::built(
            "processes",
            "Processes",
            Body::Html("<p>Rows.</p>".to_owned()),
        );
        let html = render_entries(table(landed));
        assert!(!html.contains("not-built"), "{html}");
        assert!(!html.contains("Not built yet"), "{html}");
        assert!(!html.contains("<details"), "{html}");
        assert_eq!(html.matches("<a href=\"#").count(), 4, "{html}");
    }

    #[test]
    fn a_section_that_lands_returns_to_its_place_in_the_table_order() {
        let unbuilt = Entry::unbuilt("processes", "Processes", "The process list", "Release 3");
        let before = render_entries(table(unbuilt));
        assert_eq!(section_ids(&before), ["overview", "history", "questions"]);
        assert!(before.contains("<p class=\"ws-sub\" id=\"processes\">Processes</p>"));

        let landed = Entry::built(
            "processes",
            "Processes",
            Body::Html("<p>Rows.</p>".to_owned()),
        );
        let after = render_entries(table(landed));
        assert_eq!(
            section_ids(&after),
            ["overview", "processes", "history", "questions"]
        );
        assert!(!after.contains("ws-sub"), "{after}");
    }
}

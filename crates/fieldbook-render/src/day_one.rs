//! Day-one binder page (`day-one.html`): the printed binder of briefs and stage 1/2 pages.
//!
//! **Responsibility.** Render the printable collection of all brief pages followed by
//! non-brief pages of stages 1 and 2 into `templates/day-one.html`, wrapped in the global
//! frame via [`crate::frame::render_page`].

use askama::Template;
use fieldbook_core::check::PageEntry;
use fieldbook_core::edition::EditionInfo;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::{PageType, Readiness};
use fieldbook_core::strings;

use crate::cite::OriginalAvailability;
use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::inline::InlineRenderer;
use crate::meta::{BuildInfo, MetadataError, RenderContext};
use crate::nav::{Navigation, SectionId};
use crate::page::{
    build_conflict_boxes, build_meta_row, build_part_by_name, collect_distinct_cited_claims,
    collect_distinct_conflicts, render_change_note, render_conflict_boxes, render_due_claims,
    render_meta_row, render_page_status_chip, render_superseded_by,
};
use crate::site::PlannedPaths;
use crate::sources_block::NumberedSourceRow;

/// Site-relative output path of the day-one binder page.
pub const OUTPUT_PATH: &str = strings::paths()[3];

/// Page title of `day-one.html`.
pub const PAGE_TITLE: &str = "Day-one binder";

/// One rendered part section for a page in the binder.
pub struct BinderPartView {
    /// Frozen part heading text.
    pub heading: String,
    /// `true` when the part has no prose content.
    pub is_empty: bool,
    /// Rendered HTML of the part body.
    pub body_html: String,
    /// HTML printed after the body inside the same part (the conflict boxes under Never confuse).
    pub after_html: String,
}

/// One rendered page article in the binder.
pub struct BinderPageView {
    /// Page identifier (`<area>-<slug>`).
    pub id: String,
    /// Page title.
    pub title: String,
    /// Page type string.
    pub page_type: String,
    /// Numerical stage.
    pub stage: u8,
    /// Page status chip HTML.
    pub status_chip_html: String,
    /// Rendered metadata row HTML.
    pub meta_row_html: String,
    /// `true` when `page.meta.readiness == Readiness::SyntheticNow`.
    pub is_synthetic_now: bool,
    /// Rendered superseded-by link HTML.
    pub superseded_by_html: String,
    /// `true` when `page_type == PageType::Brief`.
    pub is_brief: bool,
    /// `true` when the page writes the hidden review-overdue banner.
    pub overdue_banner: bool,
    /// Target review date string (`YYYY-MM-DD`).
    pub review_by: String,
    /// Rendered due claims list HTML with binder-prefixed IDs.
    pub due_claims_html: String,
    /// Ordered rendered parts for the page.
    pub parts: Vec<BinderPartView>,
    /// Change note part HTML.
    pub change_note_html: String,
    /// Numbered sources cited on the page.
    pub sources_rows: Vec<NumberedSourceRow>,
}

#[derive(Template)]
#[template(path = "parts/plan_timeline.html")]
struct PlanTimelinePanel<'a> {
    plan_timeline: &'a crate::plan::PlanTimelineView,
}

#[derive(Template)]
#[template(path = "./day-one.html")]
struct DayOneTemplate<'a> {
    pages: Vec<BinderPageView>,
    product: &'a str,
    edition_label: String,
    print_check: &'a str,
    empty_part_text: &'a str,
    sources_heading: &'a str,
    change_note_heading: &'a str,
}

/// Prefixes `id`, `href="#..."`, and `aria-labelledby` attributes with `b-{page_id}-`
/// so fragment identifiers and accessibility references remain unique across the binder.
fn prefix_ids(html: &str, page_id: &str) -> String {
    html.replace(" id=\"", &format!(" id=\"b-{page_id}-"))
        .replace("href=\"#", &format!("href=\"#b-{page_id}-"))
        .replace(
            "aria-labelledby=\"",
            &format!("aria-labelledby=\"b-{page_id}-"),
        )
}

/// Returns the pages included in the day-one binder in their printed order.
///
/// Order: first every `brief` page in `view.pages()` order, then every non-brief
/// page with stage 1, then every non-brief page with stage 2.
#[must_use]
pub fn binder_pages(view: &LabelledView) -> Vec<&PageEntry> {
    let mut out = Vec::new();
    for page in view.pages() {
        if page.meta.r#type == PageType::Brief {
            out.push(page);
        }
    }
    for page in view.pages() {
        if page.meta.r#type != PageType::Brief && page.meta.stage == 1 {
            out.push(page);
        }
    }
    for page in view.pages() {
        if page.meta.r#type != PageType::Brief && page.meta.stage == 2 {
            out.push(page);
        }
    }
    out
}

/// Renders the inner `<main>` content (`templates/day-one.html`) for the binder.
#[must_use]
pub fn render_main(view: &LabelledView, edition: &EditionInfo, planned: &PlannedPaths) -> String {
    let pages_to_render = binder_pages(view);
    let availability = OriginalAvailability::from_view(view);
    let overdue_banner = crate::edition::built_date(edition).is_some();
    let edition_label = match crate::edition::built_date(edition) {
        Some(date) => format!("{} · {date}", crate::edition::header_label(edition)),
        None => strings::unreleased_build().to_owned(),
    };

    let mut rendered_pages = Vec::with_capacity(pages_to_render.len());
    for page in pages_to_render {
        let is_brief = page.meta.r#type == PageType::Brief;
        let mut inline = InlineRenderer::new(OUTPUT_PATH, page, view, planned);

        let parts = if is_brief {
            let mut brief_parts = Vec::with_capacity(6);
            for &heading in strings::brief_parts() {
                let fallback = heading.to_ascii_lowercase().replace(' ', "-");
                let part = build_part_by_name(heading, &fallback, true, page, &mut inline);
                let body_html = if part.is_empty {
                    String::new()
                } else {
                    prefix_ids(&part.body_html, page.id.as_str())
                };
                brief_parts.push(BinderPartView {
                    heading: heading.to_owned(),
                    is_empty: part.is_empty,
                    body_html,
                    after_html: String::new(),
                });
            }
            brief_parts
        } else {
            let part_names = strings::parts();
            let p0 = build_part_by_name(part_names[0], "purpose", true, page, &mut inline);
            let p1 = if page.meta.r#type == PageType::Reference {
                crate::types::render_reference_the_rule(page, view, &mut inline)
            } else {
                build_part_by_name(part_names[1], "the-rule", true, page, &mut inline)
            };
            let mut p2 = build_part_by_name(part_names[2], "how-it-works", true, page, &mut inline);
            if page.meta.r#type == PageType::Checklist && !p2.is_empty {
                let plan_timeline = crate::plan::build_plan_timeline(page);
                let timeline_html = PlanTimelinePanel {
                    plan_timeline: &plan_timeline,
                }
                .render()
                .unwrap_or_else(|e| unreachable!("parts/plan_timeline.html is infallible: {e}"));
                let numbered = crate::types::number_checklist_task_lists(&p2.body_html);
                p2.body_html = format!("{timeline_html}{numbered}");
            }
            let p3 = build_part_by_name(part_names[3], "worked-example", true, page, &mut inline);
            let p4 = build_part_by_name(part_names[4], "never-confuse", true, page, &mut inline);
            let p5 = build_part_by_name(
                part_names[5],
                "company-calibration",
                true,
                page,
                &mut inline,
            );

            let conflict_ids = collect_distinct_conflicts(page, view);
            let conflict_boxes = build_conflict_boxes(OUTPUT_PATH, &conflict_ids, view, planned);
            let conflict_boxes_html =
                prefix_ids(&render_conflict_boxes(conflict_boxes), page.id.as_str());

            let raw_parts = [p0, p1, p2, p3, p4, p5];
            let mut non_brief_parts = Vec::with_capacity(raw_parts.len());
            for (idx, part) in raw_parts.into_iter().enumerate() {
                let body_html = if part.is_empty {
                    String::new()
                } else {
                    prefix_ids(&part.body_html, page.id.as_str())
                };
                non_brief_parts.push(BinderPartView {
                    heading: part.heading,
                    is_empty: part.is_empty,
                    body_html,
                    after_html: if idx == 4 {
                        conflict_boxes_html.clone()
                    } else {
                        String::new()
                    },
                });
            }
            non_brief_parts
        };

        let status_chip_html = render_page_status_chip(page.meta.status);
        let is_synthetic_now = page.meta.readiness == Readiness::SyntheticNow;
        let superseded_by_html = prefix_ids(
            &render_superseded_by(OUTPUT_PATH, page, view, planned),
            page.id.as_str(),
        );

        let distinct_claims = collect_distinct_cited_claims(page, view, &inline);
        let due_claims_html = if overdue_banner {
            render_due_claims(
                OUTPUT_PATH,
                &distinct_claims,
                view,
                planned,
                crate::chip::VERIFY_LANDED,
            )
        } else {
            String::new()
        };
        let meta_row = build_meta_row(
            page,
            view,
            &distinct_claims,
            due_claims_html,
            overdue_banner,
        );
        let raw_meta_row = render_meta_row(&meta_row);
        let meta_row_html = prefix_ids(&raw_meta_row, page.id.as_str());
        let due_claims_html = prefix_ids(&meta_row.due_claims_html, page.id.as_str());

        let markers = inline.into_markers();
        let sources_rows = crate::sources_block::build_sources_rows(
            OUTPUT_PATH,
            &markers,
            view,
            planned,
            &availability,
        );

        let change_note_html = if is_brief {
            String::new()
        } else {
            prefix_ids(&render_change_note(&page.body, page), page.id.as_str())
        };

        rendered_pages.push(BinderPageView {
            id: page.id.to_string(),
            title: page.meta.title.clone(),
            page_type: page.meta.r#type.as_str().to_owned(),
            stage: page.meta.stage,
            status_chip_html,
            meta_row_html,
            is_synthetic_now,
            superseded_by_html,
            is_brief,
            overdue_banner: meta_row.overdue_banner,
            review_by: meta_row.review_by,
            due_claims_html,
            parts,
            change_note_html,
            sources_rows,
        });
    }

    DayOneTemplate {
        pages: rendered_pages,
        product: crate::frame::PRODUCT_TITLE,
        edition_label,
        print_check: strings::print_check(),
        empty_part_text: strings::empty_part(),
        sources_heading: strings::parts()[6],
        change_note_heading: strings::parts()[7],
    }
    .render()
    .unwrap_or_else(|e| unreachable!("day-one.html is infallible: {e}"))
}

/// Renders the complete `day-one.html` document inside the global frame.
#[must_use]
pub fn render_page(
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_main(view, ctx.edition_info(), planned);
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let today_crumb = match SectionId::Today.first_path() {
        Some(first) => Crumb::linked("Today", first, planned),
        None => Crumb::current("Today"),
    };
    let crumbs = [today_crumb, Crumb::current(PAGE_TITLE)];
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

/// Separately callable renderer for `day-one.html`.
///
/// # Errors
///
/// Returns [`MetadataError`] if constructing [`RenderContext`] fails.
pub fn render(view: &LabelledView, info: &BuildInfo) -> Result<String, MetadataError> {
    let ctx = crate::site::render_context(view, info)?;
    let planned = crate::site::planned_paths(view);
    let nav = crate::nav::build(view, &planned);
    Ok(render_page(view, &ctx, &nav, &planned))
}

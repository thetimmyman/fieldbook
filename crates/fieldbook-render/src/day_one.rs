//! Day-one binder page (`day-one.html`): the printed binder of briefs and stage 1/2 pages.
//!
//! **Responsibility.** Render the printable collection of all brief pages followed by
//! non-brief pages of stages 1 and 2 into `templates/day-one.html`, wrapped in the global
//! frame via [`crate::frame::render_page`].

use askama::Template;
use fieldbook_core::check::PageEntry;
use fieldbook_core::edition::EditionInfo;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::schema::PageType;
use fieldbook_core::strings;

use crate::cite::OriginalAvailability;
use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::inline::InlineRenderer;
use crate::links::escape_html;
use crate::meta::{BuildInfo, MetadataError, RenderContext};
use crate::nav::{Navigation, SectionId};
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
    /// `true` when `page_type == PageType::Brief`.
    pub is_brief: bool,
    /// Ordered rendered parts for the page.
    pub parts: Vec<BinderPartView>,
    /// Numbered sources cited on the page.
    pub sources_rows: Vec<NumberedSourceRow>,
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
    let edition_label = match crate::edition::built_date(edition) {
        Some(date) => format!("{} · {date}", crate::edition::header_label(edition)),
        None => strings::unreleased_build().to_owned(),
    };

    let mut rendered_pages = Vec::with_capacity(pages_to_render.len());
    for page in pages_to_render {
        let is_brief = page.meta.r#type == PageType::Brief;
        let part_headings: &[&str] = if is_brief {
            strings::brief_parts().as_slice()
        } else {
            &strings::parts()[..5]
        };

        let mut inline = InlineRenderer::new(OUTPUT_PATH, page, view, planned);
        let mut parts = Vec::with_capacity(part_headings.len());
        for &heading in part_headings {
            let part_span = page.parts().get(heading);
            let raw_body = match part_span {
                Some(ps) if !ps.event_range.is_empty() => {
                    inline.render_part(ps.event_range.clone())
                }
                _ => String::new(),
            };
            let empty_para = format!("<p>{}</p>", escape_html(strings::empty_part()));
            let empty_li = format!(
                "<ul>\n<li>{}</li>\n</ul>",
                escape_html(strings::empty_part())
            );
            let trimmed_body = raw_body.trim();
            let is_empty = part_span.is_none_or(|ps| ps.event_range.is_empty())
                || trimmed_body == empty_para
                || trimmed_body == empty_li;
            let body_html = if is_empty {
                String::new()
            } else {
                // The binder holds many pages in one document, so every id a page body defines
                // (headings, citation markers) and every same-page link must carry the page id.
                raw_body
                    .replace(" id=\"", &format!(" id=\"b-{}-", page.id))
                    .replace("href=\"#", &format!("href=\"#b-{}-", page.id))
            };
            parts.push(BinderPartView {
                heading: heading.to_owned(),
                is_empty,
                body_html,
            });
        }

        let markers = inline.into_markers();
        let sources_rows = crate::sources_block::build_sources_rows(
            OUTPUT_PATH,
            &markers,
            view,
            planned,
            &availability,
        );

        rendered_pages.push(BinderPageView {
            id: page.id.to_string(),
            title: page.meta.title.clone(),
            page_type: page.meta.r#type.as_str().to_owned(),
            stage: page.meta.stage,
            is_brief,
            parts,
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

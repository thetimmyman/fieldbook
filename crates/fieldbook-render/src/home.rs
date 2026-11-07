//! The home page `index.html`, and the `ThreeUp` panels it shares with the area pages
//! (`build-home-and-areas`).
//!
//! **Responsibility.** Hero, the three panels for the whole library, the day-one questions, the
//! briefs, the `Unavailable` next-up row and `AreaIndex` with one `AreaTile` and `CoverageBar` per
//! area. Every count, link and line comes from the permitted [`LabelledView`].

use std::collections::BTreeSet;

use askama::Template;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::VersionId;
use fieldbook_core::schema::vocab::{PageType, QuestionStatus};
use fieldbook_core::schema::{ConflictStatus, SourceRow, Verification};

use crate::frame::{FrameInput, PRODUCT_TITLE, StoreContext, render_page as render_frame_page};
use crate::inline::first_sentence_of_purpose;
use crate::links::{
    claim_href, conflict_href, escape_html, href_if_planned, render_page_link, render_question_chip,
};
use crate::meta::RenderContext;
use crate::nav::{Navigation, format_not_available};
use crate::site::PlannedPaths;
use crate::source::{is_held, page_path, render_unavailable};
use crate::svg::{CoverageCounts, coverage_bar, coverage_legend};

/// Site-relative output path of the home page.
pub const OUTPUT_PATH: &str = "index.html";

/// Items a `ThreeUp` panel shows before the rest fold under "N more".
const THREE_UP_SHOWN: usize = 3;

/// The release that brings the overlay records of Continue working (section 2, AD-09).
const CONTINUE_UNTIL: &str = "the ORG overlay, Release 2 or later";

/// Counts each permitted claim of `area` once (all areas when `area` is `None`).
pub(crate) fn coverage_of(view: &LabelledView, area: Option<&str>) -> CoverageCounts {
    let mut seen = BTreeSet::new();
    let mut counts = CoverageCounts::default();
    for located in view.claims() {
        let claim = &located.value;
        if in_scope(claim.area.as_str(), area) && seen.insert(claim.claim_id.clone()) {
            counts.add(claim.verification);
        }
    }
    counts
}

fn in_scope(item_area: &str, area: Option<&str>) -> bool {
    area.is_none_or(|code| item_area.eq_ignore_ascii_case(code))
}

fn version_of(row: &SourceRow) -> Option<VersionId> {
    VersionId::parse(&format!("{}-v{}", row.source_id, row.version)).ok()
}

/// One `ReuseItem`: a link, why it can be reused, and its state chip.
fn reuse_items(
    from_path: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
    area: Option<&str>,
) -> Vec<String> {
    view.pages()
        .iter()
        .filter(|p| p.meta.r#type == PageType::Template && in_scope(p.meta.area.as_str(), area))
        .map(|page| {
            let why = first_sentence_of_purpose(page);
            let link = render_page_link(
                from_path,
                page.id.as_str(),
                None,
                &escape_html(&page.meta.title),
                Some(&page.meta.title),
                Some(&why),
                planned,
            );
            format!(
                "<li class=\"tu-item\">{link}<span class=\"meta\">{}</span><span class=\"chips\"><span class=\"chip\">Template</span></span></li>",
                escape_html(&why)
            )
        })
        .collect()
}

/// One `ChangeItem`: the kind as a warn eyebrow, a linked title, and a small action to where it is
/// settled, whose name includes the item.
fn change_item(
    kind: &str,
    title: &str,
    meta: Option<&str>,
    action: &str,
    item: &str,
    href: Option<&str>,
) -> String {
    let title = escape_html(title);
    let title_html = href.map_or_else(
        || format!("<b>{title}</b>"),
        |h| format!("<a href=\"{}\"><b>{title}</b></a>", escape_html(h)),
    );
    let meta_html = meta
        .map(|m| format!("<span class=\"meta\">{}</span>", escape_html(m)))
        .unwrap_or_default();
    let button = href
        .map(|h| {
            format!(
                "<a class=\"btn sm\" href=\"{}\">{action}<span class=\"sr-only\"> {}</span></a>",
                escape_html(h),
                escape_html(item)
            )
        })
        .unwrap_or_default();
    format!(
        "<li class=\"tu-item chg\"><div><span class=\"k\">{kind}</span>{title_html}{meta_html}</div>{button}</li>"
    )
}

/// The source versions `area` depends on through a permitted claim (every version for `None`).
fn cited_in_area(view: &LabelledView, area: Option<&str>) -> Option<BTreeSet<String>> {
    area.map(|code| {
        view.claims()
            .iter()
            .filter(|c| c.value.area.as_str().eq_ignore_ascii_case(code))
            .filter_map(|c| {
                c.value
                    .source_id
                    .as_ref()
                    .zip(c.value.version)
                    .map(|(sid, ver)| format!("{sid}-v{ver}"))
            })
            .collect()
    })
}

/// Changes in review order: contradicted claims, stale claims, open conflicts, superseded source
/// versions; within claims, the consequential ones first.
fn change_items(
    from_path: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
    area: Option<&str>,
) -> Vec<String> {
    let mut items = Vec::new();
    for (state, kind, action) in [
        (Verification::Contradicted, "Contradicted claim", "Review"),
        (Verification::Stale, "Stale claim", "Verify"),
    ] {
        let mut claims: Vec<_> = view
            .claims()
            .iter()
            .map(|c| &c.value)
            .filter(|c| c.verification == state && in_scope(c.area.as_str(), area))
            .collect();
        claims.sort_by_key(|c| !c.consequential);
        for claim in claims {
            let href = claim_href(from_path, &claim.claim_id, planned);
            items.push(change_item(
                kind,
                &claim.statement,
                Some(claim.claim_id.as_str()),
                action,
                claim.claim_id.as_str(),
                href.as_deref(),
            ));
        }
    }
    for located in view.conflicts() {
        let conflict = &located.value;
        if conflict.status == ConflictStatus::Unresolved && in_scope(conflict.area.as_str(), area) {
            let href = conflict_href(from_path, &conflict.conflict_id, planned);
            items.push(change_item(
                "Conflict",
                &conflict.topic,
                Some(conflict.conflict_id.as_str()),
                "Review",
                conflict.conflict_id.as_str(),
                href.as_deref(),
            ));
        }
    }
    let superseded: BTreeSet<&VersionId> = view
        .sources()
        .iter()
        .filter(|s| !is_held(&s.value))
        .flat_map(|s| s.value.supersedes.iter())
        .collect();
    let cited = cited_in_area(view, area);
    for located in view.sources() {
        let row = &located.value;
        let Some(vid) = version_of(row) else {
            continue;
        };
        let belongs = cited.as_ref().is_none_or(|c| c.contains(vid.as_str()));
        if is_held(row) || !belongs || !superseded.contains(&vid) {
            continue;
        }
        let href = href_if_planned(from_path, &page_path(vid.as_str()), planned);
        items.push(change_item(
            "Source update",
            &row.title,
            Some(&format!("{vid} · v{} superseded", row.version)),
            "Re-check",
            vid.as_str(),
            href.as_deref(),
        ));
    }
    items
}

fn panel(
    heading: &str,
    title: &str,
    count: Option<String>,
    items: &[String],
    empty: &str,
) -> String {
    let sub = count
        .map(|c| format!("<span class=\"panel-sub mono\">{c}</span>"))
        .unwrap_or_default();
    let body = if items.is_empty() {
        format!("<p class=\"muted\">{}</p>", escape_html(empty))
    } else {
        let (shown, folded) = items.split_at(items.len().min(THREE_UP_SHOWN));
        let fold = if folded.is_empty() {
            String::new()
        } else {
            format!(
                "<details class=\"fold\"><summary><b>{} more</b></summary><ul class=\"tu-list\">{}</ul></details>",
                folded.len(),
                folded.concat()
            )
        };
        format!("<ul class=\"tu-list\">{}</ul>{fold}", shown.concat())
    };
    let id = format!("tu-{}", title.to_ascii_lowercase().replace(' ', "-"));
    format!(
        "<section class=\"panel\" aria-labelledby=\"{id}\"><div class=\"panel-head\"><{heading} class=\"panel-title\" id=\"{id}\">{title}</{heading}>{sub}</div>{body}</section>"
    )
}

fn count_text(n: usize) -> String {
    if n == 0 {
        "None recorded".to_owned()
    } else {
        n.to_string()
    }
}

/// Renders the `ThreeUp` row: the whole library for `None`, or what belongs to one area. `heading`
/// is the panel heading element, `h2` or `h3`. An empty panel stays and says why in one line.
pub(crate) fn render_three_up(
    from_path: &str,
    view: &LabelledView,
    planned: &PlannedPaths,
    area: Option<&str>,
    heading: &str,
) -> String {
    let reuse = reuse_items(from_path, view, planned, area);
    let changes = change_items(from_path, view, planned, area);
    let continue_line = format_not_available("Continue working", CONTINUE_UNTIL);
    format!(
        "<div class=\"three-up\">{}{}{}</div>",
        panel(heading, "Continue working", None, &[], &continue_line),
        panel(
            heading,
            "Ready to reuse",
            Some(count_text(reuse.len())),
            &reuse,
            "Nothing approved or proven here yet."
        ),
        panel(
            heading,
            "Changes to review",
            Some(count_text(changes.len())),
            &changes,
            "Nothing upstream has moved."
        ),
    )
}

struct DayOneRow {
    chip_html: String,
    question: String,
    owner_role: Option<String>,
}

struct BriefRow {
    link_html: String,
    why: String,
}

struct AreaTile {
    code: String,
    name: String,
    summary: String,
    href: Option<String>,
    pages: usize,
    bar_html: String,
    coverage: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct HomeTemplate {
    three_up_html: String,
    day_one: Vec<DayOneRow>,
    briefs: Vec<BriefRow>,
    next_up_html: String,
    legend_html: String,
    tiles: Vec<AreaTile>,
}

fn day_one_rows(view: &LabelledView, planned: &PlannedPaths) -> Vec<DayOneRow> {
    view.questions()
        .iter()
        .map(|q| &q.value)
        .filter(|q| q.day_one && q.status == QuestionStatus::Open)
        .map(|q| DayOneRow {
            chip_html: render_question_chip(
                OUTPUT_PATH,
                &q.question_id,
                Some(&q.question),
                planned,
            ),
            question: q.question.clone(),
            owner_role: q.owner_role.clone(),
        })
        .collect()
}

fn brief_rows(view: &LabelledView, planned: &PlannedPaths) -> Vec<BriefRow> {
    view.pages()
        .iter()
        .filter(|p| p.meta.r#type == PageType::Brief)
        .map(|page| {
            let why = first_sentence_of_purpose(page);
            BriefRow {
                link_html: render_page_link(
                    OUTPUT_PATH,
                    page.id.as_str(),
                    None,
                    &escape_html(&page.meta.title),
                    Some(&page.meta.title),
                    Some(&why),
                    planned,
                ),
                why,
            }
        })
        .collect()
}

fn area_tiles(view: &LabelledView, planned: &PlannedPaths) -> Vec<AreaTile> {
    let Some(corpus) = view.corpus() else {
        return Vec::new();
    };
    corpus
        .area
        .iter()
        .map(|area| {
            let code = area.code.to_ascii_lowercase();
            let counts = coverage_of(view, Some(&area.code));
            AreaTile {
                code: area.code.to_ascii_uppercase(),
                name: area.name.clone(),
                summary: area.summary.clone(),
                href: href_if_planned(OUTPUT_PATH, &format!("a/{code}.html"), planned),
                pages: view
                    .pages()
                    .iter()
                    .filter(|p| p.meta.area.as_str().eq_ignore_ascii_case(&area.code))
                    .count(),
                bar_html: coverage_bar(&counts),
                coverage: counts.summary(),
            }
        })
        .collect()
}

/// Renders the inner `<main>` content of `index.html`.
#[must_use]
pub fn render_main(view: &LabelledView, planned: &PlannedPaths) -> String {
    HomeTemplate {
        three_up_html: render_three_up(OUTPUT_PATH, view, planned, None, "h2"),
        day_one: day_one_rows(view, planned),
        briefs: brief_rows(view, planned),
        // HANDOFF section 5: its order rule "what unblocks the most" depends on ADR 0001.
        next_up_html: render_unavailable("Next up", "ADR 0001 decides its order rule"),
        legend_html: coverage_legend(None),
        tiles: area_tiles(view, planned),
    }
    .render()
    .unwrap_or_else(|e| unreachable!("index.html is infallible: {e}"))
}

/// Renders the complete `index.html` document in the global frame. Today is its canonical section.
#[must_use]
pub fn render_page(
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let main_html = render_main(view, planned);
    let built_date = match ctx {
        RenderContext::Public(edition) => {
            (!edition.built.is_empty()).then_some(edition.built.as_str())
        }
    };
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    render_frame_page(&FrameInput {
        output_path: OUTPUT_PATH,
        title: PRODUCT_TITLE,
        page_area: None,
        built_date,
        store: &store,
        crumbs: &[],
        main_html: &main_html,
        nav,
    })
}

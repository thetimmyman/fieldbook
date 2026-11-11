//! Page-type template dispatch and per-type extra blocks (`build-page-types`).
//!
//! **Responsibility.** Dispatch each [`PageType`] on `p/<page-id>.html` to its typed
//! template (`templates/types/{brief,checklist,reference,case,clause_card}.html` or
//! `templates/page.html`), supplying:
//! - `brief`: `BriefSheet` with the six `brief_parts` in order followed by `Sources`,
//!   rendering any unfilled part (`strings::empty_part()`) in italic warn (`<p class="empty-part warn">`);
//! - `checklist`: numbered task lists under `###` group headings and `PlanTimeline`
//!   over exact `Day N to Day M` windows;
//! - `reference`: table rows in `The rule` dated and marked superseded strictly from
//!   a row's unique cited claim (`as_of` at original precision, sorted newest-first via
//!   `PartialDate::first_day()` only when every row has a known date, retaining authored
//!   order on ties or when any row is undated/ambiguous);
//! - `case`: `Public record` and `Inference` subsections in `How it works` kept apart in
//!   distinct `.case-block` containers with `.ev-label` badges;
//! - `clause-card`: the six-row table first in `The rule` and question chips in
//!   `Company calibration`.

use std::collections::BTreeSet;
use std::ops::Range;

use askama::Template;
use fieldbook_core::PartialDate;
use fieldbook_core::check::PageEntry;
use fieldbook_core::cite::KeyKind;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::ClaimId;
use fieldbook_core::schema::PageType;
use fieldbook_core::strings;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::inline::{InlineRenderer, find_claim, find_question};
use crate::links::{escape_html, render_question_chip};
use crate::page::{
    ChangeNoteLine, ConflictBoxView, EvidenceCardView, MetaRowView, RenderedPart, TocItem,
};
use crate::plan::{PlanTimelineView, build_plan_timeline};
use crate::site::PlannedPaths;
use crate::sources_block::NumberedSourceRow;

/// Split view of `How it works` on a `case` page (`### Public record` vs `### Inference`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseSplitView {
    /// `true` when both `### Public record` and `### Inference` subsections were found.
    pub has_sections: bool,
    /// Rendered HTML of any introductory prose before the first `###` subsection heading.
    pub prefix_html: String,
    /// Anchor `id` of the `### Public record` heading.
    pub public_record_anchor: String,
    /// Rendered HTML of the `### Public record` subsection body.
    pub public_record_html: String,
    /// Anchor `id` of the `### Inference` heading.
    pub inference_anchor: String,
    /// Rendered HTML of the `### Inference` subsection body.
    pub inference_html: String,
}

/// Common page-header, rail, and part inputs shared by all typed page templates.
pub struct TypedPageCommon {
    /// Rendered area chip HTML.
    pub area_chip_html: String,
    /// Page `subarea` string.
    pub subarea: String,
    /// Canonical `PageType` string.
    pub page_type: String,
    /// Curriculum `stage` (`1..=8`).
    pub stage: u8,
    /// Rendered page status chip HTML.
    pub status_chip_html: String,
    /// Handling label strings.
    pub handling_chips: Vec<String>,
    /// Page title.
    pub title: String,
    /// Metadata row below the `h1`.
    pub meta_row: MetaRowView,
    /// `true` when `readiness == Readiness::SyntheticNow`.
    pub is_synthetic_now: bool,
    /// Rendered `superseded_by` links HTML.
    pub superseded_by_html: String,
    /// Part 1 (`Purpose`).
    pub purpose: RenderedPart,
    /// Part 2 (`The rule`).
    pub the_rule: RenderedPart,
    /// Part 3 (`How it works`).
    pub how_it_works: RenderedPart,
    /// Part 4 (`Worked example`).
    pub worked_example: RenderedPart,
    /// Part 5 (`Never confuse`).
    pub never_confuse: RenderedPart,
    /// Part 6 (`Company calibration`).
    pub company_calibration: RenderedPart,
    /// Part 7 (`Sources`).
    pub sources_part: RenderedPart,
    /// Part 8 (`Change note`).
    pub change_note_part: RenderedPart,
    /// Frozen empty-part text (`strings::empty_part()`).
    pub empty_part_text: &'static str,
    /// Numbered rows in Part 7 (`Sources`).
    pub sources_rows: Vec<NumberedSourceRow>,
    /// Conflict boxes in Part 5 (`Never confuse`).
    pub conflict_boxes: Vec<ConflictBoxView>,
    /// Structured change-note lines in Part 8 (`Change note`).
    pub change_entries: Vec<ChangeNoteLine>,
    /// Right-rail table of contents items.
    pub toc_items: Vec<TocItem>,
    /// Right-rail cited claim cards.
    pub evidence_cards: Vec<EvidenceCardView>,
    /// Serialized context pack plain text for the page.
    pub context_pack: String,
}

#[derive(Template)]
#[template(path = "types/brief.html")]
struct BriefTemplate {
    area_chip_html: String,
    subarea: String,
    page_type: String,
    stage: u8,
    status_chip_html: String,
    handling_chips: Vec<String>,
    title: String,
    meta_row: MetaRowView,
    is_synthetic_now: bool,
    superseded_by_html: String,
    question: RenderedPart,
    recommendation: RenderedPart,
    evidence: RenderedPart,
    options: RenderedPart,
    unknowns: RenderedPart,
    next_action: RenderedPart,
    sources_part: RenderedPart,
    empty_part_text: &'static str,
    sources_rows: Vec<NumberedSourceRow>,
    toc_items: Vec<TocItem>,
    evidence_cards: Vec<EvidenceCardView>,
    context_pack: String,
}

#[derive(Template)]
#[template(path = "types/checklist.html")]
struct ChecklistTemplate {
    area_chip_html: String,
    subarea: String,
    page_type: String,
    stage: u8,
    status_chip_html: String,
    handling_chips: Vec<String>,
    title: String,
    meta_row: MetaRowView,
    is_synthetic_now: bool,
    superseded_by_html: String,
    purpose: RenderedPart,
    the_rule: RenderedPart,
    how_it_works: RenderedPart,
    worked_example: RenderedPart,
    never_confuse: RenderedPart,
    company_calibration: RenderedPart,
    sources_part: RenderedPart,
    change_note_part: RenderedPart,
    empty_part_text: &'static str,
    plan_timeline: PlanTimelineView,
    sources_rows: Vec<NumberedSourceRow>,
    conflict_boxes: Vec<ConflictBoxView>,
    change_entries: Vec<ChangeNoteLine>,
    toc_items: Vec<TocItem>,
    evidence_cards: Vec<EvidenceCardView>,
    context_pack: String,
}

#[derive(Template)]
#[template(path = "types/reference.html")]
struct ReferenceTemplate {
    area_chip_html: String,
    subarea: String,
    page_type: String,
    stage: u8,
    status_chip_html: String,
    handling_chips: Vec<String>,
    title: String,
    meta_row: MetaRowView,
    is_synthetic_now: bool,
    superseded_by_html: String,
    purpose: RenderedPart,
    the_rule: RenderedPart,
    how_it_works: RenderedPart,
    worked_example: RenderedPart,
    never_confuse: RenderedPart,
    company_calibration: RenderedPart,
    sources_part: RenderedPart,
    change_note_part: RenderedPart,
    empty_part_text: &'static str,
    sources_rows: Vec<NumberedSourceRow>,
    conflict_boxes: Vec<ConflictBoxView>,
    change_entries: Vec<ChangeNoteLine>,
    toc_items: Vec<TocItem>,
    evidence_cards: Vec<EvidenceCardView>,
    context_pack: String,
}

#[derive(Template)]
#[template(path = "types/case.html")]
struct CaseTemplate {
    area_chip_html: String,
    subarea: String,
    page_type: String,
    stage: u8,
    status_chip_html: String,
    handling_chips: Vec<String>,
    title: String,
    meta_row: MetaRowView,
    is_synthetic_now: bool,
    superseded_by_html: String,
    purpose: RenderedPart,
    the_rule: RenderedPart,
    how_it_works: RenderedPart,
    worked_example: RenderedPart,
    never_confuse: RenderedPart,
    company_calibration: RenderedPart,
    sources_part: RenderedPart,
    change_note_part: RenderedPart,
    empty_part_text: &'static str,
    case_split: CaseSplitView,
    sources_rows: Vec<NumberedSourceRow>,
    conflict_boxes: Vec<ConflictBoxView>,
    change_entries: Vec<ChangeNoteLine>,
    toc_items: Vec<TocItem>,
    evidence_cards: Vec<EvidenceCardView>,
    context_pack: String,
}

#[derive(Template)]
#[template(path = "types/clause_card.html")]
struct ClauseCardTemplate {
    area_chip_html: String,
    subarea: String,
    page_type: String,
    stage: u8,
    status_chip_html: String,
    handling_chips: Vec<String>,
    title: String,
    meta_row: MetaRowView,
    is_synthetic_now: bool,
    superseded_by_html: String,
    purpose: RenderedPart,
    the_rule: RenderedPart,
    how_it_works: RenderedPart,
    worked_example: RenderedPart,
    never_confuse: RenderedPart,
    company_calibration: RenderedPart,
    sources_part: RenderedPart,
    change_note_part: RenderedPart,
    empty_part_text: &'static str,
    calibration_question_chips: Vec<String>,
    sources_rows: Vec<NumberedSourceRow>,
    conflict_boxes: Vec<ConflictBoxView>,
    change_entries: Vec<ChangeNoteLine>,
    toc_items: Vec<TocItem>,
    evidence_cards: Vec<EvidenceCardView>,
    context_pack: String,
}

/// Convert task lists inside `How it works` on a `checklist` page into numbered steps (`<ol class="steps">`).
#[must_use]
pub fn number_checklist_task_lists(how_html: &str) -> String {
    how_html
        .replace("<ul>\n", "<ol class=\"steps\">\n")
        .replace("</ul>\n", "</ol>\n")
}

/// Render `How it works` for a `case` page into a [`CaseSplitView`] separating
/// `### Public record` and `### Inference`.
pub fn build_case_split(page: &PageEntry, inline: &mut InlineRenderer<'_>) -> CaseSplitView {
    let how_heading = strings::parts()[2];
    let Some(part_span) = page.parts().get(how_heading) else {
        return CaseSplitView::default();
    };
    if part_span.event_range.is_empty() {
        return CaseSplitView::default();
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    let events: Vec<(usize, Event<'_>, Range<usize>)> = Parser::new_ext(&page.body, opts)
        .into_offset_iter()
        .enumerate()
        .map(|(i, (ev, r))| (i, ev, r))
        .collect();

    let start_ev = part_span.event_range.start.min(events.len());
    let end_ev = part_span.event_range.end.min(events.len());
    let mut h3_sections: Vec<(String, String, usize, usize)> = Vec::new();
    let mut first_h3_start: Option<usize> = None;

    let mut idx = start_ev;
    while idx < end_ev {
        if let Event::Start(Tag::Heading {
            level: HeadingLevel::H3,
            ..
        }) = events[idx].1
        {
            let h3_start = idx;
            if first_h3_start.is_none() {
                first_h3_start = Some(h3_start);
            }
            let anchor = page
                .parts()
                .heading_anchor(h3_start)
                .unwrap_or("section")
                .to_owned();
            let mut text = String::new();
            idx += 1;
            while idx < end_ev {
                match &events[idx].1 {
                    Event::End(TagEnd::Heading(HeadingLevel::H3)) => {
                        idx += 1;
                        break;
                    }
                    Event::Text(t) | Event::Code(t) => text.push_str(t),
                    _ => {}
                }
                idx += 1;
            }
            let body_start = idx;
            if let Some(prev) = h3_sections.last_mut() {
                prev.3 = h3_start;
            }
            h3_sections.push((text, anchor, body_start, end_ev));
            continue;
        }
        idx += 1;
    }

    let pr_sec = h3_sections.iter().find(|s| s.0 == "Public record").cloned();
    let inf_sec = h3_sections.iter().find(|s| s.0 == "Inference").cloned();

    let (Some((_, pr_anchor, pr_start, pr_end)), Some((_, inf_anchor, inf_start, inf_end))) =
        (pr_sec, inf_sec)
    else {
        return CaseSplitView::default();
    };

    let prefix_end = first_h3_start.unwrap_or(start_ev);
    let prefix_html = if start_ev < prefix_end {
        inline.render_part(start_ev..prefix_end)
    } else {
        String::new()
    };
    let public_record_html = inline.render_part(pr_start..pr_end);
    let inference_html = inline.render_part(inf_start..inf_end);

    CaseSplitView {
        has_sections: true,
        prefix_html,
        public_record_anchor: pr_anchor,
        public_record_html,
        inference_anchor: inf_anchor,
        inference_html,
    }
}

#[derive(Debug, Clone)]
struct ReferenceRowMeta {
    authored_idx: usize,
    cell_ranges: Vec<Range<usize>>,
    inferred_as_of: Option<PartialDate>,
    inferred_superseded_by: Vec<ClaimId>,
}

/// Render `The rule` for a `reference` page, applying the reference-table convention
/// (`docs/reference/corpus-format.md#page-files`):
/// - A row with **exactly one distinct claim citation** (`[@CL-...]`; repeated citations of the
///   same claim count once) receives that claim's `as_of` (at original precision) and
///   `superseded_by` status.
/// - Rows with `0` or `> 1` distinct claim citations receive no inferred date or supersession.
/// - The table sorts newest-first (by `PartialDate::first_day()`, keeping authored order on ties)
///   **only when every row in the table has a known date**; otherwise the entire table stays in
///   authored order.
pub fn render_reference_the_rule(
    page: &PageEntry,
    view: &LabelledView,
    inline: &mut InlineRenderer<'_>,
) -> RenderedPart {
    let heading = strings::parts()[1];
    let part_span = page.parts().get(heading);
    let anchor = part_span
        .and_then(|ps| page.parts().heading_anchor(ps.heading_event))
        .unwrap_or("the-rule")
        .to_owned();
    let Some(ps) = part_span else {
        return RenderedPart {
            heading: heading.to_owned(),
            anchor,
            is_empty: true,
            body_html: String::new(),
        };
    };
    if ps.event_range.is_empty() {
        return RenderedPart {
            heading: heading.to_owned(),
            anchor,
            is_empty: true,
            body_html: String::new(),
        };
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    let events: Vec<(usize, Event<'_>, Range<usize>)> = Parser::new_ext(&page.body, opts)
        .into_offset_iter()
        .enumerate()
        .map(|(i, (ev, r))| (i, ev, r))
        .collect();

    let start_ev = ps.event_range.start.min(events.len());
    let end_ev = ps.event_range.end.min(events.len());
    let mut out = String::new();
    let mut cursor = start_ev;
    let mut idx = start_ev;
    let mut open_bq_starts: Vec<usize> = Vec::new();
    let mut split_bq_precise: Vec<(usize, bool)> = Vec::new();

    while idx < end_ev {
        match events[idx].1 {
            Event::Start(Tag::BlockQuote(_)) => {
                open_bq_starts.push(idx);
                idx += 1;
                continue;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                let bq_start = open_bq_starts.pop();
                if let Some(bs) = bq_start
                    && bs < cursor
                {
                    if cursor < idx {
                        out.push_str(&inline.render_part(cursor..idx));
                    }
                    let is_precise = split_bq_precise
                        .iter()
                        .position(|(s, _)| *s == bs)
                        .is_some_and(|pos| split_bq_precise.remove(pos).1);
                    if is_precise {
                        out.push_str("</div>\n");
                    } else {
                        out.push_str("</blockquote>\n");
                    }
                    idx += 1;
                    cursor = idx;
                    continue;
                }
                idx += 1;
                continue;
            }
            Event::Start(Tag::Table(_)) => {
                for &bs in &open_bq_starts {
                    if bs >= cursor && !split_bq_precise.iter().any(|(s, _)| *s == bs) {
                        let is_precise =
                            blockquote_starts_with_precise(&page.body, &events[bs + 1..idx]);
                        split_bq_precise.push((bs, is_precise));
                    }
                }
                if cursor < idx {
                    out.push_str(&inline.render_part(cursor..idx));
                }
                let table_start = idx;
                let mut table_end = idx + 1;
                while table_end < end_ev {
                    if matches!(events[table_end].1, Event::End(TagEnd::Table)) {
                        table_end += 1;
                        break;
                    }
                    table_end += 1;
                }
                out.push_str(&render_single_reference_table(
                    page,
                    view,
                    inline,
                    &events[table_start..table_end],
                ));
                idx = table_end;
                cursor = table_end;
                continue;
            }
            _ => {
                idx += 1;
            }
        }
    }

    if cursor < end_ev {
        out.push_str(&inline.render_part(cursor..end_ev));
    }

    let empty_para = format!("<p>{}</p>", escape_html(strings::empty_part()));
    let is_empty = out.trim().is_empty() || out.trim() == empty_para;

    RenderedPart {
        heading: heading.to_owned(),
        anchor,
        is_empty,
        body_html: out,
    }
}

fn blockquote_starts_with_precise(
    body: &str,
    following: &[(usize, Event<'_>, Range<usize>)],
) -> bool {
    let mut depth = 1usize;
    for (_, ev, r) in following {
        match ev {
            Event::Start(Tag::BlockQuote(_)) => depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return false;
                }
            }
            Event::Text(_) => {
                return body[r.clone()].trim_start().starts_with("Precise:");
            }
            _ => {}
        }
    }
    false
}

fn render_single_reference_table(
    page: &PageEntry,
    view: &LabelledView,
    inline: &mut InlineRenderer<'_>,
    table_events: &[(usize, Event<'_>, Range<usize>)],
) -> String {
    let mut head_cell_ranges: Vec<Range<usize>> = Vec::new();
    let mut rows: Vec<ReferenceRowMeta> = Vec::new();

    let mut in_head = false;
    let mut in_row = false;
    let mut current_cells: Vec<Range<usize>> = Vec::new();
    let mut current_row_byte_range: Option<Range<usize>> = None;
    let mut cell_start: Option<usize> = None;

    for (ev_idx, ev, byte_range) in table_events {
        match ev {
            Event::Start(Tag::TableHead) => {
                in_head = true;
            }
            Event::End(TagEnd::TableHead) => {
                in_head = false;
            }
            Event::Start(Tag::TableRow) if !in_head => {
                in_row = true;
                current_cells.clear();
                current_row_byte_range = Some(byte_range.clone());
            }
            Event::End(TagEnd::TableRow) if in_row => {
                in_row = false;
                let row_span = current_row_byte_range
                    .take()
                    .map(|r| r.start..byte_range.end.max(r.end));
                let distinct_claims = row_span
                    .as_ref()
                    .map(|s| distinct_claims_in_text_ranges(page, view, std::slice::from_ref(s)))
                    .unwrap_or_default();
                let (inferred_as_of, inferred_superseded_by) = if distinct_claims.len() == 1 {
                    let cid = distinct_claims.iter().next().cloned();
                    let crow = cid.as_ref().and_then(|id| find_claim(view, id));
                    (
                        crow.and_then(|c| c.as_of),
                        crow.map(|c| c.superseded_by.clone()).unwrap_or_default(),
                    )
                } else {
                    (None, Vec::new())
                };
                rows.push(ReferenceRowMeta {
                    authored_idx: rows.len(),
                    cell_ranges: current_cells.clone(),
                    inferred_as_of,
                    inferred_superseded_by,
                });
            }
            Event::Start(Tag::TableCell) => {
                cell_start = Some(*ev_idx + 1);
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(c_start) = cell_start.take() {
                    let range = c_start..*ev_idx;
                    if in_head {
                        head_cell_ranges.push(range);
                    } else if in_row {
                        current_cells.push(range);
                    }
                }
            }
            _ => {
                if in_row && let Some(r) = current_row_byte_range.as_mut() {
                    r.start = r.start.min(byte_range.start);
                    r.end = r.end.max(byte_range.end);
                }
            }
        }
    }

    let all_rows_dated = !rows.is_empty() && rows.iter().all(|r| r.inferred_as_of.is_some());
    if all_rows_dated {
        rows.sort_by(|a, b| {
            let da = a.inferred_as_of.map(PartialDate::first_day);
            let db = b.inferred_as_of.map(PartialDate::first_day);
            db.cmp(&da)
                .then_with(|| a.authored_idx.cmp(&b.authored_idx))
        });
    }

    let mut out = String::from("<div class=\"table-wrap\"><table>\n<thead><tr>");
    for h_range in head_cell_ranges {
        out.push_str("<th scope=\"col\">");
        out.push_str(&inline.render_part(h_range));
        out.push_str("</th>");
    }
    out.push_str("</tr></thead>\n<tbody>\n");

    for row in rows {
        let is_superseded = !row.inferred_superseded_by.is_empty();
        let sup_joined = row
            .inferred_superseded_by
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");

        out.push_str("<tr");
        if is_superseded {
            out.push_str(" class=\"row-superseded\"");
        }
        if let Some(pd) = row.inferred_as_of {
            out.push_str(&format!(" data-as-of=\"{}\"", escape_html(&pd.to_string())));
        }
        if is_superseded {
            out.push_str(&format!(
                " data-superseded-by=\"{}\"",
                escape_html(&sup_joined)
            ));
        }
        out.push('>');

        let cell_count = row.cell_ranges.len();
        for (c_idx, c_range) in row.cell_ranges.into_iter().enumerate() {
            out.push_str("<td>");
            let cell_html = inline.render_part(c_range);
            if is_superseded {
                out.push_str("<del>");
                out.push_str(&cell_html);
                out.push_str("</del>");
            } else {
                out.push_str(&cell_html);
            }
            if c_idx + 1 == cell_count {
                if let Some(pd) = row.inferred_as_of {
                    let pd_str = pd.to_string();
                    out.push_str(&format!(
                        " <span class=\"ref-as-of\">As of <time datetime=\"{}\">{}</time></span>",
                        escape_html(&pd_str),
                        escape_html(&pd_str)
                    ));
                }
                if is_superseded {
                    out.push_str(&format!(
                        " <span class=\"chip warn\">Superseded by {}</span>",
                        escape_html(&sup_joined)
                    ));
                }
            }
            out.push_str("</td>");
        }
        out.push_str("</tr>\n");
    }

    out.push_str("</tbody></table></div>\n");
    out
}

fn distinct_claims_in_text_ranges(
    page: &PageEntry,
    view: &LabelledView,
    text_ranges: &[Range<usize>],
) -> BTreeSet<ClaimId> {
    let mut set = BTreeSet::new();
    for cite in view.citations(&page.id) {
        if cite.key_kind != KeyKind::Claim {
            continue;
        }
        let in_row = text_ranges
            .iter()
            .any(|r| cite.span.start >= r.start && cite.span.end <= r.end);
        if in_row && let Ok(cid) = ClaimId::parse(&cite.key) {
            set.insert(cid);
        }
    }
    set
}

/// Build the question chips for `Company calibration` on a `clause-card` page.
#[must_use]
pub fn build_clause_calibration_questions(
    from_path: &str,
    page: &PageEntry,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut chips = Vec::new();
    for qid in &page.meta.questions {
        if seen.insert(qid.clone()) {
            let q_body = find_question(view, qid).map(|q| q.question.as_str());
            chips.push(render_question_chip(from_path, qid, q_body, planned));
        }
    }
    chips
}

/// Render a `brief` page using `templates/types/brief.html`.
#[must_use]
pub fn render_brief(common: TypedPageCommon, mut brief_parts: Vec<RenderedPart>) -> String {
    let mut drain = brief_parts.drain(..);
    let next_part = |d: &mut std::vec::Drain<'_, RenderedPart>, name: &str, slug: &str| {
        d.next().unwrap_or_else(|| RenderedPart {
            heading: name.to_owned(),
            anchor: slug.to_owned(),
            is_empty: true,
            body_html: String::new(),
        })
    };
    let bnames = strings::brief_parts();
    let question = next_part(&mut drain, bnames[0], "question");
    let recommendation = next_part(&mut drain, bnames[1], "recommendation");
    let evidence = next_part(&mut drain, bnames[2], "evidence");
    let options = next_part(&mut drain, bnames[3], "options");
    let unknowns = next_part(&mut drain, bnames[4], "unknowns");
    let next_action = next_part(&mut drain, bnames[5], "next-action");

    let tpl = BriefTemplate {
        area_chip_html: common.area_chip_html,
        subarea: common.subarea,
        page_type: common.page_type,
        stage: common.stage,
        status_chip_html: common.status_chip_html,
        handling_chips: common.handling_chips,
        title: common.title,
        meta_row: common.meta_row,
        is_synthetic_now: common.is_synthetic_now,
        superseded_by_html: common.superseded_by_html,
        question,
        recommendation,
        evidence,
        options,
        unknowns,
        next_action,
        sources_part: common.sources_part,
        empty_part_text: common.empty_part_text,
        sources_rows: common.sources_rows,
        toc_items: common.toc_items,
        evidence_cards: common.evidence_cards,
        context_pack: common.context_pack,
    };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("types/brief.html is infallible: {e}"))
}

/// Render a typed eight-part page (`checklist`, `reference`, `case`, `clause-card`), or return
/// `Err(common)` when the page type uses the base `templates/page.html` template (`concept`,
/// `procedure`, `calc`, `template`).
///
/// # Errors
///
/// Returns `Err(common)` when `page.meta.r#type` is not one of the specialized eight-part types.
pub fn render_typed_eight_part(
    page: &PageEntry,
    view: &LabelledView,
    planned: &PlannedPaths,
    common: Box<TypedPageCommon>,
    case_split: CaseSplitView,
) -> Result<String, Box<TypedPageCommon>> {
    let from_path = format!("p/{}.html", page.id);
    match page.meta.r#type {
        PageType::Checklist => {
            let plan_timeline = build_plan_timeline(page);
            let mut how_it_works = common.how_it_works;
            how_it_works.body_html = number_checklist_task_lists(&how_it_works.body_html);
            let tpl = ChecklistTemplate {
                area_chip_html: common.area_chip_html,
                subarea: common.subarea,
                page_type: common.page_type,
                stage: common.stage,
                status_chip_html: common.status_chip_html,
                handling_chips: common.handling_chips,
                title: common.title,
                meta_row: common.meta_row,
                is_synthetic_now: common.is_synthetic_now,
                superseded_by_html: common.superseded_by_html,
                purpose: common.purpose,
                the_rule: common.the_rule,
                how_it_works,
                worked_example: common.worked_example,
                never_confuse: common.never_confuse,
                company_calibration: common.company_calibration,
                sources_part: common.sources_part,
                change_note_part: common.change_note_part,
                empty_part_text: common.empty_part_text,
                plan_timeline,
                sources_rows: common.sources_rows,
                conflict_boxes: common.conflict_boxes,
                change_entries: common.change_entries,
                toc_items: common.toc_items,
                evidence_cards: common.evidence_cards,
                context_pack: common.context_pack,
            };
            Ok(tpl
                .render()
                .unwrap_or_else(|e| unreachable!("types/checklist.html is infallible: {e}")))
        }
        PageType::Reference => {
            let tpl = ReferenceTemplate {
                area_chip_html: common.area_chip_html,
                subarea: common.subarea,
                page_type: common.page_type,
                stage: common.stage,
                status_chip_html: common.status_chip_html,
                handling_chips: common.handling_chips,
                title: common.title,
                meta_row: common.meta_row,
                is_synthetic_now: common.is_synthetic_now,
                superseded_by_html: common.superseded_by_html,
                purpose: common.purpose,
                the_rule: common.the_rule,
                how_it_works: common.how_it_works,
                worked_example: common.worked_example,
                never_confuse: common.never_confuse,
                company_calibration: common.company_calibration,
                sources_part: common.sources_part,
                change_note_part: common.change_note_part,
                empty_part_text: common.empty_part_text,
                sources_rows: common.sources_rows,
                conflict_boxes: common.conflict_boxes,
                change_entries: common.change_entries,
                toc_items: common.toc_items,
                evidence_cards: common.evidence_cards,
                context_pack: common.context_pack,
            };
            Ok(tpl
                .render()
                .unwrap_or_else(|e| unreachable!("types/reference.html is infallible: {e}")))
        }
        PageType::Case => {
            let tpl = CaseTemplate {
                area_chip_html: common.area_chip_html,
                subarea: common.subarea,
                page_type: common.page_type,
                stage: common.stage,
                status_chip_html: common.status_chip_html,
                handling_chips: common.handling_chips,
                title: common.title,
                meta_row: common.meta_row,
                is_synthetic_now: common.is_synthetic_now,
                superseded_by_html: common.superseded_by_html,
                purpose: common.purpose,
                the_rule: common.the_rule,
                how_it_works: common.how_it_works,
                worked_example: common.worked_example,
                never_confuse: common.never_confuse,
                company_calibration: common.company_calibration,
                sources_part: common.sources_part,
                change_note_part: common.change_note_part,
                empty_part_text: common.empty_part_text,
                case_split,
                sources_rows: common.sources_rows,
                conflict_boxes: common.conflict_boxes,
                change_entries: common.change_entries,
                toc_items: common.toc_items,
                evidence_cards: common.evidence_cards,
                context_pack: common.context_pack,
            };
            Ok(tpl
                .render()
                .unwrap_or_else(|e| unreachable!("types/case.html is infallible: {e}")))
        }
        PageType::ClauseCard => {
            let calibration_question_chips =
                build_clause_calibration_questions(&from_path, page, view, planned);
            let tpl = ClauseCardTemplate {
                area_chip_html: common.area_chip_html,
                subarea: common.subarea,
                page_type: common.page_type,
                stage: common.stage,
                status_chip_html: common.status_chip_html,
                handling_chips: common.handling_chips,
                title: common.title,
                meta_row: common.meta_row,
                is_synthetic_now: common.is_synthetic_now,
                superseded_by_html: common.superseded_by_html,
                purpose: common.purpose,
                the_rule: common.the_rule,
                how_it_works: common.how_it_works,
                worked_example: common.worked_example,
                never_confuse: common.never_confuse,
                company_calibration: common.company_calibration,
                sources_part: common.sources_part,
                change_note_part: common.change_note_part,
                empty_part_text: common.empty_part_text,
                calibration_question_chips,
                sources_rows: common.sources_rows,
                conflict_boxes: common.conflict_boxes,
                change_entries: common.change_entries,
                toc_items: common.toc_items,
                evidence_cards: common.evidence_cards,
                context_pack: common.context_pack,
            };
            Ok(tpl
                .render()
                .unwrap_or_else(|e| unreachable!("types/clause_card.html is infallible: {e}")))
        }
        PageType::Calc => {
            let mut common = common;
            common
                .worked_example
                .body_html
                .push_str(&crate::calc_table::calc_table(&page.body));
            Err(common)
        }
        PageType::Concept | PageType::Procedure | PageType::Template | PageType::Brief => {
            Err(common)
        }
    }
}

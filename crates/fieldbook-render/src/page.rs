//! Reader page renderer (`p/<page-id>.html`) in the frozen part order (`build-page`).
//!
//! **Responsibility.** Render a [`PageEntry`] and [`LabelledView`] into `templates/page.html`
//! (and wrap it in the global frame via [`crate::frame::wrap`]), keeping the eight parts in
//! the order of [`fieldbook_core::strings::parts`], computing `review_by` through
//! [`fieldbook_core::review::page_review_by`], building Part 7 (`Sources`) strictly from
//! resolved citations via [`crate::sources_block`], and rendering Part 8 (`Change note`),
//! `ConflictBox`es, and `EvidenceCard`s.

use std::collections::BTreeSet;

use askama::Template;
use fieldbook_core::CivilDate;
use fieldbook_core::assist::PackSource;
use fieldbook_core::check::PageEntry;
use fieldbook_core::cite::KeyKind;
use fieldbook_core::date::{PartialDate, review_by};
use fieldbook_core::egress::{Dest, LabelledView};
use fieldbook_core::ids::{ClaimId, ConflictId};
use fieldbook_core::review::{newest_change_note_date, page_review_by};
use fieldbook_core::schema::{
    ChangeClass, ClaimKind, ClaimRow, ConflictStatus, PageStatus, PageType, Readiness, Verification,
};
use fieldbook_core::strings;
use pulldown_cmark::{Options, Parser};

use crate::chip;
use crate::cite::OriginalAvailability;
use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::inline::{
    CitationTarget, InlineRenderer, find_claim, find_conflict, find_page, find_question,
    first_sentence_of_purpose,
};
use crate::links::{
    escape_html, render_area_chip, render_claim_chip, render_conflict_chip, render_page_link,
    render_question_chip,
};
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::site::PlannedPaths;
use crate::sources_block::{
    ICON_ALERT, ICON_CHECK, ICON_CLOCK, NumberedSourceRow, build_sources_rows, claim_citation_chip,
    render_verification_chips,
};

/// Metadata row (`parts/meta_row.html`) below the page `h1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaRowView {
    /// Page identifier (`<area>-<slug>`).
    pub page_id: String,
    /// Page `as_of` date (`YYYY-MM-DD`).
    pub as_of: String,
    /// Computed `review_by` date (`YYYY-MM-DD`).
    pub review_by: String,
    /// Number of distinct cited claims with `Verification::PrimarySourceSupported`.
    pub supported_claims: usize,
    /// Total number of distinct cited claims on the page.
    pub total_claims: usize,
    /// Latest `verified_on` date across cited claims (`YYYY-MM-DD`), or `None` when none is verified.
    pub last_verified: Option<String>,
    /// Canonical `readiness` string (`PUBLIC-NOW`, `SYNTHETIC-NOW`, ...).
    pub readiness: String,
    /// Hidden per-claim rows of the overdue banner, one per computed review date, with the `verify`
    /// chip or, until that command lands, one landing line for the fold.
    pub due_claims_html: String,
    /// True when the page writes the hidden review-overdue banner, false on an unreleased build,
    /// where the build status banner says each page shows its review date.
    pub overdue_banner: bool,
}

/// A part that the reader shows as a closed fold under its heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartFold {
    /// Distinct questions the part lists, shown in the fold's summary; none shows no count.
    pub question_count: usize,
}

/// One rendered part section (`<h2>` + body or empty-part line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPart {
    /// Frozen part heading text.
    pub heading: String,
    /// Heading `id` anchor from `PageEntry::parts`.
    pub anchor: String,
    /// `true` when the part has no prose content.
    pub is_empty: bool,
    /// Rendered HTML of the part body.
    pub body_html: String,
    /// `Some` when the part is shown as a closed fold (Company calibration with no company overlay).
    pub fold: Option<PartFold>,
}

/// One item in the right-rail "On this page" table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocItem {
    /// Part heading text.
    pub heading: String,
    /// Target anchor `id` (without leading `#`).
    pub anchor: String,
}

/// One parsed line of Part 8 (`parts/change_note.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeNoteLine {
    /// A structured `- CLASS, YYYY-MM-DD: sentence.` item.
    Structured {
        /// Validated [`ChangeClass`] string (`NEW_KNOWLEDGE`, ...).
        class_name: String,
        /// Validated [`CivilDate`] string (`YYYY-MM-DD`).
        date: String,
        /// Change sentence following `: `.
        sentence: String,
    },
    /// Any other non-empty line in the change note part, rendered as plain text.
    Plain(String),
}

/// One `ConflictBox` rendered under `Never confuse` (`parts/conflict_box.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictBoxView {
    /// Conflict identifier (`XC-nn`).
    pub conflict_id: String,
    /// `true` when `status == ConflictStatus::Resolved`.
    pub is_resolved: bool,
    /// Status chip HTML (`Resolved` or `Unresolved`).
    pub status_chip_html: String,
    /// Conflict ID chip HTML (linked to `conflicts.html#<id>` if planned).
    pub conflict_chip_html: String,
    /// Conflict topic.
    pub topic: String,
    /// Competing position strings.
    pub positions: Vec<String>,
    /// Disputed claim chips HTML.
    pub claims_html: String,
    /// Settling question chip HTML when `question_id` is present.
    pub question_chip_html: Option<String>,
    /// Resolution text when resolved.
    pub resolution: Option<String>,
    /// Rationale text when resolved.
    pub rationale: Option<String>,
    /// Resolution date (`YYYY-MM-DD`) when resolved.
    pub resolved_on: Option<String>,
}

/// One `EvidenceCard` rendered in the right-hand `EvidenceRail` (`parts/evidence_card.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceCardView {
    /// Claim identifier (`CL-...`).
    pub claim_id: String,
    /// Claim ID chip HTML (linked to the claim's part page `claims/<AREA>-<part>.html#<id>` if planned).
    pub claim_chip_html: String,
    /// Verification `StatusChip` HTML.
    pub status_chip_html: String,
    /// Canonical `ClaimKind` string (`fact`, `assumption`, ...).
    pub kind_label: String,
    /// Claim statement text.
    pub statement: String,
    /// Optional verbatim quote.
    pub quote: Option<String>,
    /// Source `CitationChip` HTML (or `No source yet`).
    pub citation_chip_html: String,
    /// Canonical `Applicability` string.
    pub applicability: String,
    /// Canonical `Adoption` string.
    pub adoption: String,
    /// Canonical `Volatility` string (`V1`, `V2`, `V3`).
    pub volatility: String,
    /// Optional `verified_on` date (`YYYY-MM-DD`).
    pub verified_on: Option<String>,
    /// Optional `verified_by_role` string (`owner`, `lead`).
    pub verified_by_role: Option<String>,
    /// Optional question chip HTML when the claim waits on a question.
    pub question_chip_html: Option<String>,
    /// Optional conflict chip HTML when the claim waits on a conflict.
    pub conflict_chip_html: Option<String>,
}

#[derive(Template)]
#[template(path = "page.html")]
struct PageTemplate {
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
    is_brief: bool,
    brief_parts: Vec<RenderedPart>,
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
    rail_lists_html: String,
    context_pack: String,
}

fn render_page_status_chip(status: PageStatus) -> String {
    match status {
        PageStatus::Stable => {
            format!("<span class=\"chip ok\">{ICON_CHECK}<span>Stable</span></span>")
        }
        PageStatus::Reviewed => {
            format!("<span class=\"chip accent\">{ICON_CHECK}<span>Reviewed</span></span>")
        }
        PageStatus::Draft => {
            format!("<span class=\"chip\">{ICON_CLOCK}<span>Draft</span></span>")
        }
    }
}

fn parse_change_note_line(trimmed: &str) -> ChangeNoteLine {
    if let Some(after_bullet) = trimmed.strip_prefix("- ")
        && let Some((class_part, rest)) = after_bullet.split_once(", ")
        && ChangeClass::parse(class_part).is_ok()
        && let Some((date_part, sentence_part)) = rest.split_once(": ")
        && CivilDate::parse(date_part).is_ok()
        && !sentence_part.trim().is_empty()
    {
        return ChangeNoteLine::Structured {
            class_name: class_part.to_owned(),
            date: date_part.to_owned(),
            sentence: sentence_part.trim().to_owned(),
        };
    }
    ChangeNoteLine::Plain(trimmed.to_owned())
}

fn parse_change_note(body: &str, page: &PageEntry) -> Vec<ChangeNoteLine> {
    let change_heading = strings::parts()[7];
    let Some(part_span) = page.parts().get(change_heading) else {
        return Vec::new();
    };
    if part_span.event_range.is_empty() {
        return Vec::new();
    }
    let body_start_line = page.parts().body_start_line();
    let mut entries = Vec::new();
    for (idx, line) in body.lines().enumerate() {
        let line_num = body_start_line + idx;
        if line_num <= part_span.line || line_num > part_span.end_line {
            continue;
        }
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            entries.push(parse_change_note_line(trimmed));
        }
    }
    entries
}

fn build_part_by_name(
    heading: &str,
    fallback_anchor: &str,
    render_prose: bool,
    page: &PageEntry,
    inline: &mut InlineRenderer<'_>,
) -> RenderedPart {
    let part_span = page.parts().get(heading);
    let anchor = part_span
        .and_then(|ps| page.parts().heading_anchor(ps.heading_event))
        .unwrap_or(fallback_anchor)
        .to_owned();
    let body_html = match (render_prose, part_span) {
        (true, Some(ps)) if !ps.event_range.is_empty() => {
            inline.render_part(ps.event_range.clone())
        }
        _ => String::new(),
    };
    let empty_para = format!("<p>{}</p>", escape_html(strings::empty_part()));
    let empty_li = format!(
        "<ul>\n<li>{}</li>\n</ul>",
        escape_html(strings::empty_part())
    );
    let trimmed_body = body_html.trim();
    let is_empty = part_span.is_none_or(|ps| ps.event_range.is_empty())
        || trimmed_body == empty_para
        || trimmed_body == empty_li;
    RenderedPart {
        heading: heading.to_owned(),
        anchor,
        is_empty,
        body_html,
        fold: None,
    }
}

/// Distinct question ids cited in the `Company calibration` part's text.
fn questions_in_part(page: &PageEntry, view: &LabelledView) -> BTreeSet<String> {
    let mut distinct = BTreeSet::new();
    let Some(part_name) = strings::parts().get(5) else {
        return distinct;
    };
    let Some(ps) = page.parts().get(part_name) else {
        return distinct;
    };
    if ps.event_range.is_empty() {
        return distinct;
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    let events: Vec<(_, std::ops::Range<usize>)> = Parser::new_ext(&page.body, opts)
        .into_offset_iter()
        .collect();

    let start_idx = ps.event_range.start.min(events.len());
    let end_idx = ps.event_range.end.min(events.len());
    if start_idx >= end_idx {
        return distinct;
    }

    let Some(start_ev) = events.get(start_idx) else {
        return distinct;
    };
    let Some(end_ev) = events.get(end_idx - 1) else {
        return distinct;
    };
    let r_start = start_ev.1.start;
    let r_end = end_ev.1.end;
    if r_start >= r_end {
        return distinct;
    }

    for cite in view.citations(&page.id) {
        if cite.key_kind == KeyKind::Question
            && cite.span.start >= r_start
            && cite.span.end <= r_end
        {
            distinct.insert(cite.key.clone());
        }
    }
    distinct
}

fn collect_distinct_cited_claims(
    page: &PageEntry,
    view: &LabelledView,
    inline: &InlineRenderer<'_>,
) -> Vec<ClaimId> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for marker in inline.markers() {
        if let CitationTarget::Claim(ref cid) = marker.target
            && seen.insert(cid.clone())
        {
            ordered.push(cid.clone());
        }
    }
    for c in view.citations(&page.id) {
        if c.key_kind == KeyKind::Claim
            && let Ok(cid) = ClaimId::parse(&c.key)
            && seen.insert(cid.clone())
        {
            ordered.push(cid);
        }
    }
    ordered
}

/// A claim's `review_by`: the later known of `as_of` and `verified_on` plus its volatility
/// interval (`corpus-format.md#dates`); unknown when both are absent, so it is never overdue.
fn claim_review_by(claim: &ClaimRow) -> Option<CivilDate> {
    let as_of = claim.as_of.map(PartialDate::first_day);
    let base = match (as_of, claim.verified_on) {
        (Some(a), Some(v)) => a.max(v),
        (Some(a), None) => a,
        (None, Some(v)) => v,
        (None, None) => return None,
    };
    Some(review_by(base, claim.volatility))
}

/// The per-claim part of the overdue banner, a fold so that the rows never push the page down.
/// Every row is written hidden with its computed date; `app.js` reveals a row by the viewer's own
/// date and writes how many are due in the summary, so the build reads no clock.
///
/// `verify_landed` is [`chip::VERIFY_LANDED`] on every page; it is a parameter so that both
/// branches stay tested.
#[must_use]
pub fn render_due_claims(
    from_path: &str,
    claim_ids: &[ClaimId],
    view: &LabelledView,
    planned: &PlannedPaths,
    verify_landed: bool,
) -> String {
    let mut rows = String::new();
    for cid in claim_ids {
        let Some(claim) = find_claim(view, cid) else {
            continue;
        };
        let Some(date) = claim_review_by(claim) else {
            continue;
        };
        let action = if verify_landed {
            let Ok(command) = chip::quote(chip::KEY_VERIFY, &[("claim-id", cid.as_str())]) else {
                continue;
            };
            chip::render(&command)
        } else {
            String::new()
        };
        rows.push_str(&format!(
            "<li class=\"due-claim\" data-claim-review-by=\"{date}\" hidden>{} <span class=\"meta\">Review date <time datetime=\"{date}\">{date}</time></span>{}</li>",
            render_claim_chip(from_path, cid, planned),
            action,
        ));
    }
    if rows.is_empty() {
        return String::new();
    }
    let how = if verify_landed {
        "<p>Read the cited original, then run the command with the date you checked it in place of YYYY-MM-DD.</p>"
            .to_owned()
    } else {
        chip::render_or_landing(None, chip::VERIFY_THING, chip::VERIFY_UNTIL)
    };
    format!(
        "<div class=\"due-claims\" hidden><details class=\"fold\"><summary><b>Claims due for a check</b> <span class=\"due-count mono\"></span></summary>{how}<ul>{rows}</ul></details></div>"
    )
}

fn collect_distinct_conflicts(page: &PageEntry, view: &LabelledView) -> Vec<ConflictId> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for cid in &page.meta.conflicts {
        if seen.insert(cid.clone()) {
            ordered.push(cid.clone());
        }
    }
    for c in view.citations(&page.id) {
        if c.key_kind == KeyKind::Conflict
            && let Ok(cid) = ConflictId::parse(&c.key)
            && seen.insert(cid.clone())
        {
            ordered.push(cid);
        }
    }
    ordered
}

fn build_conflict_boxes(
    from_path: &str,
    conflict_ids: &[ConflictId],
    view: &LabelledView,
    planned: &PlannedPaths,
) -> Vec<ConflictBoxView> {
    let mut out = Vec::new();
    for cid in conflict_ids {
        let Some(row) = find_conflict(view, cid) else {
            continue;
        };
        let is_resolved = row.status == ConflictStatus::Resolved;
        let status_chip_html = if is_resolved {
            format!("<span class=\"chip ok\">{ICON_CHECK}<span>Resolved</span></span>")
        } else {
            format!("<span class=\"chip warn\">{ICON_ALERT}<span>Unresolved</span></span>")
        };
        let conflict_chip_html = render_conflict_chip(from_path, cid, Some(&row.topic), planned);
        let claims_html = row
            .claim_ids
            .iter()
            .map(|claim_id| render_claim_chip(from_path, claim_id, planned))
            .collect::<Vec<_>>()
            .join(" ");
        let question_chip_html = row.question_id.as_ref().map(|qid| {
            let q_body = find_question(view, qid).map(|q| q.question.as_str());
            render_question_chip(from_path, qid, q_body, planned)
        });
        out.push(ConflictBoxView {
            conflict_id: cid.to_string(),
            is_resolved,
            status_chip_html,
            conflict_chip_html,
            topic: row.topic.clone(),
            positions: row.positions.clone(),
            claims_html,
            question_chip_html,
            resolution: row.resolution.clone(),
            rationale: row.rationale.clone(),
            resolved_on: row.resolved_on.map(|d| d.to_string()),
        });
    }
    out
}

fn build_evidence_cards(
    from_path: &str,
    claim_ids: &[ClaimId],
    view: &LabelledView,
    planned: &PlannedPaths,
    availability: &OriginalAvailability,
) -> Vec<EvidenceCardView> {
    let mut out = Vec::new();
    for cid in claim_ids {
        let claim = find_claim(view, cid);
        let ver = claim.map_or(Verification::Unchecked, |c| c.verification);
        let kind = claim.map_or(ClaimKind::Fact, |c| c.kind);
        let claim_chip_html = render_claim_chip(from_path, cid, planned);
        let status_chip_html = render_verification_chips(ver, kind);
        let citation_chip_html = claim_citation_chip(from_path, claim, view, planned, availability);
        out.push(EvidenceCardView {
            claim_id: cid.to_string(),
            claim_chip_html,
            status_chip_html,
            kind_label: kind.as_str().to_owned(),
            statement: claim.map_or_else(String::new, |c| c.statement.clone()),
            quote: claim.and_then(|c| c.quote.clone()),
            citation_chip_html,
            applicability: claim.map_or_else(
                || "pending_company_confirmation".to_owned(),
                |c| c.applicability.as_str().to_owned(),
            ),
            adoption: claim
                .map_or_else(|| "proposed".to_owned(), |c| c.adoption.as_str().to_owned()),
            volatility: claim.map_or_else(|| "V2".to_owned(), |c| c.volatility.as_str().to_owned()),
            verified_on: claim.and_then(|c| c.verified_on.map(|d| d.to_string())),
            verified_by_role: claim.and_then(|c| c.verified_by_role.map(|r| r.as_str().to_owned())),
            question_chip_html: claim.and_then(|c| c.question_id.as_ref()).map(|qid| {
                let q_body = find_question(view, qid).map(|q| q.question.as_str());
                render_question_chip(from_path, qid, q_body, planned)
            }),
            conflict_chip_html: claim.and_then(|c| c.conflict_id.as_ref()).map(|xcid| {
                let xc_body = find_conflict(view, xcid).map(|xc| xc.topic.as_str());
                render_conflict_chip(from_path, xcid, xc_body, planned)
            }),
        });
    }
    out
}

/// Default character budget for context packs when unspecified in corpus settings (`HG-02`).
const DEFAULT_PACK_BUDGET_CHARS: usize = 12_000;

/// Render the inner `<main>` content (`templates/page.html`) for `page`.
#[must_use]
pub fn render_main(page: &PageEntry, view: &LabelledView, planned: &PlannedPaths) -> String {
    render_main_in(page, view, &StoreContext::Public, planned, true)
}

/// Render the inner `<main>` content for `page` in the given store context; with no company overlay (`StoreContext::Public`) the Company calibration part is a closed fold, in a company view it is open.
///
/// `overdue_banner` is true when the page writes the hidden review-overdue banner,
/// false on an unreleased build.
#[must_use]
pub fn render_main_in(
    page: &PageEntry,
    view: &LabelledView,
    store: &StoreContext,
    planned: &PlannedPaths,
    overdue_banner: bool,
) -> String {
    let packs = PackSource::new(view);
    let links = crate::related::LinkIndex::new(view);
    render_main_with_packs(page, view, store, planned, &packs, &links, overdue_banner)
}

/// Render the inner `<main>` content (`templates/page.html`) for `page` in the given store context, reusing pre-built context packs.
///
/// `overdue_banner` is true when the page writes the hidden review-overdue banner,
/// false on an unreleased build where the build status banner says each page shows its review date.
#[must_use]
pub fn render_main_with_packs(
    page: &PageEntry,
    view: &LabelledView,
    store: &StoreContext,
    planned: &PlannedPaths,
    packs: &PackSource<'_>,
    links: &crate::related::LinkIndex,
    overdue_banner: bool,
) -> String {
    let from_path = format!("p/{}.html", page.id);
    let area_title = view
        .corpus()
        .and_then(|ct| {
            ct.area
                .iter()
                .find(|a| a.code.eq_ignore_ascii_case(page.meta.area.as_str()))
        })
        .map(|a| a.name.as_str());
    let area_chip_html = render_area_chip(&from_path, &page.meta.area, area_title, planned);
    let status_chip_html = render_page_status_chip(page.meta.status);

    let mut inline = InlineRenderer::new(&from_path, page, view, planned);
    let part_names = strings::parts();
    let is_brief = page.meta.r#type == PageType::Brief;

    let mut brief_parts = Vec::new();
    let mut case_split = crate::types::CaseSplitView::default();
    let (
        purpose,
        the_rule,
        how_it_works,
        worked_example,
        never_confuse,
        mut company_calibration,
        sources_part,
        change_note_part,
    ) = if is_brief {
        for &bname in strings::brief_parts() {
            let fallback = bname.to_ascii_lowercase().replace(' ', "-");
            brief_parts.push(build_part_by_name(
                bname,
                &fallback,
                true,
                page,
                &mut inline,
            ));
        }
        let dummy = |name: &str, slug: &str| RenderedPart {
            heading: name.to_owned(),
            anchor: slug.to_owned(),
            is_empty: true,
            body_html: String::new(),
            fold: None,
        };
        (
            dummy(part_names[0], "purpose"),
            dummy(part_names[1], "the-rule"),
            dummy(part_names[2], "how-it-works"),
            dummy(part_names[3], "worked-example"),
            dummy(part_names[4], "never-confuse"),
            dummy(part_names[5], "company-calibration"),
            dummy(part_names[6], "sources"),
            dummy(part_names[7], "change-note"),
        )
    } else {
        let p0 = build_part_by_name(part_names[0], "purpose", true, page, &mut inline);
        let p1 = if page.meta.r#type == PageType::Reference {
            crate::types::render_reference_the_rule(page, view, &mut inline)
        } else {
            build_part_by_name(part_names[1], "the-rule", true, page, &mut inline)
        };
        let p2 = if page.meta.r#type == PageType::Case {
            case_split = crate::types::build_case_split(page, &mut inline);
            if case_split.has_sections {
                build_part_by_name(part_names[2], "how-it-works", false, page, &mut inline)
            } else {
                build_part_by_name(part_names[2], "how-it-works", true, page, &mut inline)
            }
        } else {
            build_part_by_name(part_names[2], "how-it-works", true, page, &mut inline)
        };
        let p3 = build_part_by_name(part_names[3], "worked-example", true, page, &mut inline);
        let p4 = build_part_by_name(part_names[4], "never-confuse", true, page, &mut inline);
        let p5 = build_part_by_name(
            part_names[5],
            "company-calibration",
            true,
            page,
            &mut inline,
        );
        let p6 = build_part_by_name(part_names[6], "sources", false, page, &mut inline);
        let p7 = build_part_by_name(part_names[7], "change-note", false, page, &mut inline);
        (p0, p1, p2, p3, p4, p5, p6, p7)
    };

    if !is_brief && matches!(store, StoreContext::Public) {
        let mut questions = questions_in_part(page, view);
        if page.meta.r#type == PageType::ClauseCard {
            // A clause card also lists its front-matter questions as chips in this part.
            questions.extend(page.meta.questions.iter().map(|q| q.as_str().to_owned()));
        }
        company_calibration.fold = Some(PartFold {
            question_count: questions.len(),
        });
    }

    let distinct_claims = collect_distinct_cited_claims(page, view, &inline);
    let markers = inline.into_markers();
    let availability = OriginalAvailability::from_view(view);
    let sources_rows = build_sources_rows(&from_path, &markers, view, planned, &availability);
    let conflict_ids = collect_distinct_conflicts(page, view);
    let conflict_boxes = build_conflict_boxes(&from_path, &conflict_ids, view, planned);
    let change_entries = parse_change_note(&page.body, page);
    let evidence_cards =
        build_evidence_cards(&from_path, &distinct_claims, view, planned, &availability);

    let cited_claim_rows: Vec<&ClaimRow> = distinct_claims
        .iter()
        .filter_map(|cid| find_claim(view, cid))
        .collect();
    let review_by_date = page_review_by(
        &page.meta,
        newest_change_note_date(&page.body),
        cited_claim_rows.iter().copied(),
    );

    let mut supported_claims = 0usize;
    let mut latest_verified: Option<CivilDate> = None;
    for crow in &cited_claim_rows {
        if crow.verification == Verification::PrimarySourceSupported {
            supported_claims += 1;
        }
        if let Some(vdate) = crow.verified_on {
            latest_verified = Some(match latest_verified {
                Some(prev) if prev >= vdate => prev,
                _ => vdate,
            });
        }
    }

    let due_claims_html = if overdue_banner {
        render_due_claims(
            &from_path,
            &distinct_claims,
            view,
            planned,
            chip::VERIFY_LANDED,
        )
    } else {
        String::new()
    };
    let meta_row = MetaRowView {
        page_id: page.id.to_string(),
        as_of: page.meta.as_of.to_string(),
        review_by: review_by_date.to_string(),
        supported_claims,
        total_claims: distinct_claims.len(),
        last_verified: latest_verified.map(|d| d.to_string()),
        readiness: page.meta.readiness.as_str().to_owned(),
        due_claims_html,
        overdue_banner,
    };

    let superseded_by_html = page
        .meta
        .superseded_by
        .iter()
        .map(|pid| {
            let target_page = find_page(view, pid.as_str());
            let pv_title = target_page.map(|p| p.meta.title.as_str());
            let pv_body = target_page.map(first_sentence_of_purpose);
            render_page_link(
                &from_path,
                pid.as_str(),
                None,
                &escape_html(pid.as_str()),
                pv_title,
                pv_body.as_deref(),
                planned,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let toc_items = if is_brief {
        let mut items: Vec<TocItem> = brief_parts
            .iter()
            .map(|p| TocItem {
                heading: p.heading.clone(),
                anchor: p.anchor.clone(),
            })
            .collect();
        items.push(TocItem {
            heading: sources_part.heading.clone(),
            anchor: sources_part.anchor.clone(),
        });
        items
    } else {
        vec![
            TocItem {
                heading: purpose.heading.clone(),
                anchor: purpose.anchor.clone(),
            },
            TocItem {
                heading: the_rule.heading.clone(),
                anchor: the_rule.anchor.clone(),
            },
            TocItem {
                heading: how_it_works.heading.clone(),
                anchor: how_it_works.anchor.clone(),
            },
            TocItem {
                heading: worked_example.heading.clone(),
                anchor: worked_example.anchor.clone(),
            },
            TocItem {
                heading: never_confuse.heading.clone(),
                anchor: never_confuse.anchor.clone(),
            },
            TocItem {
                heading: company_calibration.heading.clone(),
                anchor: company_calibration.anchor.clone(),
            },
            TocItem {
                heading: sources_part.heading.clone(),
                anchor: sources_part.anchor.clone(),
            },
            TocItem {
                heading: change_note_part.heading.clone(),
                anchor: change_note_part.anchor.clone(),
            },
        ]
    };

    let budget = view
        .corpus()
        .and_then(|c| usize::try_from(c.assistant.pack_budget_chars).ok())
        .unwrap_or(DEFAULT_PACK_BUDGET_CHARS);
    let context_pack = packs.page_pack(&Dest::Edition, &page.id, budget).to_text();
    let rail_lists_html = crate::related::render_rail_lists(page, view, planned, links);

    let common = crate::types::TypedPageCommon {
        area_chip_html,
        subarea: page.meta.subarea.clone(),
        page_type: page.meta.r#type.as_str().to_owned(),
        stage: page.meta.stage,
        status_chip_html,
        handling_chips: page.meta.handling.clone(),
        title: page.meta.title.clone(),
        meta_row,
        is_synthetic_now: page.meta.readiness == Readiness::SyntheticNow,
        superseded_by_html,
        purpose,
        the_rule,
        how_it_works,
        worked_example,
        never_confuse,
        company_calibration,
        sources_part,
        change_note_part,
        empty_part_text: strings::empty_part(),
        sources_rows,
        conflict_boxes,
        change_entries,
        toc_items,
        evidence_cards,
        rail_lists_html,
        context_pack,
    };

    if is_brief {
        return crate::types::render_brief(common, brief_parts);
    }
    let common = match crate::types::render_typed_eight_part(
        page,
        view,
        planned,
        Box::new(common),
        case_split,
    ) {
        Ok(typed_html) => return typed_html,
        Err(c) => *c,
    };

    let tpl = PageTemplate {
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
        is_brief,
        brief_parts,
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
        rail_lists_html: common.rail_lists_html,
        context_pack: common.context_pack,
    };

    tpl.render()
        .unwrap_or_else(|e| unreachable!("page.html is infallible: {e}"))
}

/// Render a complete `p/<page-id>.html` document wrapped in the global site frame.
#[must_use]
pub fn render_page(
    page: &PageEntry,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let packs = PackSource::new(view);
    let links = crate::related::LinkIndex::new(view);
    render_page_with_packs(page, view, ctx, nav, planned, &packs, &links)
}

/// Render a complete `p/<page-id>.html` document wrapped in the global site frame, reusing pre-built context packs.
#[must_use]
pub fn render_page_with_packs(
    page: &PageEntry,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
    packs: &PackSource<'_>,
    links: &crate::related::LinkIndex,
) -> String {
    let page_path = format!("p/{}.html", page.id);
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let edition = ctx.edition_info();
    let overdue_banner = crate::edition::built_date(edition).is_some();
    let main_html =
        render_main_with_packs(page, view, &store, planned, packs, links, overdue_banner);
    let area_code = page.meta.area.as_str();
    let area_label = view
        .corpus()
        .and_then(|ct| {
            ct.area
                .iter()
                .find(|a| a.code.eq_ignore_ascii_case(area_code))
        })
        .map_or_else(
            || area_code.to_ascii_uppercase(),
            |a| format!("{} · {}", a.code.to_ascii_uppercase(), a.name),
        );
    let areas_crumb = match SectionId::Areas.first_path() {
        Some(path) => Crumb::linked("Areas", path, planned),
        None => Crumb::current("Areas"),
    };
    let crumbs = [
        areas_crumb,
        Crumb::linked(
            area_label,
            format!("a/{}.html", area_code.to_ascii_lowercase()),
            planned,
        ),
        Crumb::current(page.meta.title.clone()),
    ];
    render_frame_page(&FrameInput {
        output_path: &page_path,
        title: &page.meta.title,
        page_area: Some(area_code),
        edition,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

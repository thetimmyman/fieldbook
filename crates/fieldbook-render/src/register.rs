//! Five register page renderers (`claims.html`, `sources.html`, `questions.html`,
//! `conflicts.html`, `glossary.html`) and the claims part pages (`claims/<AREA>-<part>.html`)
//! sharing `templates/register.html` (`build-registers`). `claims.html` is the index of the
//! claims part pages; the part of a claim comes from [`fieldbook_core::ids::claim_part`].
//!
//! **Responsibility.** Build [`RegisterView`] from [`LabelledView`] for any [`Register`] and
//! render all register pages through the single `templates/register.html` template.

use std::collections::{BTreeMap, BTreeSet};

use askama::Template;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::{AreaCode, ClaimId, TermId, VersionId, claim_page_route, claim_part};
use fieldbook_core::schema::{
    ClaimRow, ClaimUnit, ConflictRow, ConflictStatus, GlossaryRow, QuestionRow, RefId, Register,
    SourceRow, Verification,
};
use fieldbook_core::strings;

use crate::csv_copy;
use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::inline::{find_conflict, find_question, find_source};
use crate::links::{
    escape_html, href_if_planned, render_area_chip, render_claim_chip, render_conflict_chip,
    render_question_chip, render_source_id_chip, render_term_chip, render_version_chip,
};
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::site::PlannedPaths;
use crate::status_chip::{
    ICON_ALERT, ICON_CHECK, render_adoption_chip, render_applicability_chip, render_handling_chips,
    render_verification_chip,
};
use crate::xlsx;

/// Number of non-overlay columns rendered on `questions.html` (excludes `answer`, `answered_on`, `adoption`).
pub const QUESTIONS_CORPUS_COLS: usize = 10;

/// One rendered table cell in a register row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterCellView {
    /// Canonical column name (`claim_id`, `verification`, etc.).
    pub col_name: &'static str,
    /// Sort key for client-side sorting in `app.js`; empty when the cell's own text sorts it,
    /// which keeps the delivered page small.
    pub sort_key: String,
    /// Pre-escaped / pre-rendered cell inner HTML.
    pub html: String,
}

/// One rendered `<tr id="...">` row in a register table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterRowView {
    /// Row anchor `id` (`CL-...`, `S-nnnn-v<k>`, `Q-...`, `XC-...`, `<term_id>`).
    pub id: String,
    /// Canonical `verification` value for claim part page client-side filtering, or `None`.
    pub verification_filter: Option<String>,
    /// Ordered cells matching [`RegisterView::columns`].
    pub cells: Vec<RegisterCellView>,
}

/// One verification filter option button on claim part pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterOptionView {
    /// Filter key (`"all"` or canonical `Verification::as_str()`).
    pub key: &'static str,
    /// Button label (`"All"`, `"Unchecked"`, `"Supported"`, ...).
    pub label: &'static str,
    /// `true` for the default active button (`"all"`).
    pub active: bool,
}

/// Register-variant `ConflictBox` rendered on `conflicts.html` (`build-registers` step 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterConflictBoxView {
    /// Conflict identifier (`XC-nn`).
    pub conflict_id: String,
    /// `true` when `status == ConflictStatus::Resolved`.
    pub is_resolved: bool,
    /// Conflict ID chip HTML.
    pub conflict_chip_html: String,
    /// Area chip HTML.
    pub area_chip_html: String,
    /// Status chip HTML (`Resolved` or `Unresolved · every position kept`).
    pub status_chip_html: String,
    /// Conflict topic.
    pub topic: String,
    /// Every competing position in order.
    pub positions: Vec<String>,
    /// Disputed claim chips HTML.
    pub claims_html: String,
    /// Settling question chip HTML when present.
    pub question_chip_html: Option<String>,
    /// Resolution text when `is_resolved` is `true` (always `None` when unresolved).
    pub resolution: Option<String>,
    /// Rationale text when `is_resolved` is `true` (always `None` when unresolved).
    pub rationale: Option<String>,
    /// Rendered evidence HTML when `is_resolved` is `true` (always `None` when unresolved).
    pub evidence_html: Option<String>,
    /// Resolution date (`YYYY-MM-DD`) when `is_resolved` is `true` (always `None` when unresolved).
    pub resolved_on: Option<String>,
}

/// One claims register part page metadata (`claims/<AREA>-<part>.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimsPart {
    /// Upper-case area code (e.g. `"CMP"`).
    pub area_code: String,
    /// Human-readable area name from `area_title`, or `area_code` fallback.
    pub area_name: String,
    /// 1-based part number derived from claim ID (`(number - 1) / CLAIMS_PER_PART + 1`).
    pub part: u32,
    /// Site-relative route, e.g. `"claims/CMP-1.html"`.
    pub route: String,
    /// Lowest claim identifier on this part page (e.g. `"CL-CMP-0001"`).
    pub first_id: String,
    /// Highest claim identifier on this part page (e.g. `"CL-CMP-0500"`).
    pub last_id: String,
    /// Number of claims present on this part page.
    pub count: usize,
    /// 1-based ordinal among existing parts of this area.
    pub ordinal: usize,
    /// Total number of existing parts of this area.
    pub of: usize,
}

/// Navigation links rendered on a claims part page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimsPartNavView {
    /// Relative href to `claims.html` index page, or `None` if not planned.
    pub all_claims_href: Option<String>,
    /// Previous part page `(href, label)` within the same area, if any.
    pub prev: Option<(String, String)>,
    /// Next part page `(href, label)` within the same area, if any.
    pub next: Option<(String, String)>,
}

/// One part link entry in the claims index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimsIndexPartView {
    /// Relative href to the part page if planned, or `None` for plain text.
    pub href: Option<String>,
    /// ID range label, e.g. `"CL-CMP-0001 to CL-CMP-0500"`.
    pub label: String,
    /// Claim count label, e.g. `"500 claims"` or `"1 claim"`.
    pub count_label: String,
}

/// One area section in the claims index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimsIndexAreaView {
    /// Human-readable area name.
    pub name: String,
    /// Upper-case area code.
    pub code: String,
    /// Area claim count label, e.g. `"500 claims"` or `"1 claim"`.
    pub count_label: String,
    /// Existing part pages in this area.
    pub parts: Vec<ClaimsIndexPartView>,
}

/// View model for the claims index on `claims.html`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimsIndexView {
    /// Areas that have at least one claim, sorted by upper-case area code.
    pub areas: Vec<ClaimsIndexAreaView>,
}

/// Complete view model for one of the register pages or a claims part page (`templates/register.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterView {
    /// Register kind.
    pub register: Register,
    /// Lowercase register slug (`"claims"`, `"sources"`, `"questions"`, `"conflicts"`, `"glossary"`).
    pub slug: &'static str,
    /// Site-relative output path (`"claims.html"`, `"claims/CMP-1.html"`, `"sources.html"`, ...).
    pub output_path: String,
    /// CSV filename in `corpus/registers/` (`"claims.csv"`, ...).
    pub file_name: &'static str,
    /// Page `<h1>` and `<title>`.
    pub title: String,
    /// Subtitle under `<h1>`.
    pub subtitle: String,
    /// Table `<caption>`.
    pub caption: &'static str,
    /// Ordered column headers (`<th scope="col">`).
    pub columns: Vec<&'static str>,
    /// `true` on claim part pages to render the verification filter bar.
    pub show_claims_filter: bool,
    /// Filter buttons for claim part pages.
    pub filter_options: Vec<FilterOptionView>,
    /// Register-variant `ConflictBox` cards for `conflicts.html`.
    pub conflict_boxes: Vec<RegisterConflictBoxView>,
    /// Row count shown in the page header (`"1 row"`, `"12 rows"`); unused when `rows` is empty.
    pub row_count_label: String,
    /// Sorted table rows.
    pub rows: Vec<RegisterRowView>,
    /// Small navigation line for claims part pages (`All claims`, `Previous`, `Next`).
    pub part_nav: Option<ClaimsPartNavView>,
    /// Structured index sections for `claims.html`.
    pub claims_index: Option<ClaimsIndexView>,
    /// Display filename for the CSV link in "Copy for Excel:" (`"sources.csv"`, `"claims-CMP-1.csv"`, ...).
    pub csv_file_name: String,
    /// Link to this page's CSV copy (`data/<register>.csv` or `data/claims-<AREA>-<part>.csv`), when planned.
    pub csv_href: Option<String>,
    /// Link to the workbook copy of the registers (`data/registers.xlsx`), when that path is planned.
    pub workbook_href: Option<String>,
}

#[derive(Template)]
#[template(path = "register.html")]
struct RegisterTemplate<'a> {
    view: &'a RegisterView,
}

/// Site-relative HTML output path for `reg`.
#[must_use]
pub const fn output_path(reg: Register) -> &'static str {
    match reg {
        Register::Sources => "sources.html",
        Register::Claims => "claims.html",
        Register::Questions => "questions.html",
        Register::Conflicts => "conflicts.html",
        Register::Glossary => "glossary.html",
    }
}

/// Lowercase register slug used in `data-register`.
#[must_use]
pub const fn register_slug(reg: Register) -> &'static str {
    match reg {
        Register::Sources => "sources",
        Register::Claims => "claims",
        Register::Questions => "questions",
        Register::Conflicts => "conflicts",
        Register::Glossary => "glossary",
    }
}

/// Display title for `reg`.
#[must_use]
pub fn register_title(reg: Register) -> &'static str {
    match reg {
        Register::Sources => strings::parts()[6],
        Register::Claims => "Claims",
        Register::Questions => "Questions",
        Register::Conflicts => "Conflicts",
        Register::Glossary => "Glossary",
    }
}

const fn register_subtitle(reg: Register) -> &'static str {
    match reg {
        Register::Sources => {
            "Registered sources and byte-exact originals with authority tier, version lineage, and lifecycle state."
        }
        Register::Claims => {
            "Every load-bearing statement with separate verification, applicability, and adoption statuses."
        }
        Register::Questions => {
            "Open and answered company questions, who owns them, and which pages or claims wait on them."
        }
        Register::Conflicts => {
            "Where sources or reports disagree. Unresolved conflicts keep every position and name the settling question."
        }
        Register::Glossary => {
            "Canonical domain terms, definitions, external source references, aliases, and supersession."
        }
    }
}

const fn register_caption(reg: Register) -> &'static str {
    match reg {
        Register::Sources => "Sources register",
        Register::Claims => "Claims register",
        Register::Questions => "Questions register",
        Register::Conflicts => "Conflicts register",
        Register::Glossary => "Glossary register",
    }
}

fn area_title<'a>(view: &'a LabelledView, area: &AreaCode) -> Option<&'a str> {
    view.corpus()
        .and_then(|ct| {
            ct.area
                .iter()
                .find(|a| a.code.eq_ignore_ascii_case(area.as_str()))
        })
        .map(|a| a.name.as_str())
}

fn find_source_by_vid<'a>(view: &'a LabelledView, vid: &VersionId) -> Option<&'a SourceRow> {
    find_source(view, &vid.source(), vid.version())
}

fn empty_cell(col_name: &'static str, empty_means: &str) -> RegisterCellView {
    RegisterCellView {
        col_name,
        sort_key: String::new(),
        html: format!("<span class=\"muted\">{}</span>", escape_html(empty_means)),
    }
}

/// A cell whose "Empty means" column in the format reference is blank: nothing to say, so the
/// cell stays empty.
fn blank_cell(col_name: &'static str) -> RegisterCellView {
    RegisterCellView {
        col_name,
        sort_key: String::new(),
        html: String::new(),
    }
}

/// Records the permitted view keeps. A cross-reference to a record the view left out is never
/// printed or linked, so a kept row cannot name an omitted one.
struct Kept {
    claims: BTreeSet<ClaimId>,
    terms: BTreeSet<TermId>,
    versions: BTreeSet<VersionId>,
}

impl Kept {
    fn of(view: &LabelledView) -> Self {
        Self {
            claims: view
                .claims()
                .iter()
                .map(|l| l.value.claim_id.clone())
                .collect(),
            terms: view
                .glossary()
                .iter()
                .map(|l| l.value.term_id.clone())
                .collect(),
            versions: view
                .sources()
                .iter()
                .filter_map(|l| {
                    VersionId::parse(&format!("{}-v{}", l.value.source_id, l.value.version)).ok()
                })
                .collect(),
        }
    }
}

fn text_cell(col_name: &'static str, text: &str) -> RegisterCellView {
    RegisterCellView {
        col_name,
        sort_key: String::new(),
        html: escape_html(text),
    }
}

fn mono_cell(col_name: &'static str, text: &str) -> RegisterCellView {
    RegisterCellView {
        col_name,
        sort_key: String::new(),
        html: format!("<span class=\"mono\">{}</span>", escape_html(text)),
    }
}

fn opt_text_cell(col_name: &'static str, val: Option<&str>, empty_means: &str) -> RegisterCellView {
    match val.filter(|s| !s.is_empty()) {
        Some(v) => text_cell(col_name, v),
        None => empty_cell(col_name, empty_means),
    }
}

fn opt_mono_cell(
    col_name: &'static str,
    val: Option<String>,
    empty_means: &str,
) -> RegisterCellView {
    match val.filter(|s| !s.is_empty()) {
        Some(v) => mono_cell(col_name, &v),
        None => empty_cell(col_name, empty_means),
    }
}

fn render_ref_id(
    from_path: &str,
    ref_id: &RefId,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> String {
    match ref_id {
        RefId::Claim(cid) => render_claim_chip(from_path, cid, planned),
        RefId::SourceVersion(vid) => {
            render_version_chip(from_path, vid, find_source_by_vid(view, vid), planned)
        }
    }
}

fn render_ref_id_list(
    from_path: &str,
    refs: &[RefId],
    view: &LabelledView,
    planned: &PlannedPaths,
) -> String {
    refs.iter()
        .map(|r| render_ref_id(from_path, r, view, planned))
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_source_row_view(
    from_path: &str,
    kept: &Kept,
    row: &SourceRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterRowView {
    let cols = Register::Sources.columns();
    let vid_str = format!("{}-v{}", row.source_id, row.version);
    let vid = VersionId::parse(&vid_str)
        .unwrap_or_else(|e| unreachable!("valid SourceRow produces valid VersionId: {e}"));

    let shown_supersedes: Vec<&VersionId> = row
        .supersedes
        .iter()
        .filter(|v| kept.versions.contains(*v))
        .collect();
    let supersedes_cell = if shown_supersedes.is_empty() {
        empty_cell(cols[18].name, "Replaces nothing")
    } else {
        let sort_key = shown_supersedes
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("|");
        let html = shown_supersedes
            .iter()
            .map(|sup_vid| {
                render_version_chip(
                    from_path,
                    sup_vid,
                    find_source_by_vid(view, sup_vid),
                    planned,
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        RegisterCellView {
            col_name: cols[18].name,
            sort_key,
            html,
        }
    };

    let cells = vec![
        // 0: source_id
        RegisterCellView {
            col_name: cols[0].name,
            sort_key: row.source_id.as_str().to_owned(),
            html: render_source_id_chip(from_path, &row.source_id, view, planned),
        },
        // 1: version
        RegisterCellView {
            col_name: cols[1].name,
            sort_key: format!("{:010}", row.version),
            html: render_version_chip(from_path, &vid, Some(row), planned),
        },
        // 2: tag
        opt_text_cell(cols[2].name, row.tag.as_deref(), "No label"),
        // 3: title
        text_cell(cols[3].name, &row.title),
        // 4: publisher
        text_cell(cols[4].name, &row.publisher),
        // 5: source_kind (unmapped enum -> canonical text)
        text_cell(cols[5].name, row.source_kind.as_str()),
        // 6: source_role (unmapped enum -> canonical text)
        text_cell(cols[6].name, row.source_role.as_str()),
        // 7: authority_tier (unmapped enum -> canonical text)
        opt_text_cell(
            cols[7].name,
            row.authority_tier.map(|v| v.as_str()),
            "A synthetic fixture or a company kind has no tier",
        ),
        // 8: licence
        text_cell(cols[8].name, &row.licence),
        // 9: source_family
        opt_text_cell(
            cols[9].name,
            row.source_family.as_deref(),
            "Unknown. A blank family never means independent",
        ),
        // 10: family_basis (unmapped enum -> canonical text)
        opt_text_cell(
            cols[10].name,
            row.family_basis.map(|v| v.as_str()),
            "No family",
        ),
        // 11: location
        mono_cell(cols[11].name, &row.location),
        // 12: sha256
        mono_cell(cols[12].name, &row.sha256),
        // 13: published
        opt_mono_cell(
            cols[13].name,
            row.published.as_ref().map(ToString::to_string),
            "The document prints no date",
        ),
        // 14: effective_from
        opt_mono_cell(
            cols[14].name,
            row.effective_from.as_ref().map(ToString::to_string),
            "Not stated, or not applicable",
        ),
        // 15: as_of
        mono_cell(cols[15].name, &row.as_of.to_string()),
        // 16: retrieved
        mono_cell(cols[16].name, &row.retrieved.to_string()),
        // 17: lifecycle (unmapped enum -> canonical text)
        text_cell(cols[17].name, row.lifecycle.as_str()),
        // 18: supersedes
        supersedes_cell,
        // 19: volatility (unmapped enum -> canonical text)
        text_cell(cols[19].name, row.volatility.as_str()),
        // 20: handling (mapped in HANDOFF §7)
        RegisterCellView {
            col_name: cols[20].name,
            sort_key: row.handling.join("|"),
            html: render_handling_chips(&row.handling),
        },
        // 21: note
        match row.note.as_deref().filter(|s| !s.is_empty()) {
            Some(v) => text_cell(cols[21].name, v),
            None => blank_cell(cols[21].name),
        },
        // 22: purpose
        opt_text_cell(cols[22].name, row.purpose.as_deref(), "Unassigned"),
        // 23: origin_system
        opt_text_cell(
            cols[23].name,
            row.origin_system.as_deref(),
            "Added by hand, or not from a company system",
        ),
        // 24: origin_stream
        opt_text_cell(
            cols[24].name,
            row.origin_stream.as_deref(),
            "Added by hand, or not from a company system",
        ),
        // 25: origin_key
        opt_text_cell(
            cols[25].name,
            row.origin_key.as_deref(),
            "Added by hand, or not from a company system",
        ),
        // 26: doc_kind
        opt_text_cell(cols[26].name, row.doc_kind.as_deref(), "Not yet classified"),
        // 27: series
        opt_text_cell(cols[27].name, row.series.as_deref(), "Not part of a series"),
    ];

    RegisterRowView {
        id: vid_str,
        verification_filter: None,
        cells,
    }
}

fn build_claim_row_view(
    from_path: &str,
    kept: &Kept,
    row: &ClaimRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterRowView {
    let cols = Register::Claims.columns();

    let version_cell = match (row.source_id.as_ref(), row.version) {
        (Some(sid), Some(ver_num)) => {
            let vid_str = format!("{sid}-v{ver_num}");
            let html = match VersionId::parse(&vid_str) {
                Ok(vid) => {
                    render_version_chip(from_path, &vid, find_source(view, sid, ver_num), planned)
                }
                Err(_) => escape_html(&ver_num.to_string()),
            };
            RegisterCellView {
                col_name: cols[6].name,
                sort_key: format!("{ver_num:010}"),
                html,
            }
        }
        (None, Some(ver_num)) => mono_cell(cols[6].name, &ver_num.to_string()),
        (_, None) => empty_cell(cols[6].name, "An assumption has no source"),
    };

    let verified_on_cell = match row.verified_on {
        Some(d) => mono_cell(cols[28].name, &d.to_string()),
        None => RegisterCellView {
            col_name: cols[28].name,
            sort_key: String::new(),
            html: "<span class=\"unver\">Never verified</span>".to_owned(),
        },
    };

    let claim_list_cell = |col: usize, ids: &[ClaimId], empty_means: &str| {
        let shown: Vec<&ClaimId> = ids.iter().filter(|c| kept.claims.contains(*c)).collect();
        if shown.is_empty() {
            return empty_cell(cols[col].name, empty_means);
        }
        RegisterCellView {
            col_name: cols[col].name,
            sort_key: shown
                .iter()
                .map(|c| c.as_str())
                .collect::<Vec<_>>()
                .join("|"),
            html: shown
                .iter()
                .map(|cid| render_claim_chip(from_path, cid, planned))
                .collect::<Vec<_>>()
                .join(" "),
        }
    };
    let supersedes_cell = claim_list_cell(30, &row.supersedes, "Replaces nothing");
    let superseded_by_cell = claim_list_cell(31, &row.superseded_by, "Not replaced");

    let cells = vec![
        // 0: claim_id
        RegisterCellView {
            col_name: cols[0].name,
            sort_key: row.claim_id.as_str().to_owned(),
            html: render_claim_chip(from_path, &row.claim_id, planned),
        },
        // 1: area
        RegisterCellView {
            col_name: cols[1].name,
            sort_key: row.area.as_str().to_owned(),
            html: render_area_chip(from_path, &row.area, area_title(view, &row.area), planned),
        },
        // 2: kind (unmapped enum -> canonical text)
        text_cell(cols[2].name, row.kind.as_str()),
        // 3: statement
        text_cell(cols[3].name, &row.statement),
        // 4: as_of
        opt_mono_cell(
            cols[4].name,
            row.as_of.as_ref().map(ToString::to_string),
            "Unknown only when stale with the date unknown",
        ),
        // 5: source_id
        match &row.source_id {
            Some(sid) => RegisterCellView {
                col_name: cols[5].name,
                sort_key: sid.as_str().to_owned(),
                html: render_source_id_chip(from_path, sid, view, planned),
            },
            None => empty_cell(cols[5].name, "An assumption has no source"),
        },
        // 6: version
        version_cell,
        // 7: locator_kind (unmapped enum -> canonical text)
        opt_text_cell(
            cols[7].name,
            row.locator_kind
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            "An assumption has no source",
        ),
        // 8: locator
        opt_text_cell(cols[8].name, row.locator.as_deref(), "No fine locator"),
        // 9: quote
        opt_text_cell(cols[9].name, row.quote.as_deref(), "No quote"),
        // 10: consequential
        text_cell(
            cols[10].name,
            if row.consequential { "true" } else { "false" },
        ),
        // 11: drafted_by (unmapped enum -> canonical text)
        text_cell(cols[11].name, row.drafted_by.as_str()),
        // 12: verification (mapped in HANDOFF §7)
        RegisterCellView {
            col_name: cols[12].name,
            sort_key: row.verification.as_str().to_owned(),
            html: render_verification_chip(row.verification),
        },
        // 13: applicability (mapped in HANDOFF §7)
        RegisterCellView {
            col_name: cols[13].name,
            sort_key: row.applicability.as_str().to_owned(),
            html: render_applicability_chip(row.applicability),
        },
        // 14: adoption (mapped in HANDOFF §7)
        RegisterCellView {
            col_name: cols[14].name,
            sort_key: row.adoption.as_str().to_owned(),
            html: render_adoption_chip(row.adoption),
        },
        // 15: data_origin (unmapped enum -> canonical text)
        text_cell(cols[15].name, row.data_origin.as_str()),
        // 16: volatility (unmapped enum -> canonical text)
        text_cell(cols[16].name, row.volatility.as_str()),
        // 17: value
        opt_mono_cell(
            cols[17].name,
            row.value.clone(),
            "The claim states no number",
        ),
        // 18: unit (unmapped enum -> canonical text)
        match row.unit.as_ref().map(ClaimUnit::as_str) {
            Some(unit) => text_cell(cols[18].name, unit),
            None => blank_cell(cols[18].name),
        },
        // 19: effective_from
        opt_mono_cell(
            cols[19].name,
            row.effective_from.as_ref().map(ToString::to_string),
            "Not stated",
        ),
        // 20: effective_to
        opt_mono_cell(
            cols[20].name,
            row.effective_to.as_ref().map(ToString::to_string),
            "Still in force at as_of",
        ),
        // 21: instrument (unmapped enum -> canonical text)
        opt_text_cell(
            cols[21].name,
            row.instrument.map(|v| v.as_str()),
            "Not a regulatory claim",
        ),
        // 22: citation_codified
        match row.citation_codified.as_deref().filter(|s| !s.is_empty()) {
            Some(v) => text_cell(cols[22].name, v),
            None => blank_cell(cols[22].name),
        },
        // 23: citation_rfo
        match row.citation_rfo.as_deref().filter(|s| !s.is_empty()) {
            Some(v) => text_cell(cols[23].name, v),
            None => blank_cell(cols[23].name),
        },
        // 24: edition_date
        opt_mono_cell(
            cols[24].name,
            row.edition_date.as_ref().map(ToString::to_string),
            "None printed",
        ),
        // 25: text_status (unmapped enum -> canonical text)
        opt_text_cell(
            cols[25].name,
            row.text_status.map(|v| v.as_str()),
            "Not regulatory",
        ),
        // 26: applies_to
        opt_text_cell(cols[26].name, row.applies_to.as_deref(), "No such test"),
        // 27: evidence_grade (unmapped enum -> canonical text)
        opt_text_cell(
            cols[27].name,
            row.evidence_grade.map(|v| v.as_str()),
            "The claim is not an input to a calculation",
        ),
        // 28: verified_on
        verified_on_cell,
        // 29: verified_by_role (unmapped enum -> canonical text)
        opt_text_cell(
            cols[29].name,
            row.verified_by_role.map(|v| v.as_str()),
            "Not checked",
        ),
        // 30: supersedes
        supersedes_cell,
        // 31: superseded_by
        superseded_by_cell,
        // 32: conflict_id
        match &row.conflict_id {
            Some(xcid) => RegisterCellView {
                col_name: cols[32].name,
                sort_key: xcid.as_str().to_owned(),
                html: render_conflict_chip(
                    from_path,
                    xcid,
                    find_conflict(view, xcid).map(|c| c.topic.as_str()),
                    planned,
                ),
            },
            None => empty_cell(cols[32].name, "In no conflict"),
        },
        // 33: question_id
        match &row.question_id {
            Some(qid) => RegisterCellView {
                col_name: cols[33].name,
                sort_key: qid.as_str().to_owned(),
                html: render_question_chip(
                    from_path,
                    qid,
                    find_question(view, qid).map(|q| q.question.as_str()),
                    planned,
                ),
            },
            None => empty_cell(cols[33].name, "No question"),
        },
        // 34: note
        match row.note.as_deref().filter(|s| !s.is_empty()) {
            Some(v) => text_cell(cols[34].name, v),
            None => blank_cell(cols[34].name),
        },
    ];

    RegisterRowView {
        id: row.claim_id.as_str().to_owned(),
        verification_filter: Some(row.verification.as_str().to_owned()),
        cells,
    }
}

fn build_question_row_view(
    from_path: &str,
    row: &QuestionRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterRowView {
    let cols = &Register::Questions.columns()[..QUESTIONS_CORPUS_COLS];

    let cells = vec![
        // 0: question_id
        RegisterCellView {
            col_name: cols[0].name,
            sort_key: row.question_id.as_str().to_owned(),
            html: render_question_chip(from_path, &row.question_id, Some(&row.question), planned),
        },
        // 1: area
        RegisterCellView {
            col_name: cols[1].name,
            sort_key: row.area.as_str().to_owned(),
            html: render_area_chip(from_path, &row.area, area_title(view, &row.area), planned),
        },
        // 2: question
        text_cell(cols[2].name, &row.question),
        // 3: owner_role
        opt_text_cell(cols[3].name, row.owner_role.as_deref(), "Not yet named"),
        // 4: blocks
        opt_text_cell(cols[4].name, row.blocks.as_deref(), "Not yet written"),
        // 5: why_it_matters
        opt_text_cell(
            cols[5].name,
            row.why_it_matters.as_deref(),
            "Not yet written",
        ),
        // 6: default_assumption
        opt_text_cell(
            cols[6].name,
            row.default_assumption.as_deref(),
            "Not yet written",
        ),
        // 7: work_can_continue (unmapped enum -> canonical text)
        opt_text_cell(
            cols[7].name,
            row.work_can_continue.map(|v| v.as_str()),
            "Not yet decided",
        ),
        // 8: day_one
        text_cell(cols[8].name, if row.day_one { "true" } else { "false" }),
        // 9: status (unmapped enum -> canonical text)
        text_cell(cols[9].name, row.status.as_str()),
    ];

    RegisterRowView {
        id: row.question_id.as_str().to_owned(),
        verification_filter: None,
        cells,
    }
}

/// Build the register-variant [`RegisterConflictBoxView`] for `row` (`build-registers` step 3).
///
/// Unresolved conflicts keep every position, name the settling question, and never expose a
/// resolution, rationale, evidence, or resolved date even if those fields are populated on `row`.
#[must_use]
pub fn build_register_conflict_box(
    from_path: &str,
    row: &ConflictRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterConflictBoxView {
    let is_resolved = row.status == ConflictStatus::Resolved;
    let status_chip_html = if is_resolved {
        format!("<span class=\"chip ok\">{ICON_CHECK}<span>Resolved · never averaged</span></span>")
    } else {
        format!(
            "<span class=\"chip warn\">{ICON_ALERT}<span>Unresolved · every position kept</span></span>"
        )
    };
    let conflict_chip_html =
        render_conflict_chip(from_path, &row.conflict_id, Some(&row.topic), planned);
    let area_chip_html =
        render_area_chip(from_path, &row.area, area_title(view, &row.area), planned);
    let claims_html = row
        .claim_ids
        .iter()
        .map(|cid| render_claim_chip(from_path, cid, planned))
        .collect::<Vec<_>>()
        .join(" ");
    let question_chip_html = row.question_id.as_ref().map(|qid| {
        render_question_chip(
            from_path,
            qid,
            find_question(view, qid).map(|q| q.question.as_str()),
            planned,
        )
    });

    let (resolution, rationale, evidence_html, resolved_on) = if is_resolved {
        let ev_html = (!row.evidence.is_empty())
            .then(|| render_ref_id_list(from_path, &row.evidence, view, planned));
        (
            row.resolution.clone().filter(|s| !s.is_empty()),
            row.rationale.clone().filter(|s| !s.is_empty()),
            ev_html,
            row.resolved_on.map(|d| d.to_string()),
        )
    } else {
        (None, None, None, None)
    };

    RegisterConflictBoxView {
        conflict_id: row.conflict_id.as_str().to_owned(),
        is_resolved,
        conflict_chip_html,
        area_chip_html,
        status_chip_html,
        topic: row.topic.clone(),
        positions: row.positions.clone(),
        claims_html,
        question_chip_html,
        resolution,
        rationale,
        evidence_html,
        resolved_on,
    }
}

fn build_conflict_row_view(
    from_path: &str,
    row: &ConflictRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterRowView {
    let cols = Register::Conflicts.columns();
    let is_resolved = row.status == ConflictStatus::Resolved;

    let positions_html = row
        .positions
        .iter()
        .map(|p| format!("<li>{}</li>", escape_html(p)))
        .collect::<Vec<_>>()
        .join("");

    let claims_html = row
        .claim_ids
        .iter()
        .map(|cid| render_claim_chip(from_path, cid, planned))
        .collect::<Vec<_>>()
        .join(" ");

    let (res_cell, rat_cell, ev_cell, on_cell) = if is_resolved {
        let r = opt_text_cell(cols[4].name, row.resolution.as_deref(), "Unresolved");
        let rat = opt_text_cell(cols[5].name, row.rationale.as_deref(), "Unresolved");
        let ev = if row.evidence.is_empty() {
            empty_cell(cols[6].name, "Unresolved")
        } else {
            let sort_key = row
                .evidence
                .iter()
                .map(RefId::as_str)
                .collect::<Vec<_>>()
                .join("|");
            RegisterCellView {
                col_name: cols[6].name,
                sort_key,
                html: render_ref_id_list(from_path, &row.evidence, view, planned),
            }
        };
        let on = opt_mono_cell(
            cols[7].name,
            row.resolved_on.map(|d| d.to_string()),
            "Unresolved",
        );
        (r, rat, ev, on)
    } else {
        (
            empty_cell(cols[4].name, "Unresolved"),
            empty_cell(cols[5].name, "Unresolved"),
            empty_cell(cols[6].name, "Unresolved"),
            empty_cell(cols[7].name, "Unresolved"),
        )
    };

    let cells = vec![
        // 0: conflict_id
        RegisterCellView {
            col_name: cols[0].name,
            sort_key: row.conflict_id.as_str().to_owned(),
            html: render_conflict_chip(from_path, &row.conflict_id, Some(&row.topic), planned),
        },
        // 1: area
        RegisterCellView {
            col_name: cols[1].name,
            sort_key: row.area.as_str().to_owned(),
            html: render_area_chip(from_path, &row.area, area_title(view, &row.area), planned),
        },
        // 2: topic
        text_cell(cols[2].name, &row.topic),
        // 3: positions
        RegisterCellView {
            col_name: cols[3].name,
            sort_key: row.positions.join("|"),
            html: format!("<ul class=\"pos-list\">{positions_html}</ul>"),
        },
        // 4: resolution
        res_cell,
        // 5: rationale
        rat_cell,
        // 6: evidence
        ev_cell,
        // 7: resolved_on
        on_cell,
        // 8: status (unmapped enum -> canonical text)
        text_cell(cols[8].name, row.status.as_str()),
        // 9: claim_ids
        RegisterCellView {
            col_name: cols[9].name,
            sort_key: row
                .claim_ids
                .iter()
                .map(ClaimId::as_str)
                .collect::<Vec<_>>()
                .join("|"),
            html: claims_html,
        },
        // 10: question_id
        match &row.question_id {
            Some(qid) => RegisterCellView {
                col_name: cols[10].name,
                sort_key: qid.as_str().to_owned(),
                html: render_question_chip(
                    from_path,
                    qid,
                    find_question(view, qid).map(|q| q.question.as_str()),
                    planned,
                ),
            },
            None => empty_cell(cols[10].name, "Resolved"),
        },
    ];

    RegisterRowView {
        id: row.conflict_id.as_str().to_owned(),
        verification_filter: None,
        cells,
    }
}

fn build_glossary_row_view(
    from_path: &str,
    kept: &Kept,
    row: &GlossaryRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterRowView {
    let cols = Register::Glossary.columns();

    let source_ref_cell = if row.source_ref.is_empty() {
        empty_cell(cols[5].name, "No source row")
    } else {
        let sort_key = row
            .source_ref
            .iter()
            .map(RefId::as_str)
            .collect::<Vec<_>>()
            .join("|");
        RegisterCellView {
            col_name: cols[5].name,
            sort_key,
            html: render_ref_id_list(from_path, &row.source_ref, view, planned),
        }
    };

    let aliases_cell = if row.aliases.is_empty() {
        empty_cell(cols[7].name, "None")
    } else {
        text_cell(cols[7].name, &row.aliases.join("; "))
    };

    let cells = vec![
        // 0: term_id
        RegisterCellView {
            col_name: cols[0].name,
            sort_key: row.term_id.as_str().to_owned(),
            html: render_term_chip(from_path, &row.term_id, planned),
        },
        // 1: label
        text_cell(cols[1].name, &row.label),
        // 2: definition
        text_cell(cols[2].name, &row.definition),
        // 3: boundary_rule
        text_cell(cols[3].name, &row.boundary_rule),
        // 4: owner_area
        RegisterCellView {
            col_name: cols[4].name,
            sort_key: row.owner_area.as_str().to_owned(),
            html: render_area_chip(
                from_path,
                &row.owner_area,
                area_title(view, &row.owner_area),
                planned,
            ),
        },
        // 5: source_ref
        source_ref_cell,
        // 6: definition_status (unmapped enum -> canonical text)
        text_cell(cols[6].name, row.definition_status.as_str()),
        // 7: aliases
        aliases_cell,
        // 8: superseded_by
        match row
            .superseded_by
            .as_ref()
            .filter(|t| kept.terms.contains(*t))
        {
            Some(tid) => RegisterCellView {
                col_name: cols[8].name,
                sort_key: tid.as_str().to_owned(),
                html: render_term_chip(from_path, tid, planned),
            },
            None => empty_cell(cols[8].name, "Not replaced"),
        },
    ];

    RegisterRowView {
        id: row.term_id.as_str().to_owned(),
        verification_filter: None,
        cells,
    }
}

fn claims_filter_options() -> Vec<FilterOptionView> {
    vec![
        FilterOptionView {
            key: "all",
            label: "All",
            active: true,
        },
        FilterOptionView {
            key: Verification::Unchecked.as_str(),
            label: "Unchecked",
            active: false,
        },
        FilterOptionView {
            key: Verification::PrimarySourceSupported.as_str(),
            label: "Supported",
            active: false,
        },
        FilterOptionView {
            key: Verification::Contradicted.as_str(),
            label: "Contradicted",
            active: false,
        },
        FilterOptionView {
            key: Verification::Stale.as_str(),
            label: "Stale",
            active: false,
        },
        FilterOptionView {
            key: Verification::Unresolved.as_str(),
            label: "Unresolved",
            active: false,
        },
    ]
}

/// Build the [`RegisterView`] for `reg` from the permitted [`LabelledView`].
#[must_use]
pub fn build_register_view(
    reg: Register,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterView {
    let from_path = output_path(reg);
    let columns: Vec<&'static str> = match reg {
        Register::Questions => Register::Questions.columns()[..QUESTIONS_CORPUS_COLS]
            .iter()
            .map(|c| c.name)
            .collect(),
        other => other.columns().iter().map(|c| c.name).collect(),
    };

    let kept = Kept::of(view);
    let mut conflict_boxes = Vec::new();
    let rows: Vec<RegisterRowView> = match reg {
        Register::Sources => {
            let mut sorted: Vec<&SourceRow> = view.sources().iter().map(|l| &l.value).collect();
            sorted.sort_by(|a, b| {
                a.source_id
                    .cmp(&b.source_id)
                    .then_with(|| a.version.cmp(&b.version))
            });
            sorted
                .into_iter()
                .map(|r| build_source_row_view(from_path, &kept, r, view, planned))
                .collect()
        }
        Register::Claims => {
            // Index page has no claim rows.
            Vec::new()
        }
        Register::Questions => {
            let mut sorted: Vec<&QuestionRow> = view.questions().iter().map(|l| &l.value).collect();
            sorted.sort_by(|a, b| a.question_id.cmp(&b.question_id));
            sorted
                .into_iter()
                .map(|r| build_question_row_view(from_path, r, view, planned))
                .collect()
        }
        Register::Conflicts => {
            let mut sorted: Vec<&ConflictRow> = view.conflicts().iter().map(|l| &l.value).collect();
            sorted.sort_by_key(|c| (c.conflict_id.number(), c.conflict_id.as_str().to_owned()));
            conflict_boxes = sorted
                .iter()
                .map(|r| build_register_conflict_box(from_path, r, view, planned))
                .collect();
            sorted
                .into_iter()
                .map(|r| build_conflict_row_view(from_path, r, view, planned))
                .collect()
        }
        Register::Glossary => {
            let mut sorted: Vec<&GlossaryRow> = view.glossary().iter().map(|l| &l.value).collect();
            sorted.sort_by(|a, b| a.term_id.cmp(&b.term_id));
            sorted
                .into_iter()
                .map(|r| build_glossary_row_view(from_path, &kept, r, view, planned))
                .collect()
        }
    };

    let row_count_label = if reg == Register::Claims {
        let total_claims = view.claims().len();
        match total_claims {
            1 => "1 row".to_owned(),
            n => format!("{n} rows"),
        }
    } else {
        match rows.len() {
            1 => "1 row".to_owned(),
            n => format!("{n} rows"),
        }
    };

    let claims_index = if reg == Register::Claims && !view.claims().is_empty() {
        Some(build_claims_index(view, planned))
    } else {
        None
    };

    RegisterView {
        register: reg,
        slug: register_slug(reg),
        output_path: from_path.to_owned(),
        file_name: reg.file_name(),
        title: register_title(reg).to_owned(),
        subtitle: register_subtitle(reg).to_owned(),
        caption: register_caption(reg),
        columns,
        show_claims_filter: false,
        filter_options: Vec::new(),
        conflict_boxes,
        row_count_label,
        rows,
        part_nav: None,
        claims_index,
        csv_file_name: reg.file_name().to_owned(),
        csv_href: if reg == Register::Claims {
            None
        } else {
            href_if_planned(from_path, &csv_copy::output_path(reg), planned)
        },
        workbook_href: href_if_planned(from_path, xlsx::OUTPUT_PATH, planned),
    }
}

fn build_claims_index(view: &LabelledView, planned: &PlannedPaths) -> ClaimsIndexView {
    let parts = claims_parts(view);
    let from_path = output_path(Register::Claims);

    let mut area_map: BTreeMap<String, (String, usize, Vec<ClaimsIndexPartView>)> = BTreeMap::new();
    for part in parts {
        let part_view = ClaimsIndexPartView {
            href: href_if_planned(from_path, &part.route, planned),
            label: format!("{} to {}", part.first_id, part.last_id),
            count_label: match part.count {
                1 => "1 claim".to_owned(),
                n => format!("{n} claims"),
            },
        };
        let entry = area_map
            .entry(part.area_code.clone())
            .or_insert_with(|| (part.area_name.clone(), 0, Vec::new()));
        entry.1 += part.count;
        entry.2.push(part_view);
    }

    let areas = area_map
        .into_iter()
        .map(|(code, (name, count, parts))| ClaimsIndexAreaView {
            name,
            code,
            count_label: match count {
                1 => "1 claim".to_owned(),
                n => format!("{n} claims"),
            },
            parts,
        })
        .collect();

    ClaimsIndexView { areas }
}

/// Groups permitted claims by area code and part number into existing [`ClaimsPart`] pages.
///
/// Only parts with at least one claim are included, sorted by `(area_code, part)`.
/// Deterministic with no [`std::collections::HashMap`].
#[must_use]
pub fn claims_parts(view: &LabelledView) -> Vec<ClaimsPart> {
    let mut claims: Vec<&ClaimRow> = view.claims().iter().map(|l| &l.value).collect();
    claims.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));

    let mut grouped: BTreeMap<(String, u32), Vec<&ClaimRow>> = BTreeMap::new();
    for row in claims {
        if let Some((area, part)) = claim_part(row.claim_id.as_str()) {
            grouped.entry((area, part)).or_default().push(row);
        }
    }

    let mut area_part_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (area, _part) in grouped.keys() {
        *area_part_counts.entry(area.clone()).or_insert(0) += 1;
    }

    let mut current_area = String::new();
    let mut current_ordinal = 0usize;
    let mut result = Vec::with_capacity(grouped.len());

    for ((area_code, part), part_claims) in grouped {
        if part_claims.is_empty() {
            continue;
        }
        if area_code != current_area {
            current_area = area_code.clone();
            current_ordinal = 1;
        } else {
            current_ordinal += 1;
        }
        let of = area_part_counts.get(&area_code).copied().unwrap_or(1);
        let (Some(first), Some(last)) = (part_claims.first(), part_claims.last()) else {
            continue;
        };
        let first_id = first.claim_id.as_str().to_owned();
        let last_id = last.claim_id.as_str().to_owned();
        let area_name = AreaCode::parse(&area_code.to_ascii_lowercase())
            .ok()
            .and_then(|ac| area_title(view, &ac))
            .map_or_else(|| area_code.clone(), ToOwned::to_owned);
        let route = claim_page_route(&first_id)
            .unwrap_or_else(|| format!("claims/{area_code}-{part}.html"));
        result.push(ClaimsPart {
            area_code,
            area_name,
            part,
            route,
            first_id,
            last_id,
            count: part_claims.len(),
            ordinal: current_ordinal,
            of,
        });
    }

    result
}

/// Returns the site-relative output routes of every existing claims part page.
#[must_use]
pub fn claims_part_paths(view: &LabelledView) -> Vec<String> {
    claims_parts(view).into_iter().map(|p| p.route).collect()
}

/// Build the [`RegisterView`] for the claims part page at `parts[index]`.
#[must_use]
pub fn build_claims_part_view(
    parts: &[ClaimsPart],
    index: usize,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterView {
    let part = &parts[index];
    let from_path = &part.route;
    let kept = Kept::of(view);

    let mut sorted: Vec<&ClaimRow> = view.claims().iter().map(|l| &l.value).collect();
    sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));

    let rows: Vec<RegisterRowView> = sorted
        .into_iter()
        .filter(|r| {
            claim_part(r.claim_id.as_str())
                .map(|(a, p)| a == part.area_code && p == part.part)
                .unwrap_or(false)
        })
        .map(|r| build_claim_row_view(from_path, &kept, r, view, planned))
        .collect();

    let all_claims_href = href_if_planned(from_path, output_path(Register::Claims), planned);
    let prev = index
        .checked_sub(1)
        .and_then(|i| parts.get(i))
        .filter(|p| p.area_code == part.area_code)
        .and_then(|prev_part| {
            href_if_planned(from_path, &prev_part.route, planned).map(|href| {
                (
                    href,
                    format!("Previous: {} to {}", prev_part.first_id, prev_part.last_id),
                )
            })
        });
    let next = parts
        .get(index + 1)
        .filter(|p| p.area_code == part.area_code)
        .and_then(|next_part| {
            href_if_planned(from_path, &next_part.route, planned).map(|href| {
                (
                    href,
                    format!("Next: {} to {}", next_part.first_id, next_part.last_id),
                )
            })
        });

    let part_nav = Some(ClaimsPartNavView {
        all_claims_href,
        prev,
        next,
    });

    let row_count_label = match rows.len() {
        1 => "1 row".to_owned(),
        n => format!("{n} rows"),
    };

    let title = format!(
        "Claims: {}, part {} of {}",
        part.area_name, part.ordinal, part.of
    );

    let columns = Register::Claims.columns().iter().map(|c| c.name).collect();
    let part_csv_path = csv_copy::claims_part_output_path(&part.area_code, part.part);
    let csv_file_name = part_csv_path
        .strip_prefix("data/")
        .unwrap_or(&part_csv_path)
        .to_owned();

    RegisterView {
        register: Register::Claims,
        slug: register_slug(Register::Claims),
        output_path: from_path.clone(),
        file_name: Register::Claims.file_name(),
        title,
        subtitle: register_subtitle(Register::Claims).to_owned(),
        caption: register_caption(Register::Claims),
        columns,
        show_claims_filter: true,
        filter_options: claims_filter_options(),
        conflict_boxes: Vec::new(),
        row_count_label,
        rows,
        part_nav,
        claims_index: None,
        csv_file_name,
        csv_href: href_if_planned(from_path, &part_csv_path, planned),
        workbook_href: href_if_planned(from_path, xlsx::OUTPUT_PATH, planned),
    }
}

/// Render the inner `<main>` content (`templates/register.html`) for the claims part page at `parts[index]`.
#[must_use]
pub fn render_claims_part_main(
    parts: &[ClaimsPart],
    index: usize,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> String {
    let reg_view = build_claims_part_view(parts, index, view, planned);
    let tpl = RegisterTemplate { view: &reg_view };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("register.html is infallible: {e}"))
}

/// Render a complete claims part page (`claims/<AREA>-<part>.html`) wrapped in the global site frame.
#[must_use]
pub fn render_claims_part_page(
    parts: &[ClaimsPart],
    index: usize,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let part = &parts[index];
    let path = &part.route;
    let title = format!(
        "Claims: {}, part {} of {}",
        part.area_name, part.ordinal, part.of
    );
    let main_html = render_claims_part_main(parts, index, view, planned);
    let edition = ctx.edition_info();
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let lib_crumb = match SectionId::Library.first_path() {
        Some(first) => Crumb::linked("Library", first, planned),
        None => Crumb::current("Library"),
    };
    let claims_crumb = Crumb::linked("Claims", output_path(Register::Claims), planned);
    let current_crumb_label = format!("{}, part {} of {}", part.area_name, part.ordinal, part.of);
    let crumbs = [
        lib_crumb,
        claims_crumb,
        Crumb::current(&current_crumb_label),
    ];
    render_frame_page(&FrameInput {
        output_path: path,
        title: &title,
        page_area: None,
        edition,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

/// Render the inner `<main>` content (`templates/register.html`) for `reg`.
#[must_use]
pub fn render_main(reg: Register, view: &LabelledView, planned: &PlannedPaths) -> String {
    let reg_view = build_register_view(reg, view, planned);
    let tpl = RegisterTemplate { view: &reg_view };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("register.html is infallible: {e}"))
}

/// Render a complete `<register>.html` document wrapped in the global site frame.
#[must_use]
pub fn render_page(
    reg: Register,
    view: &LabelledView,
    ctx: &RenderContext,
    nav: &Navigation,
    planned: &PlannedPaths,
) -> String {
    let path = output_path(reg);
    let title = register_title(reg);
    let main_html = render_main(reg, view, planned);
    let edition = ctx.edition_info();
    let store = match ctx {
        RenderContext::Public(_) => StoreContext::Public,
    };
    let lib_crumb = match SectionId::Library.first_path() {
        Some(first) => Crumb::linked("Library", first, planned),
        None => Crumb::current("Library"),
    };
    let crumbs = [lib_crumb, Crumb::current(title)];
    render_frame_page(&FrameInput {
        output_path: path,
        title,
        page_area: None,
        edition,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

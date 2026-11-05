//! Five register page renderers (`claims.html`, `sources.html`, `questions.html`,
//! `conflicts.html`, `glossary.html`) sharing `templates/register.html` (`build-registers`).
//!
//! **Responsibility.** Build [`RegisterView`] from [`LabelledView`] for any [`Register`] and
//! render all five register pages through the single `templates/register.html` template.

use askama::Template;
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::{AreaCode, ClaimId, RefId, VersionId};
use fieldbook_core::schema::{
    ClaimRow, ClaimUnit, ConflictRow, ConflictStatus, GlossaryRow, QuestionRow, Register,
    SourceRow, Verification,
};
use fieldbook_core::strings;

use crate::frame::{Crumb, FrameInput, StoreContext, render_page as render_frame_page};
use crate::inline::{find_conflict, find_question, find_source};
use crate::links::{
    escape_html, render_area_chip, render_claim_chip, render_conflict_chip, render_question_chip,
    render_source_id_chip, render_term_chip, render_version_chip,
};
use crate::meta::RenderContext;
use crate::nav::{Navigation, SectionId};
use crate::site::PlannedPaths;
use crate::status_chip::{
    ICON_ALERT, ICON_CHECK, render_adoption_chip, render_applicability_chip, render_handling_chips,
    render_verification_chip,
};

/// Number of non-overlay columns rendered on `questions.html` (excludes `answer`, `answered_on`, `adoption`).
pub const QUESTIONS_CORPUS_COLS: usize = 10;

/// One rendered table cell in a register row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterCellView {
    /// Canonical column name (`claim_id`, `verification`, etc.).
    pub col_name: &'static str,
    /// Plain sort key used by client-side table sorting in `app.js`.
    pub sort_key: String,
    /// Pre-escaped / pre-rendered cell inner HTML.
    pub html: String,
}

/// One rendered `<tr id="...">` row in a register table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterRowView {
    /// Row anchor `id` (`CL-...`, `S-nnnn-v<k>`, `Q-...`, `XC-...`, `<term_id>`).
    pub id: String,
    /// Canonical `verification` value for `claims.html` client-side filtering, or `None`.
    pub verification_filter: Option<String>,
    /// Ordered cells matching [`RegisterView::columns`].
    pub cells: Vec<RegisterCellView>,
}

/// One verification filter option button on `claims.html`.
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

/// Complete view model for one of the five register pages (`templates/register.html`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterView {
    /// Register kind.
    pub register: Register,
    /// Lowercase register slug (`"claims"`, `"sources"`, `"questions"`, `"conflicts"`, `"glossary"`).
    pub slug: &'static str,
    /// Site-relative output path (`"claims.html"`, `"sources.html"`, ...).
    pub output_path: &'static str,
    /// CSV filename in `corpus/registers/` (`"claims.csv"`, ...).
    pub file_name: &'static str,
    /// Page `<h1>` and `<title>`.
    pub title: &'static str,
    /// Subtitle under `<h1>`.
    pub subtitle: &'static str,
    /// Table `<caption>`.
    pub caption: &'static str,
    /// Ordered column headers (`<th scope="col">`).
    pub columns: Vec<&'static str>,
    /// `true` on `claims.html` to render the verification filter bar.
    pub show_claims_filter: bool,
    /// Filter buttons for `claims.html`.
    pub filter_options: Vec<FilterOptionView>,
    /// Register-variant `ConflictBox` cards for `conflicts.html`.
    pub conflict_boxes: Vec<RegisterConflictBoxView>,
    /// Sorted table rows.
    pub rows: Vec<RegisterRowView>,
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
            "Where sources or reports disagree. Unresolved conflicts keep every position and name the settling question without showing a winner or average."
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

fn text_cell(col_name: &'static str, text: &str) -> RegisterCellView {
    RegisterCellView {
        col_name,
        sort_key: text.to_owned(),
        html: escape_html(text),
    }
}

fn mono_cell(col_name: &'static str, text: &str) -> RegisterCellView {
    RegisterCellView {
        col_name,
        sort_key: text.to_owned(),
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
    row: &SourceRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterRowView {
    let cols = Register::Sources.columns();
    let vid_str = format!("{}-v{}", row.source_id, row.version);
    let vid = VersionId::parse(&vid_str)
        .unwrap_or_else(|e| unreachable!("valid SourceRow produces valid VersionId: {e}"));

    let supersedes_cell = if row.supersedes.is_empty() {
        empty_cell(cols[18].name, "First version")
    } else {
        let sort_key = row
            .supersedes
            .iter()
            .map(VersionId::as_str)
            .collect::<Vec<_>>()
            .join("|");
        let html = row
            .supersedes
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
        opt_text_cell(cols[2].name, row.tag.as_deref(), "No short name"),
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
            "Fixture or company record",
        ),
        // 8: licence
        text_cell(cols[8].name, &row.licence),
        // 9: source_family
        opt_text_cell(cols[9].name, row.source_family.as_deref(), "Not grouped"),
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
            "Not stated on the document",
        ),
        // 14: effective_from
        opt_mono_cell(
            cols[14].name,
            row.effective_from.as_ref().map(ToString::to_string),
            "Same as published, or not dated",
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
        opt_text_cell(cols[21].name, row.note.as_deref(), "None"),
        // 22: purpose
        opt_text_cell(cols[22].name, row.purpose.as_deref(), "Not stated"),
        // 23: origin_system
        opt_text_cell(cols[23].name, row.origin_system.as_deref(), "Added by hand"),
        // 24: origin_stream
        opt_text_cell(cols[24].name, row.origin_stream.as_deref(), "Added by hand"),
        // 25: origin_key
        opt_text_cell(cols[25].name, row.origin_key.as_deref(), "Added by hand"),
        // 26: doc_kind
        opt_text_cell(cols[26].name, row.doc_kind.as_deref(), "Public source"),
        // 27: series
        opt_text_cell(cols[27].name, row.series.as_deref(), "Single document"),
    ];

    RegisterRowView {
        id: vid_str,
        verification_filter: None,
        cells,
    }
}

fn build_claim_row_view(
    from_path: &str,
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
        (_, None) => empty_cell(cols[6].name, "No source"),
    };

    let verified_on_cell = match row.verified_on {
        Some(d) => mono_cell(cols[28].name, &d.to_string()),
        None => RegisterCellView {
            col_name: cols[28].name,
            sort_key: String::new(),
            html: "<span class=\"muted unver\">Never verified</span>".to_owned(),
        },
    };

    let supersedes_cell = if row.supersedes.is_empty() {
        empty_cell(cols[30].name, "First claim")
    } else {
        let sort_key = row
            .supersedes
            .iter()
            .map(ClaimId::as_str)
            .collect::<Vec<_>>()
            .join("|");
        let html = row
            .supersedes
            .iter()
            .map(|cid| render_claim_chip(from_path, cid, planned))
            .collect::<Vec<_>>()
            .join(" ");
        RegisterCellView {
            col_name: cols[30].name,
            sort_key,
            html,
        }
    };

    let superseded_by_cell = if row.superseded_by.is_empty() {
        empty_cell(cols[31].name, "Current")
    } else {
        let sort_key = row
            .superseded_by
            .iter()
            .map(ClaimId::as_str)
            .collect::<Vec<_>>()
            .join("|");
        let html = row
            .superseded_by
            .iter()
            .map(|cid| render_claim_chip(from_path, cid, planned))
            .collect::<Vec<_>>()
            .join(" ");
        RegisterCellView {
            col_name: cols[31].name,
            sort_key,
            html,
        }
    };

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
            "Placeholder assumption",
        ),
        // 5: source_id
        match &row.source_id {
            Some(sid) => RegisterCellView {
                col_name: cols[5].name,
                sort_key: sid.as_str().to_owned(),
                html: render_source_id_chip(from_path, sid, view, planned),
            },
            None => empty_cell(cols[5].name, "No source yet (assumption)"),
        },
        // 6: version
        version_cell,
        // 7: locator_kind (unmapped enum -> canonical text)
        opt_text_cell(
            cols[7].name,
            row.locator_kind.map(|v| v.as_str()),
            "No source",
        ),
        // 8: locator
        opt_text_cell(
            cols[8].name,
            row.locator.as_deref(),
            "Whole source or assumption",
        ),
        // 9: quote
        opt_text_cell(cols[9].name, row.quote.as_deref(), "Not quoted"),
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
        opt_mono_cell(cols[17].name, row.value.clone(), "Prose claim"),
        // 18: unit (unmapped enum -> canonical text)
        opt_text_cell(
            cols[18].name,
            row.unit.as_ref().map(ClaimUnit::as_str),
            "No value",
        ),
        // 19: effective_from
        opt_mono_cell(
            cols[19].name,
            row.effective_from.as_ref().map(ToString::to_string),
            "Not dated",
        ),
        // 20: effective_to
        opt_mono_cell(
            cols[20].name,
            row.effective_to.as_ref().map(ToString::to_string),
            "Still in force",
        ),
        // 21: instrument (unmapped enum -> canonical text)
        opt_text_cell(
            cols[21].name,
            row.instrument.map(|v| v.as_str()),
            "Not a clause claim",
        ),
        // 22: citation_codified
        opt_text_cell(
            cols[22].name,
            row.citation_codified.as_deref(),
            "No codified text",
        ),
        // 23: citation_rfo
        opt_text_cell(
            cols[23].name,
            row.citation_rfo.as_deref(),
            "No deviation or RFO text",
        ),
        // 24: edition_date
        opt_mono_cell(
            cols[24].name,
            row.edition_date.as_ref().map(ToString::to_string),
            "Not a clause claim",
        ),
        // 25: text_status (unmapped enum -> canonical text)
        opt_text_cell(
            cols[25].name,
            row.text_status.map(|v| v.as_str()),
            "Not a clause claim",
        ),
        // 26: applies_to
        opt_text_cell(cols[26].name, row.applies_to.as_deref(), "Not scoped"),
        // 27: evidence_grade (unmapped enum -> canonical text)
        opt_text_cell(
            cols[27].name,
            row.evidence_grade.map(|v| v.as_str()),
            "Not a calculation input",
        ),
        // 28: verified_on
        verified_on_cell,
        // 29: verified_by_role (unmapped enum -> canonical text)
        opt_text_cell(
            cols[29].name,
            row.verified_by_role.map(|v| v.as_str()),
            "Never verified",
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
            None => empty_cell(cols[32].name, "No conflict"),
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
            None => empty_cell(cols[33].name, "Does not wait on a question"),
        },
        // 34: note
        opt_text_cell(cols[34].name, row.note.as_deref(), "None"),
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
        opt_text_cell(cols[3].name, row.owner_role.as_deref(), "Not yet assigned"),
        // 4: blocks
        opt_text_cell(cols[4].name, row.blocks.as_deref(), "Not yet scoped"),
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
            "None: work waits",
        ),
        // 7: work_can_continue (unmapped enum -> canonical text)
        opt_text_cell(
            cols[7].name,
            row.work_can_continue.map(|v| v.as_str()),
            "Not yet triaged",
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
        format!("<span class=\"chip ok\">{ICON_CHECK}<span>Resolved</span></span>")
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
            None => empty_cell(cols[10].name, "Resolved, or not yet tied"),
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
    row: &GlossaryRow,
    view: &LabelledView,
    planned: &PlannedPaths,
) -> RegisterRowView {
    let cols = Register::Glossary.columns();

    let source_ref_cell = if row.source_ref.is_empty() {
        empty_cell(cols[5].name, "No external source")
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
        empty_cell(cols[7].name, "No aliases")
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
        match &row.superseded_by {
            Some(tid) => RegisterCellView {
                col_name: cols[8].name,
                sort_key: tid.as_str().to_owned(),
                html: render_term_chip(from_path, tid, planned),
            },
            None => empty_cell(cols[8].name, "Current"),
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

    let mut conflict_boxes = Vec::new();
    let rows = match reg {
        Register::Sources => {
            let mut sorted: Vec<&SourceRow> = view.sources().iter().map(|l| &l.value).collect();
            sorted.sort_by(|a, b| {
                a.source_id
                    .cmp(&b.source_id)
                    .then_with(|| a.version.cmp(&b.version))
            });
            sorted
                .into_iter()
                .map(|r| build_source_row_view(from_path, r, view, planned))
                .collect()
        }
        Register::Claims => {
            let mut sorted: Vec<&ClaimRow> = view.claims().iter().map(|l| &l.value).collect();
            sorted.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
            sorted
                .into_iter()
                .map(|r| build_claim_row_view(from_path, r, view, planned))
                .collect()
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
            sorted.sort_by(|a, b| a.conflict_id.cmp(&b.conflict_id));
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
                .map(|r| build_glossary_row_view(from_path, r, view, planned))
                .collect()
        }
    };

    let show_claims_filter = reg == Register::Claims;
    let filter_options = if show_claims_filter {
        claims_filter_options()
    } else {
        Vec::new()
    };

    RegisterView {
        register: reg,
        slug: register_slug(reg),
        output_path: from_path,
        file_name: reg.file_name(),
        title: register_title(reg),
        subtitle: register_subtitle(reg),
        caption: register_caption(reg),
        columns,
        show_claims_filter,
        filter_options,
        conflict_boxes,
        rows,
    }
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
    let built_date = match ctx {
        RenderContext::Public(edition) => {
            (!edition.built.is_empty()).then_some(edition.built.as_str())
        }
    };
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
        built_date,
        store: &store,
        crumbs: &crumbs,
        main_html: &main_html,
        nav,
    })
}

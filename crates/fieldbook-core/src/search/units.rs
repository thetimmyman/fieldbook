//! Search document units and the [`UNIT_KINDS`] table (`docs/reference/search.md#document-units`).
//!
//! Defines [`Unit`], [`UnitDisplay`], [`UnitFields`], [`UnitKind`] and [`UNIT_KINDS`], builds the
//! five token fields (`id`, `title`, `headings`, `claim`, `body`) and display metadata of Corpus
//! document units with synonym expansion applied, implements the 160-code-point snippet rule
//! ([`make_snippet`]) and natural identifier ordering ([`natural_id_cmp`]), and selects Corpus
//! units through [`shipped_corpus`] and a label predicate.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::check::PageEntry;
use crate::check::markdown::{self, Ev, Event, HeadingLevel, Tag, TagEnd};
use crate::check::page_parts;
use crate::ids::VersionId;
use crate::labels::{LabelId, LabelSet};
use crate::manifest::{CorpusSource, ManifestError, shipped_corpus};
use crate::schema::page;
use crate::schema::rows::RefId;
use crate::schema::table;
use crate::schema::vocab::{Lifecycle, PageType};
use crate::schema::{ClaimRow, ConflictRow, GlossaryRow, QuestionRow, Register, SourceRow};

use super::lexer::index_tokens;
use super::normalise::normalise;
use super::synonyms::{Glossary, SynonymSet, apply_synonym_sets};

/// One row of [`UNIT_KINDS`], naming a document unit kind and its optional site link template(s)
/// from `docs/reference/search.md#document-units`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitKind {
    /// The unit kind string stored in the index (`page`, `clause`, `source`, `claim`, `question`,
    /// `conflict`, `term`, `record`, `text`).
    pub kind: &'static str,
    /// The primary link form from the Links table (`None` for `record` until `later-record-views`).
    pub link: Option<&'static str>,
    /// The section link form from the Links table (`Some("p/<page-id>.html#<anchor>")` for `page`
    /// and `clause`, `None` otherwise).
    pub section_link: Option<&'static str>,
}

/// Every document unit kind of `docs/reference/search.md#document-units`, in table order.
pub const UNIT_KINDS: [UnitKind; 9] = [
    UnitKind {
        kind: "page",
        link: Some("p/<page-id>.html"),
        section_link: Some("p/<page-id>.html#<anchor>"),
    },
    UnitKind {
        kind: "clause",
        link: Some("p/<page-id>.html"),
        section_link: Some("p/<page-id>.html#<anchor>"),
    },
    UnitKind {
        kind: "source",
        link: Some("s/S-0042-v2.html"),
        section_link: None,
    },
    UnitKind {
        kind: "claim",
        link: Some("claims/CMP-1.html#CL-CMP-0011"),
        section_link: None,
    },
    UnitKind {
        kind: "question",
        link: Some("questions.html#Q-027"),
        section_link: None,
    },
    UnitKind {
        kind: "conflict",
        link: Some("conflicts.html#XC-14"),
        section_link: None,
    },
    UnitKind {
        kind: "term",
        link: Some("glossary.html#material_weakness"),
        section_link: None,
    },
    UnitKind {
        kind: "record",
        link: None,
        section_link: None,
    },
    UnitKind {
        kind: "text",
        link: Some("s/SO-000042-v2.html"),
        section_link: None,
    },
];

/// Looks up a [`UnitKind`] row in [`UNIT_KINDS`] by its `kind` name.
#[must_use]
pub fn lookup_unit_kind(kind: &str) -> Option<&'static UnitKind> {
    UNIT_KINDS.iter().find(|k| k.kind == kind)
}

/// Formats the relative edition link for a unit of `kind` and `id`, or `None` when the kind has
/// no link (`record`) or is unknown.
#[must_use]
pub fn unit_href(kind: &str, id: &str) -> Option<String> {
    let row = lookup_unit_kind(kind)?;
    row.link?;
    match row.kind {
        "page" | "clause" => {
            if let Some((page_id, anchor)) = id.split_once('#') {
                Some(format!("p/{page_id}.html#{anchor}"))
            } else {
                Some(format!("p/{id}.html"))
            }
        }
        "source" => Some(format!("s/{id}.html")),
        "claim" => crate::ids::claim_href_target(id),
        "question" => Some(format!("questions.html#{id}")),
        "conflict" => Some(format!("conflicts.html#{id}")),
        "term" => Some(format!("glossary.html#{id}")),
        "text" => {
            let ver_id = id.split_once('#').map_or(id, |(v, _)| v);
            Some(format!("s/{ver_id}.html"))
        }
        _ => None,
    }
}

/// The seven non-searched display values stored for each [`Unit`]
/// (`docs/reference/search.md#display-values-and-the-result-line`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDisplay {
    /// Display `title` (page `title`, section `##` heading, source `title`, `claim_id`, `question`,
    /// `topic`, `label`, or record `title`).
    pub title: String,
    /// Display `parent` (page `title` for sections, record type name, version id for text windows,
    /// or `""`).
    pub parent: String,
    /// 160-code-point snippet (`docs/reference/search.md#display-values-and-the-result-line`).
    pub snippet: String,
    /// Status string (`draft`, `reviewed`, `stable`, `active`, `unchecked`, ...).
    pub status: String,
    /// `as_of` date or partial date as written, or `""`.
    pub as_of: String,
    /// `effective_from` date, partial date or period as written, or `""`.
    pub effective_from: String,
    /// `verified_on` date as written, or `""`.
    pub verified_on: String,
}

/// Applies the 160-code-point snippet rule (`docs/reference/search.md#display-values-and-the-result-line`):
/// collapses runs of whitespace to one space and trims; if at most 160 code points, returns it;
/// otherwise takes the first 159 code points, cuts back to the last space if there is one, trims,
/// and appends `…` (`U+2026`).
#[must_use]
pub fn make_snippet(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= 160 {
        return collapsed;
    }
    let first_159: String = collapsed.chars().take(159).collect();
    let prefix = match first_159.rfind(' ') {
        Some(space_idx) => first_159[..space_idx].trim(),
        None => first_159.trim(),
    };
    format!("{prefix}\u{2026}")
}

/// Compares two strings in UTF-16 code-unit order, matching JavaScript string comparison
/// (`docs/reference/search.md#where-rust-and-javascript-can-drift`).
#[must_use]
pub fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdRun<'a> {
    Digits(&'a str),
    NonDigits(&'a str),
}

fn split_id_runs(s: &str) -> Vec<IdRun<'_>> {
    let mut runs = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let is_digit = bytes[i].is_ascii_digit();
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() == is_digit {
            i += 1;
        }
        let slice = &s[start..i];
        if is_digit {
            runs.push(IdRun::Digits(slice));
        } else {
            runs.push(IdRun::NonDigits(slice));
        }
    }
    runs
}

fn cmp_digit_runs(a: &str, b: &str) -> Ordering {
    let a_sig = a.trim_start_matches('0');
    let b_sig = b.trim_start_matches('0');
    a_sig
        .len()
        .cmp(&b_sig.len())
        .then_with(|| a_sig.as_bytes().cmp(b_sig.as_bytes()))
        .then_with(|| a.len().cmp(&b.len()))
}

/// Compares two unit identifiers in natural ID order (`docs/reference/search.md#query-procedure`):
/// compares folded IDs (`normalise`) as sequences of maximal digit runs and maximal non-digit
/// runs. A digit run sorts before a non-digit run; digit runs compare by numeric value then by
/// length (shorter first); non-digit runs compare in UTF-16 order; a shorter sequence sorts before
/// a longer one that starts the same way.
#[must_use]
pub fn natural_id_cmp(a: &str, b: &str) -> Ordering {
    let fa = normalise(a);
    let fb = normalise(b);
    let runs_a = split_id_runs(&fa);
    let runs_b = split_id_runs(&fb);

    for (ra, rb) in runs_a.iter().zip(runs_b.iter()) {
        let ord = match (ra, rb) {
            (IdRun::Digits(da), IdRun::Digits(db)) => cmp_digit_runs(da, db),
            (IdRun::Digits(_), IdRun::NonDigits(_)) => Ordering::Less,
            (IdRun::NonDigits(_), IdRun::Digits(_)) => Ordering::Greater,
            (IdRun::NonDigits(na), IdRun::NonDigits(nb)) => utf16_cmp(na, nb),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }

    runs_a
        .len()
        .cmp(&runs_b.len())
        .then_with(|| utf16_cmp(a, b))
}

/// The five searchable token fields of a [`Unit`], in weight order (`id`, `title`, `headings`,
/// `claim`, `body`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnitFields {
    /// Weight 6: lexed unit identifier (empty for page section and text-window units).
    pub id: Vec<String>,
    /// Weight 4: unit title / question / topic / label.
    pub title: Vec<String>,
    /// Weight 2: section headings (`##` and deeper) or glossary aliases.
    pub headings: Vec<String>,
    /// Weight 2: claim `statement`.
    pub claim: Vec<String>,
    /// Weight 1: body text, notes, quotes, rationale, definitions and expanded synonyms.
    pub body: Vec<String>,
}

impl UnitFields {
    /// Creates [`UnitFields`] from raw field strings by running [`index_tokens`] on each field and
    /// applying `sets` to the non-`id` fields.
    #[must_use]
    pub fn from_raw(
        id_field: &str,
        title_field: &str,
        headings_field: &str,
        claim_field: &str,
        body_field: &str,
        sets: &[SynonymSet],
    ) -> Self {
        let mut fields = Self {
            id: index_tokens(id_field),
            title: index_tokens(title_field),
            headings: index_tokens(headings_field),
            claim: index_tokens(claim_field),
            body: index_tokens(body_field),
        };
        apply_synonym_sets(sets, &mut fields);
        fields
    }

    /// Returns the five token slices in fixed BM25F field order:
    /// `[id, title, headings, claim, body]`.
    #[must_use]
    pub fn as_array(&self) -> [&[String]; 5] {
        [
            &self.id,
            &self.title,
            &self.headings,
            &self.claim,
            &self.body,
        ]
    }
}

/// One searchable document unit with its identifier, kind, five token fields, display metadata,
/// optional `exact_key`, and computed in-memory [`LabelSet`].
#[derive(Debug, Clone)]
pub struct Unit {
    /// The unit identifier as written (`CL-CMP-0011`, `cmp-withholding#the-rule`).
    pub id: String,
    /// The unit kind from [`UNIT_KINDS`] (`page`, `clause`, `source`, `claim`, `question`,
    /// `conflict`, `term`, `record`, `text`).
    pub kind: String,
    /// The five final token fields (after index-side extras and synonym expansion).
    pub fields: UnitFields,
    /// The seven non-searched display values stored in `docs`.
    pub display: UnitDisplay,
    /// Unmodified NFC `origin_key` for company source/record units, or `""` otherwise.
    pub exact_key: String,
    /// In-memory labels carried by the unit (`units::labels_of`; never stored in the edition index).
    pub labels: LabelSet,
    /// Tracks whether synonym expansion has already been applied to this unit.
    pub(crate) synonyms_applied: bool,
}

impl PartialEq for Unit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.kind == other.kind
            && self.fields == other.fields
            && self.display == other.display
            && self.exact_key == other.exact_key
            && self.labels == other.labels
    }
}

impl Eq for Unit {}

/// Returns the computed in-memory [`LabelSet`] of `unit` (`docs/reference/search.md#the-site-index`).
#[must_use]
pub const fn labels_of(unit: &Unit) -> &LabelSet {
    &unit.labels
}

fn parse_handling_slice(items: &[String]) -> LabelSet {
    if items.is_empty() {
        return LabelSet::unlabelled();
    }
    let mut ids = Vec::with_capacity(items.len());
    for s in items {
        let Ok(id) = LabelId::parse(s) else {
            return LabelSet::unlabelled();
        };
        ids.push(id);
    }
    LabelSet::from_ids(ids)
}

impl Unit {
    /// Creates a [`Unit`] with its identifier, kind, and tokenized [`UnitFields`], defaulting
    /// `display` and `exact_key` to empty strings and `labels` to [`LabelSet::public`].
    #[must_use]
    pub fn new(id: impl Into<String>, kind: impl Into<String>, fields: UnitFields) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            fields,
            display: UnitDisplay::default(),
            exact_key: String::new(),
            labels: LabelSet::public(),
            synonyms_applied: false,
        }
    }

    pub(crate) const fn with_synonyms_applied(mut self, applied: bool) -> Self {
        self.synonyms_applied = applied;
        self
    }

    /// Sets the [`UnitDisplay`] of this unit.
    #[must_use]
    pub fn with_display(mut self, display: UnitDisplay) -> Self {
        self.display = display;
        self
    }

    /// Sets the unmodified NFC `exact_key` of this unit.
    #[must_use]
    pub fn with_exact_key(mut self, exact_key: impl Into<String>) -> Self {
        self.exact_key = exact_key.into().nfc().collect();
        self
    }

    /// Sets the in-memory [`LabelSet`] of this unit.
    #[must_use]
    pub fn with_labels(mut self, labels: LabelSet) -> Self {
        self.labels = labels;
        self
    }

    /// Builds the page-lead [`Unit`] and each `##` page-section [`Unit`] for a parsed [`PageEntry`].
    #[must_use]
    pub fn from_page_entry(page: &PageEntry, sets: &[SynonymSet]) -> Vec<Self> {
        let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
        let mut units = build_page_units(
            page.id.as_str(),
            &page.meta.title,
            page.meta.r#type,
            &page.body,
            base_line,
            sets,
        );
        let status = page.meta.status.as_str().to_owned();
        let as_of = page.meta.as_of.to_string();
        let labels = parse_handling_slice(&page.meta.handling);
        for u in &mut units {
            u.display.status.clone_from(&status);
            u.display.as_of.clone_from(&as_of);
            u.labels = labels.clone();
        }
        units
    }

    /// Builds a `source` [`Unit`] from a [`SourceRow`], returning `None` when the version is
    /// `quarantined`.
    #[must_use]
    pub fn from_source_row(row: &SourceRow, sets: &[SynonymSet]) -> Option<Self> {
        if row.lifecycle == Lifecycle::Quarantined {
            return None;
        }
        let unit_id = format!("{}-v{}", row.source_id, row.version);
        let body = join_optional_parts(&[
            Some(row.publisher.as_str()),
            row.tag.as_deref(),
            row.note.as_deref(),
        ]);
        let fields = UnitFields::from_raw(&unit_id, &row.title, "", "", &body, sets);
        let snippet_src = match row.note.as_deref() {
            Some(n) if !n.trim().is_empty() => n,
            _ => row.publisher.as_str(),
        };
        let display = UnitDisplay {
            title: row.title.clone(),
            parent: String::new(),
            snippet: make_snippet(snippet_src),
            status: row.lifecycle.as_str().to_owned(),
            as_of: row.as_of.to_string(),
            effective_from: row
                .effective_from
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            verified_on: String::new(),
        };
        let labels = parse_handling_slice(&row.handling);
        Some(
            Self::new(unit_id, "source", fields)
                .with_display(display)
                .with_labels(labels)
                .with_synonyms_applied(!sets.is_empty()),
        )
    }

    /// Builds a `claim` [`Unit`] from a [`ClaimRow`].
    #[must_use]
    pub fn from_claim_row(row: &ClaimRow, sets: &[SynonymSet]) -> Self {
        let id = row.claim_id.as_str();
        let body = join_optional_parts(&[row.quote.as_deref(), row.note.as_deref()]);
        let fields = UnitFields::from_raw(id, "", "", &row.statement, &body, sets);
        let display = UnitDisplay {
            title: id.to_owned(),
            parent: String::new(),
            snippet: make_snippet(&row.statement),
            status: row.verification.as_str().to_owned(),
            as_of: row
                .as_of
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            effective_from: row
                .effective_from
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            verified_on: row
                .verified_on
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
        };
        Self::new(id, "claim", fields)
            .with_display(display)
            .with_synonyms_applied(!sets.is_empty())
    }

    /// Builds a `question` [`Unit`] from a [`QuestionRow`].
    #[must_use]
    pub fn from_question_row(row: &QuestionRow, sets: &[SynonymSet]) -> Self {
        let id = row.question_id.as_str();
        let body = join_optional_parts(&[
            row.why_it_matters.as_deref(),
            row.default_assumption.as_deref(),
            row.answer.as_deref(),
        ]);
        let fields = UnitFields::from_raw(id, &row.question, "", "", &body, sets);
        let display = UnitDisplay {
            title: row.question.clone(),
            parent: String::new(),
            snippet: make_snippet(row.why_it_matters.as_deref().unwrap_or("")),
            status: row.status.as_str().to_owned(),
            as_of: String::new(),
            effective_from: String::new(),
            verified_on: String::new(),
        };
        Self::new(id, "question", fields)
            .with_display(display)
            .with_synonyms_applied(!sets.is_empty())
    }

    /// Builds a `conflict` [`Unit`] from a [`ConflictRow`].
    #[must_use]
    pub fn from_conflict_row(row: &ConflictRow, sets: &[SynonymSet]) -> Self {
        let id = row.conflict_id.as_str();
        let positions = row.positions.join(" ");
        let body = join_optional_parts(&[
            Some(positions.as_str()),
            row.resolution.as_deref(),
            row.rationale.as_deref(),
        ]);
        let fields = UnitFields::from_raw(id, &row.topic, "", "", &body, sets);
        let snippet_src = match row.resolution.as_deref() {
            Some(r) if !r.trim().is_empty() => r,
            _ => positions.as_str(),
        };
        let display = UnitDisplay {
            title: row.topic.clone(),
            parent: String::new(),
            snippet: make_snippet(snippet_src),
            status: row.status.as_str().to_owned(),
            as_of: String::new(),
            effective_from: String::new(),
            verified_on: String::new(),
        };
        Self::new(id, "conflict", fields)
            .with_display(display)
            .with_synonyms_applied(!sets.is_empty())
    }

    /// Builds a `term` [`Unit`] from a [`GlossaryRow`].
    #[must_use]
    pub fn from_glossary_row(row: &GlossaryRow, sets: &[SynonymSet]) -> Self {
        let id = row.term_id.as_str();
        let aliases = row.aliases.join(" ");
        let body = join_optional_parts(&[
            Some(row.definition.as_str()),
            Some(row.boundary_rule.as_str()),
        ]);
        let fields = UnitFields::from_raw(id, &row.label, &aliases, "", &body, sets);
        let display = UnitDisplay {
            title: row.label.clone(),
            parent: String::new(),
            snippet: make_snippet(&row.definition),
            status: row.definition_status.as_str().to_owned(),
            as_of: String::new(),
            effective_from: String::new(),
            verified_on: String::new(),
        };
        Self::new(id, "term", fields)
            .with_display(display)
            .with_synonyms_applied(!sets.is_empty())
    }
}

/// Builds the page-lead [`Unit`] and each `##` section [`Unit`] from a page's front-matter metadata
/// and Markdown body (`docs/reference/search.md#document-units`).
#[must_use]
pub fn build_page_units(
    page_id: &str,
    page_title: &str,
    page_type: PageType,
    body: &str,
    body_start_line: usize,
    sets: &[SynonymSet],
) -> Vec<Unit> {
    let kind = if page_type == PageType::ClauseCard {
        "clause"
    } else {
        "page"
    };
    let base_line = body_start_line.max(1);
    let evs = markdown::events(body, base_line);
    let parts = page_parts::split_parts(body, base_line);

    let first_h2_event = parts
        .parts()
        .first()
        .map_or(evs.len(), |first_part| first_part.heading_event);
    let lead_evs = evs.get(..first_h2_event).unwrap_or(&[]);
    let (_, lead_body_text) = extract_headings_and_body_text(lead_evs);

    // Page-lead snippet comes from the first paragraph of the first section (or the lead if the
    // page has no `##` sections).
    let lead_snippet_raw = parts.parts().first().map_or_else(
        || extract_first_paragraph_text(lead_evs),
        |first_part| {
            let first_sec_evs = evs.get(first_part.event_range.clone()).unwrap_or(&[]);
            extract_first_paragraph_text(first_sec_evs)
        },
    );
    let lead_display = UnitDisplay {
        title: page_title.to_owned(),
        parent: String::new(),
        snippet: make_snippet(&lead_snippet_raw),
        status: String::new(),
        as_of: String::new(),
        effective_from: String::new(),
        verified_on: String::new(),
    };

    let mut units = Vec::with_capacity(1 + parts.parts().len());
    units.push(
        Unit::new(
            page_id,
            kind,
            UnitFields::from_raw(page_id, page_title, "", "", &lead_body_text, sets),
        )
        .with_display(lead_display)
        .with_synonyms_applied(!sets.is_empty()),
    );

    for part in parts.parts() {
        let anchor = parts
            .heading_anchor(part.heading_event)
            .unwrap_or("section");
        let section_id = format!("{page_id}#{anchor}");
        let part_evs = evs.get(part.event_range.clone()).unwrap_or(&[]);
        let (deeper_headings, section_body) = extract_headings_and_body_text(part_evs);
        let headings_text = if deeper_headings.is_empty() {
            part.heading.clone()
        } else {
            format!("{} {deeper_headings}", part.heading)
        };
        let sec_snippet_raw = extract_first_paragraph_text(part_evs);
        let sec_display = UnitDisplay {
            title: part.heading.clone(),
            parent: page_title.to_owned(),
            snippet: make_snippet(&sec_snippet_raw),
            status: String::new(),
            as_of: String::new(),
            effective_from: String::new(),
            verified_on: String::new(),
        };
        // Section units have an empty `id` field.
        units.push(
            Unit::new(
                section_id,
                kind,
                UnitFields::from_raw("", "", &headings_text, "", &section_body, sets),
            )
            .with_display(sec_display)
            .with_synonyms_applied(!sets.is_empty()),
        );
    }

    units
}

/// Extracts the visible text (`Text` and `Code` events) of the first non-empty paragraph in `evs`.
#[must_use]
pub fn extract_first_paragraph_text(evs: &[Ev<'_>]) -> String {
    let mut in_para = false;
    let mut para = String::new();
    for ev in evs {
        match &ev.event {
            Event::Start(Tag::Paragraph) => {
                in_para = true;
                para.clear();
            }
            Event::End(TagEnd::Paragraph) if in_para => {
                if !para.trim().is_empty() {
                    return para.trim().to_owned();
                }
                in_para = false;
            }
            Event::Text(t) | Event::Code(t) if in_para => {
                para.push_str(t);
            }
            Event::SoftBreak | Event::HardBreak if in_para => {
                push_space(&mut para);
            }
            _ => {}
        }
    }
    para.trim().to_owned()
}

/// Extracts visible Markdown `Text` and `Code` events from `evs`, separating block boundaries and
/// line breaks with a single space and splitting deeper heading text from body text.
#[must_use]
pub fn extract_headings_and_body_text(evs: &[Ev<'_>]) -> (String, String) {
    let mut headings = String::new();
    let mut body = String::new();
    let mut in_heading = false;

    for ev in evs {
        match &ev.event {
            Event::Start(Tag::Heading {
                level:
                    HeadingLevel::H1
                    | HeadingLevel::H2
                    | HeadingLevel::H3
                    | HeadingLevel::H4
                    | HeadingLevel::H5
                    | HeadingLevel::H6,
                ..
            }) => {
                in_heading = true;
                push_space(&mut headings);
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                push_space(&mut headings);
            }
            Event::Text(t) | Event::Code(t) => {
                let target = if in_heading { &mut headings } else { &mut body };
                target.push_str(t);
            }
            Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::Start(
                Tag::Paragraph
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::List(_)
                | Tag::Item
                | Tag::FootnoteDefinition(_)
                | Tag::Table(_)
                | Tag::TableHead
                | Tag::TableRow
                | Tag::TableCell,
            )
            | Event::End(
                TagEnd::Paragraph
                | TagEnd::BlockQuote(_)
                | TagEnd::CodeBlock
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::FootnoteDefinition
                | TagEnd::Table
                | TagEnd::TableHead
                | TagEnd::TableRow
                | TagEnd::TableCell,
            ) => {
                let target = if in_heading { &mut headings } else { &mut body };
                push_space(target);
            }
            _ => {}
        }
    }

    (headings.trim().to_owned(), body.trim().to_owned())
}

fn push_space(buf: &mut String) {
    if !buf.is_empty() && !buf.ends_with(' ') {
        buf.push(' ');
    }
}

fn join_optional_parts(parts: &[Option<&str>]) -> String {
    let mut out = String::new();
    for part in parts.iter().flatten() {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(trimmed);
        }
    }
    out
}

/// Label sets by source version (`S-NNNN-vN`) and, as the union over its versions, by source id,
/// then by claim id. A claim that names a source the register does not hold is `unlabelled`, so a
/// dangling reference never makes a unit look public.
fn label_maps(
    sources: &[SourceRow],
    claims: &[ClaimRow],
) -> (BTreeMap<String, LabelSet>, BTreeMap<String, LabelSet>) {
    let mut source_labels: BTreeMap<String, LabelSet> = BTreeMap::new();
    for s in sources {
        let vid_str = format!("{}-v{}", s.source_id, s.version);
        let lbl = parse_handling_slice(&s.handling);
        source_labels
            .entry(s.source_id.as_str().to_owned())
            .and_modify(|e| *e = e.union(&lbl))
            .or_insert_with(|| lbl.clone());
        source_labels.insert(vid_str, lbl);
    }

    let mut claim_labels: BTreeMap<String, LabelSet> = BTreeMap::new();
    for c in claims {
        let lbl = match (&c.source_id, c.version) {
            (Some(sid), Some(ver)) => source_labels
                .get(&format!("{sid}-v{ver}"))
                .cloned()
                .unwrap_or_else(LabelSet::unlabelled),
            (Some(sid), None) => source_labels
                .get(sid.as_str())
                .cloned()
                .unwrap_or_else(LabelSet::unlabelled),
            (None, _) => LabelSet::public(),
        };
        claim_labels.insert(c.claim_id.as_str().to_owned(), lbl);
    }
    (source_labels, claim_labels)
}

/// The labels of the claim `claim_id`, or `unlabelled` when the register does not hold it.
fn claim_ref_labels(claim_id: &str, claim_labels: &BTreeMap<String, LabelSet>) -> LabelSet {
    claim_labels
        .get(claim_id)
        .cloned()
        .unwrap_or_else(LabelSet::unlabelled)
}

/// The labels of one claim or source-version reference, `unlabelled` when it does not resolve.
fn ref_labels(
    r: &RefId,
    source_labels: &BTreeMap<String, LabelSet>,
    claim_labels: &BTreeMap<String, LabelSet>,
) -> LabelSet {
    match r {
        RefId::Claim(cid) => claim_ref_labels(cid.as_str(), claim_labels),
        RefId::SourceVersion(vid) => source_labels
            .get(vid.as_str())
            .cloned()
            .unwrap_or_else(LabelSet::unlabelled),
    }
}

/// The union of the labels of everything a glossary row cites (`public` when it cites nothing).
fn glossary_row_labels(
    term: &GlossaryRow,
    source_labels: &BTreeMap<String, LabelSet>,
    claim_labels: &BTreeMap<String, LabelSet>,
) -> LabelSet {
    let mut lbl = LabelSet::public();
    for r in &term.source_ref {
        lbl = lbl.union(&ref_labels(r, source_labels, claim_labels));
    }
    lbl
}

/// The glossary of the rows `keep` accepts, so a restricted row's label and aliases never become
/// synonyms of a readable unit.
#[must_use]
pub fn kept_glossary<F>(
    terms: &[GlossaryRow],
    sources: &[SourceRow],
    claims: &[ClaimRow],
    keep: F,
) -> Glossary
where
    F: Fn(&LabelSet) -> bool,
{
    let (source_labels, claim_labels) = label_maps(sources, claims);
    let kept: Vec<GlossaryRow> = terms
        .iter()
        .filter(|t| keep(&glossary_row_labels(t, &source_labels, &claim_labels)))
        .cloned()
        .collect();
    Glossary::from_rows(&kept)
}

/// Builds Corpus document units from parsed slices, excluding any source row whose lifecycle is
/// `quarantined` or whose [`VersionId`] is in `quarantined`, computing each unit's [`LabelSet`]
/// (`units::labels_of`), filtering with `keep`, and sorting into natural ID order.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "unit builder takes the six Corpus item slices, quarantined set, glossary, and label predicate"
)]
pub fn build_corpus_units_with<F>(
    pages: &[PageEntry],
    sources: &[SourceRow],
    claims: &[ClaimRow],
    questions: &[QuestionRow],
    conflicts: &[ConflictRow],
    terms: &[GlossaryRow],
    quarantined: &BTreeSet<VersionId>,
    glossary: &Glossary,
    keep: F,
) -> Vec<Unit>
where
    F: Fn(&LabelSet) -> bool,
{
    let sets = glossary.synonym_sets();
    let (source_labels, claim_labels) = label_maps(sources, claims);

    let mut out = Vec::new();
    for page in pages {
        for u in Unit::from_page_entry(page, &sets) {
            if keep(labels_of(&u)) {
                out.push(u);
            }
        }
    }
    for source in sources {
        let is_quarantined_vid =
            VersionId::parse(&format!("{}-v{}", source.source_id, source.version))
                .is_ok_and(|vid| quarantined.contains(&vid));
        if is_quarantined_vid {
            continue;
        }
        if let Some(u) = Unit::from_source_row(source, &sets)
            && keep(labels_of(&u))
        {
            out.push(u);
        }
    }
    for claim in claims {
        let lbl = claim_labels
            .get(claim.claim_id.as_str())
            .cloned()
            .unwrap_or_else(LabelSet::unlabelled);
        let u = Unit::from_claim_row(claim, &sets).with_labels(lbl);
        if keep(labels_of(&u)) {
            out.push(u);
        }
    }
    for question in questions {
        let u = Unit::from_question_row(question, &sets);
        if keep(labels_of(&u)) {
            out.push(u);
        }
    }
    for conflict in conflicts {
        let mut lbl = LabelSet::public();
        for cid in &conflict.claim_ids {
            lbl = lbl.union(&claim_ref_labels(cid.as_str(), &claim_labels));
        }
        for r in &conflict.evidence {
            lbl = lbl.union(&ref_labels(r, &source_labels, &claim_labels));
        }
        let u = Unit::from_conflict_row(conflict, &sets).with_labels(lbl);
        if keep(labels_of(&u)) {
            out.push(u);
        }
    }
    for term in terms {
        let lbl = glossary_row_labels(term, &source_labels, &claim_labels);
        let u = Unit::from_glossary_row(term, &sets).with_labels(lbl);
        if keep(labels_of(&u)) {
            out.push(u);
        }
    }
    out.sort_by(|a, b| natural_id_cmp(&a.id, &b.id));
    out
}

/// Builds all public Corpus document units from parsed slices, applying synonyms from `glossary`.
#[must_use]
pub fn build_corpus_units(
    pages: &[PageEntry],
    sources: &[SourceRow],
    claims: &[ClaimRow],
    questions: &[QuestionRow],
    conflicts: &[ConflictRow],
    terms: &[GlossaryRow],
    glossary: &Glossary,
) -> Vec<Unit> {
    build_corpus_units_with(
        pages,
        sources,
        claims,
        questions,
        conflicts,
        terms,
        &BTreeSet::new(),
        glossary,
        LabelSet::is_public,
    )
}

/// Builds Corpus document units strictly from the files selected by
/// [`shipped_corpus(source, quarantined)`](shipped_corpus), never reading unshipped paths from the
/// raw tree, explicitly excluding quarantined source rows (`lifecycle == Quarantined` or in
/// `quarantined`), and keeping only units accepted by `keep`.
///
/// # Errors
///
/// Propagates any [`ManifestError`] returned by [`shipped_corpus`].
pub fn build_shipped_corpus_units<S, F>(
    source: &S,
    quarantined: &BTreeSet<VersionId>,
    keep: F,
) -> Result<(Vec<Unit>, Glossary), ManifestError>
where
    S: CorpusSource + ?Sized,
    F: Fn(&LabelSet) -> bool,
{
    let shipped = shipped_corpus(source, quarantined)?;
    let tree = source.public_tree()?;
    let shipped_paths: BTreeSet<&str> = shipped.iter().map(|(p, _)| p.as_str()).collect();

    let unreadable = |path: &str, reason: &str| ManifestError::NonPublicFile {
        path: path.to_owned(),
        reason: reason.to_owned(),
    };

    let mut pages = Vec::new();
    for &path in &shipped_paths {
        if path.starts_with("corpus/pages/") && path.ends_with(".md") {
            let bytes = tree
                .get_bytes(path)
                .ok_or_else(|| unreadable(path, "page is not byte-backed"))?;
            let text = std::str::from_utf8(bytes)
                .map_err(|_| unreadable(path, "page is not valid UTF-8"))?;
            let parts_split = page::split(text)
                .map_err(|_| unreadable(path, "page front matter does not split"))?;
            let meta = page::parse_meta(&parts_split.front_matter)
                .map_err(|_| unreadable(path, "page front matter does not parse"))?;
            let id = meta.id.clone();
            let body = parts_split.body;
            let body_line = parts_split.body_line;
            pages.push((path.to_owned(), id, meta, body, body_line));
        }
    }

    let read_reg = |reg: Register| -> Result<Vec<table::Row>, ManifestError> {
        let path = format!("corpus/registers/{}", reg.file_name());
        if !shipped_paths.contains(path.as_str()) {
            return Ok(Vec::new());
        }
        let bytes = tree
            .get_bytes(&path)
            .ok_or_else(|| unreadable(&path, "register is not byte-backed"))?;
        table::read(bytes)
            .map(|t| t.rows)
            .map_err(|_| unreadable(&path, "register does not parse"))
    };

    macro_rules! typed_rows {
        ($reg:expr, $ty:ty) => {{
            let path = format!("corpus/registers/{}", $reg.file_name());
            let mut out: Vec<$ty> = Vec::new();
            for r in read_reg($reg)? {
                out.push(
                    <$ty>::from_row(&r)
                        .map_err(|_| unreadable(&path, "register row does not parse"))?,
                );
            }
            out
        }};
    }

    let sources: Vec<SourceRow> = typed_rows!(Register::Sources, SourceRow);
    let claims: Vec<ClaimRow> = typed_rows!(Register::Claims, ClaimRow);
    let questions: Vec<QuestionRow> = typed_rows!(Register::Questions, QuestionRow);
    let conflicts: Vec<ConflictRow> = typed_rows!(Register::Conflicts, ConflictRow);
    let all_terms: Vec<GlossaryRow> = typed_rows!(Register::Glossary, GlossaryRow);

    // Only the glossary rows the caller may keep contribute synonyms, so a restricted row's
    // aliases never reach the tokens of a readable unit.
    let glossary = kept_glossary(&all_terms, &sources, &claims, &keep);
    let terms: Vec<GlossaryRow> = all_terms;
    let sets = glossary.synonym_sets();

    let mut page_units = Vec::new();
    for (_path, id, meta, body, body_line) in &pages {
        let base_line = usize::try_from(*body_line).unwrap_or(1).max(1);
        let mut built = build_page_units(
            id.as_str(),
            &meta.title,
            meta.r#type,
            body,
            base_line,
            &sets,
        );
        let status = meta.status.as_str().to_owned();
        let as_of = meta.as_of.to_string();
        let labels = parse_handling_slice(&meta.handling);
        for u in &mut built {
            u.display.status.clone_from(&status);
            u.display.as_of.clone_from(&as_of);
            u.labels = labels.clone();
            if keep(labels_of(u)) {
                page_units.push(u.clone());
            }
        }
    }

    let mut reg_units = build_corpus_units_with(
        &[],
        &sources,
        &claims,
        &questions,
        &conflicts,
        &terms,
        quarantined,
        &glossary,
        &keep,
    );
    page_units.append(&mut reg_units);
    page_units.sort_by(|a, b| natural_id_cmp(&a.id, &b.id));
    Ok((page_units, glossary))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::ids::PageId;

    const SEARCH_MD: &str = include_str!("../../../../docs/reference/search.md");

    fn extract_backticked(cell: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut rest = cell;
        while let Some(open) = rest.find('`') {
            let after_open = &rest[open + 1..];
            let Some(close) = after_open.find('`') else {
                break;
            };
            out.push(&after_open[..close]);
            rest = &after_open[close + 1..];
        }
        out
    }

    #[expect(
        clippy::type_complexity,
        reason = "test parser returns ordered kinds and map of (link, section_link) pairs"
    )]
    fn parse_document_unit_kinds_and_links(
        md: &str,
    ) -> (
        Vec<String>,
        BTreeMap<String, (Option<String>, Option<String>)>,
    ) {
        let mut in_doc_units = false;
        let mut in_display_table = false;
        let mut in_links_table = false;

        let mut display_kinds: Vec<String> = Vec::new();
        let mut link_map: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
        let mut link_order: Vec<String> = Vec::new();

        for line in md.lines() {
            let trimmed = line.trim();
            if trimmed == "## Document units" {
                in_doc_units = true;
                continue;
            }
            if in_doc_units && trimmed.starts_with("## ") {
                break;
            }
            if !in_doc_units {
                continue;
            }

            if trimmed.starts_with("| Unit | Display `title` | `parent` | `kind` |") {
                in_display_table = true;
                continue;
            }
            if in_display_table {
                if !trimmed.starts_with('|') {
                    in_display_table = false;
                } else if !trimmed.starts_with("|---|") {
                    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
                    let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
                    if let Some(&kind_cell) = cells.get(3) {
                        for tick in extract_backticked(kind_cell) {
                            if tick != "clause-card" && !display_kinds.iter().any(|k| k == tick) {
                                display_kinds.push(tick.to_owned());
                            }
                        }
                    }
                }
            }

            if trimmed.starts_with("| Kind | Id form | Link |") {
                in_links_table = true;
                continue;
            }
            if in_links_table {
                if !trimmed.starts_with('|') {
                    in_links_table = false;
                } else if !trimmed.starts_with("|---|") {
                    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
                    let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
                    assert_eq!(
                        cells.len(),
                        3,
                        "expected 3 columns in Links table: {line:?}"
                    );
                    let kinds = extract_backticked(cells[0]);
                    let id_forms = extract_backticked(cells[1]);
                    let id_form = id_forms.first().copied().unwrap_or("");
                    let link_cell = cells[2];
                    let link_opt = if link_cell.starts_with("No link") {
                        None
                    } else {
                        extract_backticked(link_cell).first().map(|&s| s.to_owned())
                    };

                    for k in kinds {
                        if !link_order.iter().any(|existing| existing == k) {
                            link_order.push(k.to_owned());
                        }
                        let entry = link_map.entry(k.to_owned()).or_insert((None, None));
                        if id_form.contains("#<anchor>") {
                            entry.1 = link_opt.clone();
                        } else {
                            entry.0 = link_opt.clone();
                        }
                    }
                }
            }
        }

        assert_eq!(
            display_kinds, link_order,
            "Display values table kinds {display_kinds:?} differ from Links table kinds {link_order:?}"
        );
        (link_order, link_map)
    }

    struct FixtureRows {
        sources: Vec<SourceRow>,
        claims: Vec<ClaimRow>,
        questions: Vec<QuestionRow>,
        conflicts: Vec<ConflictRow>,
        terms: Vec<GlossaryRow>,
    }

    fn fixture_rows(sources_csv: &str, glossary_csv: &str) -> FixtureRows {
        const CLAIMS: &str =
            include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
        const QUESTIONS: &str =
            include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
        const CONFLICTS: &str =
            include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
        fn typed<T>(
            csv: &str,
            from_row: impl Fn(&table::Row) -> Result<T, Vec<crate::schema::CellProblem>>,
        ) -> Vec<T> {
            table::read(csv.as_bytes())
                .expect("fixture register reads")
                .rows
                .iter()
                .map(|r| from_row(r).expect("fixture row parses"))
                .collect()
        }
        FixtureRows {
            sources: typed(sources_csv, SourceRow::from_row),
            claims: typed(CLAIMS, ClaimRow::from_row),
            questions: typed(QUESTIONS, QuestionRow::from_row),
            conflicts: typed(CONFLICTS, ConflictRow::from_row),
            terms: typed(glossary_csv, GlossaryRow::from_row),
        }
    }

    #[test]
    fn labels_decide_which_units_exist_and_never_the_score() {
        const SOURCES: &str =
            include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
        const GLOSSARY: &str =
            include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
        // S-0002 version 2 is restricted; one restricted term carries an alias.
        let restricted_sources: String = SOURCES
            .lines()
            .map(|l| {
                if l.starts_with("S-0002,2,") {
                    format!("{}\n", l.replacen(",V2,public,", ",V2,xx-0001,", 1))
                } else {
                    format!("{l}\n")
                }
            })
            .collect();
        assert_ne!(restricted_sources, SOURCES, "fixture row not found");
        let aliased = GLOSSARY.replace(
            "CL-EST-0003,public_reference,,",
            "CL-EST-0003,public_reference,ratealias,",
        );
        assert_ne!(aliased, GLOSSARY, "fixture glossary row not found");
        let FixtureRows {
            sources,
            claims,
            questions,
            conflicts,
            terms,
        } = fixture_rows(&restricted_sources, &aliased);
        let none = BTreeSet::new();

        let ids = |keep: fn(&LabelSet) -> bool, g: &Glossary| -> BTreeSet<String> {
            build_corpus_units_with(
                &[],
                &sources,
                &claims,
                &questions,
                &conflicts,
                &terms,
                &none,
                g,
                keep,
            )
            .into_iter()
            .map(|u| u.id)
            .collect()
        };
        let public_glossary = kept_glossary(&terms, &sources, &claims, LabelSet::is_public);
        let open_glossary = kept_glossary(&terms, &sources, &claims, |_| true);
        let public = ids(LabelSet::is_public, &public_glossary);
        let open = ids(|_| true, &open_glossary);

        // The restricted version, the claims that cite it and the term that cites a claim of it
        // are left out of a public index and present in an index that accepts everything.
        for held in ["S-0002-v2", "CL-EST-0002", "CL-EST-0003", "rate_window"] {
            assert!(
                !public.contains(held),
                "{held} must not be public: {public:?}"
            );
            assert!(open.contains(held), "{held} must exist when all are kept");
        }
        for shown in ["S-0001-v1", "CL-CMP-0001", "billing_withhold"] {
            assert!(public.contains(shown), "{shown} stays public: {public:?}");
        }
        // The restricted term's aliases are not synonyms in a public index.
        assert!(
            !public_glossary
                .entries()
                .iter()
                .any(|e| e.aliases.iter().any(|a| a == "ratealias")),
            "a restricted row's alias reached the public glossary"
        );
        assert!(
            open_glossary
                .entries()
                .iter()
                .any(|e| e.aliases.iter().any(|a| a == "ratealias"))
        );
    }

    #[test]
    fn a_claim_citing_an_unknown_source_is_not_public() {
        const SOURCES: &str =
            include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
        const GLOSSARY: &str =
            include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
        // Drop every S-0003 row so a claim that cites it dangles.
        let FixtureRows {
            mut sources,
            claims,
            questions,
            conflicts,
            terms,
        } = fixture_rows(SOURCES, GLOSSARY);
        let dangling = claims
            .iter()
            .position(|c| c.source_id.is_some())
            .expect("a sourced claim");
        let dangling_id = claims[dangling].claim_id.as_str().to_owned();
        let cited = claims[dangling].source_id.clone();
        sources.retain(|s| Some(&s.source_id) != cited.as_ref());
        let none = BTreeSet::new();
        let units = build_corpus_units_with(
            &[],
            &sources,
            &claims,
            &questions,
            &conflicts,
            &terms,
            &none,
            &Glossary::default(),
            LabelSet::is_public,
        );
        assert!(
            !units.iter().any(|u| u.id == dangling_id),
            "{dangling_id} cites a source the register does not hold and must not be public"
        );
    }

    #[test]
    fn unit_kinds_match_the_reference() {
        let (doc_order, doc_links) = parse_document_unit_kinds_and_links(SEARCH_MD);
        assert!(
            !doc_order.is_empty(),
            "Document units tables not found in docs/reference/search.md"
        );

        // Direction 1: every kind in search.md must exist in UNIT_KINDS with matching optional link forms.
        for (idx, doc_kind) in doc_order.iter().enumerate() {
            let Some(code_row) = UNIT_KINDS.iter().find(|r| r.kind == doc_kind) else {
                panic!("kind {doc_kind:?} in search.md is missing from UNIT_KINDS");
            };
            assert_eq!(
                UNIT_KINDS.get(idx).map(|r| r.kind),
                Some(doc_kind.as_str()),
                "UNIT_KINDS row {} kind differs from search.md order ({doc_kind:?})",
                idx
            );
            let (exp_link, exp_section_link) = doc_links
                .get(doc_kind)
                .unwrap_or_else(|| panic!("missing link entry for {doc_kind:?}"));
            assert_eq!(
                code_row.link,
                exp_link.as_deref(),
                "UNIT_KINDS kind {doc_kind:?} link {:?} differs from search.md {:?}",
                code_row.link,
                exp_link
            );
            assert_eq!(
                code_row.section_link,
                exp_section_link.as_deref(),
                "UNIT_KINDS kind {doc_kind:?} section_link {:?} differs from search.md {:?}",
                code_row.section_link,
                exp_section_link
            );
        }

        // Direction 2: every row in UNIT_KINDS must exist in search.md with matching optional link forms.
        for code_row in &UNIT_KINDS {
            let Some((exp_link, exp_section_link)) = doc_links.get(code_row.kind) else {
                panic!(
                    "UNIT_KINDS has kind {:?} that is missing from search.md Document units tables",
                    code_row.kind
                );
            };
            assert_eq!(
                code_row.link,
                exp_link.as_deref(),
                "UNIT_KINDS kind {:?} link mismatch",
                code_row.kind
            );
            assert_eq!(
                code_row.section_link,
                exp_section_link.as_deref(),
                "UNIT_KINDS kind {:?} section_link mismatch",
                code_row.kind
            );
        }
    }

    #[test]
    fn page_units_keep_section_id_field_empty_and_omit_link_destinations_and_html() {
        let body = "Lead text with [a link](https://example.invalid/secret-dest) and <span>raw</span> text [@S-0042-v2, p. 14].\n\n## The rule\n\nRule paragraph.\n\n### Subheading\n\nMore `code_var` text.\n";
        let units = build_page_units(
            "cmp-withholding",
            "Payment withholding",
            PageType::Concept,
            body,
            10,
            &[],
        );
        assert_eq!(units.len(), 2);

        let lead = &units[0];
        assert_eq!(lead.id, "cmp-withholding");
        assert_eq!(lead.kind, "page");
        assert_eq!(lead.fields.id, vec!["cmp", "withholding"]);
        assert_eq!(lead.fields.title, vec!["payment", "withholding"]);
        assert!(lead.fields.headings.is_empty());
        assert!(lead.fields.claim.is_empty());
        assert!(lead.fields.body.contains(&"s-0042-v2".to_owned()));
        assert!(lead.fields.body.contains(&"s-0042".to_owned()));
        assert!(!lead.fields.body.iter().any(|t| t.contains("secret")));

        let sec = &units[1];
        assert_eq!(sec.id, "cmp-withholding#the-rule");
        assert_eq!(sec.kind, "page");
        assert!(
            sec.fields.id.is_empty(),
            "section unit must have an empty id field"
        );
        assert!(sec.fields.title.is_empty());
        assert_eq!(sec.fields.headings, vec!["the", "rule", "subheading"]);
        assert_eq!(
            sec.fields.body,
            vec!["rule", "paragraph", "more", "code", "var", "text"]
        );
    }

    // ---- Snippet rule, natural id order, display values and the label ceiling ----

    const FIX_SOURCES: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const FIX_GLOSSARY: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
    const FIX_CLAIMS: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const FIX_QUESTIONS: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const FIX_CONFLICTS: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const FIX_SOURCE_TEXT: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt");
    const FIX_SOURCE_TEXT_S0001: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt");
    const FIX_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const FIX_PAGE_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const FIX_PAGE_CLAUSE: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-clause-9900-101.md"
    );
    const PAGE_DIR: &str = "corpus/pages/compliance/business-systems";

    fn rows() -> FixtureRows {
        fixture_rows(FIX_SOURCES, FIX_GLOSSARY)
    }

    fn words(s: &str) -> Vec<String> {
        index_tokens(s)
    }

    #[test]
    fn snippet_collapses_whitespace_and_keeps_at_most_160_code_points_whole() {
        assert_eq!(make_snippet(""), "");
        assert_eq!(make_snippet(" \t\n "), "");
        assert_eq!(
            make_snippet("  one\ttwo \n\n three\u{a0}four  "),
            "one two three four"
        );

        let exactly_160 = "a".repeat(160);
        assert_eq!(make_snippet(&exactly_160), exactly_160);
        // Code points, not bytes: 160 two-byte letters stay whole, and so do 160 four-byte ones.
        let two_byte = "\u{e9}".repeat(160);
        assert_eq!(make_snippet(&two_byte), two_byte);
        let four_byte = "\u{10000}".repeat(160);
        assert_eq!(make_snippet(&four_byte), four_byte);
    }

    #[test]
    fn snippet_cuts_161_code_points_back_to_the_last_space_and_appends_an_ellipsis() {
        let word = "w".repeat(10);
        // 15 words of 10 letters and 14 spaces are 164 code points. The first 159 end inside the
        // 15th word, and the last space before that is the one after the 14th word.
        let text = vec![word.as_str(); 15].join(" ");
        assert_eq!(text.chars().count(), 164);
        let expected = format!("{}\u{2026}", vec![word.as_str(); 14].join(" "));
        assert_eq!(make_snippet(&text), expected);
        assert_eq!(expected.chars().count(), 14 * 10 + 13 + 1);

        // The cut goes back to the last space even when the first 159 code points end exactly at
        // the end of a word: 14 words, a space and a 5-letter word make 159, then a space follows.
        let text = format!(
            "{} {} tail words",
            vec![word.as_str(); 14].join(" "),
            "v".repeat(5)
        );
        assert_eq!(text.chars().nth(159), Some(' '));
        assert!(text.chars().count() > 160);
        assert_eq!(
            make_snippet(&text),
            format!("{}\u{2026}", vec![word.as_str(); 14].join(" "))
        );
    }

    #[test]
    fn snippet_with_no_space_in_the_first_159_takes_all_159_and_stays_within_160() {
        let long = "x".repeat(300);
        let snippet = make_snippet(&long);
        assert_eq!(snippet, format!("{}\u{2026}", "x".repeat(159)));
        assert_eq!(snippet.chars().count(), 160);

        // Supplementary letters count as one code point each.
        let long = "\u{10000}".repeat(200);
        let snippet = make_snippet(&long);
        assert_eq!(snippet, format!("{}\u{2026}", "\u{10000}".repeat(159)));
        assert_eq!(snippet.chars().count(), 160);
    }

    #[test]
    fn natural_id_order_compares_folded_runs_of_digits_and_non_digits() {
        use std::cmp::Ordering::{Equal, Greater, Less};

        // Numeric value, not text: v2 before v10, 3 before 027.
        assert_eq!(natural_id_cmp("S-0042-v2", "S-0042-v10"), Less);
        assert_eq!(natural_id_cmp("Q-3", "Q-027"), Less);
        // Folded: case does not decide.
        assert_eq!(natural_id_cmp("s-0042-v2", "S-0042-V10"), Less);
        assert_eq!(natural_id_cmp("XC-2", "xc-14"), Less);
        // Equal value, shorter first.
        assert_eq!(natural_id_cmp("a7", "a007"), Less);
        assert_eq!(natural_id_cmp("a007", "a8"), Less);
        // A digit run sorts before a non-digit run.
        assert_eq!(natural_id_cmp("1a", "aa"), Less);
        assert_eq!(natural_id_cmp("aa", "1a"), Greater);
        // A shorter sequence that starts the same way sorts first.
        assert_eq!(natural_id_cmp("a", "a1"), Less);
        assert_eq!(
            natural_id_cmp("cmp-withholding", "cmp-withholding#the-rule"),
            Less
        );
        assert_eq!(natural_id_cmp("S-0042", "S-0042-v1"), Less);
        // Non-digit runs compare in UTF-16 order: U+E000 is after the surrogate pair of U+10000,
        // although it is before U+10000 by code point.
        assert_eq!(utf16_cmp("\u{e000}", "\u{10000}"), Greater);
        assert_eq!(natural_id_cmp("\u{e000}", "\u{10000}"), Greater);
        assert_eq!(natural_id_cmp("id1", "id1"), Equal);
        // Both sides are folded: a compatibility hyphen equals `-` on either side.
        assert_eq!(natural_id_cmp("a-14", "a\u{2011}2"), Greater);
        assert_eq!(natural_id_cmp("a\u{2011}2", "a-14"), Less);
        assert_eq!(natural_id_cmp("a\u{2011}14", "a-2"), Greater);

        let mut ids = [
            "S-0042-v10",
            "XC-2",
            "cmp-withholding#the-rule",
            "S-0042-v2",
            "Q-027",
            "xc-14",
            "CL-CMP-0011",
            "S-0042-v1",
            "cmp-withholding",
            "Q-3",
        ];
        ids.sort_by(|a, b| natural_id_cmp(a, b));
        assert_eq!(
            ids,
            [
                "CL-CMP-0011",
                "cmp-withholding",
                "cmp-withholding#the-rule",
                "Q-3",
                "Q-027",
                "S-0042-v1",
                "S-0042-v2",
                "S-0042-v10",
                "XC-2",
                "xc-14",
            ]
        );
    }

    #[test]
    fn unit_href_formats_claim_links_to_part_pages() {
        for (id, expected) in [
            ("CL-CMP-0001", "claims/CMP-1.html#CL-CMP-0001"),
            ("CL-CMP-0011", "claims/CMP-1.html#CL-CMP-0011"),
            ("CL-CMP-0500", "claims/CMP-1.html#CL-CMP-0500"),
            ("CL-CMP-0501", "claims/CMP-2.html#CL-CMP-0501"),
            ("CL-CMP-1000", "claims/CMP-2.html#CL-CMP-1000"),
            ("CL-CMP-1001", "claims/CMP-3.html#CL-CMP-1001"),
            ("CL-CMP-9485", "claims/CMP-19.html#CL-CMP-9485"),
            ("CL-EST-0042", "claims/EST-1.html#CL-EST-0042"),
            ("CL-CMP-10000", "claims/CMP-20.html#CL-CMP-10000"),
        ] {
            assert_eq!(unit_href("claim", id), Some(expected.to_owned()));
        }
        assert_eq!(unit_href("claim", "not-a-claim"), None);
    }

    fn display(
        title: &str,
        parent: &str,
        snippet: &str,
        status: &str,
        as_of: &str,
        effective_from: &str,
        verified_on: &str,
    ) -> UnitDisplay {
        UnitDisplay {
            title: title.to_owned(),
            parent: parent.to_owned(),
            snippet: snippet.to_owned(),
            status: status.to_owned(),
            as_of: as_of.to_owned(),
            effective_from: effective_from.to_owned(),
            verified_on: verified_on.to_owned(),
        }
    }

    #[test]
    fn source_version_unit_follows_the_display_table() {
        let FixtureRows { mut sources, .. } = rows();
        let v2 = sources
            .iter_mut()
            .find(|s| s.source_id.as_str() == "S-0002" && s.version == 2)
            .expect("S-0002 v2");

        let unit = Unit::from_source_row(v2, &[]).expect("active version has a unit");
        assert_eq!(
            (unit.id.as_str(), unit.kind.as_str()),
            ("S-0002-v2", "source")
        );
        assert_eq!(
            unit.display,
            display(
                "Synthetic Pricing Guide v2, made up for tests",
                "",
                "Synthetic fixture second version superseding S-0002-v1.",
                "active",
                "2026-02-01",
                "2026-01-20",
                "",
            )
        );
        // Fields: the version id, the title, and publisher, tag and note in the body.
        assert_eq!(unit.fields.id, words("S-0002-v2"));
        assert_eq!(
            unit.fields.title,
            words("Synthetic Pricing Guide v2, made up for tests")
        );
        assert!(unit.fields.headings.is_empty() && unit.fields.claim.is_empty());
        assert_eq!(
            unit.fields.body,
            words(
                "Synthetic Pricing Institute synthetic-pricing-guide Synthetic fixture second version superseding S-0002-v1."
            )
        );

        // The snippet is the note, else the publisher.
        v2.note = None;
        let no_note = Unit::from_source_row(v2, &[]).expect("unit");
        assert_eq!(no_note.display.snippet, "Synthetic Pricing Institute");

        // A withdrawn version is still a unit and shows its lifecycle.
        v2.lifecycle = Lifecycle::Withdrawn;
        let withdrawn = Unit::from_source_row(v2, &[]).expect("withdrawn version is a unit");
        assert_eq!(withdrawn.display.status, "withdrawn");
        v2.lifecycle = Lifecycle::Quarantined;
        assert!(Unit::from_source_row(v2, &[]).is_none());
    }

    #[test]
    fn partial_dates_are_stored_as_written() {
        let line = FIX_SOURCES
            .lines()
            .find(|l| l.starts_with("S-0003,1,"))
            .expect("S-0003 row");
        let partial = line.replacen(",2026-01-25,,2026-02-01,", ",2026-01-25,2025-11,2025,", 1);
        assert_ne!(partial, line, "fixture columns moved");
        let sources_csv = FIX_SOURCES.replace(line, &partial);
        let FixtureRows { sources, .. } = fixture_rows(&sources_csv, FIX_GLOSSARY);
        let row = sources
            .iter()
            .find(|s| s.source_id.as_str() == "S-0003")
            .expect("S-0003");
        let unit = Unit::from_source_row(row, &[]).expect("unit");
        assert_eq!(unit.display.as_of, "2025");
        assert_eq!(unit.display.effective_from, "2025-11");
    }

    #[test]
    fn claim_unit_follows_the_display_table() {
        let FixtureRows { claims, .. } = rows();
        let claim = |id: &str| {
            claims
                .iter()
                .find(|c| c.claim_id.as_str() == id)
                .unwrap_or_else(|| panic!("{id}"))
        };

        let unit = Unit::from_claim_row(claim("CL-CMP-0001"), &[]);
        assert_eq!(
            (unit.id.as_str(), unit.kind.as_str()),
            ("CL-CMP-0001", "claim")
        );
        assert_eq!(
            unit.display,
            display(
                "CL-CMP-0001",
                "",
                "Synthetic compliance reviews occur every 30 days under the synthetic rule.",
                "primary_source_supported",
                "2026-02-01",
                "2026-01-15",
                "2026-02-10",
            )
        );
        assert_eq!(unit.fields.id, words("CL-CMP-0001"));
        assert!(unit.fields.title.is_empty() && unit.fields.headings.is_empty());
        assert_eq!(
            unit.fields.claim,
            words("Synthetic compliance reviews occur every 30 days under the synthetic rule.")
        );
        assert_eq!(
            unit.fields.body,
            words(
                "Synthetic compliance reviews occur every 30 days. Synthetic owner-verified fact for fixture tests."
            )
        );

        // A claim with no effective date and no verification shows empty strings.
        let bare = Unit::from_claim_row(claim("CL-EST-0001"), &[]);
        assert_eq!(
            bare.display,
            display(
                "CL-EST-0001",
                "",
                "Synthetic escalation placeholder is 3 percent per year for fixture examples.",
                "unchecked",
                "2026-02-01",
                "",
                "",
            )
        );
    }

    #[test]
    fn question_unit_follows_the_display_table() {
        let FixtureRows { questions, .. } = rows();
        let q1 = Unit::from_question_row(&questions[0], &[]);
        assert_eq!((q1.id.as_str(), q1.kind.as_str()), ("Q-001", "question"));
        assert_eq!(
            q1.display,
            display(
                "Which synthetic business-system control owner approves the monthly compliance review?",
                "",
                "Synthetic withholding exposure depends on timely review sign-off.",
                "open",
                "",
                "",
                "",
            )
        );
        assert_eq!(
            q1.fields.title,
            words(
                "Which synthetic business-system control owner approves the monthly compliance review?"
            )
        );
        assert_eq!(
            q1.fields.body,
            words(
                "Synthetic withholding exposure depends on timely review sign-off. Synthetic reviews follow the 30-day cycle until confirmed."
            )
        );
        assert_eq!(q1.fields.id, words("Q-001"));
        assert!(q1.fields.claim.is_empty() && q1.fields.headings.is_empty());

        // The answer is part of the body.
        let mut answered = questions[0].clone();
        answered.answer = Some("Finance approves it.".to_owned());
        let answered = Unit::from_question_row(&answered, &[]);
        assert!(answered.fields.body.contains(&"finance".to_owned()));
        assert!(answered.fields.body.contains(&"approves".to_owned()));

        // No `why_it_matters` gives an empty snippet.
        assert_eq!(
            Unit::from_question_row(&questions[1], &[]).display.snippet,
            ""
        );
    }

    #[test]
    fn conflict_unit_follows_the_display_table() {
        let FixtureRows { mut conflicts, .. } = rows();
        let xc = &mut conflicts[0];

        let unit = Unit::from_conflict_row(xc, &[]);
        assert_eq!(
            (unit.id.as_str(), unit.kind.as_str()),
            ("XC-01", "conflict")
        );
        // No resolution: the snippet is the positions joined with a space.
        assert_eq!(
            unit.display,
            display(
                "Synthetic review interval versus withholding check timing",
                "",
                "Synthetic review runs every 30 days Synthetic withholding check runs at each billing cycle",
                "unresolved",
                "",
                "",
                "",
            )
        );
        assert_eq!(
            unit.fields.title,
            words("Synthetic review interval versus withholding check timing")
        );
        assert_eq!(
            unit.fields.body,
            words(
                "Synthetic review runs every 30 days Synthetic withholding check runs at each billing cycle"
            )
        );

        // With a resolution the snippet is the resolution, and the rationale joins the body.
        xc.resolution = Some("Use the 30-day cycle.".to_owned());
        xc.rationale = Some("Rule text controls.".to_owned());
        let resolved = Unit::from_conflict_row(xc, &[]);
        assert_eq!(resolved.display.snippet, "Use the 30-day cycle.");
        assert!(resolved.fields.body.contains(&"controls".to_owned()));
        // Only the resolution holds `use`.
        assert!(resolved.fields.body.contains(&"use".to_owned()));
        assert_eq!(resolved.fields.id, words("XC-01"));
    }

    #[test]
    fn term_unit_follows_the_display_table() {
        let FixtureRows { terms, .. } = rows();
        let term = terms
            .iter()
            .find(|t| t.term_id.as_str() == "billing_withhold")
            .expect("billing_withhold");
        let unit = Unit::from_glossary_row(term, &[]);
        assert_eq!(
            (unit.id.as_str(), unit.kind.as_str()),
            ("billing_withhold", "term")
        );
        assert_eq!(
            unit.display,
            display(
                "billing withhold",
                "",
                "Synthetic retention of a percentage of interim payments pending system correction.",
                "public_reference",
                "",
                "",
                "",
            )
        );
        assert_eq!(unit.fields.id, words("billing_withhold"));
        assert_eq!(unit.fields.title, words("billing withhold"));
        // Aliases are the headings field; the definition and boundary rule are the body.
        assert_eq!(unit.fields.headings, words("withholding holdback"));
        // Several aliases are joined with a space.
        let mut multi = term.clone();
        multi.aliases = vec!["withholding holdback".to_owned(), "retainage".to_owned()];
        assert_eq!(
            Unit::from_glossary_row(&multi, &[]).fields.headings,
            words("withholding holdback retainage")
        );
        assert_eq!(
            unit.fields.body,
            words(
                "Synthetic retention of a percentage of interim payments pending system correction. Do not confuse with synthetic liquidated damages or final price adjustments."
            )
        );
        assert!(unit.fields.claim.is_empty());
    }

    fn page_model(pages: &[(&str, &str)]) -> Vec<PageEntry> {
        use crate::check::{Host, Options, Tree, load};
        use crate::date::CivilDate;

        let mut tree = Tree::new();
        for dir in [
            "corpus",
            "corpus/registers",
            "corpus/pages",
            "corpus/pages/compliance",
            PAGE_DIR,
        ] {
            tree.insert_dir(dir);
        }
        tree.insert_bytes("corpus/corpus.toml", FIX_CORPUS_TOML.as_bytes());
        for (stem, text) in pages {
            tree.insert_bytes(format!("{PAGE_DIR}/{stem}.md"), text.as_bytes());
        }
        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");
        let (model, _report) = load(&Host::new(tree, today), &Options::new(false));
        model.pages().to_vec()
    }

    #[test]
    fn page_units_follow_the_display_table() {
        let pages = page_model(&[
            ("cmp-system-review", FIX_PAGE_REVIEW),
            ("cmp-clause-9900-101", FIX_PAGE_CLAUSE),
        ]);
        assert_eq!(pages.len(), 2);
        let page_title = "Synthetic business-system review and withholding overview";

        let review = pages
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .expect("page");
        let units = Unit::from_page_entry(review, &[]);
        let lead = &units[0];
        assert_eq!(
            (lead.id.as_str(), lead.kind.as_str()),
            ("cmp-system-review", "page")
        );
        // Lead: the page title, no parent, the page status and as_of, and the first paragraph of
        // the first section as the snippet; no effective or verified date.
        assert_eq!(
            lead.display,
            display(
                page_title,
                "",
                "Explain how a synthetic system review interacts with a synthetic billing withhold.",
                "draft",
                "2026-02-01",
                "",
                "",
            )
        );

        // Section: the heading as title, the page title as parent, the page's status and as_of,
        // and the first paragraph of the section as the snippet (cut at 160 code points).
        let rule = units
            .iter()
            .find(|u| u.id == "cmp-system-review#the-rule")
            .expect("section");
        assert_eq!(rule.kind, "page");
        assert_eq!(
            rule.display,
            display(
                crate::strings::parts()[1],
                page_title,
                "A synthetic system review runs on a recurring cycle of 30 days [@CL-CMP-0001]. When a synthetic deficiency remains open, the synthetic billing withhold rate\u{2026}",
                "draft",
                "2026-02-01",
                "",
                "",
            )
        );
        assert!(units.iter().skip(1).all(|u| u.display.parent == page_title));
        assert!(units.iter().skip(1).all(|u| u.fields.id.is_empty()));

        // A clause-card page has the kind `clause` on its lead and its sections.
        let clause = pages
            .iter()
            .find(|p| p.id.as_str() == "cmp-clause-9900-101")
            .expect("clause page");
        let clause_units = Unit::from_page_entry(clause, &[]);
        assert!(clause_units.iter().all(|u| u.kind == "clause"));
        assert_eq!(
            clause_units[0].display.snippet,
            "Summarize the synthetic clause governing recurring business-system reviews."
        );
    }

    #[test]
    fn handling_cells_fail_closed_to_unlabelled() {
        let cell = |items: &[&str]| {
            parse_handling_slice(&items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
        };
        assert!(cell(&["public"]).is_public());
        assert!(cell(&["public", "public"]).is_public());
        // An empty cell and an unparsable label are not public: the unit is `unlabelled`.
        assert_eq!(cell(&[]), LabelSet::unlabelled());
        assert_eq!(cell(&["Not A Label"]), LabelSet::unlabelled());
        assert_eq!(cell(&["public", "Not A Label"]), LabelSet::unlabelled());
        assert_eq!(cell(&["xx-0001", "bad_id"]), LabelSet::unlabelled());
        // `public` beside another label is that label alone; the built-in unresolved labels stay.
        assert!(!cell(&["public", "xx-0001"]).is_public());
        assert!(!cell(&["xx-0001"]).is_public());
        assert!(!cell(&["marked"]).is_public());
        assert!(!cell(&["unlabelled"]).is_public());
        assert!(!cell(&["owner-supplied"]).is_public());
    }

    /// The ids of the units of `rows` and `pages` that `keep` accepts, with the glossary the same
    /// predicate keeps.
    fn kept_ids(
        rows: &FixtureRows,
        pages: &[PageEntry],
        keep: fn(&LabelSet) -> bool,
    ) -> BTreeSet<String> {
        let glossary = kept_glossary(&rows.terms, &rows.sources, &rows.claims, keep);
        build_corpus_units_with(
            pages,
            &rows.sources,
            &rows.claims,
            &rows.questions,
            &rows.conflicts,
            &rows.terms,
            &BTreeSet::new(),
            &glossary,
            keep,
        )
        .into_iter()
        .map(|u| u.id)
        .collect()
    }

    fn parsed<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        r.expect("test id parses")
    }

    /// `rows()` plus synthetic sources, claims, conflicts and terms that cover each way a label
    /// is resolved and each way it fails closed.
    fn labelled_rows() -> FixtureRows {
        use crate::ids::{ClaimId, ConflictId, SourceId, TermId};

        let mut r = rows();
        let base_source = r.sources[0].clone();
        let base_claim = r.claims[0].clone();
        let base_conflict = r.conflicts[0].clone();
        let base_term = r
            .terms
            .iter()
            .find(|t| t.term_id.as_str() == "cost_baseline")
            .expect("cost_baseline")
            .clone();

        // Sources. S-0004: one restricted version. S-0005: an empty handling cell. S-0006: an
        // unparsable label beside `public`. S-0007: `marked`. S-0008: `public` twice (public).
        // S-0009: version 1 public and version 2 restricted.
        let src = |id: &str, version: u32, handling: &[&str]| {
            let mut s = base_source.clone();
            s.source_id = parsed(SourceId::parse(id));
            s.version = version;
            s.handling = handling.iter().map(|h| (*h).to_owned()).collect();
            s.supersedes = Vec::new();
            s
        };
        r.sources.extend([
            src("S-0004", 1, &["xx-0001"]),
            src("S-0005", 1, &[]),
            src("S-0006", 1, &["public", "Not A Label"]),
            src("S-0007", 1, &["marked"]),
            src("S-0008", 1, &["public", "public"]),
            src("S-0009", 1, &["public"]),
            src("S-0009", 2, &["public", "xx-0001"]),
            src("S-0010", 1, &["xx-0001"]),
            src("S-0010", 2, &["public"]),
        ]);

        // Claims. `(source, version)` is the reference that resolves the claim's labels.
        let claim = |id: &str, source: Option<(&str, Option<u32>)>| {
            let mut c = base_claim.clone();
            c.claim_id = parsed(ClaimId::parse(id));
            c.source_id = source.map(|(s, _)| parsed(SourceId::parse(s)));
            c.version = source.and_then(|(_, v)| v);
            c
        };
        r.claims.extend([
            claim("CL-LBL-0001", Some(("S-0001", Some(1)))),
            claim("CL-LBL-0002", Some(("S-0004", Some(1)))),
            claim("CL-LBL-0003", Some(("S-0009", None))),
            claim("CL-LBL-0004", Some(("S-0003", None))),
            claim("CL-LBL-0005", None),
            claim("CL-LBL-0006", Some(("S-0099", Some(1)))),
            claim("CL-LBL-0007", Some(("S-0099", None))),
            claim("CL-LBL-0008", Some(("S-0005", Some(1)))),
            claim("CL-LBL-0009", Some(("S-0006", Some(1)))),
            claim("CL-LBL-0010", Some(("S-0007", Some(1)))),
            claim("CL-LBL-0011", Some(("S-0008", Some(1)))),
            claim("CL-LBL-0012", Some(("S-0009", Some(1)))),
            claim("CL-LBL-0013", Some(("S-0009", Some(2)))),
            claim("CL-LBL-0014", Some(("S-0010", None))),
        ]);

        // Conflicts.
        let claim_ref = |id: &str| RefId::Claim(parsed(ClaimId::parse(id)));
        let version_ref = |id: &str| RefId::SourceVersion(parsed(VersionId::parse(id)));
        let conflict = |id: &str, claim_ids: &[&str], evidence: Vec<RefId>| {
            let mut c = base_conflict.clone();
            c.conflict_id = parsed(ConflictId::parse(id));
            c.claim_ids = claim_ids
                .iter()
                .map(|c| parsed(ClaimId::parse(c)))
                .collect();
            c.evidence = evidence;
            c
        };
        r.conflicts.extend([
            conflict("XC-90", &["CL-CMP-0001"], vec![version_ref("S-0001-v1")]),
            conflict("XC-91", &["CL-LBL-0002"], vec![]),
            conflict("XC-92", &["CL-CMP-0001"], vec![claim_ref("CL-LBL-0002")]),
            conflict("XC-93", &["CL-CMP-0001"], vec![version_ref("S-0004-v1")]),
            conflict("XC-94", &["CL-CMP-0001"], vec![claim_ref("CL-LBL-0099")]),
            conflict("XC-95", &["CL-CMP-0001"], vec![version_ref("S-0099-v1")]),
            conflict("XC-96", &["CL-LBL-0098"], vec![]),
            conflict("XC-97", &["CL-CMP-0001", "CL-LBL-0002"], vec![]),
            conflict(
                "XC-98",
                &["CL-CMP-0001"],
                vec![claim_ref("CL-CMP-0002"), version_ref("S-0003-v1")],
            ),
            conflict("XC-99", &["CL-LBL-0002", "CL-CMP-0001"], vec![]),
            conflict(
                "XC-89",
                &["CL-CMP-0001"],
                vec![claim_ref("CL-LBL-0002"), claim_ref("CL-CMP-0001")],
            ),
        ]);

        // Terms.
        let term = |id: &str, refs: Vec<RefId>, alias: &str| {
            let mut t = base_term.clone();
            t.term_id = parsed(TermId::parse(id));
            t.label = id.replace('_', " ");
            t.source_ref = refs;
            t.aliases = vec![alias.to_owned()];
            t
        };
        r.terms.extend([
            term("t_public", vec![claim_ref("CL-CMP-0001")], "aliaspublic"),
            term(
                "t_restricted_claim",
                vec![claim_ref("CL-LBL-0002")],
                "aliasrestrictedclaim",
            ),
            term(
                "t_dangling_claim",
                vec![claim_ref("CL-LBL-0099")],
                "aliasdanglingclaim",
            ),
            term(
                "t_restricted_version",
                vec![version_ref("S-0004-v1")],
                "aliasrestrictedversion",
            ),
            term(
                "t_dangling_version",
                vec![version_ref("S-0099-v1")],
                "aliasdanglingversion",
            ),
            term(
                "t_mixed",
                vec![claim_ref("CL-CMP-0001"), claim_ref("CL-LBL-0002")],
                "aliasmixed",
            ),
            term(
                "t_mixed_reversed",
                vec![claim_ref("CL-LBL-0002"), claim_ref("CL-CMP-0001")],
                "aliasmixedreversed",
            ),
        ]);
        r
    }

    #[test]
    fn the_public_ceiling_drops_every_restricted_or_unresolved_unit_and_keeps_the_rest() {
        let r = labelled_rows();
        let public = kept_ids(&r, &[], LabelSet::is_public);
        let open = kept_ids(&r, &[], |_| true);

        let held_back = [
            // A source whose handling is restricted, empty, unparsable beside `public`, or marked.
            "S-0004-v1",
            "S-0005-v1",
            "S-0006-v1",
            "S-0007-v1",
            "S-0009-v2",
            "S-0010-v1",
            // A claim whose source version is restricted, dangling, empty, unparsable or marked.
            "CL-LBL-0002",
            "CL-LBL-0006",
            "CL-LBL-0008",
            "CL-LBL-0009",
            "CL-LBL-0010",
            "CL-LBL-0013",
            // A claim that names a source with no version takes the union over its versions
            // (one restricted version is enough) and a source the register does not hold fails
            // closed.
            "CL-LBL-0003",
            "CL-LBL-0007",
            "CL-LBL-0014",
            // A conflict with a restricted or dangling claim or evidence reference, or any one
            // restricted reference among several.
            "XC-91",
            "XC-92",
            "XC-93",
            "XC-94",
            "XC-95",
            "XC-96",
            "XC-97",
            "XC-99",
            "XC-89",
            // A term whose source reference is restricted, dangling or mixed.
            "t_restricted_claim",
            "t_dangling_claim",
            "t_restricted_version",
            "t_dangling_version",
            "t_mixed",
            "t_mixed_reversed",
        ];
        let kept = [
            "S-0001-v1",
            "S-0008-v1",
            "S-0009-v1",
            "S-0010-v2",
            "CL-CMP-0001",
            "CL-LBL-0001",
            // Names a source with no version and every version is public.
            "CL-LBL-0004",
            // Names no source: nothing to restrict it.
            "CL-LBL-0005",
            "CL-LBL-0011",
            "CL-LBL-0012",
            "XC-90",
            "XC-98",
            "t_public",
            "cost_baseline",
            // A question has no source reference and no label inputs.
            "Q-001",
        ];
        for id in held_back {
            assert!(!public.contains(id), "{id} must not be in a public index");
            assert!(
                open.contains(id),
                "{id} must exist when every label is kept"
            );
        }
        for id in kept {
            assert!(public.contains(id), "{id} must stay public: {public:?}");
        }
        // No predicate that accepts everything loses a unit, and the ceiling adds none.
        assert!(public.is_subset(&open));
        // A predicate that rejects everything leaves nothing, whatever the kind.
        assert!(kept_ids(&r, &[], |_| false).is_empty());
    }

    #[test]
    fn a_restricted_or_dangling_glossary_row_adds_no_synonyms_to_a_public_index() {
        let r = labelled_rows();
        let public = kept_glossary(&r.terms, &r.sources, &r.claims, LabelSet::is_public);
        let aliases = |g: &Glossary| -> BTreeSet<String> {
            g.entries()
                .iter()
                .flat_map(|e| e.aliases.iter().cloned())
                .collect()
        };
        let public_aliases = aliases(&public);
        assert!(public_aliases.contains("aliaspublic"), "{public_aliases:?}");
        for held in [
            "aliasrestrictedclaim",
            "aliasdanglingclaim",
            "aliasrestrictedversion",
            "aliasdanglingversion",
            "aliasmixed",
            "aliasmixedreversed",
        ] {
            assert!(
                !public_aliases.contains(held),
                "{held} must not reach the public glossary"
            );
        }
        let open = kept_glossary(&r.terms, &r.sources, &r.claims, |_| true);
        assert!(aliases(&open).contains("aliasrestrictedclaim"));
        assert!(aliases(&open).contains("aliasdanglingversion"));
    }

    #[test]
    fn page_units_carry_the_page_handling_into_the_ceiling() {
        let loaded = page_model(&[("cmp-system-review", FIX_PAGE_REVIEW)]);
        let public_page = loaded[0].clone();
        let with_handling = |id: &str, handling: &[&str]| {
            let mut p = public_page.clone();
            p.id = parsed(PageId::parse(id));
            p.meta.handling = handling.iter().map(|h| (*h).to_owned()).collect();
            p
        };
        let pages = vec![
            public_page.clone(),
            with_handling("cmp-restricted-page", &["xx-0001"]),
            with_handling("cmp-mixed-page", &["public", "xx-0001"]),
            with_handling("cmp-empty-page", &[]),
            with_handling("cmp-bad-page", &["Not A Label"]),
        ];

        let r = rows();
        let public = kept_ids(&r, &pages, LabelSet::is_public);
        let open = kept_ids(&r, &pages, |_| true);
        for id in ["cmp-system-review", "cmp-system-review#the-rule"] {
            assert!(public.contains(id), "{id} is a public page unit");
        }
        // Every unit of a restricted page is left out, its sections as well as its lead.
        for prefix in [
            "cmp-restricted-page",
            "cmp-mixed-page",
            "cmp-empty-page",
            "cmp-bad-page",
        ] {
            assert!(
                !public.iter().any(|id| id.starts_with(prefix)),
                "{prefix} units must not be public: {public:?}"
            );
            assert!(open.contains(prefix));
            assert!(open.contains(&format!("{prefix}#the-rule")));
        }
        // A section unit carries the labels of its page.
        for unit in Unit::from_page_entry(&pages[1], &[]) {
            assert_eq!(labels_of(&unit).to_cell(), "xx-0001");
        }
        for unit in Unit::from_page_entry(&pages[3], &[]) {
            assert_eq!(*labels_of(&unit), LabelSet::unlabelled());
        }
    }

    #[test]
    fn corpus_units_come_out_in_natural_id_order_and_skip_quarantined_versions() {
        let mut r = labelled_rows();
        let glossary = Glossary::default();
        let build = |r: &FixtureRows, quarantined: &BTreeSet<VersionId>| -> Vec<String> {
            build_corpus_units_with(
                &[],
                &r.sources,
                &r.claims,
                &r.questions,
                &r.conflicts,
                &r.terms,
                quarantined,
                &glossary,
                |_| true,
            )
            .into_iter()
            .map(|u| u.id)
            .collect()
        };

        let all = build(&r, &BTreeSet::new());
        let mut sorted = all.clone();
        sorted.sort_by(|a, b| natural_id_cmp(a, b));
        assert_eq!(all, sorted, "units are numbered in natural id order");
        assert!(all.iter().any(|id| id == "S-0002-v2") && all.iter().any(|id| id == "S-0004-v1"));

        // A version named in the quarantined set gives no unit, whatever its row says.
        let named: BTreeSet<VersionId> = ["S-0002-v2", "S-0004-v1"]
            .iter()
            .map(|v| parsed(VersionId::parse(v)))
            .collect();
        let held = build(&r, &named);
        assert!(!held.iter().any(|id| id == "S-0002-v2" || id == "S-0004-v1"));
        assert!(held.iter().any(|id| id == "S-0002-v1"));
        assert_eq!(held.len(), all.len() - 2);

        // A row whose own lifecycle is `quarantined` gives no unit either.
        let row = r
            .sources
            .iter_mut()
            .find(|s| s.source_id.as_str() == "S-0001")
            .expect("S-0001");
        row.lifecycle = Lifecycle::Quarantined;
        let by_row = build(&r, &BTreeSet::new());
        assert!(!by_row.iter().any(|id| id == "S-0001-v1"));
        assert_eq!(by_row.len(), all.len() - 1);
    }

    #[test]
    fn the_shipped_corpus_builder_applies_the_label_predicate_to_pages_and_registers() {
        use crate::check::Tree;

        let mut tree = Tree::new();
        for dir in [
            "corpus",
            "corpus/registers",
            "corpus/sources",
            "corpus/sources/public",
            "corpus/pages",
            "corpus/pages/compliance",
            PAGE_DIR,
        ] {
            tree.insert_dir(dir);
        }
        tree.insert_bytes("corpus/corpus.toml", FIX_CORPUS_TOML.as_bytes());
        for (name, text) in [
            ("sources", FIX_SOURCES),
            ("claims", FIX_CLAIMS),
            ("questions", FIX_QUESTIONS),
            ("conflicts", FIX_CONFLICTS),
            ("glossary", FIX_GLOSSARY),
        ] {
            tree.insert_bytes(format!("corpus/registers/{name}.csv"), text.as_bytes());
        }
        tree.insert_bytes(
            "corpus/sources/public/S-0002-v2.txt",
            FIX_SOURCE_TEXT.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/sources/public/S-0001-v1.txt",
            FIX_SOURCE_TEXT_S0001.as_bytes(),
        );
        tree.insert_bytes(
            format!("{PAGE_DIR}/cmp-system-review.md"),
            FIX_PAGE_REVIEW.as_bytes(),
        );
        let none = BTreeSet::new();

        let (public, glossary) = build_shipped_corpus_units(&tree, &none, LabelSet::is_public)
            .expect("shipped units build");
        let public_ids: Vec<&str> = public.iter().map(|u| u.id.as_str()).collect();
        assert!(public_ids.contains(&"cmp-system-review"), "{public_ids:?}");
        assert!(public_ids.contains(&"cmp-system-review#the-rule"));
        assert!(public_ids.contains(&"CL-CMP-0001") && public_ids.contains(&"billing_withhold"));
        assert!(public.iter().all(|u| u.labels.is_public()));
        assert!(!glossary.entries().is_empty());
        // The result is in natural id order, pages among the register units.
        let mut sorted = public_ids.clone();
        sorted.sort_by(|a, b| natural_id_cmp(a, b));
        assert_eq!(public_ids, sorted);
        // It is what the slice builder makes of the same rows and page.
        let r = rows();
        let pages = page_model(&[("cmp-system-review", FIX_PAGE_REVIEW)]);
        let expected = kept_ids(&r, &pages, LabelSet::is_public);
        assert_eq!(
            public_ids
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<BTreeSet<_>>(),
            expected
        );

        // The predicate is applied to every page unit and every register unit, and to the rows
        // whose aliases become synonyms: one that accepts nothing leaves nothing.
        let (nothing, glossary) =
            build_shipped_corpus_units(&tree, &none, |_| false).expect("shipped units build");
        assert!(nothing.is_empty(), "{nothing:?}");
        assert!(glossary.entries().is_empty());

        // The named quarantined version gives no unit.
        let named: BTreeSet<VersionId> = [parsed(VersionId::parse("S-0002-v2"))].into();
        let (held, _) = build_shipped_corpus_units(&tree, &named, LabelSet::is_public)
            .expect("shipped units build");
        assert!(!held.iter().any(|u| u.id == "S-0002-v2"));
        assert!(held.iter().any(|u| u.id == "S-0002-v1"));

        // The shipped manifest refuses a page whose handling is not exactly `public` before any
        // unit exists, so a restricted page cannot reach this builder at all.
        let restricted = FIX_PAGE_REVIEW
            .replace("id = \"cmp-system-review\"", "id = \"cmp-restricted-page\"")
            .replace("handling = [\"public\"]", "handling = [\"xx-0001\"]");
        assert_ne!(
            restricted, FIX_PAGE_REVIEW,
            "fixture page front matter moved"
        );
        tree.insert_bytes(
            format!("{PAGE_DIR}/cmp-restricted-page.md"),
            restricted.as_bytes(),
        );
        assert!(matches!(
            build_shipped_corpus_units(&tree, &none, |_| true),
            Err(ManifestError::NonPublicFile { .. })
        ));
    }
}

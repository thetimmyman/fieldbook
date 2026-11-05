//! Search document units and the [`UNIT_KINDS`] table (`docs/reference/search.md#document-units`).
//!
//! Defines [`Unit`], [`UnitFields`], [`UnitKind`] and [`UNIT_KINDS`], and builds the five token
//! fields (`id`, `title`, `headings`, `claim`, `body`) of Corpus document units with synonym
//! expansion applied.

use crate::check::PageEntry;
use crate::check::markdown::{self, Ev, Event, HeadingLevel, Tag, TagEnd};
use crate::check::page_parts;
use crate::schema::vocab::{Lifecycle, PageType};
use crate::schema::{ClaimRow, ConflictRow, GlossaryRow, QuestionRow, SourceRow};

use super::lexer::index_tokens;
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
        link: Some("claims.html#CL-CMP-0011"),
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
        "claim" => Some(format!("claims.html#{id}")),
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
}

/// One searchable document unit with its identifier, kind, and five token fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// The unit identifier as written (`CL-CMP-0011`, `cmp-withholding#the-rule`).
    pub id: String,
    /// The unit kind from [`UNIT_KINDS`] (`page`, `clause`, `source`, `claim`, `question`,
    /// `conflict`, `term`, `record`, `text`).
    pub kind: String,
    /// The five final token fields (after index-side extras and synonym expansion).
    pub fields: UnitFields,
}

impl Unit {
    /// Creates a [`Unit`] with its identifier, kind, and tokenized [`UnitFields`].
    #[must_use]
    pub fn new(id: impl Into<String>, kind: impl Into<String>, fields: UnitFields) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            fields,
        }
    }

    /// Builds the page-lead [`Unit`] and each `##` page-section [`Unit`] for a parsed [`PageEntry`].
    #[must_use]
    pub fn from_page_entry(page: &PageEntry, sets: &[SynonymSet]) -> Vec<Self> {
        let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
        build_page_units(
            page.id.as_str(),
            &page.meta.title,
            page.meta.r#type,
            &page.body,
            base_line,
            sets,
        )
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
        Some(Self::new(unit_id, "source", fields))
    }

    /// Builds a `claim` [`Unit`] from a [`ClaimRow`].
    #[must_use]
    pub fn from_claim_row(row: &ClaimRow, sets: &[SynonymSet]) -> Self {
        let id = row.claim_id.as_str();
        let body = join_optional_parts(&[row.quote.as_deref(), row.note.as_deref()]);
        let fields = UnitFields::from_raw(id, "", "", &row.statement, &body, sets);
        Self::new(id, "claim", fields)
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
        Self::new(id, "question", fields)
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
        Self::new(id, "conflict", fields)
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
        Self::new(id, "term", fields)
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

    let mut units = Vec::with_capacity(1 + parts.parts().len());
    units.push(Unit::new(
        page_id,
        kind,
        UnitFields::from_raw(page_id, page_title, "", "", &lead_body_text, sets),
    ));

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
        // Section units have an empty `id` field.
        units.push(Unit::new(
            section_id,
            kind,
            UnitFields::from_raw("", "", &headings_text, "", &section_body, sets),
        ));
    }

    units
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

/// Builds all Corpus document units in the order of the Document units table in
/// `docs/reference/search.md#document-units`, applying synonyms from `glossary`.
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
    let sets = glossary.synonym_sets();
    let mut out = Vec::new();
    for page in pages {
        out.extend(Unit::from_page_entry(page, &sets));
    }
    for source in sources {
        if let Some(u) = Unit::from_source_row(source, &sets) {
            out.push(u);
        }
    }
    for claim in claims {
        out.push(Unit::from_claim_row(claim, &sets));
    }
    for question in questions {
        out.push(Unit::from_question_row(question, &sets));
    }
    for conflict in conflicts {
        out.push(Unit::from_conflict_row(conflict, &sets));
    }
    for term in terms {
        out.push(Unit::from_glossary_row(term, &sets));
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

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
}

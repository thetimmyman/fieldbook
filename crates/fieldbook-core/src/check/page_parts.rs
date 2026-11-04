//! The `check-page-parts` pass: `FB0130` to `FB0133`, `FB0137`, and `FB0114` on Change note dates.
//!
//! Splits each page body into its level-2 (`##`) [`Parts`], computes deterministic level-2/3
//! [`heading_anchors`], and validates part headings, blank parts, `## Sources`, `## Worked example`
//! synthetic labels, and `## Change note` entries.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::date::CivilDate;
use crate::schema::vocab::{ChangeClass, PageType};
use crate::strings;

use super::markdown::{self, Ev, Event, HeadingLevel, Tag, TagEnd};
use super::{Code, Diagnostic, Model, Options, PageEntry, Report};

/// The required first paragraph of a `## Worked example` part that has content (`FB0133`).
pub const SYNTHETIC_LABEL: &str = "Synthetic example. The numbers are made up.";

/// One level-2 (`##`) section of a page body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// Visible text of the level-2 heading.
    pub heading: String,
    /// 1-based line number of the `##` heading in the page file.
    pub line: usize,
    /// 1-based code-point column of the `##` heading on `line`.
    pub col: usize,
    /// Index of the `Event::Start(Tag::Heading { level: H2, .. })` event in
    /// [`markdown::events`].
    pub heading_event: usize,
    /// Event index range in [`markdown::events`] after the closing `Heading(H2)` event and up to
    /// the next level-2 heading (or the end of the event stream).
    pub event_range: Range<usize>,
    /// 1-based line number in the page file where this part ends (inclusive).
    pub end_line: usize,
}

/// A heading recorded in document order while reading a page body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingRecord {
    /// Heading level (`1` to `6`).
    pub level: u8,
    /// Visible text of the heading (`Text` and `Code` events).
    pub text: String,
    /// 1-based line number in the page file.
    pub line: usize,
    /// 1-based code-point column on `line`.
    pub col: usize,
    /// Index of the `Event::Start(Tag::Heading { .. })` event in [`markdown::events`].
    pub event_index: usize,
}

/// Parsed level-2 parts and level-2/3 heading anchors of a page body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parts {
    parts: Vec<Part>,
    headings: Vec<HeadingRecord>,
    heading_anchors: BTreeMap<usize, String>,
    event_count: usize,
    body_start_line: usize,
    body_end_line: usize,
}

impl Parts {
    /// The level-2 (`##`) parts of the page body, in document order.
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// Find the first level-2 part whose heading equals `heading`.
    #[must_use]
    pub fn get(&self, heading: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.heading == heading)
    }

    /// Every heading (`H1` to `H6`) in the page body, in document order.
    #[must_use]
    pub fn headings(&self) -> &[HeadingRecord] {
        &self.headings
    }

    /// Level-2 and level-3 heading anchors keyed by their opening `Event::Start(Tag::Heading)`
    /// index in [`markdown::events`].
    #[must_use]
    pub const fn heading_anchors(&self) -> &BTreeMap<usize, String> {
        &self.heading_anchors
    }

    /// Look up the heading anchor for the heading starting at `event_pos`.
    #[must_use]
    pub fn heading_anchor(&self, event_pos: usize) -> Option<&str> {
        self.heading_anchors.get(&event_pos).map(String::as_str)
    }

    /// Total number of events produced by [`markdown::events`] for this page body.
    #[must_use]
    pub const fn event_count(&self) -> usize {
        self.event_count
    }

    /// 1-based line number in the page file where the body starts.
    #[must_use]
    pub const fn body_start_line(&self) -> usize {
        self.body_start_line
    }

    /// 1-based line number in the page file where the body ends.
    #[must_use]
    pub const fn body_end_line(&self) -> usize {
        self.body_end_line
    }
}

/// Assign unique, deterministic anchors in document order to every level-2 and level-3 heading in
/// `events`, keyed by the opening heading event index (`docs/reference/search.md#page-heading-anchors`).
#[must_use]
pub fn heading_anchors(events: &[Ev<'_>]) -> BTreeMap<usize, String> {
    let mut anchors = BTreeMap::new();
    let mut used = BTreeSet::new();

    let mut idx = 0;
    while idx < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = events[idx].event {
            let start_idx = idx;
            let is_h2_or_h3 = matches!(level, HeadingLevel::H2 | HeadingLevel::H3);
            let mut text = String::new();
            idx += 1;
            while idx < events.len() {
                match &events[idx].event {
                    Event::End(TagEnd::Heading(_)) => break,
                    Event::Text(t) | Event::Code(t) => text.push_str(t),
                    _ => {}
                }
                idx += 1;
            }
            if is_h2_or_h3 {
                let base = slugify_heading(&text);
                let allocated = allocate_anchor(&base, &mut used);
                anchors.insert(start_idx, allocated);
            }
        }
        idx += 1;
    }

    anchors
}

fn slugify_heading(text: &str) -> String {
    let mut base = String::new();
    let mut pending_sep = false;
    for ch in text.to_lowercase().chars() {
        if ch.is_alphanumeric() {
            if pending_sep && !base.is_empty() {
                base.push('-');
            }
            pending_sep = false;
            base.push(ch);
        } else if ch.is_whitespace() || ch == '-' {
            pending_sep = true;
        }
    }
    if base.is_empty() {
        base.push_str("section");
    }
    base
}

fn allocate_anchor(base: &str, used: &mut BTreeSet<String>) -> String {
    if used.insert(base.to_owned()) {
        return base.to_owned();
    }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{base}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Split `body` (starting at 1-based `body_start_line` in the page file) into its level-2 [`Parts`]
/// and compute level-2/3 [`heading_anchors`].
#[must_use]
pub fn split_parts(body: &str, body_start_line: usize) -> Parts {
    let base_line = body_start_line.max(1);
    let newline_count = body.bytes().filter(|&b| b == b'\n').count();
    let max_line = base_line + newline_count;
    let body_end_line = if body.ends_with('\n') {
        base_line + newline_count.saturating_sub(1)
    } else {
        max_line
    };

    let evs = markdown::events(body, base_line);
    let anchors = heading_anchors(&evs);

    let mut headings = Vec::new();
    let mut parts: Vec<Part> = Vec::new();

    let mut idx = 0;
    while idx < evs.len() {
        if let Event::Start(Tag::Heading { level, .. }) = evs[idx].event {
            let start_idx = idx;
            let line = evs[idx].line.clamp(base_line, max_line);
            let col = evs[idx].col.max(1);
            let mut text = String::new();
            idx += 1;
            while idx < evs.len() {
                match &evs[idx].event {
                    Event::End(TagEnd::Heading(_)) => break,
                    Event::Text(t) | Event::Code(t) => text.push_str(t),
                    _ => {}
                }
                idx += 1;
            }
            let content_start = idx.saturating_add(1).min(evs.len());
            let level_num = match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                HeadingLevel::H4 => 4,
                HeadingLevel::H5 => 5,
                HeadingLevel::H6 => 6,
            };

            headings.push(HeadingRecord {
                level: level_num,
                text: text.clone(),
                line,
                col,
                event_index: start_idx,
            });

            if level == HeadingLevel::H2 {
                if let Some(prev) = parts.last_mut() {
                    let end_ev = start_idx.max(prev.event_range.start).min(evs.len());
                    prev.event_range.end = end_ev;
                    prev.end_line = line.saturating_sub(1).max(prev.line).min(max_line);
                }
                parts.push(Part {
                    heading: text,
                    line,
                    col,
                    heading_event: start_idx,
                    event_range: content_start..evs.len(),
                    end_line: body_end_line.max(line).min(max_line),
                });
            }
        }
        idx += 1;
    }

    Parts {
        parts,
        headings,
        heading_anchors: anchors,
        event_count: evs.len(),
        body_start_line: base_line,
        body_end_line,
    }
}

/// Run the `check-page-parts` pass over `model` and append diagnostics to `report`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    for page in model.pages() {
        check_page(page, options.strict, report);
    }
}

fn check_page(page: &PageEntry, strict: bool, report: &mut Report) {
    let parts = page.parts();
    let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
    let evs = markdown::events(&page.body, base_line);
    let lines = collect_body_lines(&page.body, base_line);

    if let Some((line, col, msg)) = first_heading_fault(parts, page.meta.r#type) {
        report.push(Diagnostic::new(
            Code::Fb0130,
            strict,
            &page.path,
            line,
            col,
            msg,
        ));
    }

    let sources_heading = strings::parts()[6];
    let worked_example_heading = strings::parts()[3];
    let change_note_heading = strings::parts()[7];

    for part in parts.parts() {
        let part_lines = lines_in_range(&lines, part.line + 1, part.end_line);
        if part.heading == sources_heading {
            // FB0132: `## Sources` holds text.
            if let Some(&(line_num, text)) =
                part_lines.iter().find(|(_, text)| !text.trim().is_empty())
            {
                let col = text.chars().take_while(|c| c.is_whitespace()).count() + 1;
                report.push(Diagnostic::new(
                    Code::Fb0132,
                    strict,
                    &page.path,
                    line_num,
                    col,
                    "`## Sources` must be the heading alone with no body text",
                ));
            }
            continue;
        }

        // FB0131: a part other than `Sources` is blank.
        let is_blank = part_lines.iter().all(|(_, text)| text.trim().is_empty());
        if is_blank {
            report.push(Diagnostic::new(
                Code::Fb0131,
                strict,
                &page.path,
                part.line,
                part.col,
                format!("part `## {}` is blank", part.heading),
            ));
            continue;
        }

        // FB0133: Worked example synthetic label check.
        if part.heading == worked_example_heading {
            let part_evs = &evs[part.event_range.clone()];
            if !is_single_paragraph_exact(part_evs, strings::empty_part())
                && !first_block_is_exact_paragraph(part_evs, SYNTHETIC_LABEL)
            {
                let (err_line, err_col) = part_evs.first().map_or_else(
                    || {
                        part_lines
                            .iter()
                            .find(|(_, t)| !t.trim().is_empty())
                            .map_or((part.line, part.col), |&(l, _)| (l, 1))
                    },
                    |ev| (ev.line, ev.col),
                );
                report.push(Diagnostic::new(
                    Code::Fb0133,
                    strict,
                    &page.path,
                    err_line,
                    err_col,
                    format!(
                        "`## {worked_example_heading}` with content must begin with `{SYNTHETIC_LABEL}`"
                    ),
                ));
            }
        }
    }

    // FB0137 and FB0114: Change note check for eight-part page types.
    if page.meta.r#type != PageType::Brief
        && let Some(change_note) = parts.get(change_note_heading)
    {
        let part_lines = lines_in_range(&lines, change_note.line + 1, change_note.end_line);
        check_change_note_lines(&page.path, &part_lines, strict, report);
    }
}

/// Returns the first `FB0130` heading fault `(line, col, message)` on `parts` for `page_type`,
/// if any.
#[must_use]
pub(crate) fn first_heading_fault(
    parts: &Parts,
    page_type: PageType,
) -> Option<(usize, usize, String)> {
    let expected: &[&str] = if page_type == PageType::Brief {
        strings::brief_parts()
    } else {
        strings::parts()
    };

    let mut exp_idx = 0usize;
    for h in parts.headings() {
        if h.level == 1 {
            return Some((
                h.line,
                h.col,
                format!("level-1 heading `# {}` is not allowed in a page", h.text),
            ));
        }
        if h.level > 3 {
            return Some((
                h.line,
                h.col,
                format!(
                    "heading `{}` at level {} is deeper than `###`",
                    h.text, h.level
                ),
            ));
        }
        if h.level == 3 {
            if exp_idx == 0 {
                return Some((
                    h.line,
                    h.col,
                    format!(
                        "expected `## {}` before subsection `### {}`",
                        expected[0], h.text
                    ),
                ));
            }
            continue;
        }
        // h.level == 2
        if let Some(&exp) = expected.get(exp_idx) {
            if h.text == exp {
                exp_idx += 1;
            } else {
                return Some((
                    h.line,
                    h.col,
                    format!("expected part heading `## {exp}`, found `## {}`", h.text),
                ));
            }
        } else {
            return Some((
                h.line,
                h.col,
                format!("unexpected extra part heading `## {}`", h.text),
            ));
        }
    }

    if let Some(&missing) = expected.get(exp_idx) {
        return Some((
            parts.body_end_line(),
            1,
            format!("missing required part heading `## {missing}`"),
        ));
    }

    None
}

fn collect_body_lines(body: &str, base_line: usize) -> Vec<(usize, &str)> {
    body.lines()
        .enumerate()
        .map(|(idx, line)| (base_line + idx, line))
        .collect()
}

fn lines_in_range<'a>(
    lines: &'a [(usize, &'a str)],
    start_line: usize,
    end_line: usize,
) -> Vec<(usize, &'a str)> {
    if start_line > end_line {
        return Vec::new();
    }
    lines
        .iter()
        .copied()
        .filter(|&(line_num, _)| line_num >= start_line && line_num <= end_line)
        .collect()
}

fn is_single_paragraph_exact(evs: &[Ev<'_>], expected: &str) -> bool {
    if evs.len() < 3 {
        return false;
    }
    if !matches!(
        evs.first(),
        Some(Ev {
            event: Event::Start(Tag::Paragraph),
            ..
        })
    ) {
        return false;
    }
    if !matches!(
        evs.last(),
        Some(Ev {
            event: Event::End(TagEnd::Paragraph),
            ..
        })
    ) {
        return false;
    }
    let mut text = String::new();
    for ev in &evs[1..evs.len() - 1] {
        match &ev.event {
            Event::Text(t) => text.push_str(t),
            _ => return false,
        }
    }
    text == expected
}

fn first_block_is_exact_paragraph(evs: &[Ev<'_>], expected: &str) -> bool {
    if !matches!(
        evs.first(),
        Some(Ev {
            event: Event::Start(Tag::Paragraph),
            ..
        })
    ) {
        return false;
    }
    let mut text = String::new();
    for ev in &evs[1..] {
        match &ev.event {
            Event::End(TagEnd::Paragraph) => return text == expected,
            Event::Text(t) => text.push_str(t),
            _ => return false,
        }
    }
    false
}

fn check_change_note_lines(
    path: &str,
    part_lines: &[(usize, &str)],
    strict: bool,
    report: &mut Report,
) {
    let non_empty: Vec<(usize, &str)> = part_lines
        .iter()
        .copied()
        .filter(|(_, text)| !text.trim().is_empty())
        .collect();

    // A blank Change note is reported as FB0131 alone (step 8).
    if non_empty.is_empty() {
        return;
    }

    let mut parsed_entries: Vec<(usize, ChangeClass)> = Vec::new();
    for (line_num, text) in non_empty {
        match parse_change_note_line(text) {
            Err(msg) => {
                report.push(Diagnostic::new(
                    Code::Fb0137,
                    strict,
                    path,
                    line_num,
                    1,
                    msg,
                ));
                return;
            }
            Ok((change_class, date_str, date_col)) => {
                if CivilDate::parse(date_str).is_err() {
                    report.push(Diagnostic::new(
                        Code::Fb0114,
                        strict,
                        path,
                        line_num,
                        date_col,
                        format!("Change note date `{date_str}` is not a real calendar date"),
                    ));
                }
                parsed_entries.push((line_num, change_class));
            }
        }
    }

    if let Some(&(last_line, last_class)) = parsed_entries.last()
        && last_class != ChangeClass::NewKnowledge
    {
        report.push(Diagnostic::new(
            Code::Fb0137,
            strict,
            path,
            last_line,
            1,
            format!(
                "oldest Change note entry (last line) must be `NEW_KNOWLEDGE`, found `{last_class}`"
            ),
        ));
    }
}

fn parse_change_note_line(line: &str) -> Result<(ChangeClass, &str, usize), String> {
    let Some(after_bullet) = line.strip_prefix("- ") else {
        return Err("Change note entry must have form `- CLASS, YYYY-MM-DD: sentence.`".to_owned());
    };
    let Some((class_str, after_comma)) = after_bullet.split_once(", ") else {
        return Err("Change note entry must have form `- CLASS, YYYY-MM-DD: sentence.`".to_owned());
    };
    let Some((date_str, sentence)) = after_comma.split_once(": ") else {
        return Err("Change note entry must have form `- CLASS, YYYY-MM-DD: sentence.`".to_owned());
    };
    if !has_yyyy_mm_dd_shape(date_str) {
        return Err(format!(
            "Change note entry date `{date_str}` does not have form `YYYY-MM-DD`"
        ));
    }
    let trimmed_sentence = sentence.trim();
    if trimmed_sentence.is_empty()
        || trimmed_sentence != sentence
        || !trimmed_sentence.ends_with('.')
        || trimmed_sentence == "."
    {
        return Err("Change note entry sentence must end with a full stop (`.`)".to_owned());
    }
    let Ok(change_class) = ChangeClass::parse(class_str) else {
        return Err(format!("unknown Change note class `{class_str}`"));
    };
    if change_class == ChangeClass::MaterialContractChange {
        return Err(
            "Change note class `MATERIAL_CONTRACT_CHANGE` is company-store only".to_owned(),
        );
    }
    let date_col = class_str.chars().count() + 5;
    Ok((change_class, date_str, date_col))
}

fn has_yyyy_mm_dd_shape(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit()
        && b[9].is_ascii_digit()
}

#[cfg(test)]
#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included at file level per card spec"
)]
#[path = "../../../../tests/support/rng.rs"]
mod rng;

#[cfg(test)]
mod tests {
    use super::{
        SYNTHETIC_LABEL, check_change_note_lines, collect_body_lines, first_heading_fault,
        heading_anchors, lines_in_range, rng::Rng, split_parts,
    };
    use crate::check::markdown;
    use crate::check::{Code, Host, Options, Report, Tree, load, run};
    use crate::date::CivilDate;
    use crate::schema::vocab::PageType;
    use crate::strings;

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const MINI_ASSISTANT_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const MINI_CHANGES_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const MINI_SOURCES_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const MINI_CLAIMS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const MINI_QUESTIONS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const MINI_CONFLICTS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const MINI_GLOSSARY_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");

    const PAGE_CMP_REVIEW_STEPS: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const PAGE_EST_RATE_WINDOW_BRIEF: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    const PATH_CMP_REVIEW_STEPS: &str =
        "corpus/pages/compliance/business-systems/cmp-review-steps.md";
    const PATH_CMP_SYSTEM_REVIEW: &str =
        "corpus/pages/compliance/business-systems/cmp-system-review.md";
    const PATH_EST_RATE_WINDOW_BRIEF: &str =
        "corpus/pages/estimating/pricing/est-rate-window-brief.md";

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating");
        tree.insert_dir("corpus/pages/estimating/pricing");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, PAGE_CMP_REVIEW_STEPS.as_bytes());
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, PAGE_CMP_SYSTEM_REVIEW.as_bytes());
        tree.insert_bytes(
            PATH_EST_RATE_WINDOW_BRIEF,
            PAGE_EST_RATE_WINDOW_BRIEF.as_bytes(),
        );
        tree
    }

    #[test]
    fn synthetic_label_is_pinned() {
        assert_eq!(
            SYNTHETIC_LABEL,
            "Synthetic example. The numbers are made up."
        );
    }

    #[test]
    fn parts_accessor_returns_the_parsed_parts() {
        let tree = mini_tree();
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

        let steps = model
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-review-steps")
            .expect("cmp-review-steps in model");
        let headings: Vec<&str> = steps
            .parts()
            .parts()
            .iter()
            .map(|p| p.heading.as_str())
            .collect();
        assert_eq!(headings.as_slice(), strings::parts());
        assert_eq!(steps.parts().parts()[0].line, 18);
        assert_eq!(
            steps
                .parts()
                .heading_anchor(steps.parts().parts()[0].heading_event),
            Some("purpose")
        );

        let brief = model
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-window-brief")
            .expect("est-rate-window-brief in model");
        let brief_headings: Vec<&str> = brief
            .parts()
            .parts()
            .iter()
            .map(|p| p.heading.as_str())
            .collect();
        assert_eq!(brief_headings.as_slice(), strings::brief_parts());
        assert!(brief.parts().get(strings::parts()[7]).is_none());
    }

    #[test]
    fn brief_with_six_parts_and_subsection_and_empty_part_worked_example_are_valid() {
        let mut tree = mini_tree();
        // Add a `###` subsection inside `## How it works`, set `## Worked example` to `strings::empty_part()`,
        // and include a fenced code block containing `# not a heading` and `## not a part`.
        let updated = PAGE_CMP_REVIEW_STEPS
            .replace(
                "## How it works\n\n",
                "## How it works\n\n### Checklist preparation\n\n```text\n# not a heading\n## not a part\n```\n\n",
            )
            .replace(
                "Synthetic example. The numbers are made up.\n\nA synthetic team checks 12 sample vouchers across 3 days and closes all findings.",
                strings::empty_part(),
            );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, updated.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report.diagnostics.is_empty(),
            "expected valid brief, ### subsection, code-block headings and Not established. Worked example to raise nothing, got {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn fb0130_part_headings_h1_and_h4_headings() {
        // 1. Misspelt / wrong part heading (`## Rule` instead of `## The rule`).
        let mut tree = mini_tree();
        let wrong_part = PAGE_CMP_REVIEW_STEPS.replace("## The rule", "## Rule");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, wrong_part.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0130);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 22);

        // 2. A `####` heading raises FB0130 at that heading.
        let mut tree_h4 = mini_tree();
        let with_h4 = PAGE_CMP_REVIEW_STEPS.replace(
            "## How it works\n\n",
            "## How it works\n\n#### Too deep\n\n",
        );
        tree_h4.insert_bytes(PATH_CMP_REVIEW_STEPS, with_h4.as_bytes());
        let report_h4 = run(&Host::new(tree_h4, sample_today()), &Options::new(false));
        assert_eq!(
            report_h4.diagnostics.len(),
            1,
            "{:?}",
            report_h4.diagnostics
        );
        assert_eq!(report_h4.diagnostics[0].code, Code::Fb0130);
        assert_eq!(report_h4.diagnostics[0].line, 28);

        // 3. A `#` level-1 heading raises FB0130 at that heading.
        let mut tree_h1 = mini_tree();
        let with_h1 = PAGE_CMP_REVIEW_STEPS.replace("## Purpose", "# Title\n\n## Purpose");
        tree_h1.insert_bytes(PATH_CMP_REVIEW_STEPS, with_h1.as_bytes());
        let report_h1 = run(&Host::new(tree_h1, sample_today()), &Options::new(false));
        assert_eq!(
            report_h1.diagnostics.len(),
            1,
            "{:?}",
            report_h1.diagnostics
        );
        assert_eq!(report_h1.diagnostics[0].code, Code::Fb0130);
        assert_eq!(report_h1.diagnostics[0].line, 18);

        // 4. Missing `## Change note` at the end of the body raises FB0130 alone (not FB0137).
        let mut tree_missing_cn = mini_tree();
        let no_cn = PAGE_CMP_REVIEW_STEPS.replace(
            "\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic business-system review procedure page.\n",
            "\n",
        );
        tree_missing_cn.insert_bytes(PATH_CMP_REVIEW_STEPS, no_cn.as_bytes());
        let report_no_cn = run(
            &Host::new(tree_missing_cn, sample_today()),
            &Options::new(false),
        );
        assert_eq!(
            report_no_cn.diagnostics.len(),
            1,
            "{:?}",
            report_no_cn.diagnostics
        );
        assert_eq!(report_no_cn.diagnostics[0].code, Code::Fb0130);
    }

    #[test]
    fn fb0131_blank_part_and_blank_change_note_alone() {
        // 1. Blank `## Purpose` raises FB0131 at `## Purpose` (line 18).
        let mut tree = mini_tree();
        let blank_purpose = PAGE_CMP_REVIEW_STEPS.replace(
            "## Purpose\n\nDescribe the synthetic step-by-step procedure for completing a system review.\n\n## The rule",
            "## Purpose\n\n## The rule",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, blank_purpose.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0131);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 18);

        // 2. Blank `## Change note` raises FB0131 alone (and not FB0137).
        let mut tree_blank_cn = mini_tree();
        let blank_cn = PAGE_CMP_REVIEW_STEPS.replace(
            "## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic business-system review procedure page.\n",
            "## Change note\n",
        );
        tree_blank_cn.insert_bytes(PATH_CMP_REVIEW_STEPS, blank_cn.as_bytes());
        let report_blank_cn = run(
            &Host::new(tree_blank_cn, sample_today()),
            &Options::new(false),
        );
        assert_eq!(
            report_blank_cn.diagnostics.len(),
            1,
            "{:?}",
            report_blank_cn.diagnostics
        );
        assert_eq!(report_blank_cn.diagnostics[0].code, Code::Fb0131);
        assert_eq!(report_blank_cn.diagnostics[0].line, 48);
    }

    #[test]
    fn sources_heading_alone_is_valid() {
        let tree = mini_tree();
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    }

    #[test]
    fn sources_text_is_fb0132() {
        let mut tree = mini_tree();
        let with_sources_text = PAGE_CMP_REVIEW_STEPS.replace(
            "## Sources\n\n## Change note",
            "## Sources\n\nA sentence under Sources.\n\n## Change note",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, with_sources_text.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0132);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 48);
    }

    #[test]
    fn fb0133_worked_example_without_synthetic_label() {
        let mut tree = mini_tree();
        let without_label = PAGE_CMP_REVIEW_STEPS.replace(
            "Synthetic example. The numbers are made up.\n\n",
            "| Item | Days |\n|---|---|\n| Voucher | 3 |\n\n",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, without_label.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0133);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 34);
    }

    #[test]
    fn fb0137_and_fb0114_change_note_rules() {
        // 1. Unknown class (`UPDATE`) raises FB0137.
        let mut tree_unk = mini_tree();
        let unk = PAGE_CMP_REVIEW_STEPS.replace(
            "- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic business-system review procedure page.",
            "- UPDATE, 2025-11-01: text.",
        );
        tree_unk.insert_bytes(PATH_CMP_REVIEW_STEPS, unk.as_bytes());
        let report_unk = run(&Host::new(tree_unk, sample_today()), &Options::new(false));
        assert_eq!(
            report_unk.diagnostics.len(),
            1,
            "{:?}",
            report_unk.diagnostics
        );
        assert_eq!(report_unk.diagnostics[0].code, Code::Fb0137);
        assert_eq!(report_unk.diagnostics[0].line, 50);

        // 2. Overlay-only `MATERIAL_CONTRACT_CHANGE` raises FB0137.
        let mut tree_mcc = mini_tree();
        let mcc = PAGE_CMP_REVIEW_STEPS.replace(
            "- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic business-system review procedure page.",
            "- MATERIAL_CONTRACT_CHANGE, 2026-02-02: Changed contract term.\n- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic business-system review procedure page.",
        );
        tree_mcc.insert_bytes(PATH_CMP_REVIEW_STEPS, mcc.as_bytes());
        let report_mcc = run(&Host::new(tree_mcc, sample_today()), &Options::new(false));
        assert_eq!(
            report_mcc.diagnostics.len(),
            1,
            "{:?}",
            report_mcc.diagnostics
        );
        assert_eq!(report_mcc.diagnostics[0].code, Code::Fb0137);
        assert_eq!(report_mcc.diagnostics[0].line, 50);

        // 3. Oldest entry (last line) not `NEW_KNOWLEDGE` raises FB0137 at the last line.
        let mut tree_last = mini_tree();
        let not_nk = PAGE_CMP_REVIEW_STEPS.replace(
            "- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic business-system review procedure page.",
            "- SOURCE_REFRESH, 2026-02-01: Refreshed the procedure page.",
        );
        tree_last.insert_bytes(PATH_CMP_REVIEW_STEPS, not_nk.as_bytes());
        let report_last = run(&Host::new(tree_last, sample_today()), &Options::new(false));
        assert_eq!(
            report_last.diagnostics.len(),
            1,
            "{:?}",
            report_last.diagnostics
        );
        assert_eq!(report_last.diagnostics[0].code, Code::Fb0137);
        assert_eq!(report_last.diagnostics[0].line, 50);

        // 4. Entry dated `2025-02-30` raises FB0114 (and not FB0137).
        let mut tree_date = mini_tree();
        let bad_date = PAGE_CMP_REVIEW_STEPS.replace(
            "- NEW_KNOWLEDGE, 2026-02-01: Added the synthetic business-system review procedure page.",
            "- NEW_KNOWLEDGE, 2025-02-30: Added the synthetic business-system review procedure page.",
        );
        tree_date.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_date.as_bytes());
        let report_date = run(&Host::new(tree_date, sample_today()), &Options::new(false));
        assert_eq!(
            report_date.diagnostics.len(),
            1,
            "{:?}",
            report_date.diagnostics
        );
        assert_eq!(report_date.diagnostics[0].code, Code::Fb0114);
        assert_eq!(report_date.diagnostics[0].line, 50);
    }

    #[test]
    fn heading_anchors_are_unique_and_deterministic() {
        let md = "\
## Café & Résumé — 2026
### ??? !!
### Section
### Overview 2
### Overview
### Overview
### `code_span` and *emphasis*
";
        let evs = markdown::events(md, 10);
        let anchors_1 = heading_anchors(&evs);
        let anchors_2 = heading_anchors(&evs);
        assert_eq!(anchors_1, anchors_2);

        let ordered: Vec<&str> = anchors_1.values().map(String::as_str).collect();
        assert_eq!(
            ordered,
            vec![
                "café-résumé-2026",
                "section",
                "section-2",
                "overview-2",
                "overview",
                "overview-3",
                "codespan-and-emphasis",
            ]
        );
    }

    fn verify_split_invariants(text: &str, start_line: usize) {
        let max_line = start_line + text.bytes().filter(|&b| b == b'\n').count();
        let evs = markdown::events(text, start_line);
        let parts = split_parts(text, start_line);
        assert_eq!(parts.event_count(), evs.len());
        assert!(parts.body_start_line() == start_line.max(1));
        assert!(parts.body_end_line() >= start_line.max(1) && parts.body_end_line() <= max_line);

        for p in parts.parts() {
            assert!(p.heading_event < evs.len());
            assert!(p.event_range.start <= p.event_range.end);
            assert!(p.event_range.end <= evs.len());
            assert!(p.line >= start_line && p.line <= max_line);
            assert!(p.end_line >= p.line && p.end_line <= max_line);
            assert!(p.col >= 1);
        }

        for h in parts.headings() {
            assert!(h.event_index < evs.len());
            assert!(h.line >= start_line && h.line <= max_line);
            assert!(h.col >= 1);
        }

        for (&ev_pos, anchor) in parts.heading_anchors() {
            assert!(ev_pos < evs.len());
            assert!(!anchor.is_empty());
        }

        let _ = first_heading_fault(&parts, PageType::Concept);
        let _ = first_heading_fault(&parts, PageType::Brief);

        let lines = collect_body_lines(text, start_line.max(1));
        let mut report = Report::new();
        check_change_note_lines("corpus/pages/a/b/c.md", &lines, false, &mut report);
        for p in parts.parts() {
            let sub = lines_in_range(&lines, p.line + 1, p.end_line);
            check_change_note_lines("corpus/pages/a/b/c.md", &sub, false, &mut report);
        }
        for d in &report.diagnostics {
            assert!(d.line >= start_line && d.line <= max_line);
            assert!(d.col >= 1);
        }
    }

    #[test]
    fn parts_never_panic_on_200_seeded_inputs() {
        let mut prng = Rng::new(1);
        for i in 0..200 {
            let len = usize::try_from(prng.next_u64() % 192).unwrap_or(0);
            let raw = prng.bytes(len);
            let text = String::from_utf8_lossy(&raw);
            let start_line = (i % 12) + 1;
            verify_split_invariants(&text, start_line);
        }

        let seed = "\
## Purpose

§ 15.404-1 — café — 日本語 summary.

## The rule

Rule text [@CL-CMP-0001].

## How it works

### Subheading — αβγ

1. First step.
2. Second step.

## Worked example

Synthetic example. The numbers are made up.

Worked body.

## Never confuse

Boundary text.

## Company calibration

Question [@Q-001].

## Sources

## Change note

- SOURCE_REFRESH, 2026-02-15: Updated café — 日本語 rule.
- NEW_KNOWLEDGE, 2026-02-01: Added initial page.
";
        let seed_bytes = seed.as_bytes();
        for idx in 0..seed_bytes.len() {
            let mut flipped = seed_bytes.to_vec();
            flipped[idx] ^= 0x5a;
            let text_flip = String::from_utf8_lossy(&flipped);
            verify_split_invariants(&text_flip, 18);

            let mut deleted = seed_bytes.to_vec();
            deleted.remove(idx);
            let text_del = String::from_utf8_lossy(&deleted);
            verify_split_invariants(&text_del, 18);
        }
    }
}

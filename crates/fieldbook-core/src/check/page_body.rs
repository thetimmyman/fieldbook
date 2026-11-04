//! The `check-page-body` pass: `FB0134`, `FB0135`, `FB0138` and `FB0139`.
//!
//! Validates required content per page type (`FB0134`), raw HTML, images and outside links
//! (`FB0135`), `brief` word count over 400 words (`FB0138`), and bold glossary term definitions on
//! pages (`FB0139`).

use std::collections::BTreeSet;

use pulldown_cmark::CodeBlockKind;

use crate::ids::{CaseId, ClaimId, QuestionId, VersionId};
use crate::schema::{PageType, Register};
use crate::strings;

use super::markdown::{self, Ev, Event, HeadingLevel, Tag, TagEnd};
use super::page_links::{self, LinkResolution};
use super::{Code, Diagnostic, Model, Options, PageEntry, Report};

/// Required first-column row names of the six-row opening table in `## The rule` on a
/// `clause-card` page (`docs/reference/corpus-format.md#page-files`).
pub const CLAUSE_CARD_ROWS: [&str; 6] = [
    "Instrument",
    "Codified citation",
    "RFO citation",
    "Edition date",
    "Text status",
    "Applies to",
];

const BRIEF_MAX_WORDS: usize = 400;

/// Run the `check-page-body` pass over `model` and append diagnostics to `report`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    let glossary_terms = collect_glossary_terms(model);
    for page in model.pages() {
        check_page(page, &glossary_terms, options.strict, report);
    }
}

fn collect_glossary_terms(model: &Model) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    for row in model.glossary() {
        let label = row.value.label.trim();
        if !label.is_empty() {
            terms.insert(label.to_lowercase());
        }
        for alias in &row.value.aliases {
            let trimmed = alias.trim();
            if !trimmed.is_empty() {
                terms.insert(trimmed.to_lowercase());
            }
        }
    }
    for row in model.register_rows(Register::Glossary) {
        if let Some(cell) = row.cells.get(1) {
            let label = cell.trim();
            if !label.is_empty() {
                terms.insert(label.to_lowercase());
            }
        }
        if let Some(cell) = row.cells.get(7) {
            for alias in cell.split('|') {
                let trimmed = alias.trim();
                if !trimmed.is_empty() {
                    terms.insert(trimmed.to_lowercase());
                }
            }
        }
    }
    terms
}

fn check_page(
    page: &PageEntry,
    glossary_terms: &BTreeSet<String>,
    strict: bool,
    report: &mut Report,
) {
    let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
    let evs = markdown::events(&page.body, base_line);

    check_required_content(page, &evs, strict, report);
    check_html_images_and_outside_links(page, &evs, strict, report);
    check_brief_word_count(page, &evs, strict, report);
    check_bold_glossary_terms(page, &evs, glossary_terms, strict, report);
}

fn check_required_content(page: &PageEntry, evs: &[Ev<'_>], strict: bool, report: &mut Report) {
    let parts = page.parts();
    match page.meta.r#type {
        PageType::Concept => {}
        PageType::Procedure => {
            let heading = strings::parts()[2];
            if let Some(part) = parts.get(heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_top_level_ordered_list_of_at_least_two(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`procedure` page `## How it works` must hold a top-level ordered list of two or more items",
                    ));
                }
            }
        }
        PageType::Reference => {
            let heading = strings::parts()[1];
            if let Some(part) = parts.get(heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_table(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`reference` page `## The rule` must contain a table",
                    ));
                }
            }
        }
        PageType::Case => {
            let heading = strings::parts()[2];
            if let Some(part) = parts.get(heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_case_subsections(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`case` page `## How it works` level-3 headings must be `### Public record` then `### Inference`",
                    ));
                }
            }
        }
        PageType::ClauseCard => {
            let heading = strings::parts()[1];
            if let Some(part) = parts.get(heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_valid_clause_card_table(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`clause-card` page `## The rule` must begin with the six-row metadata table",
                    ));
                }
            }
        }
        PageType::Calc => {
            let heading = strings::parts()[3];
            if let Some(part) = parts.get(heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty()
                    && !has_calc_fixture_line(page, part.line + 1, part.end_line)
                {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`calc` page `## Worked example` must contain a `Fixture: <id>` line with a valid calculation case id",
                    ));
                }
            }
        }
        PageType::Checklist => {
            let heading = strings::parts()[2];
            if let Some(part) = parts.get(heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_valid_checklist_how_it_works(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`checklist` page `## How it works` must hold only task items and `###` headings, with no `YYYY-MM-DD` date",
                    ));
                }
            }
        }
        PageType::Template => {
            let heading = strings::parts()[1];
            if let Some(part) = parts.get(heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_fenced_text_block(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`template` page `## The rule` must hold a fenced `text` code block",
                    ));
                }
            }
        }
        PageType::Brief => {
            let evidence_heading = strings::brief_parts()[2];
            if let Some(part) = parts.get(evidence_heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_valid_brief_evidence(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`brief` page `## Evidence` must be a list in which every item cites a claim or source version",
                    ));
                }
            }
            let unknowns_heading = strings::brief_parts()[4];
            if let Some(part) = parts.get(unknowns_heading) {
                let part_evs = &evs[part.event_range.clone()];
                if !part_evs.is_empty() && !has_valid_brief_unknowns(part_evs) {
                    report.push(Diagnostic::new(
                        Code::Fb0134,
                        strict,
                        &page.path,
                        part.line,
                        part.col,
                        "`brief` page `## Unknowns` must be a list in which every item cites a question or is `Not established.`",
                    ));
                }
            }
        }
    }
}

fn has_top_level_ordered_list_of_at_least_two(part_evs: &[Ev<'_>]) -> bool {
    let mut depth = 0usize;
    let mut in_top_ordered_list = false;
    let mut top_item_count = 0usize;

    for ev in part_evs {
        match &ev.event {
            Event::Start(tag) => {
                if depth == 0 && matches!(tag, Tag::List(Some(_))) {
                    in_top_ordered_list = true;
                    top_item_count = 0;
                } else if depth == 1 && in_top_ordered_list && matches!(tag, Tag::Item) {
                    top_item_count += 1;
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 && in_top_ordered_list {
                    if top_item_count >= 2 {
                        return true;
                    }
                    in_top_ordered_list = false;
                }
            }
            _ => {}
        }
    }
    false
}

fn has_table(part_evs: &[Ev<'_>]) -> bool {
    part_evs
        .iter()
        .any(|ev| matches!(ev.event, Event::Start(Tag::Table(_))))
}

fn has_case_subsections(part_evs: &[Ev<'_>]) -> bool {
    let mut h3s = Vec::new();
    let mut idx = 0;
    while idx < part_evs.len() {
        if let Event::Start(Tag::Heading {
            level: HeadingLevel::H3,
            ..
        }) = part_evs[idx].event
        {
            let mut text = String::new();
            idx += 1;
            while idx < part_evs.len() {
                match &part_evs[idx].event {
                    Event::End(TagEnd::Heading(_)) => break,
                    Event::Text(t) | Event::Code(t) => text.push_str(t),
                    _ => {}
                }
                idx += 1;
            }
            h3s.push(text);
        }
        idx += 1;
    }
    h3s.len() == 2 && h3s[0] == "Public record" && h3s[1] == "Inference"
}

fn has_valid_clause_card_table(part_evs: &[Ev<'_>]) -> bool {
    let Some(first) = part_evs.first() else {
        return false;
    };
    if !matches!(first.event, Event::Start(Tag::Table(_))) {
        return false;
    }

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut in_body_row = false;
    let mut current_row: Vec<String> = Vec::new();
    let mut in_cell = false;
    let mut current_cell = String::new();

    for ev in &part_evs[1..] {
        match &ev.event {
            Event::End(TagEnd::Table) => break,
            Event::Start(Tag::TableRow) => {
                in_body_row = true;
                current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                if in_body_row {
                    rows.push(std::mem::take(&mut current_row));
                    in_body_row = false;
                }
            }
            Event::Start(Tag::TableCell) if in_body_row => {
                in_cell = true;
                current_cell.clear();
            }
            Event::End(TagEnd::TableCell) if in_body_row => {
                if in_cell {
                    current_row.push(std::mem::take(&mut current_cell));
                    in_cell = false;
                }
            }
            Event::Text(t) | Event::Code(t) if in_cell => {
                current_cell.push_str(t);
            }
            _ => {}
        }
    }

    if rows.len() != CLAUSE_CARD_ROWS.len() {
        return false;
    }

    let empty_part = strings::empty_part();
    for (row, &expected_name) in rows.iter().zip(&CLAUSE_CARD_ROWS) {
        if row.len() != 2 {
            return false;
        }
        if row[0].trim() != expected_name {
            return false;
        }
        let val = row[1].trim();
        if val != empty_part && !has_claim_key(val) {
            return false;
        }
    }

    true
}

fn has_calc_fixture_line(page: &PageEntry, start_line: usize, end_line: usize) -> bool {
    let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
    for (idx, raw_line) in page.body.split('\n').enumerate() {
        let line_num = base_line + idx;
        if line_num < start_line || line_num > end_line {
            continue;
        }
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if let Some(id) = line.strip_prefix("Fixture: ")
            && CaseId::parse(id).is_ok()
        {
            return true;
        }
    }
    false
}

fn has_valid_checklist_how_it_works(part_evs: &[Ev<'_>]) -> bool {
    let mut task_items = 0usize;
    let mut idx = 0;

    while idx < part_evs.len() {
        match &part_evs[idx].event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H3,
                ..
            }) => {
                idx += 1;
                while idx < part_evs.len() {
                    match &part_evs[idx].event {
                        Event::End(TagEnd::Heading(HeadingLevel::H3)) => break,
                        Event::Text(t) | Event::Code(t) if contains_yyyy_mm_dd(t) => {
                            return false;
                        }
                        _ => {}
                    }
                    idx += 1;
                }
            }
            Event::Start(Tag::List(None)) => {
                idx += 1;
                let mut list_depth = 1usize;
                while idx < part_evs.len() && list_depth > 0 {
                    match &part_evs[idx].event {
                        Event::Start(Tag::List(Some(_))) => {
                            return false;
                        }
                        Event::Start(Tag::Item) => {
                            // Every item (including nested items) must start with a TaskListMarker
                            // (directly or right after Paragraph start).
                            let next_is_task = match part_evs.get(idx + 1).map(|e| &e.event) {
                                Some(Event::TaskListMarker(_)) => true,
                                Some(Event::Start(Tag::Paragraph)) => matches!(
                                    part_evs.get(idx + 2).map(|e| &e.event),
                                    Some(Event::TaskListMarker(_))
                                ),
                                _ => false,
                            };
                            if !next_is_task {
                                return false;
                            }
                            task_items += 1;
                            list_depth += 1;
                        }
                        Event::Start(_) => {
                            if list_depth == 1 {
                                return false;
                            }
                            list_depth += 1;
                        }
                        Event::End(_) => {
                            list_depth = list_depth.saturating_sub(1);
                        }
                        Event::Text(t) | Event::Code(t) if contains_yyyy_mm_dd(t) => {
                            return false;
                        }
                        _ => {}
                    }
                    idx += 1;
                }
                continue;
            }
            _ => return false,
        }
        idx += 1;
    }

    task_items > 0
}

fn contains_yyyy_mm_dd(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 10 {
        return false;
    }
    for i in 0..=(bytes.len() - 10) {
        if i > 0 && bytes[i - 1].is_ascii_digit() {
            continue;
        }
        if i + 10 < bytes.len() && bytes[i + 10].is_ascii_digit() {
            continue;
        }
        let slice = &bytes[i..i + 10];
        if slice[0].is_ascii_digit()
            && slice[1].is_ascii_digit()
            && slice[2].is_ascii_digit()
            && slice[3].is_ascii_digit()
            && slice[4] == b'-'
            && slice[5].is_ascii_digit()
            && slice[6].is_ascii_digit()
            && slice[7] == b'-'
            && slice[8].is_ascii_digit()
            && slice[9].is_ascii_digit()
        {
            return true;
        }
    }
    false
}

fn has_fenced_text_block(part_evs: &[Ev<'_>]) -> bool {
    part_evs.iter().any(|ev| {
        matches!(
            &ev.event,
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.trim() == "text"
        )
    })
}

fn collect_top_level_list_items(part_evs: &[Ev<'_>]) -> Option<Vec<String>> {
    if part_evs.is_empty() {
        return None;
    }
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut item_stack: Vec<String> = Vec::new();

    for ev in part_evs {
        match &ev.event {
            Event::Start(Tag::List(_)) => {
                depth += 1;
            }
            Event::Start(Tag::Item) => {
                if depth == 0 {
                    return None;
                }
                depth += 1;
                item_stack.push(String::new());
            }
            Event::Start(_) => {
                if depth == 0 {
                    return None;
                }
                depth += 1;
            }
            Event::End(TagEnd::Item) => {
                depth = depth.saturating_sub(1);
                if let Some(finished_item) = item_stack.pop() {
                    items.push(finished_item);
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some(top) = item_stack.last_mut() {
                    top.push_str(t);
                } else if depth == 0 {
                    return None;
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(top) = item_stack.last_mut() {
                    top.push(' ');
                }
            }
            _ => {
                if depth == 0 {
                    return None;
                }
            }
        }
    }

    if items.is_empty() { None } else { Some(items) }
}

fn has_valid_brief_evidence(part_evs: &[Ev<'_>]) -> bool {
    let Some(items) = collect_top_level_list_items(part_evs) else {
        return false;
    };
    items
        .iter()
        .all(|item| has_claim_key(item) || has_source_version_key(item))
}

fn has_valid_brief_unknowns(part_evs: &[Ev<'_>]) -> bool {
    let Some(items) = collect_top_level_list_items(part_evs) else {
        return false;
    };
    let empty_part = strings::empty_part();
    items
        .iter()
        .all(|item| item.contains(empty_part) || has_question_key(item))
}

fn extract_bracketed_keys(text: &str) -> Vec<&str> {
    let mut keys = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find("[@") {
        let after_bracket = &rest[pos + 1..];
        let content = after_bracket
            .split_once(']')
            .map_or(after_bracket, |(inside, _)| inside);
        for part in content.split(';') {
            let trimmed = part.trim();
            if let Some(after_at) = trimmed.strip_prefix('@') {
                let token = after_at
                    .split(|c: char| c == ',' || c.is_whitespace() || c == ']')
                    .next()
                    .unwrap_or("");
                if !token.is_empty() {
                    keys.push(token);
                }
            }
        }
        rest = &rest[pos + 2..];
    }
    keys
}

fn has_claim_key(text: &str) -> bool {
    extract_bracketed_keys(text)
        .into_iter()
        .any(|k| ClaimId::parse(k).is_ok())
}

fn has_source_version_key(text: &str) -> bool {
    extract_bracketed_keys(text)
        .into_iter()
        .any(|k| VersionId::parse(k).is_ok())
}

fn has_question_key(text: &str) -> bool {
    extract_bracketed_keys(text)
        .into_iter()
        .any(|k| QuestionId::parse(k).is_ok())
}

fn check_html_images_and_outside_links(
    page: &PageEntry,
    evs: &[Ev<'_>],
    strict: bool,
    report: &mut Report,
) {
    for ev in evs {
        match &ev.event {
            Event::Start(Tag::HtmlBlock) => {
                report.push(Diagnostic::new(
                    Code::Fb0135,
                    strict,
                    &page.path,
                    ev.line,
                    ev.col,
                    "raw HTML block is not allowed in a page body",
                ));
            }
            Event::InlineHtml(_) => {
                report.push(Diagnostic::new(
                    Code::Fb0135,
                    strict,
                    &page.path,
                    ev.line,
                    ev.col,
                    "inline raw HTML is not allowed in a page body",
                ));
            }
            Event::Start(Tag::Image { .. }) => {
                report.push(Diagnostic::new(
                    Code::Fb0135,
                    strict,
                    &page.path,
                    ev.line,
                    ev.col,
                    "image is not allowed in a page body",
                ));
            }
            _ => {}
        }
    }

    let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
    for link in markdown::body_links(&page.body, base_line) {
        if page_links::resolve_page_link(&page.path, &link.target) == LinkResolution::OutsideCorpus
        {
            report.push(Diagnostic::new(
                Code::Fb0135,
                strict,
                &page.path,
                link.line,
                link.col,
                format!("link `{}` points outside the Corpus", link.target),
            ));
        }
    }
}

fn check_brief_word_count(page: &PageEntry, evs: &[Ev<'_>], strict: bool, report: &mut Report) {
    if page.meta.r#type != PageType::Brief {
        return;
    }
    let words = count_body_words(evs);
    if words > BRIEF_MAX_WORDS {
        let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
        report.push(Diagnostic::new(
            Code::Fb0138,
            strict,
            &page.path,
            base_line,
            1,
            format!("`brief` body has {words} words (maximum {BRIEF_MAX_WORDS})"),
        ));
    }
}

fn is_inline_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
    )
}

fn is_inline_tag_end(tag_end: &TagEnd) -> bool {
    matches!(
        tag_end,
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link | TagEnd::Image
    )
}

fn count_body_words(evs: &[Ev<'_>]) -> usize {
    let mut combined = String::new();
    for ev in evs {
        match &ev.event {
            Event::Text(t) | Event::Code(t) => combined.push_str(t),
            Event::SoftBreak | Event::HardBreak => {
                combined.push(' ');
            }
            Event::Start(tag) if !is_inline_tag(tag) => {
                combined.push(' ');
            }
            Event::End(tag_end) if !is_inline_tag_end(tag_end) => {
                combined.push(' ');
            }
            _ => {}
        }
    }
    combined.split_whitespace().count()
}

fn check_bold_glossary_terms(
    page: &PageEntry,
    evs: &[Ev<'_>],
    glossary_terms: &BTreeSet<String>,
    strict: bool,
    report: &mut Report,
) {
    if glossary_terms.is_empty() {
        return;
    }

    let mut idx = 0;
    while idx < evs.len() {
        if matches!(evs[idx].event, Event::Start(Tag::Strong)) {
            let start_line = evs[idx].line;
            let start_col = evs[idx].col;
            let mut span_text = String::new();
            idx += 1;
            while idx < evs.len() {
                match &evs[idx].event {
                    Event::End(TagEnd::Strong) => break,
                    Event::Text(t) | Event::Code(t) => span_text.push_str(t),
                    Event::SoftBreak | Event::HardBreak => span_text.push(' '),
                    _ => {}
                }
                idx += 1;
            }

            let trimmed = span_text.trim();
            let candidate = if let Some(without_colon) = trimmed.strip_suffix(':') {
                Some(without_colon.trim())
            } else {
                let followed_by_colon = matches!(
                    evs.get(idx + 1),
                    Some(Ev {
                        event: Event::Text(next_text),
                        ..
                    }) if next_text.starts_with(':')
                );
                if followed_by_colon {
                    Some(trimmed)
                } else {
                    None
                }
            };

            if let Some(term) = candidate
                && !term.is_empty()
                && glossary_terms.contains(&term.to_lowercase())
            {
                report.push(Diagnostic::new(
                    Code::Fb0139,
                    strict,
                    &page.path,
                    start_line,
                    start_col,
                    format!("bold `**{term}**:` defines a glossary term on a page"),
                ));
            }
        }
        idx += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::CLAUSE_CARD_ROWS;
    use crate::check::{Code, Host, Options, Tree, run};
    use crate::date::CivilDate;
    use crate::strings;

    const CORPUS_FORMAT_MD: &str = include_str!("../../../../docs/reference/corpus-format.md");

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

    const PATH_CMP_SYSTEM_REVIEW: &str =
        "corpus/pages/compliance/business-systems/cmp-system-review.md";
    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );

    const PATH_CMP_REVIEW_STEPS: &str =
        "corpus/pages/compliance/business-systems/cmp-review-steps.md";
    const PAGE_CMP_REVIEW_STEPS: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );

    const PATH_EST_RATE_WINDOW_BRIEF: &str =
        "corpus/pages/estimating/pricing/est-rate-window-brief.md";
    const PAGE_EST_RATE_WINDOW_BRIEF: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid sample date")
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

    fn eight_part_page(
        page_type: &str,
        the_rule: &str,
        how_it_works: &str,
        worked: &str,
    ) -> String {
        let p = strings::parts();
        format!(
            "+++\nid = \"cmp-system-review\"\ntitle = \"Synthetic test page\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"{page_type}\"\nstage = 2\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\"]\n+++\n\n## {}\n\nExplain synthetic purpose.\n\n## {}\n\n{the_rule}\n\n## {}\n\n{how_it_works}\n\n## {}\n\n{worked}\n\n## {}\n\n{}\n\n## {}\n\n{}\n\n## {}\n\n## {}\n\n- NEW_KNOWLEDGE, 2026-02-01: Added synthetic page.\n",
            p[0],
            p[1],
            p[2],
            p[3],
            p[4],
            strings::empty_part(),
            p[5],
            strings::empty_part(),
            p[6],
            p[7],
        )
    }

    #[test]
    fn clause_card_row_names_match_the_reference() {
        let row_line = CORPUS_FORMAT_MD
            .lines()
            .find(|line| line.starts_with("| `clause-card` |"))
            .expect("clause-card row in Page types table of corpus-format.md");
        let cells: Vec<&str> = row_line.split('|').map(str::trim).collect();
        let required_col = cells
            .get(4)
            .copied()
            .expect("Required content column in clause-card row");
        let after_marker = required_col
            .split_once("in this order, ")
            .expect("`in this order, ` marker in clause-card required content")
            .1;
        let list_part = after_marker
            .split_once('.')
            .expect("sentence end after row names")
            .0;
        let mut parsed_names: Vec<&str> = Vec::new();
        let parts: Vec<&str> = list_part.split(", ").collect();
        for (i, part) in parts.iter().enumerate() {
            if i + 1 == parts.len()
                && let Some((penultimate, last)) = part.split_once(" and ")
            {
                parsed_names.push(penultimate.trim());
                parsed_names.push(last.trim());
            } else {
                parsed_names.push(part.trim());
            }
        }
        assert_eq!(parsed_names.as_slice(), &CLAUSE_CARD_ROWS);
    }

    #[test]
    fn fb0134_every_page_type_rule_met_and_broken() {
        // 1. procedure: met in mini_tree, broken with 1 step or unordered list.
        {
            let mut tree = mini_tree();
            let one_step = PAGE_CMP_REVIEW_STEPS.replace(
                "1. Open the synthetic control checklist and gather the sample logs.\n2. Verify each synthetic control checkpoint against the [review concept](cmp-system-review.md).\n3. Record the synthetic sign-off outcome before the billing cutoff.",
                "1. Open the synthetic control checklist and gather the sample logs.",
            );
            tree.insert_bytes(PATH_CMP_REVIEW_STEPS, one_step.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
            assert_eq!(report.diagnostics[0].line, 26);
        }

        // 2. reference: met with a table in The rule, broken without a table.
        {
            let mut tree = mini_tree();
            let valid_ref = eight_part_page(
                "reference",
                "| Key | Value |\n|---|---|\n| Threshold | 30 days [@CL-CMP-0001] |",
                "Read the synthetic table row.",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, valid_ref.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

            let broken_ref = eight_part_page(
                "reference",
                "No table here [@CL-CMP-0001].",
                "Read the synthetic table row.",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_ref.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
        }

        // 3. case: met with ### Public record then ### Inference in How it works, broken otherwise.
        {
            let mut tree = mini_tree();
            let valid_case = eight_part_page(
                "case",
                "Synthetic case rule [@CL-CMP-0001].",
                "### Public record\n\nSynthetic record.\n\n### Inference\n\nSynthetic inference.",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, valid_case.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

            let broken_case = eight_part_page(
                "case",
                "Synthetic case rule [@CL-CMP-0001].",
                "### Inference\n\nSynthetic inference.\n\n### Public record\n\nSynthetic record.",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_case.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
        }

        // 4. clause-card: met with six-row table first in The rule (values empty_part or claim key), broken otherwise.
        {
            let mut tree = mini_tree();
            let ep = strings::empty_part();
            let valid_table = format!(
                "| Field | Value |\n|---|---|\n| Instrument | far_codified [@CL-CMP-0001] |\n| Codified citation | 48 CFR 9900.101 [@CL-CMP-0001] |\n| RFO citation | {ep} |\n| Edition date | 2026-01 [@CL-CMP-0001] |\n| Text status | final [@CL-CMP-0001] |\n| Applies to | Fixed-price contracts [@CL-CMP-0001] |\n\nPlain summary [@CL-CMP-0001]."
            );
            let valid_cc = eight_part_page(
                "clause-card",
                &valid_table,
                "Apply clause on covered awards.",
                ep,
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, valid_cc.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

            // Broken when prose precedes the table or when a value cell has neither empty_part nor a claim key.
            let prose_before_table = format!("Introductory prose.\n\n{valid_table}");
            let broken_cc1 =
                eight_part_page("clause-card", &prose_before_table, "Apply clause.", ep);
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_cc1.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);

            let bad_cell_table = valid_table.replace(
                "| Instrument | far_codified [@CL-CMP-0001] |",
                "| Instrument | far_codified [@S-0001-v1] |",
            );
            let broken_cc2 = eight_part_page("clause-card", &bad_cell_table, "Apply clause.", ep);
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_cc2.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
        }

        // 5. calc: met when Worked example holds `Fixture: <CaseId>`, broken when missing or bad id.
        {
            let mut tree = mini_tree();
            let valid_calc = eight_part_page(
                "calc",
                "Synthetic calculation rule [@CL-CMP-0001].",
                "Multiply synthetic base by rate.",
                "Synthetic example. The numbers are made up.\n\nFixture: CALC-01\n\nBase 100 times 0.05 equals 5.",
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, valid_calc.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

            let broken_calc = eight_part_page(
                "calc",
                "Synthetic calculation rule [@CL-CMP-0001].",
                "Multiply synthetic base by rate.",
                "Synthetic example. The numbers are made up.\n\nFixture: BAD-1\n\nBase 100 times 0.05 equals 5.",
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_calc.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
        }

        // 6. checklist: met when How it works holds only task items and ### headings and no YYYY-MM-DD date.
        {
            let mut tree = mini_tree();
            let valid_cl = eight_part_page(
                "checklist",
                "Synthetic checklist rule [@CL-CMP-0001].",
                "### Day 1 to Day 30\n\n- [ ] Gather synthetic logs\n- [ ] Confirm synthetic owner",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, valid_cl.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

            // Broken by a plain bullet or a YYYY-MM-DD date.
            let broken_cl_date = eight_part_page(
                "checklist",
                "Synthetic checklist rule [@CL-CMP-0001].",
                "### Day 1 to Day 30\n\n- [ ] Gather synthetic logs by 2026-03-01",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_cl_date.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);

            let broken_cl_prose = eight_part_page(
                "checklist",
                "Synthetic checklist rule [@CL-CMP-0001].",
                "Introductory prose is not allowed.\n\n- [ ] Gather synthetic logs",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_cl_prose.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);

            // Nested plain bullet under a valid task item must also fail.
            let broken_cl_nested = eight_part_page(
                "checklist",
                "Synthetic checklist rule [@CL-CMP-0001].",
                "### Day 1 to Day 30\n\n- [ ] Gather synthetic logs\n  - Plain nested bullet",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_cl_nested.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
        }

        // 7. template: met when The rule holds a fenced `text` code block, broken otherwise.
        {
            let mut tree = mini_tree();
            let valid_tmpl = eight_part_page(
                "template",
                "```text\nField one: ____\nField two: ____\n```",
                "Fill in each blank field.",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, valid_tmpl.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

            let broken_tmpl = eight_part_page(
                "template",
                "```json\n{}\n```",
                "Fill in each blank field.",
                strings::empty_part(),
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, broken_tmpl.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
        }

        // 8. brief: met when Evidence items hold claim or source-version key and Unknowns items hold question key or empty_part.
        {
            let mut tree = mini_tree();
            let valid_brief_sv = PAGE_EST_RATE_WINDOW_BRIEF
                .replace(
                    "- Synthetic escalation placeholder stands at 3 percent per year [@CL-EST-0001].",
                    "- Synthetic source version evidence item [@S-0002-v2, sec. 1].",
                )
                .replace(
                    "- Multi-year synthetic escalation index selection remains open [@Q-002].",
                    &format!("- {}", strings::empty_part()),
                );
            tree.insert_bytes(PATH_EST_RATE_WINDOW_BRIEF, valid_brief_sv.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

            let broken_brief = PAGE_EST_RATE_WINDOW_BRIEF
                .replace(
                    "- Synthetic escalation placeholder stands at 3 percent per year [@CL-EST-0001].",
                    "- Uncited evidence item.",
                )
                .replace(
                    "- Multi-year synthetic escalation index selection remains open [@Q-002].",
                    "- Uncited unknown item without empty_part.",
                );
            tree.insert_bytes(PATH_EST_RATE_WINDOW_BRIEF, broken_brief.as_bytes());
            let report = run(
                &Host::new(tree.clone(), sample_today()),
                &Options::new(false),
            );
            assert_eq!(report.diagnostics.len(), 2);
            assert!(report.diagnostics.iter().all(|d| d.code == Code::Fb0134));

            // Nested list item without its own citation in Evidence or Unknowns must fail.
            let broken_brief_nested = PAGE_EST_RATE_WINDOW_BRIEF.replace(
                "- Synthetic escalation placeholder stands at 3 percent per year [@CL-EST-0001].",
                "- Synthetic escalation placeholder stands at 3 percent per year [@CL-EST-0001].\n  - Nested evidence bullet without a citation.",
            );
            tree.insert_bytes(PATH_EST_RATE_WINDOW_BRIEF, broken_brief_nested.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].code, Code::Fb0134);
        }
    }

    #[test]
    fn fb0135_html_images_and_outside_links_and_plain_text_address() {
        // Plain-text address raises nothing.
        {
            let mut tree = mini_tree();
            let with_plain_url = PAGE_CMP_SYSTEM_REVIEW.replace(
                "Explain how a synthetic system review interacts with a synthetic billing withhold.",
                "Explain how a synthetic system review (see https://example.com/policy as plain text) interacts with a synthetic billing withhold.",
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, with_plain_url.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert!(
                report.diagnostics.is_empty(),
                "plain-text address must raise nothing, got {:?}",
                report.diagnostics
            );
        }

        // `<!-- todo -->`, `<b>`, an image, `https://` link and a relative link climbing out of `corpus/` (`../../../../x.md`, or `../../../x.md` from a shallower path) raise FB0135.
        {
            let mut tree = mini_tree();
            let with_faults = PAGE_CMP_SYSTEM_REVIEW.replace(
                "Explain how a synthetic system review interacts with a synthetic billing withhold.",
                "<!-- todo -->\n\nExplain <b> bold and ![diagram](diagram.png) and [web](https://example.com) and [climb](../../../../x.md).",
            );
            tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, with_faults.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            let fb0135: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|d| d.code == Code::Fb0135)
                .collect();
            assert_eq!(
                fb0135.len(),
                5,
                "expected 5 FB0135 findings (comment, <b>, image, https://, climb out of corpus/), got {:?}",
                report.diagnostics
            );
        }
    }

    #[test]
    fn fb0138_brief_400_words_passes_and_401_words_warns() {
        let bp = strings::brief_parts();
        // Headings in brief:
        // `## Question` (1), `## Recommendation` (1), `## Evidence` (1), `## Options` (1),
        // `## Unknowns` (1), `## Next action` (2) -> 7 heading words total.
        // Plus 4 fixed 1-word parts (`Recommendation`, `Evidence`, `Options`, `Unknowns`):
        // - Recommendation: `Adopt.` (1 word)
        // - Evidence: `- [@CL-EST-0003]` (1 word)
        // - Options: `Wait.` (1 word)
        // - Unknowns: `- [@Q-002]` (1 word)
        // - Next action: `Revisit.` (1 word)
        // That is 7 + 5 = 12 words outside `## Question`'s body.
        // So 388 words in `## Question` gives exactly 400 words total, and 389 words gives 401 words.
        let make_brief = |question_words: usize| -> String {
            let mut words_vec = vec!["word"; question_words.saturating_sub(1)];
            if question_words > 0 {
                words_vec.push("**word**.");
            }
            let filler = words_vec.join(" ");
            format!(
                "+++\nid = \"est-rate-window-brief\"\ntitle = \"Synthetic decision brief on forward pricing rate review window\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"brief\"\nstage = 3\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\"]\n+++\n\n## {}\n\n{filler}\n\n## {}\n\nAdopt.\n\n## {}\n\n- [@CL-EST-0003]\n\n## {}\n\nWait.\n\n## {}\n\n- [@Q-002]\n\n## {}\n\nRevisit.\n",
                bp[0], bp[1], bp[2], bp[3], bp[4], bp[5],
            )
        };

        let mut tree = mini_tree();
        tree.insert_bytes(PATH_EST_RATE_WINDOW_BRIEF, make_brief(388).as_bytes());
        let report_400 = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::new(false),
        );
        assert!(
            report_400.diagnostics.is_empty(),
            "400-word brief must raise nothing, got {:?}",
            report_400.diagnostics
        );

        tree.insert_bytes(PATH_EST_RATE_WINDOW_BRIEF, make_brief(389).as_bytes());
        let report_401 = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report_401.diagnostics.len(), 1);
        assert_eq!(report_401.diagnostics[0].code, Code::Fb0138);
        assert_eq!(report_401.diagnostics[0].line, 14);
    }

    #[test]
    fn fb0139_bold_glossary_term_internal_and_external_colon_and_non_term() {
        let mut tree = mini_tree();
        let glossary_with_mw = MINI_GLOSSARY_CSV.replace(
            "\nrate_window,",
            "\nmaterial_weakness,material weakness,A synthetic deficiency in internal control.,Do not confuse with a significant deficiency.,cmp,CL-CMP-0001,public_reference,MW,\nrate_window,",
        );
        tree.insert_bytes("corpus/registers/glossary.csv", glossary_with_mw.as_bytes());

        // 1. Internal colon `**Material weakness:**` warns at the span.
        let internal_colon = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Explain how a synthetic system review interacts with a synthetic billing withhold.",
            "**Material weakness:** a synthetic deficiency in internal control.",
        );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, internal_colon.as_bytes());
        let report_int = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::new(false),
        );
        assert_eq!(report_int.diagnostics.len(), 1);
        assert_eq!(report_int.diagnostics[0].code, Code::Fb0139);
        assert_eq!(report_int.diagnostics[0].line, 20);
        assert_eq!(report_int.diagnostics[0].col, 1);

        // 2. External colon `**Material weakness**:` warns at the span.
        let external_colon = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Explain how a synthetic system review interacts with a synthetic billing withhold.",
            "**Material weakness**: a synthetic deficiency in internal control.",
        );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, external_colon.as_bytes());
        let report_ext = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::new(false),
        );
        assert_eq!(report_ext.diagnostics.len(), 1);
        assert_eq!(report_ext.diagnostics[0].code, Code::Fb0139);
        assert_eq!(report_ext.diagnostics[0].line, 20);
        assert_eq!(report_ext.diagnostics[0].col, 1);

        // 3. Bold word that is not a glossary term raises nothing.
        let non_term = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Explain how a synthetic system review interacts with a synthetic billing withhold.",
            "**Unregistered heading**: a synthetic note on control operation.",
        );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, non_term.as_bytes());
        let report_clear = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report_clear.diagnostics.is_empty(),
            "non-term bold span must raise nothing, got {:?}",
            report_clear.diagnostics
        );
    }
}

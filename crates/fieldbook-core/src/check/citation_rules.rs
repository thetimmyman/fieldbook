//! The `check-citation-rules` pass: `FB0117`, `FB0118`, `FB0309`, `FB0310` and `FB0320`.
//!
//! - `FB0117` (warning, never fails `--strict`): a page's computed `review_by` date has passed
//!   (`review_by < Model::today()`).
//! - `FB0118` (strict): a page's `as_of` date is later than the oldest known `as_of` among the
//!   claims it cites (comparing partial dates as their first day).
//! - `FB0309` (warning): the number-unit lint — a figure outside the exempt places (`Worked example`,
//!   a `Precise:` blockquote, `Change note`, a code block or inline code span, and a citation) has
//!   no claim citation in its sentence.
//! - `FB0310` (warning): a page cites a superseded, withdrawn or quarantined source version, or a
//!   claim with `superseded_by` set.
//! - `FB0320` (strict): a `stable` page cites a consequential claim whose `verification` is not
//!   `primary_source_supported`.

use std::collections::BTreeMap;

use fieldbook_calc::{Currency, Unit};

use crate::cite::{self, Citation, KeyKind, Mode};
use crate::date::{CivilDate, PartialDate};
use crate::ids::{ClaimId, VersionId};
use crate::review::{newest_change_note_date, page_review_by};
use crate::schema::vocab::{Lifecycle, PageStatus, Verification};
use crate::schema::{ClaimRow, PageMeta};
use crate::strings;

use super::markdown::{self, Ev, Event, Tag, TagEnd};
use super::{Code, Diagnostic, Model, Options, PageEntry, Report};

/// Run the `check-citation-rules` pass over `model` and append diagnostics to `report`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    let mut claim_map: BTreeMap<ClaimId, &ClaimRow> = BTreeMap::new();
    for located in model.claims() {
        claim_map
            .entry(located.value.claim_id.clone())
            .or_insert(&located.value);
    }

    let plain_time_words: Vec<&'static str> = Unit::ALL
        .iter()
        .copied()
        .filter(|u| u.is_time() && !u.word().contains('_'))
        .map(Unit::word)
        .collect();

    for page in model.pages() {
        let cites = model.citations(&page.id);
        let cited_claim_ids = cite::claims_of(cites);
        let cited_claims: Vec<&ClaimRow> = cited_claim_ids
            .iter()
            .filter_map(|id| claim_map.get(id).copied())
            .collect();

        check_fb0117(model, page, &cited_claims, options.strict, report);
        check_fb0118(page, &cited_claims, options.strict, report);
        check_fb0310_and_fb0320(model, page, cites, &claim_map, options.strict, report);
        check_fb0309(page, &plain_time_words, options.strict, report);
    }
}

fn field_loc(meta: &PageMeta, key: &str) -> (usize, usize) {
    meta.field_location(key)
        .and_then(|(l, c)| Some((usize::try_from(l).ok()?, usize::try_from(c).ok()?)))
        .unwrap_or((1, 1))
}

fn check_fb0117(
    model: &Model,
    page: &PageEntry,
    cited_claims: &[&ClaimRow],
    strict: bool,
    report: &mut Report,
) {
    let today = model.today();
    let review_date = page_review_by(
        &page.meta,
        newest_change_note_date(&page.body),
        cited_claims.iter().copied(),
    );
    if review_date < today {
        let (line, col) = field_loc(&page.meta, "as_of");
        report.push(Diagnostic::new(
            Code::Fb0117,
            strict,
            &page.path,
            line,
            col,
            format!(
                "page `{}` `review_by` (`{review_date}`) has passed (`today = {today}`)",
                page.id
            ),
        ));
    }
}

fn check_fb0118(page: &PageEntry, cited_claims: &[&ClaimRow], strict: bool, report: &mut Report) {
    let mut oldest: Option<(&ClaimId, PartialDate, CivilDate)> = None;
    for claim in cited_claims {
        let Some(claim_as_of) = claim.as_of else {
            continue;
        };
        let day = claim_as_of.first_day();
        match oldest {
            None => oldest = Some((&claim.claim_id, claim_as_of, day)),
            Some((_, _, best_day)) if day < best_day => {
                oldest = Some((&claim.claim_id, claim_as_of, day));
            }
            _ => {}
        }
    }

    if let Some((oldest_id, oldest_raw, oldest_day)) = oldest
        && page.meta.as_of > oldest_day
    {
        let (line, col) = field_loc(&page.meta, "as_of");
        report.push(Diagnostic::new(
            Code::Fb0118,
            strict,
            &page.path,
            line,
            col,
            format!(
                "page `{}` `as_of` (`{}`) is later than cited claim `{oldest_id}` `as_of` (`{oldest_raw}`)",
                page.id, page.meta.as_of
            ),
        ));
    }
}

fn check_fb0310_and_fb0320(
    model: &Model,
    page: &PageEntry,
    cites: &[Citation],
    claim_map: &BTreeMap<ClaimId, &ClaimRow>,
    strict: bool,
    report: &mut Report,
) {
    let is_stable = page.meta.status == PageStatus::Stable;
    let index = model.index();

    for cite in cites {
        match cite.key_kind {
            KeyKind::Version => {
                let Ok(vid) = VersionId::parse(&cite.key) else {
                    continue;
                };
                let Some(entry) = index.source_version(&vid) else {
                    continue;
                };
                if entry.superseded {
                    report.push(Diagnostic::new(
                        Code::Fb0310,
                        strict,
                        &page.path,
                        cite.span.line,
                        cite.span.col,
                        format!("page `{}` cites superseded source version `{vid}`", page.id),
                    ));
                } else if matches!(
                    entry.lifecycle,
                    Lifecycle::Withdrawn | Lifecycle::Quarantined
                ) {
                    report.push(Diagnostic::new(
                        Code::Fb0310,
                        strict,
                        &page.path,
                        cite.span.line,
                        cite.span.col,
                        format!(
                            "page `{}` cites {} source version `{vid}`",
                            page.id, entry.lifecycle
                        ),
                    ));
                }
            }
            KeyKind::Claim => {
                let Ok(cid) = ClaimId::parse(&cite.key) else {
                    continue;
                };
                let Some(claim) = claim_map.get(&cid) else {
                    continue;
                };
                if !claim.superseded_by.is_empty() {
                    report.push(Diagnostic::new(
                        Code::Fb0310,
                        strict,
                        &page.path,
                        cite.span.line,
                        cite.span.col,
                        format!(
                            "page `{}` cites superseded claim `{cid}` (`superseded_by` is set)",
                            page.id
                        ),
                    ));
                }
                if is_stable
                    && claim.consequential
                    && claim.verification != Verification::PrimarySourceSupported
                {
                    report.push(Diagnostic::new(
                        Code::Fb0320,
                        strict,
                        &page.path,
                        cite.span.line,
                        cite.span.col,
                        format!(
                            "`stable` page `{}` cites consequential claim `{cid}` with `verification = {}`",
                            page.id, claim.verification
                        ),
                    ));
                }
            }
            KeyKind::Question | KeyKind::Conflict | KeyKind::Note => {}
        }
    }
}

/// Checks `FB0309`: the number-unit lint.
///
/// A *figure* is a number with:
/// - a currency sign (`$`, `€`, `£`, `¥`) or 3-letter currency code (`USD`, `EUR`, `GBP`, `CAD`, `AUD`, `JPY`, `CHF`),
/// - a percent sign (`%`),
/// - a unit word (`days`, `weeks`, `months`, `years`, `hours`, `dollars`, `million`, `billion`, `thousand`, `percent`),
/// - or a `K`, `M` or `B` suffix.
///
/// Exempt places:
/// - `## Worked example`
/// - a `Precise:` blockquote
/// - `## Change note`
/// - a code block or inline code span
/// - a citation
/// - level-2/3 headings
fn check_fb0309(page: &PageEntry, plain_time_words: &[&str], strict: bool, report: &mut Report) {
    let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
    let evs = markdown::events(&page.body, base_line);
    let parts = page.parts();

    let worked_example_heading = strings::parts()[3];
    let change_note_heading = strings::parts()[7];
    let precise_label = "Precise:";

    for part in parts.parts() {
        if part.heading == worked_example_heading || part.heading == change_note_heading {
            continue;
        }
        let part_evs = &evs[part.event_range.clone()];
        check_part_blocks_for_fb0309(
            &page.path,
            part_evs,
            precise_label,
            plain_time_words,
            strict,
            report,
        );
    }
}

#[derive(Debug, Clone)]
struct TextChunk {
    text: String,
    line: usize,
    col: usize,
    is_code: bool,
}

fn check_part_blocks_for_fb0309(
    page_path: &str,
    part_evs: &[Ev<'_>],
    precise_label: &str,
    plain_time_words: &[&str],
    strict: bool,
    report: &mut Report,
) {
    let mut i = 0usize;
    while i < part_evs.len() {
        match &part_evs[i].event {
            Event::Start(Tag::CodeBlock(_)) => {
                i += 1;
                while i < part_evs.len()
                    && !matches!(part_evs[i].event, Event::End(TagEnd::CodeBlock))
                {
                    i += 1;
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                let bq_start = i;
                let mut depth = 1usize;
                i += 1;
                while i < part_evs.len() && depth > 0 {
                    match &part_evs[i].event {
                        Event::Start(Tag::BlockQuote(_)) => depth += 1,
                        Event::End(TagEnd::BlockQuote(_)) => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                let bq_slice = &part_evs[bq_start..i];
                if is_precise_blockquote(bq_slice, precise_label) {
                    continue;
                }
                check_part_blocks_for_fb0309(
                    page_path,
                    &bq_slice[1..bq_slice.len().saturating_sub(1)],
                    precise_label,
                    plain_time_words,
                    strict,
                    report,
                );
                continue;
            }
            Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::TableRow | Tag::TableHead) => {
                let is_para = matches!(part_evs[i].event, Event::Start(Tag::Paragraph));
                let is_heading = matches!(part_evs[i].event, Event::Start(Tag::Heading { .. }));
                let is_row = matches!(part_evs[i].event, Event::Start(Tag::TableRow));
                let mut chunks = Vec::new();
                i += 1;
                while i < part_evs.len() {
                    match &part_evs[i].event {
                        Event::End(TagEnd::Paragraph) if is_para => break,
                        Event::End(TagEnd::Heading(_)) if is_heading => break,
                        Event::End(TagEnd::TableRow) if is_row => break,
                        Event::End(TagEnd::TableHead) if !is_para && !is_heading && !is_row => {
                            break;
                        }
                        Event::End(TagEnd::TableCell) if !is_para && !is_heading => {
                            chunks.push(TextChunk {
                                text: " ".to_owned(),
                                line: part_evs[i].line,
                                col: part_evs[i].col,
                                is_code: false,
                            });
                        }
                        Event::Start(Tag::CodeBlock(_)) => {
                            i += 1;
                            while i < part_evs.len()
                                && !matches!(part_evs[i].event, Event::End(TagEnd::CodeBlock))
                            {
                                i += 1;
                            }
                        }
                        Event::Text(t) => {
                            chunks.push(TextChunk {
                                text: t.to_string(),
                                line: part_evs[i].line,
                                col: part_evs[i].col,
                                is_code: false,
                            });
                        }
                        Event::Code(c) => {
                            chunks.push(TextChunk {
                                text: format!("`{c}`"),
                                line: part_evs[i].line,
                                col: part_evs[i].col,
                                is_code: true,
                            });
                        }
                        Event::SoftBreak | Event::HardBreak => {
                            chunks.push(TextChunk {
                                text: " ".to_owned(),
                                line: part_evs[i].line,
                                col: part_evs[i].col,
                                is_code: false,
                            });
                        }
                        _ => {}
                    }
                    i += 1;
                }
                check_block_chunks(page_path, &chunks, plain_time_words, strict, report);
            }
            Event::Start(Tag::Item) => {
                // Collect direct inline text of the list item before any nested block (Paragraph, List, BlockQuote, CodeBlock).
                let mut chunks = Vec::new();
                i += 1;
                while i < part_evs.len() {
                    match &part_evs[i].event {
                        Event::End(TagEnd::Item) => break,
                        Event::Start(
                            Tag::Paragraph
                            | Tag::List(_)
                            | Tag::BlockQuote(_)
                            | Tag::CodeBlock(_)
                            | Tag::Table(_),
                        ) => {
                            // Flush any direct inline chunks collected before the nested block, then let the outer loop handle the nested block.
                            if !chunks.is_empty() {
                                check_block_chunks(
                                    page_path,
                                    &chunks,
                                    plain_time_words,
                                    strict,
                                    report,
                                );
                                chunks.clear();
                            }
                            break;
                        }
                        Event::Text(t) => {
                            chunks.push(TextChunk {
                                text: t.to_string(),
                                line: part_evs[i].line,
                                col: part_evs[i].col,
                                is_code: false,
                            });
                        }
                        Event::Code(c) => {
                            chunks.push(TextChunk {
                                text: format!("`{c}`"),
                                line: part_evs[i].line,
                                col: part_evs[i].col,
                                is_code: true,
                            });
                        }
                        Event::SoftBreak | Event::HardBreak => {
                            chunks.push(TextChunk {
                                text: " ".to_owned(),
                                line: part_evs[i].line,
                                col: part_evs[i].col,
                                is_code: false,
                            });
                        }
                        _ => {}
                    }
                    i += 1;
                }
                if !chunks.is_empty() {
                    check_block_chunks(page_path, &chunks, plain_time_words, strict, report);
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
}

fn is_precise_blockquote(bq_evs: &[Ev<'_>], precise_label: &str) -> bool {
    let mut text = String::new();
    for ev in bq_evs {
        match &ev.event {
            Event::Text(t) | Event::Code(t) => {
                text.push_str(t);
                if text.trim_start().starts_with(precise_label) {
                    return true;
                }
            }
            _ => {}
        }
    }
    text.trim_start().starts_with(precise_label)
}

fn check_block_chunks(
    page_path: &str,
    chunks: &[TextChunk],
    plain_time_words: &[&str],
    strict: bool,
    report: &mut Report,
) {
    if chunks.is_empty() {
        return;
    }

    // Build combined string and per-byte position + exemption flags.
    let mut combined = String::new();
    let mut byte_pos: Vec<(usize, usize)> = Vec::new();
    let mut byte_exempt: Vec<bool> = Vec::new();

    for chunk in chunks {
        let mut cur_line = chunk.line;
        let mut cur_col = chunk.col;
        for ch in chunk.text.chars() {
            let ch_len = ch.len_utf8();
            for _ in 0..ch_len {
                byte_pos.push((cur_line, cur_col));
                byte_exempt.push(chunk.is_code);
            }
            combined.push(ch);
            if ch == '\n' {
                cur_line += 1;
                cur_col = 1;
            } else {
                cur_col += 1;
            }
        }
    }

    // Mark any `[...]` citation spans inside `combined` as exempt from figure scanning.
    let lexed = cite::lex(&combined, Mode::Tolerant);
    let mut bracket_ranges = Vec::new();
    let c_bytes = combined.as_bytes();
    for c in &lexed.citations {
        // Expand to enclosing `[` and `]` around `c.span` only when `c.span` is an item-only span.
        let open = if c_bytes.get(c.span.start) == Some(&b'[') {
            c.span.start
        } else {
            combined[..c.span.start].rfind('[').unwrap_or(c.span.start)
        };
        let close = if c.span.end > 0 && c_bytes.get(c.span.end - 1) == Some(&b']') {
            c.span.end
        } else {
            combined[c.span.end..]
                .find(']')
                .map_or(c.span.end, |off| c.span.end + off + 1)
        };
        bracket_ranges.push((open, close));
    }
    for f in &lexed.findings {
        bracket_ranges.push((f.span.start, f.span.end));
    }
    for (s, e) in bracket_ranges {
        for flag in byte_exempt
            .iter_mut()
            .take(e.min(combined.len()))
            .skip(s.min(combined.len()))
        {
            *flag = true;
        }
    }

    // Split `combined` into sentences:
    // "A sentence ends at `.`, `!` or `?` followed by whitespace or the end of the block;
    // a decimal point does not end one."
    // Also be sure abbreviations inside citations (`p. 14`, `pp. 14-15`, `sec. 2`, `para. 3`) do not split a sentence!
    for (sent_start, sent_end) in split_sentences(&combined, &byte_exempt) {
        let sentence = &combined[sent_start..sent_end];
        let strict_lex = cite::lex(sentence, Mode::Strict);
        let has_claim_citation = !strict_lex.findings.is_empty()
            || strict_lex
                .citations
                .iter()
                .any(|c| c.key_kind == KeyKind::Claim);
        if has_claim_citation {
            continue;
        }

        if let Some((fig_rel_offset, fig_text)) = find_first_figure(
            sentence,
            &byte_exempt[sent_start..sent_end],
            plain_time_words,
        ) {
            let abs_offset = sent_start + fig_rel_offset;
            let (line, col) = byte_pos.get(abs_offset).copied().unwrap_or((1, 1));
            report.push(Diagnostic::new(
                Code::Fb0309,
                strict,
                page_path,
                line,
                col,
                format!("figure `{fig_text}` has no claim citation in its sentence"),
            ));
        }
    }
}

fn split_sentences(text: &str, byte_exempt: &[bool]) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut sentences = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < len {
        // Do not split inside an exempt span (code span or `[...]` citation).
        if byte_exempt.get(i).copied().unwrap_or(false) {
            i += 1;
            continue;
        }
        let b = bytes[i];
        if matches!(b, b'.' | b'!' | b'?') {
            let at_end = i + 1 >= len;
            let followed_by_ws = !at_end
                && text[i + 1..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace);
            // A decimal point (`3.5`) has a digit before and after, so `followed_by_ws` is already false.
            if at_end || followed_by_ws {
                let end = i + 1;
                sentences.push((start, end));
                i = end;
                while i < len {
                    let Some(ch) = text[i..].chars().next() else {
                        break;
                    };
                    if ch.is_whitespace() {
                        i += ch.len_utf8();
                    } else {
                        break;
                    }
                }
                start = i;
                continue;
            }
        }
        i += 1;
    }

    if start < len {
        sentences.push((start, len));
    }
    sentences
}

const SCALE_AND_MONEY_WORDS: &[&str] = &["dollars", "million", "billion", "thousand", "percent"];

fn is_currency_sign(ch: char) -> bool {
    matches!(ch, '$' | '€' | '£' | '¥')
}

/// Scans `sentence` (skipping indices where `exempt[idx]` is `true`) for the first figure:
/// - a currency sign (`$`, `€`, `£`, `¥`) followed by a number (e.g. `$100`, `$5.2M`, `$10 million`),
/// - a 3-letter currency code followed by a number (e.g. `USD 100,000`),
/// - or a number followed by:
///   - `%` (e.g. `15%`),
///   - `K`, `M`, or `B` suffix (with optional currency/unit word, e.g. `5M`, `$5M`, `100K`),
///   - a currency code (`100,000 USD`),
///   - or a unit word (`days`, `weeks`, `months`, `years`, `hours`, `dollars`, `million`, `billion`, `thousand`, `percent`), optionally joined by a hyphen (`30-day` does not use the plural word unless written `30-days`, wait: let's check exact unit words!).
fn find_first_figure(
    sentence: &str,
    exempt: &[bool],
    plain_time_words: &[&str],
) -> Option<(usize, String)> {
    let bytes = sentence.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    while i < len {
        if exempt.get(i).copied().unwrap_or(false) {
            let ch_len = sentence[i..].chars().next().map_or(1, char::len_utf8);
            i += ch_len;
            continue;
        }

        let Some(ch) = sentence[i..].chars().next() else {
            break;
        };

        // Case 1: Currency sign before number (`$100`, `€50.5`, `$5M`).
        if is_currency_sign(ch) {
            let sign_start = i;
            let after_sign = i + ch.len_utf8();
            let num_start = skip_optional_spaces(sentence, after_sign);
            if let Some(num_end) = scan_number_token(sentence, exempt, num_start) {
                let mut fig_end = num_end;
                if let Some(suf_end) = scan_kmb_or_percent_suffix(sentence, fig_end) {
                    fig_end = suf_end;
                }
                if let Some(word_end) =
                    scan_following_unit_or_currency(sentence, exempt, fig_end, plain_time_words)
                {
                    fig_end = word_end;
                }
                return Some((sign_start, sentence[sign_start..fig_end].to_owned()));
            }
        }

        // Case 2: 3-letter currency code before number (`USD 100,000`).
        if ch.is_ascii_uppercase()
            && is_word_boundary_before(sentence, i)
            && let Some(code_end) = scan_currency_code(sentence, i)
        {
            let after_ws = skip_spaces_at_least_one(sentence, code_end);
            if let Some(num_start) = after_ws
                && let Some(num_end) = scan_number_token(sentence, exempt, num_start)
            {
                let mut fig_end = num_end;
                if let Some(suf_end) = scan_kmb_or_percent_suffix(sentence, fig_end) {
                    fig_end = suf_end;
                }
                if let Some(word_end) =
                    scan_following_unit_or_currency(sentence, exempt, fig_end, plain_time_words)
                {
                    fig_end = word_end;
                }
                return Some((i, sentence[i..fig_end].to_owned()));
            }
        }

        // Case 3: Number token at word/token boundary.
        if ch.is_ascii_digit()
            && is_number_boundary_before(sentence, i)
            && let Some(num_end) = scan_number_token(sentence, exempt, i)
        {
            // Make sure the number is not immediately followed by identifier characters like `-v2`, `.101` (multi-dot clause number), etc.
            if is_clause_or_date_continuation(sentence, i, num_end) {
                i = num_end;
                continue;
            }

            // 3a: Immediate `%` or `K`/`M`/`B` suffix.
            if let Some(suf_end) = scan_kmb_or_percent_suffix(sentence, num_end) {
                let mut fig_end = suf_end;
                if let Some(word_end) =
                    scan_following_unit_or_currency(sentence, exempt, fig_end, plain_time_words)
                {
                    fig_end = word_end;
                }
                return Some((i, sentence[i..fig_end].to_owned()));
            }

            // 3b: Followed by whitespace (or hyphen) and a unit word or currency code or currency sign.
            if let Some(word_end) =
                scan_following_unit_or_currency(sentence, exempt, num_end, plain_time_words)
            {
                return Some((i, sentence[i..word_end].to_owned()));
            }
        }

        i += ch.len_utf8();
    }

    None
}

fn skip_optional_spaces(s: &str, mut idx: usize) -> usize {
    let bytes = s.as_bytes();
    while idx < bytes.len() && matches!(bytes[idx], b' ' | b'\t') {
        idx += 1;
    }
    idx
}

fn skip_spaces_at_least_one(s: &str, idx: usize) -> Option<usize> {
    let next = skip_optional_spaces(s, idx);
    (next > idx).then_some(next)
}

fn is_word_boundary_before(s: &str, idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    s[..idx]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-')
}

fn is_number_boundary_before(s: &str, idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    s[..idx]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric() && !matches!(c, '_' | '-' | '.' | '/'))
}

/// Scans a number starting at `start` (`15`, `100,000`, `3.5`, `1,250.75`).
fn scan_number_token(s: &str, exempt: &[bool], start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if start >= len || exempt.get(start).copied().unwrap_or(false) || !bytes[start].is_ascii_digit()
    {
        return None;
    }

    let mut i = start;
    while i < len && !exempt.get(i).copied().unwrap_or(false) && bytes[i].is_ascii_digit() {
        i += 1;
    }

    // Optional `,DDD` groups.
    while i + 3 < len
        && bytes[i] == b','
        && bytes[i + 1].is_ascii_digit()
        && bytes[i + 2].is_ascii_digit()
        && bytes[i + 3].is_ascii_digit()
        && (i + 4 >= len || !bytes[i + 4].is_ascii_digit())
    {
        i += 4;
    }

    // Optional `.D+` decimal part (only one dot; if followed by another `.D`, it's a clause number).
    if i + 1 < len && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
        let mut d = i + 1;
        while d < len && bytes[d].is_ascii_digit() {
            d += 1;
        }
        i = d;
    }

    Some(i)
}

fn is_clause_or_date_continuation(s: &str, start: usize, end: usize) -> bool {
    let bytes = s.as_bytes();
    if end < bytes.len() {
        let next = bytes[end];
        // E.g. `2026-02-01` or `15.404-1` or `52.204-21`
        if next == b'.' && end + 1 < bytes.len() && bytes[end + 1].is_ascii_digit() {
            return true;
        }
        if next == b'-' && end + 1 < bytes.len() && bytes[end + 1].is_ascii_digit() {
            return true;
        }
    }
    let _ = start;
    false
}

fn scan_kmb_or_percent_suffix(s: &str, idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if idx >= bytes.len() {
        return None;
    }
    match bytes[idx] {
        b'%' => Some(idx + 1),
        b'K' | b'M' | b'B' => {
            let after = idx + 1;
            let boundary_ok = s[after..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');
            boundary_ok.then_some(after)
        }
        _ => None,
    }
}

fn scan_currency_code(s: &str, idx: usize) -> Option<usize> {
    if idx + 3 > s.len() || !s.is_char_boundary(idx) || !s.is_char_boundary(idx + 3) {
        return None;
    }
    let candidate = &s[idx..idx + 3];
    if Currency::parse(candidate).is_ok()
        && s[idx + 3..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-')
    {
        Some(idx + 3)
    } else {
        None
    }
}

fn scan_following_unit_or_currency(
    s: &str,
    exempt: &[bool],
    after_num: usize,
    plain_time_words: &[&str],
) -> Option<usize> {
    let word_start = skip_spaces_at_least_one(s, after_num)?;
    if word_start >= s.len() || exempt.get(word_start).copied().unwrap_or(false) {
        return None;
    }

    // Check currency code after number (`100,000 USD`).
    if let Some(code_end) = scan_currency_code(s, word_start) {
        return Some(code_end);
    }

    // Scan lowercase/alphabetic word.
    let mut word_end = word_start;
    for ch in s[word_start..].chars() {
        if ch.is_ascii_alphabetic() {
            word_end += ch.len_utf8();
        } else {
            break;
        }
    }
    if word_end == word_start {
        return None;
    }
    // Check trailing boundary on the word.
    if s[word_end..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }

    let word = &s[word_start..word_end];
    let lower = word.to_ascii_lowercase();
    let is_unit_word = plain_time_words.contains(&lower.as_str())
        || SCALE_AND_MONEY_WORDS.contains(&lower.as_str());
    if !is_unit_word {
        return None;
    }

    // If the word was a scale word (`million`, `billion`, `thousand`) followed by `dollars` or a currency code, include that too.
    if matches!(lower.as_str(), "million" | "billion" | "thousand")
        && let Some(next_start) = skip_spaces_at_least_one(s, word_end)
    {
        if let Some(code_end) = scan_currency_code(s, next_start) {
            return Some(code_end);
        }
        let mut next_end = next_start;
        for ch in s[next_start..].chars() {
            if ch.is_ascii_alphabetic() {
                next_end += ch.len_utf8();
            } else {
                break;
            }
        }
        if next_end > next_start
            && s[next_start..next_end].eq_ignore_ascii_case("dollars")
            && s[next_end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        {
            return Some(next_end);
        }
    }

    Some(word_end)
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Severity, Tree, run};
    use crate::date::CivilDate;

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
    fn fb0117_page_review_by_passed() {
        let mut tree = mini_tree();
        let stale_page = PAGE_CMP_SYSTEM_REVIEW
            .replace("as_of = 2026-02-01", "as_of = 2025-07-01")
            .replace(
                "- NEW_KNOWLEDGE, 2026-02-01:",
                "- NEW_KNOWLEDGE, 2025-07-01:",
            );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, stale_page.as_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0117);
        assert_eq!(report.diagnostics[0].severity, Severity::Warning);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_SYSTEM_REVIEW);
        assert_eq!(report.diagnostics[0].line, 10);
    }

    #[test]
    fn fb0117_counts_from_newest_change_note_entry() {
        let mut tree = mini_tree();
        // `as_of` is old, but the page was last changed on 2026-02-01 (V2, 180 days): not overdue
        // on 2026-03-01. Counting from `as_of` alone would be overdue since 2025-12-28.
        let page = PAGE_CMP_SYSTEM_REVIEW.replace("as_of = 2026-02-01", "as_of = 2025-07-01");
        assert!(page.contains("- NEW_KNOWLEDGE, 2026-02-01:"));
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, page.as_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report.diagnostics.iter().all(|d| d.code != Code::Fb0117),
            "{:?}",
            report.diagnostics
        );
    }

    #[test]
    fn fb0117_never_fails_strict() {
        let mut tree = mini_tree();
        let stale_page = PAGE_CMP_SYSTEM_REVIEW
            .replace("as_of = 2026-02-01", "as_of = 2025-07-01")
            .replace(
                "- NEW_KNOWLEDGE, 2026-02-01:",
                "- NEW_KNOWLEDGE, 2025-07-01:",
            );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, stale_page.as_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0117);
        assert_eq!(report.diagnostics[0].severity, Severity::Warning);
        assert_eq!(report.errors(), 0);
        assert_eq!(report.warnings(), 1);
    }

    #[test]
    fn fb0118_page_as_of_later_than_oldest_cited_claim_and_partial_date_first_day() {
        let mut tree = mini_tree();
        // Set CL-CMP-0002 `as_of` to partial date `2026-01` (compares as `2026-01-01`), while
        // `cmp-system-review` has `as_of = 2026-02-01`.
        let claims = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-01,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());

        let report_non_strict = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::new(false),
        );
        assert_eq!(
            report_non_strict.diagnostics.len(),
            1,
            "{:?}",
            report_non_strict.diagnostics
        );
        assert_eq!(report_non_strict.diagnostics[0].code, Code::Fb0118);
        assert_eq!(report_non_strict.diagnostics[0].severity, Severity::Warning);

        let report_strict = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert_eq!(report_strict.diagnostics.len(), 1);
        assert_eq!(report_strict.diagnostics[0].code, Code::Fb0118);
        assert_eq!(report_strict.diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn unknown_claim_date_is_not_invented() {
        // 1. An allowed stale claim with no `as_of` (and `as_of unknown` in `note`):
        // When a page cites only that claim, no date comparison is possible -> no FB0118.
        let mut tree = mini_tree();
        let claims_stale_no_date = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,text_section,sec. 2,Synthetic withholding threshold is 5 percent.,true,codex,unchecked,pending_company_confirmation,proposed,observed,V2,5,percent,2026-01-15,,,,,,,,,,,,,,Q-001,Synthetic unchecked consequential fact for fixture tests.",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,,S-0001,1,text_section,sec. 2,Synthetic withholding threshold is 5 percent.,true,codex,stale,pending_company_confirmation,proposed,observed,V2,5,percent,2026-01-15,,,,,,,,,,,,,,Q-001,as_of unknown: synthetic stale claim without as_of.",
        );
        tree.insert_bytes(
            "corpus/registers/claims.csv",
            claims_stale_no_date.as_bytes(),
        );
        let report_clean = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::new(true),
        );
        assert!(
            report_clean.diagnostics.is_empty(),
            "expected no FB0118 when cited claim has unknown as_of and second claim is 2026-02-01, got {:?}",
            report_clean.diagnostics
        );

        // 2. When the second, dated claim (`CL-CMP-0001`) has `as_of = 2026-01-15` (older than page `2026-02-01`),
        // FB0118 compares against the known dated claim (`CL-CMP-0001`).
        let claims_with_older_dated = claims_stale_no_date.replace(
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,2026-02-01,",
            "CL-CMP-0001,cmp,fact,Synthetic compliance reviews occur every 30 days under the synthetic rule.,2026-01-15,",
        );
        tree.insert_bytes(
            "corpus/registers/claims.csv",
            claims_with_older_dated.as_bytes(),
        );
        let report_dated = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0118: Vec<_> = report_dated
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0118)
            .collect();
        assert_eq!(fb0118.len(), 2, "{:?}", report_dated.diagnostics);
        assert!(fb0118.iter().all(|d| d.message.contains("CL-CMP-0001")));
    }

    #[test]
    fn fb0309_number_unit_lint_and_exempt_places() {
        let mut tree = mini_tree();
        // Add uncited figure in `## How it works`, plus figures inside exempt places:
        // - `Precise:` blockquote
        // - inline code and fenced code block
        // - `## Worked example` (already has `100,000 USD` and `5 percent` uncited in fixture!)
        // - decimal point `3.5 percent` inside one sentence with a claim citation at the end.
        let modified = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Synthetic reviewers check control operation and record open items before billing runs.",
            "The synthetic threshold is 15 percent. A decimal rate of 3.5 percent is cited [@CL-CMP-0002].\n\n> Precise: The exact statutory withhold cap is 10 percent.\n\n```text\nthreshold = 25 percent\n```\n\nInline `40 days` is code.",
        );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, modified.as_bytes());

        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, Code::Fb0309);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.path, PATH_CMP_SYSTEM_REVIEW);
        assert_eq!(d.line, 28);
        assert!(d.message.contains("15 percent"), "{}", d.message);

        // Verify non-exempt subheading (`### ...`), non-USD currency code (`100 NZD`),
        // and uncited sentence bracketed by neighboring citations (`[@Q-001] ... [@Q-001]`).
        let mut tree2 = mini_tree();
        let modified2 = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Synthetic reviewers check control operation and record open items before billing runs.",
            "### Withholding threshold: 15 percent\n\nContext [@Q-001]. The threshold is 100 NZD. Context [@Q-001].",
        );
        tree2.insert_bytes(PATH_CMP_SYSTEM_REVIEW, modified2.as_bytes());
        let report2 = run(&Host::new(tree2, sample_today()), &Options::new(false));
        assert_eq!(report2.diagnostics.len(), 2, "{:?}", report2.diagnostics);
        assert_eq!(report2.diagnostics[0].code, Code::Fb0309);
        assert!(report2.diagnostics[0].message.contains("15 percent"));
        assert_eq!(report2.diagnostics[1].code, Code::Fb0309);
        assert!(report2.diagnostics[1].message.contains("100 NZD"));
    }

    #[test]
    fn fb0310_superseded_withdrawn_or_quarantined_version_or_superseded_claim() {
        // 1. Page cites superseded claim `CL-EST-0002` (`superseded_by = CL-EST-0003`, `as_of = 2026-01-15`).
        let mut tree = mini_tree();
        let page_sup_claim = PAGE_CMP_SYSTEM_REVIEW
            .replace("as_of = 2026-02-01", "as_of = 2026-01-15")
            .replace("[@CL-CMP-0002]", "[@CL-EST-0002]");
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, page_sup_claim.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0310);
        assert_eq!(report.diagnostics[0].severity, Severity::Warning);

        // 2. Page cites superseded source version `S-0002-v1`.
        let mut tree2 = mini_tree();
        let page_sup_src = PAGE_CMP_SYSTEM_REVIEW
            .replace("[@Q-001; @XC-01]", "[@Q-001; @XC-01; @S-0002-v1, sec. 1]");
        tree2.insert_bytes(PATH_CMP_SYSTEM_REVIEW, page_sup_src.as_bytes());
        let report2 = run(&Host::new(tree2, sample_today()), &Options::new(false));
        assert_eq!(report2.diagnostics.len(), 1, "{:?}", report2.diagnostics);
        assert_eq!(report2.diagnostics[0].code, Code::Fb0310);
    }

    #[test]
    fn fb0320_is_an_error_only_under_strict() {
        let mut tree = mini_tree();
        // `cmp-system-review` cites `CL-CMP-0002` (consequential = true, verification = unchecked).
        // Promote `cmp-system-review` to `status = "stable"`.
        let stable_page =
            PAGE_CMP_SYSTEM_REVIEW.replace("status = \"draft\"", "status = \"stable\"");
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, stable_page.as_bytes());

        let report_non_strict = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::new(false),
        );
        assert_eq!(
            report_non_strict.diagnostics.len(),
            1,
            "{:?}",
            report_non_strict.diagnostics
        );
        assert_eq!(report_non_strict.diagnostics[0].code, Code::Fb0320);
        assert_eq!(report_non_strict.diagnostics[0].severity, Severity::Warning);
        assert_eq!(report_non_strict.errors(), 0);
        assert_eq!(report_non_strict.warnings(), 1);

        let report_strict = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert_eq!(report_strict.diagnostics.len(), 1);
        assert_eq!(report_strict.diagnostics[0].code, Code::Fb0320);
        assert_eq!(report_strict.diagnostics[0].severity, Severity::Error);
        assert_eq!(report_strict.errors(), 1);
        assert_eq!(report_strict.warnings(), 0);
    }
}

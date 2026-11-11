//! Markdown (`RichText`) rendering for Corpus page parts (`pulldown_cmark`),
//! citation markers, relative `.md` page links, first-occurrence glossary term links,
//! `Precise:` blocks, and `first_sentence_of_purpose`.

use std::collections::BTreeSet;
use std::ops::Range;

use fieldbook_core::check::PageEntry;
use fieldbook_core::cite::{self, Citation, KeyKind};
use fieldbook_core::egress::LabelledView;
use fieldbook_core::ids::{ClaimId, ConflictId, QuestionId, SourceId, TermId, VersionId};
use fieldbook_core::schema::{ClaimRow, ConflictRow, GlossaryRow, QuestionRow, SourceRow};
use fieldbook_core::strings;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::links::{
    escape_html, href_if_planned, render_conflict_chip, render_page_link, render_question_chip,
    render_term_link,
};
use crate::site::PlannedPaths;

pub(crate) fn find_claim<'a>(view: &'a LabelledView, id: &ClaimId) -> Option<&'a ClaimRow> {
    let slice = view.claims();
    if let Some(&line) = view.index().claims.get(id)
        && let Ok(idx) = slice.binary_search_by_key(&line, |c| c.line)
        && slice[idx].value.claim_id == *id
    {
        return Some(&slice[idx].value);
    }
    if let Ok(idx) = slice.binary_search_by(|c| c.value.claim_id.cmp(id)) {
        return Some(&slice[idx].value);
    }
    slice
        .iter()
        .find(|c| c.value.claim_id == *id)
        .map(|c| &c.value)
}

pub(crate) fn find_source<'a>(
    view: &'a LabelledView,
    sid: &SourceId,
    ver: u32,
) -> Option<&'a SourceRow> {
    let slice = view.sources();
    if let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}"))
        && let Some(entry) = view.index().source_versions.get(&vid)
        && let Ok(idx) = slice.binary_search_by_key(&entry.line, |s| s.line)
        && slice[idx].value.source_id == *sid
        && slice[idx].value.version == ver
    {
        return Some(&slice[idx].value);
    }
    if let Ok(idx) = slice.binary_search_by(|s| {
        s.value
            .source_id
            .cmp(sid)
            .then_with(|| s.value.version.cmp(&ver))
    }) {
        return Some(&slice[idx].value);
    }
    slice
        .iter()
        .find(|s| s.value.source_id == *sid && s.value.version == ver)
        .map(|s| &s.value)
}

pub(crate) fn find_question<'a>(
    view: &'a LabelledView,
    qid: &QuestionId,
) -> Option<&'a QuestionRow> {
    let slice = view.questions();
    if let Some(&line) = view.index().questions.get(qid)
        && let Ok(idx) = slice.binary_search_by_key(&line, |q| q.line)
        && slice[idx].value.question_id == *qid
    {
        return Some(&slice[idx].value);
    }
    if let Ok(idx) = slice.binary_search_by(|q| q.value.question_id.cmp(qid)) {
        return Some(&slice[idx].value);
    }
    slice
        .iter()
        .find(|q| q.value.question_id == *qid)
        .map(|q| &q.value)
}

pub(crate) fn find_conflict<'a>(
    view: &'a LabelledView,
    cid: &ConflictId,
) -> Option<&'a ConflictRow> {
    let slice = view.conflicts();
    if let Some(&line) = view.index().conflicts.get(cid)
        && let Ok(idx) = slice.binary_search_by_key(&line, |c| c.line)
        && slice[idx].value.conflict_id == *cid
    {
        return Some(&slice[idx].value);
    }
    if let Ok(idx) = slice.binary_search_by(|c| c.value.conflict_id.cmp(cid)) {
        return Some(&slice[idx].value);
    }
    slice
        .iter()
        .find(|c| c.value.conflict_id == *cid)
        .map(|c| &c.value)
}

pub(crate) fn find_glossary<'a>(view: &'a LabelledView, tid: &TermId) -> Option<&'a GlossaryRow> {
    let slice = view.glossary();
    if let Some(&line) = view.index().terms.get(tid)
        && let Ok(idx) = slice.binary_search_by_key(&line, |g| g.line)
        && slice[idx].value.term_id == *tid
    {
        return Some(&slice[idx].value);
    }
    if let Ok(idx) = slice.binary_search_by(|g| g.value.term_id.cmp(tid)) {
        return Some(&slice[idx].value);
    }
    slice
        .iter()
        .find(|g| g.value.term_id == *tid)
        .map(|g| &g.value)
}

pub(crate) fn find_page<'a>(view: &'a LabelledView, page_id: &str) -> Option<&'a PageEntry> {
    let slice = view.pages();
    if let Ok(idx) = slice.binary_search_by(|p| p.id.as_str().cmp(page_id)) {
        return Some(&slice[idx]);
    }
    slice.iter().find(|p| p.id.as_str() == page_id)
}

/// A numbered citation marker (`1..=K`) emitted in reading order by [`InlineRenderer`]
/// and paired 1-to-1 with a row in [`crate::sources_block`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationMarker {
    /// 1-based marker number (`1..=K`).
    pub number: usize,
    /// Target of the numbered citation (a claim or a direct source version).
    pub target: CitationTarget,
}

/// Target of a numbered [`CitationMarker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitationTarget {
    /// A cited claim (`[@CL-...]`).
    Claim(ClaimId),
    /// A directly cited source version (`[@S-nnnn-v<k> ...]`) with optional locator text.
    Version {
        /// Version identifier (`S-nnnn-v<k>`).
        version_id: VersionId,
        /// Optional locator written in the citation (`p. 12`, `sec. ...`, etc.).
        locator: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct BracketGroup {
    open_byte: usize,
    close_byte: usize,
    citations: Vec<Citation>,
}

fn build_bracket_groups(body: &str, citations: &[Citation]) -> Vec<BracketGroup> {
    let bytes = body.as_bytes();
    let mut groups: Vec<BracketGroup> = Vec::new();
    for cite in citations {
        let open_byte = if bytes.get(cite.span.start) == Some(&b'[') {
            cite.span.start
        } else {
            body[..cite.span.start]
                .rfind('[')
                .unwrap_or(cite.span.start)
        };
        let close_byte = if cite.span.end > open_byte
            && bytes.get(cite.span.end.saturating_sub(1)) == Some(&b']')
        {
            cite.span.end
        } else {
            body[cite.span.end..]
                .find(']')
                .map_or(cite.span.end, |rel| cite.span.end + rel + 1)
        };

        if let Some(last) = groups.last_mut()
            && last.open_byte == open_byte
            && last.close_byte == close_byte
        {
            last.citations.push(cite.clone());
            continue;
        }
        groups.push(BracketGroup {
            open_byte,
            close_byte,
            citations: vec![cite.clone()],
        });
    }
    groups
}

fn is_word_boundary_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn find_whole_word_prelowered(
    haystack: &str,
    h_lower: &str,
    needle: &str,
    n_lower: &str,
) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    if h_lower.len() != haystack.len() || n_lower.len() != needle.len() {
        // Fallback for non-ASCII case folding: scan char indices in haystack.
        let n_chars: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
        if n_chars.is_empty() {
            return None;
        }
        let h_indices: Vec<(usize, char)> = haystack.char_indices().collect();
        for start_idx in 0..h_indices.len() {
            let start_byte = h_indices[start_idx].0;
            if start_byte > 0
                && haystack[..start_byte]
                    .chars()
                    .next_back()
                    .is_some_and(is_word_boundary_char)
            {
                continue;
            }
            let mut matched = Vec::new();
            let mut end_byte = start_byte;
            for &(b_idx, ch) in &h_indices[start_idx..] {
                end_byte = b_idx + ch.len_utf8();
                matched.extend(ch.to_lowercase());
                if matched.len() >= n_chars.len() {
                    break;
                }
            }
            if matched == n_chars {
                let after_ok = haystack[end_byte..]
                    .chars()
                    .next()
                    .is_none_or(|c| !is_word_boundary_char(c));
                if after_ok {
                    return Some((start_byte, end_byte));
                }
            }
        }
        return None;
    }

    let mut search_from = 0usize;
    while let Some(rel) = h_lower[search_from..].find(n_lower) {
        let start = search_from + rel;
        let end = start + n_lower.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_boundary_char(c));
        let after_ok = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_boundary_char(c));
        if before_ok && after_ok {
            return Some((start, end));
        }
        search_from = start + 1;
        while search_from < h_lower.len() && !h_lower.is_char_boundary(search_from) {
            search_from += 1;
        }
    }
    None
}

pub(crate) fn parse_md_link_target(dest_url: &str) -> Option<(&str, Option<&str>)> {
    if dest_url.contains("://") || dest_url.starts_with('#') || dest_url.starts_with('/') {
        return None;
    }
    let (path_part, frag_opt) = match dest_url.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (dest_url, None),
    };
    let file_name = match path_part.rfind('/') {
        Some(idx) => &path_part[idx + 1..],
        None => path_part,
    };
    let stem = file_name.strip_suffix(".md")?;
    if stem.is_empty() {
        return None;
    }
    Some((stem, frag_opt))
}

fn decode_body_range(
    body: &str,
    segments: &[(&str, Range<usize>)],
    start: usize,
    end: usize,
) -> String {
    if start >= end {
        return String::new();
    }
    let mut out = String::new();
    for (dec, r) in segments {
        if r.end <= start || r.start >= end {
            continue;
        }
        if r.start >= start && r.end <= end {
            out.push_str(dec);
        } else {
            let s = start.max(r.start);
            let e = end.min(r.end);
            if s < e {
                out.push_str(&body[s..e]);
            }
        }
    }
    out
}

/// Extract the plain-text first sentence of the `Purpose` (or `Question`) part of `page`
/// for popover and area summaries.
#[must_use]
pub fn first_sentence_of_purpose(page: &PageEntry) -> String {
    let purpose_name = strings::parts()[0];
    let question_name = strings::brief_parts()[0];
    let part = page
        .parts()
        .get(purpose_name)
        .or_else(|| page.parts().get(question_name))
        .or_else(|| {
            page.parts()
                .parts()
                .iter()
                .find(|p| !p.event_range.is_empty())
        });
    let Some(part) = part else {
        return String::new();
    };
    if part.event_range.is_empty() {
        return String::new();
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);

    enum ParaEvent {
        Text(Range<usize>, String),
        Code(String),
        Break,
    }

    let mut text_ranges: Vec<Range<usize>> = Vec::new();
    let mut para_events: Vec<ParaEvent> = Vec::new();
    let mut in_first_para = false;
    for (ev_idx, (event, range)) in Parser::new_ext(&page.body, opts)
        .into_offset_iter()
        .enumerate()
    {
        if ev_idx >= part.event_range.end {
            break;
        }
        if ev_idx < part.event_range.start {
            continue;
        }
        match event {
            Event::Start(Tag::Paragraph) => {
                in_first_para = true;
            }
            Event::End(TagEnd::Paragraph) if in_first_para => {
                break;
            }
            Event::Text(cow) if in_first_para => {
                text_ranges.push(range.clone());
                para_events.push(ParaEvent::Text(range, cow.into_string()));
            }
            Event::Code(code) if in_first_para => {
                para_events.push(ParaEvent::Code(code.into_string()));
            }
            Event::SoftBreak | Event::HardBreak if in_first_para => {
                para_events.push(ParaEvent::Break);
            }
            _ => {}
        }
    }

    // The parser splits `[` and `@CL-...` into separate text events, so look across the whole span.
    let has_cite = match (text_ranges.first(), text_ranges.last()) {
        (Some(first), Some(last)) => page.body[first.start..last.end].contains("[@"),
        _ => false,
    };
    let groups = if has_cite {
        let max_end = text_ranges.last().map_or(0, |r| r.end);
        let slice_end = page.body[max_end..]
            .find('\n')
            .map_or(page.body.len(), |nl| max_end + nl);
        let body_prefix = &page.body[..slice_end];
        let citations =
            cite::lex_from_line(body_prefix, page.body_line as usize, cite::Mode::Strict).citations;
        build_bracket_groups(body_prefix, &citations)
    } else {
        Vec::new()
    };

    let mut para_text = String::new();
    let mut idx = 0usize;
    while idx < para_events.len() {
        if matches!(para_events[idx], ParaEvent::Text(_, _)) {
            let mut segments: Vec<(&str, Range<usize>)> = Vec::new();
            let mut text_start = 0usize;
            let mut text_end = 0usize;
            while idx < para_events.len() {
                let ParaEvent::Text(ref r, ref dec) = para_events[idx] else {
                    break;
                };
                if segments.is_empty() {
                    text_start = r.start;
                    text_end = r.end;
                } else if r.start == text_end {
                    text_end = r.end;
                } else {
                    break;
                }
                segments.push((dec.as_str(), r.clone()));
                idx += 1;
            }
            let mut cursor = text_start;
            for g in &groups {
                if g.open_byte >= text_start && g.close_byte <= text_end && g.open_byte >= cursor {
                    para_text.push_str(&decode_body_range(
                        &page.body,
                        &segments,
                        cursor,
                        g.open_byte,
                    ));
                    cursor = g.close_byte;
                }
            }
            if cursor < text_end {
                para_text.push_str(&decode_body_range(&page.body, &segments, cursor, text_end));
            }
            continue;
        }
        match &para_events[idx] {
            ParaEvent::Code(code) => para_text.push_str(code),
            ParaEvent::Break => para_text.push(' '),
            ParaEvent::Text(_, _) => {}
        }
        idx += 1;
    }

    let collapsed = para_text.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = collapsed
        .replace(" .", ".")
        .replace(" ,", ",")
        .replace(" ;", ";")
        .replace(" :", ":");
    let trimmed = cleaned.trim();
    if let Some(dot_idx) = trimmed.find(". ") {
        return trimmed[..=dot_idx].to_owned();
    }
    trimmed.to_owned()
}

struct TermEntry<'a> {
    term_id: TermId,
    glossary_row: Option<&'a GlossaryRow>,
    candidates: Vec<(String, String)>,
}

/// Stateful renderer across the parts of a single page so that [`CitationMarker`]s number
/// `1..=K` in reading order and each front-matter `terms` entry links on its first occurrence only.
pub struct InlineRenderer<'a> {
    from_path: &'a str,
    page: &'a PageEntry,
    view: &'a LabelledView,
    planned: &'a PlannedPaths,
    groups: Vec<BracketGroup>,
    events: Vec<(usize, Event<'a>, Range<usize>)>,
    term_entries: Vec<TermEntry<'a>>,
    linked_terms: BTreeSet<TermId>,
    markers: Vec<CitationMarker>,
}

impl<'a> InlineRenderer<'a> {
    /// Construct a new [`InlineRenderer`] for `page` at site path `from_path` (`p/<page-id>.html`).
    #[must_use]
    pub fn new(
        from_path: &'a str,
        page: &'a PageEntry,
        view: &'a LabelledView,
        planned: &'a PlannedPaths,
    ) -> Self {
        let cites_slice = view.citations(&page.id);
        let groups = build_bracket_groups(&page.body, cites_slice);
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        opts.insert(Options::ENABLE_TASKLISTS);
        let events: Vec<(usize, Event<'a>, Range<usize>)> = Parser::new_ext(&page.body, opts)
            .into_offset_iter()
            .enumerate()
            .map(|(ev_idx, (ev, r))| (ev_idx, ev, r))
            .collect();
        let mut term_entries = Vec::with_capacity(page.meta.terms.len());
        for term_id in &page.meta.terms {
            let glossary_row = find_glossary(view, term_id);
            let mut candidates: Vec<(String, String)> = Vec::new();
            if let Some(g) = glossary_row {
                candidates.push((g.label.clone(), g.label.to_lowercase()));
                for alias in &g.aliases {
                    candidates.push((alias.clone(), alias.to_lowercase()));
                }
            } else {
                let fallback = term_id.as_str().replace('_', " ");
                let fallback_lower = fallback.to_lowercase();
                candidates.push((fallback, fallback_lower));
            }
            term_entries.push(TermEntry {
                term_id: term_id.clone(),
                glossary_row,
                candidates,
            });
        }
        Self {
            from_path,
            page,
            view,
            planned,
            groups,
            events,
            term_entries,
            linked_terms: BTreeSet::new(),
            markers: Vec::new(),
        }
    }

    /// Consume the renderer and return all numbered [`CitationMarker`]s in reading order.
    #[must_use]
    pub fn into_markers(self) -> Vec<CitationMarker> {
        self.markers
    }

    /// Return the slice of numbered [`CitationMarker`]s collected so far.
    #[must_use]
    pub fn markers(&self) -> &[CitationMarker] {
        &self.markers
    }

    /// Render the Markdown events within `event_range` of `page.body` to HTML.
    pub fn render_part(&mut self, event_range: Range<usize>) -> String {
        let start_ev = event_range.start.min(self.events.len());
        let end_ev = event_range.end.min(self.events.len());
        if start_ev >= end_ev {
            return String::new();
        }
        let all_events = self.events[start_ev..end_ev].to_vec();

        let mut out = String::new();
        let mut blockquote_precise_stack: Vec<bool> = Vec::new();
        let mut strip_precise_prefix = false;
        let mut in_heading = false;
        let mut in_code_block = false;
        let mut in_table_head = false;
        let mut active_link: Option<(String, String)> = None;

        let mut idx = 0usize;
        while idx < all_events.len() {
            let (ev_idx, ref event, ref range) = all_events[idx];

            // Coalesce contiguous Event::Text slices so citation spans never split across events,
            // while retaining pulldown-cmark's decoded text for entities and escapes.
            if let Event::Text(ref first_cow) = *event {
                let mut text_start = range.start;
                let mut text_end = range.end;
                let mut segments: Vec<(&str, Range<usize>)> =
                    vec![(first_cow.as_ref(), range.clone())];
                idx += 1;
                while idx < all_events.len() {
                    if let Event::Text(ref next_cow) = all_events[idx].1
                        && all_events[idx].2.start == text_end
                    {
                        text_end = all_events[idx].2.end;
                        segments.push((next_cow.as_ref(), all_events[idx].2.clone()));
                        idx += 1;
                    } else {
                        break;
                    }
                }

                if strip_precise_prefix {
                    let raw = &self.page.body[text_start..text_end];
                    let leading_ws = raw.len() - raw.trim_start().len();
                    let trimmed = &raw[leading_ws..];
                    if let Some(after_p) = trimmed.strip_prefix("Precise:") {
                        let after_ws = after_p.len() - after_p.trim_start_matches(' ').len();
                        text_start += leading_ws + "Precise:".len() + after_ws;
                    }
                    strip_precise_prefix = false;
                }

                let allow_cites = !in_code_block;
                let allow_terms = !in_code_block && !in_heading && active_link.is_none();
                let rendered = self.render_text_span(
                    &segments,
                    text_start,
                    text_end,
                    allow_cites,
                    allow_terms,
                );
                if let Some((_, ref mut link_buf)) = active_link {
                    link_buf.push_str(&rendered);
                } else {
                    out.push_str(&rendered);
                }
                continue;
            }

            let target_buf = if let Some((_, ref mut link_buf)) = active_link {
                link_buf
            } else {
                &mut out
            };

            match event {
                Event::Start(Tag::Paragraph) => target_buf.push_str("<p>"),
                Event::End(TagEnd::Paragraph) => target_buf.push_str("</p>\n"),
                Event::Start(Tag::Heading { level, .. }) => {
                    in_heading = true;
                    let tag = match level {
                        HeadingLevel::H1 => "h1",
                        HeadingLevel::H2 => "h2",
                        HeadingLevel::H3 => "h3",
                        HeadingLevel::H4 => "h4",
                        HeadingLevel::H5 => "h5",
                        HeadingLevel::H6 => "h6",
                    };
                    if let Some(anchor) = self.page.parts().heading_anchor(ev_idx) {
                        target_buf.push_str(&format!("<{tag} id=\"{}\">", escape_html(anchor)));
                    } else {
                        target_buf.push_str(&format!("<{tag}>"));
                    }
                }
                Event::End(TagEnd::Heading(level)) => {
                    in_heading = false;
                    let tag = match level {
                        HeadingLevel::H1 => "h1",
                        HeadingLevel::H2 => "h2",
                        HeadingLevel::H3 => "h3",
                        HeadingLevel::H4 => "h4",
                        HeadingLevel::H5 => "h5",
                        HeadingLevel::H6 => "h6",
                    };
                    target_buf.push_str(&format!("</{tag}>\n"));
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    let is_precise = Self::blockquote_starts_with_precise(
                        &self.page.body,
                        &all_events[idx + 1..],
                    );
                    blockquote_precise_stack.push(is_precise);
                    if is_precise {
                        strip_precise_prefix = true;
                        target_buf.push_str(
                            "<div class=\"precise\"><span class=\"eyebrow\">Precise</span>",
                        );
                    } else {
                        target_buf.push_str("<blockquote>");
                    }
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    strip_precise_prefix = false;
                    let is_precise = blockquote_precise_stack.pop().unwrap_or(false);
                    if is_precise {
                        target_buf.push_str("</div>\n");
                    } else {
                        target_buf.push_str("</blockquote>\n");
                    }
                }
                Event::Start(Tag::List(Some(1))) => target_buf.push_str("<ol>\n"),
                Event::Start(Tag::List(Some(start))) => {
                    target_buf.push_str(&format!("<ol start=\"{start}\">\n"));
                }
                Event::Start(Tag::List(None)) => target_buf.push_str("<ul>\n"),
                Event::End(TagEnd::List(true)) => target_buf.push_str("</ol>\n"),
                Event::End(TagEnd::List(false)) => target_buf.push_str("</ul>\n"),
                Event::Start(Tag::Item) => target_buf.push_str("<li>"),
                Event::End(TagEnd::Item) => target_buf.push_str("</li>\n"),
                Event::Start(Tag::Emphasis) => target_buf.push_str("<em>"),
                Event::End(TagEnd::Emphasis) => target_buf.push_str("</em>"),
                Event::Start(Tag::Strong) => target_buf.push_str("<strong>"),
                Event::End(TagEnd::Strong) => target_buf.push_str("</strong>"),
                Event::Start(Tag::Strikethrough) => target_buf.push_str("<del>"),
                Event::End(TagEnd::Strikethrough) => target_buf.push_str("</del>"),
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                    target_buf.push_str("<pre><code>");
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                    target_buf.push_str("</code></pre>\n");
                }
                Event::Start(Tag::Table(_)) => {
                    target_buf.push_str("<div class=\"table-wrap\"><table>\n");
                }
                Event::End(TagEnd::Table) => {
                    target_buf.push_str("</tbody></table></div>\n");
                }
                Event::Start(Tag::TableHead) => {
                    in_table_head = true;
                    target_buf.push_str("<thead><tr>");
                }
                Event::End(TagEnd::TableHead) => {
                    in_table_head = false;
                    target_buf.push_str("</tr></thead>\n<tbody>\n");
                }
                Event::Start(Tag::TableRow) => target_buf.push_str("<tr>"),
                Event::End(TagEnd::TableRow) => target_buf.push_str("</tr>\n"),
                Event::Start(Tag::TableCell) => {
                    if in_table_head {
                        target_buf.push_str("<th scope=\"col\">");
                    } else {
                        target_buf.push_str("<td>");
                    }
                }
                Event::End(TagEnd::TableCell) => {
                    if in_table_head {
                        target_buf.push_str("</th>");
                    } else {
                        target_buf.push_str("</td>");
                    }
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    active_link = Some((dest_url.to_string(), String::new()));
                }
                Event::End(TagEnd::Link) => {
                    if let Some((dest_url, link_inner)) = active_link.take() {
                        let rendered_link = self.finish_link(&dest_url, &link_inner);
                        out.push_str(&rendered_link);
                    }
                }
                Event::Code(code) => {
                    target_buf.push_str(&format!("<code>{}</code>", escape_html(code)));
                }
                Event::TaskListMarker(checked) => {
                    if *checked {
                        target_buf.push_str("<input type=\"checkbox\" checked disabled> ");
                    } else {
                        target_buf.push_str("<input type=\"checkbox\" disabled> ");
                    }
                }
                Event::SoftBreak => target_buf.push('\n'),
                Event::HardBreak => target_buf.push_str("<br>\n"),
                Event::Rule => target_buf.push_str("<hr>\n"),
                _ => {}
            }

            idx += 1;
        }

        out
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

    fn finish_link(&self, dest_url: &str, link_inner: &str) -> String {
        if let Some((page_id, fragment)) = parse_md_link_target(dest_url) {
            let target_page = find_page(self.view, page_id);
            let pv_title = target_page.map(|p| p.meta.title.as_str());
            let pv_summary = target_page.map(first_sentence_of_purpose);
            return render_page_link(
                self.from_path,
                page_id,
                fragment,
                link_inner,
                pv_title,
                pv_summary.as_deref(),
                self.planned,
            );
        }
        if dest_url.starts_with('#') {
            return format!("<a href=\"{}\">{link_inner}</a>", escape_html(dest_url));
        }
        let site_target = dest_url.trim_start_matches("../").trim_start_matches('/');
        match href_if_planned(self.from_path, site_target, self.planned) {
            Some(href) => format!("<a href=\"{}\">{link_inner}</a>", escape_html(&href)),
            None => format!("<span>{link_inner}</span>"),
        }
    }

    fn render_text_span(
        &mut self,
        segments: &[(&str, Range<usize>)],
        start: usize,
        end: usize,
        allow_cites: bool,
        allow_terms: bool,
    ) -> String {
        if start >= end {
            return String::new();
        }
        if !allow_cites {
            return escape_html(&decode_body_range(&self.page.body, segments, start, end));
        }

        let matching_indices: Vec<usize> = self
            .groups
            .iter()
            .enumerate()
            .filter_map(|(i, g)| (g.open_byte >= start && g.close_byte <= end).then_some(i))
            .collect();

        let mut out = String::new();
        let mut cursor = start;
        for g_idx in matching_indices {
            let open_byte = self.groups[g_idx].open_byte;
            let close_byte = self.groups[g_idx].close_byte;
            if open_byte < cursor {
                continue;
            }
            if open_byte > cursor {
                let plain = decode_body_range(&self.page.body, segments, cursor, open_byte);
                out.push_str(&self.render_plain_segment(&plain, allow_terms));
            }
            let cites = self.groups[g_idx].citations.clone();
            for (c_idx, cite) in cites.iter().enumerate() {
                if c_idx > 0 {
                    out.push(' ');
                }
                out.push_str(&self.render_single_citation(cite));
            }
            cursor = close_byte;
        }
        if cursor < end {
            let tail = decode_body_range(&self.page.body, segments, cursor, end);
            out.push_str(&self.render_plain_segment(&tail, allow_terms));
        }
        out
    }

    fn render_single_citation(&mut self, cite: &Citation) -> String {
        match cite.key_kind {
            KeyKind::Claim => {
                let Ok(claim_id) = ClaimId::parse(&cite.key) else {
                    return escape_html(&cite.key);
                };
                let n = self.markers.len() + 1;
                let claim_row = find_claim(self.view, &claim_id);
                let pv_title = format!("{claim_id}");
                let pv_body = claim_row.map(|c| c.statement.as_str()).unwrap_or("");
                self.markers.push(CitationMarker {
                    number: n,
                    target: CitationTarget::Claim(claim_id),
                });
                format!(
                    "<a class=\"cite-n\" href=\"#cite-{n}\" data-pv=\"cite:{n}\" data-pv-title=\"{}\" data-pv-body=\"{}\" aria-label=\"Source {n}\">{n}</a>",
                    escape_html(&pv_title),
                    escape_html(pv_body)
                )
            }
            KeyKind::Version => {
                let Ok(version_id) = VersionId::parse(&cite.key) else {
                    return escape_html(&cite.key);
                };
                let n = self.markers.len() + 1;
                let source_row = find_source(self.view, &version_id.source(), version_id.version());
                let locator_text = cite.locator.as_ref().map(|loc| loc.text.clone());
                let pv_title = match &locator_text {
                    Some(loc) => format!("{version_id} {loc}"),
                    None => version_id.to_string(),
                };
                let pv_body = source_row.map(|s| s.title.as_str()).unwrap_or("");
                self.markers.push(CitationMarker {
                    number: n,
                    target: CitationTarget::Version {
                        version_id,
                        locator: locator_text,
                    },
                });
                format!(
                    "<a class=\"cite-n\" href=\"#cite-{n}\" data-pv=\"cite:{n}\" data-pv-title=\"{}\" data-pv-body=\"{}\" aria-label=\"Source {n}\">{n}</a>",
                    escape_html(&pv_title),
                    escape_html(pv_body)
                )
            }
            KeyKind::Question => {
                let Ok(qid) = QuestionId::parse(&cite.key) else {
                    return escape_html(&cite.key);
                };
                let q_row = find_question(self.view, &qid);
                render_question_chip(
                    self.from_path,
                    &qid,
                    q_row.map(|q| q.question.as_str()),
                    self.planned,
                )
            }
            KeyKind::Conflict => {
                let Ok(cid) = ConflictId::parse(&cite.key) else {
                    return escape_html(&cite.key);
                };
                let c_row = find_conflict(self.view, &cid);
                render_conflict_chip(
                    self.from_path,
                    &cid,
                    c_row.map(|c| c.topic.as_str()),
                    self.planned,
                )
            }
            KeyKind::Note => escape_html(&cite.key),
        }
    }

    fn render_plain_segment(&mut self, text: &str, allow_terms: bool) -> String {
        if !allow_terms
            || text.is_empty()
            || self.term_entries.is_empty()
            || self.linked_terms.len() >= self.term_entries.len()
        {
            return escape_html(text);
        }

        let text_lower = text.to_lowercase();

        // Find the earliest match in `text` among unlinked front-matter `terms`.
        let mut best: Option<(usize, usize, TermId, Option<&'a GlossaryRow>)> = None;
        for entry in &self.term_entries {
            if self.linked_terms.contains(&entry.term_id) {
                continue;
            }
            for (phrase, phrase_lower) in &entry.candidates {
                if let Some((m_start, m_end)) =
                    find_whole_word_prelowered(text, &text_lower, phrase, phrase_lower)
                {
                    let replace = match &best {
                        None => true,
                        Some((b_start, b_end, _, _)) => {
                            m_start < *b_start
                                || (m_start == *b_start && (m_end - m_start) > (*b_end - *b_start))
                        }
                    };
                    if replace {
                        best = Some((m_start, m_end, entry.term_id.clone(), entry.glossary_row));
                    }
                }
            }
        }

        let Some((m_start, m_end, term_id, glossary_row)) = best else {
            return escape_html(text);
        };

        self.linked_terms.insert(term_id.clone());
        let mut out = escape_html(&text[..m_start]);
        out.push_str(&render_term_link(
            self.from_path,
            &term_id,
            &text[m_start..m_end],
            glossary_row,
            self.planned,
        ));
        out.push_str(&self.render_plain_segment(&text[m_end..], allow_terms));
        out
    }
}

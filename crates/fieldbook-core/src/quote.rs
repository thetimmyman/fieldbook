//! Pure quote matching, text normalisation, and locator resolution.
//!
//! **Responsibility.** Pure quote search across extracted pages ([`find`]), offset-preserving
//! text normalisation ([`normalise`]), and locator-to-page-range resolution ([`locator_resolves`]).
//! Each line-end hyphen is tried independently (kept with break as space, kept and joined,
//! removed and joined, in that order) and the leftmost match on the first page in range is returned.
//!
//! **Rules.** Pure: no I/O, clock, environment, or process access. Never panics on arbitrary
//! Unicode inputs.

use std::ops::RangeInclusive;

use unicode_normalization::UnicodeNormalization;

use crate::check::Code;
use crate::extraction::ExtractionToml;
use crate::schema::locator::Locator;

/// Single-quote code points folded to ASCII single quote `'`.
const SINGLE_QUOTES: &[char] = &[
    '\u{2018}', // Left single quotation mark
    '\u{2019}', // Right single quotation mark
    '\u{201A}', // Single low-9 quotation mark
    '\u{201B}', // Single high-reversed-9 quotation mark
    '\u{2032}', // Prime
];

/// Double-quote code points folded to ASCII double quote `"`.
const DOUBLE_QUOTES: &[char] = &[
    '\u{201C}', // Left double quotation mark
    '\u{201D}', // Right double quotation mark
    '\u{201E}', // Double low-9 quotation mark
    '\u{201F}', // Double high-reversed-9 quotation mark
    '\u{2033}', // Double prime
];

/// Dash and minus code points folded to ASCII hyphen-minus `-`.
const DASHES: &[char] = &[
    '\u{058A}', // Armenian hyphen
    '\u{05BE}', // Hebrew punctuation maqaf
    '\u{1400}', // Canadian syllabics hyphen
    '\u{1806}', // Mongolian todo soft hyphen
    '\u{2010}', // Hyphen
    '\u{2011}', // Non-breaking hyphen
    '\u{2012}', // Figure dash
    '\u{2013}', // En dash
    '\u{2014}', // Em dash
    '\u{2015}', // Horizontal bar
    '\u{2212}', // Minus sign
    '\u{2E17}', // Double oblique hyphen
    '\u{2E1A}', // Hyphen with diaeresis
    '\u{2E3A}', // Two-em dash
    '\u{2E3B}', // Three-em dash
    '\u{2E40}', // Double hyphen
    '\u{2E5D}', // Oblique hyphen
    '\u{FE31}', // Presentation form for vertical em dash
    '\u{FE32}', // Presentation form for vertical en dash
    '\u{FE58}', // Small em dash
    '\u{FE63}', // Small hyphen-minus
    '\u{FF0D}', // Fullwidth hyphen-minus
];

fn is_single_quote(c: char) -> bool {
    c == '\'' || SINGLE_QUOTES.contains(&c)
}

fn is_double_quote(c: char) -> bool {
    c == '"' || DOUBLE_QUOTES.contains(&c)
}

fn is_dash(c: char) -> bool {
    c == '-' || DASHES.contains(&c)
}

fn fold_char(c: char) -> char {
    if is_single_quote(c) {
        '\''
    } else if is_double_quote(c) {
        '"'
    } else if is_dash(c) {
        '-'
    } else {
        c
    }
}

/// Where a quote was found: the absolute page number (from 1) and the byte range of the match in that page's ORIGINAL text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Absolute 1-based page number where the quote was matched.
    pub page: u32,
    /// Byte offset in the original page text where the match begins.
    pub start: usize,
    /// Byte offset in the original page text where the match ends.
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineEndHyphen {
    hyphen_start: usize,
    hyphen_end: usize,
    break_end: usize,
}

fn find_line_end_hyphens(text: &str) -> Vec<LineEndHyphen> {
    let mut hyphens = Vec::new();
    let mut prev_char: Option<char> = None;

    for (h_start, ch) in text.char_indices() {
        let is_prev_alphanumeric = prev_char.is_some_and(char::is_alphanumeric);
        prev_char = Some(ch);

        if !is_prev_alphanumeric || !is_dash(ch) {
            continue;
        }

        let h_end = h_start.saturating_add(ch.len_utf8());
        let Some(after_hyphen) = text.get(h_end..) else {
            continue;
        };

        // Optional spaces/tabs before line break
        let spaces_len = after_hyphen
            .chars()
            .take_while(|&c| c == ' ' || c == '\t')
            .map(char::len_utf8)
            .sum::<usize>();

        let Some(at_break) = after_hyphen.get(spaces_len..) else {
            continue;
        };

        let break_len = if at_break.starts_with("\r\n") {
            2
        } else if at_break.starts_with('\n') || at_break.starts_with('\r') {
            1
        } else {
            continue;
        };

        let Some(after_break) = at_break.get(break_len..) else {
            continue;
        };

        // Optional whitespace (indentation)
        let indent_len = after_break
            .chars()
            .take_while(|&c| c.is_whitespace() && c != '\n' && c != '\r')
            .map(char::len_utf8)
            .sum::<usize>();

        let Some(after_indent) = after_break.get(indent_len..) else {
            continue;
        };

        // Must be followed by a non-whitespace char
        let Some(next_ch) = after_indent.chars().next() else {
            continue;
        };
        if next_ch.is_whitespace() {
            continue;
        }

        let break_end = h_end
            .saturating_add(spaces_len)
            .saturating_add(break_len)
            .saturating_add(indent_len);

        hyphens.push(LineEndHyphen {
            hyphen_start: h_start,
            hyphen_end: h_end,
            break_end,
        });
    }

    hyphens
}

struct NormalisedOutput {
    text: String,
    ranges: Vec<(usize, usize)>,
}

struct WhitespaceState {
    out: NormalisedOutput,
    pending_ws: Option<(usize, usize)>,
    has_emitted_non_ws: bool,
}

impl WhitespaceState {
    fn feed(&mut self, ch: char, orig_start: usize, orig_end: usize) {
        if ch.is_whitespace() {
            match &mut self.pending_ws {
                Some((_, end)) => {
                    *end = orig_end;
                }
                None => {
                    self.pending_ws = Some((orig_start, orig_end));
                }
            }
        } else {
            if self.has_emitted_non_ws {
                if let Some((ws_start, ws_end)) = self.pending_ws.take() {
                    self.out.text.push(' ');
                    self.out.ranges.push((ws_start, ws_end));
                }
            } else {
                self.pending_ws = None;
                self.has_emitted_non_ws = true;
            }
            let ch_len = ch.len_utf8();
            self.out.text.push(ch);
            for _ in 0..ch_len {
                self.out.ranges.push((orig_start, orig_end));
            }
        }
    }
}

fn process_chunk(chunk_str: &str, orig_start: usize, orig_end: usize, state: &mut WhitespaceState) {
    for ch in chunk_str.nfkc() {
        let folded = fold_char(ch);
        state.feed(folded, orig_start, orig_end);
    }
}

/// A chunk may start before `c` only when nothing earlier can compose or reorder with it:
/// the first character of its compatibility decomposition is a starter that never
/// combines backward (every ASCII character is one).
fn starts_chunk(c: char) -> bool {
    if c.is_ascii() {
        return true;
    }
    let mut first = None;
    unicode_normalization::char::decompose_compatible(c, |d| {
        if first.is_none() {
            first = Some(d);
        }
    });
    let lead = first.unwrap_or(c);
    unicode_normalization::char::canonical_combining_class(lead) == 0
        && unicode_normalization::is_nfkc_quick(std::iter::once(lead))
            == unicode_normalization::IsNormalized::Yes
}

fn process_segment(s: &str, base_offset: usize, state: &mut WhitespaceState) {
    let mut chunk_start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if starts_chunk(c) {
            if let Some(start) = chunk_start
                && let Some(chunk_str) = s.get(start..i)
            {
                process_chunk(
                    chunk_str,
                    base_offset.saturating_add(start),
                    base_offset.saturating_add(i),
                    state,
                );
            }
            chunk_start = Some(i);
        } else if chunk_start.is_none() {
            chunk_start = Some(i);
        }
    }
    if let Some(start) = chunk_start
        && let Some(chunk_str) = s.get(start..)
    {
        process_chunk(
            chunk_str,
            base_offset.saturating_add(start),
            base_offset.saturating_add(s.len()),
            state,
        );
    }
}

fn normalise_with_offsets(text: &str) -> (String, Vec<(usize, usize)>) {
    let mut state = WhitespaceState {
        out: NormalisedOutput {
            text: String::with_capacity(text.len()),
            ranges: Vec::with_capacity(text.len()),
        },
        pending_ws: None,
        has_emitted_non_ws: false,
    };

    process_segment(text, 0, &mut state);

    (state.out.text, state.out.ranges)
}

fn page_sites(text: &str, norm: &str, ranges: &[(usize, usize)]) -> Vec<usize> {
    let mut sites = Vec::new();
    let norm_bytes = norm.as_bytes();
    for h in find_line_end_hyphens(text) {
        let ni = ranges.partition_point(|r| r.0 < h.hyphen_start);
        if ranges.get(ni).map(|r| r.0) == Some(h.hyphen_start)
            && norm_bytes.get(ni) == Some(&b'-')
            && norm_bytes.get(ni.saturating_add(1)) == Some(&b' ')
        {
            sites.push(ni);
        }
    }
    sites.sort_unstable();
    sites.dedup();
    sites
}

fn walk(
    norm: &[u8],
    sites: &[usize],
    quote: &[u8],
    mut i: usize,
    mut q: usize,
    mut last: Option<usize>,
) -> Option<usize> {
    loop {
        if q == quote.len() {
            return last;
        }
        let &nb = norm.get(i)?;
        if sites.binary_search(&i).is_ok() {
            if quote.get(q) == Some(&b'-') {
                if let Some(res) = walk(
                    norm,
                    sites,
                    quote,
                    i.saturating_add(1),
                    q.saturating_add(1),
                    Some(i),
                ) {
                    return Some(res);
                }
                if let Some(res) = walk(
                    norm,
                    sites,
                    quote,
                    i.saturating_add(2),
                    q.saturating_add(1),
                    Some(i),
                ) {
                    return Some(res);
                }
            }
            if q > 0
                && let Some(res) = walk(norm, sites, quote, i.saturating_add(2), q, last)
            {
                return Some(res);
            }
            return None;
        }
        if Some(&nb) == quote.get(q) {
            let i_old = i;
            i = i.saturating_add(1);
            q = q.saturating_add(1);
            last = Some(i_old);
            continue;
        }
        return None;
    }
}

fn match_on_page(norm_quote: &str, page_text: &str, page_num: u32) -> Option<Span> {
    if norm_quote.is_empty() {
        return None;
    }
    let (norm, ranges) = normalise_with_offsets(page_text);
    let sites = page_sites(page_text, &norm, &ranges);
    let (start_idx, last_idx) = if sites.is_empty() {
        let start = norm.find(norm_quote)?;
        let last = start.checked_add(norm_quote.len())?.checked_sub(1)?;
        (start, last)
    } else {
        let first_byte = norm_quote.as_bytes().first()?;
        let norm_bytes = norm.as_bytes();
        let quote_bytes = norm_quote.as_bytes();
        let mut hit = None;
        for (i, &b) in norm_bytes.iter().enumerate() {
            if b == *first_byte
                && let Some(last) = walk(norm_bytes, &sites, quote_bytes, i, 0, None)
            {
                hit = Some((i, last));
                break;
            }
        }
        hit?
    };
    let &(start, _) = ranges.get(start_idx)?;
    let &(_, end) = ranges.get(last_idx)?;
    if start > end || !page_text.is_char_boundary(start) || !page_text.is_char_boundary(end) {
        return None;
    }
    Some(Span {
        page: page_num,
        start,
        end,
    })
}

/// Normalises `text` for quote matching according to the Corpus specification.
#[must_use]
pub fn normalise(text: &str) -> String {
    normalise_with_offsets(text).0
}

/// Searches for `quote` across `pages` restricted to the 1-based page range `within`.
///
/// Each line-end hyphen is tried independently (kept with break as space, kept and joined,
/// removed and joined, in that order) and the leftmost match on the first page in range is returned.
#[must_use]
pub fn find(quote: &str, pages: &[&str], within: RangeInclusive<u32>) -> Option<Span> {
    if within.is_empty() {
        return None;
    }
    let start_page = *within.start();
    let end_page = *within.end();
    if start_page > end_page || end_page == 0 {
        return None;
    }

    let page_start = start_page.max(1);
    let num_pages = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    if num_pages == 0 {
        return None;
    }
    let page_end = end_page.min(num_pages);
    if page_start > page_end {
        return None;
    }

    let norm_quote = normalise(quote);
    if norm_quote.is_empty() {
        return None;
    }

    for page_num in page_start..=page_end {
        let Ok(page_idx) = usize::try_from(page_num.saturating_sub(1)) else {
            continue;
        };
        let Some(&page_text) = pages.get(page_idx) else {
            continue;
        };

        if let Some(span) = match_on_page(norm_quote.as_str(), page_text, page_num) {
            return Some(span);
        }
    }

    None
}

/// Resolves a citation locator string against an extraction record.
///
/// # Errors
///
/// Returns [`Code::Fb0304`] when a page number exceeds `extraction.page_count`
/// or when a section label cannot be found in `extraction.locators`.
pub fn locator_resolves(
    extraction: &ExtractionToml,
    locator: &str,
) -> Result<Option<RangeInclusive<u32>>, Code> {
    let Ok(parsed) = Locator::parse(locator) else {
        return Ok(None);
    };

    match parsed {
        Locator::Page(page) => {
            if page < 1 || page > extraction.page_count {
                return Err(Code::Fb0304);
            }
            Ok(Some(page..=page))
        }
        Locator::PageRange { start, end } => {
            if start < 1 || end > extraction.page_count || start > end {
                return Err(Code::Fb0304);
            }
            Ok(Some(start..=end))
        }
        Locator::Section(text) => {
            let label = if let Some(rest) = text.strip_prefix("sec. ") {
                rest
            } else if let Some(rest) = text.strip_prefix("§ ") {
                rest
            } else if let Some(rest) = text.strip_prefix("para. ") {
                rest
            } else {
                text.as_str()
            };
            let norm_label: String = label.nfc().collect();
            for (key, pages) in &extraction.locators {
                let norm_key: String = key.nfc().collect();
                if norm_key == norm_label {
                    return Ok(Some(pages.first..=pages.last));
                }
            }
            Err(Code::Fb0304)
        }
        // Table locators (sheet, rec, field) are accepted and not read yet (a later card adds
        // arms that return Code::Fb0311). Extension locators resolve to unavailable (never a match).
        Locator::Sheet(_) | Locator::Record(_) | Locator::Field(_) | Locator::Extension { .. } => {
            Ok(None)
        }
    }
}

/// The unsupported-extension fact of `locator` (its kind and decoded payload), or None for any other locator or text that does not parse. An extension locator is never a successful locator or quote match.
#[must_use]
pub fn unsupported_extension(locator: &str) -> Option<crate::schema::locator::ExtensionResolution> {
    Locator::parse(locator).ok()?.resolve_extension()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_matches_private_with_offsets() {
        let sample = "The \u{201C}quick\u{201D} brown   fox\u{2014}jumped.";
        let (with_offsets, _) = normalise_with_offsets(sample);
        assert_eq!(normalise(sample), with_offsets);
    }
}

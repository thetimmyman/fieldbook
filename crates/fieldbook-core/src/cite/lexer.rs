//! Hand-written citation lexer for [`Mode::Strict`] and [`Mode::Tolerant`].
//!
//! Implements the EBNF grammar, locator kinds, "Strict writer, tolerant reader" repairs,
//! the `][@` touching trap (`FB0302`), and citation attempt detection (`FB0300`, `FB0301`,
//! `FB0305`) from `docs/reference/corpus-format.md#syntax-at-a-glance`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::check::Code;
use crate::ids::{AreaCode, ClaimId, ConflictId, QuestionId, VersionId};
use crate::schema::locator::{ExtensionResolution, Locator, LocatorKind};

/// Reading mode for the citation lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Strict Corpus writer mode (`check`): accepts only canonical citations and emits
    /// `FB0300`, `FB0301`, `FB0302` and `FB0305` findings.
    Strict,
    /// Tolerant reader mode (`cite-check` and key-presence checks): repairs allowed
    /// non-canonical deviations and marks repaired citations `non_canonical = true`.
    Tolerant,
}

/// Kind of identifier key in a citation (`version`, `claim`, `question`, `conflict`, or `note`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyKind {
    /// Source version (`S-0042-v2`).
    Version,
    /// Claim (`CL-CMP-0011`, `CL-CMP-10000`).
    Claim,
    /// Question (`Q-027`).
    Question,
    /// Conflict (`XC-14`).
    Conflict,
    /// Answer-only note reference (`<area>-note-<nnn>`, tolerant mode only).
    Note,
}

impl KeyKind {
    /// Canonical snake_case name used in golden vectors.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Claim => "claim",
            Self::Question => "question",
            Self::Conflict => "conflict",
            Self::Note => "note",
        }
    }
}

impl fmt::Display for KeyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Byte and 1-based line/column span of a citation or lexer finding inside the input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 0-based start byte offset in the input string.
    pub start: usize,
    /// 0-based end byte offset (exclusive) in the input string.
    pub end: usize,
    /// 1-based line number (incorporating `base_line`).
    pub line: usize,
    /// 1-based Unicode scalar (code-point) column on `line`.
    pub col: usize,
}

/// Parsed locator attached to a [`Citation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationLocator {
    /// The locator kind (`pdf_page_box`, `text_section`, `sheet_cells`, `record_key`, `record_field`, or `ext:<kind>`).
    pub kind: LocatorKind,
    /// Canonical locator text (`p. 14`, `pp. 14-15`, `sec. (e)(1)`, `ext <kind> <payload>`, etc.).
    pub text: String,
    /// Parsed [`Locator`] value (preserving lossless decoded UTF-8 payload for extensions).
    pub parsed: Locator,
}

impl CitationLocator {
    /// Resolves an extension locator if this locator is an [`Locator::Extension`].
    #[must_use]
    pub fn extension_resolution(&self) -> Option<ExtensionResolution> {
        self.parsed.resolve_extension()
    }
}

/// One parsed citation item from a bracketed citation group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    /// Canonical key string (`S-0042-v2`, `CL-CMP-0011`, `Q-027`, `XC-14`).
    pub key: String,
    /// Classification of `key`.
    pub key_kind: KeyKind,
    /// Optional locator following `key`.
    pub locator: Option<CitationLocator>,
    /// `true` when accepted via a tolerant repair (`Mode::Tolerant`).
    pub non_canonical: bool,
    /// Source span of the citation in the input text.
    pub span: Span,
}

impl Citation {
    /// Formats this single citation item in canonical form (without outer `[` and `]`),
    /// re-encoding extension payloads via [`Locator::to_canonical_string`].
    #[must_use]
    pub fn to_canonical_item_string(&self) -> String {
        match &self.locator {
            Some(loc) => format!("@{}, {}", self.key, loc.parsed.to_canonical_string()),
            None => format!("@{}", self.key),
        }
    }

    /// Formats this single citation as a standalone canonical bracketed citation `[...]`.
    #[must_use]
    pub fn to_canonical_bracketed_string(&self) -> String {
        format!("[{}]", self.to_canonical_item_string())
    }
}

/// A lexer diagnostic finding (`FB0300`, `FB0301`, `FB0302` or `FB0305`) with its [`Span`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexFinding {
    /// Diagnostic code (`FB0300`, `FB0301`, `FB0302` or `FB0305`).
    pub code: Code,
    /// Source span of the fault.
    pub span: Span,
    /// Human-readable description of the citation fault.
    pub message: String,
}

/// Output of [`lex`] and [`lex_from_line`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexOutcome {
    /// Parsed citations in reading order.
    pub citations: Vec<Citation>,
    /// Lexer findings in reading order.
    pub findings: Vec<LexFinding>,
}

impl LexOutcome {
    /// Formats all parsed `citations` as a single canonical bracketed group
    /// (`[@k1; @k2, loc]`), or `None` when `citations` is empty.
    #[must_use]
    pub fn canonical_group_string(&self) -> Option<String> {
        if self.citations.is_empty() {
            return None;
        }
        let items: Vec<String> = self
            .citations
            .iter()
            .map(Citation::to_canonical_item_string)
            .collect();
        Some(format!("[{}]", items.join("; ")))
    }
}

/// Lexes `text` starting at line 1 in `mode`.
#[must_use]
pub fn lex(text: &str, mode: Mode) -> LexOutcome {
    lex_from_line(text, 1, mode)
}

/// Lexes `text` in `mode`, offsetting 1-based line numbers by `base_line`.
#[must_use]
pub fn lex_from_line(text: &str, base_line: usize, mode: Mode) -> LexOutcome {
    let base_line = base_line.max(1);
    let line_starts = compute_line_starts(text);
    let excluded = compute_code_ranges(text);

    let mut citations = Vec::new();
    let mut findings = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut idx = 0usize;
    let mut prev_cite_end: Option<usize> = None;

    while idx < len {
        if let Some(&(_, end)) = excluded
            .iter()
            .find(|&&(start, end)| idx >= start && idx < end)
        {
            idx = end;
            prev_cite_end = None;
            continue;
        }

        if bytes[idx] != b'[' {
            let ch_len = text[idx..].chars().next().map_or(1, char::len_utf8);
            idx += ch_len;
            continue;
        }

        // Ignore `[[...]]` double brackets unless the inner `[` is itself examined;
        // "Rejected forms: `[S-0042@v2 p.14]`, `{CL-CMP-0012}` and `[[…]]`."
        if bytes.get(idx + 1) == Some(&b'[') {
            // Skip the entire `[[...]]` construct so it is not read as a valid citation.
            idx += 2;
            while idx < len && bytes[idx] != b']' && bytes[idx] != b'\n' {
                let ch_len = text[idx..].chars().next().map_or(1, char::len_utf8);
                idx += ch_len;
            }
            while idx < len && bytes[idx] == b']' {
                idx += 1;
            }
            prev_cite_end = None;
            continue;
        }

        // Find the extent of this bracket attempt (up to closing `]`, next `[` opener, or newline).
        let open_idx = idx;
        let (close_opt, scan_limit) = find_bracket_close(text, open_idx + 1, &excluded);

        let inside = &text[open_idx + 1..scan_limit];
        if !is_citation_attempt(inside, close_opt.is_some()) {
            idx = open_idx + 1;
            prev_cite_end = None;
            continue;
        }

        let Some(close_idx) = close_opt else {
            // Unbalanced bracket on a citation attempt -> FB0300.
            let span = make_span(text, &line_starts, base_line, open_idx, scan_limit);
            findings.push(LexFinding {
                code: Code::Fb0300,
                span,
                message: "unbalanced `[` in citation attempt".to_owned(),
            });
            idx = scan_limit;
            prev_cite_end = None;
            continue;
        };

        let bracket_end = close_idx + 1;
        let bracket_span = make_span(text, &line_starts, base_line, open_idx, bracket_end);

        // Check touching citations `][@` (or `][` where both are citation attempts and this one starts with `@`).
        if prev_cite_end == Some(open_idx) && inside.starts_with('@') {
            let touch_span = make_span(
                text,
                &line_starts,
                base_line,
                open_idx.saturating_sub(1),
                (open_idx + 2).min(len),
            );
            findings.push(LexFinding {
                code: Code::Fb0302,
                span: touch_span,
                message: "two citations touch as `][@`; combine them with `; ` inside one pair of brackets".to_owned(),
            });
        }

        if inside.is_empty() {
            findings.push(LexFinding {
                code: Code::Fb0300,
                span: bracket_span,
                message: "empty `[]` citation brackets".to_owned(),
            });
            idx = bracket_end;
            prev_cite_end = Some(bracket_end);
            continue;
        }

        // Parse the semicolon-separated items inside `[...]`.
        parse_bracket_contents(
            text,
            (open_idx, close_idx),
            &line_starts,
            base_line,
            mode,
            &mut citations,
            &mut findings,
        );

        idx = bracket_end;
        prev_cite_end = Some(bracket_end);
    }

    LexOutcome {
        citations,
        findings,
    }
}

fn parse_bracket_contents(
    text: &str,
    (open_idx, close_idx): (usize, usize),
    line_starts: &[usize],
    base_line: usize,
    mode: Mode,
    citations: &mut Vec<Citation>,
    findings: &mut Vec<LexFinding>,
) {
    let inside = &text[open_idx + 1..close_idx];
    let bracket_span = make_span(text, line_starts, base_line, open_idx, close_idx + 1);

    // Split on `;` while tracking exact byte offsets and checking canonical `; ` spacing.
    let mut item_start = 0usize;
    let inside_bytes = inside.as_bytes();
    let mut i = 0usize;
    let mut raw_items: Vec<(usize, usize, bool)> = Vec::new();

    while i < inside_bytes.len() {
        if inside_bytes[i] == b';' {
            let item_end = i;
            let canonical_sep =
                inside_bytes.get(i + 1) == Some(&b' ') && inside_bytes.get(i + 2) != Some(&b' ');
            let next_start = if inside_bytes.get(i + 1) == Some(&b' ') {
                i + 2
            } else {
                i + 1
            };
            raw_items.push((item_start, item_end, canonical_sep));
            item_start = next_start;
            i = next_start;
        } else {
            i += 1;
        }
    }
    raw_items.push((item_start, inside_bytes.len(), true));

    let mut group_cites = Vec::with_capacity(raw_items.len());
    for (rel_start, rel_end, sep_ok) in raw_items {
        let abs_start = open_idx + 1 + rel_start;
        let abs_end = open_idx + 1 + rel_end;
        let item_span = if raw_items_len_is_one(inside) {
            bracket_span
        } else {
            make_span(text, line_starts, base_line, abs_start, abs_end)
        };
        let raw_item = &inside[rel_start..rel_end];

        match parse_single_cite(raw_item, sep_ok, item_span, mode) {
            Ok(cite) => group_cites.push(cite),
            Err(finding) => {
                findings.push(finding);
            }
        }
    }

    citations.extend(group_cites);
}

fn raw_items_len_is_one(inside: &str) -> bool {
    !inside.as_bytes().contains(&b';')
}

fn parse_single_cite(
    raw_item: &str,
    sep_canonical: bool,
    span: Span,
    mode: Mode,
) -> Result<Citation, LexFinding> {
    if raw_item.is_empty() {
        return Err(LexFinding {
            code: Code::Fb0300,
            span,
            message: "empty citation item".to_owned(),
        });
    }

    // Check leading/trailing whitespace on the item.
    if raw_item.trim() != raw_item {
        return Err(LexFinding {
            code: Code::Fb0300,
            span,
            message: format!("unexpected whitespace in citation `{raw_item}`"),
        });
    }

    let (has_at, after_at) = if let Some(rest) = raw_item.strip_prefix('@') {
        (true, rest)
    } else {
        (false, raw_item)
    };

    if after_at.is_empty() {
        return Err(LexFinding {
            code: Code::Fb0300,
            span,
            message: "missing citation key after `@`".to_owned(),
        });
    }

    // Split key and optional locator.
    // Canonical separator is `, `.
    // Tolerant repair also allows a single space ` ` without a comma (e.g. `[@S-0042-v2 p. 14]`).
    let (key_str, locator_info) = split_key_and_locator(after_at);

    // Reject if the key itself contains `@` (e.g., `[S-0042@v2 p.14]`).
    if key_str.contains('@') {
        return Err(LexFinding {
            code: Code::Fb0300,
            span,
            message: format!("malformed citation key `{key_str}`"),
        });
    }

    // Parse key and its kind.
    let Some(key_kind) = parse_key_kind(key_str, mode) else {
        return Err(LexFinding {
            code: Code::Fb0300,
            span,
            message: format!("citation key `{key_str}` is not a valid identifier"),
        });
    };

    let mut non_canonical = !has_at || !sep_canonical;

    let locator = if let Some((comma_present, space_after_comma, raw_loc)) = locator_info {
        if matches!(
            key_kind,
            KeyKind::Claim | KeyKind::Question | KeyKind::Conflict | KeyKind::Note
        ) {
            return Err(LexFinding {
                code: Code::Fb0305,
                span,
                message: format!("{key_kind} citation `@{key_str}` takes no locator"),
            });
        }

        if !comma_present || !space_after_comma {
            non_canonical = true;
        }

        let (loc, loc_repaired) = parse_citation_locator(raw_loc).map_err(|code| LexFinding {
            code,
            span,
            message: format!("invalid locator `{raw_loc}` on `@{key_str}`"),
        })?;

        if loc_repaired {
            non_canonical = true;
        }

        Some(loc)
    } else {
        None
    };

    if mode == Mode::Strict && non_canonical {
        return Err(LexFinding {
            code: Code::Fb0301,
            span,
            message: format!("citation `[{raw_item}]` is not in canonical form"),
        });
    }

    Ok(Citation {
        key: key_str.to_owned(),
        key_kind,
        locator,
        non_canonical,
        span,
    })
}

/// Splits `after_at` into `(key_slice, Option<(comma_present, single_space_after_comma, raw_locator)>)`.
fn split_key_and_locator(after_at: &str) -> (&str, Option<(bool, bool, &str)>) {
    if let Some((key, rest)) = after_at.split_once(',') {
        if let Some(loc) = rest.strip_prefix(' ') {
            let single_space = !loc.starts_with(' ');
            (key, Some((true, single_space, loc.trim_start_matches(' '))))
        } else {
            (key, Some((true, false, rest)))
        }
    } else if let Some((key, rest)) = after_at.split_once(' ') {
        (key, Some((false, false, rest.trim_start_matches(' '))))
    } else {
        (after_at, None)
    }
}

fn parse_key_kind(key: &str, mode: Mode) -> Option<KeyKind> {
    if VersionId::parse(key).is_ok() {
        return Some(KeyKind::Version);
    }
    if ClaimId::parse(key).is_ok() {
        return Some(KeyKind::Claim);
    }
    if QuestionId::parse(key).is_ok() {
        return Some(KeyKind::Question);
    }
    if ConflictId::parse(key).is_ok() {
        return Some(KeyKind::Conflict);
    }
    if mode == Mode::Tolerant && is_answer_note_key(key) {
        return Some(KeyKind::Note);
    }
    None
}

/// Checks `<area>-note-<nnn>` (three lowercase ASCII letters, `-note-`, three ASCII digits).
fn is_answer_note_key(key: &str) -> bool {
    if key.len() != 12 {
        return false;
    }
    let area = &key[..3];
    let mid = &key[3..9];
    let digits = &key[9..12];
    AreaCode::parse(area).is_ok() && mid == "-note-" && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Parses a citation locator, returning `(CitationLocator, was_repaired)` or `Err(Code)`.
///
/// The allowed tolerant repairs ("Strict writer, tolerant reader") are:
/// - `p.14` -> `p. 14` (missing space after `p.`)
/// - `pp.14-15` -> `pp. 14-15` (missing space after `pp.`)
/// - Unicode dashes (`\u{2010}`..=`\u{2015}`, `\u{2212}`) in `pp. 14–15` -> `pp. 14-15`
/// - `§15.404-1` -> `§ 15.404-1` (missing space after `§`)
///
/// Extension locators (`ext <kind> <payload>`) are never repaired and must match
/// `schema::locator::Locator::parse` byte-for-byte in both modes.
fn parse_citation_locator(raw_loc: &str) -> Result<(CitationLocator, bool), Code> {
    if raw_loc.starts_with("ext ") || raw_loc == "ext" {
        let parsed = Locator::parse(raw_loc).map_err(|_| Code::Fb0300)?;
        if parsed.kind().is_overlay() {
            return Err(Code::Fb0300);
        }
        let text = parsed.to_canonical_string();
        return Ok((
            CitationLocator {
                kind: parsed.kind(),
                text,
                parsed,
            },
            false,
        ));
    }

    // Try canonical parse first.
    if let Ok(parsed) = Locator::parse(raw_loc) {
        if parsed.kind().is_overlay() {
            // `field` locators are company-store only (`r2-records`).
            return Err(Code::Fb0300);
        }
        let text = parsed.to_canonical_string();
        return Ok((
            CitationLocator {
                kind: parsed.kind(),
                text,
                parsed,
            },
            false,
        ));
    }

    // Check if one of the exact tolerant locator repairs produces a valid canonical locator.
    let mut candidate = String::new();
    let mut repaired = false;

    if let Some(rest) = raw_loc.strip_prefix("p.") {
        if !rest.starts_with(' ') {
            candidate = format!("p. {rest}");
            repaired = true;
        }
    } else if let Some(rest) = raw_loc.strip_prefix("pp.") {
        let rest_trimmed = if let Some(after_space) = rest.strip_prefix(' ') {
            after_space
        } else {
            repaired = true;
            rest
        };
        let dash_normalized = normalize_unicode_dashes(rest_trimmed);
        if dash_normalized != rest_trimmed {
            repaired = true;
        }
        candidate = format!("pp. {dash_normalized}");
    } else if let Some(rest) = raw_loc.strip_prefix('§')
        && !rest.starts_with(' ')
    {
        candidate = format!("§ {rest}");
        repaired = true;
    }

    if repaired && let Ok(parsed) = Locator::parse(&candidate) {
        if parsed.kind().is_overlay() {
            return Err(Code::Fb0300);
        }
        let text = parsed.to_canonical_string();
        return Ok((
            CitationLocator {
                kind: parsed.kind(),
                text,
                parsed,
            },
            true,
        ));
    }

    Err(Code::Fb0300)
}

fn normalize_unicode_dashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(
            ch,
            '\u{2010}'
                | '\u{2011}'
                | '\u{2012}'
                | '\u{2013}'
                | '\u{2014}'
                | '\u{2015}'
                | '\u{2212}'
        ) {
            out.push('-');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Determines whether a `[...]` (or unbalanced `[...`) bracket pair is a citation attempt:
/// "A pair of square brackets in a page body is read as a citation when it is empty,
/// holds an `@`, or starts with the shape of a key, such as `S-42` or `CL-CMP-1`.
/// Other brackets are prose."
fn is_citation_attempt(inside: &str, has_close: bool) -> bool {
    if inside.is_empty() {
        return has_close;
    }
    let trimmed = inside.trim_start();
    if trimmed.starts_with('@') || inside.contains('@') {
        return true;
    }
    starts_with_key_shape(trimmed)
}

/// Checks whether `s` starts with a citation key shape (`S-<digit>`, `CL-`, `Q-<digit>`,
/// `XC-<digit>`, or `<area>-note-<digit>`).
///
/// Specifically excludes task-list markers (`[ ]`, `[x]`, `[X]`) and ordinary Markdown link labels.
fn starts_with_key_shape(s: &str) -> bool {
    let bytes = s.as_bytes();
    if let Some(rest) = s.strip_prefix("S-") {
        return rest.as_bytes().first().is_some_and(u8::is_ascii_digit);
    }
    if let Some(rest) = s.strip_prefix("CL-") {
        return rest.as_bytes().first().is_some_and(u8::is_ascii_uppercase);
    }
    if let Some(rest) = s.strip_prefix("Q-") {
        return rest.as_bytes().first().is_some_and(u8::is_ascii_digit);
    }
    if let Some(rest) = s.strip_prefix("XC-") {
        return rest.as_bytes().first().is_some_and(u8::is_ascii_digit);
    }
    if bytes.len() >= 9
        && bytes[0].is_ascii_lowercase()
        && bytes[1].is_ascii_lowercase()
        && bytes[2].is_ascii_lowercase()
        && &bytes[3..9] == b"-note-"
    {
        return true;
    }
    false
}

/// Finds the closing `]` for the `[` at `open_idx`, stopping at a newline, another `[` that opens
/// a citation attempt, or an excluded code span.
///
/// When `text` is a Markdown link `[label](dest)` whose `label` does not contain `@` and is not empty,
/// returns `(None, start_idx)` if `starts_with_key_shape` is false, or if followed by `(`, treats
/// `[label](dest)` as a link unless `label` starts with `@` or a key shape without `(`.
fn find_bracket_close(
    text: &str,
    start_idx: usize,
    excluded: &[(usize, usize)],
) -> (Option<usize>, usize) {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = start_idx;

    while i < len {
        if excluded.iter().any(|&(s, e)| i >= s && i < e) {
            return (None, i);
        }
        let b = bytes[i];
        if b == b'\n' || b == b'\r' {
            return (None, i);
        }
        if b == b'[' {
            return (None, i);
        }
        if b == b']' {
            // If immediately followed by `(` and the bracket content does not start with `@`,
            // check if this is a standard Markdown inline link `[text](url)`.
            let inside = &text[start_idx..i];
            if bytes.get(i + 1) == Some(&b'(')
                && !inside.trim_start().starts_with('@')
                && !inside.is_empty()
            {
                // Standard Markdown link `[...](...)`, not a citation attempt!
                return (None, start_idx);
            }
            return (Some(i), i);
        }
        let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
        i += ch_len;
    }
    (None, len)
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

fn make_span(
    text: &str,
    line_starts: &[usize],
    base_line: usize,
    start: usize,
    end: usize,
) -> Span {
    let safe_start = start.min(text.len());
    let safe_end = end.max(safe_start).min(text.len());
    let line_idx = line_starts
        .partition_point(|&s| s <= safe_start)
        .saturating_sub(1);
    let line_start = line_starts[line_idx];
    let col = text[line_start..safe_start].chars().count() + 1;
    let line = base_line + line_idx;
    Span {
        start: safe_start,
        end: safe_end,
        line,
        col,
    }
}

/// Computes byte ranges `(start, end)` of fenced code blocks and inline code spans in `text`
/// so the citation lexer skips them.
fn compute_code_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut at_line_start = true;

    while i < len {
        if at_line_start {
            // Check for up to 3 leading spaces then ``` or ~~~ fence.
            let mut indent = 0usize;
            while i + indent < len && indent < 3 && bytes[i + indent] == b' ' {
                indent += 1;
            }
            let fence_pos = i + indent;
            if fence_pos + 3 <= len {
                let fch = bytes[fence_pos];
                if (fch == b'`' || fch == b'~')
                    && bytes[fence_pos + 1] == fch
                    && bytes[fence_pos + 2] == fch
                {
                    let mut fence_len = 3usize;
                    while fence_pos + fence_len < len && bytes[fence_pos + fence_len] == fch {
                        fence_len += 1;
                    }
                    // Advance to end of opening fence line.
                    let block_start = i;
                    let mut p = fence_pos + fence_len;
                    while p < len && bytes[p] != b'\n' {
                        p += 1;
                    }
                    if p < len {
                        p += 1;
                    }
                    // Scan lines until closing fence of at least `fence_len` of `fch`.
                    while p < len {
                        let line_s = p;
                        let mut line_indent = 0usize;
                        while p + line_indent < len
                            && line_indent < 3
                            && bytes[p + line_indent] == b' '
                        {
                            line_indent += 1;
                        }
                        let cpos = p + line_indent;
                        let mut close_len = 0usize;
                        while cpos + close_len < len && bytes[cpos + close_len] == fch {
                            close_len += 1;
                        }
                        while p < len && bytes[p] != b'\n' {
                            p += 1;
                        }
                        if p < len {
                            p += 1;
                        }
                        if close_len >= fence_len {
                            let rest_line = &text[cpos + close_len..p];
                            if rest_line.trim().is_empty() {
                                break;
                            }
                        }
                        let _ = line_s;
                    }
                    ranges.push((block_start, p));
                    i = p;
                    at_line_start = true;
                    continue;
                }
            }
        }

        let b = bytes[i];
        if b == b'\n' {
            at_line_start = true;
            i += 1;
            continue;
        }
        at_line_start = false;

        if b == b'`' {
            let tick_start = i;
            let mut tick_len = 1usize;
            while i + tick_len < len && bytes[i + tick_len] == b'`' {
                tick_len += 1;
            }
            let mut p = i + tick_len;
            let mut found_close = None;
            while p < len {
                if bytes[p] == b'`' {
                    let mut run = 1usize;
                    while p + run < len && bytes[p + run] == b'`' {
                        run += 1;
                    }
                    if run == tick_len {
                        found_close = Some(p + run);
                        break;
                    }
                    p += run;
                } else {
                    p += 1;
                }
            }
            if let Some(close_end) = found_close {
                ranges.push((tick_start, close_end));
                i = close_end;
                continue;
            }
            i += tick_len;
            continue;
        }

        let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
        i += ch_len;
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::locator::ExtensionResolution;

    #[test]
    fn unknown_extension_citation_round_trips() {
        // Preserve unknown extension kind and payload in both Strict and Tolerant modes,
        // including escaped delimiters (`%3B`, `%5D`, `%0A`) and UTF-8 (`%CE%B1%CE%B2`).
        let canonical =
            "[@S-0042-v2, ext future_custom_locator sec%2F%CE%B1%CE%B2%2525%3B%5D%0Aline2]";
        for mode in [Mode::Strict, Mode::Tolerant] {
            let out = lex(canonical, mode);
            assert!(
                out.findings.is_empty(),
                "expected no findings in {mode:?}, got {:?}",
                out.findings
            );
            assert_eq!(out.citations.len(), 1);
            let cite = &out.citations[0];
            assert_eq!(cite.key, "S-0042-v2");
            assert_eq!(cite.key_kind, KeyKind::Version);
            assert!(!cite.non_canonical);
            let loc = cite.locator.as_ref().expect("extension locator present");
            assert_eq!(
                loc.kind,
                LocatorKind::Extension("future_custom_locator".to_owned())
            );
            assert_eq!(
                loc.text,
                "ext future_custom_locator sec%2F%CE%B1%CE%B2%2525%3B%5D%0Aline2"
            );
            assert_eq!(
                loc.extension_resolution(),
                Some(ExtensionResolution::Unsupported {
                    kind: "future_custom_locator".to_owned(),
                    payload: "sec/αβ%25;]\nline2".to_owned(),
                })
            );
            assert_eq!(cite.to_canonical_bracketed_string(), canonical);
            assert_eq!(out.canonical_group_string().as_deref(), Some(canonical));
        }

        // Reject invalid/mismatched escapes and malformed known built-in locators in both modes.
        for bad in [
            "[@S-0042-v2, ext future_custom_locator %2f]",
            "[@S-0042-v2, ext future_custom_locator %41]",
            "[@S-0042-v2, ext future_custom_locator ]",
            "[@S-0042-v2, ext pdf_page_box %2F1]",
            "[@S-0042-v2, p. 0]",
            "[@S-0042-v2, pp. 15-14]",
            "[@S-0042-v2, sheet NoBang]",
        ] {
            for mode in [Mode::Strict, Mode::Tolerant] {
                let out = lex(bad, mode);
                assert!(
                    out.citations.is_empty(),
                    "malformed locator {bad:?} in {mode:?} must not produce a Citation"
                );
                assert_eq!(out.findings.len(), 1);
                assert_eq!(out.findings[0].code, Code::Fb0300);
            }
        }
    }
}

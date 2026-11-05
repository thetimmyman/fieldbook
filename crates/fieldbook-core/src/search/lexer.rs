//! Search identifier and word lexer (`docs/reference/search.md#the-lexer`).
//!
//! Holds the [`LEXER_SHAPES`] table in the order of `docs/reference/search.md#the-lexer`,
//! scans normalised text from left to right trying each shape in order with the word-character
//! boundary rule, emits index-side extras (`index_tokens`), and deduplicates/caps query-side
//! tokens (`query_tokens`).

use super::normalise::normalise;

/// Maximum number of query tokens retained by [`query_tokens`].
pub const MAX_QUERY_TOKENS: usize = 12;

/// One row of [`LEXER_SHAPES`], matching the table in `docs/reference/search.md#the-lexer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexerShape {
    /// 1-based row number in the reference table (`1..=11`).
    pub number: u8,
    /// The `Shape` column of the reference table.
    pub name: &'static str,
    /// The `Pattern (ASCII, after normalisation)` column of the reference table.
    pub pattern: &'static str,
    /// The `Example` column of the reference table.
    pub example: &'static str,
}

/// The eleven lexer shapes in the exact order of `docs/reference/search.md#the-lexer`.
pub const LEXER_SHAPES: [LexerShape; 11] = [
    LexerShape {
        number: 1,
        name: "Source version",
        pattern: "s-[0-9]{4}-v[0-9]+",
        example: "s-0042-v2",
    },
    LexerShape {
        number: 2,
        name: "Source",
        pattern: "s-[0-9]{4}",
        example: "s-0042",
    },
    LexerShape {
        number: 3,
        name: "Claim",
        pattern: "cl-[a-z]{3}-[0-9]{4}",
        example: "cl-cmp-0011",
    },
    LexerShape {
        number: 4,
        name: crate::strings::BRIEF_PARTS[0],
        pattern: "q-[0-9]{3}",
        example: "q-027",
    },
    LexerShape {
        number: 5,
        name: "Conflict",
        pattern: "xc-[0-9]+",
        example: "xc-14",
    },
    LexerShape {
        number: 6,
        name: "Company claim",
        pattern: "cl-x[a-z]{2}-[0-9]{6,10}",
        example: "cl-xes-000012",
    },
    LexerShape {
        number: 7,
        name: "Company versioned id",
        pattern: "[a-z]{2}-[0-9]{6,10}-v[0-9]+",
        example: "`so-000042-v2`, `cm-000012-v2`",
    },
    LexerShape {
        number: 8,
        name: "Company id",
        pattern: "[a-z]{2}-[0-9]{6,10}",
        example: "`so-000042`, `qo-000003`, `xo-000002`",
    },
    LexerShape {
        number: 9,
        name: "Clause number",
        pattern: r"[0-9]{1,4}\.[0-9]{3}(-[0-9]+)?(\([a-z0-9]{1,4}\))*",
        example: "`15.404-1(a)(4)`, `252.204-7012`, `15.404(a)`",
    },
    LexerShape {
        number: 10,
        name: "Number",
        pattern: "`[0-9]+` then any number of `.[0-9]+` or `,[0-9]{3}` not followed by a digit; the commas are removed from the token",
        example: "`1,000,000.50` becomes `1000000.50`",
    },
    LexerShape {
        number: 11,
        name: "Word",
        pattern: "A maximal run of word characters",
        example: "`contractor`, `15th`, `v2`",
    },
];

/// A single token produced by the lexer before index-side extras or query-side deduplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexedToken {
    /// The emitted token text (with commas removed for shape 10).
    pub text: String,
    /// The 1-based shape number (`1..=11`) from [`LEXER_SHAPES`] that matched this token.
    pub shape: u8,
}

/// Returns `true` when `ch` is a word character (`char::is_alphanumeric`).
#[must_use]
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric()
}

/// Normalises `input` and scans it with [`LEXER_SHAPES`], returning each matched token and its
/// shape number (`1..=11`).
#[must_use]
pub fn lex(input: &str) -> Vec<LexedToken> {
    let norm = normalise(input);
    lex_normalised(&norm)
}

/// Scans an already-normalised string with [`LEXER_SHAPES`], returning each matched token and its
/// shape number (`1..=11`).
#[must_use]
pub fn lex_normalised(norm: &str) -> Vec<LexedToken> {
    let mut out = Vec::new();
    let mut rest = norm;

    while let Some(ch) = rest.chars().next() {
        if !is_word_char(ch) {
            rest = &rest[ch.len_utf8()..];
            continue;
        }

        let mut matched: Option<(usize, LexedToken)> = None;
        for shape in &LEXER_SHAPES {
            if let Some((consumed, token_text)) = try_match_shape(shape, rest) {
                let after = &rest[consumed..];
                let boundary_ok = after
                    .chars()
                    .next()
                    .is_none_or(|next_ch| !is_word_char(next_ch));
                if boundary_ok {
                    matched = Some((
                        consumed,
                        LexedToken {
                            text: token_text,
                            shape: shape.number,
                        },
                    ));
                    break;
                }
            }
        }

        if let Some((consumed, tok)) = matched {
            out.push(tok);
            rest = &rest[consumed..];
        } else {
            // Shape 11 (Word) always matches at least `ch`, so this branch is unreachable.
            rest = &rest[ch.len_utf8()..];
        }
    }

    out
}

/// Tokenizes `input` for the index side (`docs/reference/search.md#the-index-side-adds-tokens`),
/// keeping duplicates and emitting ancestors and aliases in order.
#[must_use]
pub fn index_tokens(input: &str) -> Vec<String> {
    let lexed = lex(input);
    let mut out = Vec::with_capacity(lexed.len());
    for tok in lexed {
        match tok.shape {
            1 | 7 => {
                let base = tok
                    .text
                    .rsplit_once("-v")
                    .map(|(prefix, _)| prefix.to_owned());
                out.push(tok.text);
                if let Some(unversioned) = base {
                    out.push(unversioned);
                }
            }
            9 => {
                let extras = clause_extras(&tok.text);
                out.push(tok.text);
                out.extend(extras);
            }
            _ => {
                out.push(tok.text);
            }
        }
    }
    out
}

/// Tokenizes `input` for the query side (`docs/reference/search.md#the-query-side`),
/// removing duplicates (keeping the first occurrence), capping at 12 tokens, and emitting no
/// extras.
#[must_use]
pub fn query_tokens(input: &str) -> Vec<String> {
    let norm = normalise(input);
    let lexed = lex_normalised(norm.trim());
    let mut out = Vec::new();
    for tok in lexed {
        if !out.contains(&tok.text) {
            out.push(tok.text);
            if out.len() == MAX_QUERY_TOKENS {
                break;
            }
        }
    }
    out
}

fn clause_extras(clause: &str) -> Vec<String> {
    let mut extras = Vec::new();
    let mut cur = clause;

    // 1. Drop paragraph groups `(...)` from the right one at a time.
    while cur.ends_with(')') {
        let Some(open) = cur.rfind('(') else {
            break;
        };
        cur = &cur[..open];
        extras.push(cur.to_owned());
    }

    // 2. Drop the hyphen suffix `-<digits>`, if present, leaving `<part>.<section>`,
    //    and emit a four-digit suffix as the alias.
    if let Some((part_section, suffix)) = cur.split_once('-') {
        extras.push(part_section.to_owned());
        if suffix.len() == 4 && suffix.bytes().all(|b| b.is_ascii_digit()) {
            extras.push(suffix.to_owned());
        }
    }

    extras
}

fn try_match_shape(shape: &LexerShape, input: &str) -> Option<(usize, String)> {
    match shape.name {
        "Number" => match_number_shape(input),
        "Word" => match_word_shape(input),
        _ => {
            let len = match_regex_prefix(shape.pattern.as_bytes(), input.as_bytes())?;
            let slice = input.get(..len)?;
            Some((len, slice.to_owned()))
        }
    }
}

fn match_number_shape(input: &str) -> Option<(usize, String)> {
    let bytes = input.as_bytes();
    let mut pos = bytes.iter().take_while(|&&b| b.is_ascii_digit()).count();
    if pos == 0 {
        return None;
    }

    loop {
        let rem = &bytes[pos..];
        if rem.first() == Some(&b'.') {
            let digits = rem[1..].iter().take_while(|&&b| b.is_ascii_digit()).count();
            if digits >= 1 {
                pos += 1 + digits;
                continue;
            }
        }
        if rem.first() == Some(&b',') {
            let digits = rem[1..].iter().take_while(|&&b| b.is_ascii_digit()).count();
            if digits == 3 {
                pos += 4;
                continue;
            }
        }
        break;
    }

    let raw = input.get(..pos)?;
    let cleaned: String = raw.chars().filter(|&c| c != ',').collect();
    Some((pos, cleaned))
}

fn match_word_shape(input: &str) -> Option<(usize, String)> {
    let mut end = 0usize;
    for ch in input.chars() {
        if is_word_char(ch) {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let slice = input.get(..end)?;
    Some((end, slice.to_owned()))
}

#[derive(Clone, Copy)]
enum RegexAtom<'a> {
    Literal(u8),
    Class(&'a [u8]),
    Group(&'a [u8]),
}

/// Greedy prefix matcher (without backtracking) for the regex patterns in rows 1 to 9 of
/// [`LEXER_SHAPES`].
fn match_regex_prefix(pat: &[u8], input: &[u8]) -> Option<usize> {
    let mut pat_rest = pat;
    let mut pos = 0usize;

    while !pat_rest.is_empty() {
        let (atom, min, max, next_pat) = next_quantified_atom(pat_rest)?;
        pat_rest = next_pat;

        let mut count = 0usize;
        while max.is_none_or(|limit| count < limit) {
            let Some(step) = match_atom_once(atom, &input[pos..]) else {
                break;
            };
            if step == 0 {
                break;
            }
            pos += step;
            count += 1;
        }

        if count < min {
            return None;
        }
    }

    Some(pos)
}

fn match_atom_once(atom: RegexAtom<'_>, input: &[u8]) -> Option<usize> {
    match atom {
        RegexAtom::Literal(expected) => (*input.first()? == expected).then_some(1),
        RegexAtom::Class(cls) => byte_in_class(cls, *input.first()?).then_some(1),
        RegexAtom::Group(subpat) => match_regex_prefix(subpat, input),
    }
}

fn next_quantified_atom(pat: &[u8]) -> Option<(RegexAtom<'_>, usize, Option<usize>, &[u8])> {
    let first = *pat.first()?;
    let (atom, after_atom) = if first == b'\\' {
        let escaped = *pat.get(1)?;
        (RegexAtom::Literal(escaped), &pat[2..])
    } else if first == b'[' {
        let close = pat.iter().position(|&b| b == b']')?;
        (RegexAtom::Class(&pat[1..close]), &pat[close + 1..])
    } else if first == b'(' {
        let close = find_group_close(pat)?;
        (RegexAtom::Group(&pat[1..close]), &pat[close + 1..])
    } else {
        (RegexAtom::Literal(first), &pat[1..])
    };
    let (min, max, rest) = parse_quantifier(after_atom)?;
    Some((atom, min, max, rest))
}

fn find_group_close(pat: &[u8]) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_class = false;
    let mut i = 0usize;
    while i < pat.len() {
        let b = pat[i];
        if b == b'\\' {
            i += 2;
            continue;
        }
        if in_class {
            if b == b']' {
                in_class = false;
            }
        } else if b == b'[' {
            in_class = true;
        } else if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn parse_quantifier(pat: &[u8]) -> Option<(usize, Option<usize>, &[u8])> {
    match pat.first().copied() {
        Some(b'+') => Some((1, None, &pat[1..])),
        Some(b'*') => Some((0, None, &pat[1..])),
        Some(b'?') => Some((0, Some(1), &pat[1..])),
        Some(b'{') => {
            let close = pat.iter().position(|&b| b == b'}')?;
            let spec = core::str::from_utf8(&pat[1..close]).ok()?;
            let rest = &pat[close + 1..];
            if let Some((lo, hi)) = spec.split_once(',') {
                let min = lo.parse::<usize>().ok()?;
                let max = if hi.is_empty() {
                    None
                } else {
                    Some(hi.parse::<usize>().ok()?)
                };
                Some((min, max, rest))
            } else {
                let exact = spec.parse::<usize>().ok()?;
                Some((exact, Some(exact), rest))
            }
        }
        _ => Some((1, Some(1), pat)),
    }
}

fn byte_in_class(class_bytes: &[u8], b: u8) -> bool {
    let mut i = 0usize;
    while i < class_bytes.len() {
        if i + 2 < class_bytes.len() && class_bytes[i + 1] == b'-' {
            let start = class_bytes[i];
            let end = class_bytes[i + 2];
            if (start..=end).contains(&b) {
                return true;
            }
            i += 3;
        } else {
            if class_bytes[i] == b {
                return true;
            }
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FAMILIES;

    const SEARCH_MD: &str = include_str!("../../../../docs/reference/search.md");

    fn unbacktick_single(cell: &str) -> &str {
        let trimmed = cell.trim();
        if let Some(inner) = trimmed.strip_prefix('`').and_then(|s| s.strip_suffix('`'))
            && !inner.contains('`')
        {
            return inner;
        }
        trimmed
    }

    fn parse_lexer_table(md: &str) -> Vec<(u8, String, String, String)> {
        let mut rows = Vec::new();
        let mut in_lexer = false;
        let mut in_table = false;

        for line in md.lines() {
            let trimmed = line.trim();
            if trimmed == "### The lexer" {
                in_lexer = true;
                continue;
            }
            if in_lexer && trimmed.starts_with("### ") {
                break;
            }
            if !in_lexer {
                continue;
            }
            if trimmed.starts_with("| # | Shape | Pattern (ASCII, after normalisation) | Example |")
            {
                in_table = true;
                continue;
            }
            if in_table {
                if !trimmed.starts_with('|') {
                    break;
                }
                if trimmed.starts_with("|---|") {
                    continue;
                }
                let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
                let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
                assert_eq!(
                    cells.len(),
                    4,
                    "expected 4 columns in lexer table row: {line:?}"
                );
                let num: u8 = cells[0]
                    .parse()
                    .unwrap_or_else(|e| panic!("invalid shape number {:?}: {e}", cells[0]));
                let name = cells[1].to_owned();
                let pattern = unbacktick_single(cells[2]).to_owned();
                let example = unbacktick_single(cells[3]).to_owned();
                rows.push((num, name, pattern, example));
            }
        }

        rows
    }

    #[test]
    fn lexer_shapes_match_the_reference() {
        let doc_rows = parse_lexer_table(SEARCH_MD);
        assert!(
            !doc_rows.is_empty(),
            "The lexer table was not found in docs/reference/search.md"
        );

        for (idx, (doc_num, doc_name, doc_pattern, doc_example)) in doc_rows.iter().enumerate() {
            let Some(code_row) = LEXER_SHAPES.get(idx) else {
                panic!(
                    "search.md lexer table has row #{doc_num} ({doc_name}) that LEXER_SHAPES lacks"
                );
            };
            assert_eq!(
                code_row.number,
                *doc_num,
                "LEXER_SHAPES row {} ({}) number {} differs from search.md row #{doc_num} ({doc_name})",
                idx + 1,
                code_row.name,
                code_row.number
            );
            assert_eq!(
                code_row.name, doc_name,
                "LEXER_SHAPES row #{} name {:?} differs from search.md row #{doc_num} ({doc_name:?})",
                code_row.number, code_row.name
            );
            assert_eq!(
                code_row.pattern, doc_pattern,
                "LEXER_SHAPES row #{} ({}) pattern {:?} differs from search.md pattern {:?}",
                code_row.number, code_row.name, code_row.pattern, doc_pattern
            );
            assert_eq!(
                code_row.example, doc_example,
                "LEXER_SHAPES row #{} ({}) example {:?} differs from search.md example {:?}",
                code_row.number, code_row.name, code_row.example, doc_example
            );
        }

        if LEXER_SHAPES.len() > doc_rows.len() {
            let extra = &LEXER_SHAPES[doc_rows.len()];
            panic!(
                "LEXER_SHAPES has extra row #{} ({}) not present in search.md",
                extra.number, extra.name
            );
        }
    }

    #[test]
    fn lexer_covers_every_family() {
        // (family_name, public_shape, company_shape)
        const LEXED: &[(&str, Option<u8>, Option<u8>)] = &[
            ("source", Some(2), Some(8)),
            ("source_version", Some(1), Some(7)),
            ("claim", Some(3), Some(6)),
            ("question", Some(4), Some(8)),
            ("conflict", Some(5), Some(8)),
            ("record", None, Some(8)),
            ("record_version", None, Some(7)),
        ];

        const NOT_LEXED: &[(&str, &str)] = &[
            ("area", "an area code is a three-letter word to the lexer"),
            ("page", "a page id is hyphen-separated words to the lexer"),
            (
                "term",
                "a term id is underscore-separated words to the lexer",
            ),
            (
                "case",
                "a calculation case id is a word and a number to the lexer",
            ),
            (
                "error_code",
                "an error code is a single alphanumeric word to the lexer",
            ),
            (
                "adr",
                "an ADR id is a number and hyphen-separated words to the lexer",
            ),
            (
                "edition",
                "an edition id is a word and a number to the lexer",
            ),
            (
                "tool_release",
                "a tool release tag is a word and a number to the lexer",
            ),
        ];

        // 1. Every row of FAMILIES must appear in LEXED or NOT_LEXED (and never both).
        for family in &FAMILIES {
            let in_lexed = LEXED.iter().any(|(name, _, _)| *name == family.name);
            let in_not_lexed = NOT_LEXED.iter().any(|(name, _)| *name == family.name);
            assert!(
                in_lexed || in_not_lexed,
                "family {} of FAMILIES is in neither LEXED nor NOT_LEXED",
                family.name
            );
            assert!(
                !(in_lexed && in_not_lexed),
                "family {} is listed in both LEXED and NOT_LEXED",
                family.name
            );
        }

        // 2. Every public family in LEXED must exist in FAMILIES, and for every family in LEXED
        //    present in FAMILIES, its lower-case example must lex to one token equal to it by the
        //    named shape.
        for &(name, pub_shape, comp_shape) in LEXED {
            let Some(family) = FAMILIES.iter().find(|f| f.name == name) else {
                assert!(
                    pub_shape.is_none(),
                    "public family {name} in LEXED is missing from FAMILIES"
                );
                continue;
            };

            if let Some(expected_shape) = pub_shape {
                let lower = normalise(family.example);
                let tokens = lex(&lower);
                assert_eq!(
                    tokens,
                    vec![LexedToken {
                        text: lower,
                        shape: expected_shape,
                    }],
                    "family {name} example {:?} did not lex as one token by shape {expected_shape}",
                    family.example
                );
            }

            if let Some(comp_example) = family.company_example {
                let expected_comp_shape = comp_shape.unwrap_or_else(|| {
                    panic!("family {name} has company_example but no company shape in LEXED")
                });
                let lower = normalise(comp_example);
                let tokens = lex(&lower);
                assert_eq!(
                    tokens,
                    vec![LexedToken {
                        text: lower,
                        shape: expected_comp_shape,
                    }],
                    "family {name} company_example {comp_example:?} did not lex as one token by shape {expected_comp_shape}"
                );
            }
        }

        // 3. Every NOT_LEXED entry must exist in FAMILIES with a non-empty reason.
        for &(name, reason) in NOT_LEXED {
            assert!(
                !reason.is_empty(),
                "NOT_LEXED family {name} has empty reason"
            );
            assert!(
                FAMILIES.iter().any(|f| f.name == name),
                "NOT_LEXED family {name} is missing from FAMILIES"
            );
        }

        // 4. For each company prefix set in FAMILIES (set by r2-company-ids; none before it),
        //    `<prefix>-000001` must lex by shape 6, 7 or 8 and never by shape 5.
        for family in &FAMILIES {
            if let Some(prefix) = family.company_prefix {
                let probe = format!("{}-000001", prefix.to_ascii_lowercase());
                let tokens = lex(&probe);
                assert_eq!(
                    tokens.len(),
                    1,
                    "company prefix probe {probe:?} for family {} must lex as one token, got {tokens:?}",
                    family.name
                );
                let shape = tokens[0].shape;
                assert_ne!(
                    shape, 5,
                    "company prefix {prefix:?} for family {} collided with conflict shape 5",
                    family.name
                );
                assert!(
                    matches!(shape, 6..=8),
                    "company prefix probe {probe:?} for family {} must lex by shape 6, 7 or 8, got shape {shape}",
                    family.name
                );
            }
        }
    }
}

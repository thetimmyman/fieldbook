//! Identifier families table and shape matcher.
//!
//! **Responsibility.** Holds the single [`FAMILIES`] table describing every
//! Corpus identifier family, its prefixes, its public shape pattern and its
//! reference example, together with the pure shape matcher that the identifier
//! parsers in [`super`] consult.
//!
//! **Rules.** Pure: no regex crate, no I/O, no clock, no environment.

/// One row of [`FAMILIES`], describing an identifier family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Family {
    /// The snake_case family name (also the golden file stem under `tests/golden/ids/`).
    pub name: &'static str,
    /// The literal prefix(es) of the family (ten prefixes for `case`, one for
    /// prefixed families, empty for unprefixed families).
    pub prefixes: &'static [&'static str],
    /// The regular expression pattern from the Identifiers table of `corpus-format.md`.
    pub public_shape: &'static str,
    /// The canonical example from the Identifiers table of `corpus-format.md`.
    pub example: &'static str,
    /// The company-store prefix (`None` until `r2-company-ids`).
    pub company_prefix: Option<&'static str>,
    /// The company-store shape pattern (`None` until `r2-company-ids`).
    pub company_shape: Option<&'static str>,
    /// The company-store example (`None` until `r2-company-ids`).
    pub company_example: Option<&'static str>,
}

/// The ten calculation case prefixes in the order of `corpus-format.md`.
pub const CASE_PREFIXES: [&str; 10] = [
    "CALC", "QA", "FC", "EST", "QTE", "SCH", "RTE", "EAC", "EVM", "CTF",
];

/// Index of the `area` row in [`FAMILIES`].
pub const IDX_AREA: usize = 0;
/// Index of the `page` row in [`FAMILIES`].
pub const IDX_PAGE: usize = 1;
/// Index of the `source` row in [`FAMILIES`].
pub const IDX_SOURCE: usize = 2;
/// Index of the `source_version` row in [`FAMILIES`].
pub const IDX_SOURCE_VERSION: usize = 3;
/// Index of the `claim` row in [`FAMILIES`].
pub const IDX_CLAIM: usize = 4;
/// Index of the `question` row in [`FAMILIES`].
pub const IDX_QUESTION: usize = 5;
/// Index of the `conflict` row in [`FAMILIES`].
pub const IDX_CONFLICT: usize = 6;
/// Index of the `term` row in [`FAMILIES`].
pub const IDX_TERM: usize = 7;
/// Index of the `case` row in [`FAMILIES`].
pub const IDX_CASE: usize = 8;
/// Index of the `error_code` row in [`FAMILIES`].
pub const IDX_ERROR_CODE: usize = 9;
/// Index of the `adr` row in [`FAMILIES`].
pub const IDX_ADR: usize = 10;
/// Index of the `edition` row in [`FAMILIES`].
pub const IDX_EDITION: usize = 11;
/// Index of the `tool_release` row in [`FAMILIES`].
pub const IDX_TOOL_RELEASE: usize = 12;

/// Every identifier family of Corpus format v1, in the order of the Identifiers table.
pub const FAMILIES: [Family; 13] = [
    Family {
        name: "area",
        prefixes: &[],
        public_shape: "^[a-z]{3}$",
        example: "cmp",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "page",
        prefixes: &[],
        public_shape: "^[a-z]{3}(-[a-z0-9]+)+$",
        example: "cmp-withholding",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "source",
        prefixes: &["S"],
        public_shape: "^S-[0-9]{4}$",
        example: "S-0042",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "source_version",
        prefixes: &["S"],
        public_shape: "^S-[0-9]{4}-v[1-9][0-9]*$",
        example: "S-0042-v2",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "claim",
        prefixes: &["CL"],
        public_shape: "^CL-[A-Z]{3}-([0-9]{4}|[1-9][0-9]{4})$",
        example: "CL-CMP-0011",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "question",
        prefixes: &["Q"],
        public_shape: "^Q-[0-9]{3}$",
        example: "Q-025",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "conflict",
        prefixes: &["XC"],
        public_shape: "^XC-[0-9]{2,}$",
        example: "XC-14",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "term",
        prefixes: &[],
        public_shape: "^[a-z][a-z0-9]*(_[a-z0-9]+)*$",
        example: "material_weakness",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "case",
        prefixes: &CASE_PREFIXES,
        public_shape: "^[A-Z]{2,4}-[0-9]{2,3}$",
        example: "CALC-01",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "error_code",
        prefixes: &["FB"],
        public_shape: "^FB[0-9]{4}$",
        example: "FB0102",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "adr",
        prefixes: &[],
        public_shape: "^[0-9]{4}-[a-z0-9]+(-[a-z0-9]+)*$",
        example: "0002-corpus-format-v1",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "edition",
        prefixes: &["edition"],
        public_shape: "^edition-[0-9]{3,}$",
        example: "edition-007",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
    Family {
        name: "tool_release",
        prefixes: &["v"],
        public_shape: r"^v[0-9]+\.[0-9]+\.[0-9]+$",
        example: "v0.2.0",
        company_prefix: None,
        company_shape: None,
        company_example: None,
    },
];

/// Looks up a family row by its snake_case `name`.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|f| f.name == name)
}

/// Checks whether `input` matches a `^...$` shape pattern from [`FAMILIES`]
/// without a regular-expression crate.
#[must_use]
pub fn matches_shape(shape: &str, input: &str) -> bool {
    let Some(inner) = shape.strip_prefix('^').and_then(|s| s.strip_suffix('$')) else {
        return false;
    };
    match_seq(inner.as_bytes(), input.as_bytes())
}

fn match_seq(pat: &[u8], input: &[u8]) -> bool {
    // Handle top-level alternation `alt1|alt2` inside a group.
    if let Some(pipe_pos) = find_top_level_pipe(pat) {
        let left = &pat[..pipe_pos];
        let right = &pat[pipe_pos + 1..];
        return match_seq(left, input) || match_seq(right, input);
    }
    let Some((atom, min, max, rest_pat)) = next_quantified_atom(pat) else {
        return input.is_empty();
    };
    match_quantified(atom, min, max, rest_pat, input, 0)
}

#[derive(Clone, Copy)]
enum Atom<'a> {
    Literal(u8),
    Class(&'a [u8]),
    Group(&'a [u8]),
}

fn find_top_level_pipe(pat: &[u8]) -> Option<usize> {
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
        } else if b == b'|' && depth == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn next_quantified_atom(pat: &[u8]) -> Option<(Atom<'_>, usize, Option<usize>, &[u8])> {
    let first = *pat.first()?;
    let (atom, after_atom) = if first == b'\\' {
        let escaped = *pat.get(1)?;
        (Atom::Literal(escaped), &pat[2..])
    } else if first == b'[' {
        let close = pat.iter().position(|&b| b == b']')?;
        (Atom::Class(&pat[1..close]), &pat[close + 1..])
    } else if first == b'(' {
        let close = find_group_close(pat)?;
        (Atom::Group(&pat[1..close]), &pat[close + 1..])
    } else {
        (Atom::Literal(first), &pat[1..])
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

fn match_quantified(
    atom: Atom<'_>,
    min: usize,
    max: Option<usize>,
    rest_pat: &[u8],
    input: &[u8],
    count: usize,
) -> bool {
    if let Some(limit) = max
        && count > limit
    {
        return false;
    }
    if count >= min && match_seq(rest_pat, input) {
        return true;
    }
    if max.is_some_and(|limit| count == limit) {
        return false;
    }
    match atom {
        Atom::Literal(expected) => {
            if input.first().copied() == Some(expected) {
                match_quantified(atom, min, max, rest_pat, &input[1..], count + 1)
            } else {
                false
            }
        }
        Atom::Class(class_bytes) => {
            if input
                .first()
                .copied()
                .is_some_and(|b| byte_in_class(class_bytes, b))
            {
                match_quantified(atom, min, max, rest_pat, &input[1..], count + 1)
            } else {
                false
            }
        }
        Atom::Group(group_pat) => {
            for split in 1..=input.len() {
                if match_seq(group_pat, &input[..split])
                    && match_quantified(atom, min, max, rest_pat, &input[split..], count + 1)
                {
                    return true;
                }
            }
            false
        }
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
    use crate::ids::{
        AdrId, AreaCode, CaseId, ClaimId, ConflictId, EditionId, PageId, QuestionId, ReleaseId,
        SourceId, TermId, VersionId, is_error_code,
    };

    const CORPUS_FORMAT_MD: &str = include_str!("../../../../docs/reference/corpus-format.md");

    const GOLDEN_FILES: [(&str, &str); 13] = [
        (
            "area",
            include_str!("../../../../tests/golden/ids/area.json"),
        ),
        (
            "page",
            include_str!("../../../../tests/golden/ids/page.json"),
        ),
        (
            "source",
            include_str!("../../../../tests/golden/ids/source.json"),
        ),
        (
            "source_version",
            include_str!("../../../../tests/golden/ids/source_version.json"),
        ),
        (
            "claim",
            include_str!("../../../../tests/golden/ids/claim.json"),
        ),
        (
            "question",
            include_str!("../../../../tests/golden/ids/question.json"),
        ),
        (
            "conflict",
            include_str!("../../../../tests/golden/ids/conflict.json"),
        ),
        (
            "term",
            include_str!("../../../../tests/golden/ids/term.json"),
        ),
        (
            "case",
            include_str!("../../../../tests/golden/ids/case.json"),
        ),
        (
            "error_code",
            include_str!("../../../../tests/golden/ids/error_code.json"),
        ),
        ("adr", include_str!("../../../../tests/golden/ids/adr.json")),
        (
            "edition",
            include_str!("../../../../tests/golden/ids/edition.json"),
        ),
        (
            "tool_release",
            include_str!("../../../../tests/golden/ids/tool_release.json"),
        ),
    ];

    fn doc_object_matches_family(object_cell: &str, family_name: &str) -> bool {
        let slug = object_cell.to_ascii_lowercase().replace(' ', "_");
        match slug.as_str() {
            "area_code" => family_name == "area",
            "calculation_case" => family_name == "case",
            other => other == family_name,
        }
    }

    fn parse_identifiers_table(md: &str) -> Vec<(String, String, Vec<String>)> {
        let mut rows = Vec::new();
        let mut in_section = false;
        let mut in_table = false;
        for line in md.lines() {
            if line.trim() == "## Identifiers" {
                in_section = true;
                continue;
            }
            if in_section && line.starts_with("## ") {
                break;
            }
            if !in_section {
                continue;
            }
            if line.starts_with("| Object | Form | Pattern | Example |") {
                in_table = true;
                continue;
            }
            if in_table {
                if !line.starts_with('|') {
                    break;
                }
                if line.starts_with("|---|") {
                    continue;
                }
                let cells = split_markdown_row(line);
                if cells.len() >= 4 {
                    let object = cells[0].trim().to_owned();
                    let pattern = extract_backticked(&cells[2])
                        .into_iter()
                        .next()
                        .unwrap_or_default()
                        .replace(r"\|", "|");
                    let examples = extract_backticked(&cells[3]);
                    rows.push((object, pattern, examples));
                }
            }
        }
        rows
    }

    fn parse_case_prefixes_table(md: &str) -> Vec<String> {
        let mut prefixes = Vec::new();
        let mut in_table = false;
        for line in md.lines() {
            if line.starts_with("| Prefix | Cases for | State |") {
                in_table = true;
                continue;
            }
            if in_table {
                if !line.starts_with('|') {
                    break;
                }
                if line.starts_with("|---|") {
                    continue;
                }
                let cells = split_markdown_row(line);
                if let Some(first_cell) = cells.first() {
                    prefixes.extend(extract_backticked(first_cell));
                }
            }
        }
        prefixes
    }

    fn split_markdown_row(line: &str) -> Vec<String> {
        let trimmed = line.trim().trim_start_matches('|').trim_end_matches('|');
        let mut cells = Vec::new();
        let mut current = String::new();
        let bytes = trimmed.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                current.push_str(r"\|");
                i += 2;
            } else if bytes[i] == b'|' {
                cells.push(current.trim().to_owned());
                current.clear();
                i += 1;
            } else {
                if let Some(ch) = trimmed[i..].chars().next() {
                    current.push(ch);
                    i += ch.len_utf8();
                } else {
                    break;
                }
            }
        }
        cells.push(current.trim().to_owned());
        cells
    }

    fn extract_backticked(cell: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = cell;
        while let Some(start) = rest.find('`') {
            let after_start = &rest[start + 1..];
            let Some(end) = after_start.find('`') else {
                break;
            };
            out.push(after_start[..end].to_owned());
            rest = &after_start[end + 1..];
        }
        out
    }

    fn family_parser_accepts(family_name: &str, input: &str) -> bool {
        match family_name {
            "area" => AreaCode::parse(input).is_ok(),
            "page" => PageId::parse(input).is_ok(),
            "source" => SourceId::parse(input).is_ok(),
            "source_version" => VersionId::parse(input).is_ok(),
            "claim" => ClaimId::parse(input).is_ok(),
            "question" => QuestionId::parse(input).is_ok(),
            "conflict" => ConflictId::parse(input).is_ok(),
            "term" => TermId::parse(input).is_ok(),
            "case" => CaseId::parse(input).is_ok(),
            "error_code" => is_error_code(input),
            "adr" => AdrId::parse(input).is_ok(),
            "edition" => EditionId::parse(input).is_ok(),
            "tool_release" => ReleaseId::parse(input).is_ok(),
            _ => false,
        }
    }

    #[test]
    fn families_have_shapes_goldens_and_docs() {
        let doc_rows = parse_identifiers_table(CORPUS_FORMAT_MD);
        assert!(
            !doc_rows.is_empty(),
            "Identifiers table not found in corpus-format.md"
        );

        for family in &FAMILIES {
            let Some((_, golden_text)) = GOLDEN_FILES.iter().find(|(name, _)| *name == family.name)
            else {
                panic!("missing golden file for family {}", family.name);
            };
            let parsed_golden: serde_json::Value = serde_json::from_str(golden_text)
                .unwrap_or_else(|e| panic!("invalid JSON in golden for {}: {e}", family.name));
            assert_eq!(
                parsed_golden.get("family").and_then(|v| v.as_str()),
                Some(family.name),
                "golden family field mismatch for {}",
                family.name
            );
            let cases = parsed_golden
                .get("cases")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("missing cases array in golden for {}", family.name));
            assert!(
                cases.len() >= 5,
                "golden for family {} must hold at least 5 cases",
                family.name
            );

            let Some((_, doc_pattern, doc_examples)) = doc_rows
                .iter()
                .find(|(obj, _, _)| doc_object_matches_family(obj, family.name))
            else {
                panic!(
                    "family {} has no row in Identifiers table of corpus-format.md",
                    family.name
                );
            };
            assert_eq!(
                family.public_shape, doc_pattern,
                "public_shape of family {} does not match Identifiers table",
                family.name
            );
            assert!(
                doc_examples.iter().any(|ex| ex == family.example),
                "example {:?} of family {} is not in Identifiers table examples {:?}",
                family.example,
                family.name,
                doc_examples
            );

            assert!(
                matches_shape(family.public_shape, family.example),
                "example {:?} of family {} is not accepted by its public_shape {}",
                family.example,
                family.name,
                family.public_shape
            );
            assert!(
                family_parser_accepts(family.name, family.example),
                "example {:?} of family {} is not accepted by its parser",
                family.example,
                family.name
            );

            if family.name == "case" {
                assert!(
                    !family.prefixes.is_empty(),
                    "family {} must list calculation case prefixes",
                    family.name
                );
                assert!(
                    family
                        .prefixes
                        .iter()
                        .any(|p| family.example.starts_with(&format!("{p}-"))),
                    "example {:?} of family {} does not start with any listed prefix {:?}",
                    family.example,
                    family.name,
                    family.prefixes
                );
                for prefix in family.prefixes {
                    let probe = format!("{prefix}-01");
                    assert!(
                        matches_shape(family.public_shape, &probe) && CaseId::parse(&probe).is_ok(),
                        "prefix {prefix:?} of family {} does not form a valid case id",
                        family.name
                    );
                }
            } else if matches!(family.name, "area" | "page" | "term" | "adr") {
                assert!(
                    family.prefixes.is_empty(),
                    "family {} has no literal prefix in its pattern but lists {:?}",
                    family.name,
                    family.prefixes
                );
            } else {
                assert_eq!(
                    family.prefixes.len(),
                    1,
                    "family {} must have exactly one prefix",
                    family.name
                );
                let prefix = family.prefixes[0];
                let expected_shape_start = format!("^{prefix}");
                assert!(
                    family.public_shape.starts_with(&expected_shape_start),
                    "prefix {prefix:?} of family {} does not match public_shape {}",
                    family.name,
                    family.public_shape
                );
                assert!(
                    family.example.starts_with(prefix),
                    "prefix {prefix:?} of family {} does not match example {}",
                    family.name,
                    family.example
                );
            }
        }
    }

    #[test]
    fn case_prefixes_match_the_reference() {
        let doc_prefixes = parse_case_prefixes_table(CORPUS_FORMAT_MD);
        assert!(
            !doc_prefixes.is_empty(),
            "Calculation case prefixes table not found in corpus-format.md"
        );
        let case_family = &FAMILIES[IDX_CASE];
        for doc_prefix in &doc_prefixes {
            assert!(
                case_family.prefixes.contains(&doc_prefix.as_str()),
                "prefix {doc_prefix} is in Calculation case prefixes table of corpus-format.md but missing from FAMILIES case row"
            );
        }
        for &code_prefix in case_family.prefixes {
            assert!(
                doc_prefixes.iter().any(|p| p == code_prefix),
                "prefix {code_prefix} is in FAMILIES case row but missing from Calculation case prefixes table of corpus-format.md"
            );
        }
        assert_eq!(
            case_family.prefixes.len(),
            doc_prefixes.len(),
            "duplicate prefix in FAMILIES case row or reference table"
        );
    }
}

//! Marking banner scan (`FB0400`, `docs/reference/corpus-format.md#scans`).
//!
//! **Responsibility.** Hold the [`SHAPES`] table in the order of the "Banner shapes"
//! table in `docs/reference/corpus-format.md#scans` and scan text for marking banners.
//! All banner patterns are matched case-sensitively except
//! `"technical data whose export is restricted"`, which matches in any ASCII case.

use super::Finding;

/// One row of the "Banner shapes" table in `docs/reference/corpus-format.md#scans`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeRow {
    /// The `Shape` column of the reference table.
    pub shape: &'static str,
    /// The literal banner patterns belonging to this row, in table order.
    pub patterns: &'static [Pattern],
}

/// One banner pattern in [`SHAPES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pattern {
    /// The pattern text as listed in `docs/reference/corpus-format.md#scans`.
    pub text: &'static str,
    /// Whether the pattern matches in any case (`true` only for `"technical data whose export is restricted"`).
    pub any_case: bool,
}

impl Pattern {
    /// Construct a case-sensitive banner pattern.
    #[must_use]
    pub const fn exact(text: &'static str) -> Self {
        Self {
            text,
            any_case: false,
        }
    }

    /// Construct a case-insensitive banner pattern.
    #[must_use]
    pub const fn any_case(text: &'static str) -> Self {
        Self {
            text,
            any_case: true,
        }
    }
}

/// The banner shape rows in the exact order of the "Banner shapes" table in
/// `docs/reference/corpus-format.md#scans`.
pub const SHAPES: &[ShapeRow] = &[
    ShapeRow {
        shape: "Controlled information",
        patterns: &[
            Pattern::exact("CUI//"),
            Pattern::exact("CONTROLLED UNCLASSIFIED INFORMATION"),
        ],
    },
    ShapeRow {
        shape: "Dissemination",
        patterns: &[Pattern::exact("NOFORN"), Pattern::exact("FOUO")],
    },
    ShapeRow {
        shape: "Distribution statements",
        patterns: &[
            Pattern::exact("DISTRIBUTION STATEMENT B"),
            Pattern::exact("DISTRIBUTION STATEMENT C"),
            Pattern::exact("DISTRIBUTION STATEMENT D"),
            Pattern::exact("DISTRIBUTION STATEMENT E"),
            Pattern::exact("DISTRIBUTION STATEMENT F"),
        ],
    },
    ShapeRow {
        shape: "Export control",
        patterns: &[
            Pattern::exact("EXPORT CONTROLLED"),
            Pattern::exact("EXPORT-CONTROLLED"),
            Pattern::exact("ITAR CONTROLLED"),
            Pattern::any_case("technical data whose export is restricted"),
        ],
    },
    ShapeRow {
        shape: "Company and procurement",
        patterns: &[
            Pattern::exact("PROPRIETARY"),
            Pattern::exact("COMPANY CONFIDENTIAL"),
            Pattern::exact("SOURCE SELECTION INFORMATION"),
            Pattern::exact("CONTRACTOR BID OR PROPOSAL INFORMATION"),
        ],
    },
];

/// Flat list of all banner patterns in the order of [`SHAPES`].
pub const PATTERNS: &[Pattern] = &[
    Pattern::exact("CUI//"),
    Pattern::exact("CONTROLLED UNCLASSIFIED INFORMATION"),
    Pattern::exact("NOFORN"),
    Pattern::exact("FOUO"),
    Pattern::exact("DISTRIBUTION STATEMENT B"),
    Pattern::exact("DISTRIBUTION STATEMENT C"),
    Pattern::exact("DISTRIBUTION STATEMENT D"),
    Pattern::exact("DISTRIBUTION STATEMENT E"),
    Pattern::exact("DISTRIBUTION STATEMENT F"),
    Pattern::exact("EXPORT CONTROLLED"),
    Pattern::exact("EXPORT-CONTROLLED"),
    Pattern::exact("ITAR CONTROLLED"),
    Pattern::any_case("technical data whose export is restricted"),
    Pattern::exact("PROPRIETARY"),
    Pattern::exact("COMPANY CONFIDENTIAL"),
    Pattern::exact("SOURCE SELECTION INFORMATION"),
    Pattern::exact("CONTRACTOR BID OR PROPOSAL INFORMATION"),
];

const fn is_ascii_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn boundary_ok(bytes: &[u8], start: usize, end: usize, pat: &[u8]) -> bool {
    if pat.first().copied().is_some_and(is_ascii_word)
        && start > 0
        && is_ascii_word(bytes[start - 1])
    {
        return false;
    }
    if pat.last().copied().is_some_and(is_ascii_word)
        && end < bytes.len()
        && is_ascii_word(bytes[end])
    {
        return false;
    }
    true
}

/// Scan `text` for marking banner shapes (`FB0400`) and return all matches in byte order.
#[must_use]
pub fn scan(text: &str) -> Vec<Finding> {
    let bytes = text.as_bytes();
    let mut findings = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let mut matched_here: Option<(usize, usize)> = None;
        for pat in PATTERNS {
            let pat_bytes = pat.text.as_bytes();
            let end = i + pat_bytes.len();
            if end > bytes.len() {
                continue;
            }
            let slice = &bytes[i..end];
            let eq = if pat.any_case {
                slice.eq_ignore_ascii_case(pat_bytes)
            } else {
                slice == pat_bytes
            };
            if eq && boundary_ok(bytes, i, end, pat_bytes) {
                match matched_here {
                    None => matched_here = Some((i, end)),
                    Some((_, prev_end)) if end > prev_end => matched_here = Some((i, end)),
                    _ => {}
                }
            }
        }

        if let Some((start, end)) = matched_here
            && let Some(sub) = text.get(start..end)
        {
            findings.push(Finding::new(sub, start..end));
            i = end;
            continue;
        }
        i += 1;
    }

    findings
}

//! Claim-language scan (`FB0402`, `docs/reference/corpus-format.md#scans`).
//!
//! **Responsibility.** Hold [`PHRASES`] in the exact order of the "Claim language"
//! row in `docs/reference/corpus-format.md#scans` and scan prose or a claim
//! `statement` for forbidden compliance, certification or unattributed-corroboration
//! phrases in any ASCII case.

use super::Finding;

/// Forbidden claim-language phrases (`FB0402`), in the order of `docs/reference/corpus-format.md#scans`.
pub const PHRASES: &[&str] = &[
    "CMMC compliant",
    "NIST certified",
    "FIPS compliant",
    "EVMS compliant",
    "audit-ready",
    "DCAA approved",
    "independently corroborated",
    "independently confirmed",
    "multiple independent sources",
];

const fn is_ascii_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn boundary_ok(bytes: &[u8], start: usize, end: usize) -> bool {
    if start > 0 && is_ascii_word(bytes[start - 1]) {
        return false;
    }
    if end < bytes.len() && is_ascii_word(bytes[end]) {
        return false;
    }
    true
}

/// Scan `text` for any phrase in [`PHRASES`] (case-insensitively) and return all matches
/// in byte order.
#[must_use]
pub fn scan(text: &str) -> Vec<Finding> {
    let bytes = text.as_bytes();
    let mut findings = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let mut matched_here: Option<(usize, usize)> = None;
        for &phrase in PHRASES {
            let p_bytes = phrase.as_bytes();
            let end = i + p_bytes.len();
            if end > bytes.len() {
                continue;
            }
            if bytes[i..end].eq_ignore_ascii_case(p_bytes) && boundary_ok(bytes, i, end) {
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

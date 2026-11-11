//! Page `review_by` computation (`docs/reference/corpus-format.md#dates`).
//!
//! **Responsibility.** Computes a page's `review_by` date from the later of its `as_of` date
//! and the date of its newest valid Change note entry, its optional front-matter `volatility`,
//! and the volatilities of the claims it cites.

use crate::date::{CivilDate, Volatility, review_by};
use crate::schema::{ChangeClass, ClaimRow, PageMeta};

/// Extracts the date of the newest valid entry in a page body's `## Change note` part.
///
/// Returns `None` when the body contains no `## Change note` part, or when that
/// part contains no valid change-note entries.
#[must_use]
pub fn newest_change_note_date(body: &str) -> Option<CivilDate> {
    let mut in_change_note = false;
    let mut in_fence = false;
    let mut max_date: Option<CivilDate> = None;

    for line in body.lines() {
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }

        if in_fence {
            continue;
        }

        if !in_change_note {
            if line.trim_end() == "## Change note" {
                in_change_note = true;
            }
            continue;
        }

        if line.starts_with("# ") || line.starts_with("## ") {
            break;
        }

        let trimmed = line.trim();
        let Some(entry) = trimmed.strip_prefix("- ") else {
            continue;
        };
        let Some((class, rest)) = entry.split_once(", ") else {
            continue;
        };
        let Some((date_str, sentence)) = rest.split_once(": ") else {
            continue;
        };

        if ChangeClass::parse(class).is_err() {
            continue;
        }
        let Ok(d) = CivilDate::parse(date_str) else {
            continue;
        };
        if sentence.trim().is_empty() {
            continue;
        }

        max_date = Some(match max_date {
            Some(curr) => curr.max(d),
            None => d,
        });
    }

    max_date
}

/// Computes a page's `review_by` date (`docs/reference/corpus-format.md#dates`).
///
/// A page's `review_by` is the later of its `as_of` and the date of its newest
/// valid Change note entry, plus the interval of its volatility:
/// 1. the page's own `volatility` field when present,
/// 2. else the most volatile class (`V1` < `V2` < `V3`) among `cited_claims`,
/// 3. else [`Volatility::V3`].
#[must_use]
pub fn page_review_by<'a, I>(
    page: &PageMeta,
    newest_change: Option<CivilDate>,
    cited_claims: I,
) -> CivilDate
where
    I: IntoIterator<Item = &'a ClaimRow>,
{
    let vol = page_volatility(
        page.volatility,
        cited_claims.into_iter().map(|c| c.volatility),
    );
    let base = match newest_change {
        Some(change) => page.as_of.max(change),
        None => page.as_of,
    };
    review_by(base, vol)
}

/// Resolves the effective [`Volatility`] for a page from its own optional `page_volatility`
/// and the volatilities of its cited claims.
#[must_use]
pub fn page_volatility<I>(page_volatility: Option<Volatility>, claim_volatilities: I) -> Volatility
where
    I: IntoIterator<Item = Volatility>,
{
    if let Some(v) = page_volatility {
        return v;
    }
    claim_volatilities
        .into_iter()
        .min()
        .unwrap_or(Volatility::V3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::page;

    const SAMPLE_PAGE_WITH_V2: &str = r#"id = "cmp-system-review"
title = "Synthetic system review"
area = "cmp"
subarea = "business-systems"
type = "concept"
stage = 2
readiness = "SYNTHETIC-NOW"
status = "draft"
as_of = 2026-01-01
handling = ["public"]
volatility = "V2"
"#;

    const SAMPLE_PAGE_WITHOUT_VOL: &str = r#"id = "cmp-system-review"
title = "Synthetic system review"
area = "cmp"
subarea = "business-systems"
type = "concept"
stage = 2
readiness = "SYNTHETIC-NOW"
status = "draft"
as_of = 2026-01-01
handling = ["public"]
"#;

    fn sample_claim(volatility: Volatility) -> ClaimRow {
        use crate::ids::{AreaCode, ClaimId};
        use crate::schema::vocab::{
            Adoption, Applicability, ClaimKind, DataOrigin, DraftedBy, Verification,
        };

        ClaimRow {
            claim_id: ClaimId::parse("CL-CMP-0001").unwrap(),
            area: AreaCode::parse("cmp").unwrap(),
            kind: ClaimKind::Fact,
            statement: "Synthetic statement.".to_owned(),
            as_of: None,
            source_id: None,
            version: None,
            locator_kind: None,
            locator: None,
            quote: None,
            consequential: false,
            drafted_by: DraftedBy::Codex,
            verification: Verification::Unchecked,
            verified_on: None,
            verified_by_role: None,
            applicability: Applicability::PendingCompanyConfirmation,
            adoption: Adoption::Proposed,
            data_origin: DataOrigin::Observed,
            volatility,
            value: None,
            unit: None,
            effective_from: None,
            effective_to: None,
            instrument: None,
            citation_codified: None,
            citation_rfo: None,
            edition_date: None,
            text_status: None,
            applies_to: None,
            evidence_grade: None,
            supersedes: Vec::new(),
            superseded_by: Vec::new(),
            conflict_id: None,
            question_id: None,
            note: None,
        }
    }

    #[test]
    fn page_own_volatility_wins() {
        let meta = page::parse_meta(SAMPLE_PAGE_WITH_V2).expect("valid meta");
        let c_v1 = sample_claim(Volatility::V1);
        let c_v3 = sample_claim(Volatility::V3);
        // Even when citing a V1 claim, the page's own V2 (180 days from 2026-01-01 -> 2026-06-30) wins.
        assert_eq!(
            page_review_by(&meta, None, [&c_v1, &c_v3]),
            CivilDate::parse("2026-06-30").unwrap()
        );
    }

    #[test]
    fn most_volatile_cited_claim_wins_when_page_volatility_absent() {
        let meta = page::parse_meta(SAMPLE_PAGE_WITHOUT_VOL).expect("valid meta");
        let c_v3 = sample_claim(Volatility::V3);
        let c_v2 = sample_claim(Volatility::V2);
        let c_v1 = sample_claim(Volatility::V1);

        // V2 + V3 -> V2 (180 days -> 2026-06-30)
        assert_eq!(
            page_review_by(&meta, None, [&c_v3, &c_v2]),
            CivilDate::parse("2026-06-30").unwrap()
        );
        // V3 + V1 + V2 -> V1 (90 days -> 2026-04-01)
        assert_eq!(
            page_review_by(&meta, None, [&c_v3, &c_v1, &c_v2]),
            CivilDate::parse("2026-04-01").unwrap()
        );
    }

    #[test]
    fn defaults_to_v3_when_page_volatility_absent_and_no_cited_claims() {
        let meta = page::parse_meta(SAMPLE_PAGE_WITHOUT_VOL).expect("valid meta");
        let empty: [&ClaimRow; 0] = [];
        // V3 (365 days from 2026-01-01 -> 2027-01-01)
        assert_eq!(
            page_review_by(&meta, None, empty),
            CivilDate::parse("2027-01-01").unwrap()
        );
    }

    #[test]
    fn review_by_counts_from_newest_change_note() {
        let meta = page::parse_meta(SAMPLE_PAGE_WITHOUT_VOL).expect("valid meta");
        let empty: [&ClaimRow; 0] = [];
        let body = r#"## Change note
- COMPATIBLE_EXTENSION, 2026-02-15: Older.
- COMPATIBLE_EXTENSION, 2026-03-01: Newer.
- NEW_KNOWLEDGE, 2026-02-01: First draft.
"#;
        let that = newest_change_note_date(body);
        assert_eq!(that, Some(CivilDate::parse("2026-03-01").unwrap()));
        assert_eq!(
            page_review_by(&meta, that, empty),
            CivilDate::parse("2027-03-01").unwrap()
        );
    }

    #[test]
    fn review_by_keeps_as_of_when_later() {
        let meta = page::parse_meta(SAMPLE_PAGE_WITHOUT_VOL).expect("valid meta");
        let empty: [&ClaimRow; 0] = [];
        let body = r#"## Change note
- NEW_KNOWLEDGE, 2025-06-01: Earlier change.
"#;
        let newest = newest_change_note_date(body);
        assert_eq!(newest, Some(CivilDate::parse("2025-06-01").unwrap()));
        assert_eq!(
            page_review_by(&meta, newest, empty),
            CivilDate::parse("2027-01-01").unwrap()
        );
    }

    #[test]
    fn review_by_ignores_invalid_entry_dates() {
        let meta = page::parse_meta(SAMPLE_PAGE_WITHOUT_VOL).expect("valid meta");
        let empty: [&ClaimRow; 0] = [];
        let body = r#"## Change note
- COMPATIBLE_EXTENSION, 2026-02-30: Impossible date.
- COMPATIBLE_EXTENSION, 2026-5-1: Short shape.
- BOGUS, 2026-06-01: Unknown class.
- NEW_KNOWLEDGE, 2026-02-01: First draft.
"#;
        let newest = newest_change_note_date(body);
        assert_eq!(newest, Some(CivilDate::parse("2026-02-01").unwrap()));

        let invalid_only = r#"## Change note
- COMPATIBLE_EXTENSION, 2026-02-30: Impossible date.
- COMPATIBLE_EXTENSION, 2026-5-1: Short shape.
- BOGUS, 2026-06-01: Unknown class.
"#;
        let invalid_newest = newest_change_note_date(invalid_only);
        assert_eq!(invalid_newest, None);
        assert_eq!(
            page_review_by(&meta, invalid_newest, empty),
            CivilDate::parse("2027-01-01").unwrap()
        );
        assert_eq!(
            page_review_by(&meta, None, empty),
            CivilDate::parse("2027-01-01").unwrap()
        );
    }

    #[test]
    fn newest_change_note_date_ignores_other_parts_and_fences() {
        let body = r#"## Purpose
- COMPATIBLE_EXTENSION, 2030-01-01: Not in the part.
- NEW_KNOWLEDGE, 2030-01-01: Not in the part.

## Change note
```
- COMPATIBLE_EXTENSION, 2030-01-01: Inside fence.
- NEW_KNOWLEDGE, 2030-01-01: Inside fence.
```
- NEW_KNOWLEDGE, 2026-02-01: First draft.
"#;
        assert_eq!(
            newest_change_note_date(body),
            Some(CivilDate::parse("2026-02-01").unwrap())
        );

        let body_only_ignored = r#"## Purpose
- COMPATIBLE_EXTENSION, 2030-01-01: Not in the part.

## Change note
```
- COMPATIBLE_EXTENSION, 2030-01-01: Inside fence.
```
"#;
        assert_eq!(newest_change_note_date(body_only_ignored), None);
    }
}

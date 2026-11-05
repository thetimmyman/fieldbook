//! Page `review_by` computation (`docs/reference/corpus-format.md#dates`).
//!
//! **Responsibility.** Computes a page's `review_by` date from its `as_of` date, its optional
//! front-matter `volatility`, and the volatilities of the claims it cites.

use crate::date::{CivilDate, Volatility, review_by};
use crate::schema::{ClaimRow, PageMeta};

/// Computes a page's `review_by` date (`docs/reference/corpus-format.md#dates`).
///
/// A page's `review_by` is its `as_of` plus the interval of its volatility:
/// 1. the page's own `volatility` field when present,
/// 2. else the most volatile class (`V1` < `V2` < `V3`) among `cited_claims`,
/// 3. else [`Volatility::V3`].
#[must_use]
pub fn page_review_by<'a, I>(page: &PageMeta, cited_claims: I) -> CivilDate
where
    I: IntoIterator<Item = &'a ClaimRow>,
{
    let vol = page_volatility(
        page.volatility,
        cited_claims.into_iter().map(|c| c.volatility),
    );
    review_by(page.as_of, vol)
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
            page_review_by(&meta, [&c_v1, &c_v3]),
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
            page_review_by(&meta, [&c_v3, &c_v2]),
            CivilDate::parse("2026-06-30").unwrap()
        );
        // V3 + V1 + V2 -> V1 (90 days -> 2026-04-01)
        assert_eq!(
            page_review_by(&meta, [&c_v3, &c_v1, &c_v2]),
            CivilDate::parse("2026-04-01").unwrap()
        );
    }

    #[test]
    fn defaults_to_v3_when_page_volatility_absent_and_no_cited_claims() {
        let meta = page::parse_meta(SAMPLE_PAGE_WITHOUT_VOL).expect("valid meta");
        let empty: [&ClaimRow; 0] = [];
        // V3 (365 days from 2026-01-01 -> 2027-01-01)
        assert_eq!(
            page_review_by(&meta, empty),
            CivilDate::parse("2027-01-01").unwrap()
        );
    }
}

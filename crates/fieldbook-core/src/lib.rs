//! The Corpus logic: pure functions over values and bytes.
//!
//! **Responsibility.** The serde types that are the schema, the check rules, ids,
//! the citation lexer, scans, the search tokenizer and ranker, the frozen strings,
//! and the two assist functions (`context_pack` and `cite_check`). These arrive with
//! the cards that follow `workspace-skeleton`.
//!
//! **Allowed dependencies.** `fieldbook-calc` only, among workspace crates. External
//! crates join through their cards, each at its pin in `docs/reference/dependencies.md`.
//!
//! **Rules.** No files, environment, clock, process or sockets: the `fieldbook` binary
//! does all I/O and passes bytes in (workspace `clippy.toml`).

pub mod add;
pub mod assist;
pub mod audit;
pub mod authz;
pub mod cases;
pub mod check;
pub mod cite;
pub mod date;
pub mod edition;
pub mod egress;
pub mod eval;
pub mod hash;
pub mod hex;
pub mod ids;
pub mod labels;
pub mod manifest;
pub mod new;
pub mod policy;
pub mod report;
pub mod review;
pub mod scan;
pub mod schema;
pub mod search;
pub mod strings;
pub mod verify;
pub mod version;

pub use date::{
    CivilDate, Duration, EffectivePeriod, PartialDate, Volatility, interval_days, review_by,
};
pub use ids::{
    AdrId, AreaCode, CLAIMS_PER_PART, CaseId, ClaimId, ConflictId, EditionId, FAMILIES, Family,
    IdError, IdErrorKind, PageId, QuestionId, ReleaseId, SourceId, SourceVersionId, TermId,
    ToolRelease, VersionId, claim_href_target, claim_page_route, claim_part, is_error_code,
    next_claim_id, next_conflict_id, next_question_id, next_source_id,
};

/// This crate's name in the chain calc, core, render, fieldbook.
pub const LAYER: &str = "core";

#[cfg(test)]
mod tests {
    use super::LAYER;

    #[test]
    fn layer_matches_the_package_name() {
        assert_eq!(format!("fieldbook-{LAYER}"), env!("CARGO_PKG_NAME"));
    }
}

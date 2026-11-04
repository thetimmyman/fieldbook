//! Citation lexing (`cite::lexer`), resolution (`cite::resolve`) and claim extraction (`cite::claims_of`).
//!
//! **Responsibility.** Read citations from Markdown/text in [`Mode::Strict`] or [`Mode::Tolerant`]
//! (`docs/reference/corpus-format.md#syntax-at-a-glance`), resolve keys against [`crate::check::Index`],
//! and compute the sorted unique [`ClaimId`]s cited on a page.
//!
//! **Rules.** Pure: no regex crate, no I/O, no clock, no environment.

pub mod lexer;
pub mod resolve;

pub use lexer::{
    Citation, CitationLocator, KeyKind, LexFinding, LexOutcome, Mode, Span, lex, lex_from_line,
};
pub use resolve::{ResolvedKey, resolve_citations};

use std::collections::BTreeSet;

use crate::ids::ClaimId;

/// Returns the sorted unique [`ClaimId`]s cited in `citations` (the computed `claims` field of
/// `docs/reference/corpus-format.md#page-files`).
#[must_use]
pub fn claims_of(citations: &[Citation]) -> Vec<ClaimId> {
    let mut set = BTreeSet::new();
    for cite in citations {
        if cite.key_kind == KeyKind::Claim
            && let Ok(cid) = ClaimId::parse(&cite.key)
        {
            set.insert(cid);
        }
    }
    set.into_iter().collect()
}

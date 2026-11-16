//! Retained admission kernel for the Admit `corpus-add@1` adapter (published compatibility binding).
//!
//! **Responsibility.** Frozen deterministic planning behavior of `corpus-add@1` over a [`Proposal`],
//! human [`Edits`], read-only [`CorpusView`], and optional [`VerifiedExtraction`].
//! A change of contract requires a new `corpus-add@2` version module.
//!
//! **Rules.** Pure: no I/O.

use crate::pipeline::{
    Admit, AdmitError, AdmitOutcome, CorpusAdd, CorpusView, Edits, Proposal, VerifiedExtraction,
};

/// Adapter identifier retained by this kernel (`corpus-add@1`).
pub const ADAPTER_ID: &str = "corpus-add@1";

/// Execute the frozen `corpus-add@1` admission planner.
///
/// # Errors
///
/// Returns [`AdmitError`] when `proposal` targets an unbuilt proposal kind, has invalid or missing
/// source cells, has unresolved correlation or missing verified extraction, or violates Corpus schema.
pub fn plan(
    proposal: &Proposal,
    edits: &Edits,
    view: &CorpusView,
    extraction: Option<&VerifiedExtraction>,
) -> Result<AdmitOutcome, AdmitError> {
    CorpusAdd.plan(proposal, edits, view, extraction)
}

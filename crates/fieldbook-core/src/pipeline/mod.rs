//! Pure data structures and stage contracts for the four-stage intake pipeline (`AD-01`).
//!
//! **Responsibility.** Defines candidate preparation and vocabularies ([`candidate`]),
//! classifier/correlator suggestions, links, support validation, and stage traits ([`suggest`]),
//! proposal parsing and canonical rendering ([`proposal`]), read-only Corpus state view ([`view`]),
//! store admission plans and `corpus-add@1` ([`admit`]), and the closed target registry ([`targets`]).

pub mod admit;
pub mod candidate;
pub mod proposal;
pub mod suggest;
pub mod targets;
pub mod view;

pub use admit::{
    Admit, AdmitError, AdmitOutcome, BorrowedAdmit, CorpusAdd, Edits, PlannedWrite, WriteSource,
};
pub use candidate::{
    Candidate, LabelHint, Origin, SuggestField, ValueProblem, VerifiedExtraction,
    VerifiedMethodMetadata, Vocab,
};
pub use proposal::{
    AiCall, AiRecord, Judged, Links, Proposal, ProposalBody, ProposalHint, ProposalKind,
    ProposalParseError, ProposalState, RelatedLink, RowBody, SourceChoice,
};
pub use suggest::{
    By, ByParseError, Classifier, Correlator, DropReason, Link, LinkKind, RuleKind, RuleRef,
    StageError, Suggestion, Suggestions, Support, is_classifier_origin_violation,
    is_valid_present_link, validate, validate_hint,
};
pub use targets::{KeyShape, TARGETS, Target, TargetKind};
pub use view::CorpusView;

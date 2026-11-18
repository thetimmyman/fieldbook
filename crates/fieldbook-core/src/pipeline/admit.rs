//! Store admission port (`Admit`), planned write ordering, and `corpus-add@1`.

use std::collections::BTreeMap;
use std::fmt;

use crate::add::AddError;
use crate::audit::WriteFile;
use crate::check::Code;
use crate::ids::VersionId;
use crate::labels::dependencies::ItemRef;

use super::candidate::{SuggestField, VerifiedExtraction};
use super::proposal::{Proposal, ProposalBody, ProposalKind, SourceChoice};
use super::suggest::By;
use super::view::CorpusView;

/// Where the content bytes for a planned write come from when the binary applies the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteSource {
    /// Bytes come from the queued candidate file under `inbox/`.
    Inbox,
    /// Bytes come from the named file inside `staging/<sha256>/`.
    Staging(String),
    /// Bytes were rendered deterministically in core.
    Rendered(Vec<u8>),
}

/// One planned store file write paired with its byte source.
///
/// Construct external values solely via [`PlannedWrite::new`] so future fields added by
/// downstream cards remain backwards-compatible.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrite {
    /// Target relative path and expected 64-hex SHA-256 digest.
    pub file: WriteFile,
    /// Byte source used by the binary writer.
    pub source: WriteSource,
}

impl PlannedWrite {
    /// Constructs a [`PlannedWrite`] binding `file` to `source`.
    #[must_use]
    pub const fn new(file: WriteFile, source: WriteSource) -> Self {
        Self { file, source }
    }
}

/// Human field edits and optional source-target override collected during queue review.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edits {
    /// Replacement values keyed by [`SuggestField`]; recorded with `by = person` and empty `because`.
    pub fields: BTreeMap<SuggestField, String>,
    /// Optional explicit selection between `New` and `Existing(SourceId)`.
    pub source_choice: Option<SourceChoice>,
}

/// Outcome returned by [`Admit::plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitOutcome {
    /// The file's SHA-256 already exists in `sources.csv`; no writes are planned.
    Present {
        /// Existing source version identifier holding that SHA-256.
        id: VersionId,
    },
    /// Ordered planned writes and affected Corpus item references.
    Planned {
        /// Fixed write order: original first, extraction files, `extraction.toml`, register last.
        files: Vec<PlannedWrite>,
        /// Allocated or affected [`ItemRef`]s for the journal `WriteSet`.
        affected: Vec<ItemRef>,
    },
}

/// Typed admission planning error returned by [`Admit::plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitError {
    /// This adapter does not admit proposals of `kind`.
    KindNotBuilt(ProposalKind),
    /// A required suggestible field is absent from both the proposal and human edits.
    MissingField(SuggestField),
    /// Structural or validation failure with its diagnostic code.
    Invalid(Code),
    /// Propagated error from core `add::plan_add`, `CompletedSourceCells::parse`, or ID allocation.
    Add(AddError),
}

impl From<AddError> for AdmitError {
    fn from(err: AddError) -> Self {
        Self::Add(err)
    }
}

impl AdmitError {
    /// Returns `true` when admission failed due to identifier allocation exhaustion.
    #[must_use]
    pub const fn is_exhausted(&self) -> bool {
        matches!(self, Self::Add(add_err) if add_err.is_exhausted())
    }
}

impl fmt::Display for AdmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KindNotBuilt(kind) => write!(f, "proposal kind `{kind}` is not built"),
            Self::MissingField(field) => write!(f, "missing required field `{field}`"),
            Self::Invalid(code) => write!(f, "admission refused ({code})"),
            Self::Add(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AdmitError {}

/// Architectural port that converts an accepted proposal into an ordered [`AdmitOutcome`] plan.
pub trait Admit {
    /// Registered adapter identifier (`"corpus-add@1"`, ...).
    fn id(&self) -> &'static str;

    /// Builds a deterministic admission plan for `proposal` and `edits` over `view`.
    ///
    /// # Errors
    ///
    /// Returns [`AdmitError`] if `proposal.kind` is unsupported here, required fields are missing,
    /// `source_choice` is unresolved, staging extraction does not match, or [`plan_add`] fails.
    fn plan(
        &self,
        proposal: &Proposal,
        edits: &Edits,
        view: &CorpusView,
        extraction: Option<&VerifiedExtraction>,
    ) -> Result<AdmitOutcome, AdmitError>;
}

/// Forwarding wrapper allowing `&dyn Admit` to be passed where `T: Admit` is expected.
#[derive(Clone, Copy)]
pub struct BorrowedAdmit<'a>(pub &'a dyn Admit);

impl Admit for BorrowedAdmit<'_> {
    fn id(&self) -> &'static str {
        self.0.id()
    }

    fn plan(
        &self,
        proposal: &Proposal,
        edits: &Edits,
        view: &CorpusView,
        extraction: Option<&VerifiedExtraction>,
    ) -> Result<AdmitOutcome, AdmitError> {
        self.0.plan(proposal, edits, view, extraction)
    }
}

/// Default Day-1 Corpus admission adapter (`"corpus-add@1"`).
#[derive(Debug, Clone, Copy, Default)]
pub struct CorpusAdd;

impl Admit for CorpusAdd {
    fn id(&self) -> &'static str {
        "corpus-add@1"
    }

    fn plan(
        &self,
        proposal: &Proposal,
        edits: &Edits,
        view: &CorpusView,
        extraction: Option<&VerifiedExtraction>,
    ) -> Result<AdmitOutcome, AdmitError> {
        crate::versions::corpus_add_v1::plan(proposal, edits, view, extraction)
    }
}

/// Applies human [`Edits`] to a [`Proposal`] in memory: edited fields become `by = person` with
/// empty `because` and `rule = None`; `source_choice` overrides when present.
pub fn apply_edits(proposal: &mut Proposal, edits: &Edits) {
    if let ProposalBody::Row(ref mut row) = proposal.body {
        for (field, value) in &edits.fields {
            row.insert(
                *field,
                super::proposal::Judged {
                    value: value.clone(),
                    by: By::Person,
                    because: String::new(),
                    rule: None,
                },
            );
        }
    }
    if let Some(choice) = &edits.source_choice {
        proposal.source_choice = Some(choice.clone());
        if proposal.kind.is_source_bearing() {
            proposal.kind = choice.effective_kind();
            if let Some(ref mut origin) = proposal.origin {
                match choice {
                    SourceChoice::New | SourceChoice::Unresolved => {
                        origin.base = None;
                    }
                    SourceChoice::Existing(sid) => {
                        if origin.base.as_ref().is_some_and(|b| b.source() != *sid) {
                            origin.base = None;
                        }
                    }
                }
            }
        }
    }
}

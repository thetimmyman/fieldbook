//! Store admission port (`Admit`), planned write ordering, and `corpus-add@1`.

use std::collections::BTreeMap;
use std::fmt;

use crate::add::{AddError, AddOptions, AddOutcome, CompletedSourceCells, plan_add};
use crate::audit::WriteFile;
use crate::check::Code;
use crate::extraction::page_stem;
use crate::hash::sha256_hex;
use crate::ids::{SourceId, VersionId};
use crate::labels::StoreId;
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

/// Required suggestible columns for admitting a `sources.csv` row into the repository Corpus.
const REQUIRED_SUGGESTED: [SuggestField; 5] = [
    SuggestField::Title,
    SuggestField::Publisher,
    SuggestField::SourceKind,
    SuggestField::Licence,
    SuggestField::Volatility,
];

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
        if !proposal.kind.is_source_bearing() {
            return Err(AdmitError::KindNotBuilt(proposal.kind));
        }

        let candidate = proposal
            .candidate
            .as_ref()
            .ok_or(AdmitError::Invalid(Code::Fb0204))?;
        let ProposalBody::Row(row_body) = &proposal.body else {
            return Err(AdmitError::Invalid(Code::Fb0204));
        };

        if let Some(existing) = view.has_hash(&candidate.sha256) {
            return Ok(AdmitOutcome::Present { id: existing });
        }

        let source_opt = resolve_source_target(proposal, edits, view)?;
        let merged_cells = merge_row_cells(row_body, edits)?;

        for req_field in REQUIRED_SUGGESTED {
            if merged_cells
                .get(req_field.as_str())
                .is_none_or(String::is_empty)
            {
                return Err(AdmitError::MissingField(req_field));
            }
        }

        let verified_ext =
            validate_proposal_extraction(proposal, candidate.sha256.as_str(), extraction)?;

        let completed = CompletedSourceCells::parse(&merged_cells)?;
        let options = AddOptions {
            source: source_opt,
            metadata_only: false,
        };

        let outcome = plan_add(
            view.model(),
            &candidate.file,
            &candidate.sha256,
            &options,
            Some(&completed),
            view.today(),
        )?;

        match outcome {
            AddOutcome::Present(id) => Ok(AdmitOutcome::Present { id }),
            AddOutcome::Planned(mut plan) => {
                let Some(register_write) = plan.files.pop() else {
                    return Err(AdmitError::Invalid(Code::Fb0211));
                };

                let mut files = Vec::new();
                for orig_file in plan.files {
                    files.push(PlannedWrite::new(orig_file, WriteSource::Inbox));
                }

                if let Some(verified) = verified_ext {
                    for num in 1..=verified.record.page_count {
                        let stem = page_stem(num);
                        let Some(bytes) = verified.page_bytes.get(&stem) else {
                            return Err(AdmitError::Invalid(Code::Fb0212));
                        };
                        let Some(sha) = verified.page_hashes.get(&stem) else {
                            return Err(AdmitError::Invalid(Code::Fb0212));
                        };
                        let path =
                            format!("corpus/sources/text/{}/{stem}.txt", plan.ids.version_id);
                        files.push(PlannedWrite::new(
                            WriteFile::new(path, sha.clone()),
                            WriteSource::Rendered(bytes.clone()),
                        ));
                    }

                    let toml_bytes = verified.rendered_toml_bytes();
                    let toml_sha = sha256_hex(&toml_bytes);
                    let toml_path = format!(
                        "corpus/sources/text/{}/extraction.toml",
                        plan.ids.version_id
                    );
                    files.push(PlannedWrite::new(
                        WriteFile::new(toml_path, toml_sha),
                        WriteSource::Rendered(toml_bytes),
                    ));
                }

                files.push(PlannedWrite::new(
                    register_write,
                    WriteSource::Rendered(plan.sources_csv),
                ));

                let affected = vec![
                    ItemRef::new(StoreId::Corpus, plan.ids.source_id.as_str()),
                    ItemRef::new(StoreId::Corpus, plan.ids.version_id.as_str()),
                ];

                Ok(AdmitOutcome::Planned { files, affected })
            }
        }
    }
}

fn validate_proposal_extraction<'a>(
    proposal: &Proposal,
    candidate_sha256: &str,
    extraction: Option<&'a VerifiedExtraction>,
) -> Result<Option<&'a VerifiedExtraction>, AdmitError> {
    match (&proposal.extraction, extraction) {
        (Some(meta), Some(verified)) => {
            if verified.original_sha256 != candidate_sha256
                || verified.record.page_count != meta.pages
                || verified.record.thin_pages != meta.thin_pages
                || verified.record.method.name() != meta.method
            {
                return Err(AdmitError::Invalid(Code::Fb0212));
            }
            Ok(Some(verified))
        }
        (Some(_), None) | (None, Some(_)) => Err(AdmitError::Invalid(Code::Fb0212)),
        (None, None) => Ok(None),
    }
}

fn merge_row_cells(
    row_body: &BTreeMap<SuggestField, super::proposal::Judged>,
    edits: &Edits,
) -> Result<BTreeMap<String, String>, AdmitError> {
    let mut cells = BTreeMap::new();
    for (field, judged) in row_body {
        cells.insert(field.as_str().to_owned(), judged.value.clone());
    }
    for (field, value) in &edits.fields {
        cells.insert(field.as_str().to_owned(), value.clone());
    }
    if cells.contains_key(SuggestField::DocKind.as_str()) {
        return Err(AdmitError::from(AddError::UnknownField(
            SuggestField::DocKind.as_str().to_owned(),
        )));
    }
    Ok(cells)
}

fn resolve_source_target(
    proposal: &Proposal,
    edits: &Edits,
    view: &CorpusView,
) -> Result<Option<SourceId>, AdmitError> {
    let chosen = edits
        .source_choice
        .as_ref()
        .or(proposal.source_choice.as_ref());

    match chosen {
        Some(SourceChoice::Unresolved) => Err(AdmitError::Invalid(Code::Fb0204)),
        Some(SourceChoice::New) => Ok(None),
        Some(SourceChoice::Existing(id)) => {
            if view.source(id).is_none() {
                return Err(AdmitError::from(AddError::UnknownSource(id.clone())));
            }
            Ok(Some(id.clone()))
        }
        None => match proposal.kind {
            ProposalKind::NewSource => {
                if proposal.links.version_of.len() > 1 {
                    return Err(AdmitError::Invalid(Code::Fb0204));
                }
                Ok(None)
            }
            ProposalKind::NewVersion => {
                if let Some(base) = proposal.origin.as_ref().and_then(|o| o.base.as_ref()) {
                    let sid = base.source();
                    if view.source(&sid).is_none() {
                        return Err(AdmitError::from(AddError::UnknownSource(sid)));
                    }
                    return Ok(Some(sid));
                }
                if proposal.links.version_of.len() == 1 {
                    let sid = proposal.links.version_of[0].clone();
                    if view.source(&sid).is_none() {
                        return Err(AdmitError::from(AddError::UnknownSource(sid)));
                    }
                    return Ok(Some(sid));
                }
                Err(AdmitError::Invalid(Code::Fb0204))
            }
            _ => Err(AdmitError::KindNotBuilt(proposal.kind)),
        },
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
        }
    }
}

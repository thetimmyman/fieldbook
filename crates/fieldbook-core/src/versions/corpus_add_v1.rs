//! Retained admission kernel for the Admit `corpus-add@1` adapter (published compatibility binding).
//!
//! **Responsibility.** Frozen deterministic planning behavior of `corpus-add@1` over a [`Proposal`],
//! human [`Edits`], read-only [`CorpusView`], and optional [`VerifiedExtraction`].
//! A change of contract requires a new `corpus-add@2` version module; this file and its archived
//! vectors stay executable.
//!
//! **Rules.** Pure: no I/O.

use std::collections::BTreeMap;

use crate::add::{AddError, AddOptions, AddOutcome, CompletedSourceCells, plan_add};
use crate::audit::WriteFile;
use crate::check::Code;
use crate::extraction::page_stem;
use crate::hash::sha256_hex;
use crate::ids::SourceId;
use crate::labels::StoreId;
use crate::labels::dependencies::ItemRef;
use crate::pipeline::{
    AdmitError, AdmitOutcome, CorpusView, Edits, Judged, PlannedWrite, Proposal, ProposalBody,
    ProposalKind, SourceChoice, SuggestField, VerifiedExtraction, WriteSource,
};

/// Adapter identifier retained by this kernel (`corpus-add@1`).
pub const ADAPTER_ID: &str = "corpus-add@1";

/// Required suggestible columns for admitting a `sources.csv` row in `corpus-add@1`.
const REQUIRED_SUGGESTED: [SuggestField; 5] = [
    SuggestField::Title,
    SuggestField::Publisher,
    SuggestField::SourceKind,
    SuggestField::Licence,
    SuggestField::Volatility,
];

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
                for num in 1..=verified.record().page_count {
                    let stem = page_stem(num);
                    let Some(bytes) = verified.page_bytes().get(&stem) else {
                        return Err(AdmitError::Invalid(Code::Fb0212));
                    };
                    let Some(sha) = verified.page_hashes().get(&stem) else {
                        return Err(AdmitError::Invalid(Code::Fb0212));
                    };
                    let path = format!("corpus/sources/text/{}/{stem}.txt", plan.ids.version_id);
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

fn validate_proposal_extraction<'a>(
    proposal: &Proposal,
    candidate_sha256: &str,
    extraction: Option<&'a VerifiedExtraction>,
) -> Result<Option<&'a VerifiedExtraction>, AdmitError> {
    match (&proposal.extraction, extraction) {
        (Some(meta), Some(verified)) => {
            if verified.original_sha256() != candidate_sha256
                || verified.record().page_count != meta.pages
                || verified.record().thin_pages != meta.thin_pages
                || verified.record().method.name() != meta.method
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
    row_body: &BTreeMap<SuggestField, Judged>,
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

//! Shared contract suites for [`Classifier`], [`Correlator`], and [`Admit`] adapters.
//!
//! Included via `#[path = "..."] mod pipeline_suites;` from an adapter's own test file.

use fieldbook_core::date::{CivilDate, PartialDate};
use fieldbook_core::extraction::{Extraction, PageText};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::LabelSet;
use fieldbook_core::pipeline::CorpusView;
use fieldbook_core::pipeline::{
    Admit, AdmitError, AdmitOutcome, BorrowedAdmit, Candidate, Classifier, Correlator, Edits,
    LinkKind, Origin, Proposal, ProposalBody, ProposalKind, ProposalState, VerifiedExtraction,
    Vocab, WriteSource, is_classifier_origin_violation, is_valid_present_link, validate,
};
use fieldbook_core::ports::index::IndexPort;

#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included per integration test binary"
)]
#[path = "../../../../tests/support/rng.rs"]
mod rng;

fn synthetic_text_candidate() -> Candidate {
    let body = "Synthetic Policy Digest\n§1 Overview\nThis synthetic text mentions general procurement terms.";
    let sha = sha256_hex(body.as_bytes());
    Candidate {
        file: "synthetic_suite.txt".to_owned(),
        sha256: sha,
        bytes: body.len() as u64,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: body.to_owned(),
            }],
        }),
    }
}

fn synthetic_origin_candidate() -> Candidate {
    let mut cand = synthetic_text_candidate();
    cand.origin = Some(Origin {
        system: "synth-sys".to_owned(),
        stream: "synth-stream".to_owned(),
        key: "item-01".to_owned(),
        as_of: PartialDate::parse("2026-01-01").unwrap_or_else(|e| panic!("{e}")),
        retrieved: CivilDate::parse("2026-01-02").unwrap_or_else(|e| panic!("{e}")),
        base: None,
        series: None,
        workbook: false,
    });
    cand
}

fn synthetic_noise_candidate() -> Candidate {
    let mut rng = rng::Rng::new(1);
    let raw = rng.bytes(1 << 20);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let sha = sha256_hex(&raw);
    Candidate {
        file: "noise.txt".to_owned(),
        sha256: sha,
        bytes: raw.len() as u64,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText { number: 1, text }],
        }),
    }
}

/// Runs the shared contract over a [`Classifier`] adapter constructed by `make`.
pub fn classifier_suite<C: Classifier>(make: &dyn Fn() -> C) {
    let classifier = make();
    let vocab = Vocab::from_schema();
    assert!(
        !classifier.id().is_empty(),
        "classifier id must not be empty"
    );

    for cand in [synthetic_text_candidate(), synthetic_origin_candidate()] {
        let first = classifier.classify(&cand, &vocab);
        let second = classifier.classify(&cand, &vocab);
        assert_eq!(
            first,
            second,
            "{}: same candidate must produce identical suggestions",
            classifier.id()
        );
        match first {
            Ok(suggestions) => {
                for s in &suggestions.fields {
                    assert!(
                        !is_classifier_origin_violation(s),
                        "{}: classifier returned by = origin on {:?}",
                        classifier.id(),
                        s.field
                    );
                    if let Err(reason) = validate(s, &cand, &vocab) {
                        panic!(
                            "{}: returned suggestion for {:?} failed validate: {reason:?}",
                            classifier.id(),
                            s.field
                        );
                    }
                }
            }
            Err(err) => {
                assert!(
                    !err.to_string().is_empty(),
                    "{}: empty stage refusal display",
                    classifier.id()
                );
            }
        }
    }

    let noisy = synthetic_noise_candidate();
    if let Ok(suggestions) = classifier.classify(&noisy, &vocab) {
        for s in &suggestions.fields {
            assert!(
                !is_classifier_origin_violation(s),
                "{}: classifier returned by = origin on 1MB noise",
                classifier.id()
            );
            if let Err(reason) = validate(s, &noisy, &vocab) {
                panic!(
                    "{}: noisy suggestion failed validate: {reason:?}",
                    classifier.id()
                );
            }
        }
    }
}

/// Runs the shared contract over a [`Correlator`] adapter constructed by `make`.
pub fn correlator_suite<C: Correlator>(
    make: &dyn Fn() -> C,
    view: &CorpusView,
    index: &dyn IndexPort,
) {
    let correlator = make();
    assert!(
        !correlator.id().is_empty(),
        "correlator id must not be empty"
    );

    for cand in [synthetic_text_candidate(), synthetic_origin_candidate()] {
        let first = correlator.correlate(&cand, view, index);
        let second = correlator.correlate(&cand, view, index);
        assert_eq!(
            first,
            second,
            "{}: same candidate must produce identical links",
            correlator.id()
        );
        if let Ok(links) = first {
            for link in &links {
                assert!(
                    !link.support.is_empty(),
                    "{}: link support must never be empty",
                    correlator.id()
                );
                if link.kind == LinkKind::Present {
                    assert!(
                        is_valid_present_link(link),
                        "{}: invalid Present link shape",
                        correlator.id()
                    );
                }
                assert!(
                    view.has_target(&link.target),
                    "{}: link target `{}` does not exist in view/Corpus",
                    correlator.id(),
                    link.target
                );
            }
        }
    }

    let noisy = synthetic_noise_candidate();
    if let Ok(links) = correlator.correlate(&noisy, view, index) {
        for link in &links {
            assert!(
                !link.support.is_empty(),
                "{}: noisy link support must never be empty",
                correlator.id()
            );
            assert!(
                view.has_target(&link.target),
                "{}: noisy link target `{}` does not exist in view",
                correlator.id(),
                link.target
            );
        }
    }
}

/// Runs the shared contract over an [`Admit`] adapter constructed by `make`.
pub fn admit_suite<A: Admit>(
    make: &dyn Fn() -> A,
    view: &CorpusView,
    proposal: &Proposal,
    extraction: Option<&VerifiedExtraction>,
) {
    let adapter = make();
    let borrowed = BorrowedAdmit(&adapter);
    assert!(!adapter.id().is_empty(), "admit id must not be empty");
    assert_eq!(borrowed.id(), adapter.id());

    let edits = Edits::default();
    let first = adapter.plan(proposal, &edits, view, extraction);
    let second = adapter.plan(proposal, &edits, view, extraction);
    let via_borrowed = borrowed.plan(proposal, &edits, view, extraction);
    assert_eq!(
        first,
        second,
        "{}: repeated admission plan must be identical",
        adapter.id()
    );
    assert_eq!(
        first,
        via_borrowed,
        "{}: BorrowedAdmit must forward plan identically",
        adapter.id()
    );

    if let Ok(AdmitOutcome::Planned { files, affected }) = first {
        assert!(
            !affected.is_empty(),
            "{}: planned outcome must list affected ItemRefs",
            adapter.id()
        );
        for pw in &files {
            if let WriteSource::Rendered(bytes) = &pw.source {
                assert_eq!(
                    sha256_hex(bytes),
                    pw.file.sha256,
                    "{}: rendered hash mismatch for {}",
                    adapter.id(),
                    pw.file.path
                );
            }
        }
        if proposal.kind.is_source_bearing() {
            assert_fixed_source_write_order(&files, adapter.id());
        }
    }

    let mut non_built = Proposal {
        id: "P-9998".to_owned(),
        state: ProposalState::Proposed,
        kind: ProposalKind::Note,
        labels: LabelSet::public(),
        writer: 0,
        origin: None,
        candidate: None,
        extraction: None,
        source_choice: None,
        body: ProposalBody::Opaque(toml::Table::new()),
        hints: Vec::new(),
        links: Default::default(),
        ai: None,
    };
    let Err(note_err) = adapter.plan(&non_built, &edits, view, None) else {
        panic!("{}: unhandled kind must refuse", adapter.id());
    };
    assert!(
        matches!(note_err, AdmitError::KindNotBuilt(ProposalKind::Note)),
        "{}: expected KindNotBuilt for note, got {note_err:?}",
        adapter.id()
    );

    non_built.kind = ProposalKind::RegressionCase;
    let Err(reg_err) = adapter.plan(&non_built, &edits, view, None) else {
        panic!("{}: unhandled regression_case must refuse", adapter.id());
    };
    assert!(matches!(
        reg_err,
        AdmitError::KindNotBuilt(ProposalKind::RegressionCase)
    ));
}

fn assert_fixed_source_write_order(
    files: &[fieldbook_core::pipeline::PlannedWrite],
    adapter_id: &str,
) {
    let mut stage = 0u8;
    let mut saw_register = false;
    for pw in files {
        let p = pw.file.path.as_str();
        let next_stage = if p.starts_with("corpus/sources/public/") {
            0
        } else if p.starts_with("corpus/sources/text/") && p.ends_with(".txt") {
            1
        } else if p.starts_with("corpus/sources/text/") && p.ends_with("/extraction.toml") {
            2
        } else if p == "corpus/registers/sources.csv" {
            saw_register = true;
            3
        } else {
            panic!("{adapter_id}: unexpected write path in source plan: {p}");
        };
        assert!(
            next_stage >= stage,
            "{adapter_id}: write order violated at `{p}` (stage {next_stage} after {stage})"
        );
        stage = next_stage;
    }
    assert!(saw_register, "{adapter_id}: sources.csv must be last");
}

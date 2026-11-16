//! Pipeline contract tests, proposal parsing, vocabulary rules, and admission (`pipeline-ports`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "integration tests load repository fixtures"
)]

use std::collections::BTreeMap;
use std::path::Path;

use fieldbook_core::add::{RESERVED_SOURCE_FIELDS, SourceRow, render_source_row};
use fieldbook_core::check::{self, Code, Entry, Host, Options, Tree};
use fieldbook_core::date::{CivilDate, PartialDate};
use fieldbook_core::extraction::{Extraction, Extractor, PageText, Plain, build_record};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::{SourceId, VersionId};
use fieldbook_core::labels::{
    LabelContext, LabelId, LabelSet, LabelTable, MARKED, MarkingRules, OWNER_SUPPLIED, PUBLIC,
    PublicAct, StoreId, UNLABELLED,
};
use fieldbook_core::pipeline::admit::apply_edits;
use fieldbook_core::pipeline::{
    Admit, AdmitError, AdmitOutcome, BorrowedAdmit, By, Candidate, Classifier, CorpusAdd,
    CorpusView, Correlator, DropReason, Edits, LabelHint, Link, LinkKind, Origin, Proposal,
    ProposalBody, ProposalKind, ProposalParseError, RuleKind, RuleRef, SourceChoice, SuggestField,
    Suggestion, Suggestions, Support, TARGETS, TargetKind, VerifiedExtraction, Vocab, WriteSource,
    is_classifier_origin_violation, validate, validate_hint,
};
use fieldbook_core::ports::index::{Hits, IndexError, IndexPort, Query};
use fieldbook_core::scan::marking::PATTERNS;
use fieldbook_core::schema::Register;

#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included per integration test binary"
)]
#[path = "../../../tests/support/rng.rs"]
mod rng;

#[path = "support/pipeline_suites.rs"]
mod pipeline_suites;

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn insert_dir_recursive(tree: &mut Tree, dir: &Path, prefix: &str) {
    tree.insert_dir(prefix);
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}/{name}");
        let path = entry.path();
        if path.is_dir() {
            insert_dir_recursive(tree, &path, &rel);
        } else {
            let bytes = std::fs::read(&path).unwrap();
            if rel.starts_with("corpus/sources/public/") {
                tree.insert(
                    &rel,
                    Entry::Original {
                        len: bytes.len() as u64,
                        sha256: sha256_hex(&bytes),
                    },
                );
            } else {
                tree.insert_bytes(&rel, bytes);
            }
        }
    }
}

fn mini_corpus_view() -> CorpusView {
    let root = repo_root();
    let corpus_dir = root.join("tests/fixtures/corpus-mini/corpus");
    let today_raw = std::fs::read_to_string(root.join("tests/fixtures/corpus-mini/TODAY")).unwrap();
    let today = CivilDate::parse(today_raw.trim()).unwrap();
    let mut tree = Tree::new();
    insert_dir_recursive(&mut tree, &corpus_dir, "corpus");
    let (model, _report) = check::load(&Host::new(tree, today), &Options::new(false));
    CorpusView::from_model(&model)
}

fn load_state_intake_verified() -> (Vec<u8>, VerifiedExtraction) {
    let state_dir = repo_root().join("tests/fixtures/state-intake");
    let inbox_bytes = std::fs::read(state_dir.join("inbox/synthetic_notice.txt")).unwrap();
    let orig_sha = sha256_hex(&inbox_bytes);
    let staging = state_dir.join("staging").join(&orig_sha);
    let toml_bytes = std::fs::read(staging.join("extraction.toml")).unwrap();
    let p1 = std::fs::read(staging.join("p0001.txt")).unwrap();
    let mut page_files = BTreeMap::new();
    page_files.insert("p0001".to_owned(), p1);
    let verified = VerifiedExtraction::verify(&orig_sha, &toml_bytes, page_files).unwrap();
    (inbox_bytes, verified)
}

fn load_queue_proposal(name: &str) -> (String, Proposal) {
    let path = repo_root().join("tests/fixtures/queue").join(name);
    let text = std::fs::read_to_string(&path).unwrap();
    let id = "P-0001";
    let proposal = Proposal::parse(id, &text).unwrap();
    (text, proposal)
}

#[test]
fn contract_admit_corpus_add_1() {
    let view = mini_corpus_view();
    let (_bytes, verified) = load_state_intake_verified();
    let input_path = repo_root().join("tests/fixtures/ports/admit/corpus-add@1/input.toml");
    let input_text = std::fs::read_to_string(&input_path).unwrap();
    let proposal = Proposal::parse("P-0001", &input_text).unwrap();

    pipeline_suites::admit_suite(&|| CorpusAdd, &view, &proposal, Some(&verified));

    let expect_path = repo_root().join("tests/fixtures/ports/admit/corpus-add@1/expect.toml");
    let expect: toml::Table =
        toml::from_str(&std::fs::read_to_string(&expect_path).unwrap()).unwrap();
    assert_eq!(
        expect.get("admit_id").and_then(|v| v.as_str()),
        Some(CorpusAdd.id())
    );
}

#[test]
fn fixture_hashes_match() {
    let state_dir = repo_root().join("tests/fixtures/state-intake");
    let inbox_bytes = std::fs::read(state_dir.join("inbox/synthetic_notice.txt")).unwrap();
    let orig_sha = sha256_hex(&inbox_bytes);
    let staging_dir = state_dir.join("staging").join(&orig_sha);
    assert!(staging_dir.is_dir(), "missing staging dir for {orig_sha}");

    let extraction = Plain.extract(&inbox_bytes, "txt").unwrap();
    let expected_record = build_record(&extraction, "plain", "1", &orig_sha, &BTreeMap::new());
    let actual_toml = std::fs::read_to_string(staging_dir.join("extraction.toml")).unwrap();
    assert_eq!(actual_toml, expected_record.render());

    let actual_p1 = std::fs::read(staging_dir.join("p0001.txt")).unwrap();
    assert_eq!(sha256_hex(&actual_p1), expected_record.pages["p0001"]);
}

#[test]
fn suggestible_fields_exclude_the_reserved_columns() {
    let vocab = Vocab::from_schema();
    for field in SuggestField::CORPUS_FIELDS {
        let name = field.as_str();
        assert!(
            !RESERVED_SOURCE_FIELDS.contains(&name),
            "suggestible field `{name}` must never overlap RESERVED_SOURCE_FIELDS"
        );
        assert!(vocab.offers(field));
    }
    assert!(!vocab.offers(SuggestField::DocKind));
    for reserved in RESERVED_SOURCE_FIELDS {
        if reserved != "doc_kind" {
            assert!(
                SuggestField::parse(reserved).is_none(),
                "reserved column `{reserved}` must not parse as SuggestField"
            );
        }
        if let Some(field) = SuggestField::parse(reserved) {
            assert!(
                !vocab.offers(field),
                "reserved column `{reserved}` must not be offered by Corpus Vocab"
            );
        }
    }
}

#[test]
fn a_label_hint_never_sets_the_proposal_labels() {
    let label = LabelId::parse("np-0007").unwrap();
    let table = LabelTable::with([label.clone()], []);
    let cand = Candidate::from_extraction(
        "doc.txt",
        sha256_hex(b"Synthetic quote inside doc"),
        26,
        "txt",
        None,
        Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: "Synthetic quote inside doc".to_owned(),
            }],
        }),
        &MarkingRules::builtin(),
        &LabelContext::act(PublicAct::typed(false, true).unwrap()),
    );
    let hint = LabelHint {
        label: label.clone(),
        support: Support::Quote {
            page: 1,
            quote: "Synthetic quote inside doc".to_owned(),
        },
        by: By::Rules,
    };
    assert!(validate_hint(&hint, &cand, &table).is_ok());
    assert_eq!(cand.labels, LabelSet::public());
    assert!(!cand.labels.contains(&label));

    let raw_proposal = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
labels = [\"public\"]
writer = 0

[candidate]
file = \"doc.txt\"
sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"
bytes = 26

[[hints]]
label = \"np-0007\"
by = \"rules@1\"
because = \"p.1: \\\"Synthetic quote inside doc\\\"\"
";
    let parsed = Proposal::parse("P-0010", raw_proposal).unwrap();
    assert_eq!(parsed.labels, LabelSet::public());
    assert_eq!(parsed.hints.len(), 1);
    assert_eq!(parsed.hints[0].label, label);
    assert!(!parsed.labels.contains(&label));
}

#[test]
fn a_hint_for_public_unlabelled_or_marked_is_dropped() {
    let table = LabelTable::builtin();
    let cand = Candidate {
        file: "doc.txt".to_owned(),
        sha256: sha256_hex(b"Synthetic text"),
        bytes: 14,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: "Synthetic text".to_owned(),
            }],
        }),
    };
    for builtin in [PUBLIC, UNLABELLED, MARKED, OWNER_SUPPLIED] {
        let hint = LabelHint {
            label: builtin,
            support: Support::Quote {
                page: 1,
                quote: "Synthetic text".to_owned(),
            },
            by: By::Rules,
        };
        assert_eq!(
            validate_hint(&hint, &cand, &table),
            Err(DropReason::BuiltinLabelHint)
        );
    }
}

#[test]
fn a_hint_outside_the_label_table_is_dropped() {
    let table = LabelTable::with([LabelId::parse("np-0001").unwrap()], []);
    let cand = Candidate {
        file: "doc.txt".to_owned(),
        sha256: sha256_hex(b"Synthetic text"),
        bytes: 14,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: "Synthetic text".to_owned(),
            }],
        }),
    };
    let unknown_hint = LabelHint {
        label: LabelId::parse("np-9999").unwrap(),
        support: Support::Quote {
            page: 1,
            quote: "Synthetic text".to_owned(),
        },
        by: By::Rules,
    };
    assert_eq!(
        validate_hint(&unknown_hint, &cand, &table),
        Err(DropReason::UnknownLabelHint)
    );
}

#[test]
fn candidate_labels_come_from_the_labeller() {
    let ext = Extraction {
        method: "plain",
        pages: vec![PageText {
            number: 1,
            text: "Clean public synthetic notice.".to_owned(),
        }],
    };
    let pub_cand = Candidate::from_extraction(
        "clean.txt",
        sha256_hex(b"clean"),
        5,
        "txt",
        None,
        Some(ext.clone()),
        &MarkingRules::builtin(),
        &LabelContext::act(PublicAct::typed(false, true).unwrap()),
    );
    assert_eq!(pub_cand.labels, LabelSet::public());

    let unlabelled_cand = Candidate::from_extraction(
        "clean.txt",
        sha256_hex(b"clean"),
        5,
        "txt",
        None,
        Some(ext),
        &MarkingRules::builtin(),
        &LabelContext::none(),
    );
    assert_eq!(unlabelled_cand.labels, LabelSet::unlabelled());
}

#[test]
fn a_banner_in_an_inbox_file_makes_it_marked() {
    let banner = PATTERNS[0].text;
    let body = format!("Header: {banner}\nSynthetic text body.");
    let ext = Extraction {
        method: "plain",
        pages: vec![PageText {
            number: 1,
            text: body,
        }],
    };
    let cand = Candidate::from_extraction(
        "marked.txt",
        sha256_hex(b"marked"),
        64,
        "txt",
        None,
        Some(ext),
        &MarkingRules::builtin(),
        &LabelContext::act(PublicAct::typed(false, true).unwrap()),
    );
    assert!(cand.labels.contains(&MARKED));
}

#[test]
fn origin_is_optional_and_round_trips() {
    let (_plain_text, plain_prop) = load_queue_proposal("new_source.toml");
    assert!(plain_prop.origin.is_none());

    let with_series_and_wb = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_version\"
labels = [\"public\"]
writer = 0

[origin]
system = \"synth-drop\"
stream = \"monthly\"
key = \"2026-03/notice.txt\"
as_of = \"2026-03\"
retrieved = \"2026-03-02\"
base = \"S-0001-v1\"
series = \"monthly-notices\"
workbook = true

[candidate]
file = \"notice.txt\"
sha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"
bytes = 285
";
    let parsed = Proposal::parse("P-0020", with_series_and_wb).unwrap();
    let origin = parsed.origin.as_ref().unwrap();
    assert_eq!(origin.series.as_deref(), Some("monthly-notices"));
    assert!(origin.workbook);
    assert_eq!(
        origin.base.as_ref().map(VersionId::as_str),
        Some("S-0001-v1")
    );
    assert_eq!(parsed.render(), with_series_and_wb);

    let without_optional = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
labels = [\"public\"]
writer = 0

[origin]
system = \"synth-drop\"
stream = \"monthly\"
key = \"2026-03/notice.txt\"
as_of = \"2026-03\"
retrieved = \"2026-03-02\"

[candidate]
file = \"notice.txt\"
sha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"
bytes = 285
";
    let parsed_min = Proposal::parse("P-0021", without_optional).unwrap();
    let min_origin = parsed_min.origin.as_ref().unwrap();
    assert!(min_origin.series.is_none());
    assert!(!min_origin.workbook);
    assert!(min_origin.base.is_none());
    assert_eq!(parsed_min.render(), without_optional);
}

#[test]
fn origin_workbook_false_is_refused() {
    let raw = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
labels = [\"public\"]
writer = 0

[origin]
system = \"synth-drop\"
stream = \"monthly\"
key = \"item-01\"
as_of = \"2026-03\"
retrieved = \"2026-03-02\"
workbook = false

[candidate]
file = \"notice.txt\"
sha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"
bytes = 285
";
    assert!(Proposal::parse("P-0022", raw).is_err());
}

#[test]
fn origin_series_must_have_the_form_of_a_stream_id() {
    let raw = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
labels = [\"public\"]
writer = 0

[origin]
system = \"synth-drop\"
stream = \"monthly\"
key = \"item-01\"
as_of = \"2026-03\"
retrieved = \"2026-03-02\"
series = \"INVALID_SERIES!\"

[candidate]
file = \"notice.txt\"
sha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"
bytes = 285
";
    assert!(Proposal::parse("P-0023", raw).is_err());
}

#[test]
fn origin_with_a_missing_key_is_refused() {
    let raw = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
labels = [\"public\"]
writer = 0

[origin]
system = \"synth-drop\"
stream = \"monthly\"
as_of = \"2026-03\"
retrieved = \"2026-03-02\"

[candidate]
file = \"notice.txt\"
sha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"
bytes = 285
";
    assert!(Proposal::parse("P-0024", raw).is_err());
}

#[test]
fn an_origin_value_needs_an_origin_support() {
    let vocab = Vocab::from_schema();
    let cand = Candidate {
        file: "doc.txt".to_owned(),
        sha256: sha256_hex(b"Synthetic quote"),
        bytes: 15,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: Some(Origin {
            system: "synth-sys".to_owned(),
            stream: "synth-stream".to_owned(),
            key: "k1".to_owned(),
            as_of: PartialDate::parse("2026-03-01").unwrap(),
            retrieved: CivilDate::parse("2026-03-02").unwrap(),
            base: None,
            series: None,
            workbook: false,
        }),
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: "Synthetic quote".to_owned(),
            }],
        }),
    };

    let missing_origin_support = Suggestion {
        field: SuggestField::Title,
        value: "Synthetic quote".to_owned(),
        by: By::Origin,
        support: vec![Support::Quote {
            page: 1,
            quote: "Synthetic quote".to_owned(),
        }],
        rule: None,
    };
    assert_eq!(
        validate(&missing_origin_support, &cand, &vocab),
        Err(DropReason::MissingOriginSupport)
    );

    let wrong_origin_support = Suggestion {
        field: SuggestField::Title,
        value: "Synthetic quote".to_owned(),
        by: By::Origin,
        support: vec![Support::Origin {
            system: "other-sys".to_owned(),
            stream: "synth-stream".to_owned(),
        }],
        rule: None,
    };
    assert_eq!(
        validate(&wrong_origin_support, &cand, &vocab),
        Err(DropReason::MissingOriginSupport)
    );

    let valid_origin = Suggestion {
        field: SuggestField::Title,
        value: "Synthetic quote".to_owned(),
        by: By::Origin,
        support: vec![Support::Origin {
            system: "synth-sys".to_owned(),
            stream: "synth-stream".to_owned(),
        }],
        rule: None,
    };
    assert!(validate(&valid_origin, &cand, &vocab).is_ok());
}

#[test]
fn a_classifier_cannot_return_by_origin() {
    let vocab = Vocab::from_schema();
    let cand_no_origin = Candidate {
        file: "doc.txt".to_owned(),
        sha256: sha256_hex(b"body"),
        bytes: 4,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: None,
    };
    let sug = Suggestion {
        field: SuggestField::Publisher,
        value: "Example Publisher".to_owned(),
        by: By::Origin,
        support: vec![Support::Origin {
            system: "synth-sys".to_owned(),
            stream: "synth-stream".to_owned(),
        }],
        rule: None,
    };
    assert_eq!(
        validate(&sug, &cand_no_origin, &vocab),
        Err(DropReason::OriginNotAllowed)
    );
    assert!(is_classifier_origin_violation(&sug));
}

#[test]
fn proposal_fixtures_round_trip() {
    for kind in ProposalKind::ALL {
        let fname = format!("{}.toml", kind.as_str());
        let (text, parsed) = load_queue_proposal(&fname);
        assert_eq!(parsed.kind, kind);
        assert_eq!(parsed.render(), text, "round-trip mismatch for {fname}");
    }

    // Extraction-free source proposal also round-trips cleanly and admits without extraction writes.
    let no_ext_raw = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
labels = [\"public\"]
writer = 0

[candidate]
file = \"synthetic_notice.txt\"
sha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"
bytes = 285

[source_choice]
kind = \"new\"

[row.title]
value = \"Extraction-Free Source\"
by = \"person\"
because = \"\"

[row.publisher]
value = \"Example Office\"
by = \"person\"
because = \"\"

[row.source_kind]
value = \"synthetic_fixture\"
by = \"person\"
because = \"\"

[row.licence]
value = \"Synthetic CC0-1.0\"
by = \"person\"
because = \"\"

[row.volatility]
value = \"V2\"
by = \"person\"
because = \"\"
";
    let no_ext = Proposal::parse("P-0099", no_ext_raw).unwrap();
    assert!(no_ext.extraction.is_none());
    assert_eq!(no_ext.render(), no_ext_raw);

    let view = mini_corpus_view();
    let outcome = CorpusAdd
        .plan(&no_ext, &Edits::default(), &view, None)
        .unwrap();
    let AdmitOutcome::Planned { files, .. } = outcome else {
        panic!("expected Planned outcome");
    };
    assert_eq!(files.len(), 2);
    assert!(files[0].file.path.starts_with("corpus/sources/public/"));
    assert_eq!(files[1].file.path, "corpus/registers/sources.csv");
}

#[test]
fn unknown_key_is_refused() {
    let raw = std::fs::read_to_string(
        repo_root().join("tests/fixtures/pipeline/sources/known-bad/unknown_key.toml"),
    )
    .unwrap();
    let err = Proposal::parse("P-0001", &raw).unwrap_err();
    assert!(matches!(err, ProposalParseError::UnknownKey(_)));

    let old_raw = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
zone = \"public\"
writer = 0
";
    assert_eq!(
        Proposal::parse("P-0002", old_raw),
        Err(ProposalParseError::OldFormat)
    );
}

#[test]
fn model_by_without_calls_is_refused() {
    let raw = std::fs::read_to_string(
        repo_root().join("tests/fixtures/pipeline/sources/known-bad/model_by_without_calls.toml"),
    )
    .unwrap();
    assert_eq!(
        Proposal::parse("P-0001", &raw),
        Err(ProposalParseError::ModelWithoutCalls)
    );
}

#[test]
fn validate_drops_a_value_outside_the_vocabulary() {
    let vocab = Vocab::from_schema();
    let cand = Candidate {
        file: "doc.txt".to_owned(),
        sha256: sha256_hex(b"Synthetic quote"),
        bytes: 15,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: "Synthetic quote".to_owned(),
            }],
        }),
    };
    let bad = Suggestion {
        field: SuggestField::SourceKind,
        value: "invented_kind".to_owned(),
        by: By::Rules,
        support: vec![Support::Quote {
            page: 1,
            quote: "Synthetic quote".to_owned(),
        }],
        rule: None,
    };
    assert_eq!(validate(&bad, &cand, &vocab), Err(DropReason::InvalidValue));
}

#[test]
fn validate_drops_a_quote_that_is_not_on_the_page() {
    let vocab = Vocab::from_schema();
    let cand = Candidate {
        file: "doc.txt".to_owned(),
        sha256: sha256_hex(b"Actual page one text"),
        bytes: 20,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: "Actual page one text".to_owned(),
            }],
        }),
    };
    let bad = Suggestion {
        field: SuggestField::Title,
        value: "Invented Title".to_owned(),
        by: By::Rules,
        support: vec![Support::Quote {
            page: 1,
            quote: "quote that is not present".to_owned(),
        }],
        rule: None,
    };
    assert_eq!(
        validate(&bad, &cand, &vocab),
        Err(DropReason::QuoteNotFound)
    );
}

#[test]
fn present_hash_plans_nothing() {
    let view = mini_corpus_view();
    let s1 = view.source(&SourceId::parse("S-0001").unwrap()).unwrap();
    let (_, mut prop) = load_queue_proposal("new_source.toml");
    prop.candidate.as_mut().unwrap().sha256 = s1.sha256.clone();

    let outcome = CorpusAdd
        .plan(&prop, &Edits::default(), &view, None)
        .unwrap();
    assert_eq!(
        outcome,
        AdmitOutcome::Present {
            id: VersionId::parse("S-0001-v1").unwrap()
        }
    );
}

#[test]
fn planned_write_hash_matches_its_bytes() {
    let view = mini_corpus_view();
    let (inbox_bytes, verified) = load_state_intake_verified();
    let (_, prop) = load_queue_proposal("new_source.toml");

    let mut edits = Edits::default();
    edits
        .fields
        .insert(SuggestField::Title, "Edited Synthetic Title".to_owned());
    edits
        .fields
        .insert(SuggestField::Tag, "synthetic-tag".to_owned());

    let outcome = CorpusAdd
        .plan(&prop, &edits, &view, Some(&verified))
        .unwrap();
    let AdmitOutcome::Planned { files, affected } = outcome else {
        panic!("expected Planned outcome");
    };
    assert_eq!(affected.len(), 2);

    let mut expected_completed = BTreeMap::new();
    expected_completed.insert("title".to_owned(), "Edited Synthetic Title".to_owned());
    expected_completed.insert(
        "publisher".to_owned(),
        "Example Contracting Office".to_owned(),
    );
    expected_completed.insert("source_kind".to_owned(), "synthetic_fixture".to_owned());
    expected_completed.insert("licence".to_owned(), "Synthetic CC0-1.0".to_owned());
    expected_completed.insert("volatility".to_owned(), "V2".to_owned());
    expected_completed.insert("tag".to_owned(), "synthetic-tag".to_owned());
    let parsed_cells =
        fieldbook_core::add::CompletedSourceCells::parse(&expected_completed).unwrap();
    let next_src = view.next_source_id().unwrap();
    let direct_base = SourceRow::new(
        next_src.clone(),
        1,
        "synthetic_notice.txt",
        sha256_hex(&inbox_bytes),
        view.today(),
    );
    let rendered_line =
        String::from_utf8(render_source_row(&direct_base, Some(&parsed_cells)).unwrap()).unwrap();

    let mut saw_csv = false;
    for pw in &files {
        let actual_bytes = match &pw.source {
            WriteSource::Inbox => inbox_bytes.clone(),
            WriteSource::Staging(name) => {
                let p = repo_root()
                    .join("tests/fixtures/state-intake/staging")
                    .join(sha256_hex(&inbox_bytes))
                    .join(name);
                std::fs::read(p).unwrap()
            }
            WriteSource::Rendered(b) => b.clone(),
        };
        assert_eq!(
            sha256_hex(&actual_bytes),
            pw.file.sha256,
            "hash mismatch for {}",
            pw.file.path
        );
        if pw.file.path == "corpus/registers/sources.csv" {
            saw_csv = true;
            let csv_str = std::str::from_utf8(&actual_bytes).unwrap();
            assert!(
                csv_str.ends_with(&rendered_line),
                "sources.csv must contain exact render_source_row output retaining reserved defaults"
            );
        }
    }
    assert!(saw_csv);

    // Direct add output without completed cells retains reserved defaults untouched.
    let direct_only = fieldbook_core::add::plan_add(
        view.source(&SourceId::parse("S-0001").unwrap())
            .map(|_| &view)
            .unwrap()
            .sources()
            .first()
            .map(|_| {
                let root = repo_root();
                let today_raw =
                    std::fs::read_to_string(root.join("tests/fixtures/corpus-mini/TODAY")).unwrap();
                let today = CivilDate::parse(today_raw.trim()).unwrap();
                let mut tree = Tree::new();
                insert_dir_recursive(
                    &mut tree,
                    &root.join("tests/fixtures/corpus-mini/corpus"),
                    "corpus",
                );
                check::load(&Host::new(tree, today), &Options::new(false)).0
            })
            .as_ref()
            .unwrap(),
        "synthetic_notice.txt",
        &sha256_hex(&inbox_bytes),
        &fieldbook_core::add::AddOptions {
            source: None,
            metadata_only: false,
        },
        None,
        view.today(),
    )
    .unwrap();
    assert!(matches!(
        direct_only,
        fieldbook_core::add::AddOutcome::Planned(_)
    ));
}

#[test]
fn accept_orders_original_extraction_row_last() {
    let view = mini_corpus_view();
    let (_inbox_bytes, verified) = load_state_intake_verified();
    let (_, prop) = load_queue_proposal("new_source.toml");
    let AdmitOutcome::Planned { files, .. } = CorpusAdd
        .plan(&prop, &Edits::default(), &view, Some(&verified))
        .unwrap()
    else {
        panic!("expected Planned");
    };
    let paths: Vec<&str> = files.iter().map(|f| f.file.path.as_str()).collect();
    assert_eq!(paths.len(), 4);
    assert_eq!(paths[0], "corpus/sources/public/S-0004-v1.txt");
    assert_eq!(paths[1], "corpus/sources/text/S-0004-v1/p0001.txt");
    assert_eq!(paths[2], "corpus/sources/text/S-0004-v1/extraction.toml");
    assert_eq!(paths[3], "corpus/registers/sources.csv");
}

#[test]
fn new_version_uses_the_next_version_of_the_source() {
    let view = mini_corpus_view();
    let (_inbox_bytes, verified) = load_state_intake_verified();
    let (_, prop) = load_queue_proposal("new_version.toml");
    let AdmitOutcome::Planned { files, affected } = CorpusAdd
        .plan(&prop, &Edits::default(), &view, Some(&verified))
        .unwrap()
    else {
        panic!("expected Planned");
    };
    assert!(files[0].file.path.contains("S-0001-v2"));
    assert_eq!(
        files[1].file.path,
        "corpus/sources/text/S-0001-v2/p0001.txt"
    );
    assert_eq!(affected[0].id, "S-0001");
    assert_eq!(affected[1].id, "S-0001-v2");
}

#[test]
fn source_id_is_allocated_at_plan_time() {
    let view = mini_corpus_view();
    let (text, prop) = load_queue_proposal("new_source.toml");
    assert!(!text.contains("source_id ="));
    let (_inbox_bytes, verified) = load_state_intake_verified();
    let AdmitOutcome::Planned { affected, .. } = CorpusAdd
        .plan(&prop, &Edits::default(), &view, Some(&verified))
        .unwrap()
    else {
        panic!("expected Planned");
    };
    assert_eq!(affected[0].store, StoreId::Corpus);
    assert_eq!(affected[0].id, "S-0004");
}

#[test]
fn doc_kind_is_not_offered_for_the_corpus() {
    let vocab = Vocab::from_schema();
    assert!(!vocab.offers(SuggestField::DocKind));
    assert_eq!(vocab.fields().len(), 10);
}

#[test]
fn a_suggestion_for_a_field_the_vocabulary_does_not_offer_is_dropped() {
    let vocab = Vocab::from_schema();
    let cand = Candidate {
        file: "doc.txt".to_owned(),
        sha256: sha256_hex(b"contract agreement"),
        bytes: 18,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: "contract agreement".to_owned(),
            }],
        }),
    };
    let s = Suggestion {
        field: SuggestField::DocKind,
        value: "contract".to_owned(),
        by: By::Rules,
        support: vec![Support::Quote {
            page: 1,
            quote: "contract agreement".to_owned(),
        }],
        rule: None,
    };
    assert_eq!(
        validate(&s, &cand, &vocab),
        Err(DropReason::FieldNotOffered)
    );
}

#[test]
fn every_proposal_kind_has_a_fixture() {
    let queue_dir = repo_root().join("tests/fixtures/queue");
    for kind in ProposalKind::ALL {
        let p = queue_dir.join(format!("{}.toml", kind.as_str()));
        assert!(p.is_file(), "missing queue fixture {}", p.display());
    }
}

#[test]
fn reclassify_fixture_round_trips() {
    let (text, prop) = load_queue_proposal("reclassify.toml");
    assert_eq!(prop.kind, ProposalKind::Reclassify);
    assert_eq!(prop.render(), text);
}

#[test]
fn reclassify_with_an_unknown_doc_kind_or_a_bad_series_is_refused() {
    let bad_dk = "\
proposal_version = 1
state = \"proposed\"
kind = \"reclassify\"
labels = [\"public\"]
writer = 0

[reclassify]
source_version = \"S-0001-v1\"
doc_kind = \"not_a_doc_kind\"
series = \"monthly-notices\"
";
    assert!(Proposal::parse("P-0030", bad_dk).is_err());

    let bad_series = "\
proposal_version = 1
state = \"proposed\"
kind = \"reclassify\"
labels = [\"public\"]
writer = 0

[reclassify]
source_version = \"S-0001-v1\"
doc_kind = \"contract\"
series = \"Bad Series\"
";
    assert!(Proposal::parse("P-0031", bad_series).is_err());
}

#[test]
fn reclassify_that_changes_nothing_is_refused() {
    let raw = std::fs::read_to_string(
        repo_root()
            .join("tests/fixtures/pipeline/sources/known-bad/reclassify_changes_nothing.toml"),
    )
    .unwrap();
    assert_eq!(
        Proposal::parse("P-0001", &raw),
        Err(ProposalParseError::ReclassifyChangesNothing)
    );
}

#[test]
fn corpus_add_refuses_reclassify_as_not_built() {
    let view = mini_corpus_view();
    let (_text, prop) = load_queue_proposal("reclassify.toml");
    assert_eq!(
        CorpusAdd.plan(&prop, &Edits::default(), &view, None),
        Err(AdmitError::KindNotBuilt(ProposalKind::Reclassify))
    );
}

#[test]
fn proposal_kind_targets_are_rows_of_targets() {
    for kind in ProposalKind::ALL {
        if let Some(target_id) = kind.target() {
            assert!(
                TARGETS.iter().any(|t| t.id == target_id),
                "kind `{kind}` targets unknown row `{target_id}`"
            );
        }
    }
}

#[test]
fn target_columns_exist() {
    for target in TARGETS {
        match target.kind {
            TargetKind::Source => {
                let cols = Register::Sources.columns();
                for req in target.required {
                    assert!(
                        cols.iter().any(|c| c.name == *req),
                        "target `{}` requires unknown column `{req}`",
                        target.id
                    );
                }
            }
            TargetKind::Record | TargetKind::Dataset => {
                panic!("target kind {:?} has no column table in Day 1", target.kind);
            }
        }
    }
}

#[test]
fn targets_have_known_bad() {
    let base = repo_root().join("tests/fixtures/pipeline");
    for target in TARGETS {
        let dir = base.join(target.id).join("known-bad");
        assert!(
            dir.is_dir(),
            "target `{}` has no known-bad directory {}",
            target.id,
            dir.display()
        );
        let has_toml = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("toml"));
        assert!(
            has_toml,
            "target `{}` known-bad folder {} has no .toml files",
            target.id,
            dir.display()
        );
    }
}

#[test]
fn the_known_bad_proposals_are_refused_as_named() {
    let view = mini_corpus_view();
    let dir = repo_root().join("tests/fixtures/pipeline/sources/known-bad");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("toml"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    assert!(!entries.is_empty());

    for entry in entries {
        let path = entry.path();
        let text = std::fs::read_to_string(&path).unwrap();
        let first_line = text.lines().next().unwrap_or("");
        let expect = first_line
            .strip_prefix("# expect:")
            .map(str::trim)
            .unwrap_or_else(|| panic!("{}: missing `# expect:` header", path.display()));
        match expect {
            "parse" => {
                assert!(
                    Proposal::parse("P-0001", &text).is_err(),
                    "{}: expected Proposal::parse to fail",
                    path.display()
                );
            }
            "admit MissingField" => {
                let parsed = Proposal::parse("P-0001", &text).unwrap();
                let err = CorpusAdd
                    .plan(&parsed, &Edits::default(), &view, None)
                    .unwrap_err();
                assert!(
                    matches!(err, AdmitError::MissingField(_)),
                    "{}: expected MissingField, got {err:?}",
                    path.display()
                );
            }
            other => panic!("{}: unexpected directive `{other}`", path.display()),
        }
    }
}

#[test]
fn rule_provenance_round_trips_and_edit_clears_rule() {
    let raw_two_rules = "\
proposal_version = 1
state = \"proposed\"
kind = \"new_source\"
labels = [\"public\"]
writer = 0

[candidate]
file = \"synthetic_notice.txt\"
sha256 = \"89418a780df335fdd9b07e3e24271f8485ac83bc488a562d582ea13e9f88fc2a\"
bytes = 285

[row.title]
value = \"Synthetic Title\"
by = \"rules@1\"
because = 'p.1: \"identical support text\"'
rule = { id = \"rule-alpha\", kind = \"phrase\" }

[row.publisher]
value = \"Synthetic Publisher\"
by = \"rules@1\"
because = 'p.1: \"identical support text\"'
rule = { id = \"rule-beta\", kind = \"publisher\" }

[row.source_kind]
value = \"synthetic_fixture\"
by = \"rules@1\"
because = 'p.1: \"fallback\"'
";
    let mut prop = Proposal::parse("P-0050", raw_two_rules).unwrap();
    assert_eq!(prop.render(), raw_two_rules);
    let ProposalBody::Row(ref row) = prop.body else {
        panic!("expected RowBody");
    };
    assert_eq!(
        row[&SuggestField::Title].rule,
        Some(RuleRef {
            id: "rule-alpha".to_owned(),
            kind: RuleKind::Phrase
        })
    );
    assert_eq!(
        row[&SuggestField::Publisher].rule,
        Some(RuleRef {
            id: "rule-beta".to_owned(),
            kind: RuleKind::Publisher
        })
    );
    assert_eq!(row[&SuggestField::SourceKind].rule, None);

    let mut edits = Edits::default();
    edits
        .fields
        .insert(SuggestField::Title, "Human Override Title".to_owned());
    apply_edits(&mut prop, &edits);
    let ProposalBody::Row(ref edited_row) = prop.body else {
        panic!("expected RowBody");
    };
    assert_eq!(edited_row[&SuggestField::Title].by, By::Person);
    assert_eq!(edited_row[&SuggestField::Title].because, "");
    assert_eq!(edited_row[&SuggestField::Title].rule, None);
}

#[test]
fn declared_extraction_without_verified_bytes_refuses_before_writes() {
    let view = mini_corpus_view();
    let (_text, prop) = load_queue_proposal("new_source.toml");
    assert!(prop.extraction.is_some());

    let err = BorrowedAdmit(&CorpusAdd)
        .plan(&prop, &Edits::default(), &view, None)
        .unwrap_err();
    assert_eq!(err, AdmitError::Invalid(Code::Fb0212));
}

#[test]
fn source_choice_round_trips_and_needs_resolution() {
    let view = mini_corpus_view();
    let (_bytes, verified) = load_state_intake_verified();

    // Zero VersionOf links -> New; one -> Existing; two -> Unresolved.
    assert_eq!(SourceChoice::from_links(&[]), SourceChoice::New);
    let link_s1 = Link {
        kind: LinkKind::VersionOf,
        target: "S-0001".to_owned(),
        by: By::Correlator,
        support: vec![Support::Id {
            id: "S-0001".to_owned(),
            score: 10,
        }],
    };
    let link_s2 = Link {
        kind: LinkKind::VersionOf,
        target: "S-0002".to_owned(),
        by: By::Correlator,
        support: vec![Support::Id {
            id: "S-0002".to_owned(),
            score: 9,
        }],
    };
    assert_eq!(
        SourceChoice::from_links(std::slice::from_ref(&link_s1)),
        SourceChoice::Existing(SourceId::parse("S-0001").unwrap())
    );
    assert_eq!(
        SourceChoice::from_links(&[link_s1, link_s2]),
        SourceChoice::Unresolved
    );

    let (_, mut prop) = load_queue_proposal("new_source.toml");
    prop.links.version_of = vec![
        SourceId::parse("S-0001").unwrap(),
        SourceId::parse("S-0002").unwrap(),
    ];
    prop.source_choice = Some(SourceChoice::Unresolved);
    let rendered = prop.render();
    let reopened = Proposal::parse("P-0060", &rendered).unwrap();
    assert_eq!(reopened.source_choice, Some(SourceChoice::Unresolved));
    assert_eq!(reopened.links.version_of.len(), 2);

    // Unresolved must refuse (never pick the first link automatically).
    let unresolved_err = CorpusAdd
        .plan(&reopened, &Edits::default(), &view, Some(&verified))
        .unwrap_err();
    assert_eq!(unresolved_err, AdmitError::Invalid(Code::Fb0204));

    // Explicit New overrides correlations.
    let choose_new = Edits {
        source_choice: Some(SourceChoice::New),
        ..Edits::default()
    };
    let AdmitOutcome::Planned {
        affected: new_aff, ..
    } = CorpusAdd
        .plan(&reopened, &choose_new, &view, Some(&verified))
        .unwrap()
    else {
        panic!("expected Planned for explicit New");
    };
    assert_eq!(new_aff[0].id, "S-0004");

    // Existing S-0001 plans S-0001-v2; Existing S-0002 plans S-0002-v3.
    let choose_s1 = Edits {
        source_choice: Some(SourceChoice::Existing(SourceId::parse("S-0001").unwrap())),
        ..Edits::default()
    };
    let AdmitOutcome::Planned {
        affected: s1_aff, ..
    } = CorpusAdd
        .plan(&reopened, &choose_s1, &view, Some(&verified))
        .unwrap()
    else {
        panic!("expected Planned for S-0001");
    };
    assert_eq!(s1_aff[1].id, "S-0001-v2");

    let choose_s2 = Edits {
        source_choice: Some(SourceChoice::Existing(SourceId::parse("S-0002").unwrap())),
        ..Edits::default()
    };
    let AdmitOutcome::Planned {
        affected: s2_aff, ..
    } = CorpusAdd
        .plan(&reopened, &choose_s2, &view, Some(&verified))
        .unwrap()
    else {
        panic!("expected Planned for S-0002");
    };
    assert_eq!(s2_aff[1].id, "S-0002-v3");

    // Existing S-9999 not in view is refused.
    let choose_missing = Edits {
        source_choice: Some(SourceChoice::Existing(SourceId::parse("S-9999").unwrap())),
        ..Edits::default()
    };
    assert!(
        CorpusAdd
            .plan(&reopened, &choose_missing, &view, Some(&verified))
            .is_err()
    );

    // SourceChoice persists across apply_edits + render + reopen.
    let mut edited_prop = reopened.clone();
    apply_edits(&mut edited_prop, &choose_s2);
    let persisted = edited_prop.render();
    let after_reopen = Proposal::parse("P-0060", &persisted).unwrap();
    assert_eq!(after_reopen.kind, ProposalKind::NewVersion);
    assert_eq!(
        after_reopen.source_choice,
        Some(SourceChoice::Existing(SourceId::parse("S-0002").unwrap()))
    );

    // Non-existing SourceChoice variants forbid `id`; non-source kinds forbid `[source_choice]`.
    let bad_new_with_id = prop
        .render()
        .replace("kind = \"unresolved\"", "kind = \"new\"\nid = \"S-0001\"");
    assert!(Proposal::parse("P-0061", &bad_new_with_id).is_err());
    let bad_reclassify = format!(
        "{}\n[source_choice]\nkind = \"new\"\n",
        load_queue_proposal("reclassify.toml").0
    );
    assert!(Proposal::parse("P-0062", &bad_reclassify).is_err());
}

#[test]
fn opaque_proposal_rejects_non_finite_floats() {
    for literal in ["nan", "inf", "-inf"] {
        let raw = format!(
            "proposal_version = 1\nstate = \"proposed\"\nkind = \"note\"\nlabels = [\"public\"]\nwriter = 0\n\n[note]\nmetric = {literal}\n"
        );
        assert!(
            Proposal::parse("P-0070", &raw).is_err(),
            "opaque proposal must reject non-finite float `{literal}`"
        );
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let mut rng = rng::Rng::new(1);
    for idx in 0..200 {
        let len = match idx {
            0 => 0,
            1 => 2048,
            _ => usize::try_from(rng.next_u64() % 2049).unwrap_or(0),
        };
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes);
        let _ = By::parse(&text);
        if let Ok(parsed) = Proposal::parse(&format!("P-{idx:04}"), &text) {
            let rendered = parsed.render();
            let reparsed = Proposal::parse(&format!("P-{idx:04}"), &rendered).unwrap();
            assert_eq!(reparsed, parsed);
        }
    }

    for fname in ["new_source.toml", "new_version.toml", "reclassify.toml"] {
        let orig = std::fs::read(repo_root().join("tests/fixtures/queue").join(fname)).unwrap();
        for pos in 0..orig.len() {
            let mut flipped = orig.clone();
            flipped[pos] ^= 0x01;
            let flipped_txt = String::from_utf8_lossy(&flipped);
            let _ = By::parse(&flipped_txt);
            if let Ok(parsed) = Proposal::parse("P-0001", &flipped_txt) {
                let reparsed = Proposal::parse("P-0001", &parsed.render()).unwrap();
                assert_eq!(reparsed, parsed);
            }

            let mut deleted = orig.clone();
            deleted.remove(pos);
            let deleted_txt = String::from_utf8_lossy(&deleted);
            let _ = By::parse(&deleted_txt);
            if let Ok(parsed) = Proposal::parse("P-0001", &deleted_txt) {
                let reparsed = Proposal::parse("P-0001", &parsed.render()).unwrap();
                assert_eq!(reparsed, parsed);
            }
        }
    }
}

#[test]
fn classifier_and_correlator_suites_pass_on_stub_adapters() {
    struct StubClassifier;
    impl Classifier for StubClassifier {
        fn id(&self) -> &'static str {
            "stub-classifier@1"
        }
        fn classify(
            &self,
            _cand: &Candidate,
            _vocab: &Vocab,
        ) -> Result<Suggestions, fieldbook_core::pipeline::StageError> {
            Ok(Suggestions::default())
        }
    }

    struct StubIndex;
    impl IndexPort for StubIndex {
        fn id(&self) -> &'static str {
            "stub-index@1"
        }
        fn query(&self, _q: &Query) -> Result<Hits, IndexError> {
            Ok(Hits::default())
        }
    }

    struct StubCorrelator;
    impl Correlator for StubCorrelator {
        fn id(&self) -> &'static str {
            "stub-correlator@1"
        }
        fn correlate(
            &self,
            _cand: &Candidate,
            _view: &CorpusView,
            _index: &dyn IndexPort,
        ) -> Result<Vec<Link>, fieldbook_core::pipeline::StageError> {
            Ok(Vec::new())
        }
    }

    pipeline_suites::classifier_suite(&|| StubClassifier);
    let view = mini_corpus_view();
    let index = StubIndex;
    pipeline_suites::correlator_suite(&|| StubCorrelator, &view, &index);
}

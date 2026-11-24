//! Correlator behaviour for the `exact-ids+bm25@1` adapter over `corpus-mini`.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test helpers"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "integration tests load repository fixtures"
)]

use std::path::Path;

use fieldbook_core::check::{self, Code, Entry, Host, Model, Options, Tree};
use fieldbook_core::date::CivilDate;
use fieldbook_core::extraction::{Extraction, PageText};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::LabelSet;
use fieldbook_core::pipeline::bm25_index::Bm25Index;
use fieldbook_core::pipeline::correlate::{ExactIdsBm25, PURPOSE_MIN_SCORE, RELATED_LIMIT};
use fieldbook_core::pipeline::{
    Candidate, CorpusView, Correlator, Link, LinkKind, StageError, Support, is_valid_present_link,
};
use fieldbook_core::ports::index::{Hits, IndexError, IndexPort, Query, Scope};

#[path = "../../../tests/support/rng.rs"]
mod rng;

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

fn load_model(extra: Option<(&str, &str)>) -> Model {
    let root = repo_root();
    let corpus_dir = root.join("tests/fixtures/corpus-mini/corpus");
    let today_raw = std::fs::read_to_string(root.join("tests/fixtures/corpus-mini/TODAY")).unwrap();
    let today = CivilDate::parse(today_raw.trim()).unwrap();
    let mut tree = Tree::new();
    insert_dir_recursive(&mut tree, &corpus_dir, "corpus");
    if let Some((path, text)) = extra {
        tree.insert_bytes(path, text.as_bytes());
    }
    let (model, _report) = check::load(&Host::new(tree, today), &Options::new(false));
    model
}

fn mini_model() -> Model {
    load_model(None)
}

fn candidate(pages: &[&str]) -> Candidate {
    let joined = pages.join("\n");
    Candidate {
        file: "candidate.txt".to_owned(),
        sha256: sha256_hex(joined.as_bytes()),
        bytes: u64::try_from(joined.len()).unwrap(),
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: pages
                .iter()
                .enumerate()
                .map(|(index, text)| PageText {
                    number: u32::try_from(index + 1).unwrap(),
                    text: (*text).to_owned(),
                })
                .collect(),
        }),
    }
}

fn source_version_sha(view: &CorpusView, source: &str, version: u32) -> String {
    view.sources()
        .iter()
        .find(|row| row.value.source_id.as_str() == source && row.value.version == version)
        .map(|row| row.value.sha256.clone())
        .unwrap()
}

fn correlate(candidate: &Candidate) -> Vec<Link> {
    let model = mini_model();
    let view = CorpusView::from_model(&model);
    let index = Bm25Index::public(&model);
    ExactIdsBm25
        .correlate(candidate, &view, &index)
        .expect("correlate")
}

fn ids(links: &[Link]) -> Vec<&str> {
    links.iter().map(|link| link.target.as_str()).collect()
}

#[test]
fn present_hash_stops_everything() {
    let model = mini_model();
    let view = CorpusView::from_model(&model);
    let index = Bm25Index::public(&model);
    let mut cand = candidate(&["CL-CMP-0001 S-0001-v1 Q-001 XC-01"]);
    cand.sha256 = source_version_sha(&view, "S-0001", 1);

    let links = ExactIdsBm25.correlate(&cand, &view, &index).unwrap();
    assert_eq!(links.len(), 1, "only Present may come back: {links:?}");
    assert!(is_valid_present_link(&links[0]));
    assert_eq!(links[0].target, "S-0001-v1");
}

#[test]
fn exact_ids_become_links_with_their_quote() {
    let links = correlate(&candidate(&[
        "Reading CL-CMP-0001 with S-0002-v2, Q-001 and XC-01 on one page.",
    ]));
    let exact: Vec<&Link> = links
        .iter()
        .filter(|link| matches!(link.support.as_slice(), [Support::Quote { .. }]))
        .collect();
    assert_eq!(
        exact.iter().map(|l| l.target.as_str()).collect::<Vec<_>>(),
        ["CL-CMP-0001", "S-0002-v2", "Q-001", "XC-01"]
    );
    for link in exact {
        let [Support::Quote { page, quote }] = link.support.as_slice() else {
            panic!("exact-id link must carry one quote");
        };
        assert_eq!(*page, 1);
        assert!(
            quote.eq_ignore_ascii_case(&link.target),
            "quote {quote:?} must name {}",
            link.target
        );
    }
}

#[test]
fn ids_not_in_the_corpus_are_not_links() {
    let links = correlate(&candidate(&["CL-CMP-9999 S-9999 Q-999 XC-99"]));
    assert!(
        links
            .iter()
            .all(|link| link.support.iter().all(|s| matches!(s, Support::Id { .. }))),
        "an id the Corpus does not have must not become an exact-id link: {links:?}"
    );
}

#[test]
fn version_of_needs_publisher_and_title() {
    let title = "Synthetic Cost Rule, made up for tests";
    let publisher = "Synthetic Standards Board";

    let title_only = correlate(&candidate(&[title]));
    assert!(
        title_only.iter().all(|l| l.kind != LinkKind::VersionOf),
        "a title without its publisher is not VersionOf"
    );

    let publisher_only = correlate(&candidate(&[publisher]));
    assert!(
        publisher_only.iter().all(|l| l.kind != LinkKind::VersionOf),
        "a publisher without its title is not VersionOf"
    );

    let both = correlate(&candidate(&[&format!("{title}. {publisher}.")]));
    let version_of: Vec<&Link> = both
        .iter()
        .filter(|l| l.kind == LinkKind::VersionOf)
        .collect();
    assert_eq!(
        version_of.len(),
        1,
        "both on page 1 make one VersionOf link"
    );
    assert_eq!(version_of[0].target, "S-0001");
    assert_eq!(
        version_of[0].support.len(),
        2,
        "title and publisher are the two quotes"
    );

    let with_id = correlate(&candidate(&[&format!(
        "{title}. {publisher}. See S-0001-v1."
    )]));
    let version_of: Vec<&Link> = with_id
        .iter()
        .filter(|l| l.kind == LinkKind::VersionOf)
        .collect();
    assert_eq!(version_of.len(), 1);
    assert_eq!(
        version_of[0].support.len(),
        3,
        "naming the id adds a third quote"
    );
}

#[test]
fn related_is_the_top_five_by_score_then_id() {
    let links = correlate(&candidate(&[
        "Synthetic compliance reviews occur every 30 days and a billing withhold applies to open deficiencies.",
    ]));
    let related: Vec<&Link> = links
        .iter()
        .filter(|l| l.kind == LinkKind::Related)
        .collect();
    assert_eq!(RELATED_LIMIT, 5, "the related limit is five");
    assert_eq!(related.len(), 5, "expected the top five related units");
    for link in &related {
        assert!(
            matches!(link.support.as_slice(), [Support::Id { .. }]),
            "a related unit is supported by its id and score"
        );
    }
    for pair in related.windows(2) {
        let (left_score, left_id) = score_and_id(pair[0]);
        let (right_score, right_id) = score_and_id(pair[1]);
        assert!(
            left_score > right_score || (left_score == right_score && left_id < right_id),
            "related order must be score descending then id ascending: {left_id} {left_score}, {right_id} {right_score}"
        );
    }
}

fn score_and_id(link: &Link) -> (u32, &str) {
    let [Support::Id { score, .. }] = link.support.as_slice() else {
        panic!("related link without an id support: {link:?}");
    };
    (*score, link.target.as_str())
}

#[test]
fn own_source_is_not_related() {
    let title = "Synthetic Cost Rule, made up for tests";
    let publisher = "Synthetic Standards Board";
    let links = correlate(&candidate(&[&format!(
        "{title}. {publisher}. Synthetic compliance reviews occur every 30 days."
    )]));
    assert!(
        links
            .iter()
            .any(|l| l.kind == LinkKind::VersionOf && l.target == "S-0001"),
        "the candidate is a new version of S-0001"
    );
    assert!(
        links.iter().all(|l| l.kind != LinkKind::Related
            || (l.target != "S-0001" && !l.target.starts_with("S-0001-v"))),
        "the candidate's own source is never a related unit: {:?}",
        ids(&links)
    );
}

const PROBE_PAGE: &str = "\
+++
id = \"cmp-keep-probe\"
title = \"Synthetic keep probe page\"
area = \"cmp\"
subarea = \"business-systems\"
type = \"concept\"
stage = 2
readiness = \"SYNTHETIC-NOW\"
status = \"draft\"
as_of = 2026-02-01
handling = [\"np-0007\"]
terms = [\"system_review\"]
volatility = \"V2\"
+++

## Purpose

Zzcanaryword probe text for the label predicate.
";

fn probe_model() -> Model {
    load_model(Some((
        "corpus/pages/compliance/business-systems/cmp-keep-probe.md",
        PROBE_PAGE,
    )))
}

#[test]
fn a_unit_rejected_by_keep_is_never_a_hit() {
    let model = probe_model();
    let index = Bm25Index::from_model(&model, LabelSet::is_public);
    let hits = index
        .query(&Query::new("zzcanaryword", Scope::all(), 10))
        .unwrap();
    assert!(
        hits.hits.is_empty(),
        "a unit labelled np-0007 must not be in a public index: {:?}",
        hits.hits
    );
}

#[test]
fn a_predicate_that_accepts_the_label_returns_the_unit() {
    let model = probe_model();
    let index = Bm25Index::from_model(&model, |_| true);
    let hits = index
        .query(&Query::new("zzcanaryword", Scope::all(), 10))
        .unwrap();
    assert!(
        hits.hits.iter().any(
            |hit| hit.unit.id == "cmp-keep-probe" || hit.unit.id.starts_with("cmp-keep-probe#")
        ),
        "a predicate that accepts the label keeps the unit: {:?}",
        hits.hits
    );
}

#[test]
fn open_question_becomes_an_answers_link_only_above_the_floor() {
    let matching = correlate(&candidate(&[
        "Which synthetic business-system control owner approves the monthly compliance review?",
    ]));
    let answers: Vec<&Link> = matching
        .iter()
        .filter(|l| l.kind == LinkKind::Answers)
        .collect();
    assert_eq!(answers.len(), 1, "the matching open question is offered");
    assert_eq!(answers[0].target, "Q-001");
    let [Support::Id { id, score }] = answers[0].support.as_slice() else {
        panic!("an Answers link is supported by the question id and score");
    };
    assert_eq!(id, "Q-001");
    assert!(
        *score >= PURPOSE_MIN_SCORE,
        "matching score {score} must reach the floor {PURPOSE_MIN_SCORE}"
    );

    // Every open question here scores below the floor, so none is offered.
    let weak = correlate(&candidate(&["Synthetic escalation applies."]));
    assert!(
        weak.iter().all(|l| l.kind != LinkKind::Answers),
        "a question below the floor is not offered: {:?}",
        ids(&weak)
    );

    let unrelated = correlate(&candidate(&[
        "Zzqx vprmn archive retention schedule qwrtp.",
    ]));
    assert!(
        unrelated.iter().all(|l| l.kind != LinkKind::Answers),
        "an unrelated candidate answers nothing"
    );
}

#[test]
fn answered_question_is_not_offered() {
    let root = repo_root();
    let questions = std::fs::read_to_string(
        root.join("tests/fixtures/corpus-mini/corpus/registers/questions.csv"),
    )
    .unwrap();
    let answered = questions.replacen(
        ",yes,true,open,,,",
        ",yes,true,answered,Synthetic answer,,company_approved",
        1,
    );
    assert_ne!(
        answered, questions,
        "the fixture question row was not found"
    );
    let path = "corpus/registers/questions.csv";
    let model = load_model(Some((path, &answered)));
    let view = CorpusView::from_model(&model);
    let index = Bm25Index::public(&model);
    let cand = candidate(&[
        "Which synthetic business-system control owner approves the monthly compliance review?",
    ]);
    let links = ExactIdsBm25.correlate(&cand, &view, &index).unwrap();
    assert!(
        links
            .iter()
            .all(|l| !(l.kind == LinkKind::Answers && l.target == "Q-001")),
        "an answered question is not offered: {:?}",
        ids(&links)
    );
}

#[test]
fn no_extraction_gives_only_present() {
    let model = mini_model();
    let view = CorpusView::from_model(&model);
    let index = Bm25Index::public(&model);
    let mut cand = candidate(&["unused"]);
    cand.extraction = None;
    cand.sha256 = source_version_sha(&view, "S-0001", 1);
    let links = ExactIdsBm25.correlate(&cand, &view, &index).unwrap();
    assert_eq!(links.len(), 1);
    assert!(is_valid_present_link(&links[0]));
}

#[test]
fn every_link_has_non_empty_support() {
    for cand in [
        candidate(&["CL-CMP-0001 S-0001-v1 Q-001 XC-01"]),
        candidate(&[
            "Synthetic Cost Rule, made up for tests. Synthetic Standards Board. Synthetic compliance reviews occur every 30 days.",
        ]),
    ] {
        for link in correlate(&cand) {
            assert!(
                !link.support.is_empty(),
                "a link without support is a guess: {link:?}"
            );
        }
    }
}

#[test]
fn same_input_same_output() {
    let model = mini_model();
    let view = CorpusView::from_model(&model);
    let index = Bm25Index::public(&model);
    let cand = candidate(&[
        "Which synthetic business-system control owner approves the monthly compliance review?",
    ]);
    let first = ExactIdsBm25.correlate(&cand, &view, &index);
    let second = ExactIdsBm25.correlate(&cand, &view, &index);
    assert_eq!(first, second, "the same input must give the same links");
}

struct FailingIndex;

impl IndexPort for FailingIndex {
    fn id(&self) -> &'static str {
        "failing-index@1"
    }

    fn query(&self, _query: &Query) -> Result<Hits, IndexError> {
        Err(IndexError::new(Code::Fb0930, "planted index failure"))
    }
}

#[test]
fn index_failure_names_the_adapter() {
    let model = mini_model();
    let view = CorpusView::from_model(&model);
    let cand = candidate(&["Synthetic compliance reviews occur every 30 days."]);
    let err = ExactIdsBm25
        .correlate(&cand, &view, &FailingIndex)
        .unwrap_err();
    let StageError::Failed(code) = err else {
        panic!("an index failure is Failed, got {err:?}");
    };
    assert_eq!(code, Code::Fb0930);
    let line = format!("{}: {err}", ExactIdsBm25.id());
    assert!(
        line.starts_with("exact-ids+bm25@1") && line.contains("FB0930"),
        "the line names the adapter and the code: {line}"
    );
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let model = mini_model();
    let view = CorpusView::from_model(&model);
    let index = Bm25Index::public(&model);

    let fixture = std::fs::read(
        repo_root().join("tests/fixtures/ports/correlator/exact-ids+bm25@1/input.txt"),
    )
    .unwrap();
    let base = fixture[..fixture.len().min(512)].to_vec();

    let mut rng = rng::Rng::new(1);
    for idx in 0..200 {
        let len = match idx {
            0 => 0,
            1 => 2048,
            _ => usize::try_from(rng.next_u64() % 2049).unwrap(),
        };
        let bytes = rng.bytes(len);
        check_bytes(&bytes, &view, &index);
    }

    for pos in 0..base.len() {
        let mut flipped = base.clone();
        flipped[pos] ^= 0x01;
        check_bytes(&flipped, &view, &index);

        let mut deleted = base.clone();
        deleted.remove(pos);
        check_bytes(&deleted, &view, &index);
    }
}

fn check_bytes(bytes: &[u8], view: &CorpusView, index: &Bm25Index) {
    let text = String::from_utf8_lossy(bytes).into_owned();
    let mut cand = candidate(&[&text]);
    cand.sha256 = sha256_hex(bytes);
    cand.bytes = u64::try_from(bytes.len()).unwrap();
    match ExactIdsBm25.correlate(&cand, view, index) {
        Ok(limits) => {
            for link in limits {
                assert!(
                    !link.support.is_empty(),
                    "mutated input produced a link without support: {link:?}"
                );
            }
        }
        Err(StageError::Failed(_)) => {}
        Err(StageError::Deferred(_)) => panic!("the correlator never defers"),
    }
}

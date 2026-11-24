//! Contract tests for the `bm25f@1` index and `exact-ids+bm25@1` correlator adapters.

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

use fieldbook_core::check::{self, Entry, Host, Model, Options, Tree};
use fieldbook_core::date::CivilDate;
use fieldbook_core::extraction::{Extraction, PageText};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::LabelSet;
use fieldbook_core::pipeline::bm25_index::Bm25Index;
use fieldbook_core::pipeline::correlate::ExactIdsBm25;
use fieldbook_core::pipeline::{Candidate, CorpusView, Correlator, LinkKind};
use fieldbook_core::ports::index::{IndexPort, Query, Scope};
use fieldbook_core::search::{Glossary, GoldenUnit, Index, query as search_query};
use serde::Deserialize;

#[path = "support/index_suite.rs"]
mod index_suite;
#[expect(
    dead_code,
    reason = "the correlator contract test uses only correlator_suite from the shared module"
)]
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

fn mini_model() -> Model {
    let root = repo_root();
    let corpus_dir = root.join("tests/fixtures/corpus-mini/corpus");
    let today_raw = std::fs::read_to_string(root.join("tests/fixtures/corpus-mini/TODAY")).unwrap();
    let today = CivilDate::parse(today_raw.trim()).unwrap();
    let mut tree = Tree::new();
    insert_dir_recursive(&mut tree, &corpus_dir, "corpus");
    let (model, _report) = check::load(&Host::new(tree, today), &Options::new(false));
    model
}

fn candidate_from_text(text: &str) -> Candidate {
    Candidate {
        file: "candidate.txt".to_owned(),
        sha256: sha256_hex(text.as_bytes()),
        bytes: u64::try_from(text.len()).unwrap(),
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: text.to_owned(),
            }],
        }),
    }
}

#[derive(Deserialize)]
struct SeedFile {
    units: Vec<GoldenUnit>,
    #[serde(default)]
    glossary: Glossary,
    cases: Vec<SeedCase>,
}

#[derive(Deserialize)]
struct SeedCase {
    query: String,
    expect: Vec<String>,
    #[serde(default)]
    scores: Option<Vec<u32>>,
}

fn seed_cases_match_search() {
    let raw = include_str!("../../../tests/golden/search/seed-ranked.json");
    let seed: SeedFile = serde_json::from_str(raw).expect("seed-ranked.json parses");
    let search_index = Index::from_golden(&seed.units, &seed.glossary);
    let bm25 = Bm25Index::from_index(Index::from_golden(&seed.units, &seed.glossary));

    let mut seen = 0usize;
    for case in &seed.cases {
        let expected = search_query(&search_index, &case.query, 10);
        let got = bm25
            .query(&Query::new(case.query.clone(), Scope::all(), 10))
            .expect("bm25 index query");

        let got_ids: Vec<String> = got.hits.iter().map(|h| h.unit.id.clone()).collect();
        let got_scores: Vec<u32> = got.hits.iter().map(|h| h.score).collect();
        let expected_ids: Vec<String> = expected.hits.iter().map(|h| h.id.clone()).collect();
        let expected_scores: Vec<u32> = expected.hits.iter().map(|h| h.score).collect();

        assert_eq!(
            got_ids, case.expect,
            "ids and order differ from the seed for {:?}",
            case.query
        );
        assert_eq!(
            got_ids, expected_ids,
            "ids and order differ from search for {:?}",
            case.query
        );
        assert_eq!(
            got_scores, expected_scores,
            "scores differ from search for {:?}",
            case.query
        );
        if let Some(scores) = &case.scores {
            assert_eq!(
                &got_scores, scores,
                "scores differ from the seed for {:?}",
                case.query
            );
        }
        seen += 1;
    }
    assert_eq!(seen, 10, "expected the ten seed-ranked cases");
}

#[test]
fn bm25_index_matches_search_query_on_the_seed_cases() {
    seed_cases_match_search();
}

#[test]
fn contract_index_bm25f_1() {
    let model = mini_model();
    index_suite::index_suite(&|| Bm25Index::public(&model));
    seed_cases_match_search();

    let dir = repo_root().join("tests/fixtures/ports/index/bm25f@1");
    let query = std::fs::read_to_string(dir.join("input.txt")).unwrap();
    let expect: toml::Table =
        toml::from_str(&std::fs::read_to_string(dir.join("expect.toml")).unwrap()).unwrap();

    let index = Bm25Index::public(&model);
    assert_eq!(
        expect.get("index_id").and_then(toml::Value::as_str),
        Some(index.id())
    );
    let hits = index
        .query(&Query::new(query.trim(), Scope::all(), 10))
        .expect("fixture query");
    let min_hits = expect
        .get("min_hits")
        .and_then(toml::Value::as_integer)
        .unwrap();
    assert!(
        i64::try_from(hits.hits.len()).unwrap() >= min_hits,
        "fixture query found {} hits, below {min_hits}",
        hits.hits.len()
    );
}

#[test]
fn contract_correlator_exact_ids_bm25_1() {
    let model = mini_model();
    let view = CorpusView::from_model(&model);
    let index = Bm25Index::public(&model);
    pipeline_suites::correlator_suite(&|| ExactIdsBm25, &view, &index);

    let dir = repo_root().join("tests/fixtures/ports/correlator/exact-ids+bm25@1");
    let text = std::fs::read_to_string(dir.join("input.txt")).unwrap();
    let expect: toml::Table =
        toml::from_str(&std::fs::read_to_string(dir.join("expect.toml")).unwrap()).unwrap();

    assert_eq!(
        expect.get("correlator_id").and_then(toml::Value::as_str),
        Some(ExactIdsBm25.id())
    );
    let links = ExactIdsBm25
        .correlate(&candidate_from_text(&text), &view, &index)
        .expect("fixture correlate");
    let present = links
        .iter()
        .filter(|link| link.kind == LinkKind::Present)
        .count();
    let expected_present = expect
        .get("present_links")
        .and_then(toml::Value::as_integer)
        .unwrap();
    assert_eq!(i64::try_from(present).unwrap(), expected_present);
    for link in &links {
        assert!(!link.support.is_empty(), "fixture link without support");
        assert!(
            view.has_target(&link.target),
            "fixture link target {} is not in the Corpus",
            link.target
        );
    }
}

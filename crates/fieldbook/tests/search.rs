//! Process tests for `fieldbook search` (`search-cli-and-regression`,
//! `docs/reference/search.md#the-command-line`).

#![allow(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests copy the fixture into a temporary Corpus and read the regression sets"
)]

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use fieldbook::Outcome;
use fieldbook::World;
use fieldbook::io::journal::{Recovery, set_lock_timeout_for_test, write_sync_file};
use fieldbook::io::state;
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    LabelSet, OperationIntent, OperationKey, RecordRef, Sha256, Verb, Via, Word, WriteFile,
    WriteSet, request_sha256,
};
use serde_json::Value;

use support::temp_copy;
use support::world::TestWorld;

const FIXTURE_REGRESSION: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/eval/regression.toml");
const LEAD_REGRESSION: &str = include_str!("../../../corpus/eval/regression.toml");

const CLAIMS: &str = "corpus/registers/claims.csv";
const QUESTIONS: &str = "corpus/registers/questions.csv";
const SOURCES: &str = "corpus/registers/sources.csv";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_mini() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_owned()).collect()
}

fn search(root: &Path, words: &[&str]) -> Outcome {
    let world = TestWorld::new(root);
    let mut argv = vec!["search"];
    argv.extend_from_slice(words);
    fieldbook::run(&args(&argv), &world)
}

fn fixture_search(words: &[&str]) -> Outcome {
    search(&corpus_mini(), words)
}

fn ranked_ids(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (rank, rest) = line.split_once(". ")?;
            rank.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| rest.split(' ').next().unwrap_or_default().to_owned())
        })
        .collect()
}

#[test]
fn a_claim_id_prints_the_whole_record() {
    let out = fixture_search(&["CL-CMP-0001"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .starts_with("claim_id: CL-CMP-0001\narea: cmp\nkind: fact\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("\nverified_on: 2026-02-10\n"));
    assert!(
        !out.stdout.contains("\nnote:\n") && !out.stdout.contains(": \n"),
        "empty values are skipped:\n{}",
        out.stdout
    );
}

#[test]
fn a_page_id_prints_its_path_and_front_matter() {
    let out = fixture_search(&["cmp-withhold-case"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout.starts_with(
            "path: corpus/pages/compliance/business-systems/cmp-withhold-case.md\nid: cmp-withhold-case\ntitle: "
        ),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("\nterms: system_review | billing_withhold\n")
    );
}

#[test]
fn a_source_id_lists_every_version_in_version_order() {
    let out = fixture_search(&["S-0002"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(ranked_ids(&out.stdout), ["S-0002-v1", "S-0002-v2"]);
    assert!(out.stdout.contains("(source) Synthetic Pricing Guide v1"));
    assert!(!out.stdout.contains("Showing"), "{}", out.stdout);

    let one = fixture_search(&["S-0002-v2"]);
    assert_eq!(one.code, 0);
    assert!(one.stdout.starts_with("source_id: S-0002\nversion: 2\n"));
}

#[test]
fn a_word_lists_two_lines_per_result() {
    let out = fixture_search(&["withhold"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(
        lines[0].starts_with("1. billing_withhold (term) billing withhold"),
        "{}",
        out.stdout
    );
    assert!(
        lines[1].starts_with("   public_reference · "),
        "{}",
        out.stdout
    );
    assert!(lines.len() > 4);
}

#[test]
fn words_are_joined_with_single_spaces() {
    let spread = fixture_search(&["rate", "review", "window"]);
    let quoted = fixture_search(&["rate review window"]);
    assert_eq!(spread.code, 0);
    assert_eq!(spread.stdout, quoted.stdout);
    assert!(!spread.stdout.is_empty());
}

#[test]
fn a_query_with_no_results_says_so_and_exits_one() {
    let out = fixture_search(&["xylophone", "marmalade"]);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "Not established in the library.\n");
    assert!(out.stderr.is_empty());

    let json = fixture_search(&["--json", "xylophone marmalade"]);
    assert_eq!(json.code, 1);
    let value: Value = serde_json::from_str(&json.stdout).expect("valid JSON");
    assert_eq!(value["total"], 0);
    assert_eq!(value["results"], Value::Array(Vec::new()));
}

#[test]
fn json_is_one_object_with_the_keys_of_the_reference_in_order() {
    let out = fixture_search(&["--json", "CL-"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let value: Value = serde_json::from_str(&out.stdout).expect("valid JSON");
    assert_eq!(value["report_version"], 1);
    assert_eq!(value["query"], "CL-");
    assert_eq!(value["tokens"], serde_json::json!(["cl"]));
    assert_eq!(value["total"], 29);
    let results = value["results"].as_array().expect("results array");
    assert_eq!(results.len(), 10);
    assert_eq!(results[0]["rank"], 1);
    assert_eq!(results[0]["labels"], serde_json::json!(["public"]));
    assert_eq!(results[0]["store"], "corpus");

    let order = |text: &str, keys: &[&str]| {
        let positions: Vec<usize> = keys
            .iter()
            .map(|key| text.find(&format!("\"{key}\"")).expect("key present"))
            .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{keys:?}");
    };
    order(
        &out.stdout,
        &["report_version", "query", "tokens", "total", "results"],
    );
    let first = out.stdout.split("\"results\"").nth(1).expect("results");
    order(
        first,
        &[
            "rank",
            "id",
            "kind",
            "title",
            "parent",
            "status",
            "as_of",
            "effective_from",
            "verified_on",
            "labels",
            "store",
            "matched",
            "score",
            "snippet",
        ],
    );
}

#[test]
fn json_tokens_are_the_query_tokens_of_the_raw_text() {
    // A soft hyphen between a letter and a combining accent: normalising twice composes the
    // accent, normalising once does not, and the report must name the tokens the index was asked
    // for, which `query_tokens` produces from the raw text.
    let raw = "cafe\u{ad}\u{301} CL-";
    let out = fixture_search(&["--json", raw]);
    let value: Value = serde_json::from_str(&out.stdout).expect("valid JSON");
    assert_eq!(
        value["tokens"],
        serde_json::json!(fieldbook_core::search::query_tokens(raw))
    );
}

#[test]
fn a_word_that_starts_with_a_hyphen_goes_after_double_dash() {
    let out = fixture_search(&["--json", "--", "-5%", "margin"]);
    assert!(out.code <= 1, "{}{}", out.stdout, out.stderr);
    let value: Value = serde_json::from_str(&out.stdout).expect("valid JSON");
    assert_eq!(value["query"], "-5% margin");
    let bare = fixture_search(&["-5%", "margin"]);
    assert_eq!(bare.code, 2, "without `--` the word is read as an option");
}

#[test]
fn an_index_failure_points_at_check() {
    let tmp = temp_copy(&corpus_mini());
    edit(
        tmp.path(),
        CLAIMS,
        "CL-CMP-0001,cmp,fact,",
        "CL-CMP-0001,cmp",
    );
    let out = search(tmp.path(), &["withhold"]);
    assert_eq!(out.code, 3, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stderr.contains("cannot build the search index"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("run `fieldbook check`"),
        "{}",
        out.stderr
    );
}

#[test]
fn no_words_is_a_usage_error() {
    let out = fixture_search(&[]);
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.to_ascii_lowercase().contains("usage"));
}

#[test]
fn search_writes_nothing() {
    let tmp = temp_copy(&corpus_mini());
    let before = snapshot(tmp.path());
    let out = search(tmp.path(), &["CL-"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(snapshot(tmp.path()), before);
    assert!(!tmp.path().join(".fieldbook").exists());
}

fn snapshot(root: &Path) -> BTreeSet<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeSet<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .into_owned();
                out.insert((rel, std::fs::read(&path).expect("read")));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

/// A copy of the fixture whose `claims.csv` has `count` more claims that all carry `word`.
fn with_synthetic_claims(count: usize, word: &str) -> tempfile::TempDir {
    let tmp = temp_copy(&corpus_mini());
    let path = tmp.path().join(CLAIMS);
    let mut text = std::fs::read_to_string(&path).expect("read claims");
    let template = text
        .lines()
        .find(|line| line.starts_with("CL-CMP-0001,"))
        .expect("template claim")
        .to_owned();
    for n in 0..count {
        let row = template
            .replacen("CL-CMP-0001", &format!("CL-CMP-{:04}", 100 + n), 1)
            .replacen(
                "Synthetic compliance reviews",
                &format!("{word} reviews"),
                1,
            );
        text.push_str(&row);
        text.push('\n');
    }
    std::fs::write(&path, text).expect("write claims");
    tmp
}

#[test]
fn cut_list_says_how_many() {
    let tmp = with_synthetic_claims(14, "Zephyrquartz");
    let out = search(tmp.path(), &["zephyrquartz"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(ranked_ids(&out.stdout).len(), 10);
    assert!(
        out.stdout
            .ends_with("Showing 10 of 14. fieldbook search <source id> lists every version.\n"),
        "{}",
        out.stdout
    );
    insta::assert_snapshot!("cut_list_says_how_many", out.stdout);

    let json = search(tmp.path(), &["--json", "zephyrquartz"]);
    let value: Value = serde_json::from_str(&json.stdout).expect("valid JSON");
    assert_eq!(value["total"], 14);
    assert_eq!(value["results"].as_array().expect("results").len(), 10);
}

/// A copy of the fixture with `versions` eligible versions of source `S-0099` plus one
/// quarantined version. Version `n` names the source id in its title `(n * 5) % 7` times, so the
/// scores differ and are not in version order.
fn with_many_versions(versions: usize) -> tempfile::TempDir {
    const SHA: &str = "e34377dee2fda910c9afa90f77fec98562ac9bb2e9ee6c9ad8e88686dd561d8c";
    let tmp = temp_copy(&corpus_mini());
    let path = tmp.path().join(SOURCES);
    let mut text = std::fs::read_to_string(&path).expect("read sources");
    let template = text
        .lines()
        .find(|line| line.starts_with("S-0002,2,"))
        .expect("template source version")
        .to_owned();
    let row = |version: usize, lifecycle: &str| {
        let mentions = "S-0099 ".repeat((version * 5) % 7);
        template
            .replacen("S-0002,2,", &format!("S-0099,{version},"), 1)
            .replacen(
                "\"Synthetic Pricing Guide v2, made up for tests\"",
                &format!("\"{mentions}Pillar Guide\""),
                1,
            )
            .replacen(SHA, &format!("{version:064x}"), 1)
            .replacen("S-0002-v2.txt", &format!("S-0099-v{version}.txt"), 1)
            .replacen(",active,", &format!(",{lifecycle},"), 1)
            .replacen(",S-0002-v1,", ",,", 1)
    };
    for version in 1..=versions {
        text.push_str(&row(version, "active"));
        text.push('\n');
    }
    text.push_str(&row(versions + 1, "quarantined"));
    text.push('\n');
    std::fs::write(&path, text).expect("write sources");
    tmp
}

#[test]
fn source_id_lists_more_than_ten_versions() {
    let tmp = with_many_versions(12);
    let expected: Vec<String> = (1..=12).map(|v| format!("S-0099-v{v}")).collect();

    let plain = search(tmp.path(), &["S-0099"]);
    assert_eq!(plain.code, 0, "{}", plain.stderr);
    assert_eq!(ranked_ids(&plain.stdout), expected, "{}", plain.stdout);
    assert!(
        !plain.stdout.contains("S-0099-v13"),
        "the quarantined version is not listed"
    );
    assert!(
        !plain.stdout.contains("Showing"),
        "all 12 are displayed, so there is no cut:\n{}",
        plain.stdout
    );

    let json = search(tmp.path(), &["--json", "S-0099"]);
    let value: Value = serde_json::from_str(&json.stdout).expect("valid JSON");
    assert_eq!(value["total"], 12);
    let ids: Vec<&str> = value["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| r["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, expected.iter().map(String::as_str).collect::<Vec<_>>());

    // A single version id is still one exact record, and an ordinary word keeps the ten cap.
    let one = search(tmp.path(), &["S-0099-v11"]);
    assert!(one.stdout.starts_with("source_id: S-0099\nversion: 11\n"));
    let word = search(tmp.path(), &["pillar"]);
    assert_eq!(ranked_ids(&word.stdout).len(), 10, "{}", word.stdout);
    assert!(
        word.stdout
            .ends_with("Showing 10 of 12. fieldbook search <source id> lists every version.\n"),
        "{}",
        word.stdout
    );
}

#[test]
fn unequal_scoring_versions_keep_natural_order_inside_the_exact_class() {
    let tmp = with_many_versions(12);
    // Score-first order would put the longest titles first; the exact class uses natural id order
    // alone, and the ordinary word keeps matched-count, score and natural-id order.
    let exact = search(tmp.path(), &["--json", "S-0099"]);
    let value: Value = serde_json::from_str(&exact.stdout).expect("valid JSON");
    let results = value["results"].as_array().expect("results");
    let scores: BTreeSet<u64> = results
        .iter()
        .map(|r| r["score"].as_u64().expect("score"))
        .collect();
    assert!(scores.len() > 1, "the versions must score unequally");
    let ranks: Vec<&str> = results.iter().map(|r| r["id"].as_str().unwrap()).collect();
    let mut sorted = ranks.clone();
    sorted.sort_by_key(|id| {
        id.rsplit_once("-v")
            .and_then(|(_, v)| v.parse::<u32>().ok())
            .unwrap_or(0)
    });
    assert_eq!(ranks, sorted);

    let word = search(tmp.path(), &["--json", "pillar"]);
    let value: Value = serde_json::from_str(&word.stdout).expect("valid JSON");
    let scores: Vec<u64> = value["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| r["score"].as_u64().expect("score"))
        .collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]), "{scores:?}");
}

const SHARED_RANKED: &str =
    include_str!("../../../tests/golden/search/ranked-versions-and-keys.json");

/// The expected ranks of the source cases of `ranked-versions-and-keys.json`, which the Rust
/// ranking test already pins, hold through the CLI over the same three source versions. The record
/// cases join this test when company search lands.
#[test]
fn shared_version_ranks_hold_through_the_cli() {
    let golden: Value = serde_json::from_str(SHARED_RANKED).expect("golden parses");
    let units = golden["units"].as_array().expect("units");

    let tmp = temp_copy(&corpus_mini());
    let path = tmp.path().join(SOURCES);
    let mut text = std::fs::read_to_string(&path).expect("read sources");
    let template = text
        .lines()
        .find(|line| line.starts_with("S-0002,2,"))
        .expect("template source version")
        .to_owned();
    for unit in units.iter().filter(|u| u["kind"] == "source") {
        let id = unit["id"].as_str().expect("unit id");
        let (source, version) = id.rsplit_once("-v").expect("versioned id");
        let title = unit["fields"]["title"].as_str().expect("title");
        let note = unit["fields"]["body"].as_str().expect("body");
        let row = template
            .replacen("S-0002,2,", &format!("{source},{version},"), 1)
            .replacen(
                "\"Synthetic Pricing Guide v2, made up for tests\"",
                &format!("\"{title}\""),
                1,
            )
            .replacen(
                "e34377dee2fda910c9afa90f77fec98562ac9bb2e9ee6c9ad8e88686dd561d8c",
                &format!("{:064x}", version.parse::<u64>().expect("version number")),
                1,
            )
            .replacen("S-0002-v2.txt", &format!("{id}.txt"), 1)
            .replacen(
                "Synthetic fixture second version superseding S-0002-v1.",
                note,
                1,
            )
            .replacen(",S-0002-v1,", ",,", 1);
        text.push_str(&row);
        text.push('\n');
    }
    std::fs::write(&path, text).expect("write sources");

    // Every case whose expected units are all sources goes through the CLI; the count guards the
    // selection, so a rename or a removal in the golden fails instead of running nothing.
    let source_ids: BTreeSet<&str> = units
        .iter()
        .filter(|u| u["kind"] == "source")
        .map(|u| u["id"].as_str().expect("unit id"))
        .collect();
    let mut ran = 0;
    for case in golden["cases"].as_array().expect("cases") {
        let id = case["id"].as_str().expect("case id");
        let expected = case["expect"].as_array().expect("expect");
        if expected.is_empty()
            || !expected
                .iter()
                .all(|v| source_ids.contains(v.as_str().expect("expected id")))
        {
            continue;
        }
        ran += 1;
        let query = case["query"].as_str().expect("query");
        let expect: Vec<&str> = case["expect"]
            .as_array()
            .expect("expect")
            .iter()
            .map(|v| v.as_str().expect("expected id"))
            .collect();
        let out = search(tmp.path(), &[query]);
        assert_eq!(out.code, 0, "{id}: {}", out.stderr);
        let ranked = ranked_ids(&out.stdout);
        let shown: Vec<&str> = ranked
            .iter()
            .map(String::as_str)
            .filter(|rid| expect.contains(rid))
            .collect();
        assert_eq!(shown, expect, "{id} `{query}`:\n{}", out.stdout);
        assert_eq!(&ranked[..expect.len()], expect, "{id} `{query}`");
    }
    assert!(ran >= 2, "only {ran} shared source cases ran");
}

#[test]
#[expect(
    clippy::print_stdout,
    reason = "the timing of the regression queries goes to the test log"
)]
fn regression_queries_run_within_the_budget() {
    let cases = fieldbook_core::eval::parse(FIXTURE_REGRESSION).expect("fixture set parses");
    for case in &cases {
        let started = Instant::now();
        let out = fixture_search(&[case.query.as_str()]);
        let elapsed = started.elapsed();
        println!(
            "regression {} `{}`: exit {} in {elapsed:?}",
            case.id, case.query, out.code
        );
        assert_eq!(out.code, u8::from(case.expect_none));
    }
}

/// A copy of the fixture where `S-0099` version 1 is written twice, the first row in
/// `first_lifecycle` and the second active.
fn with_repeated_key(first_lifecycle: &str) -> tempfile::TempDir {
    const SHA: &str = "e34377dee2fda910c9afa90f77fec98562ac9bb2e9ee6c9ad8e88686dd561d8c";
    let tmp = temp_copy(&corpus_mini());
    let path = tmp.path().join(SOURCES);
    let mut text = std::fs::read_to_string(&path).expect("read sources");
    let template = text
        .lines()
        .find(|line| line.starts_with("S-0002,2,"))
        .expect("template source version")
        .to_owned();
    let row = |title: &str, note: &str, lifecycle: &str, sha_tail: &str| {
        template
            .replacen("S-0002,2,", "S-0099,1,", 1)
            .replacen(
                "\"Synthetic Pricing Guide v2, made up for tests\"",
                title,
                1,
            )
            .replacen(
                "Synthetic fixture second version superseding S-0002-v1.",
                note,
                1,
            )
            .replacen(SHA, &format!("{sha_tail:0>64}"), 1)
            .replacen("S-0002-v2.txt", "S-0099-v1.txt", 1)
            .replacen(",active,", &format!(",{lifecycle},"), 1)
            .replacen(",S-0002-v1,", ",,", 1)
    };
    text.push_str(&row(
        "ZQTITLE",
        "ZQNOTE held for licence review",
        first_lifecycle,
        "1",
    ));
    text.push('\n');
    text.push_str(&row("Active duplicate title", "Active note", "active", "2"));
    text.push('\n');
    std::fs::write(&path, text).expect("write sources");
    tmp
}

#[test]
fn a_quarantined_row_is_never_printed_as_the_record() {
    let tmp = with_repeated_key("quarantined");
    for words in [["S-0099"], ["S-0099-v1"]] {
        let out = search(tmp.path(), &words);
        for secret in ["ZQTITLE", "ZQNOTE", "quarantined"] {
            assert!(
                !out.stdout.contains(secret),
                "`{secret}` reached `{words:?}`:\n{}",
                out.stdout
            );
        }
        assert!(
            out.stdout.contains("Active duplicate title"),
            "{}",
            out.stdout
        );
    }
    let out = search(tmp.path(), &["S-0099"]);
    assert!(
        out.stdout.starts_with("source_id: S-0099\nversion: 1\n"),
        "{}",
        out.stdout
    );
    let query = search(tmp.path(), &["ZQTITLE"]);
    assert_eq!(query.code, 1, "{}", query.stdout);
}

#[test]
fn a_repeated_register_key_prints_the_ordinary_result_not_a_guessed_record() {
    let tmp = with_repeated_key("active");
    let out = search(tmp.path(), &["S-0099"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout.starts_with("1. S-0099-v1 (source) "),
        "{}",
        out.stdout
    );
}

fn edit(root: &Path, relative: &str, find: &str, replace: &str) {
    let path = root.join(relative);
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains(find), "`{find}` is in {relative}");
    std::fs::write(&path, text.replacen(find, replace, 1)).expect("write");
}

const SYSTEM_REVIEW: &str = "corpus/pages/compliance/business-systems/cmp-system-review.md";

#[test]
fn a_multi_line_title_cannot_forge_output_lines() {
    let tmp = temp_copy(&corpus_mini());
    edit(
        tmp.path(),
        SYSTEM_REVIEW,
        "title = \"Synthetic business-system review and withholding overview\"",
        "title = \"Synthetic forged review\\n1. CL-CMP-0001 (claim) FORGED top result\\nShowing 1 of 1. nothing else matches\\nstatus = stable\"",
    );

    let list = search(tmp.path(), &["forged"]);
    assert_eq!(list.code, 0, "{}", list.stderr);
    let lines: Vec<&str> = list.stdout.lines().collect();
    assert!(
        lines[0]
            .starts_with("1. cmp-system-review (page) Synthetic forged review\\n1. CL-CMP-0001"),
        "{}",
        list.stdout
    );
    assert!(
        lines.iter().all(|line| !line.starts_with("Showing")),
        "{}",
        list.stdout
    );
    assert_eq!(ranked_ids(&list.stdout), ["cmp-system-review"]);

    let record = search(tmp.path(), &["cmp-system-review"]);
    assert_eq!(record.code, 0, "{}", record.stderr);
    let fields: Vec<&str> = record
        .stdout
        .lines()
        .take_while(|line| !line.starts_with("2. "))
        .collect();
    assert_eq!(
        fields
            .iter()
            .filter(|line| line.starts_with("status:"))
            .count(),
        1,
        "{fields:?}"
    );
    assert!(fields.contains(&"status: draft"), "{fields:?}");
    assert!(
        fields
            .iter()
            .any(|line| line.starts_with("title: Synthetic forged review\\n1. CL-CMP-0001")),
        "{fields:?}"
    );
}

#[test]
fn control_characters_in_corpus_text_never_reach_the_terminal() {
    let tmp = temp_copy(&corpus_mini());
    edit(
        tmp.path(),
        CLAIMS,
        "occur every 30 days under the synthetic rule.",
        "occur every 30 days under the synthetic rule. \u{1b}]52;c;SGVsbG8=\u{7} \u{9b}31m \u{202e}end",
    );
    for words in [["CL-CMP-0001"], ["synthetic compliance reviews"]] {
        let out = search(tmp.path(), &words);
        assert_eq!(out.code, 0, "{}", out.stderr);
        for c in out.stdout.chars() {
            assert!(
                c == '\n' || !(c.is_control() || ('\u{202a}'..='\u{202e}').contains(&c)),
                "U+{:04X} reached the terminal for {words:?}:\n{}",
                u32::from(c),
                out.stdout
            );
        }
    }
    let record = search(tmp.path(), &["CL-CMP-0001"]);
    assert!(
        record
            .stdout
            .contains("\\u{1b}]52;c;SGVsbG8=\\u{7} \\u{9b}31m \\u{202e}end"),
        "{}",
        record.stdout
    );
}

#[test]
fn a_regression_set_that_cannot_run_is_an_error_not_a_note() {
    use fieldbook_core::hash::sha256_hex;

    const BANNER: &str = "CONTROLLED UNCLASSIFIED INFORMATION";
    let tmp = temp_copy(&corpus_mini());
    let page = tmp
        .path()
        .join("corpus/pages/compliance/business-systems/cmp-review-steps.md");
    let text = std::fs::read_to_string(&page).expect("read page");
    let edited = text.replacen(
        "## Purpose\n",
        &format!(
            "## Purpose\n\nThe synthetic rule quotes the banner \"{BANNER}\" as an example.\n"
        ),
        1,
    );
    assert_ne!(edited, text, "the page has a Purpose part");
    std::fs::write(&page, &edited).expect("write page");
    std::fs::write(
        tmp.path().join("corpus/scan-allow.toml"),
        format!(
            "[[allow]]\nsha256 = \"{}\"\nmatch = \"{BANNER}\"\nreason = \"synthetic example of a printed banner\"\n",
            sha256_hex(edited.as_bytes())
        ),
    )
    .expect("write allowance");

    let world = TestWorld::new(tmp.path());
    let out = fieldbook::run(
        &args(&["check", "--strict", "--today", "2026-03-01"]),
        &world,
    );
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout
            .contains("corpus/eval/regression.toml:1:1: error[FB0126]"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("cannot be indexed"), "{}", out.stdout);
}

fn check_regression_cases(label: &str, text: &str) {
    let table: toml::Table = toml::from_str(text).unwrap_or_else(|e| panic!("{label}: {e}"));
    let cases = table
        .get("case")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("{label}: no [[case]] tables"));
    assert!(!cases.is_empty(), "{label}: no cases");
    let mut seen = BTreeSet::new();
    for case in cases {
        let id = case
            .get("id")
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("{label}: a case has no `id`"));
        let digits = id.strip_prefix("rg-").unwrap_or("");
        assert!(
            digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit()),
            "{label}: `{id}` is not of the form rg-NNN"
        );
        assert!(seen.insert(id.to_owned()), "{label}: `{id}` is repeated");
        let query = case
            .get("query")
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("{label}: `{id}` has no `query`"));
        assert!(
            !query.trim().is_empty(),
            "{label}: `{id}` has an empty query"
        );
        let has_expect = case
            .get("expect")
            .and_then(toml::Value::as_array)
            .is_some_and(|list| !list.is_empty());
        let expect_none = case
            .get("expect_none")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        assert!(
            has_expect || expect_none,
            "{label}: `{id}` has neither `expect` nor `expect_none = true`"
        );
    }
}

#[test]
fn regression_cases_have_ids_and_expectations() {
    check_regression_cases("fixture regression.toml", FIXTURE_REGRESSION);
    check_regression_cases("corpus/eval/regression.toml", LEAD_REGRESSION);

    // The check itself fails on a repeated id and on a case with no expectation.
    let repeated = FIXTURE_REGRESSION.replace("rg-003", "rg-001");
    assert!(std::panic::catch_unwind(|| check_regression_cases("repeated", &repeated)).is_err());
    let bare = "[[case]]\nid = \"rg-001\"\nquery = \"q\"\n";
    assert!(std::panic::catch_unwind(|| check_regression_cases("bare", bare)).is_err());
}

// --- Step 8: a search reads one complete generation -----------------------------------------

struct Batch {
    world: TestWorld,
    state_dir: PathBuf,
    claims: (PathBuf, Vec<u8>),
    questions: (PathBuf, Vec<u8>),
    ws: WriteSet,
    intent: OperationIntent,
}

const NEW_CLAIM: &str = "CL-CMP-0201";
const NEW_QUESTION: &str = "Q-201";

fn batch(root: &Path) -> Batch {
    let mut claims = std::fs::read_to_string(root.join(CLAIMS)).expect("claims");
    let template = claims
        .lines()
        .find(|line| line.starts_with("CL-CMP-0001,"))
        .expect("claim template")
        .to_owned();
    claims.push_str(&template.replacen("CL-CMP-0001", NEW_CLAIM, 1).replacen(
        "Synthetic compliance reviews",
        "Alphamarker reviews",
        1,
    ));
    claims.push('\n');

    let mut questions = std::fs::read_to_string(root.join(QUESTIONS)).expect("questions");
    let template = questions
        .lines()
        .find(|line| line.starts_with("Q-001,"))
        .expect("question template")
        .to_owned();
    questions.push_str(&template.replacen("Q-001", NEW_QUESTION, 1).replacen(
        "Which synthetic",
        "Betamarker which synthetic",
        1,
    ));
    questions.push('\n');

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![
            WriteFile::new(
                CLAIMS,
                Sha256::digest(claims.as_bytes()).as_str().to_owned(),
            ),
            WriteFile::new(
                QUESTIONS,
                Sha256::digest(questions.as_bytes()).as_str().to_owned(),
            ),
        ],
        affected: vec![
            RecordRef::parse(&format!("corpus/{NEW_CLAIM}")).expect("ref"),
            RecordRef::parse(&format!("corpus/{NEW_QUESTION}")).expect("ref"),
        ],
    };
    let intent = OperationIntent {
        schema: 1,
        key: OperationKey::new(
            Verb::parse("add").expect("verb"),
            Sha256::parse(&"60".repeat(32)).expect("sha"),
        ),
        request_sha256: request_sha256(&[("req", &[0x60])]).expect("request sha"),
        allocated: vec![RecordRef::parse(&format!("corpus/{NEW_CLAIM}")).expect("ref")],
        allocated_store_uid: None,
        planned_on: CivilDate::from_ymd(2026, 3, 1).expect("date"),
    };
    Batch {
        world: TestWorld::new(root),
        state_dir: root.join(".fieldbook"),
        claims: (root.join(CLAIMS), claims.into_bytes()),
        questions: (root.join(QUESTIONS), questions.into_bytes()),
        ws,
        intent,
    }
}

fn spawn_search(root: &Path, timeout: Duration) -> mpsc::Receiver<Outcome> {
    let (tx, rx) = mpsc::channel();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        set_lock_timeout_for_test(Some(timeout));
        let out = search(&root, &[NEW_CLAIM, NEW_QUESTION]);
        set_lock_timeout_for_test(None);
        let _ = tx.send(out);
    });
    rx
}

#[test]
fn search_waits_for_a_complete_generation() {
    let tmp = temp_copy(&corpus_mini());
    let b = batch(tmp.path());
    let mut journal = state::open(&b.world, &b.state_dir, Via::Cli).expect("open journal");
    let lease = b.world.write_lease().expect("write lease");
    let token = journal.begin(&lease, &b.intent, &b.ws).expect("begin");
    write_sync_file(&b.claims.0, &b.claims.1).expect("write file 1");

    // The writer is between its two files. A search that waits long enough must block here.
    let waiting = spawn_search(tmp.path(), Duration::from_secs(20));
    assert!(
        waiting.recv_timeout(Duration::from_millis(500)).is_err(),
        "the search returned while the writer held the store between its two files"
    );

    // A search with a short timeout refuses and prints no mixed content.
    let refused = spawn_search(tmp.path(), Duration::from_millis(80))
        .recv_timeout(Duration::from_secs(15))
        .expect("the short-timeout search finishes");
    assert_eq!(refused.code, 3, "{}", refused.stdout);
    assert!(refused.stdout.is_empty(), "{}", refused.stdout);
    assert!(!refused.stderr.is_empty());

    write_sync_file(&b.questions.0, &b.questions.1).expect("write file 2");
    journal.end(token, Word::new_static("ok")).expect("end");
    drop(lease);

    let out = waiting
        .recv_timeout(Duration::from_secs(30))
        .expect("the waiting search finishes once the writer is done");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let ids = ranked_ids(&out.stdout);
    let seen = |needle: &str| out.stdout.contains(needle);
    assert!(
        seen(&format!("claim_id: {NEW_CLAIM}")) || ids.contains(&NEW_CLAIM.to_owned()),
        "the new claim is missing:\n{}",
        out.stdout
    );
    assert!(
        ids.contains(&NEW_QUESTION.to_owned()) || seen(&format!("question_id: {NEW_QUESTION}")),
        "the new question is missing, so the search saw half a batch:\n{}",
        out.stdout
    );
}

#[test]
fn a_crashed_pending_transaction_is_refused_until_recovery() {
    let tmp = temp_copy(&corpus_mini());
    let b = batch(tmp.path());
    let mut journal = state::open(&b.world, &b.state_dir, Via::Cli).expect("open journal");
    let lease = b.world.write_lease().expect("write lease");
    let token = journal.begin(&lease, &b.intent, &b.ws).expect("begin");
    write_sync_file(&b.claims.0, &b.claims.1).expect("write file 1");
    drop(token);

    drop(lease);

    let refused = search(tmp.path(), &[NEW_CLAIM, NEW_QUESTION]);
    assert_eq!(refused.code, 3, "{}", refused.stdout);
    assert!(refused.stdout.is_empty(), "{}", refused.stdout);
    assert!(refused.stderr.contains("pending"), "{}", refused.stderr);

    // Recovery completes the interrupted batch; the search then sees the whole generation.
    let lease = b.world.write_lease().expect("lease for recovery");
    let recovery = journal
        .recover(
            &lease,
            &b.intent.key,
            b.intent.request_sha256.clone(),
            &b.ws,
        )
        .expect("recover");
    let Recovery::Resume { audited, .. } = recovery else {
        panic!("expected a resumable transaction");
    };
    write_sync_file(&b.questions.0, &b.questions.1).expect("write file 2");
    journal
        .end(audited, Word::new_static("ok"))
        .expect("end resumed transaction");
    drop(lease);

    let out = search(tmp.path(), &[NEW_CLAIM, NEW_QUESTION]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains(NEW_CLAIM) && out.stdout.contains(NEW_QUESTION));
}

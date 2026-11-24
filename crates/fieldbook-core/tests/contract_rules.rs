//! Contract test for the `rules@1` classifier (`pipeline-rules-classify`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "integration tests load repository fixtures"
)]
#![expect(
    dead_code,
    reason = "the shared pipeline_suites module is included per contract test binary"
)]

use std::path::Path;

use fieldbook_core::extraction::{Extraction, PageText};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::LabelSet;
use fieldbook_core::pipeline::rules::{Rules, RulesFile};
use fieldbook_core::pipeline::{Candidate, Classifier, SuggestField, Vocab};

#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included per integration test binary"
)]
#[path = "../../../tests/support/rng.rs"]
mod rng;

#[path = "support/pipeline_suites.rs"]
mod pipeline_suites;

const MINI_RULES: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/pipeline/rules.toml");

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn rules() -> Rules {
    Rules::new(RulesFile::parse(MINI_RULES, &Vocab::from_schema()).unwrap())
}

#[test]
fn contract_classifier_rules_1() {
    pipeline_suites::classifier_suite(&rules);

    let vocab = Vocab::from_schema();
    let fix_dir = repo_root().join("tests/fixtures/ports/classifier/rules@1");
    let input = std::fs::read_to_string(fix_dir.join("input.txt")).unwrap();
    let expect_toml = std::fs::read_to_string(fix_dir.join("expect.toml")).unwrap();
    let expect: toml::Value = toml::from_str(&expect_toml).unwrap();

    let cand = Candidate {
        file: "input.txt".to_owned(),
        sha256: sha256_hex(input.as_bytes()),
        bytes: input.len() as u64,
        ext: "txt".to_owned(),
        labels: LabelSet::public(),
        origin: None,
        extraction: Some(Extraction {
            method: "plain",
            pages: vec![PageText {
                number: 1,
                text: input,
            }],
        }),
    };
    let suggestions = rules().classify(&cand, &vocab).unwrap().fields;

    let expected = expect
        .get("suggestion")
        .and_then(toml::Value::as_array)
        .unwrap();
    for entry in expected {
        let table = entry.as_table().unwrap();
        let field =
            SuggestField::parse(table.get("field").and_then(toml::Value::as_str).unwrap()).unwrap();
        let value = table.get("value").and_then(toml::Value::as_str).unwrap();
        let rule_id = table.get("rule").and_then(toml::Value::as_str).unwrap();
        let kind = table.get("kind").and_then(toml::Value::as_str).unwrap();

        let found = suggestions
            .iter()
            .find(|s| s.field == field)
            .unwrap_or_else(|| panic!("missing suggestion for `{field}`"));
        assert_eq!(found.value, value, "field `{field}`");
        assert_eq!(found.by.format(), "rules@1", "field `{field}`");
        let rule = found
            .rule
            .as_ref()
            .unwrap_or_else(|| panic!("field `{field}` lacks rule provenance"));
        assert_eq!(rule.id, rule_id, "field `{field}`");
        assert_eq!(rule.kind.as_str(), kind, "field `{field}`");
    }
}

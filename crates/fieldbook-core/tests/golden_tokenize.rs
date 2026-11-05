//! Golden vector tests for search tokenization (`tests/golden/search/*.json`).

mod golden_tokenize {
    use std::collections::BTreeSet;

    use fieldbook_core::search::lexer::{index_tokens, query_tokens};
    use serde::Deserialize;

    const SEED_TOKENIZE_JSON: &str =
        include_str!("../../../tests/golden/search/seed-tokenize.json");
    const SEED_TOKENIZE_COMPANY_JSON: &str =
        include_str!("../../../tests/golden/search/seed-tokenize-company.json");
    const TOKENIZE_NORMALISATION_JSON: &str =
        include_str!("../../../tests/golden/search/tokenize-normalisation.json");
    const TOKENIZE_WORDS_JSON: &str =
        include_str!("../../../tests/golden/search/tokenize-words.json");
    const TOKENIZE_SHAPES_JSON: &str =
        include_str!("../../../tests/golden/search/tokenize-shapes.json");
    const TOKENIZE_EXTRAS_JSON: &str =
        include_str!("../../../tests/golden/search/tokenize-extras.json");
    const TOKENIZE_QUERY_JSON: &str =
        include_str!("../../../tests/golden/search/tokenize-query.json");

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GoldenFile {
        kind: String,
        version: u32,
        cases: Vec<GoldenCase>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GoldenCase {
        id: String,
        side: String,
        input: String,
        tokens: Vec<String>,
        note: String,
    }

    fn parse_golden_file(name: &str, json: &str) -> Result<GoldenFile, String> {
        let file: GoldenFile =
            serde_json::from_str(json).map_err(|e| format!("{name}: JSON parse error: {e}"))?;
        if file.kind != "tokenize" {
            return Err(format!(
                "{name}: unknown golden kind {:?} (expected \"tokenize\")",
                file.kind
            ));
        }
        if file.version != 1 {
            return Err(format!(
                "{name}: unsupported golden version {} (expected 1)",
                file.version
            ));
        }
        for case in &file.cases {
            if case.note.trim().is_empty() {
                return Err(format!("{name} ({}): empty note", case.id));
            }
            if !matches!(case.side.as_str(), "index" | "query") {
                return Err(format!(
                    "{name} ({}): unknown side {:?} (expected \"index\" or \"query\")",
                    case.id, case.side
                ));
            }
        }
        Ok(file)
    }

    fn run_case(case: &GoldenCase) -> Result<(), String> {
        let actual = match case.side.as_str() {
            "index" => index_tokens(&case.input),
            "query" => query_tokens(&case.input),
            other => return Err(format!("{}: unknown side {other:?}", case.id)),
        };
        if actual != case.tokens {
            return Err(format!(
                "{} (side={}): expected {:?}, got {:?}",
                case.id, case.side, case.tokens, actual
            ));
        }
        Ok(())
    }

    #[test]
    fn all_tokenize_golden_cases_pass() {
        let seed_files = [
            ("seed-tokenize.json", SEED_TOKENIZE_JSON),
            ("seed-tokenize-company.json", SEED_TOKENIZE_COMPANY_JSON),
        ];
        let topic_files = [
            ("tokenize-normalisation.json", TOKENIZE_NORMALISATION_JSON),
            ("tokenize-words.json", TOKENIZE_WORDS_JSON),
            ("tokenize-shapes.json", TOKENIZE_SHAPES_JSON),
            ("tokenize-extras.json", TOKENIZE_EXTRAS_JSON),
            ("tokenize-query.json", TOKENIZE_QUERY_JSON),
        ];

        let mut seen_ids = BTreeSet::new();
        let mut failures = Vec::new();
        let mut seed_count = 0usize;
        let mut total_count = 0usize;

        for (name, raw) in seed_files.into_iter().chain(topic_files) {
            let file = parse_golden_file(name, raw).unwrap_or_else(|e| panic!("{e}"));
            if name.starts_with("seed-") {
                seed_count += file.cases.len();
            }
            total_count += file.cases.len();

            for case in &file.cases {
                if !seen_ids.insert(case.id.clone()) {
                    failures.push(format!("{name}: duplicate case id {}", case.id));
                }
                if let Err(err) = run_case(case) {
                    failures.push(err);
                }
            }
        }

        assert_eq!(
            seed_count, 52,
            "expected 52 seed cases across seed-tokenize.json and seed-tokenize-company.json, got {seed_count}"
        );
        assert!(
            total_count >= 200,
            "expected at least 200 total tokenize golden cases, got {total_count}"
        );
        assert!(
            failures.is_empty(),
            "tokenize golden failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn loader_rejects_unknown_fields_kind_and_version() {
        let unknown_top = r#"{"kind":"tokenize","version":1,"extra":true,"cases":[]}"#;
        assert!(parse_golden_file("test.json", unknown_top).is_err());

        let unknown_kind = r#"{"kind":"other","version":1,"cases":[]}"#;
        assert!(parse_golden_file("test.json", unknown_kind).is_err());

        let unknown_version = r#"{"kind":"tokenize","version":2,"cases":[]}"#;
        assert!(parse_golden_file("test.json", unknown_version).is_err());

        let unknown_case_field = r#"{
            "kind": "tokenize",
            "version": 1,
            "cases": [
                {"id": "tok-999", "side": "query", "input": "x", "tokens": ["x"], "note": "n", "bogus": 1}
            ]
        }"#;
        assert!(parse_golden_file("test.json", unknown_case_field).is_err());

        let unknown_side = r#"{
            "kind": "tokenize",
            "version": 1,
            "cases": [
                {"id": "tok-999", "side": "other", "input": "x", "tokens": ["x"], "note": "n"}
            ]
        }"#;
        assert!(parse_golden_file("test.json", unknown_side).is_err());
    }
}

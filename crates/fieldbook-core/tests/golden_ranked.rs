//! Golden vector tests for BM25F ranking, index construction, and query execution
//! (`tests/golden/search/seed-ranked.json` and `tests/golden/search/ranked-*.json`).

mod golden_ranked {
    use std::collections::BTreeSet;

    use fieldbook_core::search::{Glossary, GoldenUnit, Index, lookup_unit_kind, query};
    use serde::Deserialize;

    const SEED_RANKED_JSON: &str = include_str!("../../../tests/golden/search/seed-ranked.json");
    const RANKED_WORKED_EXAMPLE_JSON: &str =
        include_str!("../../../tests/golden/search/ranked-worked-example.json");
    const RANKED_VERSIONS_AND_KEYS_JSON: &str =
        include_str!("../../../tests/golden/search/ranked-versions-and-keys.json");
    const RANKED_FIELDS_AND_KINDS_JSON: &str =
        include_str!("../../../tests/golden/search/ranked-fields-and-kinds.json");
    const RANKED_TIES_JSON: &str = include_str!("../../../tests/golden/search/ranked-ties.json");
    const RANKED_QUERY_NORMALISATION_JSON: &str =
        include_str!("../../../tests/golden/search/ranked-query-normalisation.json");
    const RANKED_EXACT_CLASS_JSON: &str =
        include_str!("../../../tests/golden/search/ranked-exact-class.json");
    const RANKED_PREFIX_BUDGET_JSON: &str =
        include_str!("../../../tests/golden/search/ranked-prefix-budget.json");
    const RANKED_LIMIT_JSON: &str = include_str!("../../../tests/golden/search/ranked-limit.json");

    /// The command-line result limit (`search.md#budgets`); a ranked case lists at most this many.
    const LIMIT: usize = 10;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RankedFile {
        kind: String,
        version: u32,
        units: Vec<GoldenUnit>,
        #[serde(default)]
        glossary: Glossary,
        cases: Vec<RankedCase>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RankedCase {
        id: String,
        query: String,
        expect: Vec<String>,
        #[serde(default)]
        scores: Option<Vec<u32>>,
        #[serde(default)]
        impacts: Option<Vec<PinnedImpact>>,
        /// Free text for readers; loaders ignore it (`tests/golden/README.md#ranked`).
        #[serde(default, rename = "note")]
        _note: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PinnedImpact {
        unit: String,
        token: String,
        impact: u32,
    }

    fn parse_ranked_file(name: &str, raw: &str) -> Result<RankedFile, String> {
        let file: RankedFile =
            serde_json::from_str(raw).map_err(|e| format!("{name}: JSON parse error: {e}"))?;
        if file.kind != "ranked" {
            return Err(format!(
                "{name}: unknown golden kind {:?} (expected \"ranked\")",
                file.kind
            ));
        }
        if file.version != 1 {
            return Err(format!(
                "{name}: unsupported golden version {} (expected 1)",
                file.version
            ));
        }
        for u in &file.units {
            if lookup_unit_kind(&u.kind).is_none() {
                return Err(format!(
                    "{name}: unit {:?} has unknown kind {:?} (not in UNIT_KINDS)",
                    u.id, u.kind
                ));
            }
        }
        Ok(file)
    }

    fn run_case(index: &Index, case: &RankedCase) -> Result<(), String> {
        let mut problems = Vec::new();
        let results = query(index, &case.query, LIMIT);
        let actual_ids: Vec<String> = results.hits.iter().map(|h| h.id.clone()).collect();
        let actual_scores: Vec<u32> = results.hits.iter().map(|h| h.score).collect();

        if actual_ids != case.expect {
            problems.push(format!(
                "expect mismatch for query {:?}: expected {:?}, got {:?} (scores {:?})",
                case.query, case.expect, actual_ids, actual_scores
            ));
        }

        // The list is the first `LIMIT` of the full ranked list and `total` counts all of it, so
        // a surface that cuts the list can say "Showing 10 of N" (`search.md#query-procedure`).
        let full = query(index, &case.query, usize::MAX);
        let cut = full.hits.len().min(LIMIT);
        if results.total != full.hits.len()
            || full.total != full.hits.len()
            || results.hits[..] != full.hits[..cut]
        {
            problems.push(format!(
                "limit mismatch for query {:?}: total {} of {} ranked hits, first {} hits differ: {}",
                case.query,
                results.total,
                full.hits.len(),
                cut,
                results.hits[..] != full.hits[..cut]
            ));
        }

        if let Some(ref exp_scores) = case.scores
            && &actual_scores != exp_scores
        {
            problems.push(format!(
                "scores mismatch for query {:?}: expected {:?}, got {:?} (ids {:?})",
                case.query, exp_scores, actual_scores, actual_ids
            ));
        }

        if let Some(ref pinned_list) = case.impacts {
            for p in pinned_list {
                let actual_imp = index.posting_impact(&p.unit, &p.token);
                if actual_imp != Some(p.impact) {
                    problems.push(format!(
                        "impact mismatch for unit {:?} token {:?}: expected Some({}), got {:?}",
                        p.unit, p.token, p.impact, actual_imp
                    ));
                }
            }
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(format!("{}: {}", case.id, problems.join("; ")))
        }
    }

    #[test]
    fn all_ranked_golden_cases_pass() {
        let files = [
            ("seed-ranked.json", SEED_RANKED_JSON),
            ("ranked-worked-example.json", RANKED_WORKED_EXAMPLE_JSON),
            (
                "ranked-versions-and-keys.json",
                RANKED_VERSIONS_AND_KEYS_JSON,
            ),
            ("ranked-fields-and-kinds.json", RANKED_FIELDS_AND_KINDS_JSON),
            ("ranked-ties.json", RANKED_TIES_JSON),
            (
                "ranked-query-normalisation.json",
                RANKED_QUERY_NORMALISATION_JSON,
            ),
            ("ranked-exact-class.json", RANKED_EXACT_CLASS_JSON),
            ("ranked-prefix-budget.json", RANKED_PREFIX_BUDGET_JSON),
            ("ranked-limit.json", RANKED_LIMIT_JSON),
        ];

        let mut seen_ids = BTreeSet::new();
        let mut failures = Vec::new();
        let mut seed_count = 0usize;
        let mut total_count = 0usize;
        let mut seed_pinned_impacts = 0usize;

        for (name, raw) in files {
            let file = parse_ranked_file(name, raw).unwrap_or_else(|e| panic!("{e}"));
            let index = Index::from_golden(&file.units, &file.glossary);

            if name.starts_with("seed-") {
                seed_count += file.cases.len();
                for c in &file.cases {
                    if let Some(ref imps) = c.impacts {
                        seed_pinned_impacts += imps.len();
                    }
                }
            }
            total_count += file.cases.len();

            for case in &file.cases {
                if !seen_ids.insert(case.id.clone()) {
                    failures.push(format!("{name}: duplicate case id {}", case.id));
                }
                if let Err(err) = run_case(&index, case) {
                    failures.push(err);
                }
            }
        }

        assert!(
            failures.is_empty(),
            "ranked golden failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
        assert_eq!(
            seed_count, 10,
            "expected 10 seed ranked cases in seed-ranked.json, found {seed_count}"
        );
        assert!(
            seed_pinned_impacts >= 5,
            "expected at least 5 pinned impacts in seed-ranked.json, found {seed_pinned_impacts}"
        );
        assert!(
            total_count >= 30,
            "expected at least 30 ranked golden cases in all, found {total_count}"
        );
    }

    #[test]
    fn loader_rejects_unknown_kind_version_unit_kind_and_fields() {
        let bad_kind = r#"{"kind":"tokenize","version":1,"units":[],"cases":[]}"#;
        assert!(parse_ranked_file("bad_kind.json", bad_kind).is_err());

        let bad_version = r#"{"kind":"ranked","version":2,"units":[],"cases":[]}"#;
        assert!(parse_ranked_file("bad_version.json", bad_version).is_err());

        let bad_unit_kind = r#"{
            "kind":"ranked",
            "version":1,
            "units":[{
                "id":"X-1",
                "kind":"unknown_kind",
                "fields":{"id":"","title":"","headings":"","claim":"","body":""},
                "display":{"title":"","parent":"","snippet":"","status":"","as_of":"","effective_from":"","verified_on":""}
            }],
            "cases":[]
        }"#;
        assert!(parse_ranked_file("bad_unit_kind.json", bad_unit_kind).is_err());

        let bad_field = r#"{"kind":"ranked","version":1,"units":[],"cases":[],"extra":1}"#;
        assert!(parse_ranked_file("bad_field.json", bad_field).is_err());
    }

    fn valid_ranked_value() -> serde_json::Value {
        serde_json::json!({
            "kind": "ranked",
            "version": 1,
            "units": [{
                "id": "X-1",
                "kind": "text",
                "fields": {"id": "", "title": "", "headings": "", "claim": "", "body": "alpha"},
                "display": {
                    "title": "", "parent": "", "snippet": "", "status": "",
                    "as_of": "", "effective_from": "", "verified_on": ""
                },
                "exact_key": ""
            }],
            "glossary": [{"label": "alpha", "aliases": ["a"]}],
            "cases": [{
                "id": "rk-x",
                "query": "alpha",
                "expect": ["X-1"],
                "scores": [1],
                "impacts": [{"unit": "X-1", "token": "alpha", "impact": 1}],
                "note": "readers only"
            }]
        })
    }

    #[test]
    fn loader_rejects_an_unknown_key_at_every_level_of_a_ranked_file() {
        let valid = valid_ranked_value();
        parse_ranked_file("valid.json", &valid.to_string()).expect("the control file loads");

        // Every struct of the ranked format: the file, a unit, its fields and display values, a
        // glossary entry, a case and one pinned impact.
        let levels = [
            ("file", ""),
            ("unit", "/units/0"),
            ("unit fields", "/units/0/fields"),
            ("unit display", "/units/0/display"),
            ("glossary entry", "/glossary/0"),
            ("case", "/cases/0"),
            ("pinned impact", "/cases/0/impacts/0"),
        ];
        for (level, pointer) in levels {
            let mut planted = valid.clone();
            planted
                .pointer_mut(pointer)
                .and_then(serde_json::Value::as_object_mut)
                .unwrap_or_else(|| panic!("{level}: no object at {pointer:?}"))
                .insert("planted_key".to_owned(), serde_json::json!(1));
            let err = parse_ranked_file(level, &planted.to_string()).expect_err(&format!(
                "an unknown key at the {level} level must be rejected"
            ));
            assert!(
                err.contains("planted_key"),
                "{level}: rejected for another reason: {err}"
            );
        }
    }
}

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
        let results = query(index, &case.query, 10);
        let actual_ids: Vec<String> = results.hits.iter().map(|h| h.id.clone()).collect();
        let actual_scores: Vec<u32> = results.hits.iter().map(|h| h.score).collect();

        if actual_ids != case.expect {
            problems.push(format!(
                "expect mismatch for query {:?}: expected {:?}, got {:?} (scores {:?})",
                case.query, case.expect, actual_ids, actual_scores
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
}

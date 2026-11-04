//! Golden vector tests for the citation lexer (`tests/golden/citations/*.json`).

mod golden_citations {
    use std::collections::BTreeSet;

    use fieldbook_core::check::Code;
    use fieldbook_core::cite::{KeyKind, Mode, lex};
    use fieldbook_core::schema::locator::LocatorKind;
    use serde::Deserialize;

    const SEED_CITATIONS_JSON: &str =
        include_str!("../../../tests/golden/citations/seed-citations.json");
    const CITATIONS_STRICT_JSON: &str =
        include_str!("../../../tests/golden/citations/citations-strict.json");

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
        mode: String,
        input: String,
        expect: GoldenExpect,
        #[serde(default)]
        note: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GoldenExpect {
        ok: bool,
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        canonical: Option<String>,
        #[serde(default)]
        cites: Option<Vec<ExpectedCite>>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ExpectedCite {
        key: String,
        key_kind: KeyKind,
        locator: Option<ExpectedLocator>,
        #[serde(default)]
        non_canonical: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ExpectedLocator {
        kind: LocatorKind,
        text: String,
    }

    fn parse_golden_file(json: &str) -> Result<GoldenFile, String> {
        let file: GoldenFile =
            serde_json::from_str(json).map_err(|e| format!("JSON parse error: {e}"))?;
        if file.kind != "citation" {
            return Err(format!(
                "unknown golden kind {:?} (expected \"citation\")",
                file.kind
            ));
        }
        if file.version != 1 {
            return Err(format!(
                "unsupported golden version {} (expected 1)",
                file.version
            ));
        }
        Ok(file)
    }

    fn run_case(case: &GoldenCase) -> Result<(), String> {
        let _ = &case.note;
        let mode = match case.mode.as_str() {
            "strict" => Mode::Strict,
            "tolerant" => Mode::Tolerant,
            other => return Err(format!("{}: unknown mode {other:?}", case.id)),
        };

        let outcome = lex(&case.input, mode);

        match mode {
            Mode::Strict => {
                let actual_ok = outcome.findings.is_empty() && !outcome.citations.is_empty();
                if actual_ok != case.expect.ok {
                    return Err(format!(
                        "{}: expected ok={}, got citations={:?}, findings={:?}",
                        case.id, case.expect.ok, outcome.citations, outcome.findings
                    ));
                }
                if case.expect.ok {
                    let Some(expected_cites) = &case.expect.cites else {
                        return Err(format!("{}: missing expected `cites` array", case.id));
                    };
                    compare_cites(&case.id, &outcome.citations, expected_cites, false)?;
                } else {
                    let Some(expected_code_str) = case.expect.code.as_deref() else {
                        return Err(format!("{}: missing expected `code` on ok=false", case.id));
                    };
                    let expected_code = Code::parse(expected_code_str).map_err(|e| {
                        format!(
                            "{}: invalid expected code {expected_code_str:?}: {e}",
                            case.id
                        )
                    })?;
                    if !outcome.findings.iter().any(|f| f.code == expected_code) {
                        return Err(format!(
                            "{}: expected finding code {expected_code}, got {:?}",
                            case.id, outcome.findings
                        ));
                    }
                }
            }
            Mode::Tolerant => {
                let actual_ok = outcome.findings.is_empty() && !outcome.citations.is_empty();
                if actual_ok != case.expect.ok {
                    return Err(format!(
                        "{}: expected ok={}, got citations={:?}, findings={:?}",
                        case.id, case.expect.ok, outcome.citations, outcome.findings
                    ));
                }
                if let Some(expected_cites) = &case.expect.cites {
                    compare_cites(&case.id, &outcome.citations, expected_cites, true)?;
                }
                if let Some(expected_canonical) = case.expect.canonical.as_deref() {
                    let actual_canonical = outcome.canonical_group_string();
                    if actual_canonical.as_deref() != Some(expected_canonical) {
                        return Err(format!(
                            "{}: canonical mismatch: expected {:?}, got {:?}",
                            case.id, expected_canonical, actual_canonical
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn compare_cites(
        case_id: &str,
        actual: &[fieldbook_core::cite::Citation],
        expected: &[ExpectedCite],
        check_non_canonical: bool,
    ) -> Result<(), String> {
        if actual.len() != expected.len() {
            return Err(format!(
                "{case_id}: expected {} cites, got {}: {:?}",
                expected.len(),
                actual.len(),
                actual
            ));
        }
        for (idx, (act, exp)) in actual.iter().zip(expected.iter()).enumerate() {
            if act.key != exp.key {
                return Err(format!(
                    "{case_id}[{idx}]: key mismatch: expected {:?}, got {:?}",
                    exp.key, act.key
                ));
            }
            if act.key_kind != exp.key_kind {
                return Err(format!(
                    "{case_id}[{idx}]: key_kind mismatch: expected {:?}, got {:?}",
                    exp.key_kind, act.key_kind
                ));
            }
            match (&act.locator, &exp.locator) {
                (None, None) => {}
                (Some(act_loc), Some(exp_loc)) => {
                    if act_loc.kind != exp_loc.kind || act_loc.text != exp_loc.text {
                        return Err(format!(
                            "{case_id}[{idx}]: locator mismatch: expected ({:?}, {:?}), got ({:?}, {:?})",
                            exp_loc.kind, exp_loc.text, act_loc.kind, act_loc.text
                        ));
                    }
                }
                (act_opt, exp_opt) => {
                    return Err(format!(
                        "{case_id}[{idx}]: locator presence mismatch: expected {exp_opt:?}, got {act_opt:?}"
                    ));
                }
            }
            if check_non_canonical
                && let Some(exp_nc) = exp.non_canonical
                && act.non_canonical != exp_nc
            {
                return Err(format!(
                    "{case_id}[{idx}]: non_canonical mismatch: expected {exp_nc}, got {}",
                    act.non_canonical
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn seed_cases_pass() {
        let file = parse_golden_file(SEED_CITATIONS_JSON).expect("valid seed-citations.json");
        assert_eq!(
            file.cases.len(),
            30,
            "seed-citations.json must contain 30 cases"
        );
        let mut seen = BTreeSet::new();
        let mut failures = Vec::new();
        for case in &file.cases {
            if !seen.insert(case.id.clone()) {
                failures.push(format!("duplicate case id: {}", case.id));
            }
            if let Err(err) = run_case(case) {
                failures.push(err);
            }
        }
        assert!(
            failures.is_empty(),
            "seed_cases_pass failures:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn other_cases_pass() {
        let seed_file = parse_golden_file(SEED_CITATIONS_JSON).expect("valid seed-citations.json");
        let strict_file =
            parse_golden_file(CITATIONS_STRICT_JSON).expect("valid citations-strict.json");

        let mut seen: BTreeSet<String> = seed_file.cases.into_iter().map(|c| c.id).collect();
        let mut failures = Vec::new();
        for case in &strict_file.cases {
            if !seen.insert(case.id.clone()) {
                failures.push(format!(
                    "duplicate case id across citation goldens: {}",
                    case.id
                ));
            }
            if let Err(err) = run_case(case) {
                failures.push(err);
            }
        }
        assert!(
            failures.is_empty(),
            "other_cases_pass failures:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn loader_rejects_unknown_fields() {
        let unknown_top = r#"{"kind":"citation","version":1,"extra":true,"cases":[]}"#;
        assert!(parse_golden_file(unknown_top).is_err());

        let unknown_kind = r#"{"kind":"other","version":1,"cases":[]}"#;
        assert!(parse_golden_file(unknown_kind).is_err());

        let unknown_version = r#"{"kind":"citation","version":2,"cases":[]}"#;
        assert!(parse_golden_file(unknown_version).is_err());

        let unknown_case_field = r#"{
            "kind": "citation",
            "version": 1,
            "cases": [
                {"id": "cit-999", "mode": "strict", "input": "[]", "bogus": 1, "expect": {"ok": false, "code": "FB0300"}}
            ]
        }"#;
        assert!(parse_golden_file(unknown_case_field).is_err());
    }
}

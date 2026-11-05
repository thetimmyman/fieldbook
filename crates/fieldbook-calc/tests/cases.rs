//! Integration tests for calculation cases, `CALC_FNS`, `Run::canonical()`, and parser safety.

#![expect(
    clippy::disallowed_methods,
    reason = "integration tests enumerate `tests/cases/` and `tests/cases-bad/` on disk"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;

mod cases {
    use std::collections::BTreeSet;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::path::Path;

    use fieldbook_calc::fns::has_fn_id_shape;
    use fieldbook_calc::{
        CALC_FNS, CalcError, Case, Currency, Dec, Money, Reason, ReplayCase, RunInput, RunOutput,
        SumMismatch, Unit, Value, broken_fixture_rules, check_children_sum, compare, evaluate,
        evaluate_replay, verify_case,
    };

    use super::rng::Rng;

    const CALC_01_TOML: &str = include_str!("cases/calc-01.toml");
    const CALC_02_TOML: &str = include_str!("cases/calc-02.toml");
    const CALC_03_TOML: &str = include_str!("cases/calc-03.toml");
    const CALC_04_TOML: &str = include_str!("cases/calc-04.toml");
    const CALC_90_TOML: &str = include_str!("cases/calc-90.toml");
    const GOLDEN_RUN_CALC_01: &str = include_str!("golden/run-calc-01.txt");

    fn parse_case(raw: &str) -> Case {
        toml::from_str(raw).unwrap_or_else(|e| panic!("valid case TOML: {e}"))
    }

    fn dec(raw: &str) -> Dec {
        Dec::parse(raw).unwrap_or_else(|e| panic!("valid decimal `{raw}`: {e:?}"))
    }

    fn assert_case_matches(case: &Case) {
        if let Err(mismatches) = verify_case(case) {
            let formatted: Vec<String> = mismatches.iter().map(ToString::to_string).collect();
            panic!(
                "case {} failed verification:\n{}",
                case.id,
                formatted.join("\n")
            );
        }
    }

    fn output_dec(outputs: &[RunOutput], name: &str) -> Dec {
        let out = outputs
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("missing output `{name}`"));
        match out.value {
            Value::Known(d) => d,
            Value::Unknown(r) => panic!("output `{name}` is unknown ({r})"),
        }
    }

    fn read_toml_files_sorted(dir: &Path) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()))
            .filter_map(|entry| {
                let entry = entry.unwrap_or_else(|e| panic!("dir entry: {e}"));
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let text = std::fs::read_to_string(&path)
                        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
                    Some((name, text))
                } else {
                    None
                }
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    #[test]
    fn calc_01_matches() {
        let case = parse_case(CALC_01_TOML);
        assert_case_matches(&case);
        let outcome = evaluate(&case).unwrap_or_else(|e| panic!("CALC-01 evaluates: {e:?}"));
        assert_eq!(output_dec(&outcome.outputs, "unit").to_string(), "9.00");
        assert_eq!(
            output_dec(&outcome.outputs, "extension").to_string(),
            "1800.00"
        );
        assert_eq!(
            output_dec(&outcome.outputs, "landed").to_string(),
            "2050.00"
        );
        assert_eq!(output_dec(&outcome.outputs, "labour").to_string(), "450.00");
        assert_eq!(
            output_dec(&outcome.outputs, "overhead").to_string(),
            "90.00"
        );
        assert_eq!(output_dec(&outcome.outputs, "cost").to_string(), "2590.00");
        assert_eq!(output_dec(&outcome.outputs, "profit").to_string(), "259.00");
        assert_eq!(output_dec(&outcome.outputs, "price").to_string(), "2849.00");

        // When quantity_unit is a price basis other than `each` (e.g. `2 per_100`),
        // `unit` retains the `quantity_unit` basis (`900.00 per_100`) and `extension` is `1800.00`.
        let mut per_100_qty = case;
        let Some(ref mut inputs) = per_100_qty.inputs else {
            panic!("CALC-01 inputs");
        };
        inputs.quantity = "2".to_string();
        inputs.quantity_unit = Some("per_100".to_string());
        let out_per_100 =
            evaluate(&per_100_qty).unwrap_or_else(|e| panic!("CALC-01 per_100 qty: {e:?}"));
        assert_eq!(
            output_dec(&out_per_100.outputs, "unit").to_string(),
            "900.00"
        );
        let unit_out = out_per_100
            .outputs
            .iter()
            .find(|o| o.name == "unit")
            .unwrap_or_else(|| panic!("unit output"));
        assert_eq!(unit_out.unit, "per_100");
        assert_eq!(
            output_dec(&out_per_100.outputs, "extension").to_string(),
            "1800.00"
        );
    }

    #[test]
    fn calc_02_matches() {
        let case = parse_case(CALC_02_TOML);
        assert_case_matches(&case);
        let outcome = evaluate(&case).unwrap_or_else(|e| panic!("CALC-02 evaluates: {e:?}"));
        assert_eq!(output_dec(&outcome.outputs, "unit").to_string(), "9.50");
        assert_eq!(
            output_dec(&outcome.outputs, "landed").to_string(),
            "2150.00"
        );
        assert_eq!(output_dec(&outcome.outputs, "cost").to_string(), "2690.00");
        assert_eq!(output_dec(&outcome.outputs, "price").to_string(), "2959.00");
    }

    #[test]
    fn calc_03_matches() {
        let case = parse_case(CALC_03_TOML);
        assert_case_matches(&case);
        let outcome = evaluate(&case).unwrap_or_else(|e| panic!("CALC-03 evaluates: {e:?}"));
        assert_eq!(output_dec(&outcome.outputs, "cost").to_string(), "2590.00");
        assert_eq!(output_dec(&outcome.outputs, "profit").to_string(), "287.78");
        assert_eq!(output_dec(&outcome.outputs, "price").to_string(), "2877.78");
    }

    #[test]
    fn calc_04_matches() {
        let case = parse_case(CALC_04_TOML);
        assert_case_matches(&case);
    }

    #[test]
    fn an_exact_sum_is_accepted() {
        let parent = Money::usd(dec("50.00"));
        let children = [
            Money::usd(dec("10.00")),
            Money::usd(dec("15.00")),
            Money::usd(dec("12.50")),
            Money::usd(dec("7.50")),
            Money::usd(dec("5.00")),
        ];
        assert_eq!(check_children_sum(parent, &children), Ok(()));
    }

    #[test]
    fn a_gap_of_one_cent_is_refused() {
        let parent = Money::usd(dec("50.00"));
        let children = [
            Money::usd(dec("10.00")),
            Money::usd(dec("15.00")),
            Money::usd(dec("12.50")),
            Money::usd(dec("7.49")),
            Money::usd(dec("5.00")),
        ];
        assert_eq!(
            check_children_sum(parent, &children),
            Err(SumMismatch {
                parent: Money::usd(dec("50.00")),
                total: Money::usd(dec("49.99")),
                gap: Money::usd(dec("0.01")),
            })
        );
    }

    #[test]
    fn calc_02_leaves_calc_01_unchanged() {
        let case_01 = parse_case(CALC_01_TOML);
        let case_02 = parse_case(CALC_02_TOML);

        let before = evaluate(&case_01).unwrap_or_else(|e| panic!("first CALC-01: {e:?}"));
        let _ = evaluate(&case_02).unwrap_or_else(|e| panic!("CALC-02: {e:?}"));
        let after = evaluate(&case_01).unwrap_or_else(|e| panic!("second CALC-01: {e:?}"));

        assert_eq!(before, after);
        assert_eq!(before.run.canonical(), after.run.canonical());
    }

    #[test]
    fn markup_is_not_margin() {
        let case_01 = parse_case(CALC_01_TOML);
        let case_03 = parse_case(CALC_03_TOML);
        let out_01 = evaluate(&case_01).unwrap_or_else(|e| panic!("CALC-01: {e:?}"));
        let out_03 = evaluate(&case_03).unwrap_or_else(|e| panic!("CALC-03: {e:?}"));

        assert_eq!(
            output_dec(&out_01.outputs, "cost"),
            output_dec(&out_03.outputs, "cost")
        );
        assert_ne!(
            output_dec(&out_01.outputs, "profit"),
            output_dec(&out_03.outputs, "profit")
        );
        assert_ne!(
            output_dec(&out_01.outputs, "price"),
            output_dec(&out_03.outputs, "price")
        );
    }

    #[test]
    fn setup_is_added_once() {
        let mut case = parse_case(CALC_01_TOML);
        let out_200 = evaluate(&case).unwrap_or_else(|e| panic!("quantity 200: {e:?}"));
        let ext_200 = output_dec(&out_200.outputs, "extension");
        let landed_200 = output_dec(&out_200.outputs, "landed");
        assert_eq!(ext_200, dec("1800.00"));
        assert_eq!(landed_200, dec("2050.00"));

        let Some(ref mut inputs) = case.inputs else {
            panic!("CALC-01 must have inputs");
        };
        inputs.quantity = "400".to_string();
        let out_400 = evaluate(&case).unwrap_or_else(|e| panic!("quantity 400: {e:?}"));
        let ext_400 = output_dec(&out_400.outputs, "extension");
        let landed_400 = output_dec(&out_400.outputs, "landed");
        assert_eq!(ext_400, dec("3600.00"));
        assert_eq!(landed_400, dec("3850.00"));
    }

    #[test]
    fn a_mismatch_is_fb0503() {
        let mut case = parse_case(CALC_01_TOML);
        let Some(ref mut outputs) = case.outputs else {
            panic!("CALC-01 must have outputs");
        };
        outputs.landed = Some("2050.01".to_string());
        let outcome = evaluate(&case).unwrap_or_else(|e| panic!("CALC-01 evaluates: {e:?}"));
        let mismatches = compare(&case, &outcome);
        assert_eq!(mismatches.len(), 1);
        let mm = &mismatches[0];
        assert_eq!(mm.code(), "FB0503");
        let text = mm.to_string();
        assert!(text.contains("FB0503"), "missing FB0503 in `{text}`");
        assert!(text.contains("CALC-01"), "missing CALC-01 in `{text}`");
        assert!(text.contains("landed"), "missing output name in `{text}`");
        assert!(text.contains("2050.01"), "missing expected in `{text}`");
        assert!(text.contains("2050.00"), "missing found in `{text}`");
    }

    #[test]
    fn every_case_file_matches() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
        let files = read_toml_files_sorted(&dir);
        assert!(!files.is_empty(), "tests/cases/*.toml must not be empty");
        for (name, text) in &files {
            let case: Case =
                toml::from_str(text).unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));
            assert_case_matches(&case);
        }
    }

    #[test]
    fn every_case_obeys_the_fixture_rules() {
        let cases_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
        let files = read_toml_files_sorted(&cases_dir);
        let mut seen_ids = BTreeSet::new();
        for (name, text) in &files {
            let case: Case =
                toml::from_str(text).unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));
            let broken = broken_fixture_rules(name, &case, true);
            assert!(
                broken.is_empty(),
                "case file `{name}` broke fixture rule(s): {}",
                broken.join(", ")
            );
            assert!(
                seen_ids.insert(case.id.clone()),
                "case file `{name}` has duplicate case id `{}`",
                case.id
            );
        }

        let bad_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases-bad");
        let bad_files = read_toml_files_sorted(&bad_dir);
        assert!(
            !bad_files.is_empty(),
            "tests/cases-bad/*.toml must not be empty"
        );
        let mut covered_rules = BTreeSet::new();
        for (name, text) in &bad_files {
            let case: Case =
                toml::from_str(text).unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));
            let broken = broken_fixture_rules(name, &case, false);
            assert_eq!(
                broken.len(),
                1,
                "bad case file `{name}` must break exactly one rule, found: {broken:?}"
            );
            let rule = broken[0];
            covered_rules.insert(rule);
            let refusal_msg = format!("{name}: broken fixture rule `{rule}`");
            assert!(refusal_msg.contains(name));
            assert!(refusal_msg.contains(rule));
        }
        for required_rule in ["synthetic", "as_of", "fn", "overhead_base", "stages", "id"] {
            assert!(
                covered_rules.contains(required_rule),
                "tests/cases-bad/ has no file covering fixture rule `{required_rule}`"
            );
        }
    }

    #[test]
    fn calc_fns_have_cases() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
        let files = read_toml_files_sorted(&dir);
        let parsed: Vec<Case> = files
            .iter()
            .map(|(name, text)| {
                toml::from_str(text).unwrap_or_else(|e| panic!("failed to parse {name}: {e}"))
            })
            .collect();

        for row in CALC_FNS {
            let has_positive = parsed.iter().any(|c| {
                if c.func != row.id {
                    return false;
                }
                if c.refuse.is_some() {
                    return false;
                }
                if !c.budget.is_empty() {
                    c.budget.iter().any(|b| b.expect == "accepted") && verify_case(c).is_ok()
                } else {
                    verify_case(c).is_ok()
                }
            });
            let has_negative = parsed.iter().any(|c| {
                if c.func != row.id {
                    return false;
                }
                if c.refuse.is_some() {
                    verify_case(c).is_ok()
                } else if !c.budget.is_empty() {
                    c.budget.iter().any(|b| b.expect == "refused") && verify_case(c).is_ok()
                } else {
                    false
                }
            });
            assert!(
                has_positive,
                "CALC_FNS row `{}` has no passing positive case in tests/cases/",
                row.id
            );
            assert!(
                has_negative,
                "CALC_FNS row `{}` has no passing negative case in tests/cases/",
                row.id
            );
        }
    }

    #[test]
    fn an_unknown_function_is_fb0504() {
        let mut case = parse_case(CALC_01_TOML);
        case.func = "build_up@9".to_string();
        let err = evaluate(&case).expect_err("unknown function must fail");
        assert_eq!(err, CalcError::UnknownFn("build_up@9".to_string()));
        assert_eq!(err.code(), "FB0504");
    }

    #[test]
    fn a_tolerance_on_an_exact_function_is_refused() {
        let mut case = parse_case(CALC_01_TOML);
        case.tolerance = Some("0.01".to_string());
        let err = evaluate(&case).expect_err("tolerance on exact function must fail");
        assert_eq!(err, CalcError::Grammar);
        assert_eq!(err.code(), "FB0500");
    }

    #[test]
    fn a_refusal_case_matches_its_code() {
        let case = parse_case(CALC_90_TOML);
        let err = evaluate(&case).expect_err("CALC-90 must refuse");
        assert_eq!(err.code(), "FB0501");
        assert_case_matches(&case);
    }

    #[test]
    fn a_refusal_with_the_wrong_code_is_fb0503() {
        let mut case = parse_case(CALC_90_TOML);
        case.refuse = Some("FB0500".to_string());
        let mismatches = verify_case(&case).expect_err("wrong refusal code must fail");
        assert_eq!(mismatches.len(), 1);
        let mm = &mismatches[0];
        assert_eq!(mm.code(), "FB0503");
        let msg = mm.to_string();
        assert!(msg.contains("FB0503"), "missing FB0503 in `{msg}`");
        assert!(msg.contains("CALC-90"), "missing CALC-90 in `{msg}`");
    }

    #[test]
    fn canonical_run_is_stable() {
        let case = parse_case(CALC_01_TOML);
        let out1 = evaluate(&case).unwrap_or_else(|e| panic!("first evaluation: {e:?}"));
        let out2 = evaluate(&case).unwrap_or_else(|e| panic!("second evaluation: {e:?}"));
        let text1 = out1.run.canonical();
        let text2 = out2.run.canonical();
        assert_eq!(text1, text2);
        assert_eq!(text1, GOLDEN_RUN_CALC_01);
    }

    #[test]
    fn canonical_run_changes_with_epoch_rounding_or_unit() {
        let case = parse_case(CALC_01_TOML);
        let base_run = evaluate(&case)
            .unwrap_or_else(|e| panic!("CALC-01: {e:?}"))
            .run;
        let base_bytes = base_run.canonical();

        let mut diff_epoch = base_run.clone();
        diff_epoch.calculation_epoch += 1;
        assert_ne!(
            diff_epoch.canonical(),
            base_bytes,
            "epoch must change canonical()"
        );

        let mut diff_policy = base_run.clone();
        diff_policy.rounding.policy = "custom".to_string();
        assert_ne!(
            diff_policy.canonical(),
            base_bytes,
            "rounding policy must change canonical()"
        );

        let mut diff_scale = base_run.clone();
        diff_scale.rounding.scale = 4;
        assert_ne!(
            diff_scale.canonical(),
            base_bytes,
            "rounding scale must change canonical()"
        );

        let mut diff_mode = base_run.clone();
        diff_mode.rounding.mode = "other_mode".to_string();
        assert_ne!(
            diff_mode.canonical(),
            base_bytes,
            "rounding mode must change canonical()"
        );

        let mut diff_in_unit = base_run.clone();
        diff_in_unit.inputs[0].unit = Unit::Per1000.word().to_string();
        assert_ne!(
            diff_in_unit.canonical(),
            base_bytes,
            "input unit must change canonical()"
        );

        let mut diff_out_unit = base_run.clone();
        diff_out_unit.outputs[0].unit = Unit::Per100.word().to_string();
        assert_ne!(
            diff_out_unit.canonical(),
            base_bytes,
            "output unit must change canonical()"
        );

        let mut diff_in_curr = base_run.clone();
        diff_in_curr.inputs[0].currency = None;
        assert_ne!(
            diff_in_curr.canonical(),
            base_bytes,
            "input currency must change canonical()"
        );

        let mut diff_out_curr = base_run;
        diff_out_curr.outputs[0].currency = None;
        assert_ne!(
            diff_out_curr.canonical(),
            base_bytes,
            "output currency must change canonical()"
        );
    }

    #[test]
    fn canonical_fields_are_unambiguous() {
        let case = parse_case(CALC_01_TOML);
        let mut run_a = evaluate(&case)
            .unwrap_or_else(|e| panic!("CALC-01: {e:?}"))
            .run;
        run_a.inputs = vec![RunInput {
            name: "a:b c\nd".to_string(),
            value: Value::Unknown(Reason::ZeroDenominator),
            origin: "quoted:1\n2".to_string(),
            pointer: "SO-0001-v1 rec 1 col a:b".to_string(),
            unit: "each".to_string(),
            currency: None,
        }];
        run_a.outputs = vec![RunOutput {
            name: "out:1\n2".to_string(),
            value: Value::Unknown(Reason::Inexact),
            unit: "each".to_string(),
            currency: None,
        }];

        let mut run_b = run_a.clone();
        run_b.inputs[0].currency = Some(Currency::USD);

        let mut run_c = run_a.clone();
        run_c.outputs[0].currency = Some(Currency::USD);

        let mut run_d = run_a.clone();
        run_d.inputs[0].name = "a:b".to_string();
        run_d.inputs[0].origin = "c\nd quoted:1\n2".to_string();

        assert_ne!(run_a.canonical(), run_b.canonical());
        assert_ne!(run_a.canonical(), run_c.canonical());
        assert_ne!(run_b.canonical(), run_c.canonical());
        assert_ne!(run_a.canonical(), run_d.canonical());
    }

    #[test]
    fn fn_ids_have_the_shape() {
        for row in CALC_FNS {
            assert!(
                has_fn_id_shape(row.id),
                "CALC_FNS id `{}` does not match ^[a-z][a-z0-9_]*@[0-9]+$",
                row.id
            );
        }
    }

    fn check_reader_never_panics<T, F>(reader_name: &str, valid_example: &str, parse: F)
    where
        F: Fn(&str) -> Result<T, String>,
    {
        match catch_unwind(AssertUnwindSafe(|| parse(valid_example))) {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => panic!("reader `{reader_name}` rejected its valid example: {e}"),
            Err(_) => panic!("reader `{reader_name}` panicked on its valid example"),
        }

        let mut rng = Rng::new(0xCA1C_CA5E_2026_0001);
        for i in 0..200 {
            let len = usize::try_from(rng.next_u64() % 256).unwrap_or(0);
            let bytes = rng.bytes(len);
            let s = String::from_utf8_lossy(&bytes);
            let res = catch_unwind(AssertUnwindSafe(|| parse(&s)));
            assert!(
                res.is_ok(),
                "reader `{reader_name}` panicked on seeded input #{i}"
            );
        }

        let valid_bytes = valid_example.as_bytes();
        for idx in 0..valid_bytes.len() {
            let mut flipped = valid_bytes.to_vec();
            flipped[idx] ^= 0x01;
            let s_flip = String::from_utf8_lossy(&flipped);
            let res_flip = catch_unwind(AssertUnwindSafe(|| parse(&s_flip)));
            assert!(
                res_flip.is_ok(),
                "reader `{reader_name}` panicked on byte-flip at index {idx}"
            );

            let mut deleted = Vec::with_capacity(valid_bytes.len() - 1);
            deleted.extend_from_slice(&valid_bytes[..idx]);
            deleted.extend_from_slice(&valid_bytes[idx + 1..]);
            let s_del = String::from_utf8_lossy(&deleted);
            let res_del = catch_unwind(AssertUnwindSafe(|| parse(&s_del)));
            assert!(
                res_del.is_ok(),
                "reader `{reader_name}` panicked on byte-deletion at index {idx}"
            );
        }
    }

    #[test]
    fn case_and_replay_parsers_never_panic() {
        check_reader_never_panics("Case", CALC_01_TOML, |s| {
            let case: Case = toml::from_str(s).map_err(|e| e.to_string())?;
            let _ = evaluate(&case);
            Ok(case)
        });

        let valid_replay = toml::to_string(&ReplayCase {
            replay_schema: 1,
            calculation_epoch: 1,
            case: parse_case(CALC_01_TOML),
        })
        .unwrap_or_else(|e| panic!("serialize ReplayCase: {e}"));
        check_reader_never_panics("ReplayCase", &valid_replay, |s| {
            let replay: ReplayCase = toml::from_str(s).map_err(|e| e.to_string())?;
            let _ = evaluate_replay(&replay);
            Ok(replay)
        });
    }
}

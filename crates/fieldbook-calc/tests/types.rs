//! Tests for `Dec`, `Currency`, `Money`, `Value<T>`, `CalcError` and parser fuzz/mutation safety.

#[path = "../../../tests/support/rng.rs"]
mod rng;

mod types {
    use fieldbook_calc::{
        CalcError, CivilDate, Currency, Dec, EffectivePeriod, Money, PartialDate, Reason, Unit,
        Value,
    };
    use serde::{Deserialize, Serialize};

    #[test]
    fn grammar_accepts() {
        let valid = [
            "0",
            "1",
            "42",
            "1234567890",
            "-1",
            "-42",
            "0.0",
            "0.00",
            "0.125",
            "-0.125",
            "1234.5000",
            "-1234.5000",
            "999999999999999999.99999999",
        ];
        for input in valid {
            let parsed = Dec::parse(input)
                .unwrap_or_else(|err| panic!("expected `{input}` to be accepted, got {err:?}"));
            assert_eq!(
                parsed.to_string(),
                input,
                "canonical form must round-trip `{input}`"
            );
        }
    }

    #[test]
    fn grammar_rejects() {
        let invalid = [
            ("", "empty string"),
            ("-", "bare minus"),
            ("+1", "leading plus sign"),
            ("00", "leading zero on zero"),
            ("01", "leading zero on integer"),
            ("-01", "leading zero on negative integer"),
            ("00.5", "leading zero before decimal point"),
            ("-0", "negative zero"),
            ("-0.00", "negative zero with decimals"),
            ("1.", "trailing dot"),
            (".5", "leading dot without integer digit"),
            ("-.5", "negative leading dot"),
            ("1..0", "double dot"),
            ("1.2.3", "two dots"),
            ("1,000.00", "thousands separator"),
            ("1_000.00", "underscore separator"),
            ("1e5", "lowercase exponent"),
            ("1E5", "uppercase exponent"),
            ("1.5e-2", "decimal with exponent"),
            ("$10.00", "currency symbol"),
            ("10.00 USD", "trailing currency code"),
            (" 10.00", "leading space"),
            ("10.00 ", "trailing space"),
            ("10.0a", "non-digit in fraction"),
            ("NaN", "not a number"),
            ("inf", "infinity"),
        ];
        for (input, label) in invalid {
            let err = Dec::parse(input).expect_err(&format!(
                "expected `{input}` ({label}) to be rejected by Dec::parse"
            ));
            assert_eq!(
                err,
                CalcError::Grammar,
                "expected `{input}` ({label}) to fail with CalcError::Grammar (FB0500)"
            );
            assert_eq!(err.code(), "FB0500");
        }
    }

    // control: DT-04
    #[test]
    fn from_str_exact_keeps_scale() {
        let cases = [
            ("0", 0),
            ("0.0", 1),
            ("0.00", 2),
            ("1230.50", 2),
            ("-1234.5000", 4),
            ("1.000000", 6),
        ];
        for (input, expected_scale) in cases {
            let dec = Dec::parse(input).expect("valid decimal");
            assert_eq!(
                dec.scale(),
                expected_scale,
                "scale was not preserved for `{input}`"
            );
            assert_eq!(dec.to_string(), input);
        }
    }

    #[test]
    fn parse_outside_the_range_is_an_error() {
        let too_large = "999999999999999999999999999999";
        let too_small = "-999999999999999999999999999999";
        let too_many_frac_digits = "0.12345678901234567890123456789";

        for input in [too_large, too_small, too_many_frac_digits] {
            let err = Dec::parse(input)
                .expect_err(&format!("expected `{input}` to fail range/scale check"));
            assert_eq!(
                err,
                CalcError::Overflow,
                "expected `{input}` to return CalcError::Overflow (FB0501)"
            );
            assert_eq!(err.code(), "FB0501");
        }
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct SampleEnvelope {
        amount: Dec,
        money: Money,
        value: Value<Dec>,
    }

    #[test]
    fn serde_round_trip() {
        let env = SampleEnvelope {
            amount: Dec::parse("-1234.5000").expect("valid decimal"),
            money: Money::new(
                Dec::parse("42.10").expect("valid decimal"),
                Currency::parse("USD").expect("valid currency"),
            ),
            value: Value::Known(Dec::parse("0.050").expect("valid decimal")),
        };
        let encoded = toml::to_string(&env).expect("serializes to TOML");
        assert!(encoded.contains("amount = \"-1234.5000\""));
        let decoded: SampleEnvelope = toml::from_str(&encoded).expect("deserializes from TOML");
        assert_eq!(decoded, env);
        assert_eq!(decoded.amount.scale(), 4);
        assert_eq!(decoded.amount.to_string(), "-1234.5000");

        let unknown_env = SampleEnvelope {
            amount: Dec::ZERO,
            money: Money::usd(Dec::ONE),
            value: Value::Unknown(Reason::ZeroDenominator),
        };
        let encoded_unknown = toml::to_string(&unknown_env).expect("serializes unknown Value");
        let decoded_unknown: SampleEnvelope =
            toml::from_str(&encoded_unknown).expect("deserializes unknown Value");
        assert_eq!(decoded_unknown, unknown_env);
        assert_eq!(
            decoded_unknown.value.map(|d| d.scale()),
            Value::Unknown(Reason::ZeroDenominator)
        );

        #[derive(Debug, Deserialize)]
        struct OnlyDec {
            #[expect(dead_code, reason = "deserialization target only")]
            amount: Dec,
        }
        assert!(
            toml::from_str::<OnlyDec>("amount = 12.34").is_err(),
            "TOML float must be rejected for Dec"
        );
        assert!(
            toml::from_str::<OnlyDec>("amount = 12").is_err(),
            "TOML integer must be rejected for Dec"
        );
    }

    #[test]
    fn currency_accepts_three_capitals_only() {
        for valid in ["USD", "EUR", "GBP", "JPY", "CAD", "XTS", "AAA", "ZZZ"] {
            let cur = Currency::parse(valid)
                .unwrap_or_else(|err| panic!("expected `{valid}` to parse as Currency: {err:?}"));
            assert_eq!(cur.as_str(), valid);
            assert_eq!(cur.to_string(), valid);
        }

        let invalid = [
            "", "U", "US", "USDD", "usd", "Usd", "USd", "uSD", "US1", "U$D", "US ", " USD", "123",
            "ÜSD",
        ];
        for bad in invalid {
            let err = Currency::parse(bad).expect_err(&format!(
                "expected `{bad}` to be rejected by Currency::parse"
            ));
            assert_eq!(err, CalcError::Grammar, "input `{bad}`");
            assert_eq!(err.code(), "FB0500");
        }
    }

    fn check_parser<T, F>(input: &str, parse: F)
    where
        T: std::fmt::Display + PartialEq + std::fmt::Debug,
        F: Fn(&str) -> Result<T, CalcError>,
    {
        match parse(input) {
            Ok(val) => {
                let printed = val.to_string();
                let reparsed = parse(&printed).unwrap_or_else(|err| {
                    panic!("printed value `{printed}` from `{input}` failed to reparse: {err:?}")
                });
                assert_eq!(reparsed, val);
                assert_eq!(reparsed.to_string(), printed);
            }
            Err(err) => {
                assert!(
                    matches!(
                        err,
                        CalcError::Grammar | CalcError::Overflow | CalcError::Mismatch
                    ),
                    "unexpected error variant: {err:?}"
                );
            }
        }
    }

    fn mutate_and_check<T, F>(seed_example: &str, parse: F)
    where
        T: std::fmt::Display + PartialEq + std::fmt::Debug,
        F: Fn(&str) -> Result<T, CalcError>,
    {
        check_parser(seed_example, &parse);
        let original = seed_example.as_bytes();

        for pos in 0..original.len() {
            for bit in 0..8_u8 {
                let mut mutated = original.to_vec();
                if let Some(byte) = mutated.get_mut(pos) {
                    *byte ^= 1_u8 << bit;
                }
                let text = String::from_utf8_lossy(&mutated);
                check_parser(&text, &parse);
            }

            let mut deleted = Vec::with_capacity(original.len().saturating_sub(1));
            for (idx, &b) in original.iter().enumerate() {
                if idx != pos {
                    deleted.push(b);
                }
            }
            let text = String::from_utf8_lossy(&deleted);
            check_parser(&text, &parse);
        }
    }

    #[test]
    fn random_and_mutated_bytes_never_panic() {
        let mut rng = super::rng::Rng::new(1);
        for _ in 0..200 {
            let len = usize::try_from(rng.next_u64() % 65).unwrap_or(0);
            let raw = rng.bytes(len);
            let text = String::from_utf8_lossy(&raw);

            check_parser(&text, Dec::parse);
            check_parser(&text, CivilDate::parse);
            check_parser(&text, PartialDate::parse);
            check_parser(&text, EffectivePeriod::parse);
            check_parser(&text, Unit::from_word);
            check_parser(&text, Currency::parse);
        }

        mutate_and_check("-1234.5000", Dec::parse);
        mutate_and_check("2024-02-29", CivilDate::parse);
        mutate_and_check("2024-02", PartialDate::parse);
        mutate_and_check("2024-01-01/2024-12-31", EffectivePeriod::parse);
        mutate_and_check("weeks", Unit::from_word);
        mutate_and_check("USD", Currency::parse);
    }
}

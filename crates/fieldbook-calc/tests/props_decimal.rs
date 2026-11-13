//! Property tests for decimal parsing, grammar enforcement, and Display round-tripping.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm, RngSeed};
use rng::Rng;

use fieldbook_calc::{CalcError, Dec};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: None,
        rng_algorithm: RngAlgorithm::default(),
        rng_seed: RngSeed::Fixed(Rng::new(1).next_u64()),
        ..ProptestConfig::default()
    }
}

/// Independent oracle verifying the AD-14 decimal grammar `^-?(0|[1-9][0-9]*)(\.[0-9]+)?$`,
/// treating any representation of negative zero as non-matching.
fn matches_grammar(s: &str) -> bool {
    let (negative, magnitude) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    if magnitude.is_empty() {
        return false;
    }

    let (int_part, frac_part) = match magnitude.split_once('.') {
        Some((int_p, frac_p)) => {
            if frac_p.is_empty() || !frac_p.bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
            (int_p, Some(frac_p))
        }
        None => (magnitude, None),
    };

    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if int_part.len() > 1 && int_part.starts_with('0') {
        return false;
    }

    if negative {
        let int_all_zero = int_part.bytes().all(|b| b == b'0');
        let frac_all_zero = match frac_part {
            Some(frac) => frac.bytes().all(|b| b == b'0'),
            None => true,
        };
        if int_all_zero && frac_all_zero {
            return false;
        }
    }

    true
}

/// Generates decimal strings matching `^-?(0|[1-9][0-9]*)(\.[0-9]+)?$` up to 40 digits,
/// constructed so that negative zero is never generated.
fn grammar_text() -> impl Strategy<Value = String> {
    (1..=40usize)
        .prop_flat_map(|int_len| {
            let first_digit_range = if int_len == 1 {
                b'0'..=b'9'
            } else {
                b'1'..=b'9'
            };
            (
                any::<bool>(),
                first_digit_range,
                prop::collection::vec(b'0'..=b'9', int_len - 1),
                prop::collection::vec(b'0'..=b'9', 0..=(40 - int_len)),
            )
        })
        .prop_map(|(negative, first_digit, rest_digits, frac_digits)| {
            let all_zero = first_digit == b'0'
                && rest_digits.iter().all(|&b| b == b'0')
                && frac_digits.iter().all(|&b| b == b'0');
            let is_neg = negative && !all_zero;

            let mut out = String::with_capacity(42);
            if is_neg {
                out.push('-');
            }
            out.push(char::from(first_digit));
            for b in rest_digits {
                out.push(char::from(b));
            }
            if !frac_digits.is_empty() {
                out.push('.');
                for b in frac_digits {
                    out.push(char::from(b));
                }
            }
            out
        })
}

/// Generates strings guaranteed to violate the decimal grammar.
fn non_grammar_text() -> impl Strategy<Value = String> {
    prop_oneof![
        // A `+` sign prepended to grammar_text()
        grammar_text().prop_map(|s| format!("+{s}")),
        // A leading zero inserted after the optional sign
        grammar_text().prop_map(|s| {
            if let Some(rest) = s.strip_prefix('-') {
                format!("-0{rest}")
            } else {
                format!("0{s}")
            }
        }),
        // Negative zero representations: -0, -0.0, -0.00...
        (0..=30usize).prop_map(|zeros| {
            if zeros == 0 {
                "-0".to_string()
            } else {
                format!("-0.{}", "0".repeat(zeros))
            }
        }),
        // A second decimal point: I.F.G
        ("[0-9]{1,6}", "[0-9]{1,6}", "[0-9]{1,6}").prop_map(|(i, f, g)| format!("{i}.{f}.{g}")),
        // Exponent notation: grammar_text() or plain numbers followed by exponent
        (
            prop_oneof![
                grammar_text(),
                Just("1".to_string()),
                Just("2".to_string()),
                Just("5".to_string()),
            ],
            prop_oneof![Just("e"), Just("E"), Just("e+"), Just("e-")],
            "[0-9]{1,3}",
        )
            .prop_map(|(base, exp, digits)| format!("{base}{exp}{digits}")),
        // Whitespace prepended or appended
        (
            grammar_text(),
            prop_oneof![Just(" "), Just("\t"), Just("\n")],
            any::<bool>(),
        )
            .prop_map(|(s, ws, prepend)| {
                if prepend {
                    format!("{ws}{s}")
                } else {
                    format!("{s}{ws}")
                }
            }),
        // Group separators: commas or underscores
        (
            "[1-9][0-9]{0,2}",
            prop_oneof![Just(','), Just('_')],
            prop::collection::vec("[0-9]{3}", 1..=4),
            prop_oneof![
                Just(String::new()),
                "[0-9]{1,4}".prop_map(|frac| format!(".{frac}")),
            ],
        )
            .prop_map(|(mut leading, sep, groups, fraction)| {
                for group in groups {
                    leading.push(sep);
                    leading.push_str(&group);
                }
                leading.push_str(&fraction);
                leading
            }),
        // Small-alphabet string filtered by the grammar oracle
        "[-+.0-9eE ,_\u{0663}\u{ff11}]{0,10}"
            .prop_filter("oracle rejects grammar", |s| !matches_grammar(s)),
        // Arbitrary string filtered by the grammar oracle
        any::<String>().prop_filter("oracle rejects grammar", |s| !matches_grammar(s)),
    ]
}

proptest! {
    #![proptest_config(config())]

    /// Decimal parsing never panics on arbitrary, grammar, or non-grammar text and matches `FromStr`.
    #[test]
    fn parse_never_panics_on_any_text(
        s in prop_oneof![any::<String>(), grammar_text(), non_grammar_text()],
    ) {
        let direct = Dec::parse(&s);
        let from_str = s.parse::<Dec>();
        prop_assert_eq!(direct, from_str);
    }
}

proptest! {
    #![proptest_config(config())]

    /// Text generated by `grammar_text` never produces `CalcError::Grammar`.
    #[test]
    fn parse_never_reports_grammar_on_grammar_text(s in grammar_text()) {
        let res = Dec::parse(&s);
        prop_assert!(
            matches!(res, Ok(_) | Err(CalcError::Overflow)),
            "expected Ok or Overflow for grammar text `{s}`, got {res:?}"
        );
    }
}

proptest! {
    #![proptest_config(config())]

    /// Parsing valid decimal text and round-tripping through `Display` preserves value, scale, and mantissa.
    #[test]
    fn parse_then_display_then_parse_is_the_same_value(s in grammar_text()) {
        let Ok(d) = Dec::parse(&s) else {
            return Ok(());
        };
        let displayed = d.to_string();
        let reparsed = Dec::parse(&displayed);
        prop_assert_eq!(reparsed.clone(), Ok(d));
        if let Ok(second) = reparsed {
            prop_assert_eq!(second.scale(), d.scale());
            prop_assert_eq!(second.mantissa(), d.mantissa());
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// Non-grammar text is rejected with `CalcError::Grammar`.
    #[test]
    fn parse_rejects_what_the_grammar_rejects(s in non_grammar_text()) {
        prop_assert_eq!(Dec::parse(&s), Err(CalcError::Grammar));
    }
}

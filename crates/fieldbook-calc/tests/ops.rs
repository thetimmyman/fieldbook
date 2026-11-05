//! Tests for `fieldbook-calc` rounding stages and exact checked operations (`calc-ops`).

mod ops {
    use fieldbook_calc::ops::{
        add, add_dec, div, div_at_stage, div_dec, mul, mul_dec, percent_of, sub, sub_dec,
    };
    use fieldbook_calc::rounding::{RoundingPolicy, Stage, round_at_stage, round_money_at_stage};
    use fieldbook_calc::{CalcError, Currency, Dec, Money, Reason, Value};

    fn dec(s: &str) -> Dec {
        Dec::parse(s).unwrap_or_else(|e| panic!("valid Dec `{s}`: {e}"))
    }

    fn usd(s: &str) -> Value<Money> {
        Value::Known(Money::usd(dec(s)))
    }

    fn known_dec(s: &str) -> Value<Dec> {
        Value::Known(dec(s))
    }

    #[test]
    fn overflow_is_an_error() {
        let max_usd = Value::Known(Money::usd(Dec::MAX));
        let min_usd = Value::Known(Money::usd(Dec::MIN));

        assert_eq!(add(max_usd, usd("1")), Err(CalcError::Overflow));
        assert_eq!(sub(min_usd, usd("1")), Err(CalcError::Overflow));
        assert_eq!(mul(max_usd, known_dec("2")), Err(CalcError::Overflow));
        assert_eq!(
            mul(max_usd, Value::Known(Dec::MAX)),
            Err(CalcError::Overflow)
        );
        assert_eq!(div(max_usd, known_dec("0.5")), Err(CalcError::Overflow));
        assert_eq!(
            percent_of(max_usd, known_dec("200")),
            Err(CalcError::Overflow)
        );
        assert_eq!(
            div_at_stage(
                max_usd,
                known_dec("0.5"),
                Stage::Total,
                &RoundingPolicy::default()
            ),
            Err(CalcError::Overflow)
        );
    }

    #[test]
    fn add_and_sub_are_exact() {
        assert_eq!(add(usd("12.34"), usd("0.66")), Ok(usd("13.00")));
        assert_eq!(sub(usd("13.00"), usd("0.66")), Ok(usd("12.34")));
        assert_eq!(sub(usd("10.00"), usd("25.50")), Ok(usd("-15.50")));
        assert_eq!(
            add_dec(known_dec("100.125"), known_dec("-0.125")),
            Ok(known_dec("100.000"))
        );
        assert_eq!(
            sub_dec(known_dec("5.5"), known_dec("5.5")),
            Ok(known_dec("0.0"))
        );
    }

    #[test]
    fn mul_scales_an_amount() {
        assert_eq!(mul(usd("125.00"), known_dec("4")), Ok(usd("500.00")));
        assert_eq!(mul(usd("19.95"), known_dec("3")), Ok(usd("59.85")));
        assert_eq!(mul(usd("-10.50"), known_dec("2.5")), Ok(usd("-26.250")));
    }

    #[test]
    fn div_divides_an_amount() {
        assert_eq!(div(usd("100.00"), known_dec("4")), Ok(usd("25.00")));
        assert_eq!(div(usd("1.00"), known_dec("8")), Ok(usd("0.125")));
        assert_eq!(div(usd("-9.00"), known_dec("2")), Ok(usd("-4.50")));
    }

    #[test]
    fn percent_of_is_amount_times_percent_over_100() {
        assert_eq!(
            percent_of(usd("250.00"), known_dec("12.5")),
            Ok(usd("31.25"))
        );
        assert_eq!(
            percent_of(usd("1000.00"), known_dec("100")),
            Ok(usd("1000.00"))
        );
        // Dividing by 100 happens before the representability/overflow test:
        let max_usd = Value::Known(Money::usd(Dec::MAX));
        assert_eq!(percent_of(max_usd, known_dec("100")), Ok(max_usd));
    }

    #[test]
    fn zero_denominator_is_unknown() {
        assert_eq!(
            div(usd("42.00"), known_dec("0")),
            Ok(Value::Unknown(Reason::ZeroDenominator))
        );
        assert_eq!(
            div(usd("42.00"), known_dec("0.00")),
            Ok(Value::Unknown(Reason::ZeroDenominator))
        );
        assert_eq!(
            div_at_stage(
                usd("42.00"),
                known_dec("0"),
                Stage::UnitPrice,
                &RoundingPolicy::default()
            ),
            Ok(Value::Unknown(Reason::ZeroDenominator))
        );
    }

    #[test]
    fn unknown_propagates() {
        let unk_money: Value<Money> = Value::Unknown(Reason::ZeroDenominator);
        let unk_inexact_money: Value<Money> = Value::Unknown(Reason::Inexact);
        let unk_dec: Value<Dec> = Value::Unknown(Reason::Inexact);

        assert_eq!(add(unk_money, usd("1.00")), Ok(unk_money));
        assert_eq!(add(usd("1.00"), unk_inexact_money), Ok(unk_inexact_money));
        assert_eq!(sub(unk_money, usd("1.00")), Ok(unk_money));
        assert_eq!(mul(unk_money, known_dec("2")), Ok(unk_money));
        assert_eq!(mul(usd("1.00"), unk_dec), Ok(unk_inexact_money));
        assert_eq!(div(unk_money, known_dec("2")), Ok(unk_money));
        assert_eq!(div(usd("1.00"), unk_dec), Ok(unk_inexact_money));
        assert_eq!(percent_of(unk_money, known_dec("10")), Ok(unk_money));
        assert_eq!(percent_of(usd("100.00"), unk_dec), Ok(unk_inexact_money));
        assert_eq!(
            div_at_stage(
                unk_money,
                known_dec("2"),
                Stage::Extension,
                &RoundingPolicy::default()
            ),
            Ok(unk_money)
        );
        assert_eq!(
            div_at_stage(
                usd("100.00"),
                unk_dec,
                Stage::Extension,
                &RoundingPolicy::default()
            ),
            Ok(unk_inexact_money)
        );
    }

    #[test]
    fn decimal_ops_are_checked() {
        // Known results
        assert_eq!(
            mul_dec(known_dec("2.5"), known_dec("4")),
            Ok(known_dec("10.0"))
        );
        assert_eq!(
            div_dec(known_dec("1"), known_dec("2")),
            Ok(known_dec("0.5"))
        );

        // Unknown propagation
        let unk = Value::Unknown(Reason::Inexact);
        assert_eq!(mul_dec(unk, known_dec("2")), Ok(unk));
        assert_eq!(mul_dec(known_dec("2"), unk), Ok(unk));
        assert_eq!(div_dec(unk, known_dec("2")), Ok(unk));
        assert_eq!(div_dec(known_dec("2"), unk), Ok(unk));

        // Zero denominator for div_dec
        assert_eq!(
            div_dec(known_dec("10"), known_dec("0")),
            Ok(Value::Unknown(Reason::ZeroDenominator))
        );

        // Overflow for both decimal operations
        assert_eq!(
            mul_dec(Value::Known(Dec::MAX), known_dec("2")),
            Err(CalcError::Overflow)
        );
        assert_eq!(
            div_dec(Value::Known(Dec::MAX), known_dec("0.5")),
            Err(CalcError::Overflow)
        );
    }

    #[test]
    fn mixed_currency_is_an_error() {
        let eur = Value::Known(Money::new(dec("10.00"), Currency::parse("EUR").unwrap()));
        let err_add = add(usd("10.00"), eur).unwrap_err();
        assert_eq!(err_add, CalcError::Mismatch);
        assert_eq!(err_add.code(), "FB0502");

        let err_sub = sub(usd("10.00"), eur).unwrap_err();
        assert_eq!(err_sub, CalcError::Mismatch);
        assert_eq!(err_sub.code(), "FB0502");
    }

    #[test]
    fn midpoint_rounds_away_from_zero() {
        let policy = RoundingPolicy::default();
        for &stage in Stage::ALL {
            assert_eq!(
                round_at_stage(known_dec("2.345"), stage, &policy),
                Ok(known_dec("2.35"))
            );
            assert_eq!(
                round_at_stage(known_dec("-2.345"), stage, &policy),
                Ok(known_dec("-2.35"))
            );
            assert_eq!(
                round_at_stage(known_dec("2.344"), stage, &policy),
                Ok(known_dec("2.34"))
            );
            assert_eq!(
                round_at_stage(known_dec("-2.344"), stage, &policy),
                Ok(known_dec("-2.34"))
            );
            assert_eq!(
                round_money_at_stage(usd("2.345"), stage, &policy),
                Ok(usd("2.35"))
            );
        }
    }

    #[test]
    fn stage_words_round_trip() {
        assert_eq!(Stage::from_word("unit_price"), Ok(Stage::UnitPrice));
        assert_eq!(Stage::from_word("extension"), Ok(Stage::Extension));
        assert_eq!(Stage::from_word("total"), Ok(Stage::Total));

        for &stage in Stage::ALL {
            assert_eq!(Stage::from_word(stage.word()), Ok(stage));
        }

        let err = Stage::from_word("subtotal").unwrap_err();
        assert_eq!(err, CalcError::Grammar);
        assert_eq!(err.code(), "FB0500");
    }

    #[test]
    fn ordinary_ops_refuse_precision_loss() {
        let ulp_28 = known_dec("0.0000000000000000000000000001");
        let ulp_money = usd("0.0000000000000000000000000001");

        // 10^-28 * 0.1 = 10^-29 -> Unknown(Inexact)
        assert_eq!(
            mul_dec(ulp_28, known_dec("0.1")),
            Ok(Value::Unknown(Reason::Inexact))
        );
        assert_eq!(
            mul(ulp_money, known_dec("0.1")),
            Ok(Value::Unknown(Reason::Inexact))
        );
        assert_eq!(
            percent_of(ulp_money, known_dec("10")),
            Ok(Value::Unknown(Reason::Inexact))
        );

        // 10 / 7 -> Unknown(Inexact)
        assert_eq!(
            div_dec(known_dec("10"), known_dec("7")),
            Ok(Value::Unknown(Reason::Inexact))
        );
        assert_eq!(
            div(usd("10"), known_dec("7")),
            Ok(Value::Unknown(Reason::Inexact))
        );

        // 1 / 2 -> Known(0.5)
        assert_eq!(
            div_dec(known_dec("1"), known_dec("2")),
            Ok(known_dec("0.5"))
        );
        assert_eq!(div(usd("1"), known_dec("2")), Ok(usd("0.5")));

        // Exact add/sub results and high-scale add/sub precision loss when magnitude + 28th digit exceeds 96 bits
        assert_eq!(add(usd("0.125"), usd("0.375")), Ok(usd("0.500")));
        assert_eq!(sub(usd("1.000"), usd("0.125")), Ok(usd("0.875")));
        assert_eq!(
            add_dec(known_dec("1000"), ulp_28),
            Ok(Value::Unknown(Reason::Inexact))
        );
    }

    #[test]
    fn stage_quotient_rounds_once() {
        let policy = RoundingPolicy::default();
        let divisor = known_dec("20000000000000000000000000000");

        // Exact quotient is 1.00499999999999999999999999995, strictly below 1.005.
        // Pre-rounding to a 28-dp Decimal would yield 1.005 and then double-round to 1.01.
        let pos_just_below = usd("20099999999999999999999999999");
        assert_eq!(
            div_at_stage(pos_just_below, divisor, Stage::Total, &policy),
            Ok(usd("1.00"))
        );

        let neg_just_below = usd("-20099999999999999999999999999");
        assert_eq!(
            div_at_stage(neg_just_below, divisor, Stage::Total, &policy),
            Ok(usd("-1.00"))
        );

        // Exact midpoint 1.005 rounds away from zero to 1.01 / -1.01.
        let pos_midpoint = usd("20100000000000000000000000000");
        assert_eq!(
            div_at_stage(pos_midpoint, divisor, Stage::Total, &policy),
            Ok(usd("1.01"))
        );

        let neg_midpoint = usd("-20100000000000000000000000000");
        assert_eq!(
            div_at_stage(neg_midpoint, divisor, Stage::Total, &policy),
            Ok(usd("-1.01"))
        );

        // Trailing zeros at the policy scale reduce so representable values fit 96-bit mantissa.
        let max_usd = Value::Known(Money::usd(Dec::MAX));
        assert_eq!(
            div_at_stage(max_usd, known_dec("1"), Stage::Total, &policy),
            Ok(max_usd)
        );
        let p28 = RoundingPolicy::new(28).unwrap();
        assert_eq!(
            div_at_stage(usd("100"), known_dec("2"), Stage::Total, &p28),
            Ok(usd("50"))
        );
    }

    #[test]
    fn default_policy_pins_stage_scales() {
        let policy = RoundingPolicy::default();
        assert_eq!(policy.scale(), 2);

        for stage in [Stage::UnitPrice, Stage::Extension, Stage::Total] {
            assert_eq!(
                round_at_stage(known_dec("1.235"), stage, &policy),
                Ok(known_dec("1.24"))
            );
            assert_eq!(
                round_at_stage(known_dec("-1.235"), stage, &policy),
                Ok(known_dec("-1.24"))
            );
            assert_eq!(
                round_at_stage(Value::Unknown(Reason::Inexact), stage, &policy),
                Ok(Value::Unknown(Reason::Inexact))
            );
        }

        // Valid boundary scales 0 and 28
        let p0 = RoundingPolicy::new(0).expect("scale 0 is valid");
        assert_eq!(p0.scale(), 0);
        assert_eq!(
            round_at_stage(known_dec("1.5"), Stage::Total, &p0),
            Ok(known_dec("2"))
        );

        let p28 = RoundingPolicy::new(28).expect("scale 28 is valid");
        assert_eq!(p28.scale(), 28);
        let d28 = known_dec("0.0000000000000000000000000001");
        assert_eq!(round_at_stage(d28, Stage::Total, &p28), Ok(d28));

        // Invalid scales 29 and u32::MAX
        let err29 = RoundingPolicy::new(29).unwrap_err();
        assert_eq!(err29, CalcError::InvalidScale);
        assert_eq!(err29.code(), "FB0500");

        let err_max = RoundingPolicy::new(u32::MAX).unwrap_err();
        assert_eq!(err_max, CalcError::InvalidScale);
        assert_eq!(err_max.code(), "FB0500");
    }
}

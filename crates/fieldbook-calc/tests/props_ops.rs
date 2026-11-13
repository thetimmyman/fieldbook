//! Property tests for exact decimal and money arithmetic, identities, rounding bounds, and extremes.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm, RngSeed};
use rng::Rng;
use rust_decimal::Decimal;

use fieldbook_calc::ops::{
    add, add_dec, div, div_at_stage, div_dec, mul, mul_dec, percent_of, sub, sub_dec,
};
use fieldbook_calc::rounding::{RoundingPolicy, Stage};
use fieldbook_calc::{CalcError, Currency, Dec, Money, Value};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: None,
        rng_algorithm: RngAlgorithm::default(),
        rng_seed: RngSeed::Fixed(Rng::new(1).next_u64()),
        ..ProptestConfig::default()
    }
}

const MAX_MANTISSA: i128 = 79_228_162_514_264_337_593_543_950_335;

fn dec_value() -> impl Strategy<Value = Dec> {
    let mantissa = prop_oneof![
        6 => -1_000_000i128..=1_000_000,
        2 => i128::from(i64::MIN)..=i128::from(i64::MAX),
        1 => -MAX_MANTISSA..=MAX_MANTISSA,
    ];
    let scale = prop_oneof![
        6 => 0u32..=4,
        2 => 0u32..=28,
    ];
    (mantissa, scale).prop_map(|(mantissa, scale)| {
        Dec::from_decimal(Decimal::from_i128_with_scale(mantissa, scale))
    })
}

fn small_dec() -> impl Strategy<Value = Dec> {
    (-1_000_000i128..=1_000_000, 0u32..=6).prop_map(|(mantissa, scale)| {
        Dec::from_decimal(Decimal::from_i128_with_scale(mantissa, scale))
    })
}

fn currency() -> impl Strategy<Value = Currency> {
    "[A-Z]{3}".prop_map(|s| match Currency::parse(&s) {
        Ok(c) => c,
        Err(_) => Currency::USD,
    })
}

fn extreme_dec() -> impl Strategy<Value = Dec> {
    let mantissa = prop_oneof![
        Just(0i128),
        Just(1i128),
        Just(-1i128),
        Just(MAX_MANTISSA),
        Just(-MAX_MANTISSA),
        Just(MAX_MANTISSA - 1),
        Just(10i128.pow(28)),
    ];
    let scale = prop_oneof![Just(0u32), Just(28u32), Just(1u32), Just(27u32)];
    let constructed =
        (mantissa, scale).prop_map(|(m, s)| Dec::from_decimal(Decimal::from_i128_with_scale(m, s)));
    prop_oneof![constructed, Just(Dec::MAX), Just(Dec::MIN),]
}

fn known(r: Result<Value<Dec>, CalcError>) -> Option<Dec> {
    r.ok().and_then(|v| match v {
        Value::Known(d) => Some(d),
        Value::Unknown(_) => None,
    })
}

fn known_money(r: Result<Value<Money>, CalcError>) -> Option<Money> {
    r.ok().and_then(|v| match v {
        Value::Known(m) => Some(m),
        Value::Unknown(_) => None,
    })
}

fn abs(d: Dec) -> Dec {
    Dec::from_decimal(d.inner().abs())
}

fn ok_or_overflow<T>(r: &Result<T, CalcError>) -> bool {
    matches!(r, Ok(_) | Err(CalcError::Overflow))
}

proptest! {
    #![proptest_config(config())]

    /// Addition of Dec is commutative and associative when all intermediate and final results are Known.
    #[test]
    fn add_is_commutative_and_associative_without_overflow(
        a in dec_value(),
        b in dec_value(),
        c in dec_value(),
    ) {
        if let Some(x) = known(add_dec(Value::Known(a), Value::Known(b))) {
            prop_assert_eq!(add_dec(Value::Known(b), Value::Known(a)), Ok(Value::Known(x)));
        }

        let Some(ab) = known(add_dec(Value::Known(a), Value::Known(b))) else {
            return Ok(());
        };
        let Some(l) = known(add_dec(Value::Known(ab), Value::Known(c))) else {
            return Ok(());
        };
        let Some(bc) = known(add_dec(Value::Known(b), Value::Known(c))) else {
            return Ok(());
        };
        let Some(r) = known(add_dec(Value::Known(a), Value::Known(bc))) else {
            return Ok(());
        };
        prop_assert_eq!(l, r);
    }
}

proptest! {
    #![proptest_config(config())]

    /// Subtraction undoes addition for Dec and Money when all operations are Known.
    #[test]
    fn sub_undoes_add(
        a in dec_value(),
        b in dec_value(),
        cur in currency(),
    ) {
        let s = add_dec(Value::Known(a), Value::Known(b));
        let Some(sd) = known(s) else {
            return Ok(());
        };
        let t = sub_dec(Value::Known(sd), Value::Known(b));
        let Some(td) = known(t) else {
            return Ok(());
        };
        prop_assert_eq!(td, a);

        let ma = Money::new(a, cur);
        let mb = Money::new(b, cur);
        let sm = add(Value::Known(ma), Value::Known(mb));
        let Some(sm_known) = known_money(sm) else {
            return Ok(());
        };
        let tm = sub(Value::Known(sm_known), Value::Known(mb));
        let Some(tm_known) = known_money(tm) else {
            return Ok(());
        };
        prop_assert_eq!(tm_known, Money::new(a, cur));
    }
}

proptest! {
    #![proptest_config(config())]

    /// Multiplication by one in various scales is the identity for Dec and Money when Known.
    #[test]
    fn mul_by_one_is_identity(
        a in dec_value(),
        cur in currency(),
        scale in 0u32..=6,
    ) {
        let one_str = if scale == 0 {
            "1".to_string()
        } else {
            let zeros = usize::try_from(scale).unwrap_or_default();
            format!("1.{}", "0".repeat(zeros))
        };
        let Ok(one) = Dec::parse(&one_str) else {
            return Ok(());
        };

        let money = Money::new(a, cur);
        if let Some(m) = known_money(mul(Value::Known(money), Value::Known(one))) {
            prop_assert_eq!(m, money);
        }

        if let Some(d) = known(mul_dec(Value::Known(a), Value::Known(one))) {
            prop_assert_eq!(d, a);
        }
    }
}

proptest! {
    #![proptest_config(config())]

    // The bound is NOT "one unit at the scale": 1 divided by 7 at scale 2 is 0.14 and 0.14 times 7 is 0.98.
    /// Stage division then multiplication is within half a unit times the divisor (|q*b - a| * 2 * 10^s <= |b|).
    #[test]
    fn div_at_stage_then_mul_is_within_half_a_unit_times_the_divisor(
        a in small_dec(),
        b in small_dec(),
        s in 0u32..=8,
        stage in prop_oneof![Just(Stage::UnitPrice), Just(Stage::Extension), Just(Stage::Total)],
        cur in currency(),
    ) {
        prop_assume!(!b.is_zero());

        let Ok(policy) = RoundingPolicy::new(s) else {
            return Ok(());
        };

        let q = div_at_stage(
            Value::Known(Money::new(a, cur)),
            Value::Known(b),
            stage,
            &policy,
        );
        let Some(qm) = known_money(q) else {
            return Ok(());
        };

        let p = mul(Value::Known(qm), Value::Known(b));
        let Some(pm) = known_money(p) else {
            return Ok(());
        };

        let diff = sub(Value::Known(pm), Value::Known(Money::new(a, cur)));
        let Some(dm) = known_money(diff) else {
            return Ok(());
        };

        let factor_mantissa = 2 * 10i128.pow(s);
        let two_times_ten_pow_s =
            Dec::from_decimal(Decimal::from_i128_with_scale(factor_mantissa, 0));
        let lhs = mul_dec(
            Value::Known(abs(dm.amount)),
            Value::Known(two_times_ten_pow_s),
        );
        let Some(l) = known(lhs) else {
            return Ok(());
        };

        prop_assert!(l <= abs(b));
    }
}

proptest! {
    #![proptest_config(config())]

    /// Taking 100 percent of a Money value is the identity when Known.
    #[test]
    fn percent_of_100_is_identity(
        a in dec_value(),
        cur in currency(),
        scale in 0u32..=4,
    ) {
        let money = Money::new(a, cur);

        if let Some(m1) = known_money(percent_of(
            Value::Known(money),
            Value::Known(Dec::ONE_HUNDRED),
        )) {
            prop_assert_eq!(m1, money);
        }

        let pct_str = if scale == 0 {
            "100".to_string()
        } else {
            let zeros = usize::try_from(scale).unwrap_or_default();
            format!("100.{}", "0".repeat(zeros))
        };
        let Ok(pct_dec) = Dec::parse(&pct_str) else {
            return Ok(());
        };

        if let Some(m2) = known_money(percent_of(Value::Known(money), Value::Known(pct_dec))) {
            prop_assert_eq!(m2, money);
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// Operations on extreme decimal scales and magnitudes never panic and only return Ok or Overflow.
    #[test]
    fn no_operation_panics_on_extreme_scales(
        a in extreme_dec(),
        b in extreme_dec(),
        s in prop_oneof![Just(0u32), Just(28u32), 0u32..=28],
        cur in currency(),
    ) {
        prop_assert!(ok_or_overflow(&add_dec(Value::Known(a), Value::Known(b))));
        prop_assert!(ok_or_overflow(&sub_dec(Value::Known(a), Value::Known(b))));
        prop_assert!(ok_or_overflow(&mul_dec(Value::Known(a), Value::Known(b))));
        prop_assert!(ok_or_overflow(&div_dec(Value::Known(a), Value::Known(b))));

        let ma = Value::Known(Money::new(a, cur));
        let mb = Value::Known(Money::new(b, cur));
        let kb = Value::Known(b);

        prop_assert!(ok_or_overflow(&add(ma, mb)));
        prop_assert!(ok_or_overflow(&sub(ma, mb)));
        prop_assert!(ok_or_overflow(&mul(ma, kb)));
        prop_assert!(ok_or_overflow(&div(ma, kb)));
        prop_assert!(ok_or_overflow(&percent_of(ma, kb)));

        let Ok(policy) = RoundingPolicy::new(s) else {
            return Ok(());
        };

        for stage in [Stage::UnitPrice, Stage::Extension, Stage::Total] {
            prop_assert!(ok_or_overflow(&div_at_stage(ma, kb, stage, &policy)));
        }
    }
}

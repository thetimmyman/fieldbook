# calc-ops

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-calc` gains the Day-1 arithmetic: `RoundingPolicy` and its three stages, exact checked operations on money and decimals, `div_at_stage`, and unit families with exact conversion. No operation rounds except at a named stage, precision loss is `Unknown(Inexact)`, and an overflow is an error.

## Why

- Split from `calc-day1`, which keeps the value types the knowledge-base cards need. This part is needed first by `calc-cases` (CALC-01 to CALC-04 in [FM0](../decisions/0001-foundation.md#calculation-core-fm0)).
- The rounding stages and the exactness rules are in [AD-14](../decisions/0001-foundation.md#ad-14-money-and-the-calculation-core-fm0). The bans on `Decimal::round` and `round_dp` are in the root `clippy.toml`. The controls are in [Determinism and arithmetic](../controls.md#determinism-and-arithmetic) (DT-04).

## Scope

- Files: `crates/fieldbook-calc/src/{rounding.rs,ops.rs}`, `crates/fieldbook-calc/tests/ops.rs`.
- Registration lines: `crates/fieldbook-calc/src/{lib.rs,value.rs,error.rs,unit.rs}` (the module lines, `Reason::Inexact`, `CalcError::InvalidScale`, and the unit family and conversion functions), `crates/fieldbook-calc/tests/units.rs` (the conversion tests).
- Docs to update: none.

## Out of scope

- Case files, the case evaluator, `CALC_FNS` and the `Run` record (`calc-cases`).
- Rates, escalation and the other later calculations (P7 and P8).
- Any change to `Cargo.toml` or `Cargo.lock`.

## Preconditions

- Cards merged: `calc-day1`.

## Steps

1. `rounding.rs`: `RoundingPolicy` with the shared scale, validated constructor and default from the rounding contract, `round_at_stage` with that contract’s exact signature, and `Stage`, a closed enum of the three stages of AD-14: `UnitPrice`, `Extension` and `Total`. A case file names a stage by its snake_case word (`unit_price`, `extension`, `total`); `Stage::word()` and `Stage::from_word()` convert, and an unknown word is `CalcError::Grammar`. The rounding module owns every rounding decision, taking a stage and policy. Decimal inputs use `round_dp_with_strategy`; the exact quotient helper of step 2 applies the same mode directly to its integer quotient and remainder. No intermediate decimal division or multiplication may round before this named stage.
2. `ops.rs`: `add`, `sub`, `mul`, `div` and `percent_of`, each using exact scaled-integer arithmetic and checked bounds, each returning `Result<Value<_>, CalcError>`. Signatures: `add(a: Value<Money>, b: Value<Money>) -> Result<Value<Money>, CalcError>` and `sub` the same; `mul(a: Value<Money>, factor: Value<Dec>) -> Result<Value<Money>, CalcError>`; `div(a: Value<Money>, divisor: Value<Dec>) -> Result<Value<Money>, CalcError>` (a zero divisor is `Unknown(ZeroDenominator)`); `percent_of(amount: Value<Money>, percent: Value<Dec>) -> Result<Value<Money>, CalcError>` (the amount times the percent divided by 100). For unit conversion, also expose `mul_dec(a: Value<Dec>, b: Value<Dec>) -> Result<Value<Dec>, CalcError>` and `div_dec(a: Value<Dec>, b: Value<Dec>) -> Result<Value<Dec>, CalcError>` with the same exactness, unknown propagation and zero-denominator rule. `Known` requires an exactly representable result: use the signed decimal coefficients and scales, retaining exact intermediate coefficients and remainders before testing the final decimal bounds. Precision loss is `Unknown(Inexact)`; a result outside the magnitude range is `CalcError::Overflow`. Do not infer exactness from a rounded decimal result or a reverse multiply. Exact fractions such as `1 / 2` succeed. `percent_of` evaluates its exact product divided by 100 before the representability test. Add `div_at_stage(a: Value<Money>, divisor: Value<Dec>, stage: Stage, policy: &RoundingPolicy) -> Result<Value<Money>, CalcError>`: round the exact rational quotient once at the named stage, using quotient/remainder and the policy scale/mode. It shares the rounding decision of step 1; it must not call ordinary `div` and then round its rounded approximation. No ordinary operation rounds.
3. Add `Reason::Inexact` to `value.rs` and `CalcError::InvalidScale` (code `FB0500`) to `error.rs`.
4. In `unit.rs`, add `family()` and an exact rational conversion factor. Ordinary families and their multipliers to the base: days (`days` 1, `weeks` 7); months (`months` 1, `years` 12); hours (`hours` 1); business days (`business_days` 1); percent (`percent` 1, `basis_points` 1/100); ratio (`ratio` 1); count (`count` 1). Price bases are denominators, not multipliers: `each` 1, `per_100` 100, `per_1000` 1000, `per_dozen` 12. `convert(amount: Dec, from: Unit, to: Unit) -> Result<Value<Dec>, CalcError>` converts only inside one family (across two families it is `CalcError::Mismatch`), using one exact rational calculation: ordinary units use `amount × from_factor / to_factor`; price bases use `amount × to_basis / from_basis`. Cancel integer factors before the representability test; never construct a rounded reciprocal of twelve or use rounded reverse multiplication. A nonrepresentable exact result is `Unknown(Reason::Inexact)`, so no conversion rounds silently.
5. Tests in `tests/ops.rs`, in `mod ops`: `overflow_is_an_error`; `add_and_sub_are_exact`; `mul_scales_an_amount`; `div_divides_an_amount`; `percent_of_is_amount_times_percent_over_100`; `zero_denominator_is_unknown`; `unknown_propagates`; `decimal_ops_are_checked` (known, unknown and overflow for both decimal operations, plus a zero denominator for `div_dec`); `mixed_currency_is_an_error`; `midpoint_rounds_away_from_zero` (synthetic values at each stage); `stage_words_round_trip` (`Stage::from_word` with each word and an unknown word). In `tests/units.rs`: `every_unit_has_one_family`; `weeks_convert_to_days_exactly`; `per_100_converts_to_each`; `basis_points_convert_to_percent`; `conversion_across_families_is_refused` (`days` to `each` is `Mismatch`); `inexact_conversion_is_unknown` (one `days` to `weeks`).
6. Add `ordinary_ops_refuse_precision_loss`: `0.0000000000000000000000000001 × 0.1` is `Unknown(Inexact)`, `10 / 7` is `Unknown(Inexact)`, and `1 / 2` is `Known(0.5)`; cover both decimal and money wrappers and exact add/sub results. Add `stage_quotient_rounds_once`: numerator `20099999999999999999999999999`, divisor `20000000000000000000000000000`, two-decimal Total gives `1.00`, with negative and exact-midpoint counterparts. In units, pin `900.00 per_100 → 9.00 each`, the reverse, `120.00 per_dozen → 10.00 each`, and ten days to weeks as `Unknown(Inexact)`. These tests live in the ops and units modules.
7. In `tests/ops.rs`, `default_policy_pins_stage_scales` calls the actual `RoundingPolicy::default()` and `round_at_stage` at `UnitPrice`, `Extension` and `Total`, requiring `1.235` → `1.24` and `-1.235` → `-1.24` at each. Test valid scales 0 and 28, invalid 29 and `u32::MAX`, and `Unknown` propagation. The one scale is recorded by later Runs; no stage-specific scale is invented.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-calc ops:: units::
cargo clippy -p fieldbook-calc --all-targets --locked -- -D warnings
just check
```

Expected: the tests pass; clippy prints no warning; `just check` exits 0.

## Negative control

Add `let _ = 1.0_f64 * 2.0;` to a function in `ops.rs`. The clippy line must fail with `float_arithmetic`. Remove it. Replace `checked_mul` with `*` in `mul`: `ops::overflow_is_an_error` must fail or panic. Restore the file. Change the factor of `weeks` to 6: `units::weeks_convert_to_days_exactly` must fail. Restore the line.

Make `div_dec` turn a zero divisor into zero: `decimal_ops_are_checked` must fail. Bypass the exact coefficient/remainder test: `ordinary_ops_refuse_precision_loss` and the ten-day conversion must fail. Pre-round the quotient to a Decimal before the named stage: `stage_quotient_rounds_once` must fail. Reverse the price-basis ratio: the price conversion cases must fail. Change the default scale from 2 or its mode: `default_policy_pins_stage_scales` must fail. Accept scale 29: the invalid-scale test must fail. Restore each.

## Rollback

Revert the merge commit. Revert `calc-cases` first if it has merged. The value types of `calc-day1` remain.

## BLOCKED

- `rust_decimal` has no `round_dp_with_strategy` at the locked version: stop. A version change is a lock change and needs the owner.
- AD-14 and the 05 range disagree on which code an error maps to: name both lines and ask the lead.

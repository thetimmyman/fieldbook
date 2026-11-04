# calc-day1

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-calc` holds the Day-1 value types that the Corpus reads: the decimal grammar, `Currency` and `Money`, `Value<T>`, the `Unit` words, and the dates: the civil date, the partial date, the period and the duration. It also creates the one seeded test generator. It uses no float, no clock and no unchecked index, and an overflow is an error, never a wrapped or rounded number. Text that is not valid is a typed error, never a panic.

## Why

- Arithmetic, rounding and unit conversion are split out to `calc-ops`, which comes before `calc-cases`: the knowledge-base cards (`core-ids-and-dates` onward) need only these types, and pricing arithmetic follows the owner's order (knowledge base first).
- The decision, the grammar, `Value<T>`, the rounding stages and the Day-1 scope are in [AD-14](../decisions/0001-foundation.md#ad-14-money-and-the-calculation-core-fm0). The cases this core must answer are the Day-1 calculation cases CALC-01 to CALC-04 in [FM0](../decisions/0001-foundation.md#calculation-core-fm0); `calc-cases` adds them.
- The codes the errors map to are in [05: calculation](../reference/corpus-format.md#05-calculation). The controls are in [Determinism and arithmetic](../controls.md#determinism-and-arithmetic).
- The crate is the leaf of [AD-05](../decisions/0001-foundation.md#ad-05-crate-layout-and-enforcement). The bans on `Decimal::round` and `round_dp` are in the root `clippy.toml`.
- The dates, the period, the duration and `Unit` are in [AD-14](../decisions/0001-foundation.md#ad-14-money-and-the-calculation-core-fm0) (Dates and units), because rules R-02, R-04, R-06, R-15 and R-17 are date and unit rules. This card builds the types only: no function counts business days, adds months or reads a calendar yet (`later-calendar`, `later-calc-functions`).
- The seeded generator is in [Test data](../reference/testing.md#test-data). Every parser has a test that feeds it random and mutated bytes; they share one generator, so a seed means the same everywhere, and `one_rng_exists` fails a second copy. This card is the first in plan order that parses hand-written text, so it creates the generator; the other parser cards include it.

## Scope

- Files: `crates/fieldbook-calc/src/{lib.rs,decimal.rs,money.rs,value.rs,error.rs,unit.rs,date.rs,period.rs}`, `crates/fieldbook-calc/tests/{types.rs,dates.rs,units.rs}`, `tests/support/rng.rs`, `xtask/tests/one_rng.rs`.
- Docs to update: none. The crate adds no command and no code.

## Out of scope

- `RoundingPolicy`, the operations, `div_at_stage`, unit families and conversion (`calc-ops`).
- Case files, the case evaluator, the function table `CALC_FNS`, the `Run` record and FB0500 to FB0504 in `check` (`calc-cases`).
- Any function on dates: adding months or years to a date, counting business days, a calendar. An `EffectivePeriod` and a `Duration` are values here and nothing computes with them beyond the methods of step 8.
- Rates, escalation, sensitivity and the other Day-30 calculations. They are the pricing module, built after the knowledge base: pricing, estimating and the indirect rate book first, then EAC and EVM ([PLAN](../plan/PLAN.md), P7 and P8).
- Any change to `Cargo.toml` or `Cargo.lock`. `rust_decimal` and `serde` are wired by `day1-dependencies-3`.

## Preconditions

- Cards merged: `day1-dependencies-3`.
- This card does not touch `fieldbook-core`, so it may run beside the check cards. It comes before `core-ids-and-dates` in the plan, because that card re-exports the dates and includes the generator that this card creates.

## Steps

1. `decimal.rs`: parse a decimal by the grammar of AD-14 with `Decimal::from_str_exact`, after a hand-written check of the grammar. Implement `Deserialize` and `Serialize` for the crate's own `Dec` type, so no caller reaches `rust_decimal`'s own serde. Text in, text out: the canonical form keeps the scale it was written with. `Dec::parse(&str) -> Result<Dec, CalcError>` follows the decimal grammar: optional minus, digits and an optional dot followed by digits; other forms are `CalcError::Grammar`.
2. `money.rs`: `pub struct Currency([u8; 3])`, with `Currency::parse(&str) -> Result<Currency, CalcError>` (exactly three ASCII capitals, no table lookup; otherwise `CalcError::Grammar`), `as_str(&self) -> &str` and `Display`; `Money { pub amount: Dec, pub currency: Currency }`. Arithmetic on money is `calc-ops`. `Quantity` belongs to the later calculations; this card does not define it.
3. `value.rs`: `Value<T>` is `Known(T)` or `Unknown(Reason)`. Every operation on an `Unknown` returns it unchanged. `Reason` is a closed enum; this card defines `ZeroDenominator`, and `calc-ops` adds `Inexact`.
4. `error.rs`: `CalcError` with `Grammar`, `Overflow` and `Mismatch` (`calc-ops` adds `InvalidScale`). Their `code()` values are respectively `FB0500`, `FB0501` and `FB0502`; `fieldbook-core` maps that text to its `Code`, because this crate cannot see `Code`.
5. In `lib.rs`, keep the deny line (the float and indexing lints and `unsafe`), and delete the test `float_and_indexing_bans_are_declared` and its comment if they are still there: the test searched its own source for text that it contains, so it could not fail. The weekly `lint-controls` job of `scheduled-workflow` is the control for the bans. Update the crate doc comment: the dependencies were wired by `day1-dependencies-3`, and the crate also owns the dates, the period, the duration and `Unit` (AD-14).
6. `unit.rs`: `Unit` is a closed enum of the non-currency words of the `unit` row of [Claim vocabularies](../reference/corpus-format.md#claim-vocabularies): `percent`, `basis_points`, `days`, `business_days`, `weeks`, `months`, `years`, `hours`, `count`, `ratio`, `each`, `per_100`, `per_1000` and `per_dozen`. A currency code is not a `Unit`; `Money` keeps its `currency`. Write `Unit::ALL`, `word()`, `from_word()` (an unknown word is `CalcError::Grammar`) and `is_time()` (true for `days`, `business_days`, `weeks`, `months`, `years` and `hours`). Families and conversion are `calc-ops`. A new unit is added by the recipe "Adding a unit" in the [cards README](README.md#adding-a-unit).
7. `date.rs`: `CivilDate` (`YYYY-MM-DD`, a real calendar with leap years) with `parse`, `Display`, ordering, `add_days(i64)` (checked: a result outside the years 0001 to 9999 is `CalcError::Overflow`), `days_between` (signed) and `from_epoch_seconds(i64)` for `SOURCE_DATE_EPOCH`. `PartialDate` (`YYYY`, `YYYY-MM` or `YYYY-MM-DD`) prints as written and compares as its first day; a year-only date is never turned into a full date by arithmetic (R-06): `to_civil()` is `None` unless it was written in full. Text that is not a date is `CalcError::Grammar`. Nothing here reads a clock. `fieldbook-core` re-exports these types (`core-ids-and-dates`).
8. `period.rs`: `EffectivePeriod { start: CivilDate, end: Option<CivilDate> }`, an effective period (R-15) whose two ends are inclusive and whose open end means "until superseded". `EffectivePeriod::parse` reads `YYYY-MM-DD/YYYY-MM-DD` and `YYYY-MM-DD/`; an end before the start is `CalcError::Grammar`. Methods: `contains(CivilDate) -> bool` and `overlaps(&EffectivePeriod) -> bool`. `Duration` has private `amount: i64` and `unit: Unit` fields, read-only accessors and `Duration::new(amount: i64, unit: Unit) -> Result<Duration, CalcError>`, which accepts only `is_time()` units (any other is `CalcError::Mismatch`). `days() -> Result<Option<i64>, CalcError>` returns `Some` for `days` and `weeks`, `None` for the other time units and `CalcError::Overflow` if checked week-to-day multiplication overflows; it is the form of a publication lag (R-17).
9. Write `tests/support/rng.rs` at the repository root, the one seeded generator of the workspace: SplitMix64 with `Rng::new(seed)`, `next_u64()` and `bytes(n)`. No `rand` crate, no float, no indexing and no `unwrap`, so it passes the workspace lints. A test in the file pins the first three outputs of seed 0 from the published reference implementation: `0xe220a8397b1dcdaf`, `0x6e789e6aa1b965f4` and `0x06c45d188009454f`. A test file includes it with `#[path = "../../../tests/support/rng.rs"] mod rng;` (the depth of the path follows the including file). Declare it at the top level of the test file, outside any inline `mod`, and reach it from an inline module as `super::rng`.
10. Write `xtask/tests/one_rng.rs` with `one_rng_exists`: scan every `.rs` file under `crates/` and `xtask/`, strip underscores, lower-case the text, and fail naming any file that contains the SplitMix64 constant `9e3779b97f4a7c15`. Build the constant from two halves inside the test, so the test file does not contain it whole. The generator at `tests/support/rng.rs` lies outside the two folders; a second copy inside them fails the test.
11. Tests in `tests/types.rs`, in `mod types`: `grammar_accepts` and `grammar_rejects` (tables of strings, each rejection named in the failure); `from_str_exact_keeps_scale`; `parse_outside_the_range_is_an_error`; `serde_round_trip`; `currency_accepts_three_capitals_only` with valid synthetic codes, lowercase, empty and wrong-length inputs; and `random_and_mutated_bytes_never_panic`, which uses the generator of step 9 (declared at the top of `tests/types.rs`, outside `mod types`): 200 byte strings of length 0 to 64 from `Rng::new(1)`, and single-byte flips and deletions of a valid example for each parser: `-1234.5000` for `Dec::parse`, `2024-02-29` for `CivilDate::parse`, `2024-02` for `PartialDate::parse`, `2024-01-01/2024-12-31` for `EffectivePeriod::parse`, `weeks` for `Unit::from_word` and `USD` for `Currency::parse`. Feed the random strings to every parser and each example's mutations to its own parser through `String::from_utf8_lossy`. The result is `Ok` or a typed `CalcError` and never a panic, and an `Ok` value prints and parses back to itself.
12. Tests in `tests/dates.rs`, in `mod dates`, at least 20 cases: valid `2024-02-29` and `2000-02-29`; invalid `2025-02-30`, `2025-13-01`, `0000-01-01`, `2100-02-29`, `2025-1-1` and a date with a leading space; `century_years_follow_the_leap_rule` (1900 is not a leap year, 2000 is); `add_days_crosses_a_month_and_a_year`; `add_days_past_year_9999_overflows`; `days_between_is_signed`; `epoch_seconds_give_the_date` (0 is `1970-01-01`, 31536000 is `1971-01-01`, a negative value works); `partial_date_prints_as_written`; `partial_date_orders_by_its_first_day`; `partial_date_never_becomes_full`; `period_contains_is_inclusive`; `period_with_an_open_end_contains_later_dates`; `period_with_an_end_before_the_start_is_refused`; `periods_overlap`; `duration_refuses_a_non_time_unit`; `duration_days_checks_overflow`. Tests in `tests/units.rs`, in `mod units`: `unit_words_match_the_reference` (read the `unit` row of Claim vocabularies in `docs/reference/corpus-format.md` with `include_str!`, and require that its non-currency words are exactly the words of `Unit::ALL`, and that the time words of the number-unit lint sentence in the notes of the 03 range, `days`, `weeks`, `months`, `years` and `hours`, are exactly the `Unit::ALL` words with `is_time()` true and no underscore; `check-citation-rules` builds its list from `Unit::ALL`, not from a copy).

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-calc types:: dates:: units::
cargo nextest run --locked -p xtask --test one_rng
cargo clippy -p fieldbook-calc --all-targets --locked -- -D warnings
just check
```

Expected: the tests pass; clippy prints no warning; `just check` exits 0.

## Negative control

Add `let _ = 1.0_f64 * 2.0;` to a function in `date.rs`. The clippy line must fail with `float_arithmetic`. Remove it. Then treat 1900 as a leap year in `date.rs`: `dates::century_years_follow_the_leap_rule` must fail. Restore the line. Then write the SplitMix64 constant into a file under `crates/`: `one_rng_exists` must fail and name the file. Remove it.

Accept a two-letter currency: `currency_accepts_three_capitals_only` must fail. Let `Duration::new` accept `each`: `duration_refuses_a_non_time_unit` must fail. Drop `per_dozen` from `Unit::ALL`: `unit_words_match_the_reference` must fail. Restore each.

## Rollback

Revert the merge commit. The crate returns to its stub. Revert `calc-ops` and `calc-cases` first if they have merged, and revert `core-ids-and-dates` and any other card that includes `tests/support/rng.rs` before this one.

## BLOCKED

- AD-14 and the 05 range disagree on which code an error maps to: name both lines and ask the lead.
- `rust_decimal` has no `from_str_exact` at the locked version: stop. A version change is a lock change and needs the owner.
- The three outputs pinned for seed 0 in `rng.rs` differ from what the algorithm gives: recompute them with the published C reference of SplitMix64, write both sets in the pull request description and stop; the lead confirms which is right.

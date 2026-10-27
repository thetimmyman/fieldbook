# core-ids-and-dates

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-core` parses and prints every identifier form of Corpus format v1 without a regex crate and allocates the next id of a series. It also holds the const `FAMILIES`, one row per id family, which the parsers read and which the Identifiers table, the goldens, the lexer (a later card) and the identity exemption list (`check-scans`, the test `scan_lists::tracker_exemption_lists_every_family_prefix`) are tested against. The dates (`CivilDate`, `PartialDate`, `EffectivePeriod` and `Duration`) are those of `fieldbook-calc`, re-exported here, and the volatility intervals and the `review_by` date are computed on them. Text that is not an id is a typed error and never a panic. Nothing here reads a clock.

## Why

- The forms, patterns and allocation rules are in [Identifiers](../reference/corpus-format.md#identifiers); the date forms, partial dates and review intervals are in [Dates](../reference/corpus-format.md#dates) ([AD-10](../decisions/0001-foundation.md#ad-10-identifiers-citations-and-frozen-strings)).
- Every later card compares ids and dates. One tested module owns the ids. The date types are owned by `fieldbook-calc` ([AD-14](../decisions/0001-foundation.md#ad-14-money-and-the-calculation-core-fm0)), because a calculation function takes a date or a period; this card re-exports them and keeps the volatility rule, which is a Corpus rule and not a calculation rule.
- The company forms (six to ten digits, a writer prefix, areas that start with `x`) are decided in [ADR 0004](../decisions/0004-company-boundary.md#10-identity) and built by `r2-company-ids`. This card makes room for them with one `FAMILIES` table, so a new family is a row and not a new type ([Adding an id family](README.md#adding-an-id-family)). The public identifier patterns do not change, so every existing id stays valid. There is no allocator port: a table and a pure function are enough.
- A case id is a prefix from the table of calculation case prefixes in [Identifiers](../reference/corpus-format.md#identifiers), a hyphen and two or three digits (`CALC-01`, `CALC-100`). Ten prefixes exist: `CALC`, `QA` and `FC` are in use, and `EST`, `QTE`, `SCH`, `RTE`, `EAC`, `EVM` and `CTF` are reserved for the modules that come later. All ten are in `FAMILIES`, so that the identity scan does not take a case id for a tracker key (FB0401) and a new case family is a line of data.

## Scope

- Files: `crates/fieldbook-core/src/{lib.rs,ids.rs,ids/families.rs,date.rs}`, `crates/fieldbook-core/tests/{golden_ids.rs,ids_no_panic.rs}` and `tests/golden/ids/`. `date.rs` holds the re-exports and the volatility rule only; the date types are in `crates/fieldbook-calc/src/date.rs` (`calc-day1`).
- Docs to update: none needed; the reference documents already state the rules.

## Out of scope

- The `Code` enum and the diagnostic form (`check-v0`).
- Reading ids from front matter or registers (`core-schema-pages`, `core-schema-registers`).
- Reading the clock. The binary does that and passes the date in.
- The date types, their parsing and arithmetic, the period, the duration, and the seeded generator `tests/support/rng.rs` (`calc-day1`). This card uses them and writes no second copy.
- Company shapes, writer digits, the lexer shapes for them and allocation in a company store (`r2-company-ids`). The `next_` functions here serve the public Corpus, whose four-digit series are frozen.

## Preconditions

- Cards merged: `core-types-and-strings` and `calc-day1`.
- `calc-day1` gives the date types that `date.rs` re-exports and the generator that `ids_no_panic.rs` includes.

## Steps

1. For each row of the Identifiers table that is not an area code, write a type with `parse(&str) -> Result<Self, IdError>`, `as_str` and `Display`: page, source, source version, claim, question, conflict, term, calculation case, ADR, edition and tool release. Also write `AreaCode` and `is_error_code(&str)`. Apply the pattern exactly, including the 64-character limit on page ids and term ids. `IdError` names the form and keeps the input. Also write the const `FAMILIES: [Family; N]` in `ids/families.rs`, one row per family: `name`, `prefixes` (a slice: one prefix for every family except the calculation case, whose row lists the ten case prefixes `CALC`, `QA`, `FC`, `EST`, `QTE`, `SCH`, `RTE`, `EAC`, `EVM` and `CTF`), `public_shape` (the pattern of the Identifiers table), `example`, and the company fields `company_prefix`, `company_shape` and `company_example`, each `Option<&str>` and `None` on this card (`r2-company-ids` sets them). Who holds an id and how it sorts are columns of the Identifiers table, not fields of `Family`. The parsers read their pattern from the row. `CaseId::parse` accepts a listed prefix, a hyphen and two or three digits (`CALC-01`, `CALC-100`) and refuses an unlisted prefix, one digit and four digits; `prefix()` and `number()` return its parts. A reserved prefix parses: whether a case exists is decided by the tests of the module that owns the prefix.
2. Make ids order byte by byte. Add the derived values: the source and number of a `VersionId`, the lowercase area of a `ClaimId` and of a `PageId`, and the number of a question and of a conflict.
3. Add `next_source_id`, `next_claim_id` (per area), `next_question_id` and `next_conflict_id`. Each takes the ids that exist, retired rows included, and returns one more than the highest number, starting at `S-0001`, `CL-AAA-0001`, `Q-001` and `XC-01`. Source and claim sequences exhaust after 9999; questions exhaust after 999. Conflicts follow `^XC-[0-9]{2,}$`: `XC-9999` is followed by `XC-10000`, with no four-digit format cap. Use checked integer increment and return a representation-overflow error only at that implementation limit. The company allocator reads only the rows of one family and one writer, has no such cap and is part of `r2-company-ids`.
4. In `date.rs`, write `pub use fieldbook_calc::{CivilDate, PartialDate, EffectivePeriod, Duration};` and nothing else of the date types. The parsing, the ordering, the day arithmetic (`add_days`, `days_between`, `from_epoch_seconds`), the leap rules and the partial date are written and tested in `calc-day1`; do not write a second copy. The test `date::reexports_are_the_calc_types` binds a `fieldbook_core::CivilDate` to a `fieldbook_calc::CivilDate` variable (it compiles only while the two are one type), parses `2024-02-29`, and checks that `2025-02-30` is refused.
5. In `date.rs`, write `pub enum Volatility { V1, V2, V3 }`, deriving `Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize`, with `ALL`, `as_str` and `parse(&str) -> Option<Volatility>` (the exact words `V1`, `V2`, `V3`), `pub fn interval_days(v: Volatility) -> u32` (90, 180, 365) and `pub fn review_by(as_of: CivilDate, v: Volatility) -> CivilDate`, which adds the interval with `add_days`. The calc addition is checked; past 9999-12-31 the result is 9999-12-31, so `review_by` has no error type and the callers (`review::page_review_by`) stay as written. The tests `date::volatility_serde_round_trip` (all three exact words and an invalid word), `date::volatility_intervals_are_90_180_365`, `date::review_by_adds_the_interval` and `date::review_by_stops_at_the_last_day` pin the three rules.
6. Write table-driven tests with at least 40 cases. Use the examples of the Identifiers table as the valid cases, and add `S-42`, `CL-CM-0011`, `Q-1000`, `XC-1`, a 65-character page id, `CALC-100` (valid), `CALC-1` and `CALC-1000` (invalid) and `ZZ-01` (a prefix that is not listed) as invalid or boundary cases. Name one test `ids::claim_id_rejects_a_two_letter_area` (`CL-CM-0011` is refused). The date cases `2025-02-30`, `2024-02-29`, `2025-13-01` and `0000-01-01` are in the `tests/dates.rs` of `calc-day1`. Add one golden per row of `FAMILIES`, `tests/golden/ids/<family>.json` (the first id, the next, a gap, the family-specific capacity boundary (or checked representation overflow for conflicts), a malformed id), read by `tests/golden_ids.rs`, and `families_have_shapes_goldens_and_docs`: every row has a golden, a row in the Identifiers table of `corpus-format.md` (read with `include_str!`) and an example that its pattern accepts. The case row has one golden, `case.json`, with a case from a prefix in use and one from a reserved prefix. Add `case_prefixes_match_the_reference`: the prefixes of the case row equal the prefixes in the table "Calculation case prefixes" of `corpus-format.md` (read with `include_str!`), and a prefix on only one side fails the test and is named. Both tests are in `ids/families.rs`, so their names start with `ids::families::`. Add `tests/ids_no_panic.rs` with `random_and_mutated_bytes_never_panic`: it includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 64 from `Rng::new(1)`, and takes each family's own valid `FAMILIES.example`, plus `cmp` for `AreaCode::parse` and `FB0100` for `is_error_code`, with every byte once flipped and once deleted. Feed the random strings to every parser and each example's mutations to its own parser through `String::from_utf8_lossy`. The result is an `Ok` that prints back to the input or a typed `IdError`, and never a panic.

7. Add `ids::question_allocator_exhausts_after_999` and `ids::conflict_allocator_continues_after_9999`, exercising allocation from the existing retired-inclusive rows, not parsing alone.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core ids:: date::
cargo nextest run --locked -p fieldbook-core --test golden_ids --test ids_no_panic
just check
```

Expected: both succeed; the first command runs the tests named in the steps (the table-driven tests hold at least 40 cases in all).

## Negative control

In `ids.rs`, let the claim id accept a two-letter area. The test `ids::claim_id_rejects_a_two_letter_area` must fail. Restore the line. In `date.rs`, change the interval of `V2` to 181: `date::volatility_intervals_are_90_180_365` must fail. Restore the line. (The leap-year control moved to `calc-day1`, where `dates::century_years_follow_the_leap_rule` is the test.) In `FAMILIES`, change the prefix of one family: `families_have_shapes_goldens_and_docs` must fail and name that family. Restore the line. Then remove `RTE` from the prefixes of the case row: `case_prefixes_match_the_reference` must fail and name `RTE`. Restore the line. Then make the digit part of `CaseId::parse` call `unwrap()` on a failed parse: `random_and_mutated_bytes_never_panic` must fail. Restore the line.

Allow allocation after Q-999 or reject XC-10000: the corresponding allocator test must fail. Restore each.

## Rollback

Revert the merge commit. Nothing outside `fieldbook-core` uses the module yet.

## BLOCKED

A pattern in the reference disagrees with its own example. Write both lines and stop; the owner settles which is right (format gate).

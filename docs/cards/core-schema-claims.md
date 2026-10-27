# core-schema-claims

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Rows of `claims.csv` parse into a typed `ClaimRow` with its 35 columns. Every closed claim vocabulary is an enum; locator kinds also preserve extension names. A new claim can be built with the entry values the promotion rules fix, and a raise of a status can be told from a change that is not one.

## Why

- The columns are in [claims.csv](../reference/corpus-format.md#claimscsv); the values are in [Claim vocabularies](../reference/corpus-format.md#claim-vocabularies).
- The entry values and the meaning of a raise are in [Promotion rules](../reference/corpus-format.md#promotion-rules). `guard-statuses` and `check-promotion` build on the helper written here.

## Scope

- Files: `crates/fieldbook-core/src/schema/{mod.rs,vocab.rs,claims.rs,locator.rs}`, `crates/fieldbook-core/tests/claims_no_panic.rs`; `crates/fieldbook-core/src/schema/rows.rs`.
- Docs to update: none needed.

## Out of scope

- Rules across columns (`check-register-cells`), supersession and references (`check-register-links`) and the promotion rules (`check-promotion`).
- Reading the decimal in `value`. The cell stays text here; `calc-cases` checks its grammar.
- Writing claims to a file (`new-verbs`).

## Preconditions

- Cards merged: `core-schema-registers`.

## Steps

1. In `vocab.rs`, add the enums `claim_kind`, `verification`, `applicability`, `data_origin`, `drafted_by`, `verified_by_role`, `locator_kind`, `text_status`, `instrument` and `evidence_grade`, each with `ALL`, `as_str`, parsing and `is_overlay()` for the overlay values (`confirmed_applicable`, the locator kind `record_field` and the grades `A` to `D`). `adoption` comes from `core-schema-registers`, with its own overlay values. Add the enums of this step to `schema::vocab::every_enum_matches_the_reference`. Add `schema::vocab::overlay_values_are_marked`: over every enum that has `is_overlay()`, the values with `is_overlay() == true` equal exactly this set: `confirmed_applicable`, `record_field`, the grades `A` to `D`, the `question_status` value `answered`, the `source_kind` values `vendor_quote`, `company_record` and `company_document`, and the `adoption` values `pending_company_approval` and `company_approved`.
2. Add `ClaimUnit`, the type of the `unit` cell: a currency code of three capital letters, or a non-currency unit word, which `ClaimUnit::parse` takes from `fieldbook_calc::Unit::from_word`, so that the words are listed once (`percent`, `basis_points`, `days`, `business_days`, `weeks`, `months`, `years`, `hours`, `count`, `ratio`, `each`, `per_100`, `per_1000` and `per_dozen`). The shape is the check for a currency; the list of ISO 4217 codes is not stored. The words are not written again here: the test `unit_words_match_the_reference` in `calc-day1` compares them with the reference cell. Currency syntax delegates to `fieldbook_calc::Currency::parse`, and `ClaimDraft.volatility` uses `crate::date::Volatility`.
3. Write `ClaimRow` with `from_row` and `to_cells`, as for the other rows. `claim_id` is a `ClaimId`; `area` is an `AreaCode` that equals the id's area in lowercase (that comparison belongs to `check-register-cells`). `as_of: Option<PartialDate>` preserves an empty cell as `None`, not `CellProblem::Empty`; `to_cells` writes it empty. The cross-column pass decides whether that absence is allowed. Limits: `quote` 300 characters, other text 1,000.
4. Add `ClaimRow::entry(draft: ClaimDraft) -> ClaimRow`. `ClaimDraft` holds the cells an author supplies: `claim_id`, `area`, `kind`, `statement`, `as_of: Option<PartialDate>`, `source_id`, `version`, `locator_kind`, `locator` and `quote` (the last five are options, because an assumption has no source), `consequential`, `drafted_by` and `volatility`. `entry` sets the rest to the entry state: `verification = unchecked`, `applicability = pending_company_confirmation`, `adoption = proposed`, `data_origin = unknown`, and `verified_on` and `verified_by_role` empty; every other optional cell is empty. `new-verbs` and the tests of `guard-statuses` build a claim with it.
5. Add `ClaimRow::is_entry_state(&self) -> bool` (`verification`, `applicability`, `adoption` and `data_origin` hold their entry values and `verified_on` and `verified_by_role` are empty) and `is_raise(before: Option<&ClaimRow>, after: &ClaimRow) -> bool`. With `Some(before)`: true when `verification`, `applicability` or `adoption` moves to a value other than its entry value, or when `verified_on` or `verified_by_role` changes; a change back to an entry value is not a raise. With `None` (a new row): true exactly when `!after.is_entry_state()`.
6. Test with a synthetic row of each claim kind, one problem row for each problem kind, the entry constructor (`entry_is_in_entry_state`: the four state cells hold their entry values and the two verified cells are empty), and `is_raise` on at least ten cases: each of the five fields raised, a field set back to its entry value, a change of `statement` only, and two new rows, `is_raise(None, row)`: one in entry state (false) and one with `verification = primary_source_supported` (true). `schema::claims::a_change_of_verified_by_role_is_a_raise` is one of the ten `is_raise` cases, with `a_change_of_verified_on_is_a_raise`, `a_reset_to_an_entry_value_is_not_a_raise` and `a_statement_change_is_not_a_raise`.
7. Add `AppliesTo::parse(&str) -> Result<AppliesTo, AppliesToProblem>` to `claims.rs`, by the `applies_to` row of [claims.csv](../reference/corpus-format.md#claimscsv). A cell with no `=` is prose: `AppliesTo::Prose`, not read. A cell with `=` is pairs `key=value` separated by a semicolon and one space; the keys are `event`, `entered_from`, `entered_to`, `contract_type`, `fiscal_year`, `calendar` and `basis`, each at most once; `event` is one of `award_date`, `solicitation_date`, `clause_date`, `cdrl_date`, `query_date` and `event_date`; `entered_from` and `entered_to` are `PartialDate` text; every other value is one word of lowercase letters, digits and underscores. `ClaimRow::from_row` makes a `Form` problem on `applies_to` when the cell holds `=` and `AppliesTo::parse` fails, and `check-register-cells` raises FB0204 for it. The structured fields are read only by the later items that apply a rule by date (`later-effective-as-of`); nothing here uses them.
8. Test `applies_to_reads_these_cells` with a table: prose with no `=`; `event=award_date; entered_from=2025-01-01; contract_type=fixed_price`; `entered_from=2025` (a year is a partial date); and the refusals `event=award_day`, a repeated `basis`, `color=red`, `entered_from=2025-13-01`, a semicolon with no space after it, `basis=` (an empty value) and `Basis=fixed`. Each refusal names the key or the cell. Add a claim row with cell `unit = "dollars"` to the problem rows of step 6: it is refused, and `unit = "weeks"` and `unit = "EUR"` are accepted.
9. Write `crates/fieldbook-core/tests/claims_no_panic.rs` with `parser_never_panics_on_200_seeded_inputs`. It includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 128 from `Rng::new(1)`, and takes a valid `applies_to` cell (step 8), a valid `unit` cell and the valid synthetic claim row of step 6, each with its bytes once flipped and once deleted (for the row, one cell at a time). Each goes through `String::from_utf8_lossy` into `AppliesTo::parse`, `ClaimUnit::parse` and, as a cell of a row, `ClaimRow::from_row`. The result is a value or a typed problem and nothing panics.

10. Extend `schema::rows::every_column_survives_from_row_to_cells` with claims: all 35 columns have distinct, valid, nonempty samples, including optional cells. `ClaimRow::from_row` then `to_cells` returns the same cells; a column with no sample fails and names it.

11. Implement the extension-locator grammar in Syntax at a glance in `schema/locator.rs`, which owns the shared append-only kind registry, `LocatorKind::Extension` and lossless kind/payload value. Closed vocabulary `ALL` tests cover the built-in rows; extensions have separate grammar/registry tests, not a fictitious finite ALL. `ClaimRow::from_row` validates that `ext:<kind>` matches the full locator and preserves canonical bytes through `to_cells`. Test `unknown_extension_claim_round_trips`, `extension_kind_and_payload_must_agree`, canonical escaping of UTF-8, `%`, `;`, `]` and newline, and refusal of malformed escapes, empty payload or malformed built-in syntax. A future kind is accepted structurally without claiming it resolves.

12. Add `extension_locator_parser_never_panics` in `claims_no_panic.rs`: use the shared RNG for 200 seeded inputs and single-byte flips/deletions of a canonical extension containing percent-encoded Unicode and delimiters. Exercise the shared parser directly and inside an otherwise valid ClaimRow with matching `ext:<kind>`. Require typed results without panics and byte-exact round trips for accepted canonical extensions. Include payloads ending in `%` and `%0`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core schema::claims schema::vocab schema::rows
cargo nextest run --locked -p fieldbook-core --test claims_no_panic
just check
```

Expected: all three succeed; the `is_raise` table has at least ten cases.

## Negative control

Make `is_raise` return false when only `verified_by_role` changes. The test `schema::claims::a_change_of_verified_by_role_is_a_raise` must fail. Restore the line. Let `AppliesTo::parse` accept a repeated key: `schema::claims::applies_to_reads_these_cells` must fail and name the cell. Restore the line. Take `record_field` out of the overlay values: `schema::vocab::overlay_values_are_marked` must fail. Restore the line. Let `AppliesTo::parse` call `unwrap()` on its first read: `claims_no_panic::parser_never_panics_on_200_seeded_inputs` must fail. Restore the line.

Drop `quote` from `ClaimRow::to_cells`: `every_column_survives_from_row_to_cells` must fail and name `quote`. Restore it.

Reject an unknown extension kind or normalize its payload: `unknown_extension_claim_round_trips` must fail. Accept malformed built-in syntax through the extension fallback: the locator grammar test must fail. Restore each.

Plant an unchecked read past a truncated percent escape: `extension_locator_parser_never_panics` must fail. Restore it.

## Rollback

Revert the merge commit. Nothing outside `fieldbook-core` uses the module yet.

## BLOCKED

The 35 columns of the reference do not match the table that `core-csv` tests. Write the first differing column and stop (format gate).

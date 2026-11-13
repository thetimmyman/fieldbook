# decision-package-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`templates/decision-package.xlsx` gains a fifth sheet, "Price change bridge", that explains the move from the v1 unit price to the v2 unit price line by line (template TP-06). Its figures are INDEX/MATCH formulas over the Cost build sheet with cached values from `fieldbook-calc`, and a self-check that the bridge steps add up to the difference. The sheet is written inside `decision_workbook`, so `render` and the decide-port seam are unchanged. The build stays byte-identical across runs.

## Why

- The bridge is the TP-06 row of the template table in [HANDOFF.md](../design/HANDOFF.md) ("price change bridge as a sheet in the decision package"), and the mockup's price-change figures are the shape reference. It is split from [decision-package](decision-package.md) at the seam "one sheet more": the first card carries the writer, the calc, the site wiring and the leak surface, and this card adds one sheet and its tests, to keep each card near 400 to 600 changed lines.
- It shares `crates/fieldbook-render/src/decision.rs` and `crates/fieldbook-render/tests/decision.rs` with `decision-package`, so it names that card under Cards merged with the shared paths in parentheses. [decide-port](drafts/decide-port.md) and [index-output-ports](drafts/index-output-ports.md) name only `decision-package`: they need the two-function seam and the file, which the first card delivers, and neither needs the bridge sheet.
- Same rules as the first card: [AD-13](../decisions/0001-foundation.md#ad-13-spreadsheet-outputs) (INDEX/MATCH only, cached values, no `IFERROR` masking, a self-check cell), control XL-02 and DT-01. The origin tags are `strings::ORIGIN_LABELS` (Quoted, Computed, Judgment). No token group.
- Lead decisions where the repository is silent:
  1. **Bridge order is fixed by the Cost build lines**: material (the Cost build line "material with escalation", so escalation is inside it and not a step of its own), labour, overhead, G&A and profit, in that order, five steps, each step the change of that line between v1 and v2 (the "change" column of Cost build), ending at the total change. These five lines add to the unit price (total cost input is material plus labour plus overhead, burdened cost is that plus G&A, the unit price is burdened cost plus profit), so a reader can add the steps and reach the v2 price from the v1 price without a hidden step.
  2. **Rounding.** A step is the difference of two lines already rounded to cents, so the steps add to the total change exactly, with no residual line. The self-check compares the sum of the steps with the difference of the two unit prices, both at `Stage::Total`.
  3. **No new `CALC_FNS` row**, as in the first card (lead decision 3 there): the cached values use `sub` and `add` from `fieldbook-calc`.
  4. **The only percent on the sheet is each step's share of the total change.** The sheet shows no percent change of the price, so a zero v1 price is not special. A zero total change makes every share "n/a: zero denominator" (an explicit zero test, no `IFERROR`).

## Scope

- Files: `crates/fieldbook-render/src/decision.rs`, `crates/fieldbook-render/tests/decision.rs`, `CHANGELOG.md`.
- Registration lines: none. The sheet is inside the existing `render`, so `site.rs`, `lib.rs` and `leak_outputs.rs` need no change (the first card's surface row already covers the bytes).
- Docs to update: `CHANGELOG.md` (`- Added: a price change bridge sheet in templates/decision-package.xlsx.`).

## Out of scope

- A bridge between two Corpus records, or a chart: the sheet is numbers and formulas only.
- Any change to the sheet order of the first card's four sheets, or to the signatures of `decision_workbook`, `save_workbook` and `render`.
- Removing the preview label (it waits for spike S2).

## Preconditions

- Cards merged: `decision-package` (`crates/fieldbook-render/src/decision.rs`, `crates/fieldbook-render/tests/decision.rs`).

## Steps

1. `decision.rs`: `bridge(&[CostLine], &[CostLine]) -> Result<Vec<BridgeStep>, CalcError>` returns the five steps of lead decision 1 and the total, from the rounded lines of the first card.
2. `decision.rs`: in `decision_workbook`, write the sheet "Price change bridge" after Assumptions (the existing four sheets keep their order). Columns: step, change, share of the price change (a percent, `n/a: zero denominator` when the total change is zero), origin. Each change is a formula that finds the Cost build change cell by INDEX/MATCH on the line label; the first row shows the v1 unit price and the last row the v2 unit price, both found by INDEX/MATCH; a "Check" row holds the formula that the v1 price plus the steps equals the v2 price at cents, cached value "OK". Cached values by `Formula::set_result` from the calc values, with `Worksheet::set_formula_result_default("")` called on the sheet before its first formula, as in the first card.
3. Update the Read-me sheet line that lists the sheets so it names the fifth.
4. Tests in `crates/fieldbook-render/tests/decision.rs`: `bridge_steps_add_to_the_total_change`; `bridge_cached_values_equal_an_independent_recomputation` (names the cell, such as `Price change bridge!B5`); `bridge_check_cell_is_ok`; `bridge_zero_total_change_reads_n_a_for_every_share` (an input whose two versions give equal unit prices); `bridge_has_five_steps_and_no_escalation_row` (the step labels are the five of lead decision 1, and each is found in Cost build); `bridge_formulas_use_only_the_allowed_functions` (the first card's XL-02 test must still pass with the new sheet, so also run it); `bridge_sheet_follows_assumptions` (the four earlier sheets keep names and order); `two_builds_same_bytes` still passes.
5. `CHANGELOG.md` line. Run `just site`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test decision
cargo nextest run --locked -p fieldbook --test leak_outputs
just site
T="$(mktemp -d)" && cp out/edition/templates/decision-package.xlsx "$T/first.xlsx" && just site && cmp "$T/first.xlsx" out/edition/templates/decision-package.xlsx && echo same
just check
```

Expected: the tests pass; the second build prints `same`; `just check` exits 0.

## Negative control

In `decision.rs`, leave one step (for example G&A) out of the bridge: `bridge_steps_add_to_the_total_change` must fail. Restore it. Take the overhead step from the unrounded line values instead of the rounded ones: `bridge_cached_values_equal_an_independent_recomputation` must fail and name the overhead step's cell. This fails only because the example input has overhead lines whose v1 and v2 differ in their sub-cent parts, so this card sets the v1 and v2 overhead inputs of `synthetic_example` in `decision.rs` to differ in their sub-cent parts if the first card's do not (for example 4.16625 in v1 and 4.28375 in v2 before rounding). Restore it. Change one bridge formula to use `OFFSET`: the XL-02 test `no_macro_external_link_hyperlink_dde_volatile_or_dynamic_array` must fail and name `Price change bridge` and the cell. Restore it. Drop one `set_result` on the sheet: `every_formula_cell_has_a_cached_value` must fail. Restore it.

## Rollback

Revert the merge commit. The workbook returns to four sheets, and `render` keeps its signature.

## BLOCKED

`decision-package` has not merged: comment `BLOCKED: decision-package-2 waits on decision-package`, push the branch and take the next card. A step needs a `fieldbook-calc` operation that does not exist: comment `BLOCKED: decision-package-2 needs <operation>`, and do not write float arithmetic.

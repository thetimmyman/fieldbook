# build-charts-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A `calc` page shows its CalcTable: the outputs of the case named by its `Fixture:` line, with the rounding sentence and the pass or fail of the case in the footer. This is the second half of `build-charts`, split at the chart-panel and calc-table seam.

## Why

- A `calc` page names its case with a `Fixture:` line in its Worked example ([Page files](../reference/corpus-format.md#page-files)). The figures come from the calc core and never from a model ([AD-14](../decisions/0001-foundation.md#ad-14-money-and-the-calculation-core-fm0)); the cases are CALC-01 to CALC-04 in [FM0](../decisions/0001-foundation.md#calculation-core-fm0). The rounding sentence is the `rounding` string in [Strings](../reference/corpus-format.md#strings).
- Design: the [MAP.md](../design/MAP.md) row "CalcTable". [HANDOFF.md](../design/HANDOFF.md) section 5 Components (CalcTable) and section 2 Page template: eight parts. Token groups: colour (`--color-bad`, `--color-ok`, `--color-muted`, `--color-surface-2`), type, space.
- Why a core change: the pass or fail needs the case's expected outputs, and the render crate has no TOML reader. `fieldbook_core::cases` holds the case text and runs a case but exposes no check of it, so this card adds one small function there (`verify`, a wrapper over `fieldbook_calc::verify_case`).

## Scope

- Files: `crates/fieldbook-core/src/cases.rs`, `crates/fieldbook-render/src/{lib.rs,calc_table.rs,types.rs}`, `crates/fieldbook-render/templates/parts/calc_table.html`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/{charts.rs,page_types.rs}`, `crates/fieldbook-render/tests/snapshots/`, `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (the `- Added:` line of `build-charts` gains "and the calc table on calc pages").

## Out of scope

- `page.rs`, `page.html` and `templates/types/`: a calc page still renders through `templates/page.html`, because `types.rs` returns `Err(common)` for `PageType::Calc`; the CalcTable is injected into the common struct there.
- A case that refuses (CALC-90) as a table: it shows `Unavailable`.

## Preconditions

- Cards merged: `build-charts` (it adds `charts::format_amount`, which this card reuses and extends with parentheses for a negative amount) and `calc-cases`.

## Steps

1. `fieldbook-core/src/cases.rs`: `pub fn verify(id: &str) -> Option<Result<(), Vec<Mismatch>>>` (None for an unknown id), with a test in its `mod tests` for CALC-01 passing and an unknown id.
2. `calc_table.rs` and `parts/calc_table.html`: on a `calc` page, read the `Fixture:` id from the Worked example, `cases::run` it, and render CalcTable with the footer HANDOFF gives (the `rounding` string and the pass or fail of `verify`). A negative amount prints in parentheses in `--color-bad`. An unknown id, or a case that refuses, shows `Unavailable`; `check` already reports a malformed id. `types.rs` appends the table to the Worked example body of `common` for `PageType::Calc` and still returns `Err(common)`.
3. `app.css`: the `.calc` and `.calc-foot` rules with the existing tokens; no new custom property, no hex colour.
4. Tests in `tests/charts.rs`: `calc_table_shows_fixture_result` (a synthetic `calc` page naming CALC-01; the cells equal the outputs of `cases::run`, typing no number) and `negative_amount_in_parentheses`.
5. Remove Calc from `GOLDEN_LATER` in `tests/page_types.rs` and record `calc_page_renders`, now including CalcTable. Run the full page-type golden coverage test.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core cases::
cargo nextest run --locked -p fieldbook-render --test charts
cargo nextest run --locked -p fieldbook-render --test page_types
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p xtask literals::
just site
just check
```

Expected: the tests pass; the guarded line exits 0; `just site` exits 0; `just check` exits 0.

## Negative control

In `calc_table.rs`, add one cent to the first output cell, so the cell no longer equals what `cases::run` returned. `calc_table_shows_fixture_result` must fail and print the output name, the value from `cases::run` and the value found. Restore the file.

Remove `calc_page_renders`: `every_page_type_has_a_render_golden` must fail and name calc. Restore it.

## Rollback

Revert the merge commit. `calc` pages show the Worked example as text again; `cases::verify` stays unused.

## BLOCKED

`fieldbook-calc` offers no check of a case against its expected outputs (`verify_case` is removed or private): set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Do not copy a case's numbers into the render crate.

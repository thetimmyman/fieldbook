# build-charts

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The area pages carry their charts as inline SVG written by the build: the RTE rate build-up as a waterfall from the calculation case CALC-02, and an `Unavailable` panel for every chart without data; a `calc` page shows its CalcTable with the fixture result.

## Why

- Charts are inline SVG in `a/<area>.html` with no chart script ([AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types)). The figures come from the calc core and never from a model ([AD-14](../decisions/0001-foundation.md#ad-14-money-and-the-calculation-core-fm0)); the cases are the Day-1 calculation cases CALC-01 to CALC-03 in [FM0](../decisions/0001-foundation.md#calculation-core-fm0).
- A `calc` page names its case with a `Fixture:` line in its Worked example ([Page files](../reference/corpus-format.md#page-files)). The rounding sentence is the `rounding` string in [Strings](../reference/corpus-format.md#strings).
- Design: the [MAP.md](../design/MAP.md) rows "Charts" (ChartPanel, LineChart, BarList, ChartFoot, TableFold, Unavailable) and "CalcTable". [HANDOFF.md](../design/HANDOFF.md) section 2 Area workspaces (which charts Overview holds), section 5 Components and its Charts subsection, section 12 Implementation notes. Token groups: colour (`--chart-1`, `--chart-neutral`, `--color-fg`, `--color-bad`, the status tokens), type, space.

## Scope

- Files: `crates/fieldbook-render/src/{lib.rs,charts.rs,svg.rs,calc_table.rs,area.rs,types.rs}`, `crates/fieldbook-render/templates/area.html`, `crates/fieldbook-render/templates/parts/{chart_panel.html,calc_table.html}`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/charts.rs`, `CHANGELOG.md`; `crates/fieldbook-render/tests/page_types.rs`, `crates/fieldbook-render/tests/snapshots/`.
- Docs to update: `CHANGELOG.md` (`- Added: charts on the area pages and the calc table on calc pages.` under `## Unreleased`).

## Out of scope

- The EST escalation series and the company series beside each chart: no Corpus format holds a data series yet, so they stay `Unavailable`. Bid and rate history stay `Unavailable` until Release 2.
- Sensitivity, Bridge and the decision-package charts (Day 30 and later).

## Preconditions

- Cards merged: `build-about`, `calc-cases`, `build-page-types`.
- The lead may move this card after P3 (PLAN.md, Cut line and floor). The area pages then keep their empty-state lines.

## Steps

1. `charts.rs` and `parts/chart_panel.html`: ChartPanel with the headline figure in HTML, the plot, ChartFoot and TableFold. TableFold holds the chart's numbers as a real `<table>` with a caption. `svg.rs` gains the BarList track and the LineChart plot, with the geometry and stroke rules of HANDOFF Charts.
2. RTE Overview: a BarList waterfall of the CALC-02 outputs `landed`, `cost` and `price`, in that order, read from the `Outcome` that `cases::run` returns (`Outcome { outputs: Vec<RunOutput>, run: Run }`, from `calc-cases`). The first bar is `landed`, from 0. Each next bar spans from the previous output to this one, so a step is a difference. The last bar, `price`, is the total in `--color-fg`, and every other bar is in `--chart-1`. The output `unit` is a per-unit figure on another basis and is not a bar. The scale runs from 0 to the largest output. ChartFoot names the case as its source.
3. EST Overview and the history charts: the `Unavailable` part from `build-source-pages`, filled with the thing, the release or trigger, and one line of why.
4. `calc_table.rs` and `parts/calc_table.html`: on a `calc` page, read the `Fixture:` id from the Worked example, run that case, and render CalcTable with the footer HANDOFF gives (the `rounding` string and the pass or fail of the case). A negative amount prints in parentheses in `--color-bad`. An unknown id shows `Unavailable`; `check` already reports it.
5. `app.css`: chart, bar, legend and table rules with the existing tokens. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF. No mark uses `--color-accent`.
6. Tests in `tests/charts.rs`: `waterfall_rows_are_differences_of_outcome_outputs` (the bars are `landed`, `cost` minus `landed` and `price` minus `cost`, taken from `Outcome.outputs`, and the last bar ends at `price`; `calc-cases` already pins the outputs to the expected values in `calc_02_matches`, so this test types no number); `svg_has_role_label_and_no_text` (`role="img"`, an `aria-label`, no `<text>`, no `<script>`, no `style` attribute); `marks_never_use_the_accent`; `est_shows_unavailable`; `calc_table_shows_fixture_result` (a synthetic `calc` page naming CALC-01); `negative_amount_in_parentheses`.

7. Remove Calc from `GOLDEN_LATER` in `tests/page_types.rs` and record `calc_page_renders`, now including CalcTable. Run the full page-type golden coverage test.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test charts
just site
grep -c '<svg' out/edition/a/rte.html
grep -c 'class="unavailable"' out/edition/a/est.html
! grep -l -E '<text|<script>|style="' out/edition/a/*.html
! grep -L 'http-equiv="Content-Security-Policy"' out/edition/a/*.html | grep .
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p xtask literals::
cargo nextest run --locked -p fieldbook-render --test page_types
just check
```

Expected: the tests pass; `just site` exits 0; both `grep -c` lines print at least `1`; the guarded lines exit 0; the literal test passes; `just check` exits 0.

## Negative control

In `charts.rs`, add one cent to the second row of the waterfall. `cargo nextest run --locked -p fieldbook-render --test charts waterfall_rows_are_differences_of_outcome_outputs` must fail and print the bar, the expected value and the value found. Restore the file.

Remove `calc_page_renders`: `every_page_type_has_a_render_golden` must fail and name calc. Restore it.

## Rollback

Revert the merge commit. The area pages lose their charts and keep their empty-state lines; `calc` pages show the Worked example as text.

## BLOCKED

`calc-cases` exposes no API that a renderer can call to run a case (the cases are only test files): set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Do not copy CALC-02's numbers into this crate.

# search-page-view

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook search` lists exact hits, then pages (a page's sections collapsed into one row), then every other kind under one heading, and `--units` prints the former flat unit list. A golden file pins the view so that `search-js` can be held to it.

## Why

- On the real Corpus most search units are claim rows, so a flat unit list is dominated by claims and a page lead rarely reaches the top five. The owner chose the page-grouped view as the default.
- [Query procedure](../reference/search.md#query-procedure) is unchanged: ranking, average field lengths and tokenization stay as they are. The [regression set](../reference/search.md#regression-set) still judges the unit list, so FB0127 counts do not change and no regression case changes.
- [Published compatibility](../reference/compatibility.md) binds calculations, run schemas, adapters, templates and schema keywords, not `--json` reports, so the added report fields stay at `report_version` 1.
- `tests/golden/json/search.json` gains `view`, by the recipe "Changing a golden or snapshot": the sentence that decides it is the `--json` bullet of [The command line](../reference/search.md#the-command-line).

## Scope

- Files: `crates/fieldbook-core/src/search/pages.rs`, `crates/fieldbook-core/tests/golden_page_view.rs`, `crates/fieldbook/src/search.rs`, `crates/fieldbook/tests/search.rs`, `crates/fieldbook/tests/snapshots/`, `tests/golden/search/page-view.json`, `tests/golden/json/search.json`, `tests/golden/json/search-pages.json`, `docs/reference/search.md`, `docs/reference/cli.md`, `docs/cards/search-page-view.md`.
- Registration lines: `crates/fieldbook-core/src/search/mod.rs` (the module line and its exports); `crates/fieldbook-core/src/report.rs` (the `view` field and the page-row fields of the search report); `crates/fieldbook/src/cli.rs` (the `--units` flag); `crates/fieldbook/tests/json_reports.rs` (one row); `tests/golden/README.md` (the row and the file name of the page view); `docs/plan/PLAN.md` (this card's line); `docs/cards/search-js.md` and `docs/cards/search-palette.md` (one line each).
- Docs to update: `docs/reference/search.md`, `docs/reference/cli.md` (generated), `docs/plan/PLAN.md`, `docs/cards/search-js.md`, `docs/cards/search-palette.md`, `tests/golden/README.md`, `CHANGELOG.md`.

## Out of scope

- The ranking, the tokenizer, `query`, the ranked goldens and the regression set.
- The JavaScript mirror (`search-js`) and the palette (`search-palette`).
- Mapping a claim to the pages that cite it.

## Preconditions

- Cards merged: `search-cli-and-regression`.

## Steps

1. Write the golden and the failing tests at the public interface: `cargo nextest run --locked -p fieldbook-core --test golden_page_view` and `cargo nextest run --locked -p fieldbook --test search`.
2. Add the pure view in `crates/fieldbook-core/src/search/pages.rs`, above `query`: no I/O and no dependence on a hash order.
3. Make the view the default of `fieldbook search`, add `--units`, and add the report fields. Plain and JSON take the same selection.
4. Follow [Adding a JSON report](README.md#adding-a-json-report) for the page rows: the golden `search-pages.json`, one row in `json_reports.rs` and `cargo xtask docs`.
5. Update the docs named above.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test golden_page_view
cargo nextest run --locked -p fieldbook --test search
cargo nextest run --locked -p fieldbook-core --test golden_ranked
cargo nextest run --locked -p fieldbook --test json_reports
just check
```

Expected: exit 0 from each, and step 7 of `just check` reports no new regression miss.

## Negative control

(a) Key the page group on the whole unit id, so that sections are not collapsed: `golden_page_view` fails naming the grouping cases. (b) Leave OTHER rows out of the view: `golden_page_view` fails and the CLI test `other_rows_are_listed_after_the_heading` fails. Restore the file after each.

## Rollback

Revert the merge. The default output returns to the flat unit list, and `--units` and the added report fields go with it. Nothing is stored between runs.

## BLOCKED

If a ranked golden, a seed golden or the regression set would have to change: `BLOCKED: the page view changed <case id>`. A negative control that stops failing stops the card.

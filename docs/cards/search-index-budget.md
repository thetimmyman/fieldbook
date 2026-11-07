# search-index-budget

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The edition's site search index is specified to hold page and section units only (kinds `page` and `clause`) under a cap of 15 MB, and `build` enforces that cap on `assets/search-index.js` alone: every other data file keeps its 3 MB cap. The specification, the check catalogue and the cards that build or use the site index say the same thing.

## Why

- Owner decision: the edition's site search index holds page and section units only, and its cap is 15 MB. Measured on the real Corpus, the full index (every unit) is 34.6 MB and the pages and sections alone are 14.6 MB, against a 3 MB cap. The command line keeps searching every unit. Claims, sources, questions, conflicts and terms stay reachable on the site through the register pages and `ids.html`.
- FB0605 is a lint on built site output (`build`, range 06 of the [check catalogue](../reference/corpus-format.md#06-site-and-edition)). It accepts or refuses no Corpus file, so the [schema impact](../reference/corpus-format.md#scope-and-versioning) is none.
- The budget lines are in [Budgets](../reference/search.md#budgets), [The site index](../reference/search.md#the-site-index) and AD-11 and AD-24 in [ADR 0001](../decisions/0001-foundation.md).

## Scope

- Files: `crates/fieldbook-render/src/lint.rs`, `crates/fieldbook-render/tests/site_lints.rs`.
- Docs to update: `docs/reference/corpus-format.md` (the FB0605 row), `docs/reference/errors.md` (generated: `cargo xtask docs`), `docs/reference/search.md`, `docs/decisions/0001-foundation.md` (AD-11, the JS budgets line, the `later-sqlite-tantivy` trigger), `docs/reference/dependencies.md` (the `rusqlite`, `tantivy` row), `docs/controls.md` (WB-03), the cards `search-site-index`, `search-js`, `search-palette`, `scale-fixture` and `page-context-packs`, `docs/plan/PLAN.md` and `CHANGELOG.md`.

## Out of scope

- Building the index: `search-site-index` does that. This card changes the budget and the text only.
- `docs/design/`, which is the interface lane's; the stale lines there go in the pull request description for a separate pull request.
- `corpus/eval/regression.toml` and every seed golden.

## Preconditions

- Cards merged: `build-site-lints`.

## Steps

1. In `lint.rs`, keep `DATA_FILE` at 3,000,000 for every other data file and add `SEARCH_INDEX: u64 = 15_000_000`, applied only to `assets/search-index.js`.
2. In `site_lints.rs`, prove that `assets/search-index.js` passes at the cap and fails one byte over, that `assets/assist-data.js` and a `data/` file still fail one byte over 3 MB, and that the FB0605 row states both numbers.
3. Reword FB0605 in `corpus-format.md`, regenerate `errors.md`, and update the other documents named in Scope.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test site_lints
cargo xtask docs --check
just check
```

Expected: the tests pass, `docs --check` exits 0 and `just check` exits 0.

## Negative control

Set `SEARCH_INDEX` to 3,000,000: the at-the-cap test must fail. Restore it. Apply the 15 MB cap to every data file: the other-file test must fail. Restore it.

## Rollback

Revert the merge commit. The cap returns to 3 MB for every data file, and `search-site-index` has not yet written an index that would exceed it.

## BLOCKED

Anything shows that the FB0605 wording is a Corpus format change (a guard, a freeze test or ADR 0002 text): write what it is and stop.

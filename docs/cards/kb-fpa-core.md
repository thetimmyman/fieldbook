# kb-fpa-core

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The FP&A area holds three core pages, drafted at entry values: the annual budget and how it rolls up from programs and indirect pools, the rolling forecast and how it is reconciled to backlog and estimates at completion, and variance analysis of actuals against budget and forecast.

## Why

- The gap. FP&A has 39 pages, and nearly all of them restate Regulation S-K disclosure items (pay ratio, description of securities). Nothing says how a defence contractor builds a budget, keeps a forecast or explains a variance, which is the area's everyday work.
- Sources. Few public primary documents state FP&A practice. Where one does, cite it: variance analysis and thresholds in the EVMS guide (`S-0040`), funding and obligation rules already cited by the backlog pages, the critical-accounting-estimate items of Regulation S-K (`S-0055`). A practice that no public source states is an `assumption` claim with an evidence grade, never a fact, and the page says "single source" or "Not established." where that is the truth ([corpus/AGENTS.md](../../corpus/AGENTS.md#claims-quotes-and-citations)).
- Corpus work runs on `kb/fpa-core`. Entry values only.
- Design: none.

## Scope

- Files: `corpus/pages/fpa/` (new pages, and `links` lines of existing pages that should point to them), `corpus/registers/claims.csv` (appended rows only).
- Docs to update: none.

## Out of scope

- Any company figure, plan or calendar: worked examples are synthetic.
- A new source: if a page needs a public source the register lacks, list it in the pull request for a sources card.
- Rewriting the Regulation S-K pages.

## Preconditions

- Cards merged: `kb-hub-pages` (both append to `claims.csv`).

## Steps

1. Write `fpa-annual-budget-build` (procedure) under `planning-chain/`: the inputs (backlog, new business, indirect pool budgets and their bases), the order of the build, and how the budget sets the planned indirect rates; link `rte-pool-and-base-structure`.
2. Write `fpa-rolling-forecast` (procedure) under `forecast-vintage/`: the cadence, what is re-estimated each cycle, how the forecast ties to backlog and to the estimates at completion of the program-finance pages, and how a forecast vintage is kept.
3. Write `fpa-variance-analysis` (concept) under `kpi/`: price, rate, volume and mix variances, their arithmetic in a synthetic worked example, the thresholds an explanation needs, and in Never confuse how a budget variance differs from an earned-value cost variance.
4. Draft each claim at entry values. Grade every assumption.
5. Run `fieldbook check --strict`; fix every error and every warning on a consequential claim.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
cargo xtask guard-statuses origin/main HEAD
just site
just check
```

Expected: `check` exits 0; the guard exits 0; `just site` exits 0 with no FB0605; `just check` exits 0.

## Negative control

Remove the `evidence_grade` of one new assumption claim: `check` must fail and name the claim. Restore it. Remove the line `Synthetic example. The numbers are made up.` from `fpa-variance-analysis`: `check` must fail with FB0133. Restore it.

## Rollback

Revert the merge commit. The three pages and their claims go.

## BLOCKED

`just site` fails with FB0605: comment `BLOCKED: index cap, <bytes>` on the work item and wait for the size plan of `adr-0006-glossary-units`.

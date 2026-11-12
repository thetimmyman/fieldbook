# kb-rates-practitioner

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The indirect-rates area holds four practitioner pages, each drafted at entry values with cited claims: the wrap-rate build-up, the structure of fringe, overhead and G&A pools and their bases, the cycle from provisional billing rates to final rates, and a walkthrough of an incurred cost submission.

## Why

- The gap. Indirect rates has 35 pages, almost all clause cards. "wrap rate" appears on no page and "provisional billing" on two, and no page says how fringe, overhead and G&A pools are built and stacked. A reader who knows the rules cannot find how the work is done.
- Sources. FAR 42.704 and 42.705 (`S-0005`), the allowable-cost and payment clause in FAR 52 (`S-0006`), CAS 418 and 410 (`S-0014`), CAM chapter 6 (`S-0037`) and the Contract Audit Manual chapters the Corpus already holds (`S-0036`). A step of practice that no public source states is an `assumption` claim with an evidence grade, never a fact ([corpus/AGENTS.md](../../corpus/AGENTS.md#claims-quotes-and-citations)).
- Entry values only: every new claim is `unchecked`, drafted by the role that wrote it; no page is `stable`. Corpus work runs on `kb/rates-practitioner`.
- Size: each page adds to the edition search index, which has little headroom under its 15 MB cap ([ADR 0006](../decisions/0006-edition-search-index.md)).
- Design: none. The pages use existing page types.

## Scope

- Files: `corpus/pages/indirect-rates/` (new pages, and `links` lines of existing pages that should point to them), `corpus/registers/claims.csv` (appended rows only).
- Docs to update: none.

## Out of scope

- Glossary rows and regression cases: `search-glossary-aliases`.
- Company rates, company pools or any figure that is not invented and marked synthetic.
- Rewriting existing clause cards.

## Preconditions

- Cards merged: `search-ranking-warnings` (it edits `corpus/pages/`).

## Steps

1. Write `rte-wrap-rate-build-up` (concept): direct labour, then fringe, overhead and G&A applied in turn, the arithmetic in a synthetic worked example, and in Never confuse how a wrap rate differs from a provisional billing rate and from a forward pricing rate.
2. Write `rte-pool-and-base-structure` (concept): what each pool holds, the usual bases, how a pool's base can include another pool's allocation, and which CAS and FAR rules bound each choice.
3. Write `rte-billing-to-final-rate-cycle` (procedure): provisional billing rates, interim adjustment, the final indirect cost rate proposal, audit, negotiation or unilateral determination, and the final adjustment of billed costs.
4. Write `rte-incurred-cost-submission-walkthrough` (procedure): the schedules of a submission and what each shows, the due date rule, and adequacy, each step citing its claim.
5. Draft each claim at entry values with a locator and a quote of at most 300 characters copied from the original. Link the pages to one another and to the existing clause cards through `links`.
6. Run `fieldbook check --strict`; fix every error and every warning on a consequential claim. Record the four page ids in the pull request.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
cargo xtask guard-statuses origin/main HEAD
just site
just check
```

Expected: `check` exits 0; the guard exits 0; `just site` exits 0 with no FB0605; `just check` exits 0.

## Negative control

Leave the Never confuse part of `rte-wrap-rate-build-up` blank: `check` must fail with FB0131 and name the page. Restore it. On a scratch commit set one new claim's `verification` to `primary_source_supported`: `cargo xtask guard-statuses origin/main HEAD` must fail and name the claim. Drop the commit.

## Rollback

Revert the merge commit. The four pages and their claims go.

## BLOCKED

`just site` fails with FB0605, or a step has no public source and cannot be written as a graded assumption: comment `BLOCKED: <what>` on the work item with the page and the figure, push the branch and take the next card.

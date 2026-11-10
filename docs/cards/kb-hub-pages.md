# kb-hub-pages

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A reader can start from one overview of the FAR 31.205 cost principles, one page per contract type, and a concept hub for each Cost Accounting Standard this part writes, in number order from CAS 401 within about 600 changed lines; each page links every existing page about its subject, and `kb-hub-pages-2` carries the standards this part leaves.

## Why

- The gap. Coverage is 1,692 clause cards out of 1,709 pages, with 7 concept pages. CAS pages are split across areas (CAS 412 has five compliance pages and one estimating page) with no page that gathers them. "FAR 31.205" returns single cost principles and no overview; "fixed price incentive" returns clause prescriptions above any explanation of the contract type. `cmp-cas-overview` and `ctf-contract-types` exist and stay the top of each family.
- A hub is a `concept` page: what the standard or type is for, how it works, a synthetic example, what it is often confused with, and `links` to the clause cards. Its claims cite the codified text (`S-0003`, `S-0013`, `S-0014`, `S-0041`) at entry values.
- Corpus work runs on `kb/hub-pages`. The search cases for these pages are appended by `search-glossary-aliases`.
- Size: about 27 pages against the index headroom ([ADR 0006](../decisions/0006-edition-search-index.md)). The card splits at a family boundary ([Size and splitting](README.md#size-and-splitting)): the cost-principles overview and the contract-type pages come first, and the CAS hubs not written by about 600 lines move to `kb-hub-pages-2`.
- Design: none. The pages use the `concept` type.

## Scope

- Files: `corpus/pages/compliance/cas-allowability/` (new hubs, and the `links` line of `cmp-cas-overview`), `corpus/pages/contract-finance/billing-payment/` (new contract-type pages, and `links` of `ctf-contract-types`), `corpus/registers/claims.csv` (appended rows only).
- Docs to update: none.

## Out of scope

- A list of related pages or backlinks in the rendered page: that is a renderer card of its own.
- Rewriting the clause cards the hubs link to.
- CAS 419, which is reserved.

## Preconditions

- Cards merged: `kb-rates-practitioner` (both append to `claims.csv`, and the overview links its pages).

## Steps

1. Write `cmp-far-31-205-cost-principles` (concept) under `compliance/cas-allowability/`: what a cost principle decides, the factors of allowability in FAR 31.201-2, and a grouped list linking each cost-principle clause card.
2. Write one page per contract type under `contract-finance/billing-payment/`: firm-fixed-price, fixed-price incentive (firm target), fixed-price with economic price adjustment, cost-plus-fixed-fee, cost-plus-incentive-fee, cost-plus-award-fee, time-and-materials and labour-hour. Each says who carries cost risk and how fee or profit is set, citing FAR 16 (`S-0041`), and is added to the `links` of `ctf-contract-types`.
3. For each of the 19 standards, list the existing pages that cite it, from `rg` over `corpus/pages/`, in the pull request.
4. Write `cmp-cas-<nnn>-hub` for each standard: Purpose, The rule (the standard's purpose and fundamental requirement, cited), How it works, a synthetic worked example, Never confuse (a real contrast with a neighbouring standard or FAR rule), Company calibration, and `links` to every page of Step 3. Add each hub to the `links` of `cmp-cas-overview`; this is the one existing CAS page the step edits, and the clause cards a hub links to stay as they are.
5. Split when the pull request reaches about 600 lines: move the standards not yet written to `docs/cards/kb-hub-pages-2.md`, add its line to PLAN.md after this one, and say so in the pull request. Steps 1 and 2 stay in this part, because `search-glossary-aliases` needs their pages.
6. Run `fieldbook check --strict`; fix every error and every warning on a consequential claim.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
cargo xtask guard-statuses origin/main HEAD
just site
just check
```

Expected: `check` exits 0; the guard exits 0; `just site` exits 0 with no FB0605; `just check` exits 0.

## Negative control

Cite a claim id that does not exist, `[@CL-CMP-9999]`, in one new page: `check --strict` must fail and name the id and the page. Restore it. Leave its Never confuse part blank: `check` must fail with FB0131. Restore it.

## Rollback

Revert the merge commit. The hubs and their claims go, and `cmp-cas-overview` and `ctf-contract-types` lose the added `links` entries.

## BLOCKED

`just site` fails with FB0605: comment `BLOCKED: index cap, <bytes>` on the work item and wait for the size plan of `adr-0006-glossary-units`. A standard's text is not in a source version of the register: comment `BLOCKED: <standard> has no source` and write the others.

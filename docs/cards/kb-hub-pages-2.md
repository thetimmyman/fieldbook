# kb-hub-pages-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A reader can start from one concept hub for each Cost Accounting Standard that `kb-hub-pages` left, in number order from CAS 404 to CAS 420 (16 standards, CAS 419 being reserved), each linking every existing page about its subject; the `cmp-cas-<nnn>-hub` pages of CAS 401 to 403 and the cost-principles and contract-type pages are already in `kb-hub-pages`.

## Why

- The gap and the page form are those of `kb-hub-pages`: CAS pages are split across areas with no page that gathers them. `cmp-cas-overview` stays the top of the family.
- A hub is a `concept` page: what the standard is for, how it works, a synthetic example, what it is often confused with, and `links` to every page that cites it. Its claims cite the codified text (`S-0013`, `S-0014`) at entry values.
- The standards of this part, by number: 404, 405, 406, 407, 408, 409, 410, 411, 412, 413, 414, 415, 416, 417, 418 and 420. Standards 404, 407, 408, 409 and 411 have few pages of their own; a hub for one of them may link the GAAP-conformance pages and say "Not established." in a part with nothing to cite.
- Corpus work runs on `kb/hub-pages-2`. Entry values only.
- Size: about 16 pages against the index headroom ([ADR 0006](../decisions/0006-edition-search-index.md)). If the pull request passes about 600 changed lines, split again at a standard boundary ([Size and splitting](README.md#size-and-splitting)).
- Design: none. The pages use the `concept` type.

## Scope

- Files: `corpus/pages/compliance/cas-allowability/` (new hubs, and the `links` line of `cmp-cas-overview`), `corpus/registers/claims.csv` (appended rows only).
- Docs to update: none.

## Out of scope

- A list of related pages or backlinks in the rendered page: that is a renderer card of its own.
- Rewriting the clause cards the hubs link to.
- CAS 419, which is reserved.
- The hubs of CAS 401 to 403, the cost-principles overview and the contract-type pages (`kb-hub-pages`).

## Preconditions

- Cards merged: `kb-hub-pages` (both append to `claims.csv`, and `cmp-cas-overview` is edited by both).

## Steps

1. For each standard of this part, list the existing pages that cite it, in the pull request. The list is the pages whose text names the standard (`rg` over `corpus/pages/` for "CAS nnn" or "9904.nnn"), plus the pages that cite a claim whose `citation_codified` falls in 48 CFR 9904.nnn, found by joining the `[@CL-...]` ids in each page to `corpus/registers/claims.csv`. A text search alone misses pages that cite a standard only through a claim: for 404, 405, 408, 409, 410, 413, 417, 418 and 420 it does.
2. Write `cmp-cas-<nnn>-hub` for each standard: Purpose, The rule (the standard's purpose and fundamental requirement, cited), How it works, a synthetic worked example, Never confuse (a real contrast with a neighbouring standard or FAR rule), Company calibration, and `links` to every page of Step 1. Add each hub to the `links` of `cmp-cas-overview`; this is the one existing CAS page the step edits, and the clause cards a hub links to stay as they are.
3. Split when the pull request reaches about 600 lines: move the standards not yet written to `docs/cards/kb-hub-pages-3.md`, add its line to PLAN.md after this one, and say so in the pull request.
4. Run `fieldbook check --strict`; fix every error and every warning on a consequential claim.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
cargo xtask guard-statuses origin/main HEAD
just site
just check
```

Expected: `check` exits 0; the guard exits 0; `just site` exits 0 with no FB0605; `just check` exits 0.

## Negative control

Cite a claim id that does not exist, `[@CL-CMP-9999]`, in one new hub: `check --strict` must fail and name the id and the page. Restore it. Leave its Never confuse part blank: `check` must fail with FB0131. Restore it.

## Rollback

Revert the merge commit. The hubs and their claims go, and `cmp-cas-overview` loses the added `links` entries.

## BLOCKED

`just site` fails with FB0605: comment `BLOCKED: index cap, <bytes>` on the work item and wait for the size plan of `adr-0006-glossary-units`. A standard's text is not in a source version of the register: comment `BLOCKED: <standard> has no source` and write the others.

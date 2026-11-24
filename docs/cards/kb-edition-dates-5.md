# kb-edition-dates-5

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

In `cmp`, the Edition date row of each clause card's rule table gives the clause's edition with a citation, or `Not established.` only where no source version in the register states it; the pages of the area that Part 4's size budget left.

## Why

- Continuation of `kb-edition-dates`, `kb-edition-dates-2`, `kb-edition-dates-3`, and `kb-edition-dates-4`. Parts 1 to 4 covered `fnd`, `mkt`, `est`, `sup`, the eight areas `rte`, `prg`, `fpa`, `ctf`, `cls`, `mna`, `opm`, `ind`, and the first 142 fillable `cmp` clause cards (audit-retention, business-systems, cas-allowability, and the first 122 of regulatory-framework in the tool's page order). This part carries the remaining 89 fillable `cmp` pages (the rest of regulatory-framework and trade-cyber), about 105 new claims, in about 372 changed lines, within this part's budget.
- The area does not split at a whole-subarea seam: its largest subarea, regulatory-framework, alone needs about 840 changed lines (195 fillable clause cards, 252 new claims), so Part 4 stopped in the tool's page order and this part carries the rest.
- A date in a page cites a claim ([corpus/AGENTS.md](../../corpus/AGENTS.md#claims-quotes-and-citations)). Where a cited claim of the page already carries `edition_date`, the row cites it. Otherwise the card drafts a new claim at entry values whose statement gives the edition, at the clause heading locator, with the heading quoted exactly.
- Size: as many whole areas per part, with their claims, as fit in about 600 changed lines ([Size and splitting](README.md#size-and-splitting)).
- Design: none.

## Scope

- Files: `corpus/pages/` (the Edition date row and the Change note of each page this part fills), `corpus/registers/claims.csv` (appended rows only).
- Docs to update: none.

## Out of scope

- Any other row or part of a page.
- Editing the `edition_date` of an existing claim.
- Showing edition dates in the renderer.

## Preconditions

- Cards merged: `kb-never-confuse-pass`, `kb-never-confuse-pass-2`, `kb-never-confuse-pass-3`, `kb-never-confuse-pass-4`, `kb-never-confuse-pass-5`, `kb-never-confuse-pass-6` (all edit pages across every area), `kb-edition-dates` (Part 1 covers `fnd`, `mkt`, and `est`), `kb-edition-dates-2` (Part 2 covers `sup`), `kb-edition-dates-3` (Part 3 covers `rte`, `prg`, `fpa`, `ctf`, `cls`, `mna`, `opm`, and `ind`), `kb-edition-dates-4` (Part 4 covers the first 142 fillable `cmp` pages).

## Steps

1. With a script that is not committed, list for each `cmp` page with the empty row that Part 4 left the clause it cards, whether a cited claim has `edition_date`, and the heading text of the clause in its original. Put the counts in the pull request.
2. Fill the rows that a cited claim covers.
3. For the rest, draft one claim per clause: `kind` `fact`, `edition_date` set, locator at the clause heading, the heading quoted exactly, entry values. Cite it in the row.
4. Add a Change note line to each page.
5. Run `fieldbook check --strict`.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
cargo xtask guard-statuses origin/main HEAD
just check
```

Expected: `check` exits 0; the guard exits 0; `just check` exits 0.

## Negative control

Cite a claim id that does not exist, `[@CL-CMP-9999]`, in one filled row: `check --strict` must fail and name the id. Restore it. On a scratch commit set one new claim's `verification` to `primary_source_supported`: the guard must fail and name the claim. Drop the commit.

## Rollback

Revert the merge commit. The rows say `Not established.` again and the new claims go.

## BLOCKED

A clause's original has no dated heading: leave `Not established.` and list the page. An original's heading disagrees with an existing claim's `edition_date`: note the disagreement on the work item and carry on.

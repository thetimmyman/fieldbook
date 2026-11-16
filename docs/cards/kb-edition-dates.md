# kb-edition-dates

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

In the areas this part covers, listed in the pull request (as many pages as fit in about 600 changed lines), the Edition date row of each clause card's rule table gives the clause's edition with a citation, or `Not established.` only where no source version in the register states it; `kb-edition-dates-2` and on carry the other areas.

## Why

- The measurement. 1,686 pages say `| Edition date | Not established. |`. The claims register has an `edition_date` column, filled on 2,286 claims, 2,266 of them from the RFO FAR 52 model (`S-0020`); a codified FAR or DFARS clause states its date in its heading, for example "(APR 1984)", which the register's original holds.
- A date in a page cites a claim ([corpus/AGENTS.md](../../corpus/AGENTS.md#claims-quotes-and-citations)). Where a cited claim of the page already carries `edition_date`, the row cites it. Otherwise the card drafts a new claim at entry values whose statement gives the edition, at the clause heading locator, with the heading quoted exactly. Corpus work runs on `kb/edition-dates`.
- Size: one line per page, so the card runs in parts by area ([Size and splitting](README.md#size-and-splitting)), as many pages per part, with their claims, as fit in about 600 changed lines.
- Design: none.

## Scope

- Files: `corpus/pages/` (the Edition date row and the Change note of each page this part fills), `corpus/registers/claims.csv` (appended rows only).
- Docs to update: none.

## Out of scope

- Any other row or part of a page.
- Editing the `edition_date` of an existing claim.
- Showing edition dates in the renderer.

## Preconditions

- Cards merged: `kb-never-confuse-pass`, `kb-never-confuse-pass-2`, `kb-never-confuse-pass-3` (all edit pages across every area).

## Steps

1. With a script that is not committed, list for each page with the empty row the clause it cards, whether a cited claim has `edition_date`, and the heading text of the clause in its original. Put the counts per area in the pull request.
2. Fill the rows that a cited claim covers.
3. For the rest, draft one claim per clause: `kind` `fact`, `edition_date` set, locator at the clause heading, the heading quoted exactly, entry values. Cite it in the row.
4. Add a Change note line to each page.
5. Stop at about 600 changed lines. Write `docs/cards/kb-edition-dates-2.md` with the remaining areas, add its line to PLAN.md after this one, and say so in the pull request.
6. Run `fieldbook check --strict`.

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

A clause's original has no dated heading: leave `Not established.` and list the page. An original's heading disagrees with an existing claim's `edition_date`: comment `BLOCKED: <claim id> edition date` on the work item and carry on.

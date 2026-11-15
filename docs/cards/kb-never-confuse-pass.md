# kb-never-confuse-pass

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every Never confuse part in the areas this part covers, the first areas in order up to about 600 changed lines and listed in the pull request, states a contrast, a thing the subject is not or differs from, or says `Not established.`, and none of them only restates a rule sentence of the page; `kb-never-confuse-pass-2` and on carry the other areas.

## Why

- The measurement. Of 1,703 Never confuse parts, 724 contain no contrast word (not, versus, unlike, rather than, differs, instead of) and repeat a sentence of The rule or How it works. The part exists to stop a reader from mixing up two things ([corpus-format.md](../reference/corpus-format.md), the parts table); a repeated rule tells the reader nothing.
- A rewrite is one paragraph on one line. A contrast that states a rule or a number cites a claim; a new claim is drafted at entry values. Editing the statement, source, locator or quote of a claim that is not `unchecked` voids its check in the same commit ([corpus/AGENTS.md](../../corpus/AGENTS.md#claims-quotes-and-citations)); prefer citing an existing claim. Where no true contrast is known, the part says `Not established.`, never a guess.
- Every page that changes meaning gets a Change note line, newest first.
- Size: about 724 lines each way, so the card runs in parts by area, as [Size and splitting](README.md#size-and-splitting) says: this part takes the first areas up to about 600 changed lines, and `kb-never-confuse-pass-2` and on take the rest. Corpus work runs on `kb/never-confuse-pass`.
- Design: none.

## Scope

- Files: `corpus/pages/` (the Never confuse part and the Change note of each page this part rewrites), `corpus/registers/claims.csv` (appended rows only), `docs/cards/kb-never-confuse-pass-2.md`.
- Registration lines: `docs/plan/PLAN.md` (the `kb-never-confuse-pass-2` line), `docs/cards/kb-edition-dates.md` (`Cards merged` adds `kb-never-confuse-pass-2`).
- Docs to update: none.

## Out of scope

- Any other part of a page, and the edition dates of the rule table: `kb-edition-dates`.
- Raising any status, and changing an existing claim's evidence.
- A lint for contrast words: `check` does not judge prose.

## Preconditions

- Cards merged: `kb-fpa-core` (every Corpus card before it edits pages and claims).

## Steps

1. List the 724 parts with a script that is not committed: page id, area and the first 80 characters, in the pull request, grouped by area.
2. Area by area, rewrite each part as a contrast with the nearest thing a reader confuses it with: a neighbouring clause, the FAR versus the CAS rule, a pool versus a base, a billing rate versus a final rate. Keep cited sentences cited.
3. Add a Change note line to each page: `COMPATIBLE_EXTENSION, <date>: Never confuse states a contrast.`
4. Stop at about 600 changed lines. Write `docs/cards/kb-never-confuse-pass-2.md` with the remaining areas, add its line to PLAN.md after this one, and say so in the pull request.
5. Run `fieldbook check --strict`.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
cargo xtask guard-statuses origin/main HEAD
just check
```

Expected: `check` exits 0; the guard exits 0; `just check` exits 0. The pull request lists the parts rewritten and those left for the next part.

## Negative control

Cite a claim id that does not exist, `[@CL-RTE-9999]`, in one rewritten part: `check --strict` must fail and name the id and the page. Restore it. Leave one rewritten part blank: `check` must fail with FB0131. Restore it.

## Rollback

Revert the merge commit. The parts return to their earlier text.

## BLOCKED

A rewrite would need a source the register lacks: write `Not established.` for that part and list it in the pull request. A cited claim's evidence would have to change: leave the claim, comment `BLOCKED: <claim id> evidence` on the work item and carry on with the next page.

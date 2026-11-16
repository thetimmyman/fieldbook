# kb-never-confuse-pass-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every Never confuse part in the areas this part covers, the first remaining areas after `estimating` in order up to about 600 changed lines and listed in the pull request, states a contrast, a thing the subject is not or differs from, or says `Not established.`, without only restating a rule sentence of the page; `kb-never-confuse-pass-3` and on carry any areas still remaining.

## Why

- Continuation of `kb-never-confuse-pass`, which rewrote `foundation`, `market` and `estimating` and stopped at the whole-area seam around 600 changed lines. Across the ten remaining areas in `corpus/corpus.toml` order (`supply-chain`, `indirect-rates`, `program-finance`, `fpa`, `contract-finance`, `compliance`, `close-revrec`, `mna`, `operating-model` and `industrialization`), 663 Never confuse parts contain no contrast word (not, versus, unlike, rather than, differs, instead of), plus any part in those areas that mentions `not` only inside a repeated rule sentence ([corpus-format.md](../reference/corpus-format.md), the parts table).
- A rewrite is one paragraph on one line. A contrast that states a rule or a number cites a claim; a new claim is drafted at entry values. Editing the statement, source, locator or quote of a claim that is not `unchecked` voids its check in the same commit ([corpus/AGENTS.md](../../corpus/AGENTS.md#claims-quotes-and-citations)); prefer citing an existing claim. Where no true contrast is known, the part says `Not established.`, never a guess.
- Every page that changes meaning gets a Change note line, newest first.
- Size: the card runs in parts by area, as [Size and splitting](README.md#size-and-splitting) says: this part takes the first of the remaining areas (`supply-chain`, `indirect-rates`, `program-finance`, `fpa`, `contract-finance` and on in `corpus/corpus.toml` order) up to about 600 changed lines, and `kb-never-confuse-pass-3` and on take the rest and redirect `kb-edition-dates` to the new part. Corpus work runs on `kb/never-confuse-pass-2`.
- Design: none.

## Scope

- Files: `corpus/pages/` (the Never confuse part and the Change note of each page this part rewrites), `corpus/registers/claims.csv` (appended rows only), `docs/cards/kb-never-confuse-pass-3.md`.
- Registration lines: `docs/plan/PLAN.md` (adds `kb-never-confuse-pass-3`), `docs/cards/kb-edition-dates.md` (adds `kb-never-confuse-pass-3` to `Cards merged`).
- Docs to update: none.

## Out of scope

- Areas already covered by `kb-never-confuse-pass` (`foundation`, `market`, `estimating`), any other part of a page, and the edition dates of the rule table: `kb-edition-dates`.
- Raising any status, and changing an existing claim's evidence.
- A lint for contrast words: `check` does not judge prose.

## Preconditions

- Cards merged: `kb-fpa-core`, `kb-never-confuse-pass` (`kb-fpa-core` edits pages and claims; `kb-never-confuse-pass` rewrites the earlier areas on the same pattern).

## Steps

1. List the remaining parts in `supply-chain`, `indirect-rates`, `program-finance`, `fpa`, `contract-finance`, `compliance`, `close-revrec`, `mna`, `operating-model` and `industrialization` with a script that is not committed: page id, area and the first 80 characters, in the pull request, grouped by area.
2. Area by area in `corpus/corpus.toml` order, rewrite each Never confuse part that lacks a contrast (including any part that contains `not` only inside a restated rule sentence) as a contrast with the nearest thing a reader confuses it with: a neighbouring clause, the FAR versus the CAS rule, a pool versus a base, a billing rate versus a final rate. Keep cited sentences cited.
3. Add a Change note line to each page: `COMPATIBLE_EXTENSION, <date>: Never confuse states a contrast.`
4. Stop at a whole-area seam around 600 changed lines. If areas remain, write `docs/cards/kb-never-confuse-pass-3.md` with those areas, add its line to PLAN.md after this one, add `kb-never-confuse-pass-3` to `Cards merged` in `docs/cards/kb-edition-dates.md`, and say so in the pull request.
5. Run `fieldbook check --strict`.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
cargo xtask guard-statuses origin/main HEAD
just check
```

Expected: `check` exits 0; the guard exits 0; `just check` exits 0. The pull request lists the parts rewritten and any areas left for the next part.

## Negative control

Cite a claim id that does not exist, `[@CL-RTE-9999]`, in one rewritten part: `check --strict` must fail and name the id and the page. Restore it. Leave one rewritten part blank: `check` must fail with FB0131. Restore it.

## Rollback

Revert the merge commit. The parts return to their earlier text.

## BLOCKED

A rewrite would need a source the register lacks: write `Not established.` for that part and list it in the pull request. A cited claim's evidence would have to change: leave the claim, comment `BLOCKED: <claim id> evidence` on the work item and carry on with the next page.

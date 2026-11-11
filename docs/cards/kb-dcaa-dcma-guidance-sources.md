# kb-dcaa-dcma-guidance-sources

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The sources register holds, byte-exact under `corpus/sources/public/` with one row each, the DCAA Contract Audit Manual chapters 3, 4, 7, 10 and 14, DCAA's public guidance for contractors, and the DoD Contract Pricing Reference Guides, each taken from its publisher's public site.

## Why

- The gap. The Corpus has 70 source versions. Of the Contract Audit Manual only chapters 5, 6, 8 and 9 are in (`S-0036` to `S-0039`); there is no DCAA guidance written for contractors, no DCMA material and no Contract Pricing Reference Guide. The practitioner pages of `kb-rates-practitioner` (audit planning, selected areas of cost, incurred cost) need primary sources a claim can cite, and [corpus/AGENTS.md](../../corpus/AGENTS.md) forbids sourcing a claim to a summary.
- The recipe is [Adding a version](../reference/corpus-format.md#adding-a-version). An agent may add public sources and their originals ([AGENTS.md](../../AGENTS.md#authority)). Corpus work runs on a `kb/<topic>` branch, here `kb/dcaa-dcma-guidance-sources`.
- Public only. Every file is a public release by a federal agency. A file that carries a distribution or handling marking is not public; it stops the card ([corpus/AGENTS.md](../../corpus/AGENTS.md#hard-stops)).
- Design: none.

## Scope

- Files: `corpus/sources/public/` (new files only), `corpus/registers/sources.csv` (appended rows only).
- Docs to update: none.

## Out of scope

- Claims and pages that cite the new sources: `kb-rates-practitioner` and later cards.
- Extraction and `s/` locator anchors: `extract-and-locators`.
- Any DCMA instruction or manual that is marked, restricted or behind a login.

## Preconditions

- Cards merged: none.

## Steps

1. List the candidate documents with their publisher, title, edition and size in the pull request: the five CAM chapters, the DCAA guidance pages or pamphlets for contractors (for example the audit process overview and the incurred cost submission guidance), the Contract Pricing Reference Guide volumes, and any public DCMA instruction on contractor business systems or forward pricing rates. Drop any file over 25 MiB (`cargo xtask hosted-limits` refuses it) and say so.
2. For each, take the file from the publisher's public site, open it, and stop on any marking. Add it as [Adding a version](../reference/corpus-format.md#adding-a-version) says, and complete the row: `location` is the address it came from, `lifecycle` is `active`, `handling` is `public`, `source_family` and `family_basis` at their entry values (an agent never marks a family basis reviewed, so two of these sources never count as corroboration).
3. Run `fieldbook check`. Fix every error the new rows raise.

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
git diff --diff-filter=MD --name-only origin/main -- corpus/sources/public
cargo xtask guard-statuses origin/main HEAD
just check
```

Expected: `check` exits 0; the `git diff` prints nothing (no original was changed or deleted); the guard exits 0; `just check` exits 0.

## Negative control

Change one byte of a new original in the working tree: `fieldbook check` must fail with the hash mismatch code and name the file. Restore it. Blank the `location` of a new row: `check` must report it. Restore it.

## Rollback

Revert the merge commit. The files and rows go; no claim cites them yet.

## BLOCKED

A document carries a marking, needs a login, or exists only as a copy on a site other than its publisher's: comment `BLOCKED: <document> <reason>` on the work item, leave it out, and finish the rest.

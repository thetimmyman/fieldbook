# design-claims-copy-paths

- **Runs where:** home
- **Schema impact:** none
- **Review:** owner

## Outcome

A revision of `docs/design/` is accepted in which every place the design names the claims copy for Excel says what the build writes after `claims-csv-parts`: one file per claims part, `data/claims-<AREA>-<part>.csv`, and no `data/claims.csv`, with the claims index's links to the part files specified, so that no build card invents wording or a link.

## Why

- The need. `claims-csv-parts` replaces `data/claims.csv` with one CSV per claims part. The design still names the old file: the Paths row of [HANDOFF.md](../design/HANDOFF.md) section 0 (`data/<register>.csv`), the same wording in `fieldbook_core::strings::PATHS`, and the mockup sentence "Every build writes data/claims.csv". A build card may not edit `docs/design/` ([PLAN.md](../plan/PLAN.md#stop-and-ask)), so the design is stale until this revision.
- What the revision must say: the Paths row's wording for the register copies with the claims parts named beside `data/<register>.csv`; the mockup sentence in the same words; whether the claims index (`claims.html`) and each part page (`claims/<AREA>-<part>.html`) link the part's CSV, and if so the link's text, place, accessible name and narrow-width and print behaviour (section 5, `RegisterTable`); and the frozen string `PATHS` as the build must hold it.
- Take every name and rule from what `claims-csv-parts` merged, not from its card: the file stem, the part rule (`fieldbook_core::ids::claim_part`), and the rule that the set of part CSVs equals the set of part pages. This revision invents no file, no index CSV and no grouping.
- This is a revision of the interface lane in a pull request of its own; each revision needs acceptance ([AGENTS.md](../../AGENTS.md#authority)), so the card is `Review: owner`.
- Design: this card is the design. It follows the `openui-design` skill at design time where it adds a link to the claims index or a part page: a study of the row or line that carries the link, implemented in the mockup. A wording-only change needs no study and says so in section 13. It reuses existing tokens and components.

## Scope

- Files: `docs/design/HANDOFF.md`, `docs/design/MAP.md`, `docs/design/README.md`, `docs/design/MANIFEST.txt`, `docs/design/mockup/index.html`.
- Registration lines: none.
- Docs to update: the design files this revision changes; `MANIFEST.txt` is re-hashed.

## Out of scope

- Any Rust, template, CSS or script change, including the text of `fieldbook_core::strings::PATHS`: a build card after this revision brings the build to it.
- The files themselves and the edition checks: `claims-csv-parts`.
- The other register copies, `data/registers.xlsx` and the templates.

## Preconditions

- Cards merged: `claims-csv-parts` (the files this revision names). The revision is numbered after the current one in `docs/design/README.md`, and after `design-brief-print-footnote` if that merged first.

## Steps

1. Read what merged: list `out/edition/data/` after `just site` and note the part files' names, and read the claims index and one part page for any CSV link the build already renders.
2. `HANDOFF.md` section 0, the Paths row: name the claims parts beside the other register copies, in the form the build writes. Search the whole of `docs/design/` for `claims.csv` and for `data/<register>.csv` and bring each hit to the same wording; list the hits in the pull request.
3. The mockup: the sentence "Every build writes data/claims.csv" in the new wording, with synthetic part names.
4. Section 5 (`RegisterTable`, the claims index): state whether a part's CSV is linked, and specify the link if it is. The `#claims` row of `MAP.md` names this revision.
5. A revision paragraph at the top of `HANDOFF.md`, the README's current revision, the study record or the wording-only note in section 13, and `MANIFEST.txt` re-hashed.

## Acceptance

```sh
(cd docs/design && sha256sum -c MANIFEST.txt)
! grep -rn 'data/claims\.csv' docs/design
typos docs/design
just check
```

Expected: each file prints `OK` and `sha256sum` exits 0; the `grep` finds nothing, so the second line exits 0; `typos` exits 0; `just check` exits 0.

## Negative control

Put the sentence "Every build writes data/claims.csv" back into the mockup: the `grep` line of Acceptance must print it and exit 1. Then change one byte of `docs/design/HANDOFF.md` after hashing: `(cd docs/design && sha256sum -c MANIFEST.txt)` must print `HANDOFF.md: FAILED` and exit 1. Restore both.

## Rollback

Revert the merge commit. The design names `data/claims.csv` again, which the build no longer writes.

## BLOCKED

`claims-csv-parts` has not merged: comment `BLOCKED: the claims-copy revision waits on claims-csv-parts` and push nothing. What merged differs from this card's Why (another stem, an index CSV, a different part rule): follow what merged, say so in the pull request, and stop only if the design would need a new component or token: comment `BLOCKED: claims copy needs <what>`.

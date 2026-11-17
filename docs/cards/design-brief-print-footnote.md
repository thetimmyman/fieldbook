# design-brief-print-footnote

- **Runs where:** home
- **Schema impact:** none
- **Review:** owner

## Outcome

A revision of `docs/design/` is accepted that specifies the print form of a brief's Sources footnote (what a row prints, its type step, its columns and its spacing, all from existing tokens), such that each of the six real briefs fits one US Letter sheet with the unreleased-build banner counted, as its binder article and as its own page, so that `binder-brief-one-sheet` invents nothing.

## Why

- The need. [HANDOFF.md](../design/HANDOFF.md) section 12 (print contract) and the Print row of the build status banner say a brief prints on one Letter page with the banner inside that budget. Printed in headless Chromium from a build of `main`, the six briefs run to two or three sheets; the measurements and the levers already tried are in [binder-brief-one-sheet](binder-brief-one-sheet.md), under Why. The Corpus is not at fault: each brief is inside the 400-word rule of `FB0138`.
- The choice already made. The fix is a smaller print footnote, not a Corpus trim (the lead decision recorded in `binder-brief-one-sheet`). The design today gives the footnote no print form: section 5 (`SourcesBlock`) describes the screen row, and Print is a layout family, not a token table. A build card may not invent the form ([PLAN.md](../plan/PLAN.md#stop-and-ask): do not invent; do not edit `docs/design/` from a build card).
- What the revision must say: which parts of a Sources row print under a brief sheet and which do not (the claim statement, the status chip and its words, the source chip and its locator, the "Never verified" line, the back-references); the row's layout (inline or stacked, one column or two); the type step from the Type table; the gaps from the Space table; whether the binder head prints on the first article; and what stays true on paper: status words always print, a locator always prints, and paper never says more than the screen does.
- This is a revision of the interface lane in a pull request of its own; each revision needs acceptance ([AGENTS.md](../../AGENTS.md#authority)), so the card is `Review: owner`.
- Design: this card is the design. It follows the `openui-design` skill at design time: a study of the brief sheet in print, at Letter size, with and without the unreleased banner, implemented in the mockup. It reuses existing tokens; a value no token covers follows "Adding a token" in [the cards README](README.md).

## Scope

- Files: `docs/design/HANDOFF.md`, `docs/design/MAP.md`, `docs/design/README.md`, `docs/design/MANIFEST.txt`, `docs/design/mockup/index.html`.
- Registration lines: none.
- Docs to update: the design files this revision changes; `MANIFEST.txt` is re-hashed.

## Out of scope

- Any Rust, template, CSS or script change in the build: `binder-brief-one-sheet` implements the footnote in the last `@media print` block of `app.css`.
- Corpus text: no brief is edited and no cite is dropped.
- The screen form of `SourcesBlock`, and the print of pages that are not briefs.
- The Edge count in the Windows drill (`transfer-and-windows-drill`).

## Preconditions

- Cards merged: `day-one-binder-and-registers-xlsx` (the binder, the print block and `cargo xtask print-count`). The revision is numbered after the current one in `docs/design/README.md`.
- Tools at home: `chromium` and `pdftotext` on the path, and a built `just site`.

## Steps

1. Measure first. Build with `just site`, print the six standalone briefs and the binder with `chromium --headless=new --no-sandbox --no-pdf-header-footer --print-to-pdf=FILE URL`, and record each count from `cargo xtask print-count FILE` in the pull request.
2. Find the form by experiment, in a scratch copy of the built `app.css` that is not committed: apply candidate footnote forms using `var(--...)` tokens only until all six briefs print on one sheet with the banner, as binder articles and as standalone pages. Record the counts for the form chosen and for each form rejected.
3. Write the form into `HANDOFF.md`: section 12 (print contract) and the `SourcesBlock` and `BriefSheet` rows of section 5 state what a footnote row prints under a brief sheet, its layout, its type step and its spacing by token name, and what never drops (status words and locators). State the measured counts as the evidence in section 13.
4. The mockup: the brief's print view shows the new footnote for a synthetic brief with at least as many cites as the largest real brief, in the light values print uses.
5. The `MAP.md` row "Print: footnotes, one-page briefs" names this revision and `binder-brief-one-sheet`.
6. A revision paragraph at the top of `HANDOFF.md`, the README's current revision, the study record in section 13, and `MANIFEST.txt` re-hashed.

## Acceptance

```sh
(cd docs/design && sha256sum -c MANIFEST.txt)
typos docs/design
just check
```

Expected: each file prints `OK` and `sha256sum` exits 0; `typos` exits 0; `just check` exits 0. The pull request holds the before and after page counts of step 1 and step 2: six standalone briefs and six binder articles at `pages: 1` with the banner, under the form the revision specifies.

## Negative control

In the scratch copy of step 2, remove the footnote form: at least one brief must print more than one sheet and `cargo xtask print-count` must report `FB0607` for it. Then change one byte of `docs/design/HANDOFF.md` after hashing: `(cd docs/design && sha256sum -c MANIFEST.txt)` must print `HANDOFF.md: FAILED` and exit 1. Restore it.

## Rollback

Revert the merge commit. The design returns to the revision before it, and `binder-brief-one-sheet` stays blocked on its precondition.

## BLOCKED

No form built from existing tokens fits all six briefs with the banner while status words and locators still print: comment `BLOCKED: print footnote, <id> prints <n> sheets under <form>`, list the counts of every form tried, and stop; do not trim the Corpus and do not add a smaller type size. The form needs a token or a component the design does not give: comment `BLOCKED: print footnote needs <what>` and stop.

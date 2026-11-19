# design-workspace-opening

- **Runs where:** home
- **Schema impact:** none
- **Review:** owner

## Outcome

A revision of `docs/design/` is accepted that specifies what an empty Changes to review panel says for each check state of the claims in its scope, and that a workspace section whose object has not landed leaves the section order and the jump list for one closed "Not built yet" disclosure after the built sections, so that `changes-empty-by-check` and `workspace-unbuilt-fold` invent nothing.

## Why

- The need. On a build where no claim has been checked, Today and every area overview say "Nothing upstream has moved." under Changes to review. Contradicted and stale claims reach that panel only after a check, so the sentence states a result nobody observed ([HANDOFF.md](../design/HANDOFF.md) section 0, Fixed template text, and section 5, `ThreeUp`). On an area workspace, four sections that hold only a "not available until" line (Processes, Procedures, Templates, Module) sit between the Overview and the area's library pages, which then begin below the first screen at 1560 × 980 and several screens down on a phone ([HANDOFF.md](../design/HANDOFF.md) section 2, Area workspaces, and section 11, Empty workspace section).
- The pattern already exists. Revision 22 moved unbuilt navigation entries into one closed "Not built yet" disclosure that keeps each landing line; this revision applies the same form to a workspace's sections and its jump list.
- A build card may not invent the form ([PLAN.md](../plan/PLAN.md#stop-and-ask); [AGENTS.md](../../AGENTS.md#authority)): the fixed text and the section order are design, so the change is a revision of the interface lane in a pull request of its own, and the card is `Review: owner`.
- Design: this card is the design. It follows the `openui-design` skill at design time: one study compares three section layouts and sets the four Changes lines, using existing tokens and components only.

## Scope

- Files: `docs/design/HANDOFF.md`, `docs/design/MAP.md`, `docs/design/README.md`, `docs/design/MANIFEST.txt`, `docs/design/mockup/index.html`.
- Registration lines: none.
- Docs to update: the design files this revision changes; `MANIFEST.txt` is re-hashed.

## Out of scope

- Any Rust, template, CSS or script change in the build: `changes-empty-by-check` and `workspace-unbuilt-fold` implement the revision.
- The Overview's own panels (the module card, templates and a company series beside a chart), which keep their in-place lines.
- The print form of a workspace.

## Preconditions

- Cards merged: none. The revision is numbered after the current one in `docs/design/README.md`.

## Steps

1. Measure first: on a built edition at 1560 × 980, record where the Library pages heading of an area begins and where its landing-line sections sit.
2. Compare section layouts in a study: unbuilt sections folded after the built ones, unbuilt sections shrunk in place, and built sections first with unbuilt ones open after them; set the Changes lines for no claims, none checked, some checked and all checked.
3. Write the chosen form into `HANDOFF.md`: the revision paragraph, section 0 (Fixed template text), section 2 (Area workspaces), section 5 (`ThreeUp`, `Fold`), section 11, section 13 (the study) and section 16.
4. The mockup: a `#workspace/unbuilt` specimen, reached from the states gallery, shows the build form closed, open and narrow, and the four Changes lines.
5. `README.md` names revision 26, `MAP.md` gains the specimen's row, and `MANIFEST.txt` is re-hashed.

## Acceptance

```sh
(cd docs/design && sha256sum -c MANIFEST.txt)
typos docs/design
just check
```

Expected: each file prints `OK` and `sha256sum` exits 0; `typos` exits 0; `just check` exits 0. The specimen renders in Chromium with the disclosure closed by default, a jump list that links no unbuilt section, and no horizontal scroll at 375px.

## Negative control

Change one byte of `docs/design/HANDOFF.md` after hashing: `(cd docs/design && sha256sum -c MANIFEST.txt)` must print `HANDOFF.md: FAILED` and exit 1. Restore it.

## Rollback

Revert the merge commit. The design returns to revision 25, and the two build cards stay blocked on their precondition.

## BLOCKED

The form needs a token or a component the design does not give: comment `BLOCKED: workspace opening needs <what>` and stop.

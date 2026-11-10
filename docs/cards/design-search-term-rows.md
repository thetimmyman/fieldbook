# design-search-term-rows

- **Runs where:** home
- **Schema impact:** none
- **Review:** owner

## Outcome

A revision of `docs/design/` is accepted that specifies how a glossary term row looks on the edition's search page and in the command palette, with its wording, position, states and tokens, so that `search-site-terms` invents nothing.

## Why

- The need. `search-site-terms` puts glossary term units in the edition index, and `search.js` must then render a term row. `HANDOFF.md` section 10 and the `MAP.md` row Search: CommandPalette, SearchTrigger describe the edition index as page and section units only, and revision 19 says the page view's other rows are always empty on the edition. A build card may not invent a row ([PLAN.md](../plan/PLAN.md#stop-and-ask): do not invent; do not edit `docs/design/` from a build card).
- What the revision must say: where a term row sits among the page view's rows, what it shows (the term's label, the alias that matched, a short definition line or none), where it links (the term's row in `glossary.html`), how it reads in the palette's list, its accessible name, and the empty state when no term matches.
- The amendment that `adr-0006-glossary-units` adds to [ADR 0006](../decisions/0006-edition-search-index.md) admits the units and sets the size plan; this revision only gives them a look.
- This is a revision of the interface lane in a pull request of its own; the owner accepts each revision ([AGENTS.md](../../AGENTS.md#authority)), so the card is `Review: owner`.
- Design: this card is the design. It follows the `openui-design` skill at design time: a study for the search page's result list and the palette's list, implemented in the mockup. It reuses existing tokens; a value no token covers follows "Adding a token" in [the cards README](README.md).

## Scope

- Files: `docs/design/HANDOFF.md`, `docs/design/MAP.md`, `docs/design/README.md`, `docs/design/MANIFEST.txt`, `docs/design/mockup/index.html`.
- Registration lines: none.
- Docs to update: the four design files; `MANIFEST.txt` is re-hashed.

## Out of scope

- Any Rust, template, CSS or script change: `search-site-terms` builds the row.
- Claim, source, question and conflict rows on the edition.
- Any change of ranking, tokens, weights or synonyms.

## Preconditions

- Cards merged: `design-r22-reader-pages` (revision 22, its own design pull request), so this revision is numbered after it and builds on its text; `adr-0006-glossary-units` (the units this revision draws).

## Steps

1. Section 10 (Search) and section 5 (the result row and `CommandPalette`): the term row's place in the page view, its fields, its link to the term's `glossary.html` row, the matched alias, its accessible name and its empty state, on the search page and in the palette.
2. The `MAP.md` row Search: CommandPalette, SearchTrigger: the edition index's unit list says it holds term units, and the row names `search-site-terms`.
3. The mockup: one term row for a synthetic term with an alias on the search page and in the palette, at the widths the study covers, in both themes.
4. A revision paragraph at the top of `HANDOFF.md`, the README's current revision, the study record in section 13, and `MANIFEST.txt` re-hashed.

## Acceptance

```sh
cd docs/design && sha256sum -c MANIFEST.txt
typos docs/design
```

Expected: each file prints `OK` and `sha256sum` exits 0; `typos` exits 0.

## Negative control

Change one byte of `HANDOFF.md` after hashing: `sha256sum -c MANIFEST.txt` must print `HANDOFF.md: FAILED` and exit 1. Restore it.

## Rollback

Revert the merge commit. The design returns to the revision before it, and `search-site-terms` stays blocked on its precondition.

## BLOCKED

`design-r22-reader-pages` has not merged: comment `BLOCKED: the term-row revision waits on revision 22` and push the branch. The row needs a token, a component or a change to the frozen part order that the design does not give: comment `BLOCKED: term row needs <what>` and stop.

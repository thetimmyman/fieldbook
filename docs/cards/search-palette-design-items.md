# search-palette-design-items

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The palette's input placeholder, the quotes on its "Search for" row and the name of the shortcuts dialog follow the design, and the palette has no measurement left open at the smallest width.

## Why

- `search-palette` merged with these items left to the interface lane, in its pull request notes and on its work item. This card is the one list of them, so nothing is lost between the two.
- Each item, concretely:
  1. **Input placeholder.** The palette input has none. The mockup's is "Clause, term, page, DP-, AS-, Q- or CL- id…" (`docs/design/mockup/index.html`, `palInput`), which promises rows the edition does not return: revision 19 limits the edition index to page and section units ([ADR 0006](../decisions/0006-edition-search-index.md)). The design has to give the edition's wording; the build does not invent reader text.
  2. **Quotes on the "Search for" row.** The build prints `Search for <text>` (`app.js`, the `pal-search` row); the mockup prints `Search for “<text>”` with curly quotes.
  3. **Name of the shortcuts dialog.** The design's `Dialog` row says it is named by its h2. The build names it with `aria-label="Keyboard shortcuts"` on the `<dialog>` (`base.html`); it must use `aria-labelledby` pointing at the h2.
  4. **Six values no token covers**, recorded on the work item: palette input height `calc(var(--control-h) + var(--space-4))`, palette width 720px, palette top offset 10vh, palette max height 76vh, shortcuts dialog top 12vh and width 440px (the `Dialog` row gives the last two in words, with no token), and the switch track width `calc(var(--space-4) + var(--space-3))`. The design either adds tokens or says these are component geometry.
  5. **375px was not measured.** The top bar was measured by hand in headless Chromium from 500 to 1600px; the harness page loads no `app.css`, so the CSS has no automatic check at any width.
  6. **Results-list roles and announcements** were not checked in the `search-js` pull request. The palette is a combobox with a listbox; the design's `CommandPalette` row names the roles, and the search page's list has none checked.
- Items 1, 2 and 4 are changes to the design, so they need a revision of `docs/design/` by the interface lane, accepted by the owner, before this card builds them. Items 3, 5 and 6 are build work the design already settles.
- Not in this card, and found on the way: `app.js` is 29,998 of its 30,000-byte budget (FB0605) and `search.js` is 9,950 of 10,000. The next card that adds to either must trim it or raise the budget, which AD-15 owns. Steps 1 to 3 below are written to add no bytes to `app.js`; if they do, the card is BLOCKED.
- Design: `MAP.md` rows `CommandPalette`, `SearchTrigger` and `Dialog`; `HANDOFF.md` section 5 (those three rows) and section 10 (search); token groups: spacing, radius and z-index as they stand. A value no token covers is a gap: use the nearest token and follow "Adding a token" in [docs/cards/README.md](README.md).

## Scope

- Files: `crates/fieldbook-render/assets/app.js`, `crates/fieldbook-render/assets/search.js`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/templates/base.html`, `crates/fieldbook-render/templates/search.html`, `crates/fieldbook-render/tests/palette.rs`, `crates/fieldbook-render/tests/tokens.rs`, `xtask/harness/shortcuts.html`, `xtask/harness/shortcuts.js`, `xtask/src/harness.rs` (the required case ids only), `CHANGELOG.md`.
- Registration lines: none.
- Docs to update: `CHANGELOG.md` (`- Changed: the search palette has a placeholder, quoted search text and a named shortcuts dialog.`).

## Out of scope

- Any edit of `docs/design/`, `docs/intake/` or `SECURITY.md`.
- The byte budgets of `app.js` and `search.js`, and the Edge Unicode tables (the Windows drill checks them).
- Rows of other kinds in the palette. The edition index holds none.

## Preconditions

- Cards merged: `search-palette`, `search-js`.
- For items 1, 2 and 4: a `docs/design/` revision, accepted by the owner, that gives the placeholder wording, the quote characters and the six values (as tokens or as stated geometry).

## Steps

1. Item 3 (no design change needed): replace the `aria-label` of the shortcuts dialog in `base.html` by `aria-labelledby` pointing at its h2. Add a case to `xtask/harness/shortcuts.js` that reads the dialog's accessible name and fails when it is empty or differs from the h2 text, and add its id to the required cases in `xtask/src/harness.rs`.
2. Item 6: in `palette.rs` assert the roles the design names (`combobox`, `listbox`, `option`, `aria-selected` on the active row, `aria-activedescendant` on the input) on the built page, and the same for the search page's list. Add harness cases that move the selection with the arrow keys and read `aria-activedescendant`.
3. Item 5: add a harness page at 375px that loads the built `app.css` and checks that the top bar does not scroll sideways and that its buttons keep their 32px size. This needs `app.css` on the harness page, which `xtask/harness/shortcuts.html` does not load: load it, and keep the page's net-log at zero non-file requests.
4. Items 1, 2 and 4, once the revision is accepted: set the placeholder (from the spec text only, in `base.html`), use the quote characters the revision gives in the `Search for` row, and replace the six values by the tokens the revision adds, or by the stated geometry, in `app.css`. Update the `tokens.rs` expectations in the same commit.
5. Run `just site && just browser` and record the three new case ids and the 375px result in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test palette
cargo nextest run --locked -p fieldbook-render --test tokens
cargo nextest run --locked -p xtask harness::
just site && just browser
just check
```

Expected: the tests pass; `just browser` exits 0 with the line `harness: shortcut: shortcuts/shortcuts.html: N cases, 0 failed` where N is the earlier 18 plus the new cases, and the 375px page reports 0 failed; `just check` exits 0.

## Negative control

Remove the `aria-labelledby` of the dialog, or point it at a missing id: the new accessible-name case must fail in `just browser`, naming its id. Restore it. Add a rule that makes the top bar 400px wide at 375px: the 375px case must fail. Restore it.

## Rollback

Revert the merge commit. The palette returns to its merged behaviour; the harness loses the new cases.

## BLOCKED

The design revision for items 1, 2 and 4 is not accepted: do Steps 1 to 3 and 5, and comment `BLOCKED: palette placeholder, quote characters and tokens need the accepted design revision`. Or `app.js` or `search.js` would pass its budget: stop and comment `BLOCKED: app.js budget`.

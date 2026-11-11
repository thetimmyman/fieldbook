# search-palette-design-items

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The palette's input placeholder, the quotes on its "Search for" row and the name of the shortcuts dialog follow the design, and the roles of the palette and of the search page's list are checked.

## Why

- `search-palette` merged with these items left to the interface lane, in its pull request notes and on its work item. This card is the one list of them, so nothing is lost between the two.
- Each item, concretely:
  1. **Input placeholder.** The palette input has none. The mockup's is "Clause, term, page, DP-, AS-, Q- or CL- id…" (`docs/design/mockup/index.html`, `palInput`), which promises rows the edition does not return: revision 19 limits the edition index to page and section units ([ADR 0006](../decisions/0006-edition-search-index.md)). The design has to give the edition's wording; the build does not invent reader text.
  2. **Quotes on the "Search for" row.** The build prints `Search for <text>` (`app.js`, the `pal-search` row); the mockup prints `Search for “<text>”` with curly quotes.
  3. **Name of the shortcuts dialog.** The design's `Dialog` row says it is named by its h2. The build names it with `aria-label="Keyboard shortcuts"` on the `<dialog>` (`base.html`). The h2 does not exist in the built HTML: `app.js` builds it at run time (`make('h2', '', 'Keyboard shortcuts')`) with no id, so `aria-labelledby` has nothing to point at until the script gives it one, which costs bytes that `app.js` does not have (see below). Moving the h2 into `base.html` instead breaks the test that pins the dialog empty until the script fills it, and `app.js` would still append its own h2.
  4. **Six values no token covers**, recorded on the work item: palette input height `calc(var(--control-h) + var(--space-4))`, palette width 720px, palette top offset 10vh, palette max height 76vh, shortcuts dialog top 12vh and width 440px (the `Dialog` row gives the last two in words, with no token), and the switch track width `calc(var(--space-4) + var(--space-3))`. The design either adds tokens or says these are component geometry.
  5. **375px was not measured.** This needs a harness mechanism of its own (the browser flags are fixed by AD-08, so the page must be framed at 375px) and has its own card, `narrow-width-check`; it is not built here.
  6. **Results-list roles and announcements** were not checked in the `search-js` pull request. The palette is a combobox with a listbox; the design's `CommandPalette` row names the roles. `app.js` sets them at run time (`combobox`, `aria-expanded`, `aria-controls`, `aria-activedescendant`, `option`, `aria-selected`), so only a browser case can see them; the built HTML holds the search page's static `listbox` and `option` roles (`search.html`) and nothing of the palette's, which it pins empty.
- Items 1, 2 and 4 are changes to the design, so they need a revision of `docs/design/` by the interface lane, accepted by the owner, before this card builds them. Items 3 and 6 are build work the design already settles. Item 3 also needs bytes in `app.js` (Step 3).
- Not in this card, and found on the way: `app.js` is 29,998 of its 30,000-byte budget (FB0605) and `search.js` is 9,950 of 10,000. A byte added to either needs a trim of at least as many bytes elsewhere in the same file, in the same commit, or a raised budget, which AD-15 and the owner own. Steps 1 and 2 add no byte to either file; Step 3 adds about 25 to `app.js`.
- Design: `MAP.md` rows `CommandPalette`, `SearchTrigger` and `Dialog`; `HANDOFF.md` section 5 (those three rows) and section 10 (search); token groups: spacing, radius and z-index as they stand. A value no token covers is a gap: use the nearest token and follow "Adding a token" in [docs/cards/README.md](README.md).

## Scope

- Files: `crates/fieldbook-render/assets/app.js`, `crates/fieldbook-render/assets/search.js`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/templates/base.html`, `crates/fieldbook-render/templates/search.html`, `crates/fieldbook-render/tests/palette.rs`, `crates/fieldbook-render/tests/tokens.rs`, `xtask/harness/shortcuts.html`, `xtask/harness/shortcuts.js`, `CHANGELOG.md`.
- Registration lines: `xtask/src/harness.rs`, the case ids that Steps 1 and 2 add to the `shortcuts` entry of `SHORTCUT_PAGES` (nothing else in the file).
- Docs to update: `CHANGELOG.md` (`- Changed: the search palette has a placeholder, quoted search text and a named shortcuts dialog.`).

## Out of scope

- Any edit of `docs/design/`, `docs/intake/` or `SECURITY.md`.
- The byte budgets of `app.js` and `search.js` (a raised budget is the owner's AD-15 decision), and the Edge Unicode tables (the Windows drill checks them).
- A 375px check of the top bar: card `narrow-width-check`.
- Rows of other kinds in the palette. The edition index holds none.

## Preconditions

- Cards merged: `search-palette`, `search-js` and `unreleased-status-strip` (it extends `templates/base.html`, which Steps 3 and 4 also change).
- For items 1, 2 and 4: a `docs/design/` revision, accepted by the owner, that gives the placeholder wording, the quote characters and the six values (as tokens or as stated geometry).

## Steps

1. Pin the harness's dialogs to the build's, and check the name. The harness page `xtask/harness/shortcuts.html` carries its own copy of the two `<dialog>` tags, and nothing ties it to `base.html`. In `tests/palette.rs` add `harness_dialogs_match_base`: it reads both files (`include_str!`) and asserts that the `fb:palette` and `fb:keys` tags have identical attributes. In `xtask/harness/shortcuts.js` add the case `shortcuts_dialog_has_an_accessible_name`: after `?` opens the dialog, resolve its name (the text of the element its `aria-labelledby` names, else its `aria-label`) and fail when it is empty or differs from its h2's text. Add the id to the `shortcuts` entry of `SHORTCUT_PAGES`.
2. Item 6. In `tests/palette.rs`, on the built search page, assert the static roles: `search.html`'s `listbox` lists with their `aria-label` and `option` rows. The palette's roles are not in the built HTML, so add harness cases in `shortcuts.js`: `palette_roles_follow_the_design` (the input is a `combobox` with `aria-controls` naming the list; the list is a `listbox` of `option` rows) and `palette_arrow_keys_move_the_active_option` (each arrow key moves `aria-selected` and `aria-activedescendant` together). Add both ids to `SHORTCUT_PAGES`.
3. Item 3. In `app.js` give the h2 an id (`fb:keysTitle`) and, in the same commit, trim at least as many bytes elsewhere in `app.js` without changing behaviour, so that it stays within 30,000 bytes (FB0605). In `base.html` and in the harness copy replace the `aria-label` of the `fb:keys` dialog by `aria-labelledby="fb:keysTitle"`, and update the pins of `tests/palette.rs` in the same commit. If the trim cannot be found, or the interface lane's revision accepts `aria-label` as the name of a script-built dialog, change nothing here, say so on the work item, and keep the Step 1 case as the guard.
4. Items 1, 2 and 4, once the revision is accepted: set the placeholder (from the spec text only, in `base.html`), use the quote characters the revision gives in the `Search for` row, and replace the six values by the tokens the revision adds, or by the stated geometry, in `app.css`. Update the `tokens.rs` expectations in the same commit.
5. Run `just site && just browser` and record the new case ids in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test palette
cargo nextest run --locked -p fieldbook-render --test tokens
cargo nextest run --locked -p xtask harness::
just site && just browser
just check
```

Expected: the tests pass; `just browser` exits 0 with the line `harness: shortcut: shortcuts/shortcuts.html: N cases, 0 failed` where N is the earlier count plus the three new cases (`shortcuts_dialog_has_an_accessible_name`, `palette_roles_follow_the_design`, `palette_arrow_keys_move_the_active_option`); `just check` exits 0.

## Negative control

On the harness page the browser really loads, remove the `aria-label` of the `fb:keys` dialog in `xtask/harness/shortcuts.html` (after Step 3, point its `aria-labelledby` at a missing id): `just browser` must fail, naming `shortcuts_dialog_has_an_accessible_name`. Restore it. Then change one attribute of the `fb:keys` tag in `base.html` only: `harness_dialogs_match_base` must fail. Restore it. Then make the palette input drop its `aria-activedescendant` on an arrow key: `palette_arrow_keys_move_the_active_option` must fail. Restore it.

## Rollback

Revert the merge commit. The palette returns to its merged behaviour; the harness loses the new cases and the pin test.

## BLOCKED

The design revision for items 1, 2 and 4 is not accepted: do Steps 1, 2, 3 and 5, and comment `BLOCKED: palette placeholder, quote characters and tokens need the accepted design revision`. Or `app.js` or `search.js` would pass its budget with no trim found: stop that step and comment `BLOCKED: app.js budget`.

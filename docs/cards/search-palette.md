# search-palette

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every page has the SearchTrigger and the CommandPalette, which `/` and Ctrl K open and Esc closes, with focus kept inside and returned to the trigger. `?` opens the ShortcutsDialog, whose switch turns single-key shortcuts off; the choice is stored only inside try/catch. With scripts blocked the trigger is a plain link to `ids.html`. The Edge harness tests the shortcuts.

## Why

- The palette rules (id rows, "Search for", Enter) are in [The site index](../reference/search.md#the-site-index) (Palette, Without scripts). The engine is `search.js` from `search-js`.
- Script and storage rules are in [AD-15](../decisions/0001-foundation.md#ad-15-ui-technology-and-design-system). WB-03 (budgets) and WB-04 (storage) are in [controls.md](../controls.md); FB0605 holds the budgets.
- Design: the [MAP.md](../design/MAP.md) rows Search (CommandPalette, SearchTrigger) and ShortcutsDialog. [HANDOFF.md](../design/HANDOFF.md) §10 Search, §5 Components (SearchTrigger, CommandPalette, ShortcutsDialog, Switch) and §6 States and interactions (search trigger, palette no results, `?`, single-key shortcuts). Token groups: colour (`--color-scrim`, `--color-accent-soft`), layout, motion (`--dur-med`, `--ease-out`, `--shadow-pop`).

## Scope

- Files: `crates/fieldbook-render/assets/{app.js,app.css}`, `crates/fieldbook-render/templates/base.html`, `crates/fieldbook-render/templates/frame/` (the top bar's trigger and the sidebar foot's "Keyboard shortcuts" link), `xtask/harness/shortcuts.js`, the shortcut step in `xtask/src/harness.rs`, `crates/fieldbook-render/tests/palette.rs`.
- Docs to update: none. No command or verb arrives.

## Out of scope

- `.` for Ask (Day 30, `assistant-page`) and "Ask" on no results, beside "Not established in the library."
- `p` for precise wording, unless `build-page` already binds it; the dialog lists only shortcuts that exist.
- Recent searches in the palette, and filters by kind.

## Preconditions

- Cards merged: `search-js`.

## Steps

1. Templates: the SearchTrigger is an `<a href="ids.html">` (relative to the page) that `app.js` upgrades to a button showing `Ctrl K`. The palette and the dialog are `<dialog>` elements in `base.html`, empty until script fills them.
2. `app.js`: Ctrl K and `/` open the palette with focus in the input; `/` is ignored while focus is in a text field. Esc closes and returns focus to the trigger. Open the palette and the shortcuts dialog with `dialog.showModal()` (never `show()`, and never a `div` with `aria-modal`), so that the browser itself keeps focus inside while a dialog is open. A synthetic Tab cannot move focus, so no case tests wrap-around.
3. Empty input has its own empty state. For nonempty single-token input, list ID-prefix matches first in unit order, excluding section units, then fill with nonduplicate ranked results up to eight total. For multiword input use ranked results. Reuse `window.FB_SEARCH` ranking and row rendering; append “Search for <text>” linking to `search.html#q=`. Rows are `<li role="option">` in one `<ul role="listbox">`; Enter follows the highlighted link. No matches shows the abstention phrase from `window.FB_STRINGS`. Use `textContent` only.
4. `?` and the sidebar link open the ShortcutsDialog. Its Switch (`aria-pressed`) turns off `/`, `?` and every other single-key shortcut; Ctrl K and Esc keep working. The choice is stored under `fieldbook.singleKeys` as `on` or `off`; an absent key means the single-key shortcuts are on. Read and write it only inside try/catch; on failure show `not_remembered` from `window.FB_STRINGS` and keep the choice for the session.
5. `app.css`: scrim, panel, selected row and motion with the named tokens. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF. Honour `prefers-reduced-motion`. No `style=""`.
6. `shortcuts.js`: a harness page that loads `app.js`, dispatches key events and writes the outcomes into the DOM. Cases: Ctrl K opens; `/` opens; `/` in a text field types; Esc closes and returns focus; `?` opens the dialog; with the switch off, `/` and `?` do nothing and Ctrl K still opens; with storage throwing, the page still works. Also `modal_dialogs_contain_focus`: after each dialog opens, `dialog.matches(':modal')` is true and the dialog contains `document.activeElement`.
7. `tests/palette.rs`: every built page has the trigger link to `ids.html`, both dialogs, no `style=` attribute and the CSP meta once.

8. Browser tests cover a title-only single-token match, duplicate suppression between prefix and ranked groups, the eight-result cap, empty input and the final Search row. Removing ranked fallback must fail the title-only test.

9. Use the search-js exports load, rank(index, text, limit) and render(ordered_hits, text, container). The palette selects eligible id-prefix hits first in natural order, deduplicates ranked fallback and limits the combined list to eight; the render helper preserves that explicit order. A title-only hit remains reachable.

10. Exercise the real shared search.js exports in the palette browser tests: `load()`, `rank(index, text, limit)` and `render(ordered_hits, text, container)`. Assert prefix-first ordering, duplicate suppression and query highlighting. Restoring the swapped render arguments must fail this integration case.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test palette
just site && just browser
grep -L 'href="[./]*ids.html"' out/edition/index.html out/edition/p/*.html
grep -rc 'style="' out/edition --include=*.html | grep -v ':0$'
just check
```

Expected: the tests pass; `just browser` prints the shortcut cases passing and `0 non-file requests`; both greps print nothing (every page links `ids.html`, no page has `style="`); `just check` exits 0, with FB0605 holding `app.js` within 30 KB.

## Negative control

Remove the text-field test from the `/` handler, so `/` opens the palette while focus is in a text field. `just browser` must fail and name the case "slash in a text field types". Restore it. Then swap `showModal()` for `show()` where the palette opens: `just browser` must fail and name the case `modal_dialogs_contain_focus`. Restore it.

## Rollback

Revert the merge commit. The trigger stays a link to `ids.html` and `search.html` still works.

## BLOCKED

A shortcut cannot be tested in headless Edge without a flag that AD-08 forbids. Write which case and stop.

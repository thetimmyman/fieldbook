# views-and-day-n-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A reader can enter a start date in the browser; from 1280px the top bar then shows a "Day N of 90" chip, the Today day-one list offers "Set your start date" (from 1280px only until a day can be shown; below 1280px, where the chip is hidden, always), and the Plan timeline draws a dashed line for today. The date lives only in `localStorage` under `fieldbook.start`, is never injected at build, and a build that reads no clock stays byte-identical.

## Why

- The date is browser-only: NFR-D-03 says the build never reads the clock, the Controls row WB-04 covers browser storage, and [data handling](../intake/data-handling.md) already lists the Day-N start date as a `localStorage` value. The frozen strings are in the HANDOFF.md frozen-strings table (revision 24 froze the four start-date strings with `not_remembered`; the Day counter row of section 11 and `DayChip`, `Dialog` and `PlanTimeline` in section 5 give the rules).
- Design: the [MAP.md](../design/MAP.md) rows `DayChip` with "Set your start date" (header on every page, `index.html`, `app.js`) and `#plan` (the today line). [HANDOFF.md](../design/HANDOFF.md) section 5 (`DayChip`: unset, set, not remembered; `.day-chip` is a button that opens a dialog with one labelled `<input type="date">` and Close, Esc cancels and returns focus, hidden below 1280px and hidden until a day can be shown; the Today start prompt stays below 1280px; `PlanTimeline`: a dashed accent line marks today, the header reads "Day N of 90 · the line marks today" or "Set a start date to mark today", and `--z-raised` keeps rows above the line), section 6 States and interactions, section 8 Responsive behaviour and section 11 Day counter. Token groups: colour (`--color-accent`), type, space, layer (`--z-raised`).
- The frozen strings must come from `fieldbook-core::strings` and appear in the Strings section of [corpus-format.md](../reference/corpus-format.md#strings); `day-n-strings` added the four start-date strings beside `not_remembered`, so this card has no literal of its own.
- Lead decision (the repository is silent): the script goes into `assets/app.js`, not a new file. `app.js` has 32,000 bytes of budget (FB0605) and holds 30,447, so the new code must add no more than 1,500 bytes; `localIsoDate()` and `str()` are reused, not written again.
- No snapshot changes: the twelve page snapshots in `crates/fieldbook-render/tests/snapshots/` hold the top bar, the checklist snapshot also holds the plan header, and the drafts `handoff-new-note` and `assistant-page` list that directory; sharing it would make this card wait for both drafts, whose work it does not use (AGENTS.md). So `topbar.html` does not change: `app.js` adds the chip, the dialog and the today line at load, and the plan header's fallback text keeps its bytes. A page without script shows no chip and no line, as today.

## Scope

- Files: `crates/fieldbook-render/templates/parts/plan_timeline.html`, `crates/fieldbook-render/templates/index.html`, `crates/fieldbook-render/src/day_one.rs`, `crates/fieldbook-render/src/home.rs`, `crates/fieldbook-render/src/plan.rs`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/assets/app.js`, `crates/fieldbook-render/tests/frame.rs`, `crates/fieldbook-render/tests/day_n.rs`, `crates/fieldbook-render/tests/home.rs`, `xtask/harness/dayn.html`, `xtask/harness/dayn.js`, `CHANGELOG.md`.
- Registration lines: `xtask/src/harness.rs` (the `DAYN_PAGES` group, `prepare_dayn_page` and one `run_case_pages` call, this card's lines only).
- Docs to update: `CHANGELOG.md` (`- Added: the Day-N chip, "Set your start date" and the today line on the Plan timeline, kept only in the browser.` under `## Unreleased`).

## Out of scope

- Any reading progress: there is none ([HANDOFF.md](../design/HANDOFF.md) section 11 Day counter). The Progress row of `PlanTimeline` stays unbuilt.
- The view pages (the card `views-and-day-n`), the `fieldbook.theme`, `wording`, `singleKeys` and `packs` keys, and a start-date field in the Corpus or in an edition.
- The `strings.rs` change and the Strings section of corpus-format (the card `day-n-strings`).

## Preconditions

- Cards merged: `day-n-strings` (the four strings `set_start_date`, `set_start_date_plan`, `day_n_of_90` and `day_n_today`, with `window.FB_STRINGS` keys of the same names; it is open), `build-frame`, `build-page-types`, `edge-harness`, `build-home-and-areas`, `views-and-day-n` (open; it runs first, since both extend the frame tests). `build-frame`, `build-page-types`, `edge-harness` and `build-home-and-areas` are merged on origin/main. Open cards that share a path, with the shared paths in parentheses: `home-area-placeholders` (`crates/fieldbook-render/src/home.rs`, `crates/fieldbook-render/templates/index.html`, `crates/fieldbook-render/tests/home.rs`) and `decision-package` (`crates/fieldbook-render/src/home.rs`, `crates/fieldbook-render/templates/index.html`). The tests live in a file of their own (`tests/day_n.rs`) and the harness group is a registration line, so the drafts `handoff-new-note` and `transfer-and-windows-drill` are not preconditions.

## Steps

1. The chip: `initDay()` (step 2) makes it with `make()`, a `<button class="chip solid day-chip" id="fb:dayBtn" type="button" hidden aria-haspopup="dialog">`, and inserts it in `.topbar-end` before the build-date link, with no text until a day can be shown and no `title`: the design gives the chip no tooltip, so none is invented. `topbar.html` does not change (Why, no snapshot changes). `app.css`: `.day-chip` with the existing chip tokens, `display: none` below 1280px (in the existing `max-width: 1279px` query; the topbar has none of its own), and `.day-chip[hidden]`.
2. `app.js`, one `initDay()` called from `initFrame()` and written to fit the 1,500 bytes: read `fieldbook.start` inside `try`, accept only `YYYY-MM-DD` that parses to a real date, compute N as whole days from `localIsoDate()` plus 1, and set the chip text to `str('day_n_of_90')` with `N` filled in (N is not capped: after Day 90 it is the real N). With no valid date, or N below 1, the chip stays `hidden`. Clicking the chip opens a small dialog holding a `<input type="date">` labelled with the frozen `str('set_start_date')` string; the dialog is a `<dialog class="dlg">` that `initDay()` makes with `make()` and appends to `body` (no template changes, Why), opened with `showModal()` as `openKeys()` does, named by `aria-labelledby` on that label, with focus in the field and one Close button reading "Close" as in the keys dialog; Esc, Close and a click on the backdrop close it and save nothing; on every close focus returns to the control that opened the dialog (the chip, or the start-prompt button of step 4, which opens it while the chip is hidden) and, when that control is now hidden, to the chip if it shows and otherwise to `main` (HANDOFF section 5, `Dialog`). Enter in the field, or a `change` that no key press in the field has preceded since the dialog opened (the browser's date picker set it), writes the key inside `try` and closes the dialog; after any other key press in the field a `change` saves nothing and waits for Enter, because Chromium and Edge fire `change` at the first digit typed in a year or a day (`0002-11-13`). There is no Save button and no label or title besides the field label and Close: the design gives none, and a card that needs a string the design does not give reports the conflict and does not invent one (PLAN.md); an emptied field removes the key; Enter on a partly typed date (`validity.badInput`) does nothing; a thrown `localStorage` shows the toast `str('not_remembered')` (never chip text) and still updates the page for this visit, the chip keeping its day. Every `localStorage` use stays inside `try`, so `storage_only_inside_try` still holds.
3. `plan_timeline.html`: the header's `<span class="meta">` keeps the unset text as its static fallback, now from a `PlanTimelineView` field that `plan.rs` sets to `fieldbook_core::strings::set_start_date_plan()`, so both includes of the part (`day_one.rs` and `types/checklist.html`) render the same bytes with no literal, and nothing else in the part changes. `initDay()` finds that span beside `#fb:plan-timeline-head` and appends to the `.tl90` an empty `<span class="tl-now" hidden>` made with `make()`, positioned from `data-axis-max`. `initDay()` sets the header to `str('day_n_today')` with N and moves the line to `N / axis_max` of the track width while N is 1 to 90. After Day 90 the header is `str('day_n_of_90')` and no line is drawn, so the header never says a line marks today when none does. `app.css`: `.tl-now` as a dashed `--color-accent` line over the track, and `.tl90 > .k, .tl90 .win { position: relative; z-index: var(--z-raised) }` so rows stay above it. Without script, or with no date, the page shows "Set a start date to mark today" and no line.
4. `index.html` and `home.rs`: at the top of the day-one list, a `<li class="tu-item" id="fb:startPrompt">` with a button reading `set_start_date`; the script marks it with a data attribute once a day can be shown, and CSS hides it from 1280px up, so below 1280px, where the chip is hidden, it stays and is the way to change the date; the button opens the same dialog. The `<li>` is `hidden` in the markup and the script shows it, so a reader without script sees no dead control.
5. The strings come only from `fieldbook_core::strings`: the templates and `app.js` hold no literal of them (the existing literals test fails for `app.js`, and the Acceptance `grep` for the templates, which that test does not scan; the unset plan header moves from the template to the `strings` accessor through `plan.rs`).
6. Tests. `tests/frame.rs`: `built_pages_hold_no_day_chip_and_no_start_date` (no built page holds `fb:dayBtn` or a `tl-now` span, which only `app.js` adds, and no page of the fixture holds a date in `fieldbook.start` form, so a build injects nothing); `app_js_adds_start_date_inside_try` (extends the existing `storage_only_inside_try` count by the new uses). `tests/day_n.rs`: `plan_timeline_header_text_comes_from_strings` (the day-one page and a checklist page show `set_start_date_plan()`, and `plan_timeline.html` holds no literal). `tests/home.rs`: `day_one_list_offers_start_prompt_hidden_until_script`. `xtask/src/harness.rs` (registration lines): a `DAYN_PAGES` group with `prepare_dayn_page` (like `prepare_shortcut_pages`: copies `strings.js` and `app.js` and loads `dayn.js`) and a call to `run_case_pages` beside the narrow page; `xtask/harness/dayn.js` runs these cases against a synthetic page that holds a top bar with the build-date link, a timeline and the day-one list: `chip_hidden_without_a_date`, `chip_shows_day_n_of_90`, `chip_opens_a_labelled_date_field`, `esc_returns_focus_to_the_chip_that_opened_it` and `esc_returns_focus_to_the_start_prompt_that_opened_it`, `changing_the_date_saves_it_shows_the_chip_and_marks_the_prompt` (the row stays shown with its data attribute, and `app.css` hides a marked row only from 1280px), `invalid_stored_date_is_ignored`, `typing_a_date_saves_nothing_until_enter` (a key press, then a `change` to `0002-11-13`: nothing is stored and the dialog stays open; Enter on `2026-11-13` stores it), `a_date_picker_change_saves_at_once` (a `change` with no key press stores the date and closes), `close_and_the_backdrop_save_nothing`, `saving_returns_focus_to_the_opener_or_its_fallback`, `plan_header_and_line_follow_the_date`, `plan_line_is_absent_after_day_90_and_the_header_says_day_n_of_90`, `storage_throwing_keeps_the_page_working_and_shows_the_not_remembered_toast`. Each case sets the clock with an injected `Date`, so no run depends on today.
7. Size and budget: `just site` and the lint must report `app.js` at or under 32,000 bytes. If the script does not fit after reuse, stop (BLOCKED below).

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test frame --test day_n --test home
cargo nextest run --locked -p xtask harness::
just browser
wc -c crates/fieldbook-render/assets/app.js
! grep -rn 'Set your start date\|Set a start date to mark today\|Day N of 90' crates/fieldbook-render/templates crates/fieldbook-render/assets/app.js
cargo nextest run --locked -p xtask literals::
just check
```

Expected: the tests pass; `just browser` runs `cargo xtask harness` and prints every case of the `dayn` page as passing; `wc -c` prints a number at or under `32000`; the guarded `grep` exits 0; the literal test passes; `just check` exits 0. Open `out/edition/index.html` in a browser at least 1280px wide, press "Set your start date" in the day-one list, type a date no later than today and press Enter, and the chip reads "Day N of 90" with N filled in.

## Negative control

In `app.js`, write `fieldbook.start` outside its `try`. `cargo nextest run --locked -p fieldbook-render --test frame storage_only_inside_try` must fail and name the line. Restore the file.

In `dayn.js`, make `esc_returns_focus_to_the_start_prompt_that_opened_it` expect the chip to take focus: `just browser` must fail and print that case id. Make `esc_returns_focus_to_the_chip_that_opened_it` expect the chip to lose focus: it must fail too. Restore it.

In `app.js`, save on every `change` (drop the key-press check): `just browser` must fail and print `typing_a_date_saves_nothing_until_enter`. Restore it.

## Rollback

Revert the merge commit. The top bar loses the chip, the Plan timeline returns to its static "Set a start date to mark today" header, and a stored `fieldbook.start` is left unread in readers' browsers (it holds only a date they typed).

## BLOCKED

`day-n-strings` is not merged, so `fieldbook_core::strings` lacks the four keys: set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Also stop if `app.js` cannot stay at or under 32,000 bytes with the new code: the owner then decides whether a separate script file takes a budget row in `lint.rs` and a sentence in FB0605.

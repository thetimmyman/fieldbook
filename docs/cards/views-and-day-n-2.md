# views-and-day-n-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A reader can enter a start date in the browser; the top bar then shows a "Day N of 90" chip, the Today day-one list offers "Set your start date" until a date is set, and the Plan timeline draws a dashed line for today. The date lives only in `localStorage` under `fieldbook.start`, is never injected at build, and a build that reads no clock stays byte-identical.

## Why

- The date is browser-only: NFR-D-03 says the build never reads the clock, the Controls row WB-04 covers browser storage, and [data handling](../intake/data-handling.md) already lists the Day-N start date as a `localStorage` value. The frozen strings are in the HANDOFF.md frozen-strings table (it lists "Set your start date", "Set a start date to mark today", "Day N of 90" and "Not remembered in this browser").
- Design: the [MAP.md](../design/MAP.md) rows `DayChip` with "Set your start date" (header on every page, `index.html`, `app.js`) and `#plan` (the today line). [HANDOFF.md](../design/HANDOFF.md) section 5 (`DayChip`: unset, set, not remembered; `.day-chip` is a button that opens a labelled `<input type="date">`, Esc cancels and returns focus, hidden below 1280px and hidden until a date is set; `PlanTimeline`: a dashed accent line marks today, the header reads "Day N of 90 · the line marks today" or "Set a start date to mark today", and `--z-raised` keeps rows above the line), section 6 States and interactions, section 8 Responsive behaviour and section 11 Day counter. Token groups: colour (`--color-accent`), type, space, layer (`--z-raised`).
- The frozen strings must come from `fieldbook-core::strings` and appear in the Strings section of [corpus-format.md](../reference/corpus-format.md#strings); only `not_remembered` does today. The design-revision card `day-n-strings` adds the other four, so this card has no literal of its own.
- Lead decision (the repository is silent): the script goes into `assets/app.js`, not a new file. `app.js` has 32,000 bytes of budget (FB0605) and holds 30,447, so the new code must add no more than 1,500 bytes; `localIsoDate()` and `str()` are reused, not written again.

## Scope

- Files: `crates/fieldbook-render/templates/frame/topbar.html`, `crates/fieldbook-render/templates/parts/plan_timeline.html`, `crates/fieldbook-render/templates/index.html`, `crates/fieldbook-render/src/day_one.rs`, `crates/fieldbook-render/src/home.rs`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/assets/app.js`, `crates/fieldbook-render/tests/frame.rs`, `crates/fieldbook-render/tests/day_n.rs`, `crates/fieldbook-render/tests/home.rs`, `xtask/harness/dayn.html`, `xtask/harness/dayn.js`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/frame.rs` (the topbar view fields the new template reads only); `xtask/src/harness.rs` (the `DAYN_PAGES` group, `prepare_dayn_page` and one `run_case_pages` call, this card's lines only).
- Docs to update: `CHANGELOG.md` (`- Added: the Day-N chip, "Set your start date" and the today line on the Plan timeline, kept only in the browser.` under `## Unreleased`).

## Out of scope

- Any reading progress: there is none ([HANDOFF.md](../design/HANDOFF.md) section 11 Day counter). The Progress row of `PlanTimeline` stays unbuilt.
- The view pages (the card `views-and-day-n`), the `fieldbook.theme`, `wording`, `singleKeys` and `packs` keys, and a start-date field in the Corpus or in an edition.
- The `strings.rs` change and the Strings section of corpus-format (the card `day-n-strings`).

## Preconditions

- Cards merged: `day-n-strings` (the four strings `set_start_date`, `set_start_date_plan`, `day_n_of_90` and `day_n_today`, with `window.FB_STRINGS` keys of the same names; it is open), `build-frame`, `build-page-types`, `edge-harness`, `build-home-and-areas`, `views-and-day-n` (open; it runs first, since both extend the frame tests). `build-frame`, `build-page-types`, `edge-harness` and `build-home-and-areas` are merged on origin/main. Open cards that share a path, with the shared paths in parentheses: `home-area-placeholders` (`crates/fieldbook-render/src/home.rs`, `crates/fieldbook-render/templates/index.html`, `crates/fieldbook-render/tests/home.rs`) and `decision-package` (`crates/fieldbook-render/src/home.rs`, `crates/fieldbook-render/templates/index.html`). The tests live in a file of their own (`tests/day_n.rs`) and the harness group is a registration line, so the drafts `handoff-new-note` and `transfer-and-windows-drill` are not preconditions.

## Steps

1. `topbar.html`: a `<button class="chip solid day-chip" id="fb:dayBtn" type="button" hidden aria-haspopup="dialog">` in `topbar-end`, before the build date, with no text (the script writes it) and no `title`: the design gives the chip no tooltip, so none is invented. `app.css`: `.day-chip` with the existing chip tokens, `display: none` below 1280px (the media query the topbar already uses), and `.day-chip[hidden]`.
2. `app.js`, one `initDay()` called from `initFrame()` and written to fit the 1,500 bytes: read `fieldbook.start` inside `try`, accept only `YYYY-MM-DD` that parses to a real date, compute N as whole days from `localIsoDate()` plus 1, and set the chip text to `str('day_n_of_90')` with `N` filled in. With no valid date the chip stays `hidden`. Clicking the chip opens a small dialog holding a `<input type="date">` labelled with the frozen `str('set_start_date')` string; Esc cancels and returns focus to the chip; a change of the field, or Enter in it, writes the key inside `try` and closes the dialog. There is no Save button and no other label or title: the design gives none, and a card that needs a string the design does not give reports the conflict and does not invent one (PLAN.md); a thrown `localStorage` shows the toast `str('not_remembered')` and still updates the page for this visit. Every `localStorage` use stays inside `try`, so `storage_only_inside_try` still holds.
3. `plan_timeline.html`: the header's `<span class="meta">` keeps the unset text (from the `set_start_date_plan` string, passed in by `day_one.rs`) as its static fallback and gets `id="fb:tlMeta"`; the `.tl90` gets an empty `<span class="tl-now" id="fb:tlNow" hidden></span>` positioned by the script from `data-axis-max`. `initDay()` sets the header to `str('day_n_today')` with N and moves the line to `N / axis_max` of the track width (clamped to the track; after the last day the line sits at the end). `app.css`: `.tl-now` as a dashed `--color-accent` line over the track, and `.tl90 > .k, .tl90 .win { position: relative; z-index: var(--z-raised) }` so rows stay above it. Without script, or with no date, the page shows "Set a start date to mark today" and no line.
4. `index.html` and `home.rs`: at the top of the day-one list, a `<li class="tu-item" id="fb:startPrompt">` with a button reading `set_start_date`; the script hides it once a valid date is set and the button opens the same dialog. The `<li>` is `hidden` in the markup and the script shows it, so a reader without script sees no dead control.
5. The strings come only from `fieldbook_core::strings`: the templates and `app.js` hold no literal of them (the existing literals test fails otherwise; the unset plan header moves from the template to the `strings` accessor in `day_one.rs`).
6. Tests. `tests/frame.rs`: `topbar_has_a_hidden_day_chip_and_no_start_date` (the chip is `hidden`, and no page of the fixture holds a date in `fieldbook.start` form, so a build injects nothing); `app_js_adds_start_date_inside_try` (extends the existing `storage_only_inside_try` count by the new uses). `tests/day_n.rs`: `plan_timeline_unset_text_and_now_marker_are_present_but_hidden`. `tests/home.rs`: `day_one_list_offers_start_prompt_hidden_until_script`. `xtask/src/harness.rs` (registration lines): a `DAYN_PAGES` group with `prepare_dayn_page` (like `prepare_shortcut_pages`: copies `strings.js` and `app.js` and loads `dayn.js`) and a call to `run_case_pages` beside the narrow page; `xtask/harness/dayn.js` runs these cases against a synthetic page that holds the chip, a timeline and the day-one list: `chip_hidden_without_a_date`, `chip_shows_day_n_of_90`, `chip_opens_a_labelled_date_field`, `esc_cancels_and_returns_focus`, `changing_the_date_saves_it_shows_the_chip_and_hides_the_prompt`, `invalid_stored_date_is_ignored`, `plan_header_and_line_follow_the_date`, `plan_line_clamps_at_the_end`, `storage_throwing_keeps_the_page_working_and_shows_not_remembered`. Each case sets the clock with an injected `Date`, so no run depends on today.
7. Size and budget: `just site` and the lint must report `app.js` at or under 32,000 bytes. If the script does not fit after reuse, stop (BLOCKED below).

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test frame --test day_n --test home
cargo nextest run --locked -p xtask harness::
just browser
wc -c crates/fieldbook-render/assets/app.js
! grep -rn 'Set your start date\|Day N of 90' crates/fieldbook-render/templates crates/fieldbook-render/assets/app.js
cargo nextest run --locked -p xtask literals::
just check
```

Expected: the tests pass; `just browser` runs `cargo xtask harness` and prints every case of the `dayn` page as passing; `wc -c` prints a number at or under `32000`; the guarded `grep` exits 0; the literal test passes; `just check` exits 0. Open `out/edition/index.html` in a browser, press the chip's date field, enter a date, and the chip reads "Day N of 90".

## Negative control

In `app.js`, write `fieldbook.start` outside its `try`. `cargo nextest run --locked -p fieldbook-render --test frame storage_only_inside_try` must fail and name the line. Restore the file.

In `dayn.js`, make `esc_cancels_and_returns_focus` expect the chip to lose focus: `just browser` must fail and print that case id. Restore it.

## Rollback

Revert the merge commit. The top bar loses the chip, the Plan timeline returns to its static "Set a start date to mark today" header, and a stored `fieldbook.start` is left unread in readers' browsers (it holds only a date they typed).

## BLOCKED

`day-n-strings` is not merged, so `fieldbook_core::strings` lacks the four keys: set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Also stop if `app.js` cannot stay at or under 32,000 bytes with the new code: the owner then decides whether a separate script file takes a budget row in `lint.rs` and a sentence in FB0605.

# build-about

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build` writes `about.html` and `edition.toml`, `about.html` names the build profile and states the network posture, and every command the site shows is a CommandChip built by one quoting function, whose chip strings a test parses with the program's own command-line definition.

## Why

- The contents of `about.html` are its row in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types); "unreleased build" and the source of the header's edition and date are in the same section. The fields of `edition.toml`, and the rule that `build` writes it with fixed key order and no clock reading, are in [Scope and versioning](../reference/corpus-format.md#scope-and-versioning). The AI position, what is Later and what the AI build adds, is in [AD-24](../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later), and the two builds are in [The architecture in one page](../decisions/0001-foundation.md#the-architecture-in-one-page); the labels are [Handling and labels](../reference/corpus-format.md#handling-and-labels).
- Chips: the form, quoting, tags, clipboard and the landing line are [Command chips](../reference/corpus-format.md#command-chips); the chip strings, their tags and the CI parse test are the verb map in [Strings](../reference/corpus-format.md#strings) (key `chips`). External addresses are text with a copy button ([AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types), D-20). Only `fieldbook` reads the environment ([AD-05](../decisions/0001-foundation.md#ad-05-crate-layout-and-enforcement)).
- Design: the [MAP.md](../design/MAP.md) rows "About" and "CommandChip". [HANDOFF.md](../design/HANDOFF.md) section 2 Routes, section 5 Components (CommandChip), section 6 States and interactions (copy and its fallback), section 12 Implementation notes (clipboard fallback, no browser dialogs, "unreleased build"). Token groups: colour, type (`--font-mono`), motion (`--dur-fast`).

## Scope

- Files: `crates/fieldbook-render/tests/snapshots/` (only reader-page snapshots changed by due-claim CommandChip integration), `crates/fieldbook-render/templates/{about.html,parts/chip.html}`, `crates/fieldbook-render/templates/frame/topbar.html` (the build-date element only), `crates/fieldbook-render/src/{lib.rs,about.rs,chip.rs,edition.rs}`, `crates/fieldbook-render/src/frame.rs` (the build-date element only), `crates/fieldbook-render/assets/{app.css,app.js}`, `crates/fieldbook-render/tests/about.rs`, `crates/fieldbook/src/{lib.rs,build.rs}`, `crates/fieldbook/tests/chips.rs`, `CHANGELOG.md`, `crates/fieldbook-render/src/meta.rs`.
- Registration lines: `crates/fieldbook-render/src/site.rs` (this card's paths in `planned_paths` and its `EditionInfo` call, page call and `Site` insert in `render` only); `crates/fieldbook-render/src/page.rs` and `crates/fieldbook-render/templates/page.html` (due-claim chip integration); `crates/fieldbook-render/tests/page.rs` (due-claim render cases); `crates/fieldbook/tests/build.rs` (metadata failure and unreleased delivery cases).; `crates/fieldbook/tests/build.rs` and `crates/fieldbook/tests/leak_outputs.rs` (about delivery/count assertions and surface registration only)
- Docs to update: `CHANGELOG.md` (`- Added: the about page, edition.toml and copyable command chips.` under `## Unreleased`).

## Out of scope

- The edition zips, `SHA256SUMS` and the release values in a tagged build (`edition-zips`). Here a build without `SOURCE_DATE_EPOCH` is the normal case.
- The verbs the chips name; `add` and `new` arrive in `add-new-and-edition` and `new-verbs`.

## Preconditions

- Cards merged: `build-home-and-areas`, `edition-manifest`, `build-page` (the overdue banner; redirect to its interaction part if split).

## Steps

1. `edition.rs` consumes and re-exports core EditionInfo from edition-manifest. Inside render, supply the permitted view, complete shipped-file selection, latest edition heading, BuildInfo and tool/schema versions to its pure constructor and serializer. The header and about page read that same value; no second fingerprint, metadata schema or released predicate is added. BuildInfo.profile is Silent until model-gate enables Ai. The pure metadata tests stay with their producer.
2. `about.rs` and `about.html`: the tool version, the edition (or the `unreleased_build` string), the commit, the label statement (an edition holds `public` only), status counts, stale items, unreviewed consequential claims, the build profile (silent or AI) and the AI position as text. The build profile is a `BuildProfile` value that `build.rs` passes in: this card always passes `Silent`, and the card that adds the `net` feature passes `Ai` under it. The profile line is the Short form of the canonical network-posture sentence ([Changing the network-posture sentence](README.md#changing-the-network-posture-sentence)), held as one constant in `about.rs` that quotes it exactly (the test `about_short_form_matches_the_readme` of step 7 reads the README block and fails when the two differ); an edition page says that the silent build is the only build in the edition. The AI position is text only, with no button and no link: the AI build is not in this edition, and what it would add is named as in AD-24. A list with no items shows its empty line. A static page cannot probe the later policy or connected company data sources. Its handling statement follows the build-selected store context: this public edition says that it holds `public` only; a later company view retains its company-storage and effective-label snapshot and shows the chips `fieldbook policy show` and `fieldbook sync --dry-run`, which print those three on the laptop. `render` adds `about.html` and `edition.toml` to the `Site`; in `render/src/site.rs`, add their paths to `planned_paths`, so that `planned_paths_equal_the_site_keys` holds.
3. Header. `frame.rs` takes `&EditionInfo` instead of the date alone and writes `edition NNN` before the date in the existing build-date element of the header; `topbar.html` changes in that element only. A build with no date keeps the `unreleased_build` string, as `build-v0` has it. `app.js` replaces the date with the edition's age computed from `built` and keeps the date as the text without scripts and when the computation fails. The strings come from `strings`. The header reads the EditionInfo built inside render; the fallible render signature and the two unreleased tests stay unchanged. Propagate MetadataError through the existing build handler before any output replacement or write.begin; retain `released_build_without_heading_preserves_output`.
4. `chip.rs`: the one quoting function. It takes a chip key from `strings` `chips` and the values for its placeholders, accepts only an id, a lowercase area code or a page type, and leaves text placeholders as written. It returns the command and its tag (`tag_at_home` or `tag_r2_laptop`). `parts/chip.html` renders it with `role="group"`, the command in mono, a Copy button and the tag. Where the object does not exist in the edition it renders the `not_available` line instead. An external address uses the same part without a command tag.
5. `app.js`: Copy writes the text with the clipboard API. If that is refused, it selects the command text so the reader can copy it by hand. It never reads the clipboard and opens no dialog. Without scripts the text is still selectable.
6. Reuse `tests/chips.rs` from `check-command`, including its existing `PENDING` parser test; do not recreate or reset that list. Its contract remains: for every chip in `strings` `chips`, fill each placeholder with a sample value and parse the whole command with the clap command from `fieldbook::cli` (`try_get_matches_from`; export the module from `lib.rs` if it is private), so that `build --overlay` and `audit verify` are matched by their full path. A chip whose command the clap command does not define at this merge must be in the test's `PENDING` list; run the test and list those chips by chip key. The test fails on a `PENDING` chip that now parses, so the list only shrinks, and each later verb card removes its own chips from `PENDING`.
7. Tests in `tests/about.rs`: `unreleased_without_source_date_epoch`; `edition_toml_keys_in_fixed_order`; `two_builds_give_the_same_edition_toml`; `about_short_form_matches_the_readme` (reads `docs/cards/README.md` with `include_str!`, takes the paragraph that starts `**Short.**`, and requires the constant in `about.rs` to equal it without the bold lead); `chip_refuses_free_text_in_an_id_placeholder`; `r2_chips_carry_the_r2_tag`; `about_names_the_build_profile` (the default build's page contains "silent build" and the Short form, and the about page does not say "Connected"); `about_states_public_only_and_has_no_company_control` (the public page says that the edition holds `public` only, and contains neither chip text nor any other active company control; the company snapshot and its two chips belong to the `r2-overlay` About registration, not this public test); `header_shows_edition_and_build_date_without_scripts` (the fixture built with `SOURCE_DATE_EPOCH` and a commit: the build-date element holds `edition NNN` and the date in `<time datetime>`, and neither needs `app.js`); `header_age_script_keeps_the_date_text_on_error` (reads `app.js` with `include_str!`: the code that writes the age sits inside a `try` block whose `catch` leaves the element's text as the build wrote it, as `storage_only_inside_try` of `build-frame` does for `localStorage`); `every_page_has_the_csp_meta_and_no_style_attribute`.

8. Integrate the shared CommandChip into the reader page overdue banner for each claim with a computed review date. Render `fieldbook verify <claim-id> --on YYYY-MM-DD` with the literal date placeholder and At home tag. app.js uses the viewer date to reveal only due claim rows; unknown review dates are never overdue and the deterministic build reads no clock. `page::overdue_claim_uses_verify_command_chip` checks the actual template/helper output, known and unknown dates, the literal placeholder, tag and viewer-side condition. Keep the later verify command pending in the shared parser test until its verb lands.

9. In the binary build and leak-output suites, `build_about_counts_use_the_permitted_view` runs the real build handler and checks the about routes with nonzero permitted records and a disallowed record canary. Counts, links and text reflect only the permitted view; register the assertions on their edition SURFACES row. A final-handler view bypass must fail these assertions.

10. Match the design’s About row: render only compiled profile and its static connection posture; do not consult runtime model settings or infer a live connection state. Keep existing profile tests and network-silence checks.

11. Review the due-claim reader-page snapshot changes with [Changing a golden or snapshot](README.md#changing-a-golden-or-snapshot) and HANDOFF’s CommandChip specification. Keep fixed PlannedPaths inputs, literal YYYY-MM-DD, the At home tag, unknown-date behavior and viewer-side visibility assertions. Do not alter seed goldens or suppress chip markup in snapshot tests.

12. Read metadata through the producer’s RenderContext, initially Public(EditionInfo). Keep the public About/header/edition.toml values from that one value. A later company variant supplies local counts and labels to the same presentation without requiring a public edition manifest.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test about
cargo nextest run --locked -p fieldbook-core manifest::
cargo nextest run --locked -p fieldbook --test chips
just site
grep -c 'unreleased build' out/edition/about.html
grep -c 'silent build' out/edition/about.html
! grep -L 'http-equiv="Content-Security-Policy"' out/edition/about.html | grep .
! grep -l 'style="' out/edition/about.html
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p xtask literals::
cargo nextest run --locked -p fieldbook-render page::
cargo nextest run --locked -p fieldbook --test build
cargo nextest run --locked -p fieldbook --test build --test leak_outputs
just check
```

Expected: the three test runs pass; `just site` exits 0; each `grep -c` prints at least `1`; the guarded lines exit 0; the literal test passes; `just check` exits 0 (FB0605 keeps `app.js` inside its budget).

## Negative control

Edit the constant in `about.rs` so that it no longer names the silent build: `about_names_the_build_profile` must fail. Restore it. Then edit one word of the constant: `about_short_form_matches_the_readme` must fail. Restore it. Then add a chip `fieldbook show <id>` to the `chips` table on the branch. `cargo nextest run --locked -p fieldbook --test chips` must fail and name `show`. Restore the file. Then drop `edition NNN` from the header in `frame.rs`: `header_shows_edition_and_build_date_without_scripts` must fail. Restore the file.

Remove the rendered due-claim chip: `page::overdue_claim_uses_verify_command_chip` must fail. Restore it.

## Rollback

Revert the merge commit. `about.html` and `edition.toml` are no longer built and the chips and the header fall back to what `build-v0` showed.

## BLOCKED

`corpus/changes.md` holds no `## edition-NNN` heading and `info.is_released()` is true: `EditionInfo` cannot name the edition. Set the item to Blocked with a `BLOCKED:` comment that says so, and stop; do not invent a number.

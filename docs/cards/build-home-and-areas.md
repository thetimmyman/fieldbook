# build-home-and-areas

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build` writes the home page `index.html` and one `a/<area>.html` for each of the areas in `corpus.toml`, each area with its sections, its claim coverage bar and an empty-state line in every section that has no data.

## Why

- The two outputs are the `index.html` and `a/<area>.html` rows of [Site output paths](../reference/corpus-format.md#site-output-paths) and of the page table in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types). Areas and their order come from [Areas and views](../reference/corpus-format.md#areas-and-views).
- Coverage counts claims by status; the vocabulary is [Claim vocabularies](../reference/corpus-format.md#claim-vocabularies) and the colours are HANDOFF section 7.
- Design: the [MAP.md](../design/MAP.md) rows `#today` (Hero, ThreeUp with ReuseItem and ChangeItem, NextUpRow, AreaIndex / AreaTile), `#browse` (AreaIndex, CoverageBar per area), `#areas`, and `#area-<code>`, `#area-<code>/<tab>` (WorkspaceHeader / Tabs, ThreeUp, CoverageBar). [HANDOFF.md](../design/HANDOFF.md) section 2 (Routes, Area workspaces), section 5 (those components, and Charts for the bar), section 6 States and interactions, section 8 Responsive behaviour and section 11 Edge cases. Token groups: colour (the status tokens and `--chart-neutral`), type, space, shape.

## Scope

- Files: `crates/fieldbook-render/templates/{index.html,area.html}`, `crates/fieldbook-render/src/{lib.rs,home.rs,area.rs,svg.rs}`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/home.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/site.rs` (this card's paths in `planned_paths` and its calls and `Site` inserts in `render` only).; `crates/fieldbook/tests/build.rs` and `crates/fieldbook/tests/leak_outputs.rs` (home and areas delivery/count assertions and surface registration only); `crates/fieldbook-render/src/nav.rs`, `crates/fieldbook-render/assets/app.js` and `crates/fieldbook-render/tests/frame.rs` (All areas/Browse aliases and current-item/map-parity assertions only)
- Docs to update: `CHANGELOG.md` (`- Added: the home page and the area pages.` under `## Unreleased`).

## Out of scope

- Composer, Starter and the **Ask about <area>** button (Day 30, Handoff); Continue working items (overlay records, R2 or later).
- The CPY, CPT and PGM views (Day 30, views and Day-N); the search trigger (`search-palette`); the DayChip (Day 30).
- The charts in Overview (`build-charts`). This card leaves their place to that card.

## Preconditions

- Cards merged: `build-source-pages`.

## Steps

1. `svg.rs`: one function that writes a CoverageBar as inline SVG with the track geometry of HANDOFF section 5 Charts, the five segments in the status tokens and `--chart-neutral`, `role="img"`, and an `aria-label` that gives all five counts. It writes no text inside the SVG and no `style` attribute; colour comes from classes in `app.css`. `build-charts` extends this file. Read `LabelledView::pages` and `LabelledView::claims` for coverage and areas.
2. `home.rs` and `index.html`: Hero (its h1 only), the ThreeUp panels for the whole library, the day-one question list, the briefs, and AreaIndex (with `id="areas"`, one AreaTile and CoverageBar per area). ReuseItem takes only the states HANDOFF allows; ChangeItem takes stale claims, contradicted claims, open conflicts and superseded source versions. An empty panel stays and shows its one-line reason.
3. NextUpRow renders the `Unavailable` panel from `build-source-pages`: its order rule ("what unblocks the most") is not yet decided in ADR 0001.
4. `area.rs` and `area.html`: WorkspaceHeader with the eyebrow and description, then every section of the default workspace in HANDOFF order as a `<section id="…">` under an in-page jump list. Overview holds the area's ThreeUp and CoverageBar. A section with no data shows its empty-state line from HANDOFF section 11; Processes, Module and History show `Unavailable` with the release that brings them.
5. `render` adds the home and area pages to the Site; in `render/src/site.rs`, add their paths to `planned_paths`, so that `planned_paths_equal_the_site_keys` holds. In `app.css`, add the grid and panel rules with the existing tokens. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF.
6. Tests in `tests/home.rs`: `one_area_page_per_area`; `coverage_segments_sum_to_track` (for nonzero count sets the segment widths and gaps fill the track and the label repeats all five counts; zero produces an empty track and no-claims label); `coverage_svg_has_no_text_and_no_style`; `empty_sections_show_their_line`; `index_lists_day_one_questions_and_briefs` (the fixture's day-one question and brief); `every_page_has_the_csp_meta_and_no_style_attribute`.

7. Coverage counts every eligible claim once within its area across `unchecked`, `primary_source_supported`, `contradicted`, `stale` and `unresolved`. The denominator is all five counts and the supported numerator is `primary_source_supported`. Zero claims has an empty track and explicit no-claims state. A fixture with one claim of each state shows five accounted-for claims and “1 of 5” supported; dropping either previously omitted state must fail. The interface lane must settle matching component tokens and accessible labels before implementation.

8. `index.html` has canonical Today section metadata and Today current. All areas and Library Browse are aliases to its areas anchor, added only when this card creates that anchor; their no-script destination keeps the Today frame. Activate All areas and Browse as `index.html#areas` in nav.rs and the matching app.js map when this card adds `id="areas"`. Keep the index.html PlannedPaths check; fragments do not enter PlannedPaths. Extend `current_tab_and_item_are_marked` and map-parity tests in frame.rs for both aliases and canonical Today state, including without scripts. An alias to an absent anchor or competing canonical section must fail.

9. Extend this route's real delivered-page tests to require its permitted count and to keep that count and links unchanged when forbidden pages/rows are added. Register the assertion with its edition surface. Using unrestricted model counts must fail that test.

10. In the binary build and leak-output suites, `build_home_and_areas_counts_use_the_permitted_view` runs the real build handler and checks the home and areas routes with nonzero permitted records and a disallowed record canary. Counts, links and text reflect only the permitted view; register the assertions on their edition SURFACES row. A final-handler view bypass must fail these assertions.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test home
just site
ls out/edition/a/*.html | wc -l
! grep -L 'http-equiv="Content-Security-Policy"' out/edition/index.html out/edition/a/*.html | grep .
! grep -l 'style="' out/edition/index.html out/edition/a/*.html
grep -o 'role="img"' out/edition/index.html | wc -l
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p xtask literals::
cargo nextest run --locked -p fieldbook --test build --test leak_outputs
cargo nextest run --locked -p fieldbook-render frame::
just check
```

Expected: the tests pass; `just site` exits 0; `wc -l` prints `13`; the guarded `grep` lines and the diff line exit 0; the `grep -o` count prints `13` or more (one bar per tile); the literal test passes; `just check` exits 0. The real Corpus has few pages, so `out/edition/a/mna.html` shows the empty-state lines; open it in a text viewer to see them.

## Negative control

In `svg.rs`, leave out the contradicted segment. `cargo nextest run --locked -p fieldbook-render --test home coverage_segments_sum_to_track` must fail and print the count set whose widths do not fill the track. Restore the file.

Remove `id="areas"` while retaining both alias links: the existing site-link lint must report FB0603. Restore it.

## Rollback

Revert the merge commit. `index.html` returns to what `build-v0` wrote and the area pages are no longer built; revert later cards that link to `a/` first.

## BLOCKED

The real `corpus.toml` does not hold the 13 areas when this card runs (that is the lead's content item in P1): set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Also stop if HANDOFF section 11 gives no empty-state line for a section this card renders.

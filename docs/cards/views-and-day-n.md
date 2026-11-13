# views-and-day-n

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build` writes one `a/<code>.html` workspace page for each `[[view]]` in `corpus.toml` (CPY, CPT and PGM in the real Corpus), the sidebar's view entries link to those pages and show as current on them, and a section whose data the Corpus cannot yet supply shows an `Unavailable` panel instead of a made-up table.

## Why

- The view routes are the `a/<area>.html` row of [Site output paths](../reference/corpus-format.md#site-output-paths) ("views from Day 30") and the views are defined in [Areas and views](../reference/corpus-format.md#areas-and-views): a code, a name, and a filter on one area and one subarea.
- Design: the [MAP.md](../design/MAP.md) rows `#area-<code>` (CPY, CPT, PGM), the competitor row (CPT view), and `ProgramsTable` (PGM overview). [HANDOFF.md](../design/HANDOFF.md) section 2 (Area workspaces: the eyebrow "View over MKT", the line "MKT pages in subarea <company|peers|programs>", and the section lists of the three views), section 5 (`ProgramsTable`, `CompareMatrix`, `CapLadder`), section 8 Responsive behaviour (`ProgramsTable` hides the budget line below 768px) and section 11 Edge cases. Token groups: colour, type, space, shape; no new token.
- The Corpus has no format for a program's budget line, budget stage or linked work, for a competitor's matrix cell, or for a capability stage ([PageMeta](../reference/corpus-format.md) has no such field). The design draws these as tables over data that does not exist, so this card renders the `Unavailable` panel for them, as the escalation chart does ([MAP.md](../design/MAP.md) Charts row). No design-revision card is needed: the design says what to show once the data exists, and this card leaves that place ready.

## Scope

- Files: `crates/fieldbook-render/src/area.rs`, `crates/fieldbook-render/templates/area.html`, `crates/fieldbook-render/src/frame.rs`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/views.rs`, `crates/fieldbook-render/tests/home.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/site.rs` (the view paths in `planned_paths` and the `Site` inserts in `render_with_info` only); `crates/fieldbook-render/src/nav.rs` (the view entries' targets and `view-<code>` as the current item only); `crates/fieldbook-render/tests/frame.rs` (view current-item assertions only); `crates/fieldbook/tests/build.rs` (view page count assertion only, if one counts `a/` pages).
- Docs to update: `CHANGELOG.md` (`- Added: the Company profile, Competitors and Programs view pages.` under `## Unreleased`).

## Out of scope

- The DayChip, "Set your start date", the today line on `PlanTimeline` and the start date in `localStorage` (the card `views-and-day-n-2`).
- Real `ProgramsTable`, `CompareMatrix` and `CapLadder` rows: they need a Corpus format for budget lines, matrix cells and capability stages, and a card for it.
- Any change to `corpus.toml`, to `corpus-format.md`, to the allowlist (it already admits `a/<name>.html`) or to the harness (it opens every built page).
- Processes, Procedures, Module and History records (Release 3); this card shows the same `Unavailable` lines the area pages show.

## Preconditions

- Cards merged: `build-home-and-areas`, `build-charts` (it extends `area.rs`), `build-frame`, `edge-harness`, `home-area-placeholders` (open; shared paths `crates/fieldbook-render/src/area.rs`, `crates/fieldbook-render/tests/home.rs`), `frame-page-id-hook` (open; shared paths `crates/fieldbook-render/src/frame.rs`, `crates/fieldbook-render/tests/frame.rs`; it runs first). The first four are merged on origin/main. Do the placeholder wording changes in `area.rs` after `home-area-placeholders`, and keep this card's edits to the lines it names.

## Steps

1. `area.rs`: add `view_pages(view, def: &ViewDef) -> Vec<&PageEntry>` (pages whose area and subarea equal the filter, case-insensitively, ordered by stage then id), `view_page_paths(view)` (`page_path(&v.code)` for each `corpus.view`) and `render_view_page`, which builds the page through the same `render_frame_page` call as `render_page`, with crumbs `Areas / <name>`. A view code never equals an area code ([Areas and views](../reference/corpus-format.md#areas-and-views)); if they ever did, `build` already stops on a duplicate output path.
2. `templates/area.html`: one template for both. For a view the eyebrow is `View over <AREA>` (the filter's area code in capitals), the line under the description is `<AREA> pages in subarea <subarea>` with the same code (`MKT pages in subarea peers` on the CPT view, as HANDOFF section 2 writes it, and `EST pages in subarea pricing` on the fixture's PRC view), and the sections are the design's, in its order: default and PGM `Overview, Processes, Procedures, Templates, Module, Library pages, History, Questions`; CPY `Background, Organization, Processes, Procedures, Templates, Systems, History, Questions`; CPT `Compare, Profiles, Moves, Watch process, Templates, Library pages, Sources, Questions`. Each is a `<section class="ws-section" id="…">` under the in-page jump list.
3. Section content, by this rule: Library pages and Templates list the view's own pages (the same row as the area page); Questions lists the questions of those pages; Sources lists the sources they cite; Profiles lists the view's `Reference` and `Concept` pages. The leading table of PGM Overview (`ProgramsTable`), CPT Compare (`CompareMatrix`) and the CPY Background / Organization / Systems sections, CPT Moves and Watch process, and the Processes, Procedures, Module and History sections call `render_unavailable(thing, until)` with "a later release" as `until`, naming the thing in the design's words ("The programs table", "The comparison matrix"). A section with pages but no rows shows its HANDOFF section 11 empty-state line. `CapLadder` is not rendered: it has no section in a built view.
4. `site.rs`: add `area::view_page_paths(view)` to `planned_paths` and insert each view page in `render_with_info`, so `planned_paths_equal_the_site_keys` holds.
5. `nav.rs`: the view entries take `Some("a/<code>.html")` as their target path instead of `None` and a "Day 30" candidate, so they link and the unbuilt-section notice goes. `frame.rs`: when `input.nav.entry("view-<stem>")` exists for `a/<stem>.html`, that entry is the current item; otherwise `resolve_route` decides as now. `route_sections()` and `ROUTE_SECTIONS` in `app.js` are unchanged (a view page is in Areas), and `section_map_matches_nav_rs` must still pass.
6. `app.css`: one rule for the `.view-note` line under the description, and none for the tables (they are not built). Use only existing tokens; a value no token covers is a gap: use the nearest token and record the gap on the card's work item.
7. Tests in `tests/views.rs`, against the fixture's one view (PRC, filter est/pricing, 4 pages) and trees built with `tree_with` for the MKT cases (synthetic pages only; no real Corpus page is copied): `one_page_per_view`; `view_eyebrow_and_note_follow_the_filter`; `view_sections_follow_the_design_order` (default list for PRC, and the CPY and CPT lists for synthetic views); `view_lists_only_its_own_pages` (a page in est/other is absent); `unbuilt_tables_show_unavailable_not_rows`; `sidebar_view_entry_links_and_is_current` (the entry on the view page has `aria-current` and an href to `a/prc.html`, and it is not current on `a/est.html`); `view_page_has_the_csp_meta_and_no_style_attribute`. In `tests/home.rs`, change `one_area_page_per_area` to expect `a/prc.html` after `a/mna.html`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test views --test home
cargo nextest run --locked -p fieldbook-render frame::
just site
ls out/edition/a/*.html | wc -l
grep -c 'View over MKT' out/edition/a/cpy.html out/edition/a/cpt.html out/edition/a/pgm.html
! grep -l 'style="' out/edition/a/*.html
just check
```

Expected: the tests pass; `just site` exits 0; `wc -l` prints the number of areas plus the number of views (16 with the real Corpus); each `grep -c` prints `<file>:1`, one line per file; the guarded `grep` exits 0; `just check` exits 0. Open `out/edition/a/pgm.html` in a text viewer: the Overview shows the "programs table" panel and the SBA 8(a) clause cards under Library pages.

## Negative control

In `area.rs`, drop the subarea test from `view_pages`. `cargo nextest run --locked -p fieldbook-render --test views view_lists_only_its_own_pages` must fail and print the page id from the other subarea. Restore the file.

In `nav.rs`, set a view entry's target back to `None`. `sidebar_view_entry_links_and_is_current` must fail. Restore it.

## Rollback

Revert the merge commit. The sidebar's view entries return to the unbuilt notice and the view pages are no longer built; revert `views-and-day-n-2` first if it merged, since it extends `area.rs` only by reading these pages.

## BLOCKED

The real `corpus.toml` holds no `[[view]]` for CPY, CPT or PGM when this card runs: set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Also stop if HANDOFF section 2 no longer lists the sections of a view, or if a view's filter names an area that is not in `corpus.toml`.

# build-frame

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every built page carries the global frame: the shell with its skip link, the top bar with the six section tabs, the sidebar with the current section's links and its foot, the crumbs row where a route has two levels, the theme toggle, and the base components (Button, Chip, Field, Panel, Eyebrow, Segmented, Box, Kbd and Dialog) with their rules in `app.css`. A page shows its section and sidebar group with scripts blocked; `app.js` adds the overlay sidebar, the section switch and the stored theme. A link is written only to a file this build writes; any other entry shows its landing line.

## Why

- Static pages that work without JS, the JS budgets and storage only inside try/catch are in [AD-15](../decisions/0001-foundation.md#ad-15-ui-technology-and-design-system). Links, the header and the CSP are in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types). The landing line is the `not_available` string and the theme message is `not_remembered`, both in [Strings](../reference/corpus-format.md#strings).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) row "AppShell, TopBar, SectionTabs, BuildChip, CrumbsRow, Sidebar, ContextNav, SectionSwitch, NavItem, Banner, Toast, Switch, theme toggle" and the row "Base components: Button, Chip, Field, Panel, Eyebrow, Segmented, Box, Kbd, Dialog". [HANDOFF.md](../design/HANDOFF.md) [§2 Global frame](../design/HANDOFF.md#global-frame), [Routes](../design/HANDOFF.md#routes) and Where each object lands; [§3 Layout](../design/HANDOFF.md#3-layout) (Frame); [§5 Components](../design/HANDOFF.md#5-components) (those rows); [§6 States and interactions](../design/HANDOFF.md#6-states-and-interactions) (theme toggle); [§8 Responsive behaviour](../design/HANDOFF.md#8-responsive-behaviour); [§11 Edge cases](../design/HANDOFF.md#11-edge-cases) (storage blocked, unreleased build); [§12 Implementation notes](../design/HANDOFF.md#12-implementation-notes) (Sections, Storage, Dates). Token groups: colour, type, space, shape and line, size, motion, layering and elevation.

## Scope

- Files: `crates/fieldbook-render/templates/base.html`, `crates/fieldbook-render/templates/frame/{topbar.html,sidebar.html,crumbs.html}`, `crates/fieldbook-render/src/{lib.rs,site.rs,nav.rs,frame.rs}`, `crates/fieldbook-render/assets/{app.css,app.js}`, `crates/fieldbook-render/tests/frame.rs`, `CHANGELOG.md`
- Registration lines: `crates/fieldbook/tests/leak_outputs.rs` (edition navigation counts); `crates/fieldbook/tests/build.rs` (navigation noninterference cases only)
- Docs to update: `CHANGELOG.md` (`- Added: the page frame with section tabs, sidebar and theme toggle.` under `## Unreleased`).

## Out of scope

- SearchTrigger, CommandPalette and ShortcutsDialog (`search-palette`, `search-js`). The sidebar foot's shortcuts link waits for them. The Skeleton bar (`search-js`).
- DayChip and the start date (Day 30). The Ask button and drawers (Day 30, Handoff).
- The edition name and age in the header (`build-about`). This card keeps the build date or the `unreleased_build` string from `build-v0`.
- Page bodies (`build-page` and later). New tokens (`build-tokens` owns them).

## Preconditions

- Cards merged: `build-tokens`.

## Steps

1. `nav.rs`: the six sections of HANDOFF §2 Global frame, each with its entries in order, and the route-to-section map from each AD-12 output path to its section and sidebar entry. Each entry has its label and either an output path or its landing (release or trigger) from HANDOFF Routes and Where each object lands. Counts come from `LabelledView`; an entry without a count shows none.
2. `site.rs` gets `planned_paths(&LabelledView) -> PlannedPaths`: the paths this build writes. `PlannedPaths` is a set of output paths with `contains`, `iter` and `PlannedPaths::of(&[&str])`, which builds a set by hand for a test. Each later `build-*` card that adds a file to the `Site` adds its path to `planned_paths` in the same step. `nav.rs` and `links.rs` take that set and no other. An entry links only to a path in it. Any other entry, and any tab whose first page is not built, renders as text with the `not_available` line filled from its landing, in an element with the class `not-available`. This keeps FB0603 clean while pages arrive card by card.
3. `frame.rs` and the templates: AppShell with the skip link to `<main>`; TopBar with SectionTabs (`<nav aria-label="Sections">`, `aria-current="true"` on the current tab), BuildChip and the theme Switch; Sidebar (`<aside aria-label="Navigation">`) with ContextNav, NavItem (`aria-current="page"`) and the foot; CrumbsRow only for two or more levels; a Banner and a Toast region with a live region, empty until a card fills them. Asset and link paths are relative to the page's depth.
4. `app.css`: frame layout, the rules of the base components and the component rules from HANDOFF §3, §5 and §8, using tokens. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF. No colour literal.
5. `app.js` (a classic ES2020 script, no module): the overlay sidebar below 960px with SectionSwitch, focus moving and Escape closing; the theme toggle, which sets `data-theme` and keeps the choice in localStorage under the key `fieldbook.theme` inside try/catch, and on failure keeps it for the session and shows `FB_STRINGS.not_remembered`; with no choice, the system theme. The route-to-section map is one object in `app.js`. No `alert`, `confirm`, `eval` or inline handler.
6. Tests in `tests/frame.rs`, `mod frame`: `section_map_matches_nav_rs` (reads `app.js` with `include_str!` and compares its map with `nav.rs`); `planned_paths_equal_the_site_keys` (every key of `render(..)` is in `planned_paths`, and every planned path is a key); `links_only_to_built_paths`; `unbuilt_entries_show_the_landing_line`; `current_tab_and_item_are_marked`; `crumbs_only_with_two_levels`; `relative_paths_at_each_depth`; `no_inline_handler_or_style_attribute`; `storage_only_inside_try` (every `localStorage` in `app.js` sits inside a `try` block).

7. Add `navigation_counts_use_the_filtered_view` in `tests/frame.rs` over the real navigation-data builder: permitted counts are correct and forbidden pages change neither counts nor available destinations. The real build handler tests shell delivery and unavailable links only. Page title/body and emitted page links belong to build-page; rendered counts are tested by the first route that displays them. Register only these currently available shell assertions with the edition surface.

8. Treat `index.html` canonically as Today, with Today current. All areas and Browse become aliases to its areas anchor only after `build-home-and-areas` creates it. Until then suppress those unavailable fragment links. Route/current-item tests pin canonical metadata and staged alias availability; the interface lane supplies the matching route specification.

9. Give the shared frame an explicit public/company store context and effective-label presentation input as HANDOFF specifies. This card supplies the public-edition context; the overlay producer later supplies the company view. Never infer context from an item label or hardcode public-only assertions inside the reusable component. The existing frame test pins the public text and scripts-blocked behavior.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render frame::
cargo nextest run --locked -p xtask literals::
just site
test "$(grep -c 'http-equiv="Content-Security-Policy"' out/edition/index.html)" = 1
grep -c '<nav aria-label="Sections">' out/edition/index.html
grep -c 'class="not-available"' out/edition/index.html
! grep -rl -E 'style="| on[a-z]+=' out/edition
! grep -n -E '\b(alert|confirm|eval)\(' crates/fieldbook-render/assets/app.js
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p fieldbook --test build --test leak_outputs
just check
```

Expected: the tests pass and the literal test finds no retyped frozen string in `app.js`; `just site` exits 0; the CSP count is 1; the two `grep -c` lines print at least `1`; the guarded lines exit 0; `just check` exits 0.

## Negative control

Delete one route from the map in `app.js`. `frame::section_map_matches_nav_rs` must fail and name the route. Restore the file. Then make an unbuilt entry a link: `frame::links_only_to_built_paths` must fail and name the path. Restore the file.

Build navigation counts from the unrestricted model: `navigation_counts_use_the_filtered_view` must fail. Restore it.

## Rollback

Revert the merge commit. Pages lose the frame and keep the `build-v0` shell. Revert every later `build-*` card first.

## BLOCKED

- HANDOFF does not say where a sidebar entry or a section tab lands, so no landing line can be filled: name the entry and ask the lead.
- `app.js` cannot apply the stored theme before first paint without an inline script, which the CSP refuses: write what the page shows and ask the lead. Do not add a hash or relax the CSP.

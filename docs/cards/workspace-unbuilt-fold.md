# workspace-unbuilt-fold

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

On every workspace page (the areas and the views), a section whose object has not landed leaves the section sequence and the jump list and sits, with its landing line, in one closed "Not built yet" disclosure after the last built section, so a reader reaches the library pages without scrolling past four "is not available until" sections.

## Why

Four "is not available until" sections pushed the area's library pages below the first screen. Only whole sections fold; a section that is built but empty keeps its place, and the Overview's inner panels keep their own notes.

- Design: [HANDOFF.md](../design/HANDOFF.md) revision 26, section 0 (Fixed template text) and section 2 (Area workspaces); [MAP.md](../design/MAP.md) rows `#area-<code>` and `#area-<code>/<tab>` (WorkspaceHeader / Tabs), and the views' pages; component `Fold` in its sidebar "Not built yet" form. Token groups: Type, Space and Border; no token is added, and the summary reuses the sidebar disclosure's rules.

## Scope

- Files: `crates/fieldbook-render/src/area.rs`, `crates/fieldbook-render/templates/area.html`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/views.rs`, `crates/fieldbook-render/tests/home.rs`, `CHANGELOG.md`.
- Registration lines: none.
- Docs to update: the changelog. The plan line came with `design-workspace-opening`.

## Out of scope

- The sidebar's "Not built yet" disclosure and its lines, which are unchanged.
- The Overview's inner panels (the module card, the templates panel and the company series beside a chart), which keep their in-place notes.
- The landing lines' wording, the section order and the releases that bring each object.
- The design files and the mockup: `design-workspace-opening` brings them.

## Preconditions

- Cards merged: `design-workspace-opening` (design revision 26), `changes-empty-by-check` (both cards edit `crates/fieldbook-render/tests/home.rs`).

## Steps

1. Pin the behaviour at the render interface in `crates/fieldbook-render/tests/views.rs`, module `views::unbuilt_fold`: for an area, the Pricing view and the Company profile, Competitors and Programs views, the built sections in table order, the jump list without the unbuilt ids and ending with the "Not built yet" link and its count, the closed disclosure with each landing line once under its old id, no unavailable note in a built section, a built empty section kept in place, and Templates landing in its place when a template page exists. Run them red.
2. In `crates/fieldbook-render/src/area.rs`, give each workspace section either a built body or a landing line, and split a workspace into its built sections and its unbuilt ones, both in table order. An empty Templates section is unbuilt until the template list lands.
3. In `crates/fieldbook-render/templates/area.html`, list only the built sections in the jump list and the sequence, end the jump list with the "Not built yet" link, and write the disclosure after the last section in the markup of HANDOFF revision 26, only when a section is unbuilt.
4. In `crates/fieldbook-render/assets/app.css`, give the disclosure's summary the sidebar disclosure's rules (36px tall, the chevron, the focus ring), and add `.ws-sub` and `.ws-line`, with tokens only.
5. Add `area::tests::` for a workspace with nothing unbuilt (no disclosure, no jump entry) and for a section that lands (it returns to its place).
6. Update the assertions that expected the unbuilt sections in place: `home::one_area_page_per_area` and `home::empty_sections_show_their_line` in `tests/home.rs`, and `views::view_sections_follow_the_design_order` and `views::unbuilt_tables_show_unavailable_not_rows` in `tests/views.rs`.
7. Add the changelog line.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test views views::
cargo nextest run --locked -p fieldbook-render --test home home::
cargo nextest run --locked -p fieldbook-render area::tests::
cargo nextest run --locked -p fieldbook-render --test tokens
just check
```

Expected: each command exits 0; `views::unbuilt_fold::` runs five tests, all passing. A workspace page lists no unbuilt section in its jump list or its sequence, carries one closed `<details class="fold">` holding each landing line once, and the token tests still pass with no literal colour or size added.

## Negative control

On the branch, make `split` in `area.rs` place an unbuilt entry in the section sequence as a section holding its landing line. `cargo nextest run --locked -p fieldbook-render --test views views::unbuilt_fold::` must then fail four of its five tests, the first with the section list `["overview", "processes", "procedures", "templates", "module", "library-pages", "history", "questions"]` where `["overview", "templates", "library-pages", "history", "questions"]` was expected. Restore the file.

## Rollback

Revert the merge commit. Every workspace page again places each unbuilt section in the sequence with its landing line, and links it from the jump list.

## BLOCKED

The design files do not carry the disclosure's markup (`design-workspace-opening` is not merged): comment `BLOCKED: workspace-unbuilt-fold waits on design revision 26`.

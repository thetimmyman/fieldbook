# build-registers

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`build` writes `claims.html`, `sources.html`, `questions.html`, `conflicts.html` and `glossary.html` from one generic register template, with one anchor per row named by the row's id, and `ids.html`, which lists every id with its title and link. The chips that `build-page` wrote as text now link to these rows. An empty register shows one line, never a bare zero.

## Why

- The five register pages, "one generic template, an anchor per row" and `ids.html` are in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) and [Site output paths](../reference/corpus-format.md#site-output-paths). The anchor of each row is in [Identifiers](../reference/corpus-format.md#identifiers). The columns and their order are in [Registers](../reference/corpus-format.md#registers). The status words come from [Status vocabularies and promotion rules](../reference/corpus-format.md#status-vocabularies-and-promotion-rules).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) rows `#claims`, `#questions`, `#glossary`, `#sources` (RegisterTable, StatusChip), `#conflicts`, `#conflicts-<id>`, "Id list (no mockup route)" and CitationChip. [HANDOFF.md](../design/HANDOFF.md) [§2 Routes](../design/HANDOFF.md#routes); [§5 Components](../design/HANDOFF.md#5-components) (RegisterTable, StatusChip, CitationChip, RefChip, ClaimMark, ConflictBox in its register variant); [§6 States and interactions](../design/HANDOFF.md#6-states-and-interactions) (Claims filter); [§7 Status display rules](../design/HANDOFF.md#7-status-display-rules); [§11 Edge cases](../design/HANDOFF.md#11-edge-cases) (empty register filter, 100+ results, long ids). Token groups: colour (the status tokens, `--color-mark`), type (mono for ids), space, shape.

## Scope

- Files: `crates/fieldbook-render/templates/{register.html,ids.html}`, `crates/fieldbook-render/src/{lib.rs,site.rs,register.rs,ids.rs,status_chip.rs}`, `crates/fieldbook-render/assets/{app.css,app.js}`, `crates/fieldbook-render/tests/registers.rs`, `crates/fieldbook-render/tests/snapshots/` (new snapshots), `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/links.rs` (source-id selection and lifecycle eligibility).; `crates/fieldbook/tests/build.rs` and `crates/fieldbook/tests/leak_outputs.rs` (registers and ids delivery/count assertions and surface registration only)
- Docs to update: `CHANGELOG.md` (`- Added: the five register pages and the id list.` under `## Unreleased`).

## Out of scope

- `data/registers.xlsx` and the CSV copies (`registers-xlsx`). The source pages `s/` (`build-source-pages`).
- `search.html` and the palette (`search-palette`). Command chips (`build-about`).
- Overlay columns (`answer`, `answered_on`, `adoption`): empty in the Corpus and not shown.

## Preconditions

- Cards merged: `build-page-types`.

## Steps

1. `register.rs`: one `RegisterView` (caption, columns, rows) per register, made through `LabelledView::claims`, `sources`, `questions`, `conflicts` and `glossary`, in the register's sort order. `register.html` is the one template for all five: a real `<table>` with a caption, a non-sticky header with `<th scope="col">`, its own horizontal scroll wrapper, and `id="<row id>"` on each row as Identifiers gives it. `sources.html` uses the version id as the row anchor.
2. Cells: an id cell is a mono chip that links through `links.rs` (a claim to `claims.html#<id>`, a version, only if not quarantined, to `s/<version-id>.html` once planned and to its `sources.html` row before then; a source id to the first non-quarantined version's sources.html anchor in numeric version order, a question or conflict to its row, a page to `p/<page-id>.html`). A list cell renders one chip per value. An enum field uses the presentation explicitly assigned in HANDOFF §7. Use StatusChip only where prescribed; all other enum values render as escaped canonical text with no inferred status colour or icon. An empty cell shows what the column's "Empty means" says, in muted text.
3. `conflicts.html` renders each row as a ConflictBox in its register variant. Unresolved rows keep every position and name the settling question. No row shows a winner or an average.
4. An empty register, or an empty claims filter, shows its HANDOFF §11 line. `app.js` adds the sortable header buttons and the claims filter; without scripts the table is complete and sorted by id.
5. `ids.rs` and `ids.html`: every page, source, version, claim, question, conflict and term id, sorted, each with its title or label and its link from step 2.
6. `site.rs`: add the six files to the `Site` and their paths to `site::planned_paths`. The `build-page` chips now link, since their targets are built.
7. `app.css`: table, chip and filter rules with the existing tokens. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF.
8. Tests in `tests/registers.rs`, `mod registers`: `one_template_for_five_registers`; `row_anchor_is_the_id` (for each register, against Identifiers); `anchors_are_unique_per_page` (a synthetic register of three rows); `version_links_to_sources_row_until_s_page_is_built`; `empty_register_shows_its_line`; `unresolved_conflict_keeps_every_position`; `ids_lists_every_id_once`; `status_chip_has_icon_and_word`; `chips_link_once_their_targets_are_planned` (render a fixture page that cites a claim with `links` and a fixed `PlannedPaths::of(...)` that includes `claims.html`: its claim chips link to `claims.html#<id>`; with the same set minus `claims.html` they are plain text). Golden renders with `insta`: `claims_page_renders` and `ids_page_renders` from the fixture. The snapshot tests call `links` and `nav` with a fixed `PlannedPaths::of(...)` (each `p/<page-id>.html` of the fixture plus `claims.html`, `sources.html`, `questions.html`, `conflicts.html`, `glossary.html` and `ids.html`) and never read `site::planned_paths`, so a later card that registers a path cannot change them.

9. Check lifecycle eligibility in links.rs before choosing either destination: a held version is always plain text, while its register metadata remains. A source with no eligible version is also text. Apply the same source/version helper to cells and ids.html. Tests `enum_presentations_are_explicit` cover primary_law, primary_evidence and pdf_page_box as text and a mapped verification chip; `source_ids_choose_first_eligible_version` covers several versions, a held first version and all held; `held_links_have_no_register_fallback` covers citations, cells and ids with sources.html present, alongside an active linked version. Every emitted link must pass FB0603.

10. Keep the local horizontal scroll wrapper. Script-added sort buttons set `aria-sort` on the sorted column header, as RegisterTable specifies. The existing register render and script tests assert non-sticky headers, column scope and that sort state.

11. Extend this route's real delivered-page tests to require its permitted count and to keep that count and links unchanged when forbidden pages/rows are added. Register the assertion with its edition surface. Using unrestricted model counts must fail that test.

12. In the binary build and leak-output suites, `build_registers_counts_use_the_permitted_view` runs the real build handler and checks the registers and ids routes with nonzero permitted records and a disallowed record canary. Counts, links and text reflect only the permitted view; register the assertions on their edition SURFACES row. A final-handler view bypass must fail these assertions.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render registers::
cargo nextest run --locked -p fieldbook build::
cargo nextest run --locked -p xtask literals::
just site
ls out/edition/claims.html out/edition/sources.html out/edition/questions.html out/edition/conflicts.html out/edition/glossary.html out/edition/ids.html
! grep -L 'http-equiv="Content-Security-Policy"' out/edition/*.html | grep .
! grep -l -E 'style="| on[a-z]+=' out/edition/*.html
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p fieldbook --test build --test leak_outputs
just check
```

Expected: the tests pass, both snapshots included; `just site` exits 0 and the six files exist; the guarded lines exit 0, and the build's FB0603 lint passes with the chips now linked; `just check` exits 0.

## Negative control

In `register.rs`, write the same fixed anchor on every row. `registers::anchors_are_unique_per_page` and `registers::row_anchor_is_the_id` must fail and name the register. Restore the file. Then render an empty register with no rows and no line: `registers::empty_register_shows_its_line` must fail. Restore the file.

Colour an unmapped enum or restore held-version register links: `enum_presentations_are_explicit` or `held_links_have_no_register_fallback` must fail. Select the held first version for a source id: `source_ids_choose_first_eligible_version` must fail. Restore each.

## Rollback

Revert the merge commit. The register pages and `ids.html` go, and the chips fall back to text. Revert `build-source-pages`, `registers-xlsx` and every later card that links to a register first.

## BLOCKED

- HANDOFF §7 gives no colour or icon for a value in a column explicitly designated for StatusChip presentation: name the value and ask the lead. Do not pick one.
- A register id cannot be an HTML `id` as Identifiers gives it: name the id form and ask the lead.

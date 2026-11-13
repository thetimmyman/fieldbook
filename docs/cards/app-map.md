# app-map

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`map.html`, the App map, is part of the edition. It shows how Fieldbook fits together as five layer bands (Outputs, Work core, Workspaces, Modules, Library), each a set of link pills, and an Assistant column that names the adapters and their states as compiled-in text. A pill links only to a page the build plans; any other pill is listed in the band's closed "Not built yet" fold with a one-line landing sentence. The Today nav entry "App map" becomes a link, the page is delivered to the edition folder by the existing edition sink, and it contains no model call, no script, no inline style and no network request.

## Why

- The App map is the Day 30 row of the output table in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) ("The App map: layers, releases, stores and labels; the AI layer as text"). Its path `map.html` is already frozen as `strings::paths()[5]`, so no path or schema decision is made here. The page is derived from the corpus view and compiled-in data, so no count or list is copied by hand.
- Design: [MAP.md](../design/MAP.md) row `#map` (component `AppMap`, route `map.html`), the Today-section nav entry "Today, App map", and the Workspaces and Modules bands that the row names; [HANDOFF.md](../design/HANDOFF.md) section 2 Routes (`#map`), section 5 Components (`AppMap`, with `Unavailable`, `Fold`, `Panel`, `Chip` and `Button` that it reuses), and the section 12 and 13 checks (single column at 900px, links grouped under band headings); tokens: the space, shape and line, size and colour groups, with `.appmap` as the component root in `assets/app.css` ([Using the tokens](../design/HANDOFF.md)). The mockup's `V.map` and its `.appmap`, `.layers`, `.layer`, `.pill` and `.asst-col` rules are the visual source.
- Two things in the mockup are not yet settled in the design documents, so the first Precondition is a design revision and not a choice made here: its Releases rows use release names that differ from the [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) release table, and its Stores and effective labels panel does not use the four built-in labels and the stores of [ADR 0004](../decisions/0004-company-boundary.md). A third gap, that HANDOFF gives `AppMap` only a Static state so a pill to an unbuilt page has no stated look, is a lead decision of this card and not a revision string: revision 22's `frame-unbuilt-entries` convention (a closed "Not built yet" disclosure with one landing line per item) is reused, and the revision does not need to give it.
- The AI layer is text because `fieldbook-render` is pure: it cannot read the adapter table, which lives in `crates/fieldbook/src/adapters/registry.rs` in the `fieldbook` crate (the `ai-kernel` card has merged, as #404, and built it there). HANDOFF says builds show Handoff and Off only, and that Connected is text on About and the App map until D-1. The Assistant column is therefore a small compiled-in table that this card keeps; a later card may replace it with generated rows if the registry's data is handed to the renderer.
- Delivery follows the existing edition sink and the real-handler surface rule of [ADR 0004](../decisions/0004-company-boundary.md): the map carries only public text, and a planted canary in its planned bytes must make `write_edition_outputs` return `Error::OutOfScope`. The page carries no company data, so it adds no new sink.

## Scope

- Files: `crates/fieldbook-render/src/map.rs`, `crates/fieldbook-render/templates/map.html`, `crates/fieldbook-render/tests/map.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/lib.rs` (the `pub mod map;` line); `crates/fieldbook-render/src/site.rs` (one `planned_paths` entry and one `site.insert` call for `map::OUTPUT_PATH`, this card's entries only); `crates/fieldbook-render/assets/app.css` (the `.appmap`, `.layers`, `.layer`, `.pill`, `.asst-col` and 900px stacking rules, written with tokens, this card's block only); `crates/fieldbook/tests/leak_outputs.rs` (the test `delivered_map_is_public_and_bound` and its call in the edition surface; no new `BUILT_SINKS` entry, because `map.html` goes through the `edition` sink).
- Docs to update: `CHANGELOG.md`, one line under `## Unreleased` that starts `- Added: map.html, the App map (layers, workspaces, the AI layer as text).`

## Out of scope

- A change to `nav.rs`. Its "App map" entry is already `NavTarget::from_candidate(Some(map_html), "The app map", "Day 30", planned)` and turns into a link when the path is planned.
- A change to `xtask/src/allowlist.rs` (it already admits `fieldbook_core::strings::paths()[3..=5]`), `xtask/src/edition.rs` `REQUIRED_FILES`, the harness (it opens every built html page itself), `app.js`, any snapshot file (the eleven snapshots use a fixed planned set) or `docs/intake/` and `SECURITY.md`.
- The Releases rows and the Stores and effective labels panel, until the design revision names their strings: this card builds them from that revision only. (The unbuilt-pill state is not one of them: step 3 decides it.)
- Pages that the pills point at (`views-and-day-n` builds the three view pages; `assistant-page` builds `assistant.html`; modules are Release 3). A pill to a page that is not planned shows in the fold.
- Reading an adapter registry, a model call, a key or a socket. The AI column is text.
- Any edit of `home.rs`, `area.rs` or `index.html`.

## Preconditions

- Cards merged: `build-about`, `build-frame`, `frame-unbuilt-entries`, `build-tokens`, `build-home-and-areas`, `build-v0`, `build-site-lints`, `edge-harness`, `narrow-width-check` (all merged on origin/main).
- `design-app-map-page` merged: the design revision that gives the App map's Releases rows in the [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) release names, and its Stores and effective labels panel in the four labels and the stores of [ADR 0004](../decisions/0004-company-boundary.md). Its strings and the mockup's `V.map` are the text source. This is the only unmerged precondition; if it lacks a string this card needs, the card is BLOCKED.
- No open card or draft lists this card's `Files:` paths. The cards that touch `lib.rs`, `site.rs`, `app.css` and `leak_outputs.rs` do so by registration line only, as above.

## Steps

1. Read `docs/design/` (the revision for the App map) and `V.map` in `docs/design/mockup/index.html`. Note the band titles, the pill codes and the Assistant column text. Do not copy a string that the revision changed.
2. `crates/fieldbook-render/src/map.rs`: `pub const OUTPUT_PATH: &str = strings::paths()[5];`, `PAGE_TITLE`, `render_main(view, ctx, profile, planned)` and `render_page(view, ctx, profile, nav, planned)`, modelled on `about.rs` (a `Crumb::linked("Today", ...)` and a `Crumb::current`, and `render_frame_page` with the `CSP_STATIC` policy). Add `pub mod map;` to `lib.rs`.
3. Bands. Outputs: the planned output pages. Work core: the registers (`register::output_path`) and the day-one pages. Workspaces: `area::page_path(code)` for each `[[area]]` of `view.corpus()`, then the three views CPY, CPT and PGM only if their page paths are planned. Modules: an `Unavailable` line ("Release 3") from `source::render_unavailable`, because `modules.csv` is later. Library: the reference pages that exist. Each pill is built through `links::href_if_planned(OUTPUT_PATH, target, planned)`; a pill that returns `None` goes into that band's closed `details.fold.nav-more` "Not built yet" fold with its one landing line from `nav::format_not_available(thing, until)`. Lead decision (the design is silent): this is the unbuilt-pill state. It is not a revision string and not a BLOCKED case.
4. Assistant column: a compiled-in `const ADAPTER_ROWS` of (label, state) pairs: Handoff (built), Off (built), Connected (text only, "where a model adapter exists"), with the "No model connected here" wording of HANDOFF section 9. It names no endpoint and no key. If the build profile is Silent the page also quotes the Short network posture sentence of `docs/cards/README.md` verbatim through `about::NETWORK_POSTURE_SHORT`; it never rewords it.
5. Releases and Stores panels from the revision's strings, with the labels as `Chip` components. No hand-typed count.
6. `templates/map.html`: the markup of `AppMap`: five `section.layer` elements under band headings (`h2`), link pills in `ul.links`, the `.asst-col` aside, and one column below 900px. No `style=""`, no `<script>` and no `href="http`.
7. `assets/app.css`: the `.appmap` block from the mockup's rules, with every padding, margin, gap, border width, radius, font property and z-index a token and only grid tracks, width and height literal. Add the `@container (max-width:900px)` stacking rule. Do not touch another component's rules.
8. `site.rs`: add `map::OUTPUT_PATH` to `planned_paths(view)` and `site.insert(map::OUTPUT_PATH, ...)` next to the other pages. The `debug_assert_eq!` and the test `planned_paths_equal_the_site_keys` keep the two in step.
9. Tests, `crates/fieldbook-render/tests/map.rs`: `map_page_has_five_bands_in_order`, `bands_have_headings_and_pills`, `assistant_column_names_the_adapter_states`, `silent_profile_quotes_the_short_posture_sentence`, `pills_link_only_to_planned_paths`, `an_unplanned_pill_is_in_the_not_built_fold`, `modules_band_is_unavailable`, `map_has_the_static_csp_and_no_inline_style`, `map_has_no_external_href`, `map_stylesheet_stacks_at_900px`, and the surface test in step 10. Use the synthetic corpus the other render tests use (`tests/support`).
10. `crates/fieldbook/tests/leak_outputs.rs`: `delivered_map_is_public_and_bound`, modelled on `run_day_one_binder_surface`: plant a canary in the planned `map.html` bytes and require `write_edition_outputs` to return `Error::OutOfScope`; then with clean bytes require the delivered file to equal the planned bytes. The canary is a synthetic string marked as such.
11. Add the `CHANGELOG.md` line under `## Unreleased`.
12. Run the filters in Acceptance, then the negative control.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test map
cargo nextest run --locked -p fieldbook-render --test frame
cargo nextest run --locked -p fieldbook --test leak_outputs
just site && test -f out/edition/map.html && echo present
grep -c 'style="' out/edition/map.html || true
grep -c 'http-equiv="Content-Security-Policy"' out/edition/map.html
grep -c 'href="http' out/edition/map.html || true
cargo xtask cards --check
just check
```

Expected: the three test runs pass, with the `map` run selecting the ten tests of step 9 and `delivered_map_is_public_and_bound` selected in `leak_outputs`; `present` prints; the `style="` count prints 0 (`grep -c` exits 1 on no match, so the `|| true` keeps the sequence going); the CSP count is 1; the `href="http` count is 0; `cards --check` prints no finding; `just check` exits 0. `planned_paths_equal_the_site_keys` is among the passing `frame` tests, and the nav line "The app map is not available until Day 30." no longer appears on a built site.

## Negative control

Three breaks, each restored after it is seen failing. (1) Delete the `site.insert` for `map::OUTPUT_PATH` in `site.rs`: `planned_paths_equal_the_site_keys` must fail (and the `debug_assert_eq!` panics in a debug build). (2) In `map.rs`, give one pill a literal `href` instead of `href_if_planned` for a path that is not planned: `pills_link_only_to_planned_paths` must fail. (3) Plant a synthetic canary string in the rendered map bytes before delivery: `delivered_map_is_public_and_bound` must fail if the edition sink accepts it. If any of the three stops failing, the card is BLOCKED.

## Rollback

Revert the merge commit. `map.html` leaves the edition, the nav entry returns to "The app map is not available until Day 30.", and the CSS block goes with it. Nothing in the Corpus, a store or any other page changes, and no later card depends on this one except a card that replaces the AI column text (such as `assistant-page`), which would revert first.

## BLOCKED

`design-app-map-page` is not merged, or the revision lacks a string this card needs (the Releases rows or the Stores and effective labels text). Set the item to Blocked with `BLOCKED: the App map design strings are missing`, and stop. Do not invent release names or labels here. A second block: `map.html` is not admitted by the allowlist; set `BLOCKED: allowlist does not admit map.html` and stop, and do not edit `allowlist.rs` in this card.

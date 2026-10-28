# build-page

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`build` writes `p/<page-id>.html` for every page: the header and meta row, the eight parts in the frozen order, prose with numbered citation markers, term and page links, question and conflict chips, precise wording, conflict boxes, the evidence rail, the Sources part built from the citations, and the Change note. Each page reads correctly with scripts blocked.

## Why

- The output path, "frozen part order, Sources built from citations" and the link rules are in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) and [Site output paths](../reference/corpus-format.md#site-output-paths). The part headings, the `empty_part` string and precise wording are in [Strings](../reference/corpus-format.md#strings) and [Page files](../reference/corpus-format.md#page-files). The computed `claims` and `review_by` are in [Layout of corpus/](../reference/corpus-format.md#layout-of-corpus) and [Dates](../reference/corpus-format.md#dates).
- What the reader reads is in [Contract for the reader UI](../reference/corpus-format.md#contract-for-the-reader-ui). Status words follow [Status vocabularies and promotion rules](../reference/corpus-format.md#status-vocabularies-and-promotion-rules).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) rows `#page-<id>` (PageHeader, MetaRow, PageActions, WordingToggle, RichText, TermLink, PageLink, CitationMarker, RefChip, ClaimMark, PreciseBlock, ConflictBox, EvidenceRail / EvidenceCard, SourcesBlock, ChangeNote, Popover) and CitationChip. [HANDOFF.md](../design/HANDOFF.md) [§2 Page template: eight parts](../design/HANDOFF.md#page-template-eight-parts); [§3 Layout](../design/HANDOFF.md#3-layout) (Reader, Rail, Rail fold); [§5 Components](../design/HANDOFF.md#5-components) (those rows) and [Inline token kinds](../design/HANDOFF.md#inline-token-kinds); [§6 States and interactions](../design/HANDOFF.md#6-states-and-interactions); [§7 Status display rules](../design/HANDOFF.md#7-status-display-rules); [§11 Edge cases](../design/HANDOFF.md#11-edge-cases) (review date passed, never verified); [§12 Implementation notes](../design/HANDOFF.md#12-implementation-notes) (Links, Dates, Testing). Token groups: colour (the status tokens), type, space, shape. Interface lane: confirm the browser key `fieldbook.wording` (the wording choice) or name the key you prefer; the card works without the answer.

## Scope

- Files: `crates/fieldbook-render/templates/page.html`, `crates/fieldbook-render/templates/parts/{meta_row.html,sources_block.html,evidence_card.html,conflict_box.html,change_note.html}`, `crates/fieldbook-render/src/{lib.rs,site.rs,page.rs,inline.rs,sources_block.rs,links.rs}`, `crates/fieldbook-render/assets/{app.css,app.js}`, `crates/fieldbook-render/tests/page.rs`, `crates/fieldbook-render/tests/snapshots/` (new snapshots), `CHANGELOG.md`
- Registration lines: `crates/fieldbook/tests/leak_outputs.rs` (edition page surface); `crates/fieldbook/tests/build.rs` (page delivery tests).
- Docs to update: `CHANGELOG.md` (`- Added: page files in the frozen part order with Sources from citations.` under `## Unreleased`).

## Out of scope

- The extra blocks of `brief`, `checklist`, `reference`, `case`, `clause-card` and curriculum pages (`build-page-types`). CalcTable (`build-charts`).
- Ask about this page and Copy context pack (Day 30; `page-context-packs`). The `verify` chip on an overdue page (`build-about` brings chips).
- Excerpt and locator anchors (`extract-and-locators`, `quote-match-and-anchors`). The `p` key (`search-palette`).
- Register pages and source pages, which the chips link to once they exist (`build-registers`, `build-source-pages`).

## Preconditions

- Cards merged: `build-frame`, `build-site-lints`.

## Steps

1. `links.rs`: one function that returns a link when its target path is in `site::planned_paths` (`build-frame`), and plain text otherwise. This card adds the `p/<page-id>.html` path of every page to `planned_paths`. TermLink, CitationChip, CitationMarker rows and RefChip use it, so a page built before the register or source pages raises no FB0603. Read `LabelledView::pages`, `PageEntry::parts`, `LabelledView::citations(page)` and `LabelledView::index` for page data and references.
2. `inline.rs`: RichText over the body that `check` already parsed. Relative `.md` links become PageLink to `.html`. Terms from front matter become TermLink. A citation from `LabelledView::citations` becomes a numbered CitationMarker, numbered in reading order. A `Precise:` blockquote becomes PreciseBlock after its paragraph. Q and XC references become RefChip. No `style` attribute and no inline handler.
3. `page.rs` and `page.html`: PageHeader; MetaRow with `as_of`, `review_by` from `review::page_review_by`, claims supported, last verified (or never verified, in warn) and the id, each date in `<time>`; PageActions with Copy link and WordingToggle; the parts in the order of the `parts` string as h2, with "On this page" in the rail. An empty part shows the `empty_part` string.
   The Change note part is rendered by `parts/change_note.html` and `page.rs`: each line `- CLASS, YYYY-MM-DD: sentence.` becomes a ChangeNote row in the order written, with the class as a chip, the date in `<time>` and the sentence. A line that does not match renders as plain text; `check` reports it as FB0137.
4. `sources_block.rs`: part 7 has one numbered row per marker, in the same reading order. Two existing row variants: a **claim row** carries the claim id, ClaimMark, verification, CitationChip (version and locator), quote and verified date; a **direct source-version row**, from a numbered citation of a source version rather than a claim, renders that version and its locator with whatever source metadata the view holds (title, edition date, as available) and invents no claim, verification or quote. Q and XC references are the neutral RefChip of step 2, not further numbered claim rows. Record-field rows are `r2-records`. In print the rows are footnotes. EvidenceRail holds one EvidenceCard per cited claim with AnswerMeta and the question or conflict it waits on. ConflictBox renders under Never confuse for each conflict the page cites.
5. Overdue review: write a warn Banner with the `hidden` attribute and the page's `review_by`. `app.js` removes `hidden` when the viewer's date is past it. The build never reads the clock.
6. `app.js`: WordingToggle (`aria-pressed`, the choice kept in localStorage under `fieldbook.wording` inside try/catch as in `build-frame`); the marker and link Popover. Without scripts every precise block is shown and every marker is a plain anchor.
7. Tests in `tests/page.rs`, `mod page`: `parts_in_frozen_order`; `empty_part_shows_the_string`; `markers_number_in_reading_order_and_match_sources_rows` (including a direct source-version citation with no claim: the numbers and the Sources rows still correspond, and that row shows the version and locator without claim, verification or quote); `sources_built_only_from_citations`; `unbuilt_targets_are_text`; `md_links_become_html`; `review_by_is_computed`; `never_verified_in_warn`; `change_note_renders_chip_date_and_sentence`; `change_note_is_the_last_part`; `no_style_attribute_or_handler`. Golden renders with `insta`: `concept_page_renders` and `procedure_page_renders` from the fixture. The snapshot tests call `links` and `nav` with a fixed `PlannedPaths::of(...)` (each `p/<page-id>.html` of the fixture and nothing else) and never read `site::planned_paths`, so a later card that registers a path cannot change them. The live behaviour is covered by `build::the_fixture_site_is_lint_clean`.

8. Through the real render/delivery handler in `tests/build.rs`, add `build_from_view_omits_hidden_canary`: a permitted page title and body and an actually emitted PageLink from another permitted page must reach the site; a forbidden page's title, body and link must be absent. The fixture includes that real inter-page link, not a future sidebar entry. Extend the edition SURFACES row with these content canaries. Keep the shell's binding and directory tests.

9. Use each heading anchor retained by PageEntry::parts for the rendered h2/h3 id; never derive a second slug. In the existing page tests, a two-sentence Purpose supplies first-sentence summaries to popovers/area lists, without any summary front-matter key. Search keeps its first-paragraph snippet rule.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render page::
cargo nextest run --locked -p fieldbook build::
cargo nextest run --locked -p xtask literals::
just site
ls out/edition/p/*.html
! grep -L 'http-equiv="Content-Security-Policy"' out/edition/p/*.html | grep .
! grep -l -E 'style="| on[a-z]+=' out/edition/p/*.html
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p fieldbook --test build --test leak_outputs
just check
```

Expected: the tests pass, both snapshots included, and the literal test finds no retyped frozen string; `just site` exits 0 and lists one file per Corpus page; the guarded lines exit 0; `just check` exits 0.

## Negative control

In `page.html`, swap Never confuse and Worked example. `page::parts_in_frozen_order` must fail and name both parts, and the `concept_page_renders` snapshot must differ. Restore the file. Then make `links.rs` always return a link: `build::the_fixture_site_is_lint_clean` must fail with FB0603. Restore the file.

Bypass page filtering before rendering: `build_from_view_omits_hidden_canary` must fail while still requiring the permitted canary. Restore it.

## Rollback

Revert the merge commit. `p/` pages are no longer built. Revert `build-page-types`, `page-context-packs` and every later card that links to `p/` first.

## BLOCKED

- A fixture or Corpus page has a citation that `LabelledView::citations` does not hold: name the page and stop; `check-citations` owns the parse.
- HANDOFF gives a component no state for scripts blocked and the page would hide content: name the component and ask the lead.
- This card passes 600 changed lines. Use the atomic split recipe: keep steps 1–4, their step-7 tests/snapshots, step 8 and its delivery negative control in build-page. Move only steps 5–6 and their interaction tests to `build-page-2`, which requires build-page. Split scopes, registrations and Acceptance filters with those tests, and redirect build-about to build-page-2 for its overdue-banner integration. Update PLAN and all provider dependencies in the same edit; neither part may depend on a test moved to the other.

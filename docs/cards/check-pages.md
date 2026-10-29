# check-pages

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` reads every page through `core-schema-pages` and raises FB0101 to FB0109, FB0114 and FB0116 for its front matter: the fences, the TOML, required fields, vocabularies, the id against the file stem, the area and subarea against `corpus.toml` and the folders, the page type, computed fields that are written, dates that are not real, and an `as_of` after today.

## Why

- The codes and their known-bad examples are in [01: pages, corpus.toml and files](../reference/corpus-format.md#01-pages-corpustoml-and-files). The shape of a page and its fields are in [Page files](../reference/corpus-format.md#page-files); the id form is in [Identifiers](../reference/corpus-format.md#identifiers); dates and today are in [Dates](../reference/corpus-format.md#dates).
- `core-schema-pages` returns each failure with its kind, line and column and leaves the codes to this card.

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,model.rs,index.rs,pages.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports page front matter, ids, areas, types, computed fields and dates (FB0101 to FB0109, FB0114, FB0116).`). `errors.md` does not change.

## Out of scope

- References between pages and to registers, and links in the body (`check-page-links`).
- Parts, body content and citations (`check-page-parts`, `check-page-body`, `check-citations`).
- `handling` other than `public` (FB0410, `check-scans`), `review_by` passed and the claim-date rule (FB0117, FB0118, `check-citation-rules`).
- Files under `corpus/templates/`. They are not pages.

## Preconditions

- Cards merged: `check-promotion`.

## Steps

1. Add the pages to the model: for each path of `files::page_paths` that is UTF-8, the result of `split` and `parse_meta`, the line where the body starts, and the file stem. A page that fails FB0101 or FB0102 to FB0104 offers no `PageMeta` to later passes. Add the page ids of parsed pages to the `Index`. Add `Model::pages(&self) -> &[PageEntry]`, sorted by id, with `PageEntry { pub id: PageId, pub path: String, pub meta: PageMeta, pub body: String, pub body_line: u32 }`. A page whose metadata did not parse is absent and its fault remains in the report. Test `page_accessor_keeps_body_positions_and_skips_bad_meta`.
2. Add `pages.rs` with one pass. Map the failures of `core-schema-pages`: a fence fault is FB0101; CalendarDate is FB0114; syntax, an unknown field or a wrong type is FB0102; a missing field is FB0103; a value outside its vocabulary or form is FB0104, except that a fault in `id` is FB0105, in `area` FB0106, in `subarea` FB0107, in `type` FB0108, and in `handling` only a missing key (FB0103) or wrong type (FB0102); FB0410 in `check-scans` judges the values. Report at the line and column the parser gives.
3. FB0105: the id differs from the file stem, or a second page has the same id (at the second, naming the first). FB0106: `area` against the area codes of `corpus.toml`, the id's first segment and the area folder. FB0107: `subarea` against that area's subareas and the subarea folder.
4. FB0109: `claims` or `review_by` is written, at its line.
5. FB0114 for every front-matter date that is not a real calendar date, or is partial where [Dates](../reference/corpus-format.md#dates) needs a full date, at its line. Whether the TOML reader or `CivilDate` refuses the date, the finding is FB0114 alone, not FB0102.
6. FB0116: `as_of` after `Model::today()`, a warning at the `as_of` line.
7. Unit tests named `check::pages::...` for each code, with the catalogue's examples as in-memory pages, and a page that is not UTF-8 that raises nothing here.
8. Move the ten codes other than FB0114 from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture. FB0114 already has its register case; add a front-matter unit test without replacing that case. If a case of an earlier card now also raises one of these codes (a created page file, for example), add it to that case's `also` and say so in the pull request.

9. For diagnostics on a successfully parsed front-matter field, use `PageMeta::field_location(key)` from the page parser. Do not infer a line by searching the raw text; comments and reordered keys keep their parser-recorded positions.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::pages
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with ten new known-bad cases and the fixture still strict clean; `check` on the real Corpus prints `exit 0`; `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini` and add `review_by = 2026-01-01` to the front matter of one of its pages. Run `(cd target/nc/corpus-mini && cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"; echo "exit $?")`: it must print an `error[FB0109]` line at that line of that page, and `exit 1`. Remove `target/nc/`.

Discard the body position: `page_accessor_keeps_body_positions_and_skips_bad_meta` must fail. Restore it.

## Rollback

Revert the merge commit. The ten new codes go back to `pending.toml`, and the model no longer holds pages. FB0114 remains active for registers.

## BLOCKED

The lead's committed pages raise an error code of this card: write the code, the file and the line and stop. Fix the content in a pull request of its own (GS-02 judges it), then continue.

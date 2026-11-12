# search-section-snippets

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

In the edition's site search index, `assets/search-index.js`, the snippet of each section unit (an id of the form `<page-id>#<anchor>`) has at most 100 code points, and the file is about 450,000 bytes smaller than before, so it stays under the FB0605 cap of 15,000,000 bytes with room for the next content cards. Page-lead and term snippets stay at 160 code points, the format stays `"v":1`, and `fieldbook search` and the ranking are unchanged.

## Why

- The decision. The amendment "glossary term units" of [ADR 0006](../decisions/0006-edition-search-index.md#amendment-glossary-term-units) chose size plan (a): section snippets of at most 100 code points in the site index only. It measured the saving at 453,508 bytes on section units, against 289,771 for 120 code points (not enough) and 622,598 for 80. Page-lead and term snippets stay at 160, and the cut is "a card of its own, which lands before or with the term units on the site". This card is that cut.
- The reason. The index was 14,679,806 bytes when the ADR measured it, 2.1% under the cap, and about 14.8 MB on `main` once the first Corpus cards had landed. A scratch build of `search-site-terms` came to about 14.92 MB, 77,000 bytes under the cap, before the other planned pages. The term units of `search-site-terms` and the planned Corpus pages grow it past the cap without this card. The trigger in the ADR reopens the decision at 14,700,000 bytes.
- Where the cut goes. The site serializer, `crates/fieldbook-render/src/search_index.rs`, already builds the `Index` over the edition's units, then removes URL schemes from each display string. The snippet of the unit is built by `make_snippet` in `fieldbook-core::search`, which the command line also uses, so the cut is made in the site serializer, after the snippet is built and before the URL schemes are removed, and `fieldbook-core` does not change. The unit set, the tokens, the postings and the impacts do not change, so no rank changes.
- The rule. A section unit is a unit of kind `page` or `clause` whose id contains `#`; a page lead has no `#`. For a section snippet of more than 100 code points the cut is the rule of [Display values and the result line](../reference/search.md#display-values-and-the-result-line) with 100 in place of 160: take the first 99 code points, cut back to the last space if there is one, trim, and append `…` (U+2026). A snippet of at most 100 code points is kept as built.
- Reference. [The site index](../reference/search.md#the-site-index) and [Budgets](../reference/search.md#budgets) state the 160-code-point snippet and the 15 MB cap; this card adds the section figure there. FB0605 keeps its meaning and its figure, so [corpus-format.md](../reference/corpus-format.md) does not change.
- Design: none. No template, `assets/app.css`, `search.js` or string changes. `search.js` builds a row from whatever snippet the index holds, with `textContent`, so a shorter section snippet shows less text in a section row on `search.html` and in the palette, and nothing else changes. `search.js` stays held to Rust by the goldens, which this card does not touch.

## Scope

- Files: `crates/fieldbook-render/src/search_index.rs`, `crates/fieldbook-render/tests/search_index.rs`, `docs/reference/search.md`, `CHANGELOG.md`.
- Docs to update: `docs/reference/search.md` (the snippet rule of Display values and the result line: a sentence that the site index cuts a section unit's snippet to 100 code points; the site index bullets and the Budgets Snippet row: 160 on the command line and for page leads and terms, 100 for section units of the site index; the size sentence: the measured size after the cut); `CHANGELOG.md` (`- Changed: section snippets in the site search index are cut to 100 characters to keep the index within its size cap.`).

## Out of scope

- Any change to `fieldbook-core::search`, `make_snippet`, the command line's index or its snippets, tokens, weights, synonyms or sort keys.
- The `parent` page title repeated on every section unit (893,314 bytes) and any change of the index layout: a format change (`v` 2) or the split into two files of the ADR's plan (c).
- Term units on the site: `search-site-terms`, which changes the same file after this card.
- Changing, reordering or removing an existing seed golden or regression case. If a case or golden fails, the cut is wrong; do not edit the case. Add a new one only if a new behaviour needs one.
- Any change to `search.js`, `app.js`, a template or `docs/design/`.
- The cap in code: FB0605 stays at 15,000,000 bytes.

## Preconditions

- Cards merged: none. The amendment of `adr-0006-glossary-units` that decides plan (a) has merged.
- `search-site-terms` lists this card under Cards merged, because both change `search_index.rs` and its test file, and the cut lands first.

## Steps

1. Measure first. Run `just site` on `main` and record `wc -c < out/edition/assets/search-index.js` and the number of section units (`grep -o '\["[^"]*#' out/edition/assets/search-index.js | wc -l`, or a count of `docs` rows whose id contains `#`) in the pull request. Record also the size of `gzip -c out/edition/assets/search-index.js | wc -c`.
2. In `search_index.rs`, add `pub const SECTION_SNIPPET_MAX: usize = 100;` and a pure function `cut_section_snippet(id: &str, snippet: &str) -> String` that returns `snippet` unchanged unless `id` contains `#` and `snippet` has more than 100 code points, and otherwise applies the rule above (first 99 code points, back to the last space if there is one, trim, append `…`). Count `chars()`, never bytes. In `build`, call it for each `DocEntry` after `Index::build` and before `strip_url_schemes`, so the removal of `http://` and `https://` still runs over the final text. Leave every other field of the entry as it is.
3. Unit tests in `mod tests` of `search_index.rs` (the filter `search_index::tests::`) for `cut_section_snippet`: a page lead id (no `#`) with a 160-code-point snippet is returned whole; a section id with exactly 100 code points is returned whole; with 101 code points it is at most 100 code points and ends with `…`; a section snippet with no space in its first 99 code points takes all 99 and stays at 100; a snippet of multi-byte characters is cut by code points and never inside a character.
4. Integration tests in `tests/search_index.rs`, appended and named in this card: `section_snippets_hold_at_most_100_code_points` (every `docs` row whose id contains `#` has a snippet of at most 100 code points, and the fixture holds at least one section row whose built snippet was longer, so the cut is exercised) and `lead_snippets_stay_at_160` (the page lead of `cmp-system-review` still equals `make_snippet(LONG_PURPOSE)` and ends with `…`, as `snippets_hold_at_most_160_code_points` already asserts, and at least one lead has a snippet of more than 100 code points). Keep `snippets_hold_at_most_160_code_points` as it is; it still holds. If no section of the fixture has a first paragraph longer than 100 code points, the test adds such a section to its own copy of the fixture tree, in the test, not in `tests/fixtures/corpus-mini`.
5. Check the unchanged parts. `delivered_index_holds_no_remote_url` and `statistics_come_from_the_page_and_section_units_alone` still pass without edit: the unit set, the impacts and the order of `docs` are those of the index before the cut, and a section snippet that quoted `https://example.org/x` still has the scheme removed after the cut.
6. Run `just site` and record the size of `assets/search-index.js` after the cut, its difference from the size of step 1 (about 450,000 bytes, 453,508 on the measured unit set of the ADR) and the headroom to 15,000,000, in the pull request. A saving under 400,000 bytes means the cut is not applied to every section unit: find out why before going on.
7. Update `search.md` and `CHANGELOG.md` as listed. Run `cargo xtask docs --check`; `errors.md` does not change.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test search_index
cargo nextest run --locked -p fieldbook-render search_index::
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)" --json | grep -cE '"code": "FB012[678]"'
cargo xtask guard-statuses origin/main HEAD
just site && wc -c < out/edition/assets/search-index.js
just browser
cargo xtask docs --check
just check
```

Expected: the tests pass; the `grep` prints `0` (grep exits 1 for no match, which is the expected result, so the regression set still passes and no case changed); the guard exits 0; the `wc` figure is about 450,000 bytes under the figure measured in Step 1 and under 15,000,000, and `just site` exits 0 with no FB0605; `just browser` exits 0 with the Rust and JavaScript agreement checks passing; `docs --check` exits 0; `just check` exits 0. The pull request states the size before and after.

## Negative control

In `build`, raise the cut to 160 (or skip the call to `cut_section_snippet`): `section_snippets_hold_at_most_100_code_points` must fail and show a snippet of more than 100 code points. Restore it. Make the function cut every unit, page leads included: `lead_snippets_stay_at_160` must fail. Restore it.

## Rollback

Revert the merge commit. The site index holds 160-code-point section snippets again and is about 450,000 bytes larger, which can pass the cap once the planned pages land, and `search-site-terms` then has to take a different size plan before it can start.

## BLOCKED

The index after the cut is not under 15,000,000 bytes, a seed golden or regression case fails and only an edit of it would pass, or `just browser` reports a Rust and JavaScript disagreement: comment `BLOCKED: <what>` on the work item with the measured sizes and take the next card whose Scope is disjoint. Never raise the cap in code and never edit a case to pass.

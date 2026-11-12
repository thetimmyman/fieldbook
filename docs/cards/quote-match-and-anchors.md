# quote-match-and-anchors

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook check` resolves each claim's locator in the extraction of the version it cites (FB0304) and finds its quote there after normalisation (FB0308), `new claim` makes the same match before it writes, and each `s/` page shows the extracted text with one anchor per section label and the cited passages highlighted.

## Why

- Quote normalisation, the hyphen rule, the section-anchor rule and the versions that are not matched are [Extraction](../reference/corpus-format.md#extraction) and the "Locators and extraction" note in [03: citations](../reference/corpus-format.md#03-citations). FB0304 and FB0308, with FB0308's severity by release, are in the same table; the severity column is explained at the top of the [Check catalogue](../reference/corpus-format.md#check-catalogue).
- `new claim` matches the quote at once ([AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience)). The `s/` page contents are [The `s/` page](../reference/corpus-format.md#the-s-page).
- Design: the [MAP.md](../design/MAP.md) row "Source drawer" (LocatorView, Excerpt, AnswerMeta). [HANDOFF.md](../design/HANDOFF.md) section 5 Components (those rows and SourceDrawer), section 6 States and interactions, section 11 Edge cases. Token groups: colour (`--color-mark`), type (`--font-mono`).

## Scope

- Files: `crates/fieldbook-core/src/{lib.rs,quote.rs}`, `crates/fieldbook-core/src/check/{mod.rs,quotes.rs}`, `crates/fieldbook-core/tests/{quote_normalisation.rs,quote_no_panic.rs}`, `tests/fixtures/quote-normalisation/` (new, synthetic), `tests/fixtures/corpus-mini/corpus/registers/claims.csv` and `tests/fixtures/corpus-mini/corpus/sources/text/` (so one claim cites the extracted version with a label and a quote that match), `tests/known-bad/{cases.toml,pending.toml}`, `crates/fieldbook-render/src/source.rs`, `crates/fieldbook-render/templates/source.html`, `crates/fieldbook-render/templates/parts/locator-view.html`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/source.rs`, `crates/fieldbook/src/new.rs`, `crates/fieldbook/tests/new.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-core/src/strings.rs` (EXTRACTION_SUPPLIED); `crates/fieldbook-render/templates/parts/supplied-note.html` and `crates/fieldbook-render/tests/source_supplied.rs` (supplied provenance display); `crates/fieldbook/tests/build.rs` (the S-0099 canary's quote and `[locators]` label).
- Docs to update: `CHANGELOG.md` (`- Added: check matches claim quotes in the extracted text; source pages show the text with section anchors.` under `## Unreleased`).

## Out of scope

- Raising FB0308 to an error from Day 30 (the `v0.2.0` change the catalogue names).
- The Ask drawer's Back button and "Used by" assumptions (Day 30 and later).

## Preconditions

- Cards merged: `extract-and-locators` (it reaches `calc-day1`, whose seeded generator `tests/support/rng.rs` step 7 includes).

## Steps

1. `quote.rs`: `normalise(text: &str) -> String` and `find(quote: &str, pages: &[&str], within: RangeInclusive<u32>) -> Option<Span>`, where `pages[i]` is the text of extraction page `i + 1` and `Span { page: u32, start: usize, end: usize }` names the absolute page (from 1) and the byte range of the match in that page's text. Search only pages inside `within`, never an earlier occurrence outside it, with exactly the rules of Extraction, trying a line-end hyphen kept and removed. No other folding: case is kept. `quote.rs` also holds `pub fn locator_resolves(extraction: &ExtractionToml, locator: &str) -> Result<Option<RangeInclusive<u32>>, Code>`, the one locator test. A document locator returns `Some` of its resolved inclusive page range (a single page is N..=N); a deferred table locator returns `None`, not a verified quote. For `p.` and `pp.` the page, or both ends of a range, is within `page_count`; for `sec.`, `§` and `para.` the label is in `[locators]`, matched exactly after NFC; `Err` carries FB0304 for those two groups. The kinds `sheet`, `rec` and `field` are accepted and not read until `extract-csv-rows` and `extract-xlsx-sheets` add their arms, which return FB0311.
2. `check/quotes.rs`: for each claim with a locator on an extracted version, raise the code that `quote::locator_resolves` returns in `Err` (FB0304 when the page or label does not resolve), otherwise, for a non-empty quote and a resolved `Some(range)`, FB0308 when `find` in that range returns nothing, at the severity the catalogue gives for this release. Versions without an extraction and deferred table locators are skipped. An empty optional quote is not a mismatch; the claim-cell rule still enforces required quotes. `check/quotes.rs` holds `pub fn match_claim(model: &Model, claim: &ClaimRow) -> Option<Diagnostic>`: the one diagnostic the pass raises for that claim (FB0304, FB0308 and, from `extract-csv-rows` and `extract-xlsx-sheets`, FB0311), or `None` when the quote matches, is optional and empty, or cannot be checked because the extraction or locator facts are absent. The pass calls it for every claim, and it calls `quote::locator_resolves` for its locator test. Also visit each page's `Model::citations`. For a resolved source-version citation with a locator and available extraction, call `quote::locator_resolves`; emit its error at that page path and citation span. Missing extraction and deferred locators stay unavailable, with no invented match. Claim quote matching remains in `match_claim`. Register the pass in `check/mod.rs`.
3. `new claim`: build the row, then call `check::quotes::match_claim` on it before writing. `new.rs` holds no locator or method logic, so a later locator kind edits `check/quotes.rs` and `extraction/` only. A diagnostic prints, exits 1 and writes nothing.
4. `source.rs` and `source.html`: show each page's text in a LocatorView block, one `id` per section label from `section_anchor`, and for each citing claim an Excerpt (the matched passage, cut to HANDOFF's length) and AnswerMeta. The matched passage is wrapped in `<mark>` and styled with `--color-mark`; a `:target` rule highlights the cited section. A version without an extraction keeps the `Unavailable` parts.
5. Fixtures: `tests/fixtures/quote-normalisation/` holds synthetic pairs with the expected result: curly and straight quotes, dashes, whitespace runs, a line-end hyphen both ways, and no-match pairs that differ only in case or in one letter. Update the corpus-mini claim so the fixture stays strict clean.
6. Known-bad: move FB0304 and FB0308 from `pending.toml` to `cases.toml` (a page beyond `page_count`; one word of the quote changed).
7. Tests: `quote_normalisation` reads every fixture pair, and the same file holds four tests of `quote::locator_resolves` on a three-page extraction with the labels `(e)(1)` and `15.404-1`: `locator_in_range_resolves` (`p. 1`, `p. 3`, `pp. 2-3`, `sec. (e)(1)` and `§ 15.404-1`), `page_beyond_page_count_is_fb0304` (`p. 4` and `pp. 3-4`), `known_label_resolves_and_unknown_is_fb0304` (a label in `[locators]` after NFC, an unknown label, and one that differs in case) and `table_locators_are_accepted_and_not_read` (`sheet`, `rec` and `field` locators give `Ok(None)`); `check::quotes::` covers each code and the skip, `quote_only_outside_locator_is_fb0308`, `quote_repeated_inside_and_outside_matches_inside`, `section_range_limits_the_match` and `empty_optional_quote_is_not_a_mismatch`; `new_claim_refuses_unmatched_quote` and `new_claim_and_check_give_the_same_diagnostic` in `tests/new.rs` (for a quote that does not match, a locator that does not resolve and a quote that matches, the verb prints the diagnostic that `check` raises for the row it would have written, or writes the row when `check` raises none); `anchors_and_marks_on_s_page` in `tests/source.rs`; and `parser_never_panics_on_200_seeded_inputs` in `quote_no_panic.rs` (the file includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 128 from `Rng::new(1)`, and takes a quote of `tests/fixtures/quote-normalisation/` and the locator `§ 15.404-1` with each byte once flipped and once deleted. Each goes through `String::from_utf8_lossy` into `normalise`, into `find` (against a one-page set whose page is the string itself and against the pages of the three-page extraction above, passing the corresponding inclusive range) and into `locator_resolves` (against that extraction); the result is a value, `Ok` or `Err`, never a panic).

8. Own the supplied display at its first use. `strings.rs` defines `EXTRACTION_SUPPLIED = "text supplied by hand; not checked against the original"`; source.rs/source.html render parts/supplied-note.html below the title for method supplied. Use the neutral text token, no status class or inline style. `tests/source_supplied.rs` tests warning presence for supplied, absence for other methods and neutral presentation. FB0223 already covers supplied in extract-and-locators; this card does not introduce another condition.

9. Through the registered `check::quotes::` pass, add `page_citation_locator_resolves`, `page_citation_out_of_range_is_fb0304` and `page_citation_without_extraction_is_unavailable`. Assert the page path and citation line/column as well as the code; use an active extracted version. The known-bad FB0304 plant is a page-body source-version citation beyond that extraction, so it cannot pass through claim traversal alone.

10. Consume the shared unsupported-locator result. `unsupported_extension_is_unavailable` covers page citations and claim rows: retain kind/payload, report no successful locator or quote match, and make the fact available to cite-check; do not report an unknown-but-well-formed kind as malformed built-in syntax. A supported claim with that locator fails the shared promotion requirement for an available locator.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test quote_normalisation
cargo nextest run --locked -p fieldbook-core --test quote_no_panic
cargo nextest run --locked -p fieldbook-core check::quotes::
cargo nextest run --locked -p fieldbook --test known_bad
cargo nextest run --locked -p fieldbook --test new
cargo nextest run --locked -p fieldbook-render --test source
T="$(mktemp -d)"; (cd tests/fixtures/corpus-mini && cargo run -q --locked -p fieldbook -- build --out "$T") && grep -c 'id="sec-' "$T"/edition/s/*.html
! grep -l 'style="' "$T"/edition/s/*.html
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p xtask literals::
just site
cargo nextest run --locked -p fieldbook-render --test source_supplied
just check
```

Expected: every test run passes; `grep -c` prints at least `1` for the extracted version's page; the guarded lines exit 0; `just site` and `just check` exit 0.

## Negative control

In `normalise`, lowercase the text. `cargo nextest run --locked -p fieldbook-core --test quote_normalisation` must fail on the no-match pair that differs only in case. Restore the file. In `locator_resolves`, compare a page with `page_count - 1`: `locator_in_range_resolves` must fail. Restore it. Accept every page: `page_beyond_page_count_is_fb0304` must fail. Restore it. Match a label by its first characters: `known_label_resolves_and_unknown_is_fb0304` must fail. Restore it. Return `Err` for a `rec` locator: `table_locators_are_accepted_and_not_read` must fail. Restore it. Then make `find` slice at `start + 1`: `parser_never_panics_on_200_seeded_inputs` must fail. Restore it.

Search the whole extraction instead of the resolved range: `quote_only_outside_locator_is_fb0308` must fail. Check only the first global match: `quote_repeated_inside_and_outside_matches_inside` must fail. Treat an empty optional quote as a mismatch: `empty_optional_quote_is_not_a_mismatch` must fail. Restore each.

Remove the supplied warning: the supplied-source rendering test must fail. Restore it.

Remove only the page-citation traversal: `check::quotes::page_citation_out_of_range_is_fb0304` and the known-bad FB0304 case must fail. Restore it.

Treat an unsupported extension as a matching locator: `unsupported_extension_is_unavailable` must fail. Restore it.

## Rollback

Revert the merge commit. Quotes are no longer matched, FB0304 and FB0308 return to `pending.toml`, and `s/` pages go back to showing the row only.

## BLOCKED

A real Corpus claim fails FB0304 when this card merges (a locator the lead must fix): set the item to Blocked with a `BLOCKED:` comment naming the claim ids, and stop. Do not edit `corpus/`.

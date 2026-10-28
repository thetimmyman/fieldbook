# search-ranking-and-index

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook_core::search` computes BM25F integer impacts at build time, builds the index and answers a query, and passes every case of `tests/golden/search/seed-ranked.json` and Codex's `ranked-<topic>.json` cases, at least 30 in all. The later `search-site-index` card writes the site index from these pure results. No floating point is used at query time.

## Why

- [AD-11](../decisions/0001-foundation.md#ad-11-search) fixes the outline. The rules are in [Display values and the result line](../reference/search.md#display-values-and-the-result-line), [Ranking](../reference/search.md#ranking), [Query procedure](../reference/search.md#query-procedure) and [The site index](../reference/search.md#the-site-index). The [Worked example](../reference/search.md#worked-example) gives numbers to check by hand.
- The ranked golden format is in [tests/golden/README.md](../../tests/golden/README.md#ranked). The size cap is FB0605 in the [check catalogue](../reference/corpus-format.md#06-site-and-edition), enforced by `build-site-lints`; this card adds no check for the 1 MB target, and `cargo xtask scale` prints the size.
- Design: the [MAP.md](../design/MAP.md) row Search (CommandPalette, SearchTrigger), the index half only; `search.html` and the palette are `search-js` and `search-palette`. [HANDOFF.md](../design/HANDOFF.md) §10 Search. Token groups: none; the index carries no styling.

## Scope

- Files: `crates/fieldbook-core/src/search/{units.rs,bm25f.rs,index.rs,query.rs}`, the `mod` lines in `crates/fieldbook-core/src/search/mod.rs`, `crates/fieldbook-core/tests/golden_ranked.rs`, `tests/golden/search/`.
- Registration lines: the core search module declarations only.
- Docs to update: none. No command or verb arrives.

## Out of scope

- `fieldbook search` and the regression set (`search-cli-and-regression`).
- `search.js`, `search.html` and the palette (`search-js`, `search-palette`).
- The FB0605 size lint itself (`build-site-lints`).
- The company index: one in-memory index over every store the person may read, labels in results, the readers check before a store is opened, and the overlay site index built on demand into `state` (`r2-company-ids`). The edition index stays public-only.

## Preconditions

- Cards merged: `search-core-and-cli`, `core-labels`, `edition-manifest` (`shipped_corpus`), `check-page-parts` (shared heading anchors).
- The lead's `tests/golden/search/seed-ranked.json` is on `main` with its 10 cases.

## Steps

1. `units`: the display values of each unit kind, the snippet rule and natural id order. Unit numbers follow natural id order. Each unit carries `labels`, those of its page or source version (`units::labels_of`, computed; the edition index never stores them). The unit builder takes a `keep` predicate over the label set. The edition build passes `labels::is_public`, so a unit with any other label is left out of `assets/search-index.js`. The company index of `r2-company-ids` calls the same builder with a `keep` that accepts what the store holds ([The company index](../reference/search.md#the-company-index)). A label is never an input to a score. The Corpus files that make units are the ones `shipped_corpus(tree, quarantined)` lists, never the raw tree, so a version named in `quarantined` gives no unit, no snippet and no posting.
2. `bm25f`: the hand-written `ln` with `LN_2` and 15 terms, and the test `ln_matches_the_table_within_1e-12`, which checks `ln(2)`, `ln(3)` and `ln(10)` against the reference values of [Ranking](../reference/search.md#ranking) to within 1e-12. Then the IDF, field lengths, `tf'` in the fixed field order and `impact` with round half away from zero and a minimum of 1. Use only `+`, `-`, `*` and `/` on `f64`.
3. `index`: `Index { docs, terms, post }`, terms sorted by UTF-16 code units, postings ascending by unit. `Index::build(units: Vec<Unit>, glossary: &Glossary) -> Index` serves the Corpus. `Index::from_golden(units: &[GoldenUnit], glossary: &Glossary) -> Index` serves a ranked file: a `GoldenUnit` holds the strings of `fields`, the values of `display` and the optional `exact_key` (default `""`), so it makes the token lists with `index_tokens`, as `units` does, and calls `build`. Preserve `exact_key` unmodified and in NFC through `Index::from_golden` and the existing browser serialization slot; compare normalized query and normalized `exact_key` before tokenization, retaining the original NFC string in storage and serialization. One builder serves both.
4. `query(&Index, &str, limit) -> Results`: exact and last-token prefix scores, the exact class, `matched` and `sum`, and the sort of the Query procedure. Integers only. `Results` carries the total count.
5. `tests/golden_ranked.rs`: load each ranked file with `include_str!`, reject an unknown `kind`, `version` or field, build the index with `Index::from_golden` from its `units` and `glossary`, and check `expect`, `scores` and `impacts`. Accepted kinds are the rows of `UNIT_KINDS`, including `record` and `text`. `exact_key` is optional and defaults to `""`. Report every failing case id. Assert at least 30 cases in all.
6. Add `ranked-<topic>.json` (ids `rk-01` onwards) until the total is at least 30, with the worked example's impacts among the pinned postings.
9. Add ordinary non-seed cases with unequal-scoring versions sharing an unversioned ID or `origin_key`. Exact-class results use natural ID order alone; other results retain matched-count, summed-impact and natural-ID order. Include an exact-key pair `quote-a` and `quote a` in Rust and browser vectors, so dropping exact-key metadata fails them. Share expected ranks between Rust, browser and CLI tests. Restoring score-first order inside the exact class must fail.

10. During source-unit construction explicitly exclude quarantined source register rows; `shipped_corpus` still selects files and editions still retain those rows. Add `search::quarantined_source_rows_have_no_units` with active and quarantined rows carrying distinct title/note canaries: active content remains searchable, and the quarantined unit, snippet and postings are absent. Preserve search-site-index’s later `held_version_not_in_index` coverage.

11. UNIT_KINDS carries optional link forms and unit_kinds_match_the_reference compares that optionality with the reference. Record units have no link until later-record-views registers their pages; this does not remove their searchable text or metadata.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test golden_ranked
cargo nextest run --locked -p fieldbook-core search::
just check
```

Expected: every test passes, with 10 seed ranked cases and at least 30 in all, and `ln_matches_the_table_within_1e-12`; `just check` exits 0. `git diff --stat main -- tests/golden/search/seed-ranked.json` is empty.

## Negative control

Reverse the last tie-break in `query`, so a larger unit number sorts first. `golden_ranked` must fail and name at least one `rk-s-` case. Restore it. Then replace the hand-written `ln` with `f64::ln` in one call: clippy must fail with `disallowed_methods`. Restore it. Bypass the quarantined-row exclusion during unit construction: `search::quarantined_source_rows_have_no_units` must fail. Restore it. Drop `exact_key` from the `quote-a`/`quote a` vector: its case must fail. Restore it.

## Rollback

Revert the merge commit. The pure ranker is removed; revert dependent CLI work first if it has merged.

## BLOCKED

`seed-ranked.json` is missing from `main`, or a seed case fails while the code follows search.md. Do not edit the seed: write the case id and the rule it disagrees with, and stop.

# pipeline-correlate

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`exact-ids+bm25@1`, the default `Correlator`, takes a candidate and the Corpus and returns the links a person needs to judge it: that the file is already in the register, which Corpus ids its text names, which existing source it may be a new version of, which units it sits near, and which open question it may answer. Every link carries its reason, an id found in the text with its quote or a score with the unit id. `Bm25Index`, the `bm25f@1` implementation of `IndexPort` over the existing search code, is built here because the correlator needs one, and it returns exactly what `search` returns.

## Why

- The two questions at intake are "have we got this" and "what is it for". The first is a hash and an id lookup; the second is the `purpose` column, which makes "a source is added only if it addresses a named gap" checkable, because `check` reports an empty `purpose` as FB0205. The correlator links the open question, `run_pipeline` offers the question's text as the suggestion and a person accepts or changes it (the paragraph "Suggestions from the add pipeline" in the [Corpus format](../reference/corpus-format.md)).
- It uses only things the Corpus already has: the id lexer, the BM25F index with integer impacts and exact identifiers first ([Search](../reference/search.md#ranking)), and the registers. No model and no clock, so the same candidate and the same Corpus give the same links on every machine. A model correlator (`pipeline-model-adapters`) may only reorder or choose from this shortlist.
- A link without support would be a guess in the proposal, so the type has no way to build one: `Link` carries a list of `Support` that is never empty. An id that is not in the Corpus is not a link.
- `Correlator::correlate` takes an `&dyn IndexPort`, and `pipeline-ports` wrote only the trait. This card writes the one implementation, registers `bm25f@1` and gives it its contract test, so the correlator has something real to run against. `index-output-ports` later adds `merge_exact_first` and the output wrappers beside it.
- No page, template or string shown in a browser is built here, so there is no Design bullet.

## Scope

- Files: `crates/fieldbook-core/src/pipeline/{correlate.rs,bm25_index.rs}`, `crates/fieldbook-core/tests/{correlate.rs,contract_correlate.rs,support/index_suite.rs}`, `tests/fixtures/ports/correlator/exact-ids+bm25@1/`, `tests/fixtures/ports/index/bm25f@1/`.
- Registration lines: the `exact-ids+bm25@1` and `bm25f@1` rows in `ADAPTERS` (`crates/fieldbook/src/adapters/registry.rs`) and the two `pub mod` lines in `crates/fieldbook-core/src/pipeline/mod.rs`.
- Docs to update: `docs/reference/ports.md` through `cargo xtask docs`. No `CHANGELOG.md` line: no verb calls the correlator yet.

## Out of scope

- A model re-rank and the check that a named id exists (`pipeline-model-adapters`, FB0823).
- `merge_exact_first`, any second index, `OutputPort` and the output wrappers (`index-output-ports`). Semantic retrieval is `later-semantic-search` and is not planned here.
- Using the links: the proposal files, `queue show`, and the choice of what a person sees (`review-queue`).
- Any change to `search`, its index builder, its goldens or the static `search.js`.

## Preconditions

- Cards merged: `pipeline-ports`, `search-ranking-and-index`, `search-cli-and-regression`, `quote-match-and-anchors` (`quote::find`, which step 2 calls) and `calc-day1` (the seeded generator `tests/support/rng.rs` that steps 5 and 6 include).

## Steps

1. `pipeline/bm25_index.rs`: `Bm25Index` holds the built `search::Index` and the unit table, with `from_model(&Model, keep)` using the same builder as `build` and `search` and the same `keep` predicate over the label set of a unit, so a unit whose labels `keep` rejects is left out. `Bm25Index::public(&Model)` is `from_model` with `labels::is_public`, and is the index that `Stages::defaults(model)` holds for the repository Corpus and that `run_pipeline` passes to every `correlate`. A correlator for a company store passes a predicate that accepts what that store holds, so the owner sees company units in the links on screen (which is not egress); the `model@1` correlator is given an index whose predicate is the allow set of its planned destination. It implements `IndexPort` with id `bm25f@1`. `query` calls `search::query` with the query text, maps the exact class to `Tier::Exact` and everything else to `Tier::Ranked`, and keeps the score as the summed integer impact. A `Scope` is applied to the full list and the limit after it. With no scope the hits equal `search::query`'s, in the same order.
2. `pipeline/correlate.rs`, `ExactIdsBm25` implementing `Correlator`. It works in this fixed order, and the first step can end the run. Calls to `quote::find` pass the complete candidate-page range, `1..=page_count`; an empty page set has no match. (1) If `view.has_hash(candidate.sha256)` the result is one `Present` link naming the version id, supported by one `Support::Id` with that id and score 0, and nothing else. (2) A candidate with no extraction gets no further link. (3) Exact ids: run the lexer over every page; each token of the shapes source version, source, claim, question, conflict or clause number that is a row or unit of the Corpus becomes a `Related` link with the page and the text it came from. An id that the Corpus does not have is not a link. (4) `version_of`: for each source whose `title` and `publisher` both appear on page 1 by `quote::find`, one `VersionOf` link to that source id, whose `support` is the two `Quote` supports and a third `Quote` when the text also names the id. (5) Related: the query is `Candidate::first_line()` followed by `first_words(QUERY_WORDS)`; the top `RELATED_LIMIT` hits, with the candidate's own source left out, become `Related` links supported by the unit id and the score. (6) Questions: the same query with `Scope` set to the question kind; the top open question becomes an `Answers` link, supported by one `Support::Id { id: <the question id>, score }`, only if its score is at least `PURPOSE_MIN_SCORE`. The correlator builds no `Suggestion`: `run_pipeline` reads the question's text through `CorpusView` and makes it the `purpose`. An answered or withdrawn question is not offered.
3. Constants: `RELATED_LIMIT = 5`, `QUERY_WORDS = 50` [J] and `PURPOSE_MIN_SCORE` [J]. Set the last one from the fixture so that its one matching question passes and an unrelated question does not, and write the figure and the reason in a comment. The order of the result is fixed: `Present`, the exact-id links in page order, `VersionOf`, `Related` by descending score then unit id, `Answers`.
4. A failure of the index is `StageError::Failed` with the code of the index error and the adapter's name. The correlator never returns `Deferred`.
5. `tests/support/index_suite.rs`, `index_suite(make)`, the Index suite: the same query gives the same hits, an empty query gives no hits and no error, 1 MB of bytes from `Rng::new(1).bytes(1 << 20)` as a query does not panic (`index_suite.rs` includes the generator itself with `#[path = "../../../../tests/support/rng.rs"] mod rng;`, a path relative to the file, which sits one folder deeper than a test file), hits of the exact tier come before ranked hits, a scope restricts the kinds, and the limit holds. `contract_index_bm25f_1`, in `tests/contract_correlate.rs`, which includes the suite with `#[path = "support/index_suite.rs"] mod index_suite;`, runs it, and also `bm25_index_matches_search_query_on_the_seed_cases`, which reads `tests/golden/search/seed-ranked.json` and compares ids, order and scores.
6. Tests in `tests/correlate.rs`, on `corpus-mini` and synthetic candidates: `present_hash_stops_everything`, `exact_ids_become_links_with_their_quote`, `ids_not_in_the_corpus_are_not_links`, `version_of_needs_publisher_and_title`, `related_is_the_top_five_by_score_then_id`, `own_source_is_not_related`, `a_unit_rejected_by_keep_is_never_a_hit` (a hand-built model with a page labelled `np-0007`), `a_predicate_that_accepts_the_label_returns_the_unit`, `open_question_becomes_an_answers_link_only_above_the_floor`, `answered_question_is_not_offered`, `no_extraction_gives_only_present`, `every_link_has_non_empty_support`, `same_input_same_output`, `index_failure_names_the_adapter` and `random_and_mutated_bytes_never_panic` (the file includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`; the test builds 200 byte strings of length 0 to 2048 from `Rng::new(1)` and takes page 1 of the fixture candidate cut at 512 bytes with each of its bytes once flipped and once deleted; each goes through `String::from_utf8_lossy` and is the only page of a candidate for `ExactIdsBm25`, and the result is a list of links that each have a non-empty `support`, or `StageError::Failed`, and never a panic). `contract_correlator_exact_ids_bm25_1` in `tests/contract_correlate.rs` runs `correlator_suite` from `pipeline-ports`.
7. Add the rows: `exact-ids+bm25@1` (Correlator, the default, `Built`, `Silent`, crate `fieldbook-core`, `Needs::None`, `sees` public, `calls_model` false, no settings keys, suite `correlator_suite`) and `bm25f@1` (Index, the default, same flags, suite `index_suite`). Run `cargo xtask docs` and commit `ports.md`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test correlate --test contract_correlate
cargo nextest run --locked -p fieldbook-core --test golden_ranked
cargo nextest run --locked -p fieldbook --test adapters
cargo xtask docs --check
just check
```

Expected: the tests pass; the seed goldens of `search-ranking-and-index` pass with no change, which shows `Bm25Index` and `search` agree; `docs --check` prints `docs: up to date`; `just check` exits 0. `ports.md` lists `exact-ids+bm25@1` and `bm25f@1`.

## Negative control

Set `RELATED_LIMIT` to 6: `related_is_the_top_five_by_score_then_id` must fail. Restore it. Let the support check accept an id that the Corpus does not have: `ids_not_in_the_corpus_are_not_links` must fail. Restore it. In `Bm25Index`, break ties by unit id descending: `bm25_index_matches_search_query_on_the_seed_cases` must fail. Restore it. Then let the BM25 index helper index the first token of a page that has no token: `random_and_mutated_bytes_never_panic` must fail. Restore it. Last, give `Link` a constructor with an empty `support`: `every_link_has_non_empty_support` must fail, or the code must not compile.

Remove the `bm25f@1` row from `ADAPTERS`: `adapters_match_registry_docs_and_fixtures` must fail. Restore it. Do the same for the `exact-ids+bm25@1` row.

## Rollback

Revert the merge commit. `ports.md` loses the two rows. `search` is untouched, so nothing a reader sees changes. If `review-queue` has merged, revert it first, because it calls the correlator.

## BLOCKED

`search::query` takes a built `Index` and no scope, and the question units cannot be told from the others without reading the display values, so the open question cannot be selected. Set the item to Blocked with `BLOCKED: search cannot filter by unit kind`, and stop. Do not build a second index for `questions.csv`.

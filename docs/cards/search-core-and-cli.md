# search-core-and-cli

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook_core::search` holds the one tokenizer of search.md, both sides, and builds the five token fields of every document unit with synonyms applied. It passes all 52 seed cases (`tests/golden/search/seed-tokenize.json`, 40 cases, and `seed-tokenize-company.json`, 12 cases) and Codex's `tokenize-<topic>.json` cases, at least 200 cases in all. `f64::ln`, `powf`, `powi` and `exp` are in `disallowed-methods` in the root `clippy.toml`. The lexer shapes and the document unit kinds are two const tables, `LEXER_SHAPES` and `UNIT_KINDS`, each tested against the reference, and the lexer keeps every id of the `FAMILIES` table whole.

## Why

- The outline is [AD-11](../decisions/0001-foundation.md#ad-11-search). The rules are in [Tokenizer](../reference/search.md#tokenizer), [Synonyms](../reference/search.md#synonyms), [Where Rust and JavaScript can drift](../reference/search.md#where-rust-and-javascript-can-drift) and [Document units](../reference/search.md#document-units).
- The clippy ban is in [Ranking](../reference/search.md#ranking) (Determinism). It lands here, before any ranking code exists.
- The golden format, the seed rule and how Rust loads the vectors are in [tests/golden/README.md](../../tests/golden/README.md) and [Golden vectors](../reference/search.md#golden-vectors).
- This is the first of the three search cards, with `search-ranking-and-index` and `search-cli-and-regression` ([PLAN.md](../plan/PLAN.md#p3-find)).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) row Search (CommandPalette, SearchTrigger), the tokenizer half only. Nothing is drawn here; `search-ranking-and-index`, `search-js` and `search-palette` build what a reader sees. [HANDOFF.md](../design/HANDOFF.md) [§10 Search](../design/HANDOFF.md#10-search). Token groups: none.

## Scope

- Files: `crates/fieldbook-core/src/search/{mod.rs,normalise.rs,lexer.rs,synonyms.rs,units.rs}`, `crates/fieldbook-core/src/lib.rs` (the `pub mod search;` line), `crates/fieldbook-core/tests/{golden_tokenize.rs,search_no_panic.rs}`, `clippy.toml`, `tests/golden/search/`.
- Docs to update: none. No command arrives.

## Out of scope

- Display values, snippets, BM25F, the index and the query function (`search-ranking-and-index`).
- `fieldbook search` and the regression set (`search-cli-and-regression`).
- Any edit to a `seed-*.json` file.

## Preconditions

- Cards merged: `corpus-mini-fixture`.
- It reaches `core-ids-and-dates` through `core-schema-claims`, so the `FAMILIES` table and, through `calc-day1`, the generator `tests/support/rng.rs` exist.
- The lead's seed files are on `main`: `tests/golden/search/seed-tokenize.json` (40 cases) and `tests/golden/search/seed-tokenize-company.json` (12 cases, ids `tok-sc-001` onwards), as [Seed coverage](../reference/search.md#seed-coverage) lists them.
- May run early, beside the critical path, with any card that touches none of `clippy.toml`, `crates/fieldbook-core/src/lib.rs` and `crates/fieldbook-core/src/search/`. The check cards up to `check-scans`, `calc-cases`, `add-new-and-edition`, `new-verbs`, `extract-and-locators` and `quote-match-and-anchors` edit `lib.rs`, so not beside those.

## Steps

1. `normalise(&str) -> String`: the steps of [Normalisation](../reference/search.md#normalisation) in their order, with `unicode-normalization` for NFKC. Unit tests for each fold.
2. `lexer`: the shapes of [The lexer](../reference/search.md#the-lexer), tried in the order given, each with its boundary rule; every other character that is not a word character separates. Word characters use `char::is_alphanumeric` only. The shapes are the const `LEXER_SHAPES` in `lexer.rs`: one row per shape, in the order of the table, with its name, its pattern text and its example, and the lexer reads the table. Write all eleven shapes, including 6 to 8, the company ids (a writer-digit prefix and six sequence digits make six to ten digits).
3. `index_tokens(&str) -> Vec<String>` adds the extras of [The index side adds tokens](../reference/search.md#the-index-side-adds-tokens) in the order given. `query_tokens(&str) -> Vec<String>` follows [The query side](../reference/search.md#the-query-side): duplicates removed, at most 12, no extras.
4. `synonyms`: `Glossary` (a list of rows, each a `label` and its `aliases`, the form of the `glossary` in a ranked file) and the synonym sets made from its rows, applied on the index side to the body field, in glossary order, once per set per unit.
5. `units`: `Unit` with `id`, `kind` and the five fields as token lists. This card defines types and the kind table; `search-ranking-and-index` builds live units from the Corpus model in the order of the Document units table. Page text follows the rules under that table. Section units have an empty `id` field. The kinds are the const `UNIT_KINDS` in `units.rs`: `page`, `clause`, `source`, `claim`, `question`, `conflict`, `term`, `record` and `text`, each with its link form from the table of document units. This card builds the kinds the Corpus has; `record` and `text` are rows of the const that `r2-records` and `r2-company-ids` build.
6. `tests/golden_tokenize.rs`: a loader that rejects an unknown `kind`, `version` or field, runs every case on its `side`, and reports every failing case id at once. Include each file with `include_str!` from a list in the test: `seed-tokenize.json`, `seed-tokenize-company.json`, then the five topic files of step 7. The test also asserts 52 seed cases (the ids that begin `tok-s-` or `tok-sc-`) and at least 200 cases in all.
7. Add five files of cases, ids `tok-001` onwards, in the groups of [Seed coverage](../reference/search.md#seed-coverage): `tokenize-normalisation.json` (30 cases), `tokenize-words.json` (30), `tokenize-shapes.json` (45), `tokenize-extras.json` (25) and `tokenize-query.json` (20). These 150 cases and the 52 seed cases make 202; the total must be at least 200. Each expected list follows from search.md; a case the document does not settle goes to the lead and is not added. Include at least three cases for each of the shapes 6 to 8, taken from the examples of the table, on both sides, in `tokenize-shapes.json`.
8. Add `f64::ln`, `f64::powf`, `f64::powi` and `f64::exp` to `disallowed-methods`, each with a reason that names search.md Ranking. Check that clippy resolves each path (step 2 of the Negative control).
9. `lexer_shapes_match_the_reference` (in `search::lexer`): reads the lexer table of `search.md` with `include_str!` and fails when a row of `LEXER_SHAPES` differs from the table in name, order, pattern or example, or when the table has a row that the const lacks, and names the row.
10. `unit_kinds_match_the_reference` (in `search::units`): reads the table of document units and compares its kinds and link forms with `UNIT_KINDS`, in both directions.
11. `lexer_covers_every_family` (in `search::lexer`): two tables in the test. `LEXED` lists the families whose ids the lexer keeps whole, with the shape: the source, source version, claim, question and conflict families (shapes 2, 1, 3, 4 and 5) and, from the card `r2-company-ids` on, the company source, source version, claim, question, conflict and record families (shapes 8, 7, 6, 8 and 8, and shape 8 for a record and 7 for a record version). `NOT_LEXED` lists every other family with a one-line reason (an area, a page, a term, a calculation case, an error code, an ADR, an edition and a tool release are words to the lexer). A row of `FAMILIES` that is in neither table fails the test and is named, so a new id family forces a decision here. For each family in `LEXED` the lower-case example lexes to one token that equals it, by the shape named. For each company prefix (set by `r2-company-ids`; none before it) the text `<prefix>-000001` must lex by shape 6, 7 or 8 and never by shape 5, which is why the company conflict prefix is `XO` and `XC` is refused.
12. Write `tests/search_no_panic.rs` with `random_and_mutated_bytes_never_panic`. It includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 96 from `Rng::new(1)`, and takes the valid text `Cost-type contracts under 252.204-7012 cite CL-CMP-0011, S-0042-v2 and 1,000,000.50.` with each of its bytes once flipped and once deleted. Each goes through `String::from_utf8_lossy` into `normalise`, `index_tokens` and `query_tokens`. The result has no empty token, `query_tokens` holds at most 12 tokens with no duplicate, and nothing panics.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core search::
cargo nextest run --locked -p fieldbook-core --test golden_tokenize --test search_no_panic
cargo clippy --workspace --all-targets --locked -- -D warnings
just check
```

Expected: every test passes, including `lexer_shapes_match_the_reference`, `unit_kinds_match_the_reference`, `lexer_covers_every_family` and `random_and_mutated_bytes_never_panic`, and `golden_tokenize` reports 52 seed cases and at least 200 cases in all; clippy exits 0 with the new bans in place; `just check` exits 0. `git diff --stat main -- tests/golden/search/seed-tokenize.json tests/golden/search/seed-tokenize-company.json` is empty.

## Negative control

Remove the NFKC step from `normalise`. `golden_tokenize` must fail and list the failing case ids, among them at least one `tok-s-` id from the Normalisation group. Restore it. Then add `let _ = 2.0_f64.ln();` to a test in `search::`: clippy must fail and name `disallowed_methods`. Remove the line. Then change the pattern of shape 3 in `LEXER_SHAPES`: `lexer_shapes_match_the_reference` must fail and name the row. Restore the line. Then remove `term` from `UNIT_KINDS`: `unit_kinds_match_the_reference` must fail. Restore the line. Then remove `claim` from `LEXED`: `lexer_covers_every_family` must fail and name the family. Restore the line. Then make `normalise` call `unwrap()` on the first character of its input: `random_and_mutated_bytes_never_panic` must fail. Restore the line.

## Rollback

Revert the merge commit. The search module and the four bans are removed; no other crate uses them yet.

## BLOCKED

A seed case fails while the code follows search.md. Do not edit the seed and do not bend the code to it: write the case id and the rule it disagrees with, and stop.

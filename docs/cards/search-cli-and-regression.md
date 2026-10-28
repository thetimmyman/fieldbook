# search-cli-and-regression

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook search [--json] <query>...` prints at most 10 results in the form of search.md, with the full record for an exact id and every version for a source id. `check::run` runs `corpus/eval/regression.toml` and raises FB0126, FB0127 and FB0128. After this card `tests/known-bad/pending.toml` holds no row for FB0126, FB0127 or FB0128, or for a code owned by a card this card reaches through Cards merged. The rows of codes owned by later cards, and the `[[pending_control]]` rows of later cards, stay in the file, and each later card deletes its own.

## Why

- The command, its output, `--json` and exit codes are in [The command line](../reference/search.md#the-command-line). The verb is in [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience).
- The regression file and what counts as a miss are in [Regression set](../reference/search.md#regression-set). The three codes are in [01: pages, `corpus.toml` and files](../reference/corpus-format.md#01-pages-corpustoml-and-files) of the check catalogue.
- The known-bad harness and its three files come from `check-known-bad`. This card deletes the `pending.toml` rows of its three codes and no others.
- Design: the [MAP.md](../design/MAP.md) row CommandChip gains the `search` verb only; the chip text comes from the verb map in `fieldbook-core::strings`. No page. [HANDOFF.md](../design/HANDOFF.md) §5 (CommandChip). Token groups: none.

## Scope

- Files: `crates/fieldbook/src/{lib.rs,cli.rs,search.rs}`, `crates/fieldbook/tests/{search.rs,chips.rs}`, `crates/fieldbook/tests/snapshots/` (new snapshot), `crates/fieldbook-core/src/{eval.rs,check/regression.rs}`, `crates/fieldbook-core/src/check/mod.rs`, the `pub mod eval;` line in `crates/fieldbook-core/src/lib.rs`, `crates/fieldbook-core/tests/eval.rs`, `tests/golden/json/search.json`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/corpus/eval/regression.toml` (new, synthetic).
- Registration lines: `tests/support/chip_cases.rs` (the `fieldbook search` case, this card's entry only); the Commands line of `AGENTS.md` and one quick start line of `README.md` (Docs to update); `crates/fieldbook-core/src/report.rs` (the struct `SearchReport`) and `crates/fieldbook/tests/json_reports.rs` (one row), both made by `check-command`; `crates/fieldbook/tests/cli.rs` (remove `search` from `NOT_BUILT`, this card's verb only).
- Docs to update: `AGENTS.md` (Commands: `fieldbook search`), `README.md` (one quick start line), `docs/reference/cli.md` through `cargo xtask docs`, `CHANGELOG.md` (`- Added: the search command.` under `## Unreleased`).

## Out of scope

- `search.js`, `search.html` and the palette (`search-js`, `search-palette`).
- The p95 timing gate: the Edge harness times lookups (`search-js`), and the laptop gates them at E1. This card reports the CLI time and gates nothing.
- Writing or changing the lead's `corpus/eval/regression.toml`.
- The question log. `ask-log` edits `crates/fieldbook/src/search.rs` to call the logger after this card merges; nothing in this card's scope changes for it, and `search` writes no log here.

## Preconditions

- Cards merged: `search-ranking-and-index`, `check-command` (it made `report.rs`, `json_reports.rs` and `chips.rs`).
- The lead's `corpus/eval/regression.toml` (20 to 30 questions) is on `main` ([PLAN.md](../plan/PLAN.md#p3-find)).

## Steps

1. Add `search` to the clap `Cli`: `--json` and one or more words, joined with single spaces. It reads the Corpus through `World`, builds the units and the index in memory, runs `query` with limit max(10, eligible exact-class count) for exact unversioned source IDs, record IDs or natural keys, and 10 otherwise and writes nothing. Remove `search` from `PENDING` in `crates/fieldbook/tests/chips.rs` (`check-command`), so the chip test parses the `search` chip, and from `NOT_BUILT` in `crates/fieldbook/tests/cli.rs`. Register its case in `tests/support/chip_cases.rs` (a fresh Home `corpus-mini` copy, exit 0 and the expected hits).
2. Print the two-line form, the exact-id record, and the source-id version list as search.md gives them. No results prints the abstention phrase from `fieldbook-core::strings` and exits 1.
3. `--json` prints one object with the keys of search.md in order. Follow [Adding a JSON report](README.md#adding-a-json-report): the struct `SearchReport` in `report.rs` (`report_version` first), the golden `tests/golden/json/search.json` made on the fixture, one row in `json_reports.rs`, and `cargo xtask docs`. No `insta` snapshot pins `--json`.
4. `eval.rs`: parse `regression.toml` into cases (`id`, `query`, `expect`, `top` default 5, `expect_none`, `critical` default false), rejecting unknown keys. `from` is not a key here: `ask-promote` adds it to `eval.rs` and to the key table of [search.md](../reference/search.md#regression-set).
5. `check/regression.rs`: a pass of `check::run`. A malformed file, a repeated `id` or an expected id that does not exist is FB0126; a miss is FB0127; a critical miss is FB0128. Each points at the case's line. Add the pass to the fixed list in `check/mod.rs`.
6. Write the fixture's `regression.toml`: three synthetic cases that pass (one critical, one `expect_none`, one with `top = 3`). Add a `cases.toml` row for each of FB0126, FB0127 and FB0128, and delete the FB0126, FB0127 and FB0128 rows of `pending.toml`, which leaves the rows of the codes that later cards own.
7. Process tests in `tests/search.rs`: a claim id, the second fixture source's id (both versions, in version order), a word with results, a query with none (exit 1), `--json` valid, no words (exit 2), and a word that matches more than 10 units (a temporary copy of the fixture with synthetic claims added), whose output ends with the `Showing 10 of N.` line (an `insta` snapshot, `cut_list_says_how_many`). Print the time of the fixture's regression queries to the test log. Add `regression_cases_have_ids_and_expectations`, the drift check of "Adding a regression case" in the [cards README](README.md#adding-a-regression-case): in the fixture's file and in the lead's `corpus/eval/regression.toml`, every case has an `id` of the form `rg-NNN` that no other case repeats, a non-empty `query`, and either `expect` or `expect_none = true`. In `crates/fieldbook-core/tests/eval.rs`, add `regression_toml_never_panics_on_random_and_mutated_bytes`: it includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, feeds the parser of `eval.rs` 200 byte strings of length 0 to 96 from `Rng::new(1)` and the fixture's `regression.toml` with each byte flipped once (through `String::from_utf8_lossy`), and requires a list of cases or a typed error, never a panic.

8. In the binary integration suite for `search`, `search_waits_for_a_complete_generation` pauses a two-file writer between files and calls the real handler. It waits and sees one complete generation, or refuses on timeout; no mixed content appears. A crashed pending transaction is refused until recovery.

9. Add ordinary non-seed cases with unequal-scoring versions sharing an unversioned ID or `origin_key`. Exact-class results use natural ID order alone; other results retain matched-count, summed-impact and natural-ID order. Share expected ranks between Rust, browser and CLI tests. Restoring score-first order inside the exact class must fail.

10. `source_id_lists_more_than_ten_versions` uses twelve eligible versions plus a quarantined version and checks plain and JSON results, natural order, all eligible versions and the actual displayed-count message. Extend the same vector to record IDs and natural keys when company search lands.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test search --test known_bad --test json_reports --test chips
cargo nextest run --locked -p fieldbook-core --test eval
(cd tests/fixtures/corpus-mini && cargo run -q --locked -p fieldbook -- search CL-; echo "exit $?")
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: every test passes, `regression_toml_never_panics_on_random_and_mutated_bytes` and `json_goldens_match_reports` among them, and `pending.toml` has no row for FB0126, FB0127 or FB0128 or for a code owned by a card this card reaches through Cards merged (list the `code =` values that remain and confirm that each belongs to a later card); the fixture search prints numbered results that include a claim id and `exit 0`; `check --strict` on the real Corpus prints no FB0126 or FB0128 and `exit 0`; `just check` exits 0, and its step 7 now runs the lead's regression set.

## Negative control

In a temporary copy of the fixture, mark a case `critical`, give it a word that only one fixture page contains as `query`, and set its `expect` to a claim id whose row lacks that word. `check` there must exit 1 and print `error[FB0128]` with the case's line. Remove the copy. Then put the FB0128 row back in `pending.toml`: `every_code_is_accounted_for` must fail. Restore the file.

Bypass the handler's store lease: `search_waits_for_a_complete_generation` must fail. Restore it.

Restore a fixed ten-result cap for an exact source query: `source_id_lists_more_than_ten_versions` must fail. Restore it.

## Rollback

Revert the merge commit. `search` becomes a usage error, the regression set stops running, and the three codes return to `pending.toml`.

## BLOCKED

The lead's regression set is missing, or it has a critical miss on the real Corpus. Name the case id and stop. Do not change the set, and do not tune the ranking to pass it.

# scale-fixture

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask scale` makes a synthetic Corpus at 1, 5 and 20 times the Day 1 counts in `scale/base.toml`, runs the built `fieldbook` over each, and prints one table: the Corpus size, the unit count, the time of `check`, of `check <path>`, of `build` and of one `search` call, the size of the search index and the peak memory. It is advisory: no figure fails a run, a test or CI. Two runs give the same counts and sizes.

## Why

- NFR-F-04 reads "A full `fieldbook check` takes 5 seconds or less at Day 1 size and 60 seconds or less at 20 times Day 1 size (measured by `cargo xtask scale`)" ([Requirements referenced](../decisions/0001-foundation.md#requirements-referenced)). This card is that measurement. The R2 gate takes the check, build and search times, the index size and the peak memory from its latest table ([Gates](../decisions/0001-foundation.md#gates)).
- Search builds its index in memory on every call and keeps no cache, because a cache would show stale cited text ([Budgets](../reference/search.md#budgets)). The reopen triggers there (a full `check` above 60 s, an edition index above 15 MB) are the numbers this table shows before they bite.
- The run is advisory because a time depends on the machine ([When each runs](../reference/testing.md#when-each-runs)): the figure is printed and a person judges it. The weekly `scale` job of [scheduled-workflow](scheduled-workflow.md) uploads the table. `xtask` is the only code that starts processes ([AD-05](../decisions/0001-foundation.md#ad-05-crate-layout-and-enforcement)).
- Seeded random input comes from the one generator of [Test data](../reference/testing.md#test-data); this card writes no second one.

## Scope

- Files: `xtask/src/scale.rs` (its tests are in its `mod tests`), `xtask/tests/{scale.rs,scale-tiny.toml}` (`scale.rs` holds the one test that runs the built program), `scale/base.toml` (new), `xtask/Cargo.toml` and `Cargo.lock` (only xtask's `dependencies` list, and only if an edge is missing: `toml`, `serde`, `serde_json`, `sha2` and the path edge to `fieldbook-core` are the ones this card uses).
- Registration lines: `xtask/src/main.rs` (the `mod scale;` line and one arm of the dispatch); the Commands line of `AGENTS.md` and, if an edge was added, the Used by cell of its row in `docs/reference/dependencies.md` (both in Docs to update).
- Docs to update: `AGENTS.md` (Commands: `cargo xtask scale`, advisory and not part of `just check`). `docs/reference/dependencies.md` only if an edge was added: the Used by cell of that row.

## Out of scope

- A gate. No figure fails a run, a test or CI. The `check` budgets are judged by a person at the R2 gate, and the p95 lookup time of NFR-F-01 is timed by the Edge harness (`search-js`), not here.
- The company index and the overlay site (`r2-company-ids`), the backup size (`backup-snapshot` prints its own), the AI build, and any real content.
- A fingerprint cache, shards, or an incremental `check`. `check` reads the whole Corpus on every run, and the table says what that costs.
- A `just` recipe. The command is run by name.

## Preconditions

- Cards merged: `search-site-index`, `search-cli-and-regression` (the `check`, `build` and `search` verbs and everything they need), `edge-harness` (the path edges from `xtask` to `fieldbook-core` and `fieldbook-render`), `calc-day1` (the seeded generator at `tests/support/rng.rs`), `extract-and-locators` (the fixture's one stored original and its extraction, whose shape the generator repeats).
- Must merge before `scheduled-workflow`, whose `scale` job runs this command. May run beside `search-js` and `search-palette`: their paths are disjoint.

## Steps

1. Write `scale/base.toml`, the Day 1 counts. Every value is an integer above zero, and the file holds these keys and no others:

   ```toml
   # Day 1 counts for `cargo xtask scale`. Synthetic: a rounded estimate of the
   # Day 1 Corpus. The lead edits these when `fieldbook status` shows real counts.
   seed = 1
   areas = 13
   pages = 120
   words_per_section = 80
   source_versions = 90
   stored_originals = 30
   original_bytes = 150000
   claims = 300
   questions = 49
   conflicts = 10
   glossary_terms = 80
   ```

   A size `m` multiplies `pages`, `source_versions`, `stored_originals`, `claims`, `questions`, `conflicts` and `glossary_terms` by `m`. The other keys describe the shape and do not change with the size.
2. In `xtask/src/scale.rs`, write the pure part. `Base::parse(&str) -> Result<Base, String>` reads the file with `deny_unknown_fields` and names the key of every error: an unknown key, a missing key, a value that is not an integer, a value of zero. `generate(&Base, m, today) -> Generated` returns the files as `(path, bytes)` pairs in path order, and `Counts` (pages, sections, claims and the other record counts, and `corpus_bytes`). It takes no clock and reads no file. `today` is the text of `tests/fixtures/corpus-mini/TODAY`. `xtask` is a binary crate, so a file in `xtask/tests/` cannot call `Base::parse`, `generate` or `vmhwm_kb`: step 9 puts their tests in `mod tests` of this file, which reads `scale/base.toml`, `xtask/tests/scale-tiny.toml` and `tests/fixtures/corpus-mini/TODAY` with `include_str!` (`../../scale/base.toml`, `../tests/scale-tiny.toml`, `../../tests/fixtures/corpus-mini/TODAY`).
3. The text. Include the generator with `#[path = "../../tests/support/rng.rs"] mod rng;` and write no second one (`one_rng_exists`). Make 2,000 pseudo-words once from `Rng::new(seed)`, each of two to four syllables of a consonant and a vowel. Pick a word with the index `floor(2000 x u x u)` for `u` uniform in [0, 1), so that a few words are common and most are rare, as in real text. A section is its heading and `words_per_section` words. The most common word is the search query of step 5.
4. The files. Model each on its twin in `tests/fixtures/corpus-mini/` (the shapes and cells are there and are not repeated here): `corpus/corpus.toml` with `areas` areas (synthetic codes) and one view; `corpus/AGENTS.md`, `corpus/ASSISTANT.md`, `scan-allow.toml` and `changes.md`; the five registers in normal form (use the writer of `core-csv`); the pages under `corpus/pages/`, the parts required by each type’s template, types in rotation (`concept`, `procedure`, `brief`), `status = "draft"`, each citing two claims and listing one term, `as_of` set to `today`; omit `review_by` and `claims` from front matter because they are computed; `source_versions` source rows, every third source with a second version that supersedes the first; `stored_originals` plain-text originals of `original_bytes` bytes each under `corpus/sources/public/`, with the hash in the row and the extraction files, as the fixture's one original; claims that are not consequential (so none needs a locator or a quote); questions, conflicts (each with its question) and glossary terms. Every id is unique, every reference resolves, and every record says it is synthetic. A `TODAY` file at the root of the generated tree holds `today`. Where a rule of the check catalogue and the generator disagree, the generator changes.
5. The run, `scale::run(args)`. Flags: `--sizes 1,5,20` (the default; integers of 1 or more, each once), `--base FILE` (default `scale/base.toml`) and `--bin PATH`. Without `--bin` it runs `cargo build --locked --release -p fieldbook` and uses `target/release/fieldbook`; a build that fails exits 3 with cargo's message. An unknown flag or a bad size exits 2. For each size, in the order given:
   1. Make the folder `<temporary directory>/fieldbook-scale-<pid>-<m>x` with `create_dir` (it fails if the name exists, so an earlier run's folder is never reused), write the files, and delete the folder on every path, success or failure (on Windows, clear read-only flags first).
   2. With the folder as the working directory, run in this order and time each with a monotonic clock: `check --strict --today <TODAY text>`; the same with the path `corpus/pages`; `build --out site` (the site is then `site/edition/`); and `search <the most common word>` once. Progress lines go to standard error.
   3. `check` must exit 0. Any other exit stops the run with exit 1 and the line `scale: check failed on the generated Corpus (<m>x), exit <n>`, followed by the first 20 lines of its output: the generator is wrong, and a figure from it means nothing. The same holds for `check <path>` and `search` (which must exit 0), and for a `build` that exits non-zero for any reason but FB0605.
   4. A `build` that exits 1 and prints FB0605 is a result and not an error: the edition holds `search-index.js` to 15 MB (other data files to 3 MB). Record it; the `units` and `index_kb` cells are `-`.
   5. When `build` exits 0, `index_kb` is the length of `site/edition/assets/search-index.js` in kilobytes (1,000 bytes) and `units` is the length of the `docs` array in that file (the page and section units that the edition index holds, not every unit of the Corpus) (strip `window.FB_INDEX = ` and the final `;`, parse with `serde_json`).
   6. Peak memory: on Linux, while a command runs, read `VmHWM` from `/proc/<pid>/status` of the child every 5 ms and keep the largest value; the cell is the largest over the four commands, in megabytes (1,000,000 bytes), rounded. Elsewhere the cell is `n/a`. The parser is a pure function, `vmhwm_kb(&str) -> Option<u64>`.
6. The output, on standard output only (the weekly job pipes it into a file). Line 1 is `scale base=<--base value> seed=<seed>`. Line 2 is the header, `size units corpus_kb check_s check_path_s build_s search_ms index_kb rss_mb`, with cells separated by two spaces and aligned. Then one row per size, with `size` written `1x`, `5x`, `20x`; seconds with two decimals, the others whole numbers. A stopped build writes its seconds with a trailing `*`, and one line after the rows says `* build stopped by FB0605 (a data file over the cap); the time is to the stop.` Then, for the sizes 1 and 20 when they were run, one line each: `NFR-F-04 check 1x: <s> s, budget 5 s, within` or `over`; and for 20x with budget 60 s. The verdict compares whole milliseconds, so `5.000 s` is within and `5.001 s` is over. The run still exits 0.
7. In `xtask/src/main.rs`, add the `mod scale;` line and one arm of the dispatch, and the command's line to the usage text.
8. Write `xtask/tests/scale-tiny.toml`, a base of the same keys with small values (`pages = 6`, `words_per_section = 12`, `source_versions = 6`, `stored_originals = 2`, `original_bytes = 2000`, `claims = 12`, `questions = 4`, `conflicts = 2`, `glossary_terms = 6`, `areas = 3`, `seed = 7`).
9. Tests. All but the last are in `mod tests` of `xtask/src/scale.rs` and run as `-p xtask scale::`. The last, `scale_runs_end_to_end`, is in `xtask/tests/scale.rs`, which can only run the built program.
   - `base_parses_and_rejects`: `scale/base.toml` parses with every value above zero; a table of strings with a missing key, an unknown key, a zero, and a string value each fails and names the key.
   - `generated_corpus_is_clean_under_strict`: for `scale-tiny.toml` at 1x and 3x, and for `scale/base.toml` at 1x, put the files into a `Tree` and run `fieldbook_core::check::run` with `strict` and the `TODAY` date: no diagnostic of any severity.
   - `generation_is_deterministic`: two calls give equal files; a different `seed` gives different files.
   - `counts_scale_with_the_size`: at 5x the page, claim, source, question, conflict and term counts are five times those at 1x, the area count is unchanged, and no path and no id repeats.
   - `table_has_the_columns`: the header line and a sample row are exactly as step 6 says. `a_stopped_build_is_starred`: the sample with FB0605 has the `*`, the two `-` cells and the explanation line. `nfr_lines_say_within_or_over`: 5.000 s within and 5.001 s over at 1x; 60.000 s within and 60.001 s over at 20x.
   - `vmhwm_is_read_from_status`: a sample `status` text with `VmHWM:     123456 kB` gives `Some(123456)`; a text without the line gives `None`.
   - `scale_runs_end_to_end` (in `xtask/tests/scale.rs`; `#[ignore]`: it needs a built `fieldbook`): run `cargo build --locked -p fieldbook`, then the built `xtask` (`env!("CARGO_BIN_EXE_xtask")`) with `scale --sizes 1,2 --base xtask/tests/scale-tiny.toml --bin target/debug/fieldbook` twice, with the repository root (`CARGO_MANIFEST_DIR/..`) as the working directory. Both exit 0; the output has the header and the rows `1x` and `2x`; the `units`, `corpus_kb` and `index_kb` cells are equal in the two runs; no folder named `fieldbook-scale-*` remains in the temporary directory.
10. Update `AGENTS.md` (Commands) with the command and the words "advisory, not part of `just check`", keeping the file within 100 lines.

11. The R2 multi-year workload is already specified in [Company browser growth](../reference/search.md#company-browser-growth); this card makes no edit to that recipe. The company records card fills it once record/observation types exist; this card keeps the Day-1 scale run unchanged. Never treat a Day-1 Corpus multiplier as evidence for company-history capacity.

12. Remove `sections_per_page` from `Base`; derive required parts from the page type and report actual section counts. `words_per_section` is a ceiling for filler, after required text. Count headings and citations within the brief’s 400-word body limit. Keep `Sources` empty on types that have it; briefs have no such part. Use checked multiplication and validate every ID namespace, including claims per area, before creating files. Generation returns a validation error and the CLI exits 2 on excess capacity. Test question counts 999 and 1,000 and an oversized CLI run that writes nothing. The default 20× run has 980 questions. The determinism test compares two consecutive equal-input calls; the invocation-counter plant must fail it.

## Acceptance

```sh
cargo nextest run --locked -p xtask scale::
cargo build --locked -p fieldbook
cargo nextest run --locked -p xtask --test scale --run-ignored only scale_runs_end_to_end
cargo xtask scale --sizes 1,2 --base xtask/tests/scale-tiny.toml --bin target/debug/fieldbook; echo "exit $?"
cargo xtask scale; echo "exit $?"
just check
```

Expected: every test passes, the ignored one included; the tiny run prints `scale base=xtask/tests/scale-tiny.toml seed=7`, the header, the rows `1x` and `2x`, and `exit 0`; the full run (a release build, then the three sizes) prints the rows `1x`, `5x` and `20x`, the two `NFR-F-04` lines and `exit 0`, whatever the figures are; `just check` exits 0 and runs none of this. A figure over a budget is not a failure here: copy the table into the work item for the lead.

## Negative control

In `generate`, make the first claim name a source version that does not exist: `generated_corpus_is_clean_under_strict` must fail and name FB0207. Restore it. Then append a process-wide monotonically increasing invocation counter to the generated text: `generation_is_deterministic` must fail. Restore it. Then copy the SplitMix64 constant into `scale.rs`: `one_rng_exists` must fail and name the file. Remove it. Then run `cargo xtask scale --sizes 1 --base xtask/tests/scale-tiny.toml --bin /bin/false`: it must exit 1 and print `scale: check failed on the generated Corpus (1x), exit 1`, and leave no `fieldbook-scale-*` folder behind.

## Rollback

Revert the merge commit. `cargo xtask scale` is then an unknown command. If `scheduled-workflow` has merged, revert it too, or its `scale` job fails because the command cannot run. Nothing else changes: no shipped crate, no Corpus file and no register reads this card.

## BLOCKED

The generated Corpus fails `check --strict` for a reason the generator cannot avoid (a rule that rejects every shape it may write): write the code and the rule, and stop. Or the release build, or the 20x run, exceeds the 30 minutes of the weekly job on the runner: write the time of each step and stop; the lead lowers the counts in `scale/base.toml` or raises the job's timeout. Do not turn a figure into a failure.

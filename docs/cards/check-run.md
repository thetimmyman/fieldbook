# check-run

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook_core::check::load` takes a Corpus as a value and returns the `Model` and a `Report`; `check::run` returns the `Report` of `load`, and `Model` is exported with private fields. It reads `corpus/corpus.toml` first and raises FB0100 for a missing, unreadable or malformed file. The later checks plug into a fixed list of passes.

## Why

- The libraries take values and bytes; only `fieldbook` does I/O ([AD-05](../decisions/0001-foundation.md#ad-05-crate-layout-and-enforcement)). The Corpus therefore arrives as a `Tree` of bytes.
- FB0100 is defined in [01: pages, corpus.toml and files](../reference/corpus-format.md#01-pages-corpustoml-and-files). The strict rules and the clock are described in [Dates](../reference/corpus-format.md#dates).

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,tree.rs,model.rs,corpus_toml.rs}`, `crates/fieldbook-core/src/hash.rs`, `crates/fieldbook-core/src/lib.rs`. `check/corpus_toml.rs` holds the FB0100 pass and imports `schema::CorpusToml` from `core-schema-pages`.
- Docs to update: none needed.

## Out of scope

- Reading files and the command line (`check-command`).
- Any code other than FB0100.

## Preconditions

- Cards merged: `check-v0`.

## Steps

1. Write `Tree`: an ordered map from a `/`-separated path relative to the repository root to an `Entry`, which is `Bytes(Vec<u8>)` or `Original { sha256, len }` (a file under `corpus/sources/public/` that the reader hashed without keeping its bytes). Keep an ordered directory set independently of file entries, including empty directories. Add `get`, `paths` (in byte order), `contains_dir(path)` and `with_prefix(prefix)`; a subtree retains its directory entries. In-memory fixtures declare their directories explicitly.
2. Write `hash::Sha256Stream` (`new`, `update`, `finish() -> String` in lowercase hex through `hex::lower`) so the binary can hash an original while it reads it.
3. Write `Host { tree, today: CivilDate }` and `Options { strict }`. No clock is read in core; `today` is a value.
4. Write `Model`, the parsed state the passes share. It is built once. `load` copies `tree` and `today` out of the `Host` into it (`Tree` and `Entry` derive `Clone`), and it also holds `corpus: Option<CorpusToml>`, with the accessors `tree(&self) -> &Tree`, `today(&self) -> CivilDate` and `corpus(&self) -> Option<&CorpusToml>`. A pass reads the files, today's date and `corpus.toml` only through these three accessors and never takes the `Host`. Each later check card adds only the parsed pages or registers it needs, and the accessor that a caller outside `check` needs. `check/mod.rs` exports it with `pub use model::Model`; its fields stay private. The render crate reads the Model only through these accessors and never parses a file.
5. Write `check::load(&Host, &Options) -> (Model, Report)`. It builds the model, raises FB0100 at `corpus/corpus.toml` for a missing or unreadable file, a parse error (with the line and column of the error), a problem from `validate()` and a `schema_version` above the supported one (help: this tool reads schema 1), and then runs the list of passes in order. A pass is a function `fn(&Model, &Options, &mut Report)`; the list is empty. When FB0100 fires, the passes do not run, because they would only repeat it. Write `check::run(&Host, &Options) -> Report` as `load(..).1`; `build` calls `load`, and every other caller calls `run`.
6. Tests, with small in-memory trees: `empty_directory_is_distinct_from_absent` (including a subtree with no files); `clean_corpus_toml`; `missing_file_is_fb0100`; `syntax_error_has_line_and_column` (a syntax error on a known line); `unknown_key_in_an_area`; `repeated_area_code`; `schema_version_above_the_supported` (`schema_version = 9`); `two_runs_give_equal_reports`; `run_equals_load_report`; `the_model_returns_its_tree_and_today` (a `Host` with a known tree and date: `Model::tree()` and `Model::today()` return them); and `a_broken_corpus_toml_stops_the_passes` (`load` gives its list of passes to a private function that takes `&[Pass]`, so the test can give it a pass that records that it ran, and the record must stay empty).

7. Add `check::pass_notes_survive_load_and_run`, injecting a pass that appends a note to Report. Require identical notes from load and run, the note in rendered text and unchanged diagnostic/error/warning counts. Keep typed Model accessors immutable.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check:: hash::
just check
```

Expected: both succeed; the run includes one test for each FB0100 case of step 6, and `the_model_returns_its_tree_and_today`.

## Negative control

Let `run` continue to the passes after FB0100. The test `check::a_broken_corpus_toml_stops_the_passes` must fail (use a test pass that records that it ran). Restore the line. Then make `load` store a fixed date in the `Model` instead of the date of the `Host`: `check::the_model_returns_its_tree_and_today` must fail. Restore the line.

Drop pass notes when returning or rendering Report: `check::pass_notes_survive_load_and_run` must fail. Restore it.

## Rollback

Revert the merge commit. Nothing outside `fieldbook-core` uses `run` yet.

## BLOCKED

None expected. Write BLOCKED if the real `corpus/corpus.toml` is reported as FB0100: the file is the lead's, so do not edit it.

# index-output-ports

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`build` now produces the edition through one trait, `OutputPort`, with five registered renderers (`site@1`, `xlsx@1`, `csv@1`, `binder@1`, `decision-package@1`), and `build --only ID` runs one of them. The search side gains `merge_exact_first`, the only place where hit lists from more than one index are combined, and `checked_query`, which drops a hit whose id is not in the Corpus (FB0930). Not one byte of any output changes: the tree hashes of a `corpus-mini` build, recorded before the first change, are the proof. `semantic@1` is registered as a Later row.

## Why

- A second index or a second output should be one registered row and not a rewrite, and no adapter may reorder or remove the exact tier of a result: the merge builds the final list, not the adapter. This is a Could, the first card to cut; if it is cut, `build` and `search` keep calling their code directly, and nothing built after it depends on it.
- The merge is the rule of [Ranking](../../reference/search.md#ranking) extended to several lists: exact ids first and in their order, then the rest by integer reciprocal-rank fusion. With no extra list the output equals the primary list, which is what makes adding the trait free. Nothing calls the merge with a second index yet; the tests are its callers until `later-semantic-search` is built, and that is the cost of this card.
- `semantic@1` is a Later row of the registry, so `ports.md` states what it would be and when it comes, and `[search] semantic = true` is refused as FB0900 until a row is `Built`. The trigger and the candidate wires are in the AD-24 row `later-semantic-search` ([What is not built yet](../../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later)); its way back is to delete the row.
- A renderer is a pure function of the LabelledView, typed Assistant input, the edition stamp and `SOURCE_DATE_EPOCH`: no clock, no environment, no file, no model. The static edition never contains a model's text unless a person accepted it into a file the Model already holds. FB0931 is raised by the adapter contract suite when one adapter gives two different byte strings for one input, and the double-build test (FB0606) stays.
- No page, template or string shown in a browser is built here, so there is no Design bullet. The chip for `build` is unchanged and `--only` has none.

## Scope

- Files: `crates/fieldbook-core/src/ports/merge.rs`, `crates/fieldbook-core/tests/merge.rs`, `crates/fieldbook-render/src/out/{mod.rs,site.rs,xlsx.rs,csv.rs,binder.rs,decision.rs}`, `crates/fieldbook-render/src/lib.rs`, `crates/fieldbook-render/tests/{out.rs,contract_output.rs,support/output_suite.rs}`, `crates/fieldbook/src/{lib.rs,cli.rs,build_ports.rs}`, `crates/fieldbook/tests/build_ports.rs`, `tests/fixtures/ports/output/{site@1,xlsx@1,csv@1,binder@1,decision-package@1}/`, `tests/golden/out/{corpus-mini-silent-sums.txt,corpus-mini-ai-sums.txt}`, `tests/known-bad/{FB0930,FB0931}.toml`, `tests/known-bad/pending.toml` (the two rows are deleted), `CHANGELOG.md`.
- Registration lines: the five Output rows and the Later row of `semantic@1` in `ADAPTERS` (`crates/fieldbook/src/adapters/registry.rs`); the `pub mod merge;` line in `crates/fieldbook-core/src/ports/mod.rs`; the call in `crates/fieldbook/src/build.rs` that replaces the direct renderer calls with `build_ports::run`; `docs/reference/search.md` (Ranking: one sentence for the merge and its constants).; `crates/fieldbook/src/io/out.rs` (non-removing partial writer and selected-file delivery binding); `crates/fieldbook/tests/build.rs` (full-build then partial-build preservation)
- Docs to update: `docs/reference/ports.md` through `cargo xtask docs`, `docs/reference/cli.md` through `cargo xtask docs` (the `--only` flag), `docs/reference/search.md` (Ranking: one sentence for the merge and its constants), `CHANGELOG.md` (`- Added: build --only ID; the Index and Output ports, with byte-identical output.`).

## Out of scope

- Any second index, embeddings, or a change to the stored index format (`later-semantic-search`). The static edition stays exact-only offline and ranks in its own JavaScript, which `edge-harness` compares with the Rust ranking.
- The decision spec and the AI sheet (`decide-port`), and any other variant of `Extra`: `Extra` has one variant, `None`, and is `#[non_exhaustive]`, so a later card that has a payload to carry adds its own variant. `decide-port` builds its sheet with `add_ai_sheet` and adds none.
- New outputs (`deck`, `letter`, `dashboard`) and any change to a renderer, template or asset.
- The callers of `merge_exact_first` and `checked_query`, and the `IndexPort` types and `bm25f@1` themselves (`pipeline-ports`, `pipeline-correlate`).

## Preconditions

- Cards merged: `ai-kernel`, `assistant-page` (the typed Assistant render input), `search-cli-and-regression`, `decision-package`, `pipeline-ports` (the `IndexPort` types), `pipeline-correlate` (`Bm25Index`), `build-v0`, `registers-xlsx` and `day-one-binder-and-registers-xlsx` (the renderers the wrappers call), `check-known-bad` (the harness with its skip of an `in_process` file and its pointer test, and the two rows of `pending.toml` that step 10 deletes), and `calc-day1` (the seeded generator `tests/support/rng.rs` that step 4 includes).

## Steps

1. Record the baseline before any other change, in its own commit. A test helper `tree_sums(dir)` lists every file of a folder as `<sha256>  <relative path>`, sorted by path. `record_profile_tree_sums` (marked `#[ignore]`, run once with `--run-ignored only`) builds `corpus-mini` with `SOURCE_DATE_EPOCH=1700000000` into a temporary folder (`--out DIR` writes the site under `DIR/edition`) and writes separate lists of `DIR/edition`, selected by the actual compiled Silent/Ai profile, to `tests/golden/out/{corpus-mini-silent-sums.txt,corpus-mini-ai-sums.txt}`. The test `build_matches_the_recorded_sums` in `tests/build_ports.rs` repeats the build and compares. Run and verify both profiles in this baseline commit before wrapping, then on every later commit. Pin the tool version to this baseline commit’s workspace version, BuildInfo commit to 1111111111111111111111111111111111111111, epoch to 1700000000, the fixture TODAY and workbook epoch. FIELDBOOK_GIT_SHA is a compile-time environment input, so set it before each profile build. In the baseline commit run `FIELDBOOK_GIT_SHA=1111111111111111111111111111111111111111 cargo nextest run --locked -p fieldbook --test build_ports --run-ignored only record_profile_tree_sums` once with default features and once with `--all-features`; never run the recording helper during ordinary Acceptance. The helper fixes runtime dates/epochs itself. Binary tests select their actual profile; never force Silent or strip profile bytes.
2. Core, `ports/merge.rs`: `merge_exact_first(primary: Hits, extra: &[Hits], limit: usize) -> Hits`. (1) The hits of tier `Exact` in `primary` keep their order and come first. (2) Every other unit of every list is scored by integer reciprocal-rank fusion: a hit at rank r in a list, counted from 1 in the list it came from with the exact tier included, adds `1_000_000 / (60 + r)` by integer division; scores add across lists; ties break by unit id in byte order. (3) A unit already in tier one is removed from the rest. The tier of a hit in an extra list is ignored, and fused hits are `Ranked`, so no extra list can claim the exact tier. (4) The limit is applied last. With no extra list the result equals `primary` cut to the limit. The function is pure and total.
3. Core, `ports/merge.rs`: `checked_query(index: &dyn IndexPort, q: &Query, known: &dyn Fn(&UnitId) -> bool) -> Result<(Hits, Vec<Finding>), IndexError>`. It calls the adapter, drops each hit whose id `known` rejects, and returns one FB0930 warning for each dropped id, naming the adapter and the id. An `IndexError` passes through. `ScriptedIndex::new(id, hits)` returns the same hits for every query and counts its calls; it is test-only data in `crates/fieldbook-core/tests/merge.rs`, never in production ports.
4. Core tests, `tests/merge.rs`. A worked case with hand-computed numbers: primary `a` (exact), `b`, `c`; extra `c`, `d`; the result is `a`, then `c` at 15873 + 16393 = 32266, then `b` at 16129 and `d` at 16129 in id order. Then: `no_extra_lists_equals_the_primary_list` and `exact_tier_is_a_prefix_of_any_merge` (each over 500 lists made from `Rng::new(1)` of `tests/support/rng.rs`, included in `tests/merge.rs` with `#[path = "../../../tests/support/rng.rs"] mod rng;` and written nowhere else (`one_rng_exists`), so there is no new dependency), `fusion_uses_integer_division`, `ties_break_by_unit_id`, `unit_in_tier_one_is_removed_from_the_rest`, `extra_exact_tier_is_ignored`, `limit_is_applied_last`, `merge_is_deterministic`, `checked_query_drops_unknown_ids_with_fb0930`, `checked_query_keeps_known_ids_in_order` and `index_error_passes_through`.
5. Render, `out/mod.rs`: `trait OutputPort { id, render(&self, &OutputInput) -> Result<Vec<OutFile>, OutputError> }`; `OutputInput { view: LabelledView, assistant: AssistantRenderInput, context: RenderContext, source_date_epoch, extra }`; `#[non_exhaustive] enum Extra { None }`; `OutFile { path: OutPath, bytes }`, where `OutPath::new` refuses an empty path, a leading slash, a backslash, a `..` part and a Windows drive prefix (both absolute and drive-relative), independently of the host; `OutputError`. `default_set()` returns the five adapters in the order site, xlsx, csv, binder, decision-package. `render_all(set, input, only)` calls each adapter, sorts the files by path, and refuses two adapters that write one path (`OutputError::Collision`). An `only` that is not an id is `OutputError::UnknownId`, which lists the ids. Tests in `tests/out.rs`: `out_path_refuses_empty_leading_slash_backslash_and_dotdot` (one case for each shape, and `a/b.txt` accepted), `render_all_sorts_files_by_path`, `render_all_refuses_two_adapters_that_write_one_path` (`OutputError::Collision` names both ids and the path) and `unknown_only_id_lists_the_ids`.
6. The five wrappers, one file each under `out/`. A wrapper calls the function that `build` calls today for that output and turns its result into `OutFile`s; it holds no rendering logic and no copy of any. `site@1` delegates its page, asset and metadata paths to the shared RenderContext; its current Public variant includes `edition.toml`; `xlsx@1` is `data/registers.xlsx`; `csv@1` is the five `data/<register>.csv` files; `binder@1` is `day-one.html`; `decision-package@1` is the file or files that `decision-package` writes, with `Extra::None`. If a wrapper cannot reproduce the recorded sums without a change in the renderer, stop (see BLOCKED).
7. Binary, `build_ports.rs` and `cli.rs`: `build [--out DIR] [--only ID]`. `run` loads the Model, selects the destination-filtered LabelledView as build does, constructs OutputInput with that view and the separately authorized AssistantRenderInput from the existing build factory, calls render_all and writes the files in path order through `io/out.rs` under `DIR/edition`. `--only` calls a new non-removing `io::out::write_partial` operation over the existing audited atomic writer. It validates the existing output-root ownership marker and binds selected file bytes, marker and required mkdir operations to the exact Delivery; it never invokes replace or removes an unrelated file. Ordinary builds retain whole-root replacement. An unknown id prints `unknown output adapter nope@1; ids: site@1, ...` and exits 2. `build` stays a deterministic verb on the plain `Ctx` and names no model type (DT-05 and mechanism M3 hold).
8. Suite O, `tests/support/output_suite.rs`: `output_suite(make)` requires, for explicit Silent and Ai `corpus-mini` inputs with fixed build metadata and fixture clock, that two calls give the same files byte for byte (the failure message begins `FB0931`), that the files come back sorted by path, that every path is an `OutPath`, that no path is also written by another adapter of the default set, and that a call with a different epoch changes only files that embed a time. `contract_output_site_1`, `contract_output_xlsx_1`, `contract_output_csv_1`, `contract_output_binder_1` and `contract_output_decision_package_1` run it. Each `tests/fixtures/ports/output/<id>/expect.toml` has silent and ai expected file/hash sets, sharing equal values where appropriate; `fixtures_agree_with_the_recorded_sums` requires the five lists together to equal the matching profile golden of step 1. Keep rendered_profile_matches_build_features unchanged.
9. The planted adapter. In `contract_output.rs`, `PlantedClock` is an `OutputPort` that appends a counter to one file; `planted_clock_adapter_is_fb0931` requires `output_suite` to fail on it with FB0931.
10. Registry and known-bad. Add the rows as listed under Registration lines: each Output row is `Built`, `Silent`, crate `fieldbook-render`, `Needs::None`, `sees` labelled, `calls_model` false, no settings keys, suite `output_suite`, and `default` true because the five are the default set. The `semantic@1` row is port Index, `Later`, `Ai`, `Needs::Net`, `sees` labelled, `calls_model` true, the key `search.semantic`, no fixtures and no suite, and its doc line names `later-semantic-search`. `tests/known-bad/FB0930.toml` and `FB0931.toml` have no verb to run, because the codes are raised by a scripted index and by a planted adapter; each holds `code`, `in_process = "<crate>/<test file>/<test name>"` (the key is defined under [Known-bad files](../../reference/corpus-format.md#known-bad-files); `fieldbook-core/merge/checked_query_drops_unknown_ids_with_fb0930`, `fieldbook-render/contract_output/planted_clock_adapter_is_fb0931`) and the expected code. Delete the two rows of `pending.toml`.
11. Run `cargo xtask docs` and commit `ports.md` and `cli.md`. `adapters_match_registry_docs_and_fixtures` must pass with the new rows.

12. An OutputPort receives only LabelledView and typed output options, never an unfiltered Model or Tree. Pure render output is then scanned and bound to a concrete Delivery before its sink. Add `output_port_cannot_read_hidden_canary` to the output suite and use it in every adapter fixture.

13. `out_path_refuses_drive_prefixes_on_every_host` rejects `C:/escape.txt`, `C:escape.txt` and lowercase equivalents while accepting `a/b.txt`. The Windows output-boundary case verifies containment below the selected edition root and retains canonical-target and delivery checks.

14. In the real build suite, `partial_output_preserves_unrelated_files` first performs a full build, records every file hash, then runs --only xlsx@1 into the same owned root. Verify selected workbook output and byte-identical preservation of every unselected file, plus foreign-root refusal before mutation.

15. Preserve the Assistant input while wrapping the site renderer. Run the real-build independent Handoff/Clipboard cases from `assistant-page` through `site@1`; emitted sets, availability, expiry, hash and banners remain byte-identical. No OutputPort receives an unfiltered Model.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test merge
cargo nextest run --locked -p fieldbook-render --test out --test contract_output
FIELDBOOK_GIT_SHA=1111111111111111111111111111111111111111 cargo nextest run --locked -p fieldbook --test build_ports --test adapters --test known_bad
cargo build -q --locked -p fieldbook && B="$PWD/target/debug/fieldbook" && T="$(mktemp -d)" && cp -R tests/fixtures/corpus-mini/. "$T"
(cd "$T" && SOURCE_DATE_EPOCH=1700000000 "$B" build --out "$T/a" && SOURCE_DATE_EPOCH=1700000000 "$B" build --out "$T/b" && diff -r "$T/a" "$T/b" && echo builds equal)
(cd "$T" && SOURCE_DATE_EPOCH=1700000000 "$B" build --only xlsx@1 --out "$T/c" && find "$T/c" -type f | sort)
(cd "$T" && "$B" build --only nope@1 --out "$T/d"; echo "exit $?")
cargo xtask docs --check
cargo nextest run --locked -p fieldbook --test build
FIELDBOOK_GIT_SHA=1111111111111111111111111111111111111111 cargo nextest run --locked -p fieldbook --all-features --test build_ports
just check
```

Expected: the tests pass; the two builds print `builds equal`; the `--only xlsx@1` run writes one file, `registers.xlsx` under `edition/data/`; the unknown id prints the ids and `exit 2`; `docs --check` prints `docs: up to date`; `just check` exits 0 and its step for the edition goldens is unchanged. `ports.md` lists the five Output rows and `semantic@1` as Later. Remove the temporary folder afterwards.

## Negative control

On the branch, reverse `csv@1`’s returned files and require `contract_output_csv_1` to fail its sorted-order assertion; separately change one CSV cell’s bytes, or round one cell differently: `build_matches_the_recorded_sums` must fail and name the path. Restore it. Let an extra list's hit of tier `Exact` come first: `extra_exact_tier_is_ignored` must fail. Restore it. Round the fusion with `+ 30` before the division: `fusion_uses_integer_division` must fail. Restore it. Keep an unknown id in `checked_query`: `checked_query_drops_unknown_ids_with_fb0930` must fail. Restore it. Add a counter to the output of `xlsx@1`: `contract_output_xlsx_1` must fail with FB0931. Restore it. Make `output_suite` skip the second call: `planted_clock_adapter_is_fb0931` must fail. Restore it. Then let `OutPath::new` accept `a/../b`: `out_path_refuses_empty_leading_slash_backslash_and_dotdot` must fail. Restore it. Last, let `render_all` keep both files of a collision: `render_all_refuses_two_adapters_that_write_one_path` must fail. Restore it.

Remove the `csv@1` row from `ADAPTERS`: `adapters_match_registry_docs_and_fixtures` must fail. Restore it. Do the same for the `site@1`, `xlsx@1`, `binder@1` and `decision-package@1` rows.

Pass the full model to an output adapter: `output_port_cannot_read_hidden_canary` must fail. Restore it.

Remove drive-prefix rejection: `out_path_refuses_drive_prefixes_on_every_host` must fail. Restore it.

Route --only through replace: `partial_output_preserves_unrelated_files` must fail. Restore it.

Substitute Edition permissions for AssistantRenderInput in site@1: the real-build independent destination-permission cases must fail. Restore it.

Run the changed-CSV negative control against both profile baselines; each must fail. Restore the CSV. Removing a profile comparison or forcing Silent output must fail the profile test.

## Rollback

Revert the merge commit. `build` calls the renderers directly again (revert the line in `build.rs` with it), `--only` goes, the five Output rows and the Later row leave `ADAPTERS`, `ports.md` and `cli.md` regenerate, and the two rows return to `pending.toml`. No output byte ever changed, so no edition is affected. The AD-24 row `later-semantic-search` is not edited by this card, so the revert does not touch it.

## BLOCKED

`IndexPort`, `Bm25Index` or one of the renderers is not on `main`, or a wrapper cannot give the recorded bytes without a change in the renderer it wraps. Name which one and stop. Do not change a renderer to fit its wrapper, do not edit the recorded sums to match, and do not write a second index type here.

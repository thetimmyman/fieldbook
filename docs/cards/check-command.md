# check-command

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook check [--strict] [--today YYYY-MM-DD] [--json] [paths]` reads the Corpus from disk, runs `fieldbook_core::check::run` and prints the report. The exit code is 0 with no errors, 1 with at least one error, 2 for a usage error and 3 when a file cannot be read. `--version` prints what it printed before. A test requires that every verb the program defines has a chip in the verb map, and another that the verbs of the clap definition and the `verbs` block under the AD-17 table are the same list.

## Why

- Flags, output form and exit codes are in [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience). Only `fieldbook` reads files, the environment and the clock ([AD-05](../decisions/0001-foundation.md#ad-05-crate-layout-and-enforcement)).
- The AD-17 table is a decision record that a person edits; the clap definition is the program. `verbs_match_ad17` ties the two through the fenced `verbs` block under the table, so a verb cannot be added to one and not the other ([Adding a verb](README.md#adding-a-verb)).
- The date comes from `--today` or the clock, and a strict rule is a warning without `--strict` and an error with it ([Dates](../reference/corpus-format.md#dates)).

## Scope

- Files: `crates/fieldbook/src/{lib.rs,cli.rs}`, `crates/fieldbook/src/io/{mod.rs,tree.rs}`, `crates/fieldbook-core/src/report.rs`, `crates/fieldbook/tests/{cli.rs,chips.rs,json_reports.rs}`, `tests/golden/json/check.json`, `CHANGELOG.md`. The manifest already holds the `clap`, `serde_json`, `insta` and `tempfile` edges from `day1-dependencies`.
- Registration lines: the `pub mod report;` line in `crates/fieldbook-core/src/lib.rs`; the Commands line of `AGENTS.md`; the quick start line about `fieldbook check` in `README.md`.
- Docs to update: `AGENTS.md` (Commands: say that `fieldbook check` is available), `README.md` (the quick start line about `fieldbook check`), `CHANGELOG.md` (`- Added: the check command.` under `## Unreleased`).

## Out of scope

- The known-bad harness (`check-known-bad`) and `cli.md` (`check-ci-and-docs`).
- Any check except FB0100.
- The other verbs. An unknown verb is a usage error.

## Preconditions

- Cards merged: `check-run`.

## Steps

1. Add the `clap` derive `Cli`. `check` takes `--strict`, `--today <YYYY-MM-DD>` (parsed with `CivilDate`), `--json` and any number of paths. Keep the `--version` line byte for byte. A usage error prints clap's message on standard error and exits 2.
2. Change `run` to `run(args, &dyn World)`. `World` has `read_tree() -> Result<Tree, WorldError>` and `today() -> CivilDate`, plus `cwd(&self) -> &Path` and `tree_root(&self) -> &Path`. Disk captures cwd once and the root returned by find_root; the root is the directory passed to read_tree, containing corpus/. The private double supplies both paths. `lib.rs` stays free of file access; the existing tests pass a private double of `World` in the tests of `lib.rs`, which stays; the shared `TestWorld` of the verb tests is made by `audit-journal`.
3. In `io/tree.rs`, add two free functions, and re-export them from `io/mod.rs` so that tests and later cards call `io::find_root` and `io::read_tree`. `find_root(start: &Path) -> PathBuf` walks up from `start` to the nearest ancestor containing a `corpus/` directory, even if `corpus.toml` is missing (`start` itself if none does). `read_tree(root: &Path) -> Result<Tree, WorldError>` records every directory, including empty ones, and reads every file under `corpus/` into a `Tree`, skipping `.git`; a file under `corpus/sources/public/` becomes an `Original` hashed with `hash::Sha256Stream` while it is read. A read error exits 3 with the path. In `io`, add `Disk`, which implements `World`: its `read_tree` is `read_tree(&find_root(cwd))`. `build` uses `find_root` for its default output folder. Tests in `tests/cli.rs`: `read_tree_of_a_given_folder_matches_the_fixture` and `find_root_walks_up` and `empty_originals_directory_is_retained`. `today` reads the clock and converts it with `CivilDate::from_epoch_seconds`.
4. Print the text report on standard output (`Report::render_text`). A path argument keeps only the diagnostics under that path (relative to the working directory); the whole Corpus is still read and checked. Resolve filters against captured cwd and diagnostic paths against tree_root, lexically normalize `.` and `..` without requiring a path to exist, then compare whole path components for equality or descent. Filter before text/JSON counts and exit status are computed. Exit 1 when at least one error remains, otherwise 0; warnings are printed and do not change the exit code.
5. `--json` prints one object and nothing else: `report_version` (1), `strict`, `today`, `errors`, `warnings`, `notes` (a list of strings) and `diagnostics`, each with `code`, `severity`, `path`, `line`, `col`, `message` and `help`. Write the report as the struct `CheckReport` in `crates/fieldbook-core/src/report.rs` (`Serialize` only, `report_version` first; add `pub mod report;` to `lib.rs`) and follow [Adding a JSON report](README.md#adding-a-json-report): create `crates/fieldbook/tests/json_reports.rs` with the table of reports (one row, `check`) and the test `json_goldens_match_reports`, and pin the output with the golden `tests/golden/json/check.json`. No `insta` snapshot is used. `cargo xtask docs` does not list JSON reports yet; `check-ci-and-docs` adds that section and reads this golden.
6. Remove `check` from the usage-error tests in `lib.rs` and `tests/cli.rs`, and add process tests. A process test that runs `check` on the fixture reads its date from `tests/fixtures/corpus-mini/TODAY` (`include_str!("../../../tests/fixtures/corpus-mini/TODAY")`, trimmed) and passes it as `--today`; none reads the clock. The tests: `check` in an empty temporary folder exits 1 and prints `FB0100`; `check --today <TODAY>` in `tests/fixtures/corpus-mini` exits 0 and prints `clean`; `check --json --today <TODAY>` there prints valid JSON; a bad `--today` exits 2; `--strict --today <TODAY>` is accepted.
7. `every_verb_has_a_chip` in `tests/chips.rs` (a new file here; this card also adds the chip-parse test and its `PENDING` list): for each leaf subcommand of the clap command (export `fieldbook::cli` from `lib.rs` if it is private; skip clap's own `help`), require a chip in `strings::chips` whose verb is that subcommand, or a name in `NO_CHIP`. `NO_CHIP` is the 14 verbs that the verb map in [Strings](../reference/corpus-format.md#strings) lists as having no chip: `audit show`, `audit export`, `backup list`, `queue list`, `queue show`, `queue ingest`, `queue pin`, `ai test`, `policy check`, `label add`, `label reapply`, `label show`, `mcp` and `chat`. An entry for a verb the program does not define yet is allowed. A verb added later without a chip, and not in `NO_CHIP`, fails here; the chip-parse test below covers the other direction.
8. `verbs_match_ad17` in `tests/cli.rs`: read the fenced block with the info string `verbs` under the AD-17 table in `docs/decisions/0001-foundation.md` with `include_str!`. It lists the leaf verb paths one per line (`check`, `audit verify`, `queue accept`). Compare it with the leaf commands of the clap tree: a command with `subcommand_required` is a group and not a verb, and clap's own `help` is skipped. A verb of the block that the program does not define yet is listed in a `NOT_BUILT` constant in the test, which starts as every line of the block except `check`. The test fails on a clap leaf command that is not in the block, on a line of the block that is neither defined nor in `NOT_BUILT`, on a `NOT_BUILT` entry that is not in the block, and on a `NOT_BUILT` entry whose verb the program now defines, with the message `remove <verb> from NOT_BUILT`. A flag such as `--propose` or `--watch` is not a verb and is not in the block.

9. Add `pass_notes_survive_json` in `tests/cli.rs`: the check report note survives JSON serialization without increasing diagnostic counts. Dropping notes from JSON must fail it.

10. Add `missing_nested_corpus_config_does_not_select_ancestor` in `tests/cli.rs`: a valid outer Corpus surrounds an inner `corpus/` lacking its configuration. Running from the inner tree must report FB0100 and exit 1, naming the inner configuration.

11. Bootstrap the chip parser here: fill typed placeholders with synthetic values and parse every full chip through clap. Keep unbuilt chip keys in `PENDING`; a pending chip that now parses fails, and later verb cards remove their entries. This enables CLI-only consumers before About exists. Test a new leaf with no chip and a stale pending chip; each must fail. `build-about` later reuses this test without resetting the list.

12. In `tests/cli.rs`, `check_paths_are_relative_to_cwd` uses the same malformed or missing corpus/corpus.toml from the root and from corpus/: `check corpus.toml` excludes FB0100 in the first case and includes it in the second, in text and JSON with matching counts/exits. `path_filter_uses_components` covers exact files, normalized dot components and partial-component nonmatches.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook
cargo run -q --locked -p fieldbook -- check --today "$(git log -1 --format=%cs)"; echo "exit $?"
(cd tests/fixtures/corpus-mini && cargo run -q --locked -p fieldbook -- check --strict --today "$(cat TODAY)"; echo "exit $?")
just check
```

Expected: the tests pass; the first `check` prints its summary and `exit 0`; the second prints `clean` and `exit 0`; `just check` exits 0.

## Negative control

Delete the `corpus.toml` of a temporary copy of the fixture and run `check` there. It must exit 1 and print an FB0100 line that starts `corpus/corpus.toml:1:1: error[FB0100]`. Then pass `--today 2025-02-30`: it must exit 2. Remove the copy. Add a subcommand `hello` to the clap command with no chip: `every_verb_has_a_chip` and `verbs_match_ad17` must fail and name `hello`. Remove it. Then delete the line `check` from the `verbs` block of ADR 0001: `verbs_match_ad17` must fail and name `check`. Restore the file.

Restore configuration-file-based root discovery: `missing_nested_corpus_config_does_not_select_ancestor` must fail. Restore it.

Ignore cwd, then use string-prefix matching: `check_paths_are_relative_to_cwd` and `path_filter_uses_components` must fail respectively. Restore each.

## Rollback

Revert the merge commit. `check` becomes a usage error again.

## BLOCKED

`check` reports FB0100 on the real `corpus/corpus.toml`. The file is the lead's: write the line and stop.

# build-watch

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build --watch [--out DIR]` builds the edition once, then checks the Corpus folder every 500 ms and builds again after each change that has settled, until the owner presses Ctrl-C. Each build prints one line, `built` or `build failed`, and a build that fails (a check error, a refusal, a busy store) leaves the earlier `DIR/edition` as it was and the watch carries on. Each rebuild gives the bytes that a plain `fieldbook build` of the same files gives, and `build` without `--watch` behaves exactly as it does today.

## Why

- The flag and its delay are decided: [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience) lists `build --watch` among the Day-30 additions and the CLI table says "(500 ms polling)". The row of [dependencies.md](../reference/dependencies.md#in-house-instead-of-a-crate) writes the watcher by hand in the `io` module of `fieldbook` and bans `notify` and `watchexec`; its "Replaces" list also says "polling" for a sorted `read_dir` walk because the output order must be deterministic and no background process may run. This card adds no crate.
- The watch is a loop in the one process, not a second process or a listener: the hard stops of [AGENTS.md](../../AGENTS.md) allow process code only in `xtask` and the only listener is the loopback Bridge. Nothing in this card opens a socket, a thread or a child process.
- `build` is a deterministic verb ([AD-01](../decisions/0001-foundation.md#ad-01-architecture)); the watch never reaches a model and reads no clock of its own. The pause between polls is a seam passed in, so a test never sleeps and the binary is the only place that does. `std::time::Instant::now` and `SystemTime::now` stay banned by `clippy.toml`: the poll compares file facts, never reads the time.
- Every rebuild is an ordinary run of `build::run_with_info`: its journal events, its store lease, its FB0600 refusal, its `held <version-id>` lines and its atomic replace of `DIR/edition` are unchanged ([audit-contract.md](../reference/audit-contract.md)). The watch holds no lease between polls, so `add` or `new` can run in another terminal while it waits.
- Lead decisions where the repository is silent. (1) The watched set is `corpus/` under the tree root, without `.git`; `out/` is not watched, so a build never wakes itself, and files outside `corpus/` (`edition.toml` is written by `build`) are not input. (2) A change is "settled" when two polls in a row give the same fingerprint, so a save in two steps (an editor writing a temporary file, then renaming it) causes one build. (3) A failed build does not end the watch, including the first one: the owner is watching in order to fix the file, and the failure text is the same text `build` prints. (4) The process has no signal handler: Ctrl-C ends it as it ends `build`, and the replace of `DIR/edition` is already safe at any point. (5) There is no new chip: `build --watch` is a flag of the `build` verb that [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience) already names, so the Verb map and `PENDING` of `chips.rs` do not change.
- This is a Could ([Cut line and floor](../decisions/0001-foundation.md#cut-line-and-floor)); it is also a P4 item with no other card waiting on it, so it can be cut without effect.
- Design: none. The card builds nothing a reader sees; `--watch` prints two fixed lines on a terminal.

## Scope

- Files: `crates/fieldbook/src/{build.rs,io/watch.rs}`, `crates/fieldbook/tests/build_watch.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook/src/cli.rs` (`BuildArgs` gains `pub watch: bool`, the `--watch` flag); `crates/fieldbook/src/io/mod.rs` (the `pub mod watch;` line and the dispatch of `build --watch` in `run_process`); `crates/fieldbook/src/lib.rs` (a `build --watch` reached through `run` is refused with exit 2, step 4).
- Docs to update: `CHANGELOG.md` (`- Added: build --watch.` under `## Unreleased`), `docs/reference/cli.md` (generated: run `cargo xtask docs`, never hand-edit).

## Out of scope

- `build --since` and `build --overlay` (`changes-page`, R2). A rebuild that also opens the page in a browser or reloads one: the output is files, and a browser reload is the owner's.
- Watching the Corpus of a company store, a data root or any folder outside the tree root. Watching `docs/`, `crates/` or the render templates: this is a Corpus authoring aid.
- A `notify`, `watchexec` or other watcher crate, a thread or a signal handler.
- Any change to what `build` does, its exit codes, its journal events or its output bytes.

## Preconditions

- Cards merged: `e1-workbook-templates` (it also changes `crates/fieldbook/src/build.rs`, to list its template files), `build-v0` (`build::run_with_info`, the shape of `BuildArgs` and the atomic replace of `DIR/edition`), `audit-journal` (the store lease that each rebuild takes and releases).

## Steps

1. `io/watch.rs`, the seam and the production poll. `pub trait Poll { fn fingerprint(&mut self) -> Result<u64, WorldError>; fn pause(&mut self) -> bool; }`: `fingerprint` is a hash over the sorted `(relative path, length, modified time)` of every file and folder under `<tree root>/corpus/` except `.git`, using `std::fs::metadata` and `std::fs::symlink_metadata` and the `io` module's sorted directory walk, never the content (the originals alone are 20 MB or more, and a poll every 500 ms must stay cheap); `pause` waits 500 ms and returns `false` when the watch must end. `DiskPoll { root }` is the production implementation and `pause` is `std::thread::sleep(Duration::from_millis(500))` returning `true`. The modified time is read as a duration since the epoch from `Metadata::modified`; the poll never calls `SystemTime::now`. A folder that cannot be listed is a `WorldError` with its path.
2. `build.rs`: `pub fn watch_with_info(args: &BuildArgs, world: &dyn World, info: &BuildInfo, poll: &mut dyn Poll, out: &mut dyn std::io::Write) -> u8`. Take the fingerprint, run `run_with_info`, write its stdout and stderr to `out` and `std::io::stderr()` at once (a watch cannot hold its text until it ends), then write `built` or `build failed (exit N)` and a newline. Loop: `pause`; stop and return 0 when it returns `false`; take the fingerprint; when it differs from the last one built, take it again after one more `pause` (settled when two polls agree; if it moved again, wait again); then build with the fingerprint taken before the build starts as the new last, so an edit made during a build is built next. A `WorldError` from `fingerprint` stops the watch and returns `EXIT_IO` with the message on standard error. Every exit code of `run_with_info` other than 0 is printed and the loop goes on. `build::run` and `run_with_info` are not changed.
3. `cli.rs`: `BuildArgs` gains `#[arg(long)] pub watch: bool` with the help line `Rebuild when the Corpus changes (checks every 500 ms; Ctrl-C stops)`. `io/mod.rs`: `run_process` parses the arguments as it does now and, when the command is `build` with `--watch`, calls `build::watch_with_info` with `DiskPoll`, `std::io::stdout().lock()` and the `BuildInfo` that `build_info_from_env` gives, and exits with its code; every other command goes through `crate::run` as before. `lib.rs`: `run` called with `build --watch` returns exit 2 with `error: --watch needs a terminal; run it from the program` and runs no build, so the in-process verb tests and `DETERMINISTIC_VERBS` runs never loop. Per [Adding a verb](README.md#adding-a-verb), a flag of a built verb needs no new `NOT_BUILT`, chip or `verbs.rs` row; run `verbs_match_ad17` and `every_verb_has_a_chip` to see that.
4. Tests in `crates/fieldbook/tests/build_watch.rs`, on a temporary copy of `corpus-mini` with `TestWorld::new` and a scripted `Poll` whose `pause` runs one step of a script (write a file, leave the files alone, return `false`) and so never sleeps: `watch_builds_once_then_waits_while_nothing_changes` (three idle pauses, then the end: one `built` line and one write set in the journal); `watch_rebuilds_after_a_changed_page` (the script appends a sentence to a page: two builds, and the second edition's page file holds the sentence); `watch_waits_for_a_change_to_settle` (the script changes a file on two polls in a row: one rebuild, not two); `watch_rebuild_equals_a_plain_build` (after the rebuild, every file of `DIR/edition` is byte-equal to the files of a plain `run_with_info` of the same tree in a second directory); `a_failed_build_keeps_the_old_edition_and_the_watch_goes_on` (the script plants an unknown page type so that `check` fails: the output holds `build failed (exit 1)`, `DIR/edition` is byte-equal to its earlier copy, the script then restores the page and the next build prints `built`); `an_edit_during_a_build_is_built_next` (the script changes a file from inside the `out` writer of the first build: a second build follows); `out_is_not_watched` (the files of `DIR` change between polls and no build follows); `a_fingerprint_error_ends_the_watch_with_exit_3` (the script removes the `corpus/` folder); `watch_through_run_is_refused` (`run(&["build".into(), "--watch".into()], &world)` has code 2 and no journal event). Seeded input: none; use `tests/fixtures/corpus-mini` and its `TODAY`.
5. `fingerprint_ignores_git_and_reads_no_content` in the same file: a change in `corpus/.git/` gives the same fingerprint; a file rewritten with the same length and a new modified time gives another; the test counts reads with a path that is not readable (a file with mode 000 on Linux, skipped on Windows) and still gets a fingerprint.
6. Run `cargo xtask docs` and commit the changed `docs/reference/cli.md`; add the `CHANGELOG.md` line.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test build_watch
cargo nextest run --locked -p fieldbook --test cli --test chips
cargo xtask docs --check
cargo build -q --locked -p fieldbook && B="$PWD/target/debug/fieldbook" && T="$(mktemp -d)" && cp -R tests/fixtures/corpus-mini/. "$T" && S="$T/state" && mkdir -p "$S" && cd "$T" && (timeout 8 "$B" --state "$S" build --watch --out "$T/out"; echo "exit $?"); cd - >/dev/null; rm -rf "$T"
just check
```

Expected: the build-watch tests pass, `cli` and `chips` pass unchanged; `docs --check` exits 0; the shell run prints `built` and, after 8 seconds, `exit 124` (stopped by `timeout`, as Ctrl-C would stop it); `just check` exits 0.

## Negative control

In `watch_with_info`, build on every poll instead of on a changed fingerprint: `watch_builds_once_then_waits_while_nothing_changes` must fail on its second `built` line. Restore it. Then build as soon as the fingerprint differs, without the second poll: `watch_waits_for_a_change_to_settle` must fail. Then let a failed build end the loop: `a_failed_build_keeps_the_old_edition_and_the_watch_goes_on` must fail. Then take the fingerprint after the build instead of before: `an_edit_during_a_build_is_built_next` must fail. Then include `DIR` in the fingerprint: `out_is_not_watched` must fail. Restore each. Last, call `std::thread::sleep` inside `watch_with_info` instead of through `Poll::pause`: the test run must hang or take over 10 seconds, and `cargo clippy` must show no new allowance for it; undo it.

## Rollback

Revert the merge commit. `--watch` becomes an unknown flag of `build` (exit 2); `build` itself, its journal and its output are unchanged, and nothing remains on disk.

## BLOCKED

`BuildArgs`, `run_with_info` or the sorted directory walk of `io` is not where this card says: stop and ask the lead; do not add a watcher crate. A negative control that stops failing stops the card.

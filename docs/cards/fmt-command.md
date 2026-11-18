# fmt-command

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook fmt` rewrites the registers and pages under `corpus/` into normal form, and `fieldbook fmt --check` lists each file that would change and exits 1 without writing. `fmt` changes form, never a value: a test formats every fixture file and finds the parsed values unchanged.

## Why

- The verb is in [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience). Normal form is FB0122 in the [check catalogue](../reference/corpus-format.md#01-pages-corpustoml-and-files) and the File rules of [Layout of corpus/](../reference/corpus-format.md#layout-of-corpus), including that `fmt` never rewrites front matter.
- Registers are normalised by `core-csv`'s `table::normalize` and checked with `is_normal`; this card does not write a second writer.
- This is a Could ([Cut line and floor](../decisions/0001-foundation.md#cut-line-and-floor)).
- Design: the [MAP.md](../design/MAP.md) row CommandChip gains `fmt`; the chip is the verb map's "Format the files" in `fieldbook-core::strings`. No page. [HANDOFF.md](../design/HANDOFF.md) §5 (CommandChip). Token groups: none.

## Scope

- Files: `crates/fieldbook/src/{cli.rs,fmt.rs}`, `crates/fieldbook/tests/fmt.rs`, `crates/fieldbook/tests/chips.rs`, `crates/fieldbook-core/src/schema/{mod.rs,page.rs}`, `crates/fieldbook/src/io/journal.rs` (`read_tree_coherent` only: step 12).
- Registration lines: `tests/support/chip_cases.rs` (the `fieldbook fmt` case, this card's entry only); the quick start line of `README.md` (the `fmt` chip); `crates/fieldbook/tests/cli.rs` (remove `fmt` from `NOT_BUILT`, this card's verb only); `crates/fieldbook/src/lib.rs` (the `mod fmt;` line and the dispatch arm, this card's entries only).; `crates/fieldbook/src/lib.rs` (public World::write_file paired token doctests only); `xtask/binscan/silent.txt` and `xtask/binscan/ai.txt` (the `IsProcessorFeaturePresent` entry only)
- Docs to update: `README.md` (one quick start line), `CHANGELOG.md` (`- Added: the fmt command.`), `docs/reference/cli.md` (generated: run `cargo xtask docs`, never hand-edit).

## Out of scope

- Other text files under `corpus/` (`corpus.toml`, `changes.md`, the rule files, templates); `check` still reports them with FB0122.
- Reserialising front matter or reordering its keys.
- Originals under `corpus/sources/public/`, which are bytes.

## Preconditions

- Cards merged: `status-command` and `add-new-and-edition` (`World::write_file`).

## Steps

1. Collect `corpus/registers/*.csv` and `corpus/pages/**/*.md` from the tree, in path order.
2. A register goes through `table::normalize(register, bytes)`. A register that does not parse is left alone and reported with its first problem; `fmt` exits 1.
3. A page body gets the File rules: no byte-order mark, LF, NFC, no trailing whitespace, one final newline. Front matter is never parsed and written back. A front-matter fault other than a byte-order mark or a line ending is reported, not fixed.
4. Without `--check`, take `world.write_lease()`, open the journal, load with `world.read_tree_under(&lease)`, then collect the changes and retain the lease through the write. If there are none, write nothing, append nothing and print `fmt: <n> files in normal form`. Otherwise call `Journal::begin(&lease, &intent, &write_set)` with `WriteSet { labels: LabelSet::public(), files, affected: Vec::new() }`, one `WriteFile` for each changed file (its path and the SHA-256 of its new bytes), write each file through `World::write_file(&token, path, bytes)` (added by `add-new-and-edition`, implemented by `Disk` with `io::atomic`), call `Journal::end(token, outcome)` and print `formatted <path>` for each. With `--check`, write nothing, open no write event, print `would format <path>` for each, and exit 1 if any. Remove `fmt` from `PENDING` in `crates/fieldbook/tests/chips.rs` (`build-about`) and from `NOT_BUILT` in `crates/fieldbook/tests/cli.rs`. Register its case in `tests/support/chip_cases.rs` (a fresh Home `corpus-mini` copy, exit 0).
5. Tests on a temporary copy of the fixture: `a_normal_copy_passes_check` (the copy is already normal, so `fmt --check` exits 0); `a_crlf_register_is_fixed`, `a_page_with_a_byte_order_mark_is_fixed` and `trailing_spaces_are_fixed` (each is fixed and then passes `--check`); `fmt_is_idempotent_and_journaled` (the first `fmt` of a copy with one CRLF register appends one `write.begin` and one `write.end` for the verb `fmt` with files = 1 on begin, empty affected references on both, and no filenames, and `audit verify` prints `journal ok`; the second run changes nothing and appends no event); `fmt_changes_form_never_a_value` (for every fixture file, the parsed rows and pages before and after `fmt` are equal); `check_strict_is_clean_after_fmt` (the formatted copy reports no FB0122); and `fmt_refuses_without_a_token`, a `compile_fail` doc test on the existing public `World::write_file` interface that calls it without an `Audited` token, paired with a compiling `no_run` example supplying `&Audited`; keep the fmt handler private.
6. Run `cargo xtask docs` and commit the changed `docs/reference/cli.md`.

7. Before collecting ordinary changes or returning no-change, recover matching pending `fmt` intent under the exclusive lease. Validate every recorded path against its before/after hashes. Normalize every recorded file, including files already at their after hash, and require the full ordered path/after-hash set to match the intent. Preserve recorded before hashes, write only unfinished files and complete the original transaction. `fmt --check` stays read-only and refuses pending generations. Tests interrupt between two writes, after the last write and after journal completion before cleanup; third hashes refuse. Removing recovery or omitting already-written paths must fail.

8. Format-only WriteSet values use `affected: Vec::new()` because parsed register values do not change. The journal test asserts files = 1 on begin, empty affected on both events and no filenames; compare normalized CSV separately and retain the no-event second run.

9. For fmt, OperationKey binds the verb and store; request_sha256 binds canonical path filters and formatting mode, but not changing live output hashes. Preserve the original requested file set and use pending_intent before the no-change branch; recover verifies the reconstructed complete after hashes.

10. `fmt_preserves_unknown_extension_locator` runs real formatting on a copied page and claim containing canonical extension kind/payloads, then parses again and asserts byte-identical locator cells and citation text. Formatting neither drops unknown extensions nor changes percent escapes.

11. Extend the existing check-after-format test with the trailing-space note case from check-files, supplied with BOM/CRLF. Formatting preserves the exact cell, `fmt --check` succeeds and strict checking emits no FB0122. Trimming the cell or restoring the register line-level whitespace check must fail this case.

12. A read-only command reads a hand-edited Corpus after a journaled write. `read_tree_coherent` keeps every lock, initialization and pending-write refusal and no longer refuses a tree whose hash differs from `state/generation`: with no pending write and the shared lease held, that difference is a hand edit, which `fmt --check` exists to report. The tree it returns carries its own generation hash and the marker is not rewritten. `readers_accept_a_hand_edit_after_a_journaled_write` runs `fmt`, edits a page by hand and requires `fmt --check` to list it and exit 1 and `check`, `status` and `search` not to exit 3; `readers_still_refuse_a_pending_write_after_a_hand_edit` keeps the refusal of an interrupted write.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test fmt
cargo test --doc --locked -p fieldbook
cargo run -q --locked -p fieldbook -- fmt --check; echo "exit $?"
cargo xtask docs --check
just check
```

Expected: every test passes, the doc test included; `fmt --check` on the committed Corpus prints `fmt: <n> files in normal form` and `exit 0`, and `git status --short` is empty; `docs --check` and `just check` exit 0.

## Negative control

In a temporary copy of the fixture, convert one register to CRLF line endings. `fmt --check` there must exit 1 and print `would format corpus/registers/<name>.csv`. Then make `fmt.rs` skip registers: the test `a_crlf_register_is_fixed` must fail. Restore both.

Remove the token parameter from the public writer in a temporary plant: the tokenless example must compile, making its compile_fail test fail. Restore it; module privacy must not be the failure.

Strip an unknown extension during formatting: `fmt_preserves_unknown_extension_locator` must fail. Restore it.

## Rollback

Revert the merge commit. `fmt` becomes a usage error. Files it already formatted stay valid.

## BLOCKED

`fmt --check` reports a committed file. The file is the lead's: name it and stop; do not format the Corpus in this card.

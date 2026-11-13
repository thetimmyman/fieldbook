# backup-journal-recording-gaps

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every backup event that happens reaches the journal or the owner's screen: a raw `backup verify` that cannot append says so, a refusal on an earlier-broken chain is recorded, a backup in recovery mode removes no snapshot, a failed `backup.end` append names the snapshots it already removed, and a leftover partial that cannot be removed fails the run. Five smaller faults that let a snapshot or the journal lie are closed with them, each with a test that fails on the old code.

## Why

- The need. The `backup-snapshot` builder and the judge left nine defects in merged code. Each one is confirmed in the code on `origin/main`, with its place:
  1. `run_verify_raw` in `crates/fieldbook/src/backup.rs` (about line 1320) writes `backup.verify` with `if let Ok(mut j) = Journal::open(..)` and `let _ = j.append_backup_verify(..)`. When the journal cannot be opened or appended, the verify happens and nothing says it was not recorded.
  2. `Journal::refuse` (`io/journal.rs`, about line 2103) returns `BrokenChain` when `!is_journal_chain_clean`, and `run_backup` calls it as `let _ = j.refuse(..)` at three places (target place refusal, git refusal, egress refusal). `state::open` returns a journal that is not clean, and `backup.begin` appends on it, so a backup on an earlier-broken chain records its begin but never its refusal.
  3. In recovery mode (the journal append is refused) retention still removes the snapshots of `pruned_snapshots` (step 13), and no `backup.prune` is written, so a removal is not in any journal.
  4. When `end_backup` fails after retention removed snapshots, `run_backup` returns at once (about lines 994 to 1010): the `removed` list and the `prune_problems` lines are dropped, and the exit is 3 with nothing about the removals.
  5. `FolderTarget::clean_leftover_partials` (`io/backup_folder.rs`) ignores every error from `dirs::remove_outside` and returns `Ok`, so a partial that cannot be removed lets the next snapshot start beside it.
  6. `dirs::remove_outside` for `Removable::Snapshot` (`io/dirs.rs`, about line 303) accepts any folder named `fieldbook-backup-NNNN` that holds a regular file `backup.toml`. It does not parse it or match its `snapshot` number to the folder name, so a hand-made or copied folder passes as a snapshot of that number.
  7. `journal_state` is `"broken"` through `fieldbook_core::strings::cite_statuses()[0]` in `run_backup` (line 706) and in `crates/fieldbook-core/src/backup/toml.rs` (lines 72 and 73): the first of the nine citation-check statuses. A reorder of that list changes the backup file format.
  8. `last_recorded_settings_baseline` (`io/journal.rs`, about line 2927) reads `read_valid_events_prefix`, the events before the first break. After a break, a `settings.change` appended after it is never seen, and every `Journal::open` appends another. The audit contract (line 33) says "across all segments" and gives no reason to stop at a break.
  9. `enumerate_roots` (`io/backup_classify.rs`, about line 526) skips a missing `corpus/` root without a word, and a regular file named `corpus` as well, so a checkout with no Corpus backs up to a snapshot of `state/` alone with exit 0.
- Lead decisions, each one the repository does not settle:
  1. Item 1: raw verify prints `note: verify was not recorded in the journal: <reason>` on stderr and keeps its exit code. It does not fail the verify; the check of the copy is what the owner asked for, and the exit stays the copy's result.
  2. Item 2: a new `Journal::refuse_backup(code, reason, about)` appends a `refuse` with `verb = backup` without the clean-chain test, as `begin_backup` already appends on that chain. `refuse` itself stays strict, as does the `journal.recover` path, because the builder kept them strict on purpose. The three backup call sites use `refuse_backup`. The audit contract words no rule for it and this card does not edit it; if a gate reader wants it worded, see BLOCKED.
  3. Item 3: in recovery mode `pruned_snapshots` is emptied before the Delivery is built, so a backup with the journal refused removes nothing and prints `no snapshot removed: the journal is refused` on stderr. An unrecorded removal is worse than an extra snapshot.
  4. Item 4: when `end_backup` fails, the removals are listed on stderr as `removed without a journal record: 0003, 0004` with each `prune_problems` line, and the exit stays 3. The new test hook `set_fail_on_backup_end_append`, beside `set_fail_after_end_append` in `journal.rs`, fails that one append.
  5. Item 5: `clean_leftover_partials` returns the first `BackupError::Target` naming the partial, and `run_backup` exits 3 with `clean leftover partials error:`.
  6. Item 6: `remove_outside` for a snapshot parses `backup.toml` with `BackupToml::parse` and refuses with `OutOfScope` when the parse fails or `snapshot` differs from the folder number.
  7. Item 7: `pub const JOURNAL_STATE_BROKEN: &str = "broken"` in `backup/toml.rs`, with `ok` as `JOURNAL_STATE_OK`; both uses and the verify side take them. The file format is unchanged.
  8. Item 8: `last_recorded_settings_baseline` takes the last `settings.change` among verified events only. The trust rule, which satisfies line 33 of the audit contract ("across all segments") and line 98 ("Normal operation uses verified schema-valid journal history") together: events up to the first break count, as today; events after the first break count only when they verify as a chain from the first line after the break, each line parsing, linking to the hash stored in the line before it and hashing correctly; a line that does not parse or verify never counts, and it never suppresses a `settings.change` (the next `Journal::open` appends a fresh baseline). The contract needs no new wording, so there is no owner precondition.
  9. Item 9: a missing `corpus/` root, or a regular file in its place, refuses the backup with exit 3 and `no corpus folder at <path>`.
- Size. Nine small fixes with tests may pass 600 changed lines. The seam is between steps 4 and 5: steps 1 to 4 are the recording gaps (items 1 to 4), steps 5 to 9 the robustness fixes. BLOCKED says what to do.
- Gate: none. No label, policy or egress module of `fieldbook-core` changes (`backup/toml.rs` is the backup format, not a gate module) and the audit contract text does not change. `Review: auto`.

## Scope

- Files: `crates/fieldbook/src/backup.rs`, `crates/fieldbook/src/io/journal.rs`, `crates/fieldbook/src/io/backup_folder.rs`, `crates/fieldbook/src/io/backup_classify.rs`, `crates/fieldbook/src/io/dirs.rs`, `crates/fieldbook-core/src/backup/toml.rs`, `crates/fieldbook/tests/backup.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook/src/lib.rs` (none expected; a `pub use` only if a test needs `refuse_backup`).
- Docs to update: `CHANGELOG.md` (`- Fixed: backup records or reports every journal event, and refuses a missing corpus, an unremovable partial and a snapshot folder that does not match its manifest.`).

## Out of scope

- The memory of `backup` (`backup-bounded-memory`) and the `backup verify` and label edges (`backup-verify-and-label-edges`).
- Any edit to `docs/reference/audit-contract.md`, to `Journal::refuse`, `Journal::recover` or `begin`, and to the generated `docs/reference/audit.md`.
- The unlabelled default for a store under a loaded policy (`policy-load`, a policy-gate card).

## Preconditions

- Cards merged: `backup-snapshot`, `restore-and-drill` (it changes `io/journal.rs` and `io/backup_folder.rs`), `backup-bounded-memory` (it changes `backup.rs` and `io/backup_classify.rs` in the same loop).

## Steps

1. Tests first, each in `crates/fieldbook/tests/backup.rs`, each failing on the old code: `raw_verify_that_cannot_append_says_it_was_not_recorded` (item 1: a raw copy verified while the journal append really fails, with the journal lock held by another `Journal` handle and `set_lock_timeout_for_test(Some(50 ms))`, so `Journal::open` or the append returns the lock error; stderr holds the note, exit 0. A mid-chain break is not the fixture: `Journal::open` runs `inspect_and_recover_tail` and `append_backup_verify` checks only the tail, so a raw verify on such a journal is recorded and prints no note); `refusal_on_a_broken_chain_is_recorded` (item 2: a backup refused by the egress check on a journal with an earlier break; the journal holds one `refuse` with `verb = backup` after the break, and `audit verify` still reports the break); `recovery_mode_backup_removes_no_snapshot` (item 3: three snapshots, `keep_last = 1`, journal append refused; all three remain and stderr says so).
2. `io/journal.rs`: add `Journal::refuse_backup` and the hook `set_fail_on_backup_end_append`; `backup.rs`: the three refusal sites call `refuse_backup`; `run_verify_raw` prints the note. Item 3: empty `pruned_snapshots` when `recovery_opt` is `Some`, before the Delivery is built, and print the stderr line.
3. Item 4: `end_backup` failure path prints the removals and `prune_problems`, then returns exit 3. Test `end_append_failure_names_the_removed_snapshots`: `keep_last = 1`, hook armed; exit 3, stderr names the removed number, and the removed snapshot is gone.
4. Run the four tests: they pass. Run `cargo nextest run --locked -p fieldbook --test backup` and `cargo nextest run --locked -p fieldbook --test backup_no_panic`.
5. Items 5 and 6: `clean_leftover_partials` returns the error; `remove_outside` parses the manifest. Tests `unremovable_partial_fails_the_backup` (a partial folder with a locked entry on Linux, made with a read-only parent folder; skip on a platform without it with `#[cfg(unix)]`), `snapshot_folder_with_a_foreign_manifest_is_not_removed` (a folder `fieldbook-backup-0009` whose `backup.toml` says `snapshot = 2`) and `snapshot_folder_with_an_unparsable_manifest_is_not_removed`.
6. Item 7: add the two constants, replace both uses, and test `journal_state_words_are_ok_and_broken` (the constants equal `"ok"` and `"broken"`; a `backup.toml` made by the old code still parses).
7. Item 8: `last_recorded_settings_baseline` reads all parseable events. Tests `settings_change_after_a_break_is_not_appended_again` (a journal with a break and a later verified `settings.change`; ten `Journal::open` calls append no second `settings.change`) and `unverified_settings_change_after_a_break_does_not_suppress_a_real_one` (a planted `settings.change` line after a break whose hash does not verify; the next `Journal::open` still records the real baseline).
8. Item 9: `enumerate_roots` returns a `BackupReadError` for a missing or non-folder `corpus` root. Test `backup_of_a_checkout_without_corpus_is_refused`.
9. Run the acceptance commands. Mark no test `#[ignore]`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test backup
cargo nextest run --locked -p fieldbook --test backup_no_panic
cargo nextest run --locked -p fieldbook-core backup::
cargo xtask cards --check
just check
```

Expected: every test passes, the eleven new ones included; `just check` exits 0.

## Negative control

For each fix, restore the old line and run its test: it must fail. Use the old `let _ = j.refuse(..)`: `refusal_on_a_broken_chain_is_recorded` fails. Remove the `pruned_snapshots` emptying: `recovery_mode_backup_removes_no_snapshot` fails. Swallow the error in `clean_leftover_partials`: `unremovable_partial_fails_the_backup` fails. Read the prefix again in `last_recorded_settings_baseline`: `settings_change_after_a_break_is_not_appended_again` fails; accept every parseable line: `unverified_settings_change_after_a_break_does_not_suppress_a_real_one` fails. Restore each.

## Rollback

Revert the merge commit. The nine behaviours return as they were; no snapshot and no journal written by the new code is unreadable by the old, because the file formats are unchanged.

## BLOCKED

- The diff of steps 1 to 4 passes 600 changed lines, or the whole diff passes 800: set the item to Blocked with a `BLOCKED:` comment that names the seam between steps 4 and 5, and stop; the lead splits the card there.
- A gate reader requires the audit contract to word `refuse_backup` or the recovery rule: set the item to Blocked with a `BLOCKED:` comment, and stop. Do not edit `docs/reference/audit-contract.md`.
- A test needs a new hook beyond `set_fail_on_backup_end_append` (the existing hooks, `set_lock_timeout_for_test` among them, may be used), or `write_outside` to change: set the item to Blocked with a `BLOCKED:` comment, and stop.

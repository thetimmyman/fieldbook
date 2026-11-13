# backup-bounded-memory

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook backup` holds at most one file's bytes in memory at a time, so its peak memory on a Corpus of 200 MB stays within a few times its largest file instead of the Corpus size, and a test that runs the real binary on a 200 MB synthetic Corpus reads the child's peak resident memory from `/proc` and fails above the bound. A snapshot written by the new code is byte-identical to one written by the old code for the same tree.

## Why

- The need. `run_backup` in `crates/fieldbook/src/backup.rs` reads every planned file into `file_bytes_cache` (step 7) before it writes anything, and `FolderSnapshotStage` in `crates/fieldbook/src/io/backup_folder.rs` copies every written payload into `written_files` until `finish` compares it. The whole tree is in memory twice over. The `backup-snapshot` builder measured 720 MB of resident memory for a 68 MB Corpus in a debug build. The planned Corpus at 20 times Day 1 size (`scale-fixture`) is larger than that. The ignored test `backup_200_mb` in `crates/fieldbook/tests/backup.rs` calls `run_backup` inside the test process and measures only time, so nothing watches memory today.
- A second cost sits in the same loop. `classify_file` in `crates/fieldbook/src/io/backup_classify.rs` calls `world.support_facts()` for each support file, and `Disk::support_facts` in `io/tree.rs` re-reads the journal prefix and re-verifies the catalogues on every call. Computing it once per run removes that repeat and is part of the same loop.
- The judge found this a follow-up, not a merge blocker of `backup-snapshot`. "One file in memory at a time" is the bound this card keeps. Chunk streaming is not, see lead decision 2.
- Lead decisions, each one the repository does not settle:
  1. `write_outside(&Allowed, path, bytes)` keeps its signature. control EG-01 in [docs/controls.md](../controls.md) (test `io_has_two_writer_doors`) forbids a third writer door, and a streaming door would be a third one next to the egress-checked door. One file at a time through the existing door meets the bound for the largest file the Corpus holds today. A later card that needs a larger file than memory allows opens that door on its own review.
  2. The bound is one file, not a chunk. A chunked write would need a streaming writer in the door of decision 1.
  3. Two passes over the files. Pass 1 reads each file, classifies it, records `(rel, sha256, len)` and drops the bytes. After `prepare_backup_begin_at` the active journal segment is re-read once and `prep.bytes` appended, to hash the segment that the snapshot will hold. Pass 2 is the existing write loop: it re-reads each file, hashes it, and refuses with exit 3 and `source changed during backup: <rel>` when the hash differs from pass 1. `write_outside` already refuses bytes that differ from the ones the egress check bound (`check_outside_write_binding` in `io/atomic.rs`, `OutOfScope`, printed as `write error for <rel>`); the new comparison runs first and gives the owner the cause in plain words, and a test asserts that message. The active segment is compared with the hash of pass 1 plus `prep.bytes`.
  4. `support_facts` is computed once per run and passed into `classify_file`. `classify_file` and `support_has_public_provenance` in `io/backup_classify.rs` (the two callers of `world.support_facts()`, lines 38 and 438) take `&SupportFacts` in place of calling the world; the tests that call them directly pass it.
  5. `finish` re-reads each written file from disk with a streaming hasher (a `File::open` needs the same `clippy::disallowed_types` expect that `hash_original_file` in `io/tree.rs` carries) and compares `(sha256, len)` recorded at `write`, in place of cloning every payload. The readback guard stays; its message stays `readback mismatch for <rel>`.
  6. The memory test runs the real binary as a child process and reads `VmHWM` from `/proc/<pid>/status`, as `xtask/src/scale.rs` does (`vmhwm_kb`). It is Linux only and carries `#[cfg(target_os = "linux")]`. The bound is the baseline of an empty run plus four times the largest file plus 64 MB; the card states the numbers so the test fails with them in its message.
- Gate: none. No label, policy or egress module of `fieldbook-core` and no audit contract change. `Review: auto`.

## Scope

- Files: `crates/fieldbook/src/backup.rs`, `crates/fieldbook/src/io/backup_classify.rs`, `crates/fieldbook/src/io/backup_folder.rs`, `crates/fieldbook/tests/backup.rs`, `crates/fieldbook/tests/backup_memory.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook/src/io/tree.rs` (no change expected: only a caller of `support_facts` goes away; a doc line may change).
- Docs to update: `CHANGELOG.md` (`- Changed: backup holds one file in memory at a time.`).

## Out of scope

- Streaming a single file larger than memory, and any change to `write_outside`, `Transfer::write_path` or the two writer doors.
- The journal, the labels, the retention logic and the recovery paths (`backup-journal-recording-gaps`), and the `backup verify` edges (`backup-verify-and-label-edges`).
- `restore`, which `restore-and-drill` adds.

## Preconditions

- Cards merged: `backup-snapshot`, `restore-and-drill` (it changes `io/backup_folder.rs` too, and this card changes `FolderSnapshotStage`).

## Steps

1. In `crates/fieldbook/tests/backup_memory.rs`, add `backup_peak_memory_is_bounded` first, on a 200 MB synthetic Corpus (ten files of 20 MB, the fixture shape of `backup_200_mb`), written into a temp checkout with the settings the fixture of `tests/backup.rs` writes. Before any backup it gives the checkout the support provenance the real binary needs. `Disk::support_facts` (`io/tree.rs`) accepts public support only from a successful raw `backup.verify` event in the journal plus the retained catalogue under `state/public-support/`; the in-process tests inject it through their `World` (`set_support_catalogue`), which a spawned binary cannot. So the test builds the catalogue with `SupportCatalogue::from_public_source(&tree, <40 hex>)` as the fixture of `tests/backup.rs` does, writes it to a file, takes its digest from `to_toml_bytes_and_sha256`, and runs `fieldbook backup verify --raw DIR --support-catalogue FILE --expect-support-sha256 HEX` on the same checkout and state (or uses the fixture helper that the support setup of `restore-and-drill`'s `tests/drill4.rs` names, if it writes the same state), for the baseline run and the 200 MiB run alike, asserting exit 0 each time; without it the support files are `unlabelled` and the egress check refuses before any memory is measured. It then spawns the binary (`CARGO_BIN_EXE_fieldbook`, `current_dir` the checkout, `backup`), polls `/proc/<pid>/status` every 20 ms for `VmHWM` while the child runs, and asserts exit 0 and `peak_kb * 1024 <= baseline + 4 * 20 MiB + 64 MiB`. `baseline` is the peak of the same command on the fixture without the ten files, asserted to exit 0 as well; it includes the model load (`load_model_if_available` reads the tree, and `read_tree` hashes originals without holding them). Ten files of 20 MiB stay: the bound leaves 4 × 20 + 64 = 144 MiB of headroom, and the negative control's one extra copy of about 200 MiB exceeds it only at this size. Run it: it fails on the current code. Keep `backup_200_mb`. The test is not `#[ignore]`d, so every pull request's `just check` runs it; the builder measures its debug-profile time and records it in the pull request body, and BLOCKED says what to do above 120 seconds.
2. `classify_file` and `support_has_public_provenance` in `io/backup_classify.rs`: take `facts: &SupportFacts`; `run_backup` calls `world.support_facts()` once before the loop. Update the callers in `tests/backup.rs` (about the `classify_file` tests). Name the new test `support_facts_are_computed_once`: a counting `World` wrapper (the tests' fixture already wraps `World`) counts the calls during one `run_backup` and asserts one.
3. `run_backup` step 7: pass 1 reads one file, classifies it, records `(rel, sha256, len)` and drops the bytes. The segment prepared by `prepare_backup_begin_at` is read once, `prep.bytes` appended, hashed and recorded; `file_bytes_cache` is removed. `SHA256SUMS`, `backup.toml` and the `Transfer` list are built from the recorded triples, never from bytes.
4. A test seam for that rewrite. `read_file_content` in `io/backup_classify.rs` reads with `std::fs`, not through `World`, so no `World` wrapper sees the read. Add a hook in the style of the journal's `set_fail_*` hooks: `pub fn set_after_read_hook(hook: Option<Box<dyn FnMut(&str) + Send>>)` in `io/backup_classify.rs`, armed for the current thread only, called by `read_file_content` with the relative path after each successful read. The test arms it to append a byte to one named file on that file's first read.
5. The write loop at step 12: re-read one file, hash it, compare with the recorded sha, and on a mismatch return exit 3 with `source changed during backup: <rel>`; otherwise `stage.write`. Name the tests `file_changed_between_passes_refuses` (exit 3 and stderr holding `source changed during backup: <rel>`, not `write error`; the file is rewritten by the test hook below) and `active_journal_segment_hash_includes_begin_event`.
6. `FolderSnapshotStage` in `io/backup_folder.rs`: `written_files` becomes `Vec<(PathBuf, String, u64)>`, filled in `write` from the bytes it was given; `finish` re-reads each file with a hasher over `std::io::Read` in 64 KiB blocks and compares sha and length. Keep the message. Name the test `finish_detects_a_changed_written_file` (a file edited on disk between `write` and `finish`).
7. Add `snapshot_bytes_are_unchanged_by_the_two_passes`: a fixture Corpus backed up and compared with the expected snapshot file list and `SHA256SUMS` the existing test `backup_writes_a_verifiable_snapshot` asserts (reuse its expectations; do not weaken them).
8. Run step 1's test: it passes. Record the measured baseline and peak in the pull request body.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test backup_memory
cargo nextest run --locked -p fieldbook --test backup
cargo nextest run --locked -p fieldbook --test backup_no_panic
cargo xtask cards --check
just check
```

Expected: every test passes, `backup_peak_memory_is_bounded` included; `just check` exits 0.

## Negative control

In `run_backup`, push every pass-1 file's bytes into a `Vec` again: `backup_peak_memory_is_bounded` must fail and print the peak and the bound. Restore it. Remove the comparison of pass 2: `file_changed_between_passes_refuses` must fail, because the refusal then reads `write error for <rel>` and not `source changed during backup: <rel>`. Restore it. Edit a written file between `write` and `finish`: `finish_detects_a_changed_written_file` must fail if the comparison is removed. Restore it.

## Rollback

Revert the merge commit. `backup` goes back to reading the whole tree before it writes.

## BLOCKED

- The binary cannot be pointed at a temp checkout without a new flag: set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Do not add a flag in this card.
- The measured peak on the 200 MB fixture is above the bound with one file in memory (a library or the allocator keeps more): set the item to Blocked with a `BLOCKED:` comment that gives the measured numbers, and stop. Do not raise the bound.
- The memory test takes more than 120 seconds in the debug profile: set the item to Blocked with a `BLOCKED:` comment that gives the time, and stop; the lead decides whether it is `#[ignore]`d and run with `--run-ignored all` from a named workflow.
- A third writer door looks necessary: set the item to Blocked with a `BLOCKED:` comment, and stop.

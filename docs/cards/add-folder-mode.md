# add-folder-mode

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook add <dir> [--recursive] [--confirm-public] [--json]` takes a folder, handles each file as `add <file>` does, and prints one report line per file in byte order of file name: `added`, `present` or `refused <code>`, then a count line. The person gives `--confirm-public` once and it covers every file of the run; without it each file is refused with FB0410. A refusal does not stop the folder; the exit code is 1 if any file was refused, 3 on an I/O error, otherwise 0. `--json` prints the same report as one document with `report_version = 1`, and each run keeps that document in `reports/last-add.json` in the state folder. A file of a type that can run code is refused with FB0821; a macro-enabled Office file is refused only in the repository Corpus.

## Why

- What `add` writes and the idempotence rule are [Adding a version](../reference/corpus-format.md#adding-a-version). The flags and exit codes are [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience); the folder report replaces "a batch is a shell loop", which gave no refusal list.
- FB0821 is in [08: audit, backup and pipeline](../reference/corpus-format.md#08-audit-backup-and-pipeline). The folder mode is the plain deterministic path: it needs no model, extractor or queue. The report is also the source of the Import report page: each run keeps it in `reports/last-add.json`, which `status --page` reads (`status-command`).
- Design: the [MAP.md](../design/MAP.md) row "CommandChip", for the `add <folder>` chip only; the Import report page reads `reports/last-add.json`. No template, string or token group is built here.

## Scope

- Files: `crates/fieldbook/src/{cli.rs,add.rs,report.rs}`, `crates/fieldbook-core/src/add.rs` (the `DISALLOWED` extension list and `check_type`), `crates/fieldbook/tests/{add.rs,chips.rs}`, `tests/fixtures/add-folder/` (synthetic text files), `tests/fixtures/state-add-folder/`, `tests/golden/json/add.json`, `tests/known-bad/FB0821.toml`, `tests/known-bad/pending.toml` (the FB0821 row is deleted).
- Registration lines: the `AddReport` struct in `crates/fieldbook-core/src/report.rs` and one row in `crates/fieldbook/tests/json_reports.rs` ([Adding a JSON report](README.md#adding-a-json-report)); `crates/fieldbook/src/lib.rs` (the `mod report;` line, this card's entries only).; `crates/fieldbook/tests/support/faulty.rs` (fail-on-Nth matching write counter); `docs/reference/policy.md` (saved-report classification contract)
- Docs to update: `docs/reference/cli.md` through `cargo xtask docs`, `CHANGELOG.md` (`- Added: add accepts a folder and prints a per-file report.`).

## Out of scope

- `--propose` and the word `proposed` (`review-queue`), extraction (`extract-and-locators`), and reading the text of a PDF, a Word file or a workbook (a workbook is `extract-xlsx-sheets`).
- A new journal kind: each added file uses the `write.begin` and `write.end` pair of `add-new-and-edition`, and each refusal one `refuse` event.
- `--source` with a folder. Which source would each file be a version of? It is a usage error.
- The company-store form of the folder mode and the sync of a share (`r2-org-intake`, `r2-pull-and-sync`). `check_type` already takes the store, so those cards call it with theirs and add no second type list.

## Preconditions

- Cards merged: `add-new-and-edition`, `check-command` (it makes core report.rs, the json_reports.rs table, the chips test and its PENDING list), `core-policy` (State admission and the effective floor), `audit-journal` (the state builder `build_state` and the `StateScope` rule for `reports/`), `check-known-bad` (the harness that reads `FB0821.toml` and the `pending.toml` row this card deletes).

## Steps

1. In `fieldbook-core/src/add.rs`, `check_type(file_name, first_bytes, store) -> Result<(), Refusal>`: refuse a program or script extension (`exe dll msi com scr jar bat cmd ps1 psm1 vbs vbe js jse wsf sh lnk url reg hta`, compared in lower case) in every store, and any file whose first bytes are `MZ` or `\x7fELF`, whatever its name. A macro-enabled Office extension (`docm xlsm pptm`) is refused only when `store` (a `StoreId` from `core-labels`) is `corpus`, because the repository Corpus is published in an edition. In a company store the same file is kept: it is stored under its hash and never run, and a later card (`extract-xlsx-sheets`) reads the cell values of an `xlsm` and never its macro project. Vendor workbooks are often `xlsm`, and the failure to prevent is running code, not holding the file. The only caller in this card passes `corpus`; `r2-org-intake` and `r2-pull-and-sync` pass theirs. The refusal carries FB0821 and the file name only. Text, Markdown, HTML, CSV, PDF and the other Office types are allowed. Beside `DISALLOWED` add `TEXT_TYPES` (`txt md markdown html htm xhtml svg`, lower case), the types that a built-in extractor reads (`extract-and-locators` holds the equal list `SELECT_TYPES` and the test that keeps the two the same; `extract-csv-rows` and `extract-xlsx-sheets` append their types to both), and `needs_extraction(file_name) -> bool`, true for an allowed type that is not in `TEXT_TYPES`.
2. In `add.rs` (binary), split the single-file path into `add_one(world, journal, path, options) -> Result<FileResult, Error>` with the results `Added(version_id)`, `Present(version_id)` and `Refused(code)`. The order is fixed: read the bytes, `check_type` (FB0821), the labeller and store admission of `add-new-and-edition` (FB0400 for a marking, and FB0410 when `--confirm-public` is missing: the run calls `add::label_context(world, confirm_public)` once, as `add-new-and-edition` does, and every file uses it), then matching recovery through the single-file coordinator, and only with no pending operation `plan_add` and the write set of `add-new-and-edition`. A refusal appends one `refuse` event (code, `sha256`, `name_sha256`; use the shared add refusal mapping: FB0410/unlabelled, FB0400/store, FB0822/label and FB0821/store; retain labels wherever already known) and no `write.begin`. `present` appends nothing.
3. Walk the folder: regular files only, no links followed, in byte order of file name. Without `--recursive` subfolders are not entered; with it they are, in byte order of the path. Each file is independent and atomic. A refusal goes on to the next file. An I/O error stops the folder and exits 3; completed files stay. Before ordinary traversal on a rerun, recover any matching later file as specified below, then report earlier completed files as `present`.
4. `report.rs` prints `added     <version-id>  <name>`, `present   <version-id>  <name>` and `refused   <code>  <name>`, then `done: 1 added, 1 present, 2 refused` (zero counts are left out). A single-file `add` keeps the one-line form of `add-new-and-edition`. `--json` prints one document, `report_version` first:

   ```json
   {
     "report_version": 1,
     "verb": "add",
     "dir": "inbox",
     "labels": ["owner-supplied", "unlabelled"],
     "inputs": [{ "store": "corpus", "id": "S-0001-v1" }, { "store": "corpus", "id": "S-0002-v1" }],
     "files": [
       { "name": "a-note.txt", "result": "added", "id": "S-0001-v1", "sha256": "<64 hex>", "bytes": 1204, "needs_extraction": false },
       { "name": "b-copy.txt", "result": "present", "id": "S-0001-v1", "sha256": "<64 hex>", "bytes": 1204, "needs_extraction": false },
       { "name": "c-tool.ps1", "result": "refused", "sha256": "<64 hex>", "bytes": 88, "needs_extraction": false, "code": "FB0821" },
       { "name": "d-scan.pdf", "result": "added", "id": "S-0002-v1", "sha256": "<64 hex>", "bytes": 90311, "needs_extraction": true }
     ],
     "counts": { "added": 2, "present": 1, "refused": 1, "needs_extraction": 1 },
     "exit": 1
   }
   ```

   Fields: required `labels` classify the complete report and required `inputs` are sorted distinct store/id references for every admitted or present item; they are not optional text-free control fields. The refusal in this example has unresolved classification. `dir` is the argument as given, without a leading path outside the Corpus; `files` are in byte order of `name`; `result` is one of `added`, `present` or `refused` (`review-queue` adds `proposed` with `--propose`, and its golden row); `id` is the version id (absent for `refused`); `code` is present for `refused` only; `needs_extraction` is true when no built-in extractor reads the file type. The built-in extractors read text, Markdown, HTML and SVG; the list is the const `TEXT_TYPES` in `fieldbook-core/src/add.rs`, next to the `DISALLOWED` list, and the test `text_types_equal_the_extractor_list` of `extract-and-locators` fails when it differs from the list that `extraction::select` reads. `counts` always lists `added`, `present`, `refused` and `needs_extraction`, each 0 when there is none; `counts.needs_extraction` counts files with the flag. `exit` is 3 on an I/O error, otherwise 1 when any file is refused, otherwise 0. A single-file `add --json` gives the same document with one entry in `files` and `dir` set to the directory of the file.

   After the run, classify the entire report as text-bearing request metadata: union owner-supplied, directory/filename markings, applicable loaded-policy State defaults and every input’s labels, including refused inputs. Missing classification stays unlabelled; unknown markings stay marked. Keep admitted/present input references for current-label resolution on later reads. Use the run’s EffectivePolicy (floor before policy loading exists), State admission and Git placement checks before acquiring `journal.state_scope(&lease)?` and replacing `reports/last-add.json` atomically. Any admission, placement, lease or write failure prints "report not saved", preserves the prior report and leaves the command exit unchanged. The Import report page (`status-command`) reads it. Exit 3 on an I/O error, otherwise 1 if any file is refused, otherwise 0.
5. `cli.rs`: `add` takes a file or a folder; `--confirm-public` is the flag of `add-new-and-edition`, given once per run; `--recursive` and `--json` apply to a folder; `--source` with a folder is a usage error (exit 2). The `add <folder>` chip parses and nothing is added to `PENDING`.
6. Tests in `tests/add.rs`: `folder_report_is_in_byte_order`, `folder_adds_present_and_refuses` (one new file, a copy of it, a file with a banner, a `.ps1` file), `refusal_does_not_stop_the_folder`, `exit_is_1_when_any_file_is_refused`, `io_error_exits_3_and_keeps_earlier_files` (a `FaultyWorld`), `recursive_walks_subfolders`, `without_recursive_ignores_subfolders`, `json_report_has_version_1`, `last_report_is_written_and_replaced`, `report_write_failure_is_a_warning`, `needs_extraction_is_true_for_a_pdf`, `source_with_a_folder_is_a_usage_error`, `renamed_executable_is_refused`, `each_added_file_has_one_begin_and_end`, `folder_without_confirmation_refuses_every_file_fb0410` (no flag: every file that passes `check_type` is refused with FB0410, exit 1, no `write.begin`) and `one_confirmation_covers_the_whole_folder` (the flag given once adds every allowed file), `macro_types_are_refused_in_the_corpus_and_pass_for_another_store`, `each_refusal_has_one_refuse_event` and `state_add_folder_matches_the_writer` (`mod support;` for `build_state`).
7. Follow [Adding a JSON report](README.md#adding-a-json-report): the struct `AddReport` in `report.rs` of the core crate, the golden `tests/golden/json/add.json` (the document above, with its four files in byte order of name), and the `add` row of `json_reports.rs`. The test `json_goldens_match_reports` of `check-command` runs the new row.
8. `tests/known-bad/FB0821.toml`: `verb = "add"`, `args = ["inbox"]`, `state = "state-add-folder"`, `expect` FB0821 and exit 1. `state-add-folder` is a state folder whose `settings.toml` holds `role = "lead"` and whose journal is written by `build_state(dir, events)` with the first two events of `state-mini`; its one input is `inbox/tool.ps1`, a synthetic one-line script. This case is isolated: the harness copies `tests/fixtures/corpus-mini` to one temporary folder and makes it the working directory (so `find_root` finds that corpus), copies `tests/fixtures/state-add-folder` to a separate temporary folder passed as `--state`, and copies the fixture's `inbox/tool.ps1` into the working directory's `inbox/` before the run. It asserts FB0821 and exit 1, and that the source fixtures and the live repository are unchanged. Never run it from the real Corpus. The test `state_add_folder_matches_the_writer` rebuilds the journal in a temporary folder and compares it; `settings.toml` and the inbox file are inputs with no writer and are not rebuilt. Run `cargo xtask docs` and commit `cli.md`.

9. `Ok(Refused(..))` continues traversal; `Err(..)` stops it immediately. The partial stdout JSON and best-effort saved report contain completed results and matching counts with `exit = 3`; later files are unvisited. Extend `io_error_exits_3_and_keeps_earlier_files` to check both reports and traversal. Continuing traversal or recording exit 0 or 1 must fail it.

10. Before ordinary folder traversal, enumerate candidate paths/bytes without loading the pending store tree. Under the exclusive lease call pending_key, then match candidates using the single-file OperationKey and request normalization, including options. An unrelated pending verb or zero matching unchanged candidates refuses before any ordinary writes. For the unique match repeat type, label, admission and option checks and call the existing single-file pending_intent/recover path with original allocation, date and complete WriteSet. Record its result once, skip its ordinary add call and print the final report in byte order. Keep transactions independent per file; never wrap the folder in a new transaction.

11. `folder_rerun_recovers_a_later_file` fails the second sources.csv write after the second file’s original is written, using a fail-on-Nth-match FaultyWorld option. Rerun the same folder command and require exactly one row/original per file, no extra transaction begin, cleared pending state and one byte-ordered result per file. `folder_recovery_refuses_missing_or_changed_candidate` removes or changes that second candidate and requires refusal before other writes. Restore each fixture before the next case.

12. `report_state_admission_failure_preserves_previous` denies one report label and checks the warning, intact old report and unchanged add exit. `saved_report_requires_classification` rejects missing labels/inputs. Keep names intact in the JSON golden and saved-report reader; a parse failure becomes unavailable, never public. The later backup producer tests real add-to-backup delivery and input relabelling.

13. `folder_recovery_distinguishes_equal_basenames` recursively reads identical `a/note.txt` and `b/note.txt`, interrupts the first register write and reruns. The shared canonical input-path hash uniquely matches the interrupted candidate: one recovered addition, one Present result, one source row/original, no additional write.begin and cleared pending state.

14. Extend `each_refusal_has_one_refuse_event` to assert the shared reason for all four add refusal codes, required file/name hashes and known labels, exactly one refusal, no write.begin, and successful journal verification. Type refusal before classification has no fabricated labels.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test add --test json_reports
cargo build -q --locked -p fieldbook && B="$PWD/target/debug/fieldbook" && T="$(mktemp -d)" && cp -R tests/fixtures/corpus-mini/. "$T" && mkdir "$T/incoming" && printf 'Synthetic text, made up for tests.\n' > "$T/incoming/a-note.txt" && cp "$T/incoming/a-note.txt" "$T/incoming/b-copy.txt" && printf 'echo synthetic\n' > "$T/incoming/c-tool.ps1"
(cd "$T" && "$B" add incoming --confirm-public; echo "exit $?")
(cd "$T" && "$B" add incoming --confirm-public --json | grep -c report_version)
cargo xtask docs --check
just check
```

Expected: the tests pass, `json_goldens_match_reports` among them; the first `add` prints `added` with a new version id for `a-note.txt`, `present` with the same id for `b-copy.txt`, `refused   FB0821` for `c-tool.ps1`, `done: 1 added, 1 present, 1 refused` and `exit 1`; the second run adds nothing and `grep -c` prints `1`; `docs --check` and `just check` exit 0. Remove the temporary folder afterwards.

## Negative control

In a fresh copy of the fixture (`T2`), make `incoming` hold one file that starts with the two letters `MZ`, named `report.txt`, and run `"$B" add incoming --confirm-public` there: it must print `refused   FB0821`, exit 1, and `diff -r tests/fixtures/corpus-mini/corpus "$T2/corpus"` must print nothing. Then empty the extension list and the magic-number test in `check_type`: `folder_adds_present_and_refuses` and `renamed_executable_is_refused` must fail. Restore it. Then make `check_type` ignore its `store` argument: `macro_types_are_refused_in_the_corpus_and_pass_for_another_store` must fail. Restore it.

Continue traversal or record exit 0 or 1 on I/O error: `io_error_exits_3_and_keeps_earlier_files` must fail. Restore it.

Restore traversal-before-recovery: `folder_rerun_recovers_a_later_file` must fail. Save a report after State admission fails: `report_state_admission_failure_preserves_previous` must fail. Restore each.

Remove directory identity from shared request normalization: `folder_recovery_distinguishes_equal_basenames` must fail. Restore it.

Replace an expected refusal reason or omit its required hash/known-label field: `each_refusal_has_one_refuse_event` must fail. Restore each.

## Rollback

Revert the merge commit. Files added with the folder mode stay in the Corpus as ordinary committed files, and `add <file>` works as before.

## BLOCKED

The single-file path of `add-new-and-edition` cannot be called once per file without writing the register for every file at the end (it writes the row last, per file). Set the item to Blocked with `BLOCKED: add_one cannot be split from add`, and stop. Do not batch the register write.

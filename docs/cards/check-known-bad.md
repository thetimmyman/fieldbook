# check-known-bad

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

One test plants the known-bad example of each check in a copy of `tests/fixtures/corpus-mini` and requires exactly that code. Every code of ranges 01 to 07 is in one of three data files, and every code of ranges 08 and 09 has a file of its own, `tests/known-bad/<CODE>.toml`, so a card that adds a check has to move its codes into the cases. A second test requires that every control of `docs/controls.md` that runs in `check` has a known-bad case, so a control row cannot exist without a negative control.

## Why

- The negative controls of [docs/controls.md](../controls.md) work this way: one violation, one expected error code ([AD-07](../decisions/0001-foundation.md#ad-07-developer-tooling)).
- The known-bad examples are the last column of the [Check catalogue](../reference/corpus-format.md#check-catalogue). For ranges 08 and 09 the cases are per-code files, read by the same harness (`every_code_has_a_known_bad_file`; [Adding a check code](README.md#adding-a-check-code)).

## Scope

- Files: `crates/fieldbook/tests/{known_bad.rs,known_bad_pointers.rs}`, `tests/known-bad/{cases.toml,pending.toml,elsewhere.toml}`.
- Registration lines: `docs/controls.md` (one sentence under the table).
- Shared data: the per-code files `tests/known-bad/<CODE>.toml` of ranges 08 and 09 are added by the cards that raise those codes, with the state folders under `tests/fixtures/` that they name; `state-mini` arrives with `audit-journal`.
- Docs to update: [docs/controls.md](../controls.md), one sentence under the table that says where the harness and its data live and that a control which runs in `check` is named by a case's `control` key.

## Out of scope

- A case for any code except FB0100. Each `check-*` card adds its own, and each card that raises a code of range 08 or 09 adds that code's file.
- The other negative controls (build, edition, guard). They stay in the tests of their cards.

## Preconditions

- Cards merged: `check-command`.

## Steps

1. Write `tests/known-bad/cases.toml` with one `[[case]]`: `code`, `strict` (default false), `edits`, `at` (`path:line`), `also` (other codes the plant is allowed to raise, default none) and `control` (optional: the id of the controls.md row the plant is the negative control for, such as `SC-03`). An edit is one of `{ path, find, replace }`, `{ path, create }` (text), `{ path, delete = true }`, `{ path, append_hex }` or `{ path, prepend_hex }` (bytes written as hex, for an encoding fault or a byte-order mark) and `{ path, crlf = true }` (every LF becomes CRLF). The case for FB0100 sets `schema_version = 9`. For ranges 08 and 09, a code has a file `tests/known-bad/<CODE>.toml` with the fields `verb`, `args`, `state` (a folder under `tests/fixtures/`, for instance `state-mini`) and `expect` (the code and the exit status). The harness runs the verb of the built `fieldbook` binary against a copy of that folder. A code that needs an endpoint runs against `ScriptedHttp`, and a code that needs a socket against a loopback test server in `xtask`; both arrive with the cards that need them. An in-process case substitutes `code` and `in_process` for `verb`, `args`, `state` and `expect`. An HTTP case keeps all four invocation fields and adds `http`, as [Known-bad files](../reference/corpus-format.md#known-bad-files) says.
2. Write `tests/known-bad/pending.toml` with a `[[pending]]` row (`code`, `card`) for each code that `check` will raise, and `tests/known-bad/elsewhere.toml` with an `[[elsewhere]]` row (`code`, `card`, and `test`, empty until the card that owns the code names the test that raises it) for each code raised by something other than `check`. This card is the only writer of rows in `elsewhere.toml`: the card that owns a code sets the `test` key of its row and adds or deletes no row. Use the table below, and add to `pending.toml` a row for each used code of ranges 08 and 09 with the card that raises it: `audit-journal` (FB0800 to FB0805), `backup-snapshot` (FB0810 to FB0814, FB0918), `restore-and-drill` (FB0815, FB0816), `add-folder-mode` (FB0821), `review-queue` (FB0820, FB0822, FB0825, FB0917), `pipeline-model-adapters` (FB0823), `pipeline-rules-classify` (FB0826), `ai-config-and-keys` (FB0900), `model-handoff-ask` (FB0901 to FB0904, FB0915, FB0916, FB0919, FB0925), `model-adapter-openai` (FB0905 to FB0914), `decide-port` (FB0920 to FB0924), `index-output-ports` (FB0930, FB0931), `mcp-server` (FB0940 to FB0944), `chat-bridge` (FB0950 to FB0955), `audit-export` (FB0806), `r2-state-and-journal` (FB0808), `r2-pull-and-sync` (FB0830 to FB0832) and `r2-company-ids` (FB0836). A card that adds a per-code file deletes that code's row. FB0807 (an `Authz` denial, raised in a test by `ScriptedAuthz`) and FB0834 (raised only by contract suite P of the `Pull` port) have no verb that reaches them in the shipped build, FB0833 (a link in a drop folder) needs a fixture that a Windows checkout cannot hold, and FB0835 (a stream outside its source `root`) needs a policy and a folder that a fixture cannot hold on every machine, so each has an `[[elsewhere]]` row, and `review-queue` (FB0807) and `r2-pull-and-sync` (FB0833 to FB0835) set its `test` key. In `pending.toml`, add a `[[pending_control]]` row (`control`, `card`) for each row of controls.md whose Runs cell names the Fieldbook `check` verb (standalone `check`, `fieldbook check` or `(check)`, excluding tool commands such as `cargo deny check`, `deps --check` and tag checks) and that no case names yet; `card` is the row's Brought by cell.
3. Write `known_bad.rs`. It loads the fixture with `io::read_tree(root)` from `check-command`, applies the edits in memory (a `find` must occur exactly once), runs `check::run` with the date in `tests/fixtures/corpus-mini/TODAY` (read with `include_str!`), and compares. A per-code file that has `in_process` in place of `verb` is read and not run, because the harness cannot run a test by name; the `http` key is handled by the card that adds it, as step 1 says.
4. Tests: `the_fixture_is_strict_clean` (zero diagnostics, warnings included, at the date in `TODAY`: the test that fails when a fixture date moves outside the window); `each_case_raises_its_code` (one diagnostic with the case's code at `at`, plus the `also` codes, and nothing else); `every_code_is_accounted_for` (the three files together name every code of FB0100 to FB0709 in `Code::ALL` once; no code is in two files); `every_code_has_a_known_bad_file` (each used code of ranges 08 and 09 has a file `tests/known-bad/<CODE>.toml` or a `pending.toml` row, and not both; FB0807, FB0833, FB0834 and FB0835 have an `elsewhere.toml` row instead, and once its `test` key is set the named test must exist; a file that parses is run, except a file with `in_process`, which is accounted for and not run, and a file whose `state` folder is missing fails); `an_edit_must_apply_exactly_once`; `every_check_control_has_a_known_bad_case` (reads `docs/controls.md` with `include_str!`, takes each row whose Runs cell names the Fieldbook `check` verb (standalone `check`, `fieldbook check` or `(check)`, excluding tool commands such as `cargo deny check`, `deps --check` and tag checks), and requires its id in some case's `control` key or in a `[[pending_control]]` row, and not in both); `runs_cell_selects_only_fieldbook_check` (positive verb forms and the excluded tool commands); `in_process_names_an_existing_test`, in `known_bad_pointers.rs` (for every `tests/known-bad/*.toml` with `in_process = "<crate>/<test file>/<test name>"`, the file `crates/<crate>/tests/<test file>.rs` exists and contains `fn <test name>`; it passes when no file has the key).

| Card | Codes |
|---|---|
| `check-files` | FB0120 to FB0125 |
| `check-registers` | FB0200 to FB0203 |
| `check-register-cells` | FB0114, FB0204, FB0205, FB0206, FB0210 |
| `check-claim-cells` | FB0306, FB0307 |
| `check-register-links` | FB0207, FB0208, FB0211, FB0214 |
| `check-promotion` | FB0209, FB0213, FB0220, FB0221, FB0222, FB0230 |
| `check-pages` | FB0101 to FB0109, FB0116 |
| `check-page-links` | FB0110 to FB0113, FB0115, FB0136 |
| `check-page-parts` | FB0130 to FB0133, FB0137 |
| `check-page-body` | FB0134, FB0135, FB0138, FB0139 |
| `check-citations` | FB0300 to FB0303, FB0305 |
| `check-citation-rules` | FB0117, FB0118, FB0309, FB0310, FB0320 |
| `check-scans` | FB0400 to FB0405, FB0410 |
| `calc-cases` | FB0500 |
| `extract-and-locators` | FB0212, FB0223 |
| `quote-match-and-anchors` | FB0304, FB0308 |
| `search-cli-and-regression` | FB0126, FB0127, FB0128 |

Those 17 cards go in `pending.toml`. In `elsewhere.toml`: `calc-cases` (FB0501 to FB0504; `calc-day1` merges before this card, so `calc-cases` is the first card that can set these keys), `build-v0` (FB0600, FB0606), `edition-zips` (FB0601, FB0602, FB0608), `build-site-lints` (FB0603, FB0604, FB0605), `day-one-binder-and-registers-xlsx` (FB0607), `handoff-context` (FB0700), `handoff-cite-check` (FB0701 to FB0707), `handoff-new-note` (FB0708, FB0709), `core-labels` (FB0411, FB0412, FB0415), `core-policy` (FB0413), `policy-load` (FB0414, FB0418, FB0419), `r2-company-ids` (FB0416, FB0417, FB0420), `r2-pull-and-sync` (FB0215, FB0216, FB0833, FB0834, FB0835), `r2-records` (FB0240, FB0241, FB0380, FB0381, FB0382), `extract-csv-rows` (FB0311) and `review-queue` (FB0807). Each of those cards sets the `test` key of its row in its own known-bad step, names `tests/known-bad/elsewhere.toml` under Registration lines, and adds or deletes no row.

5. Add parser tests `http_case_keeps_invocation_fields` and `http_case_requires_state_and_expect` in the known-bad harness: accept a complete invocation plus http; remove state and expect independently and require a typed parse error. HTTP execution arrives with its adapter.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test known_bad --test known_bad_pointers
just check
```

Expected: both succeed; the run includes the tests of step 4.

## Negative control

Remove the FB0100 case from `cases.toml`: `known_bad::every_code_is_accounted_for` must fail and name FB0100. Restore the file. Then remove the `audit-journal` row for FB0800 from `pending.toml`: `known_bad::every_code_has_a_known_bad_file` must fail and name FB0800. Restore the file. Then remove the `model-handoff-ask` row for FB0925: the same test must fail and name FB0925. Restore the file. Then remove one `[[pending_control]]` row: `known_bad::every_check_control_has_a_known_bad_case` must fail and name that control. Restore the file. Then change `at` to the wrong line: `known_bad::each_case_raises_its_code` must fail and show both lines. Restore the file. Then add a temporary `tests/known-bad/FB0999.toml` with `in_process = "fieldbook/none/missing"`: `known_bad_pointers::in_process_names_an_existing_test` must fail and name the file. Remove it.

Select every Runs cell containing the word check: `runs_cell_selects_only_fieldbook_check` must fail on the tool-command rows. Restore it.

Allow an HTTP case missing state or expect: `http_case_requires_state_and_expect` must fail. Restore it.

## Rollback

Revert the merge commit. The three data files go with it.

## BLOCKED

The fixture is not clean under `--strict`. Fix the fixture if the fault is in the fixture. If the fault is in a rule, write the code and stop.

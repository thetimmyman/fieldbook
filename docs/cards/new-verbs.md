# new-verbs

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook new page|claim|question|conflict|term` starts a page from its template or appends a register row with the next free id, prompting for a missing flag only when standard input is a terminal.

## Why

- The verbs and flags are [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience); the chip forms they must accept are the verb map in [Strings](../reference/corpus-format.md#strings). Ids and allocation are [Identifiers](../reference/corpus-format.md#identifiers). Page templates live in `corpus/templates/<type>.md` ([AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience)); front matter is [Page files](../reference/corpus-format.md#page-files).
- A new claim enters `unchecked` with the entry values of [Claim vocabularies](../reference/corpus-format.md#claim-vocabularies) and the [Promotion rules](../reference/corpus-format.md#promotion-rules).
- Design: the [MAP.md](../design/MAP.md) row "CommandChip", for the `new` chips only. [HANDOFF.md](../design/HANDOFF.md) [§5 Components](../design/HANDOFF.md#5-components) (CommandChip). No template or style changes. No token group.

## Scope

- Files: `crates/fieldbook/src/{lib.rs,cli.rs,new.rs}`, `crates/fieldbook-core/src/{lib.rs,new.rs}`, `crates/fieldbook/tests/new.rs`, `crates/fieldbook/tests/chips.rs`, `tests/fixtures/corpus-mini/corpus/templates/` (only if the fixture has no templates: copy `concept.md` and `brief.md` from `corpus/templates/`), `docs/reference/cli.md` (regenerated), `CHANGELOG.md`.
- Registration lines: the `new` verbs in the Commands line of `AGENTS.md`, and the quick start lines of `README.md` for a page and a claim; `crates/fieldbook/tests/cli.rs` (remove `new page`, `new claim`, `new question`, `new conflict` and `new term` from `NOT_BUILT`, this card's verbs only); `crates/fieldbook/tests/support/faulty.rs` (the forwarding lines for `stdin_is_terminal` and `read_line`, this card's entries only); `crates/fieldbook/tests/support/world.rs` (the `stdin_terminal` and `input` fields with `set_stdin_terminal(bool)` and `push_input_line(&str)`, this card's entries only); `crates/fieldbook/src/io/mod.rs` (the Disk bodies of stdin_is_terminal and read_line); `crates/fieldbook/src/io/mod.rs` (World::prompt and Disk stderr writer); `crates/fieldbook/tests/support/{world.rs,faulty.rs}` (prompt capture/forwarding and output failure).; `crates/fieldbook-core/src/strings.rs` (the new-verb chip strings); `docs/reference/corpus-format.md` (new-verb chip forms and public-act wording)
- Docs to update: `AGENTS.md` (Commands: the `new` verbs), `README.md` (the quick start lines for a page and a claim), `cli.md` through `cargo xtask docs`, `CHANGELOG.md` (`- Added: the new command for pages, claims, questions, conflicts and terms.` under `## Unreleased`).

## Out of scope

- Matching a claim's quote against the extracted text when the claim is written. There is no extraction or quote normalisation yet; `quote-match-and-anchors` adds the match to `new claim`.
- `new note` (Day 30). Editing anything in `corpus/templates/`: the templates are read, never written.

## Preconditions

- Cards merged: `core-schema-claims` (`ClaimRow::entry`), `core-ids-and-dates` (`next_claim_id`, `next_question_id`, `next_conflict_id`), `add-new-and-edition`, `audit-journal` (`TestWorld`), `build-v0` (both cards regenerate `docs/reference/cli.md` and add rows to `AGENTS.md` and `README.md`, so one merges before the other), `check-command` (`tests/chips.rs` and its `PENDING` list).

## Steps

1. In `fieldbook-core/src/new.rs`, pure planners. `plan_page(model: &Model, id: &PageId, ty: PageType, subarea: &str, template_text: &str) -> Result<PagePlan, NewError>` returns `PagePlan { path: PathBuf, text: String }`, the file path under `corpus/` and the template with its front matter filled. `plan_row(model: &Model, kind: RowKind, area: Option<&AreaCode>, fields: &Fields) -> Result<RowPlan, NewError>` returns `RowPlan { path: PathBuf, cells: Vec<String>, id: String }`, the register path and the row with the next free id in its series (claims per area; `next_claim_id`, `next_question_id` and `next_conflict_id` of `core-ids-and-dates`; a term takes its id from `fields`). `RowKind` is `Claim`, `Question`, `Conflict` or `Term`, and `Fields` maps a register column to the value of its flag. `NewError` has one variant for each of: a page id that exists, an unknown area, subarea or type, a version id not in `sources.csv`, and allocator exhaustion (carrying the typed Exhausted family).
2. In `new.rs`, collect any missing flags before taking a lease. Then take `world.write_lease()`, open the journal, read the tree through `world.read_tree_under(&lease)` and its template, call the planner, then classify and admit as below before calling `Journal::begin`. Keep the lease through `Journal::end`, so planning and writing use one generation. Apply the plan, write the page or append the row in normal form through `world.write_file` (from `add-new-and-edition`), and print the new id. `World` gains `stdin_is_terminal() -> bool` and `read_line() -> String` (empty at end of input). `Disk` uses `std::io::IsTerminal` and reads one line of standard input. `TestWorld` (`audit-journal`) gets `set_stdin_terminal(bool)` and `push_input_line(&str)`, and `FaultyWorld` forwards both. A missing required flag prompts on a terminal: call `World::prompt(&str) -> Result<(), WorldError>` to write `<flag>: ` on standard error and flush it before calling read_line; a write/flush failure exits 3. Disk implements the actual writer in io; TestWorld records flushed prompts and FaultyWorld forwards or fails them. Do not repeat the prompt in Outcome.stderr, and an empty answer is the same usage error as a missing flag. Without a terminal a missing flag is a usage error (exit 2) and nothing is written. Exit codes: 0 when the page or row is written; 1 for admission or typed-public refusal; 2 for a usage error, which is each `NewError` and a missing flag without a terminal; 3 for an I/O failure. `new` does not run `check`: a cell that no flag fills keeps its required entry default, or is empty when optional; `check` reports any remaining invalid value. `quote-match-and-anchors` adds exit 1 for a quote that does not match (FB0308).
3. `new claim --confirm-public --area --source --at --quote` builds the claim with `ClaimRow::entry` of `core-schema-claims` (`unchecked`, with the entry values), sets the version id, the locator and the quote from the flags, and writes it. `new question --confirm-public --area <a> "<text>"` takes the text as its one positional argument and writes the allocated ID, area, question, `day_one = false` and `status = open`; optional and company-answer cells stay empty.
4. Add the subcommands to `cli.rs` so that every `new` chip in the verb map parses. Remove `new` from `PENDING` in `tests/chips.rs`, and `new page`, `new claim`, `new question`, `new conflict` and `new term` from `NOT_BUILT` in `crates/fieldbook/tests/cli.rs`.
5. Tests in `tests/new.rs`, each on a temporary copy of `tests/fixtures/corpus-mini`: `new_page_from_template`, `new_claim_takes_next_id_in_its_area`, `next_id_counts_retired_rows`, `new_question_takes_text`, `new_conflict_and_term`, `missing_flag_without_terminal_is_usage_error` (it calls `set_stdin_terminal(false)`), `missing_flag_on_a_terminal_is_prompted` (terminal true, one input line: the row is written with that value), `unknown_version_writes_nothing`, `templates_are_not_changed`. In the existing new-verb test suite, `prompt_holds_no_store_or_journal_lease` pauses a missing-flag prompt, completes another writer, then resumes and checks that the new id was allocated from the updated tree.
6. Run `cargo xtask docs` and commit the regenerated `cli.md`.


7. Resolve matching pending `new` intent before existence checks and ID allocation. Persist the typed subcommand, request hash, original allocated ID and complete path/hash plan. Resume that plan without allocating again; all after-hashes finish the original transaction without another content write. Tests `new_page_recovers_after_write_before_end` and `new_row_recovery_keeps_the_original_id` check one item, the original ID, cleared pending state and successful reads; third hashes and changed requests are refused. Bypassing recovery or reallocating must fail them. Real `new claim` events both carry its affected reference and are found by `audit show --id`; removing it fails. Extend `new_question_takes_text` to assert both defaults and strict validation; omitting either default fails.

8. `id_exhaustion_writes_nothing` in `tests/new.rs` runs real claim and question commands at their 9999 and 999 limits. Require `<family> id space exhausted`, exit 2, unchanged content and no write.begin. Keep conflict allocation unbounded by any invented four-digit cap. Recovery retains original allocated IDs before considering a fresh allocation.

9. Extend `missing_flag_on_a_terminal_is_prompted` and `prompt_holds_no_store_or_journal_lease` with a bounded interaction through the real prompt writer that withholds input until stderr is flushed. Complete another writer during that wait, then send input and assert the updated allocation and one prompt only. A failed prompt write/flush exits 3 without content writes.

10. Use the journal OperationIntent for new: the key binds the new leaf verb and destination store; the normalized request includes the kind, supplied ID, area, type, subarea, template hash and all final field values and confirm-public. Store only its hash, original allocations and planning date. Match recovery before planner existence/allocation checks and pass the intent and exclusive lease to begin.

11. Add `--confirm-public` to each leaf command. Before fresh writes or recovery, reuse `add::label_context(world, confirm_public)`, classify all supplied fields and final planned text with the current marking rules, union applicable source/template input labels, and call `labels::admit(..., &StoreRule::corpus())`. Populate WriteSet.labels from this admitted result. No flag gives FB0410/reason unlabelled; marked text gives FB0400/reason store even with the flag; a typed act on a machine with a company store gives FB0822/reason label. Each exits 1, records one refusal, changes no content and adds no new write.begin. Recovery repeats these checks before resuming the original intent. Update clap, the registered chip strings and generated cli.md together.

12. Add `new_admission_precedes_write_and_recovery` through the real handler for all five leaf commands: public confirmation succeeds, missing confirmation and marked content refuse, and a company-store TestWorld refuses the typed act. Repeat the three refusal cases against an interrupted original operation and assert no content change or new write.begin. Existing success tests and examples supply confirmation.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test new
cargo nextest run --locked -p fieldbook --test chips
cargo build -q --locked -p fieldbook && B="$PWD/target/debug/fieldbook" && T="$(mktemp -d)" && cp -R tests/fixtures/corpus-mini/. "$T" && TD="$(cat tests/fixtures/corpus-mini/TODAY)"
(cd "$T" && "$B" new page --confirm-public cmp-synthetic-test --type concept --subarea business-systems); echo "exit $?"
(cd "$T" && "$B" new question --confirm-public --area cmp "Synthetic question, made up for tests?" && "$B" check --strict --today "$TD" corpus/registers/questions.csv); echo "exit $?"
cargo xtask docs --check
just check
```

Expected: both test runs pass; `new page` prints the new page's path and `exit 0`; `new question` prints the next `Q-` id, and `check` on `questions.csv` reports no diagnostics and `exit 0`; `cargo xtask docs --check` and `just check` exit 0.

## Negative control

In the temporary copy, run `"$B" new claim --confirm-public --area cmp --source S-9999-v1 --at "p. 1" --quote "synthetic" < /dev/null`. It must exit 2 and name `S-9999-v1`, and `diff -r tests/fixtures/corpus-mini/corpus "$T/corpus"` must show only the page and the question row added in Acceptance. Remove the copy. Then, on the branch, make `new` never prompt: `missing_flag_on_a_terminal_is_prompted` must fail. Restore it.

Move the write lease before the missing-flag prompt: `prompt_holds_no_store_or_journal_lease` must fail at a bounded timeout. Restore it.

Bypass recovery or reallocate an ID on rerun: `new_page_recovers_after_write_before_end` or `new_row_recovery_keeps_the_original_id` must fail. Remove affected references from new claim: the `audit show --id` assertion must fail. Omit either question default: `new_question_takes_text` must fail. Restore each.

Let exhausted claim/question allocation succeed or wrap: `id_exhaustion_writes_nothing` must fail. Restore each.

Buffer the prompt until Outcome or skip its flush: the bounded prompt interaction must fail. Restore it.

Bypass new’s classification or admission before begin/recovery: `new_admission_precedes_write_and_recovery` must fail. Restore each.

## Rollback

Revert the merge commit. Pages and rows already made with `new` stay; they are ordinary Corpus files.

## BLOCKED

A `new` chip in the verb map cannot be parsed without changing its string (a change to the frozen verb map): set the item to Blocked with a `BLOCKED:` comment that says so, and stop.

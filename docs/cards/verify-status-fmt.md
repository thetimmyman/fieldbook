# verify-status-fmt

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

`fieldbook verify <claim-id> --on YYYY-MM-DD` shows the claim, its source version, locator and quote, and asks for a yes at a terminal. Only then does it write the verification fields with `verified_by_role` set to the person's role (`owner` or `lead`), by the promotion rules. With standard input that is not a terminal it exits 2 and writes nothing, so it never runs in CI or for Codex.

## Why

- The verb is in [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience) (owner, interactive). Verification of a consequential claim is the status gate ([Gates](../decisions/0001-foundation.md#gates)), so the review is the owner's.
- Roles. The owner verifies any claim, and only the owner verifies a consequential one; the lead verifies the rest; Codex never raises a status. The role is the one in `settings.toml` that `audit-journal` reads into the journal's actor: `owner`, `lead`, or `agent` when none is set. Only `owner` and `lead` may verify; with `agent`, the verb prints the line to add (`role = "owner"` or `role = "lead"`) and exits 2.
- What the fields may hold, and the conditions for `primary_source_supported`, are in [Promotion rules](../reference/corpus-format.md#promotion-rules) (FB0209). The row is written in normal form by `core-csv`'s writer.
- This is a Could ([Cut line and floor](../decisions/0001-foundation.md#cut-line-and-floor)). If it is cut, the owner verifies the 15 consequential claims by hand edit.
- Design: the [MAP.md](../design/MAP.md) row CommandChip gains `verify`; its chip is the verb map's in `fieldbook-core::strings`, labelled "At home". No page. [HANDOFF.md](../design/HANDOFF.md) §5 (CommandChip). Token groups: none.

## Scope

- Files: `crates/fieldbook/src/{cli.rs,verify.rs}`, `crates/fieldbook-core/src/verify.rs`, `crates/fieldbook/tests/verify.rs`, `crates/fieldbook/tests/chips.rs`.
- Registration lines: `tests/support/chip_cases.rs` (the `fieldbook verify` case with its `Terminal` expectation, this card's entry only); `crates/fieldbook-core/src/lib.rs` (the verify module line); the quick start line of `README.md` (the owner verifies with `fieldbook verify`); `crates/fieldbook/tests/cli.rs` (remove `verify` from `NOT_BUILT`, this card's verb only); `crates/fieldbook/src/lib.rs` (the `mod verify;` line and the dispatch arm of `verify`, this card's entries only); `crates/fieldbook-render/src/chip.rs` (the value `VERIFY_LANDED`, step 12) and the reader-page snapshots it changes.
- Docs to update: `README.md` (one line: the owner verifies with `fieldbook verify`), `CHANGELOG.md` (`- Added: the verify command, for the owner at a terminal.`), `docs/reference/cli.md` (generated: run `cargo xtask docs`, never hand-edit).

## Out of scope

- `status` (`status-command`) and `fmt` (`fmt-command`).
- Setting `contradicted`, `stale` or `unresolved`: those stay hand edits, because they need a conflict or question row.
- Verifying any claim in the repository's Corpus. Codex never runs the command there.

## Preconditions

- Cards merged: `guard-statuses`, `audit-journal` (`Journal::open`, `begin`, `end`, the `Audited` token and the actor's role), `check-command` (`tests/chips.rs` with its `PENDING` list), `add-new-and-edition` (`World::write_file` and the `FaultyWorld` of `tests/support/faulty.rs`), `new-verbs` (`World::stdin_is_terminal` and `read_line`).

## Steps

1. `fieldbook_core::verify::plan(model, claim_id, on, role: audit::Role) -> Result<VerifyPlan, Refusal>`: find the claim, its source version, locator and quote. Refuse, naming the rule, when a condition of FB0209 other than the person's read fails, or when the claim is already verified on that date, or when the claim is consequential and the role is not `owner`. Every date comparison of FB0209 uses `on` as today: `verify` reads no clock. The plan holds the new row: `verification = primary_source_supported`, `verified_on = on`, `verified_by_role` = the role. The role in the plan is `owner` or `lead`; the caller has refused every other role (step 2). Add `verify` to the clap `Cli` and remove it from `PENDING` in `crates/fieldbook/tests/chips.rs` (`build-about`) and from `NOT_BUILT` in `crates/fieldbook/tests/cli.rs`. Register its case in `tests/support/chip_cases.rs` with the `Terminal` expectation: a fresh Home copy, the documented stderr `needs a terminal`, and no success exit.
2. The order is fixed. First, when `World::stdin_is_terminal()` (from `new-verbs`) is false, print "verify needs a terminal; it is for the owner at home" and exit 2 before reading anything and before `Journal::open`, so that this run writes nothing at all. Then `Journal::open`, which gives the `Actor` with the role from `settings.toml` and releases its initialization lease. After checking matching pending verification as described below, load the display snapshot through ordinary `World::read_tree`; neither lease is retained for the prompt. If the role is not `owner` or `lead`, print the role message, "verify needs a role: add the line " followed by `ROLE_LINE` and " (or `role = "lead"`) to settings.toml in the state folder", and exit 2 before the claim is shown; nothing is written to the Corpus. The constant is `pub const ROLE_LINE: &str = "role = \"owner\"";` in `crates/fieldbook-core/src/verify.rs`, and the two messages are built in `verify.rs`. Both messages ("needs a terminal" and "needs a role") go to standard error; standard output stays empty.
3. Show the claim id, statement, source version id and title, locator and quote, then ask `Verified at this locator on <date>? Type yes:`. Any answer but `yes` exits 1 and writes nothing.
4. On `yes`, take `world.write_lease()`, reload through `world.read_tree_under(&lease)` and refresh the actor role. Pass that role to `verify::plan` on both the display and final plan. Compare the claim and all evidence shown with the displayed snapshot and rerun its eligibility checks. If anything changed, print `claim changed; run verify again`, exit 1 and write nothing. Otherwise build `claims.csv` with `table::write` and check that the result `is_normal`. Then call `Journal::begin(&lease, &intent, &write_set)` with `WriteSet { labels: LabelSet::public(), files, affected: vec![claim_ref] }`, where `claim_ref` is the claim’s store-qualified `audit::RecordRef` and `files` holds one `WriteFile` for `corpus/registers/claims.csv` with the SHA-256 of those bytes; write them with `World::write_file(&token, path, bytes)` (added by `add-new-and-edition`); and call `Journal::end(token, outcome)`. Print one line naming the claim and the date.
5. Tests drive `run` with the `TestWorld` of `audit-journal` over a temporary copy of the fixture: `yes_writes_exactly_the_three_cells`; `no_writes_nothing`; `stdin_not_a_terminal_exits_2_and_writes_nothing` (`set_stdin_terminal(false)`; no journal line either; the first message is on standard error, standard output is empty and the exit is 2); `verify_without_a_role_prints_the_line_and_exits_2` (a `settings.toml` with no `role` gives the role `agent`: the role message on standard error, `exit 2`, the Corpus unchanged and no `write.begin`); `an_unknown_id_exits_1`; `a_tier_c_source_is_refused`; `a_consequential_claim_needs_the_owner` (refused for the role `lead` and accepted for `owner`, and a claim that is not consequential is accepted for both); `an_impossible_date_exits_2` (`--on`); `verify_records_role_in_the_journal` (after a yes, the journal holds one `write.begin` and one `write.end` for the verb `verify` whose `role` is the role of the `settings.toml`, whose `labels` are `public`, and `audit verify` on the state folder prints `journal ok`); `verify_prompt_holds_no_lease` (another writer completes while the prompt waits), `changed_evidence_after_prompt_is_refused` (that writer changes the displayed evidence; yes writes no status cell and no write.begin); and `check_strict_is_clean_after_yes` (run `check --strict` on the copy after the yes case: it must print no new finding).
6. Run `cargo xtask docs` and commit the changed `docs/reference/cli.md`.

7. The verification planner uses the same instrument-numbering requirements as the promotion pass. Test each instrument and a permitted empty counterpart through the existing verify suite; a missing required cell refuses before confirmation or any write.

8. After terminal and role checks, inspect pending verification before the ordinary read or already-verified refusal. Bind claim ID, `--on` date and role to the original request. Under the exclusive lease validate the complete paths and hashes. A before-state retry retains evidence checks and confirmation outside the lease, then reloads and rechecks. An all-after retry finishes the existing transaction without verifying again or adding another begin. Keep no-begin discard and successful-end cleanup. Test faults before the CSV write, after it and during completion; changed requests, unauthorized roles and third hashes refuse. Require ordinary reads to recover and exactly one successful transaction. Bypassing recovery must fail.

9. Extend `verify_records_role_in_the_journal` to require the verified claim’s affected reference in both write events and retrieve both through audit show --id.

10. For verify, OperationKey binds the claim reference and store. The request hash binds the requested status, role, explicit date option and hashes of the originally displayed evidence and confirmation; absent date is a fixed absent field, while planned_on preserves the first planning date. pending_intent is checked before eligibility/no-op branches, then recover verifies the full reconstructed claim-row plan. A rerun must obtain the same interactive confirmation; changed evidence or role refuses.

11. `verify_refuses_unsupported_extension` calls `verify::plan` with a structurally valid unknown locator and requires refusal while the locator is unavailable, preserving its kind/payload for display.

12. Set `VERIFY_LANDED` in `crates/fieldbook-render/src/chip.rs` to `true`. The overdue fold then shows the chip on each row in place of its landing line; the page code and both branches are already built and tested, so change nothing else there. `verify_landed_matches_the_command_line` in `crates/fieldbook/tests/chips.rs` fails until the verb and the value agree. Accept only the reader-page snapshots this changes, and read each changed line.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook --test verify
cargo run -q --locked -p fieldbook -- verify CL-SYN-0001 --on 2025-01-02 < /dev/null; echo "exit $?"
cargo xtask docs --check
just check
```

Expected: every test passes; the second command exits 2 and prints the terminal message, and `git status --short corpus/` is empty and no journal line was added; `docs --check` and `just check` exit 0.

The owner also runs `fieldbook verify <synthetic claim id> --on <date>` at a terminal in a temporary copy of the fixture and records `fieldbook verify <synthetic id> --on <date> @ <sha> (owner at home) -> pass`.

## Negative control

Remove the terminal test from `run`. The test `stdin_not_a_terminal_exits_2_and_writes_nothing` must fail. Restore it. Then print the first message on standard output: the same test must fail. Restore it.

Hold a lease across the prompt or skip the post-prompt evidence comparison: `verify_prompt_holds_no_lease` or `changed_evidence_after_prompt_is_refused` must fail. Restore each.

Empty the affected references: `verify_records_role_in_the_journal` must fail. Restore it.

Treat an unsupported extension as available: `verify_refuses_unsupported_extension` must fail. Restore it.

## Rollback

Revert the merge commit. `verify` becomes a usage error; the owner verifies by hand edit. Rows already written stay valid.

## BLOCKED

The owner wants `verify` to set a value other than `primary_source_supported`. No document says it may: stop and ask.

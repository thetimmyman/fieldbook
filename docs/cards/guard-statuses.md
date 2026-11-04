# guard-statuses

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask guard-statuses BASE HEAD` already judges content, not paths (GS-02): an agent commit may add and edit sources, originals, claims, questions and pages, and may not certify its own work. What is left is the reader. `xtask/src/guard_fields.rs` reads `claims.csv` with its own RFC 4180 reader and a page's `status` with its own front-matter scan; this card swaps both for the `fieldbook-core` helpers, so the guard and `fieldbook check` read a register and a page the same way. No rule changes.

## Why

- The rule and the agent identity are in [AD-21](../decisions/0001-foundation.md#ad-21-codex-execution-plan) and GS-02 of [controls.md](../controls.md). A raise and the entry values are defined in [Promotion rules](../reference/corpus-format.md#promotion-rules). The card changes no status and no rule, so it is `auto`.
- Two readers of one format drift. `table::read` (`core-csv`), `ClaimRow` and `is_raise` (`core-schema-claims`), and `page::split` and `parse_meta` (`core-schema-pages`) are the readers `check` uses.
- An agent commit is one whose author name ends in "(agent)" or that carries a `Co-authored-by` trailer. That stops mistakes and an unattended agent, not a determined actor; the repository ruleset and the owner's weekly read are the backstop.

## Scope

- Files: `xtask/src/{guard.rs,guard_fields.rs}`. `xtask`'s path edge to `fieldbook-core` already exists (`day1-dependencies-3`), so no manifest or lock change.

## Out of scope

- Any change to what the guard refuses or allows. A new rule is the owner's to write in GS-02 first.
- `fieldbook verify` (`verify-status-fmt`) and FB0209, which `check` already raises.
- Any change to `ci.yml`: `ci-ok` already runs `guard-statuses`.

## Preconditions

- Cards merged: `core-csv`, `core-schema-claims`, `core-schema-pages`.

## Steps

1. In `guard_fields.rs`, parse `claims.csv` with `table::read` and pair rows by `claim_id`. Replace `claim_problems` with `is_raise(before, after)` for the certify fields, and keep the evidence-edit rule (an edit of `statement`, `value`, `unit`, `source_id`, `version`, `locator_kind`, `locator` or `quote` on a claim that is not `unchecked` fails unless the same commit resets the check).
2. Read a page's `status` with `page::split` and `parse_meta`.
3. Delete `csv`, `claims` and `status` from `guard_fields.rs`, and their unit tests. A blob that does not parse still fails with "cannot judge".
4. Keep every test in `guard.rs` and its `// control: GS-02` markers unchanged. They are the proof the swap kept the rule.

## Acceptance

```sh
cargo nextest run --locked -p xtask
cargo xtask guard-statuses "$(git merge-base origin/main HEAD)" HEAD
just check
```

Expected: every test passes; the guard on this branch prints `guard-statuses: ok` and exits 0; `just check` exits 0.

## Negative control

Make `is_raise` return false for `verified_by_role`: `an_agent_setting_verified_by_role_fails` must fail. Restore it. Skip `statement` in the evidence rule: `statement_edit_on_verified_claim_fails` must fail. Restore it. Judge a merge against its first parent only: `agent_merge_keeps_owner_status_and_refuses_a_resolution_that_raises` must fail. Restore it.

## Rollback

Revert the merge commit. The xtask reader returns; the rule is the same either way.

## BLOCKED

The core helpers read a register or a page differently from the xtask reader on a case the guard tests cover: write the case and stop. The format owner decides which reading is right.

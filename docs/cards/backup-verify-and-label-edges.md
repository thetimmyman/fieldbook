# backup-verify-and-label-edges

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A snapshot made by `fieldbook backup` always holds the journal segments of `state/audit/`, whatever the target's `include` says, so `backup verify` passes on every snapshot the command writes. An original under `corpus/sources/public/` takes a source's labels only when its file name is a version id and an extension (`S-0001-v1.pdf`), not when it carries a bare source id or a longer name that begins with one; such a file is `unlabelled` and keeps the labels the old rule matched.

## Why

- Part 1, confirmed. A target whose `include` omits `state/audit` writes a snapshot that numbered verify always fails with `FB0802` (`check_snapshot_journal` in `crates/fieldbook/src/backup.rs`, step 6 of `run_verify_numbered`): the manifest records `journal_seq` and `journal_head` and the snapshot holds no journal to reach them. The judge's probe confirmed it: `include = ["corpus/registers"]` gave backup exit 0 with 5 files and `backup verify` `FB0802`, exit 1. `restore --to` of `restore-and-drill` re-verifies the copy it makes and would fail on the same snapshot, so the owner would find out at the restore, the worst time. The existing test `backup_refuses_registers_when_corpus_toml_does_not_load` and its neighbours use such a target.
- Part 2, confirmed. In `classify_file` (`crates/fieldbook/src/io/backup_classify.rs`, about line 137) an original matches a source when `file_name.split('.').next()` equals the version id or the bare source id. A file `S-0001.html`, or `S-0001-v1.draft.txt`, an unregistered name no build produces, takes the handling of source `S-0001`. The egress check then tests the file against the wrong labels. A wrong label can move a file either way: at floor B the backup floor row of `policy.md` lets a snapshot to a `company_storage = true` target hold `unlabelled`, and step 2 of the decision order denies a label that is not in the table but lets `unlabelled` through to that target. So a file that takes a source's not-in-table labels today and is refused would pass as plain `unlabelled` if the fix only dropped the match.
- Dropped from this card, each with its reason:
  - A third item, over-labelling (a file labelled by a broader set than its own), is fail-closed: the snapshot is refused or stays inside the strictest target, never wider. The judge adopted it in its round 5 as the rule, so it is not a defect.
  - A fourth item, the unlabelled default for a store under a loaded policy, belongs to `policy-load`: that card owns the policy loader and is a policy-gate card with `Review: owner`. Changing it here would touch a gate module without its review.
- Lead decisions, each one the repository does not settle:
  1. Part 1 is fixed in `plan_backup` (`crates/fieldbook-core/src/backup/plan.rs`), which always adds the files of `state/audit/` to the plan, whatever `include` holds. The audit folder is text-free and public by the contract ([audit contract](../reference/audit-contract.md), the label fields paragraph). `include = ["audit"]` keeps working and adds nothing twice. The other fix, to leave the audit out and have verify skip the journal check, would make a snapshot that cannot be checked, which the `backup verify` promise rules out.
  2. Part 2 drops the bare source-id match and requires the file name to be exactly `<version-id>.<ext>`, with `<ext>` one or more ASCII letters or digits and no further dot. The register has no registered file-name column to compare, and the version id is what every build names an original by. Matching the file's hash with the row's `sha256` would be exact, but the synthetic fixtures of `tests/backup.rs` carry a zero hash, so it is a later card's change. A name that fails the rule gets `LabelSet::unlabelled()` unioned with the labels the old rule matched (the bare or prefixed source's labels), so the result is never wider than either the old set or plain `unlabelled`, and a file the old rule refused stays refused.
- Gate: none. Part 2 never gives a file fewer labels than both the old rule and plain `unlabelled` would: a nonconforming name carries `unlabelled` and the labels the old rule matched, so at floor B it is refused wherever the old rule refused it. It touches `io/backup_classify.rs`, not a label module of `fieldbook-core`. `plan.rs` is the backup plan, not a gate module. `Review: auto`.

## Scope

- Files: `crates/fieldbook-core/src/backup/plan.rs`, `crates/fieldbook/src/io/backup_classify.rs`, `crates/fieldbook-core/tests/backup.rs`, `crates/fieldbook/tests/backup.rs`, `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Fixed: a backup always holds the journal, so every snapshot verifies; an original takes a source's labels only by its version id.`).

## Out of scope

- The unlabelled default for a store under a loaded policy (`policy-load`).
- Matching an original by its hash, and over-labelling.
- `crates/fieldbook/src/backup.rs`: its active-segment step calls `plan_backup` with `include_rules` and keeps working unchanged; the card reads it and does not edit it.
- The memory and recording changes of `backup-bounded-memory` and `backup-journal-recording-gaps`, and any `restore` code.

## Preconditions

- Cards merged: `backup-snapshot`, `restore-and-drill` (a snapshot that fails verify fails its re-verify), `backup-bounded-memory` (it changes `io/backup_classify.rs`), `backup-journal-recording-gaps` (it changes `backup.rs` and `io/backup_classify.rs`).

## Steps

1. In `crates/fieldbook-core/tests/backup.rs` add `plan_always_holds_the_audit_files`: with `include = ["corpus/registers"]` the plan holds `state/audit/journal.jsonl` and the registers, with roots `corpus` and `state`; with `include = ["audit"]` the plan is the one of `plan_backup_excludes_derived_and_applies_include`; no file appears twice. It fails on the old code. Keep that existing test, unchanged.
2. `plan.rs`: in `plan_backup`, a path under `state/audit/` passes the include test. Do not touch `matches_include`, `is_derived` or the derived list.
3. In `crates/fieldbook/tests/backup.rs` add `backup_with_a_registers_only_target_verifies` (the probe: `include = ["corpus/registers"]`; backup exit 0, `backup verify` exit 0, the snapshot holds a `data/state/audit/` segment). Update `backup_refuses_registers_when_corpus_toml_does_not_load`, `backup_refuses_a_claims_only_target_when_sources_row_is_np` and any other test whose expected file count or file list counts the audit files, to the new list. Do not delete an assertion or lower an expected refusal.
4. `io/backup_classify.rs`: in the originals branch, match `file_name` against `<version-id>.<ext>` as the second lead decision says; remove the bare source-id comparison. Tests: `original_by_version_id_takes_the_sources_labels` (`S-0001-v1.pdf`), `original_by_bare_source_id_is_unlabelled` (`S-0001.pdf`), `original_with_a_longer_name_is_unlabelled` (`S-0001-v1.draft.txt`) and `original_without_an_extension_is_unlabelled`. The last three assert that the set contains `unlabelled` and also the source's labels the old rule matched (use a source whose handling is a not-in-table label, and assert a floor B backup to a `company_storage = true` target still refuses the file). The first one must pass on the old code too.
5. Run the acceptance commands; fix any other test that expected the old match, naming it in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test backup
cargo nextest run --locked -p fieldbook --test backup
cargo nextest run --locked -p fieldbook --test backup_no_panic
cargo xtask cards --check
just check
```

Expected: every test passes, the six new ones included; `just check` exits 0.

## Negative control

Remove the audit clause from `plan_backup`: `plan_always_holds_the_audit_files` and `backup_with_a_registers_only_target_verifies` must fail, the second with `FB0802`. Restore it. Put back the bare source-id comparison: `original_by_bare_source_id_is_unlabelled` must fail. Restore it.

## Rollback

Revert the merge commit. A snapshot written meanwhile still verifies, since it holds more files, not fewer.

## BLOCKED

- A test needs a snapshot without a journal to exist (a deliberate `FB0802` case that `plan_backup` now prevents): build that snapshot by hand in the test, never by a target setting. If the existing `backup verify` tests cannot be kept that way, set the item to Blocked with a `BLOCKED:` comment that names them, and stop.
- A fix needs a change to a label, policy or egress module of `fieldbook-core`: set the item to Blocked with a `BLOCKED:` comment, and stop. That is an owner-reviewed card.

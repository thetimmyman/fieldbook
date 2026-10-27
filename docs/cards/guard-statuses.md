# guard-statuses

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask guard-statuses BASE HEAD` judges content, not paths. It fails a commit by the agent author that raises a claim's `verification`, `applicability` or `adoption`, changes `verified_on` or `verified_by_role`, sets a page's `status` to `stable`, touches a `seed-*` file or `corpus/eval/regression.toml`, or changes the statement, value, unit, source, version, locator kind, locator or quote of a claim that is not `unchecked` (GS-02). It names the commit, the item and the field. Every other agent change under `corpus/` passes, so Codex may now do upkeep there. Moving a source version's `lifecycle` back to `active` is an `AGENTS.md` hard stop, not a guard rule.

## Why

- The rule, the agent author and when it replaces the path rule are in [AD-21](../decisions/0001-foundation.md#ad-21-codex-execution-plan). It guards the status gate, verification and promotion ([Gates](../decisions/0001-foundation.md#gates)); the rules are fixed by GS-02 and tested with negative controls, and the card changes no status itself, so it is `auto`.
- A raise and the entry values are defined in [Promotion rules](../reference/corpus-format.md#promotion-rules), which also says an edit of a verified claim's evidence fields voids the check and who may accept a page as `stable`. `ClaimRow::entry`, `ClaimRow::is_entry_state` and `is_raise` come from `core-schema-claims`; `page::split` and `parse_meta` from `core-schema-pages`; `table::read` from `core-csv`.
- The guard keys on the author name "Codex (agent)", so it stops mistakes and an unattended agent, not a determined actor. The rule for people who edit a verified claim (reset the check in the same commit) stays in the Corpus format and in `corpus/AGENTS.md`, and is not enforced here.
- Controls: GS-02 replaces GS-01 in [controls.md](../controls.md).
- Design: the [MAP.md](../design/MAP.md) row Status display (who may raise a status), enforced here; the UI only displays a status and never writes one. [HANDOFF.md](../design/HANDOFF.md) §7 Status display rules. Token groups: none.

## Scope

- Files: `xtask/src/{guard.rs,guard_fields.rs}`, the `mod` line in `xtask/src/main.rs`, `xtask/tests/guard.rs` (the scratch-repository tests move here from `guard.rs`), `xtask/Cargo.toml` and `Cargo.lock` (xtask's `dependencies` list only: the path edge to `fieldbook-core`, if it is not there yet).
- Registration lines: `AGENTS.md` (the Authority line of the rule); `docs/controls.md` (the GS-01 and GS-02 rows only: GS-01 is marked retired and loses its dagger, GS-02 gains one).
- Docs to update: `AGENTS.md` (Authority: replace the line that says the path rule holds until this card lands with the field-level rule; Codex may do upkeep in `corpus/`), `docs/controls.md` (GS-01: `Retired by GS-02 (guard-statuses)` in place of `Retired when GS-02 is brought`, and `None: retired by GS-02, whose tests replace it` in the Negative control cell, with no dagger; GS-02 brought by this card, with the evidence-edit rule and `regression.toml` in its text, and its Negative control cell ending `A non-owner status raise fails †`).

## Out of scope

- Any page field except `status = stable`, and any `lifecycle` change: moving a version back to `active` is a hard stop in `AGENTS.md`, and FB0213 fails a verified claim on a quarantined or withdrawn version. A wider rule is the owner's to write.
- `fieldbook verify` (`verify-status-fmt`) and FB0209, which `check` already raises.
- Any change to `ci.yml`: `ci-ok` already runs `guard-statuses`.

## Preconditions

- Cards merged: `core-schema-claims`, `core-schema-pages`, `core-csv`, `check-run`, `check-promotion`.

## Steps

1. Extend `agent_commits` to include agent-authored merges within the same range, and also list every commit of the range (the `n` of the ok line). For each commit, list changed paths with `git diff-tree -r --root --no-commit-id --name-status --no-renames -z <sha>`, parsed as status/path pairs (`A`, `M`, `D`, `T`); a rename is a deletion and an addition.
2. For an agent commit, a changed path whose file name starts with `seed-`, or that is `corpus/eval/regression.toml` (a change, a rename or a deletion), fails the commit.
3. In `guard_fields.rs`, for `corpus/registers/claims.csv`, read the parent's and the commit's blobs with `git show <sha>^:<path>` and `<sha>:<path>`, parse both with `table::read`, and pair rows by `claim_id`. For an agent commit, a pair fails when `is_raise(Some(before), after)` is true, and a new row fails when `is_raise(None, after)` is true. A deleted row passes.
4. Evidence edit, for an agent commit: in a pair whose `verification` after is not `unchecked`, a change to `statement`, `value`, `unit`, `source_id`, `version`, `locator_kind`, `locator` or `quote` fails. Setting a field back to its entry value is not a raise, so a reset to `unchecked` with `verified_on` and `verified_by_role` empty passes for the agent too. A commit by another author is not judged here.
5. For an agent commit and each changed file under `corpus/pages/`, read the front matter with `page::split` and `parse_meta`; a `status` of `stable` fails unless the parent's blob of that path was already `stable`.
6. A blob that does not parse fails the commit with "cannot judge <path>", so a broken file never passes by accident. Absence is determined by status, never by a failed `git show`: A has an empty before-side, D passes the content rules, M/T read both existing blobs. The protected-path rule still judges deletions. Root additions follow A. Existing bytes that fail to parse remain a refusal.
7. Every other change passes. Keep `guard-statuses: ok ({total} commits checked, {m} by the agent author)` and the existing final `help:` line. Every finding starts `guard-statuses: <sha12> `, then the claim/page id and field or protected path, with the reason; sha12 is the first 12 hexadecimal characters.
8. Correct the module comment in `guard.rs`: the field-level rule replaces the path rule when this card merges.
9. Tests in `xtask/tests/guard.rs`, each in a scratch git repository with synthetic rows, through the built binary:
   - an agent raise of each of the three fields fails, and so does a raise of `verified_by_role`: `an_agent_setting_verified_by_role_fails`, and the same for `verification`, `applicability` and `adoption` (`an_agent_setting_<field>_fails`); a change of `verified_on` fails; a new claim row with `verification = primary_source_supported` by the agent author fails (`an_agent_adding_a_raised_row_fails`); a new claim with entry values and `drafted_by = "codex"` passes; a field set back to its entry value passes; the same raise by another author passes;
   - `statement_edit_on_verified_claim_fails`, and `the_same_edit_with_a_reset_passes`; one case for each of the eight evidence fields fails; the same edit by the owner or the lead passes; a statement edit on an `unchecked` claim passes;
   - a seed edit fails; `an_agent_edit_of_regression_fails`; a typo fix in a page passes;
   - `an_agent_setting_a_page_stable_fails`, and an agent body edit of a page that was already `stable` passes;
   - a commit by the agent author that sets a version's `lifecycle` back to `active` passes the guard (it is a hard stop of `AGENTS.md`, not a guard rule).
10. Retire the tests of the path rule. `is_protected` keeps only a file name that starts `seed-` and the path `corpus/eval/regression.toml`. `protected_paths_are_corpus_and_seed_files` becomes `protected_paths_are_seed_files_and_the_regression_file` (a page, `corpus/registers/claims.csv` and `corpus/a.md` are not protected). `an_agent_commit_touching_the_corpus_or_a_seed_file_fails` becomes `an_agent_commit_touching_a_seed_file_fails` in `xtask/tests/guard.rs` and asserts only the seed-file and `corpus/eval/regression.toml` cases. `a_first_push_checks_every_commit` uses a seed file in place of `corpus/a.md`. No test keeps the line `// control: GS-01`.
11. Mark the tests that carry a control row of [the controls table](../controls.md) with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: `// control: GS-02` on `an_agent_setting_verified_by_role_fails`, on `statement_edit_on_verified_claim_fails` and on `an_agent_setting_a_page_stable_fails`. The dagger of the GS-02 row and these markers land in the same pull request.

12. In `xtask/tests/guard.rs`, add `a_root_commit_touching_a_seed_file_fails`, `an_agent_adding_claims_csv_with_a_raised_row_fails`, `an_agent_adding_claims_csv_in_entry_state_passes`, `an_agent_deleting_claims_csv_passes`, `an_agent_deleting_a_page_passes` and `a_deleted_protected_file_still_fails`. Each uses a scratch repository and checks the stated result and the diagnostic prefix.

13. Git author text is attribution, not proof of a human identity. The diff guard prevents accidental status changes; consequential decisions retain the owner review flow.

14. For a merge, compare the resulting protected state against every parent explicitly; do not rely on the default merge diff or `<sha>^` alone. Inherited state passes only when the complete protected claim tuple (status, verification metadata and evidence) exists unchanged in one parent. A new combination assembled from fields in different parents is not inherited. Apply the same whole-state rule to stable pages and protected files; resolution-introduced raises, evidence edits and protected-path changes fail. In `xtask/tests/guard.rs`, scratch repository tests are `merge_preserves_owner_approved_status`, `agent_merge_resolution_cannot_raise_status`, `agent_merge_resolution_cannot_set_page_stable`, `agent_merge_cannot_edit_protected_file` and `merge_protected_evidence_recombination_fails`. Excluding merges or comparing individual fields against different parents must fail the refusal tests.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test guard
cargo xtask guard-statuses "$(git merge-base origin/main HEAD)" HEAD
just check
```

Expected: every test passes; the guard on this branch prints `guard-statuses: ok` and exits 0; `just check` exits 0.

## Negative control

In a scratch repository, commit as `Codex (agent)` a claims row that sets `verified_by_role = owner`. The guard must exit 1 and print the commit, the claim id and `verified_by_role`. Then make `guard_fields` skip `verified_by_role`: the test `an_agent_setting_verified_by_role_fails` must fail. Restore it. Then make it skip `statement` in the evidence rule: `statement_edit_on_verified_claim_fails` must fail. Restore it. Then let a new row skip the `is_raise(None, after)` test: `an_agent_adding_a_raised_row_fails` must fail. Restore it.

Drop `--root` or skip D paths: `a_root_commit_touching_a_seed_file_fails` or `a_deleted_protected_file_still_fails` must fail. Restore each.

Exclude merges or compare protected fields against different parents: `agent_merge_resolution_cannot_raise_status` or `merge_protected_evidence_recombination_fails` must fail. Restore each.

## Rollback

Revert the merge commit. The path rule returns, and Codex may no longer touch `corpus/`. Restore the AGENTS.md line, GS-01 and the dagger of GS-01.

## BLOCKED

The owner wants a page field other than `status`, or another `lifecycle` move, guarded as well. AD-21 lists only the claim fields: stop and ask for the rule in writing before adding it.

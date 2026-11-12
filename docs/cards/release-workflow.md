# release-workflow

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

`just release X.Y.Z` prepares the release commit on a branch, and `.github/workflows/release.yml` runs on `v*` and `edition-*` tags, refuses a tag that fails `cargo xtask release-check`, and uploads the files the release runbook names, with a read-only token.

## Why

- What `just release` refuses, runs and changes, and what each tag's run uploads, are the [release runbook](../runbooks/release.md) (tool release step 1 and step 6; edition step 6). The version and tag rules are [AD-20](../decisions/0001-foundation.md#ad-20-signing-release-pipeline-and-versioning); the release job is [AD-08](../decisions/0001-foundation.md#ad-08-ci-design). Only the owner tags.
- Controls RL-01 (a release is what CI built) and CI-01 (read-only token, actions pinned by SHA, GitHub-hosted runners) are in the [control register](../controls.md). The `dist` profile is already in the root `Cargo.toml`.
- This is a release step and a CI permission, so the card is owner reviewed ([cards README](README.md#the-three-header-fields), release gate).
- Design: none. Nothing a reader sees changes.

## Scope

- Files: `.github/workflows/release.yml` (new), `justfile`, `xtask/src/{main.rs,release.rs}`, `xtask/tests/release.rs`, `docs/runbooks/release.md`, `CHANGELOG.md`; `xtask/src/archive_release.rs`, `xtask/tests/archive_release.rs`.; `release-baseline.toml` (public compatibility identity and hashes only). `crates/fieldbook/src/io/journal.rs` (the `open_stamped` test seam), `crates/fieldbook/tests/support/state.rs` (checked-in journal fixtures pin their own tool version, so a release does not rewrite them).
- Registration lines: the Commands line of `AGENTS.md` (`just release`); `compat/published.toml` (new-release rows only); `docs/cards/README.md` (compatibility recipe); `xtask/src/compat.rs` (release baseline lifecycle tests); `xtask/src/edition_release.rs` (Site/tool provider registration; reuse common helpers); `xtask/src/{release.rs,archive_release.rs,edition_release.rs}` (only the support-evidence entries: the two sidecar files and the edition catalogue in the artifact, copy, reopen and verification lists); `docs/runbooks/release.md` (the same evidence list, this card’s entries only); `.github/workflows/release.yml` (the edition artifact list gains `corpus-support.toml` and `corpus-support.sha256`); `edition-release.toml` (optional owner-selected mode).
- Docs to update: `docs/runbooks/release.md` (the two lines that say `just release`, `just edition` and `release.yml` arrive with an earlier card now name `edition-zips` and this card), `AGENTS.md` (Commands: `just release`; Codex never pushes a tag), the justfile header comment, `CHANGELOG.md` (`- Added: just release and the release workflow.` under `## Unreleased`).

## Out of scope

- Signing and `sign.yml` (R2); the `windows` job and drill 2 (`transfer-and-windows-drill`); GitHub release pages. The run uploads workflow artifacts only.
- Tagging. Codex never creates or pushes a tag.

## Preconditions

- Cards merged: `edition-zips`, `core-policy` (`cargo xtask binscan` and its lists), `calc-cases`.

## Steps

1. `release.rs`, subcommand `release <X.Y.Z>`: the refusals and edits of tool release step 1, in that order, through `std::process` for `git` and `just check`. After the workspace version and changelog edits, run the existing dependency recipe (`just lock`, then committed-lock Cargo/cargo-deny checks) and `just check` on the resulting tree. Allowed changes are `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`; review dependency changes under that recipe. It commits nothing.
2. `release.rs`, subcommand `release-check <tag> --base FILE`: the tag must point at a commit on `origin/main`; a `vX.Y.Z` tag must equal the workspace version; an `edition-NNN` tag must equal the newest heading of `corpus/changes.md`. It prints one line naming the failed rule and exits 1. When `origin/main` is missing from the clone it exits 1 and the line names `origin/main`, and it never passes.
3. The justfile recipe `release VERSION` runs `cargo xtask release {{VERSION}}`.
4. `release.yml`: `on:` has `push: tags: ['v*', 'edition-*']`, `workflow_dispatch:` and `pull_request:` with `paths: ['.github/workflows/release.yml']`, so that the workflow runs on its own pull request and can be started by hand; `permissions: contents: read` at the top and no job raises it; `ubuntu-24.04`; every action pinned by the SHAs already used in `ci.yml`. Job `tag-check` has `if: github.ref_type == 'tag'`; it copies the checkout of `ci.yml` (`fetch-depth: 0` and `persist-credentials: false`, so that the clone holds `origin/main`), installs the toolchains the way `ci.yml` does, and runs `cargo xtask release-check "$GITHUB_REF_NAME" --base release-baseline.toml`. It runs no `git fetch`, because credentials are not persisted. Each job has `timeout-minutes: 30`, a pinned checkout with `fetch-depth: 0` and `persist-credentials: false`, and the pinned Rust toolchain before repository commands. Job `edition` has `if: github.ref_type == 'tag' && startsWith(github.ref_name, 'edition-')`, needs `tag-check`, installs `just` with the pinned `taiki-e/install-action` step of `ci.yml`, and runs `cargo xtask edition`, which owns the stamped build, harness and packaging in that order (Edge is preinstalled on the `ubuntu-24.04` image), and uploads the artifact the runbook names with the two zips, `SHA256SUMS`, `corpus-support.toml` and `corpus-support.sha256`. Job `binary` needs `tag-check`, and runs on every event except an edition tag (`if: ${{ !cancelled() && needs.tag-check.result != 'failure' && !startsWith(github.ref_name, 'edition-') }}`, so that it runs when `tag-check` is skipped off a tag): it binds `FIELDBOOK_GIT_SHA` in the Cargo environment to the full checked-out `git rev-parse HEAD` (including trial runs), builds `fieldbook` with `--profile dist`, runs that executable with `--version` and requires its commit component to equal that SHA before hashing or uploading, runs `cargo xtask binscan target/dist/fieldbook --build silent` on the result (the job fails on a finding) and uploads the Linux binary and its `SHA256SUMS` as the artifact `fieldbook-v<version>-linux`. A step with `id: version` sets `<version>` from the workspace `version` in `Cargo.toml` (`cargo metadata --no-deps --format-version 1` and `jq`, both on the runner image), and the upload step's `name:` is built from `steps.version.outputs.version`, never from `github.ref_name`. On a tag run `release-check` has already proved that the workspace version equals the tag; off a tag the same name is used for the trial build, and nothing is published. The upload sets no `retention-days`. A change to the workflow therefore shows its build on its own pull request, and only the tag rules wait for a tag. There is no `dry_run` input.
5. Tests in `xtask/tests/release.rs`, each on a temporary git repository and through the built binary (`env!("CARGO_BIN_EXE_xtask")`; `xtask` is a binary crate, so a file in `xtask/tests/` cannot call `release.rs`): `refuses_on_main`, `refuses_dirty_tree`, `refuses_version_not_higher`, `refuses_empty_unreleased`, `check_refuses_tag_off_main`, `check_refuses_version_mismatch`, `check_refuses_edition_heading_mismatch`, `check_refuses_when_origin_main_is_missing` (a temporary repository with no remote ref: exit 1, and the message names `origin/main`), `release_yml_is_read_only_and_pinned` (reads the workflow text: one `permissions` block with `contents: read`, no `write`, every `uses:` pinned by a 40-character SHA, and `fetch-depth: 0` on the checkout of `tag-check`), `release_yml_edition_runs_the_harness` (reads the workflow text: in the `edition` job the job calls the single `cargo xtask edition` operation without a separate rebuild or prebuilt summary), `release_yml_binary_scans_the_binary` (reads the workflow text: in the `binary` job the step `cargo xtask binscan target/dist/fieldbook --build silent` comes after the build with `--profile dist`), `release_yml_triggers` (reads the workflow text: `workflow_dispatch:` is present, `pull_request:` lists only `.github/workflows/release.yml` under `paths`, and the `tag-check` job carries `if: github.ref_type == 'tag'`) and `release_yml_artifact_name_uses_the_workspace_version` (reads the workflow text: in the `binary` job the step with `id: version` runs `cargo metadata`, and the `name:` of the upload step is `fieldbook-v${{ steps.version.outputs.version }}-linux` and does not contain `github.ref_name`).
6. Mark the tests that carry a control row of [the controls table](../controls.md) with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: `// control: RL-01` on `check_refuses_version_mismatch`.

7. Register the Site/tool providers in the shared archive-release and verify-archive dispatcher as specified by the permanent release runbook. They use a caller-supplied approved archive path, no credentials or service SDK, and never overwrite a different version/hash folder. The archive also retains current compat/published.toml and its public contract/vector blobs through edition_release’s shared evidence collector. It also retains the edition's `evidence/corpus-support.toml` and the separately delivered `corpus-support.toml` and `corpus-support.sha256` as public release evidence, verifies their exact bytes and digest against the paired edition, and refuses completion on a missing or changed sidecar. Verification reads all copied bytes and the archive manifest; tool rollback uses the archived executable and payload without a rebuild; edition rollback restores the exact previous pair by that archive’s Raw/Site provider, with no package requirement. Tests `archive_round_trip`, `changed_archive_byte_fails`, `missing_previous_set_blocks_delivery`, `explicit_first_release_verifies_current_archive`, `missing_previous_is_not_an_empty_baseline`, `archive_required_before_delivery`, `existing_different_archive_is_refused` and `rollback_uses_archived_bytes` run through the built xtask binary on temporary synthetic packages and repositories, as step 5 does. They require no later adapter Runner. The release completion checklist requires the verified durable copy; upload-artifact alone cannot satisfy it.

8. release-check compares against the compatibility binding before publishing the immutable manifest. Archive verification selects the tag’s tool or edition binding independently. An empty tool binding rehearses the current archived package and synthetic restore fixture, even after a raw edition; a declared missing predecessor refuses. Add `check_accepts_unchanged_old_vectors` and `check_refuses_changed_old_vectors` through the built binary and synthetic archives; changed vectors fail before publication.

9. Consume the three-table descriptor and advancement rules in [compatibility.md](../reference/compatibility.md#baseline-descriptor), using the calc-cases parser. Tag CI verifies exact commit/blob hashes with full history and no extra network permissions. Archive verification selects the matching stream and manifest hash. Test the real invocation with empty bindings, a valid predecessor, unavailable history and wrong identity.

10. Reuse the baseline parser and exact-commit compatibility retrieval already provided by `calc-cases`. This card wires the tag job and archive verification to that same descriptor; it creates no second baseline format or discovery rule.

11. Release completion requires an owner-reviewed follow-up PR advancing compatibility and the accepted stream’s binding in release-baseline.toml to the newly accepted verified archive while preserving the other stream before further implementation cards merge. Reuse the calc-cases descriptor parser; preserve the tagged previous baseline and the release preparation/lock-refresh contract. Synthetic lifecycle tests for the first release and subsequent releases advance it, pass unchanged bindings and fail an implementation-plus-golden change to a newly published binding in an ordinary PR check. Omitting advancement must fail the lifecycle test.

12. Use edition_release’s committed mode parser and edition tag validator. tag-check exposes the validated kind as a job output; the edition job additionally requires kind == site. A raw tag skips the Site job and is handled only by raw-edition.yml, if that card has landed. Missing mode descriptor means site; malformed or mismatched descriptors refuse instead of selecting a default. Add release_yml_raw_mode_skips_site with raw/site/absent/invalid cases and a negative plant that invokes Site on a raw tag. Shared archive helpers remain usable without either concrete provider.

13. Use the shared baseline parser’s separate compatibility, tool and edition bindings. Lifecycle tests cover raw edition → first tool and tool → edition update → next tool: retain new compatibility vectors, select the prior executable only from tool, and select raw/rendered pairs from edition. Explicit absence succeeds only for that stream; missing declared predecessors fail. The archive manifest records its stream and the advancement preserves the other stream.

14. Extend archive tests with `raw_to_site_restores_edition_predecessor_without_tool` and `missing_declared_edition_predecessor_refuses`, using edition-manifest’s synthetic-provider seam without a raw-edition prerequisite. Exercise Site archival through the built xtask binary with no --package, current-archive verification, the exact edition predecessor and explicit-empty edition binding. Scope executable smoke tests to the tool stream.

15. Extend the shared validator/workflow lifecycle test with raw-001 → site-002: both descriptor removal and a matching explicit Site descriptor select Site; retaining the 001 descriptor refuses. Malformed or mismatched descriptors never default to Site.

16. Test release preparation offline through a small private `run_with` command seam in release.rs: production delegates to xtask::proc; module tests record command order and emulate lock outputs without a registry. Move the successful edit-path assertion from the binary-only harness to this seam; keep the binary refusal tests. `release_refreshes_before_verifying` requires precheck, edit, lock, postcheck in order and propagates any failure. `release_rejects_unrelated_changes`: an unrelated changed file fails the allowed-path assertion. The real resolution/build rehearsal and exact-source MAX/JEV release-age/profile evidence remain required under the dependency recipe; the command cannot fabricate that approval.

17. Add `linux_release_binds_and_checks_commit`: workflow coverage requires the exact checked-out SHA in FIELDBOOK_GIT_SHA at compile time and --version verification before checksum/upload. Missing or renamed binding must fail; an unreleased-build fallback is not a successful release stamp.

18. For Site edition tags, tag validation emits evidence with the shared edition_release collector into a separate directory and uploads it as `edition-NNN-evidence`. Include tagged release-baseline.toml, edition-release.toml when present, compat/published.toml and every `Published::bound_files()` entry, including implementation blobs. Descriptor absence remains valid Site mode. The edition catalogue and its separate digest are not compatibility blobs: they travel in the edition artifact and are retained by step 7's archive evidence, never duplicated into this compatibility set. Archive verification binds evidence to the same tag/commit as the pair. `site_evidence_reaches_archive` runs the complete producer-to-archive path on a synthetic repository and rejects missing blobs, wrong hashes and mismatched tag/commit identity. Keep the delivered zip pair unchanged.

19. Bind `FIELDBOOK_SECURITY_CONTACT` from the owner-provisioned same-named repository secret in every packaging step this card adds, including scheduled, release and Windows calls as applicable. Forward the environment through child edition/intake commands. Tests use synthetic contact text, verify the workflow binding and require IN-01 for missing/blank input; do not print or commit the real value. No token permission changes are needed.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test release
D="$(mktemp -d)" && git clone -q . "$D/c" && git -C "$D/c" switch -q -c card/release-dry
(cd "$D/c" && just release 0.1.0) && git -C "$D/c" status --short
git -C "$D/c" checkout -q -- . && (cd "$D/c" && just release 0.0.0); echo "exit $?"
cargo nextest run --locked -p xtask --test archive_release
cargo nextest run --locked -p xtask release::
just check
```

Expected: the tests pass; in the throwaway clone `just release 0.1.0` exits 0 and `git status --short` lists the version/changelog changes with the refreshed committed lock; no unrelated file changes; `just release 0.0.0` prints the "not higher" refusal and `exit 1`; `just check` exits 0. Remove `$D` afterwards. The workflow runs on its own pull request through the `paths` trigger: `gh run list --workflow release.yml --limit 1` lists that run, and its `binary` job passes. The tag-only check first runs on the tag, when the owner pushes `v0.1.0` (release runbook, step 6).

## Negative control

Make `release-check` return success before it compares a `v*` tag with the workspace version. `cargo nextest run --locked -p xtask --test release check_refuses_version_mismatch` must fail and show the tag that was accepted. Restore the file. Then remove the `if: github.ref_type == 'tag'` line from `tag-check`: `release_yml_triggers` must fail. Restore the line. Then replace the edition operation with a packaging-only call: `release_yml_edition_runs_the_harness` must fail. Restore the line. Then remove the `binscan` step from the `binary` job: `release_yml_binary_scans_the_binary` must fail. Restore it. Then change the upload `name:` to `fieldbook-${{ github.ref_name }}-linux`: `release_yml_artifact_name_uses_the_workspace_version` must fail. Restore it. Then drop `fetch-depth: 0` from the `tag-check` checkout: `release_yml_is_read_only_and_pinned` must fail. Restore it. Then make `release-check` pass when `origin/main` is missing: `check_refuses_when_origin_main_is_missing` must fail. Restore it.

Skip verifying a copied payload: `changed_archive_byte_fails` must fail. Restore it.

Drop a support sidecar from the archive copy or accept a digest that does not name the retained catalogue bytes: `changed_archive_byte_fails` must fail. Restore each.

Skip the archive delivery gate, reject the explicit first-release baseline, or treat a missing previous archive as that baseline: the matching archive test must fail. Restore each.

Allow a modified old vector during compatibility verification: `check_refuses_changed_old_vectors` must fail. Restore it.

Insert a separate site build or harness invocation before edition: the existing workflow sequence test must fail. Restore it.

Replace the retained tool binding with the intervening edition binding: the mixed-stream lifecycle tests must fail. Restore it.

Select the tool binding for edition rollback or treat a missing declared edition predecessor as empty: the corresponding new archive test must fail. Preserve the proper binding and restore each plant. Retain the old raw descriptor for site-002: the lifecycle test must reject it; bypass that rejection and the assertion must fail.

Skip the lock refresh or postcheck, remove the Linux SHA binding, or drop a bound implementation blob from Site evidence: the corresponding release-refresh, workflow or producer-to-archive test must fail. Restore each.

## Rollback

Revert the merge commit. Tags then start no workflow; any tag already pushed stays and its artifacts expire on GitHub's schedule.

## BLOCKED

A rule in step 2 cannot be checked from a tag checkout (for example `origin/main` is missing in the runner's clone even with full history): set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Never widen the token's permissions to make a step pass.

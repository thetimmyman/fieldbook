# public-hosted-runners

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** auto

## Outcome

CI and hosted edition jobs use standard GitHub-hosted runners. Native Windows tests and the binary import scan run again, including backups while mandatory state-file locks remain held.

## Why

Restore CI-01 in [controls.md](../controls.md) after the temporary self-hosted departure. Keep HO-05's exact runner policy and deployment hardening.

## Scope

- Files: `.github/scripts/prepare-edge-sandbox.sh`, `.github/workflows/ci.yml`, `.github/workflows/hosted.yml`, `.github/workflows/scheduled.yml`, `xtask/binscan/silent.txt`, `xtask/binscan/ai.txt`, `xtask/src/binscan.rs`, `xtask/tests/hosted_workflow.rs`, `xtask/tests/scheduled_controls.rs`, `crates/fieldbook/src/io/backup_classify.rs`, `crates/fieldbook/tests/backup.rs`, `crates/fieldbook/tests/restore.rs`, `crates/fieldbook/tests/leak_outputs.rs`, `crates/fieldbook/tests/support/mod.rs`, `crates/fieldbook-core/tests/restore.rs`.
- Docs to update: `docs/controls.md`, and only the CI-foundation prerequisite in `docs/cards/claims-csv-parts.md`; its owner review, CSV/ADR outcome and all other contract fields stay unchanged.

## Out of scope

No token permissions, action pins, secrets, publication of company data, tags or release changes.

## Preconditions

- Cards merged: workspace-skeleton, hosted-workflow, scheduled-controls.

## Steps

1. Require the approved standard hosted runners in workflow policy checks.
2. Restore Linux runner routing and native Windows tests without changing event filters or hardening. Verify the installed Edge sandbox helper against its package inventory and restore only root-owned mode 0755 to 4755 in the disposable runner, refusing unknown states and preserving sandboxing. Require a bounded startup probe with the harness browser flags before the expensive checks. Run the complete browser sweep with four workers on the dedicated four-core standard runner; preserve every target, assertion, negative control and the existing timeout.
3. Preserve empty state-lock snapshot payloads without opening a second read handle on Windows; nonempty lock contents still follow ordinary classification and refusal.
4. Use ordinary canonical drive/UNC paths in backup policy fixtures and report all Windows test failures without failing fast; retain every existing assertion. Use coherent home/state roots and capture the independent reference lock payloads before holding OS locks; keep exact-token and no-copy refusal controls unchanged.
5. Reconcile only the nine traced existing Windows storage, pinned XLSX math and Rust thread imports in the binary inventories; retain unknown-import rejection and all network, process-start and clipboard bans, with checked-in-inventory negative controls.
6. Verify the existing backup CLI suite, workflow policy tests, negative controls and the required CI gate.

## Acceptance

```sh
cargo test --locked -p fieldbook --test backup
cargo test --locked -p xtask --test hosted_workflow --test scheduled_controls
just check
```

Expected: tests and existing checks pass; native Windows tests and import scan pass in CI.

## Negative control

Substitute a self-hosted runner in the hosted workflow. Its runner-policy test must fail. Existing backup tests must continue to reject nonempty state lock contents.

## Rollback

Revert the workflow migration. Keep the repository private until the publication audit and runner checks permit exposure.

## BLOCKED

Stop if sensitive source or repository assets would become public, a required check fails, or a secret/permission change is needed outside the owner's authorization.

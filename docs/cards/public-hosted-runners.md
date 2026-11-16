# public-hosted-runners

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** auto

## Outcome

CI and hosted edition jobs use standard GitHub-hosted runners. Native Windows tests and the binary import scan run again, including backups while mandatory state-file locks remain held.

## Why

Restore CI-01 in [controls.md](../controls.md) after the temporary self-hosted departure. Keep HO-05's exact runner policy and deployment hardening.

## Scope

- Files: `.github/workflows/ci.yml`, `.github/workflows/hosted.yml`, `.github/workflows/scheduled.yml`, `xtask/tests/hosted_workflow.rs`, `xtask/tests/scheduled_controls.rs`, `crates/fieldbook/src/io/backup_classify.rs`, `crates/fieldbook/tests/backup.rs`.
- Docs to update: `docs/controls.md`.

## Out of scope

No token permissions, action pins, secrets, publication of company data, tags or release changes.

## Preconditions

- Cards merged: workspace-skeleton, hosted-workflow, scheduled-controls.

## Steps

1. Require the approved standard hosted runners in workflow policy checks.
2. Restore Linux runner routing and native Windows tests without changing event filters or hardening.
3. Preserve empty state-lock snapshot payloads without opening a second read handle on Windows; nonempty lock contents still follow ordinary classification and refusal.
4. Use ordinary canonical drive/UNC paths in backup policy fixtures and report all Windows test failures without failing fast; retain every existing assertion.
5. Verify the existing backup CLI suite, workflow policy tests, negative controls and the required CI gate.

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

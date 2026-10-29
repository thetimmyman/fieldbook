# day1-dependencies-checks-4

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Complete pure catalog-policy evaluation consumes located inputs and measured graphs, composes all five checks and returns ordered findings and measurement reports. The dependent `day1-dependencies-checks-4-2` alone activates the command and gate.

## Scope

- Files: `xtask/src/deps_policy.rs`, `tests/fixtures/deps-policy/`.
- Registration lines: `xtask/src/main.rs` (module only), `docs/reference/dependencies.md` (provider contract), `docs/plan/PLAN.md` and dependent cards (atomic split prerequisites).
- Docs to update: [dependencies.md](../reference/dependencies.md).

## Preconditions

- Cards merged: `day1-dependencies-checks-3`, `day1-dependencies-inputs`.

## Steps

1. Reuse complete inputs, graph facts and budgets; perform no I/O or resolution. Compare declared requirements and canonical crate identities, every unused workspace pin, exact workflow commits and tool pins. Unsupported requirements refuse before missing-row findings; Cargo-normalized direct requirements compare release identities while literal workspace pins retain build metadata.
2. Test the evaluation interface for a clean report, each check, both applicable budgets and the alias/build-metadata/refusal controls. Retain source locations and structured-input error behavior; add no parser or collector.
3. Preserve every complete-command control in the dependent card before first activation. Redirect downstream preconditions atomically. No partial command, gate, measurement row, manifest/lock or replay change belongs here.

## Acceptance

`cargo nextest run --locked -p xtask deps_policy` and `just check` pass. Production inputs and commands remain unchanged. The dependent card owns real-command tests and activation.

## Negative control

Bypass each catalog check, canonical aliases, build-metadata release normalization, requirement refusal precedence or either profile budget: its evaluation test must fail. Restore each plant.

## Rollback

Revert the merge; the dependent complete command stays ineligible.

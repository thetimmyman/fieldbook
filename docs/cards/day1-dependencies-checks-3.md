# day1-dependencies-checks-3

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`deps_budget` evaluates the complete profile budgets and reports from measured graph facts. There is no CLI or gate; `day1-dependencies-checks-4` retains all five policy algorithms, the complete command, baseline row 0, DP-02 and all command controls before activation.

## Why

The full checker exceeds the card-size limit. Pure budget evaluation is a complete provider over the [measurement contract](../reference/dependencies.md#counting-crates), with literal boundaries and actual Cargo integration tests.

## Scope

- Files: `xtask/src/deps_budget.rs`.
- Registration lines: `xtask/src/main.rs` (budget module only); `docs/cards/day1-dependencies-checks-4.md`, `docs/cards/day1-dependencies-2.md`, `docs/cards/day1-dependencies-3.md`, `docs/cards/docs-checks.md`, `docs/cards/drafts/mutation-report.md`, `docs/plan/PLAN.md`, `docs/controls.md` (complete-checker scope/order and consumer prerequisites); `docs/reference/dependencies.md` (provider and baseline ownership).
- Docs to update: `docs/reference/dependencies.md`.

## Out of scope

Other policy checks, command registration, baseline row, clean gate, replay/capture, manifests/lock/dependencies, Corpus and seed files.

## Preconditions

- Cards merged: `day1-dependencies-checks-2`.

## Steps

1. Implement the pure budget comparison and report from measured Graph/Profile facts, using the exact [policy](../reference/dependencies.md#policy). No process, file read, second collector or configuration parser.
2. Keep every direct and transitive boundary, exact named finding/report, distinct name/version pair, both-profile finding order and uncapped all-target case. Real Cargo measurements prove workbook eligibility and separate silent/AI consumption; use the merged collector, immutable fixtures and atomically claimed scratch.
3. Retain the complete final checker and all its literal process/budget/error controls before its first enabled clean command. This provider enables no partial DEPS gate.

## Acceptance

```sh
cargo nextest run --locked -p xtask deps_budget::
just check
```

Expected: all provider tests pass; manifests and the production lock are unchanged.

## Negative control

Change each ceiling/cap, workbook selection, comparison/report branch or AI iteration: the owning literal test must fail. Restore every plant.

## Rollback

Revert the merge commit; the final command cannot become eligible without this provider.

## BLOCKED

A required budget cannot be evaluated from the existing complete measurement facts. Report the exact gap rather than inventing a policy or weakening a boundary.

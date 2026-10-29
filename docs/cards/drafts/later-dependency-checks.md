# later-dependency-checks

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask deps --check` reports DEPS-1 to DEPS-5 through its real command interface on the current workspace, without changing a manifest or lock. `day1-dependencies-checks` supplies the strict Cargo-metadata decoder; `day1-dependencies-checks-2` supplies complete target/profile measurements and raw declaration facts; `day1-dependencies-checks-3` supplies complete profile budgets and reports; this card alone owns the complete command and gate. Initial adoption uses the reviewed standard-Cargo recipe; this deferred capability supplies no present adoption prerequisite. Offline replay/capture remain separate in `later-lock-replay`.

## Why

This is `later-dependency-checks` in [AD-24](../../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later), deferred until its trigger. The merged providers remain available. The complete checker is split at real provider seams under [Size and splitting](../README.md#size-and-splitting). Cargo owns dependency semantics; the final command consumes the complete bounded providers and preserves every [dependency-policy](../../reference/dependencies.md) control before its first clean gate.

## Scope

- Files: `xtask/src/deps.rs`, `xtask/src/deps_policy.rs`, `xtask/tests/deps.rs`, `xtask/tests/support/deps.rs`, `justfile` (add the checker after the existing dependency-policy checks).
- Registration lines: `xtask/src/main.rs` (module, command, usage and cwd selection for this command, plus removal of both input-provider unused-code expectations when full fact consumption makes them unnecessary); `xtask/src/deps_inputs.rs` (only the complete aggregate interface integration; module expectations live in main); `docs/reference/dependencies.md` (checker contract and next Measurements row); `docs/controls.md` (DP-02 checker/test ownership); `AGENTS.md` (one Commands line).
- Docs to update: [dependencies.md](../../reference/dependencies.md), DP-02 in `docs/controls.md`, one Commands line in `AGENTS.md`.

## Out of scope

Manifest, lock, crate, feature or pin changes; registry access; capture/replay; CI permissions; shipped behavior; Corpus/seed files; and activation of replay remain outside this card. Initial adoption does not wait for this card; it follows the current reviewed-evidence recipe.

## Preconditions

- Cards merged: `day1-dependencies-checks-3` (complete budgets over target/profile measurement facts) and `day1-dependencies-inputs` (the complete aggregate, including `day1-dependencies` reference facts).

## Steps

1. Read the complete strict `deps_inputs` aggregate once from the invocation workspace, then consume `deps_graph::measure` and `deps_budget::deps3`/`summary`. Reuse the actual nested reference/configuration facts; add no parallel reader, collector, budget constants or report algorithm. Preserve relative input locations, exit 2 for unsupported/unreadable input or failed metadata, absent workspace-dependencies on the skeleton and all unused workspace pins. Preserve every applicability/error and raw-declaration/active-closure distinction, compatible replay shapes and uncapped all-target report. Cargo owns cfg, aliases and dependency semantics. Decoder, measurement and budget controls stay with their producing cards; this card proves their complete real-command consumption.
2. Expose no partial `deps --check` command: its only clean exit must mean all five checks ran for every applicable target/profile. Print one `DEPS-n <name>: <what is wrong>` line per finding. DEPS-1 requires a row with the same name and pin for every direct dependency, including a bare member pin before workspace inheritance; DEPS-2 compares every workspace pin; DEPS-3 enforces profile/target direct budgets and transitive caps; DEPS-4 compares each exact 40-hex workflow action SHA with its row; DEPS-5 compares each documented tool pin, including Rust and the lock resolver. Exit 0 clean, 1 findings, 2 for unsupported/unreadable input or failed metadata. Cargo-deny retains bans, licenses and source checks.
3. Tests in `xtask/tests/deps.rs` invoke `env!("CARGO_BIN_EXE_xtask")` in temporary synthetic workspaces. Write `deps_clean_workspace_passes`, `deps_reports_deps_1` through `deps_reports_deps_5`, `deps_uses_the_invocation_workspace`, `deps_rejects_duplicate_or_unsupported_input` and `deps_rejects_failed_metadata`. DEPS-1 covers both a missing row and a mismatched member pin. `deps_reports_deps_3` tests literal direct-budget boundaries 12/13 before workbook reader edges, 14/15 after them and 17/18 for the AI profile, plus transitive-cap boundaries 80/81 and 100/101. Catalog rows, lock entries, xtask edges and dev/build-only `zip` cannot select the higher silent ceiling; a single workbook dependency cannot select it either. Assert literal documented exits, code and relevant names. A fault in the invocation workspace must not be masked by a clean build checkout. Include all applicable targets/profiles, mixed normal/dev/build kinds, external namesakes, absent app and malformed applicability facts through the actual command, with independent process metadata and real local-Cargo cases. Every boundary and error control precedes the first enabled clean command; no later test-only part may defer one. Scratch is atomically claimed without pre-deletion, and fixtures bind through CARGO_MANIFEST_DIR.
4. Add the checker after the two cargo-deny lines in step 4 of `just check`. Append the next measured row without rewriting adoption/history rows. Restore automated DP-02 ownership and its dagger only when every command control passes. Replay activation remains with `later-lock-replay`.

5. Retain the demonstrated comparator repairs and their literal witnesses: canonical package identity for workspace aliases; valid build-metadata normalization for DEPS-1 while DEPS-2 compares the full spelling; malformed unused workspace requirements refuse as exit 2; and unsupported declaration requirements refuse as exit 2 with or without a catalog row. Close the remaining unmerged-candidate failures: validate a full catalog crate pin before stripping build metadata (`0.1.0+` and `0.1.0+build..7` refuse); a prerelease/build suffix requires a three-component core (`1-rc.1`, `1.2-rc.1`, `1+public`, `1.2+public` refuse). Preserve bare partial `1`/`1.2`, full valid metadata and large numeric prerelease/build identifiers. These private controls are not a passed public gate.
6. Measure the complete first public diff, including executable test setup and coordination. Split the comparator/policy at a real interface if needed; every required command/process control still precedes the first clean command.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test deps
cargo xtask deps --check
git diff --exit-code -- Cargo.toml Cargo.lock crates/fieldbook/Cargo.toml crates/fieldbook-core/Cargo.toml crates/fieldbook-calc/Cargo.toml crates/fieldbook-render/Cargo.toml crates/fieldbook-win/Cargo.toml xtask/Cargo.toml
just check
```

Expected: real and synthetic command cases prove the documented behavior; manifests/lock are unchanged; the new row is an actual measurement, with earlier rows retained. No new dependency or registry access is needed.

## Negative control

Skip each DEPS check in turn: its named fault case must fail. Use the build checkout instead of invocation cwd: `deps_uses_the_invocation_workspace` must fail. Accept a duplicate tool key: `deps_rejects_duplicate_or_unsupported_input` must fail. Bypass a collector failure or skip an applicable graph in the command: its end-to-end graph case must fail; the producing decoder keeps its own shape/identity/kind/target controls. Restore every plant.

## Rollback

Revert the merge commit. The existing manifests and lock remain unchanged. Reviewed standard-Cargo adoption remains the current work order; this reverts only the custom command/gate.

## BLOCKED

The current metadata cannot be read, a required shape cannot be consumed unambiguously, or full DEPS behavior needs an unadopted dependency. Report the exact input; do not weaken the check or bootstrap it with a manifest change.

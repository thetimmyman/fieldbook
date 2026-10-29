# day1-dependencies-checks-4-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask deps --check` reports DEPS-1 to DEPS-5 through its real command interface on the current workspace, without changing a manifest or lock. `day1-dependencies-checks` supplies the strict Cargo-metadata decoder; `day1-dependencies-checks-2` supplies complete target/profile measurements and raw declaration facts; `day1-dependencies-checks-3` supplies complete profile budgets and reports; `day1-dependencies-checks-4` supplies complete pure catalog evaluation; this card alone owns the complete command and gate. `day1-dependencies-2` supplies actual offline replay; `day1-dependencies-3` supplies bounded capture, initial declarations and production activation. Consumers of the initial declarations and recipes wait for `day1-dependencies-3`.

## Why

The complete checker is split at real provider seams under [Size and splitting](README.md#size-and-splitting). Cargo owns dependency semantics; the final command consumes the complete bounded providers and preserves every [dependency-policy](../reference/dependencies.md) control before its first clean gate.

## Scope

- Files: `xtask/src/deps.rs`, `xtask/src/deps_policy.rs`, `xtask/tests/deps.rs`, `xtask/tests/support/deps.rs`, `tests/fixtures/deps-command/`, `justfile` (add the checker after the existing dependency-policy checks).
- Registration lines: `xtask/src/main.rs` (module, command, usage and cwd selection for this command, plus removal of both input-provider unused-code expectations when full fact consumption makes them unnecessary); `xtask/src/deps_inputs.rs` (only the complete aggregate interface integration; module expectations live in main); `docs/reference/dependencies.md` (checker contract and Measurements row 0); `docs/controls.md` (DP-02 checker/test ownership); `AGENTS.md` (one Commands line).
- Docs to update: [dependencies.md](../reference/dependencies.md), DP-02 in `docs/controls.md`, one Commands line in `AGENTS.md`.

## Out of scope

Manifest, lock, crate, feature or pin changes; registry access; capture/replay; CI permissions; shipped behavior; Corpus/seed files; and activation of `just lock` or `just lock-verify` belong to later parts. No manifest change is admitted before the complete part-three gate exists.

## Preconditions

- Cards merged: `day1-dependencies-checks-4` (complete policy evaluation over all merged input, graph and budget providers).

## Steps

1. Read the complete strict `deps_inputs` aggregate once from the invocation workspace, then consume `deps_graph::measure` and `deps_policy::evaluate`. Reuse the complete catalog comparison and budget composition rather than copying its algorithms. Reuse the actual nested reference/configuration facts; add no parallel reader, collector, budget constants or report algorithm. Preserve relative input locations, exit 2 for unsupported/unreadable input or failed metadata, absent workspace-dependencies on the skeleton and all unused workspace pins. Preserve every applicability/error and raw-declaration/active-closure distinction, compatible replay shapes and uncapped all-target report. Cargo owns cfg, aliases and dependency semantics. Decoder, measurement and budget controls stay with their producing cards; this card proves their complete real-command consumption.
2. Expose no partial `deps --check` command: its only clean exit must mean all five checks ran for every applicable target/profile. Print one `DEPS-n <name>: <what is wrong>` line per finding. DEPS-1 requires a row with the same name and pin for every direct dependency, including a bare member pin before workspace inheritance; DEPS-2 compares every workspace pin; DEPS-3 enforces profile/target direct budgets and transitive caps; DEPS-4 compares each exact 40-hex workflow action SHA with its row; DEPS-5 compares each documented tool pin, including Rust and the lock resolver. Exit 0 clean, 1 findings, 2 for unsupported/unreadable input or failed metadata. Cargo-deny retains bans, licenses and source checks.
3. Tests in `xtask/tests/deps.rs` invoke `env!("CARGO_BIN_EXE_xtask")` in temporary synthetic workspaces. Write `deps_clean_workspace_passes`, `deps_reports_deps_1` through `deps_reports_deps_5`, `deps_uses_the_invocation_workspace`, `deps_rejects_duplicate_or_unsupported_input` and `deps_rejects_failed_metadata`. DEPS-1 covers both a missing row and a mismatched member pin. `deps_reports_deps_3` tests literal direct-budget boundaries 12/13 before workbook reader edges, 14/15 after them and 17/18 for the AI profile, plus transitive-cap boundaries 80/81 and 100/101. Catalog rows, lock entries, xtask edges and dev/build-only `zip` cannot select the higher silent ceiling; a single workbook dependency cannot select it either. Assert literal documented exits, code and relevant names. A fault in the invocation workspace must not be masked by a clean build checkout. Include all applicable targets/profiles, mixed normal/dev/build kinds, external namesakes, absent app and malformed applicability facts through the actual command, with independent process metadata and real local-Cargo cases. Every boundary and error control precedes the first enabled clean command; no later test-only part may defer one. Scratch is atomically claimed without pre-deletion, and fixtures bind through CARGO_MANIFEST_DIR.
4. Add the checker after the two cargo-deny lines in step 4 of `just check`. Measure the current graph and write only row 0; part three owns expanded-graph row 1. This part owns DP-02's no-row negative. Production replay activation and DP-04 remain with part three.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test deps
cargo xtask deps --check
git diff --exit-code -- Cargo.toml Cargo.lock crates/fieldbook/Cargo.toml crates/fieldbook-core/Cargo.toml crates/fieldbook-calc/Cargo.toml crates/fieldbook-render/Cargo.toml crates/fieldbook-win/Cargo.toml xtask/Cargo.toml
just check
```

Expected: real and synthetic command cases prove the documented behavior; manifests/lock are unchanged; row 0 is the actual baseline. No new dependency or registry access is needed.

## Negative control

Skip each DEPS check in turn: its named fault case must fail. Use the build checkout instead of invocation cwd: `deps_uses_the_invocation_workspace` must fail. Accept a duplicate tool key: `deps_rejects_duplicate_or_unsupported_input` must fail. Bypass a collector failure or skip an applicable graph in the command: its end-to-end graph case must fail; the producing decoder keeps its own shape/identity/kind/target controls. Restore every plant.

## Rollback

Revert the merge commit. The existing manifests and lock remain unchanged; the other two parts cannot become eligible without this checker.

## BLOCKED

The current metadata cannot be read, a required shape cannot be consumed unambiguously, or full DEPS behavior needs a dependency before replay exists. Report the exact input; do not weaken the check or bootstrap it with a manifest change.

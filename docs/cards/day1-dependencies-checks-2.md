# day1-dependencies-checks-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask deps --check` reports DEPS-1 to DEPS-5 through its real command interface on the current workspace, without changing a manifest or lock. `day1-dependencies-checks` supplies the strict Cargo-metadata decoder; this card alone owns the complete command and gate. `day1-dependencies-2` supplies actual offline replay; `day1-dependencies-3` supplies bounded capture, initial declarations and production activation. Consumers of the initial declarations and recipes wait for `day1-dependencies-3`.

## Why

- This is the complete checker part of the original card under [Size and splitting](README.md#size-and-splitting). The full [dependency policy](../reference/dependencies.md) is retained; the first manifest change still requires complete cooldown and replay gates.
- The test interface is the built xtask command in a temporary synthetic workspace. Cargo parses Cargo semantics; the checker does not grow a second general TOML parser or inspect its compile-time checkout instead of the invocation workspace.

## Scope

- Files: `xtask/src/deps_graph.rs`, `xtask/src/deps.rs`, `xtask/src/deps_policy.rs`, `xtask/src/deps_budget.rs`, `xtask/tests/deps.rs`, `xtask/tests/support/deps.rs`, `justfile` (add the checker after the existing dependency-policy checks).
- Registration lines: `xtask/src/main.rs` (module, command, usage and cwd selection for this command, plus removal of both input-provider unused-code expectations when full fact consumption makes them unnecessary); `xtask/src/deps_inputs.rs` (only the complete aggregate interface integration; module expectations live in main); `docs/reference/dependencies.md` (checker contract and Measurements row 0); `docs/controls.md` (DP-02 checker/test ownership); `AGENTS.md` (one Commands line).
- Docs to update: [dependencies.md](../reference/dependencies.md), DP-02 in `docs/controls.md`, one Commands line in `AGENTS.md`.

## Out of scope

- Manifest, lock, crate, feature or pin changes; registry access; capture/replay; CI permissions; shipped behavior; Corpus and seed files.
- Activating `just lock` or `just lock-verify`. No manifest change is admitted before the complete part-three gate exists.

## Preconditions

- Cards merged: `day1-dependencies-checks` (validated Cargo metadata facts) and `day1-dependencies-inputs` (the complete aggregate, including `day1-dependencies` reference facts).

## Steps

1. Use only existing serde_json and std. Implement target/profile measurement through the real `deps_metadata::decode` provider for every Cargo response, including feature applicability; add no second JSON shape/identity reader. Use `cargo metadata --locked --offline --format-version 1` from the invocation workspace for Linux/Windows, default features and `net` only when the actual workspace member exposes it. An absent app is inapplicable; an external namesake cannot select it. Preserve all raw member declaration requirements, including inactive optional/target dependencies, separately from active normal closures. Count distinct external crate/version pairs for transitives, workspace excluded; xtask is transitive-only. Keep the uncapped all-target report separate and let Cargo interpret cfg/version syntax.
2. Reuse the complete strict `deps_inputs` aggregate from `day1-dependencies-inputs`; never bypass it with the reference-only provider or add a second reader. Propagate its malformed/unsupported-input errors as exit 2 with relative source locations. Cargo remains the semantic parser for dependencies. An absent workspace-dependencies table is valid on the skeleton; later unused workspace pins must still match their rows. Consume every Cargo fact without silent `filter_map` skips; workflow enumeration must propagate entry errors. The tools reader must remain compatible with the documented replay record shapes.
3. Expose no partial `deps --check` command: its only clean exit must mean all five checks ran for every applicable target/profile. Print one `DEPS-n <name>: <what is wrong>` line per finding. DEPS-1 requires a row with the same name and pin for every direct dependency, including a bare member pin before workspace inheritance; DEPS-2 compares every workspace pin; DEPS-3 enforces profile/target direct budgets and transitive caps; DEPS-4 compares each exact 40-hex workflow action SHA with its row; DEPS-5 compares each documented tool pin, including Rust and the lock resolver. Exit 0 clean, 1 findings, 2 for unsupported/unreadable input or failed metadata. Cargo-deny retains bans, licenses and source checks.
4. Tests in `xtask/tests/deps.rs` invoke `env!("CARGO_BIN_EXE_xtask")` in temporary synthetic workspaces. Write `deps_clean_workspace_passes`, `deps_reports_deps_1` through `deps_reports_deps_5`, `deps_uses_the_invocation_workspace`, `deps_rejects_duplicate_or_unsupported_input` and `deps_rejects_failed_metadata`. DEPS-1 covers both a missing row and a mismatched member pin. `deps_reports_deps_3` tests literal direct-budget boundaries 12/13 before workbook reader edges, 14/15 after them and 17/18 for the AI profile, plus transitive-cap boundaries 80/81 and 100/101. Catalog rows, lock entries, xtask edges and dev/build-only `zip` cannot select the higher silent ceiling; a single workbook dependency cannot select it either. Assert literal documented exits, code and relevant names. A fault in the invocation workspace must not be masked by a clean build checkout.
5. Decoder shape/identity/kind/target/selector controls stay with the first provider. This part owns the complete real process/measurement seam: targets/profiles, mixed kinds, normal/dev/build closures, same-named external packages, absent app, invocation-root and failed metadata. Reuse the real decoder for the applicability query too; malformed facts cannot bypass its validation. Its typed declarations supply every direct pin for DEPS-1, while DEPS-3 uses active normal edges. Tests invoke the actual command with independent process metadata and real local-Cargo cases. Keep every literal direct/transitive boundary from step 4 before the first enabled clean command; no later test-only part may defer a required control. Test scratch must be atomically claimed, never pre-delete a guessed path; bind fixtures through the native Cargo manifest directory.
6. Add the checker after the two cargo-deny lines in step 4 of `just check`. Measure the current graph and write only row 0; part three owns expanded-graph row 1. This part owns DP-02's no-row negative. Production replay activation and DP-04 remain with part three.

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

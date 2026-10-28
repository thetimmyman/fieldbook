# day1-dependencies-checks

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask deps --check` reports DEPS-1 to DEPS-5 through its real command interface on the current workspace, without changing a manifest or lock. `day1-dependencies-2` supplies actual offline replay; `day1-dependencies-3` supplies bounded capture, initial declarations and production activation. Consumers of the initial declarations and recipes wait for `day1-dependencies-3`.

## Why

- This is the complete checker part of the original card under [Size and splitting](README.md#size-and-splitting). The full [dependency policy](../reference/dependencies.md) is retained; the first manifest change still requires complete cooldown and replay gates.
- The test interface is the built xtask command in a temporary synthetic workspace. Cargo parses Cargo semantics; the checker does not grow a second general TOML parser or inspect its compile-time checkout instead of the invocation workspace.

## Scope

- Files: `xtask/src/deps.rs`, `xtask/tests/deps.rs`, `justfile` (add the checker after the existing dependency-policy checks).
- Registration lines: `xtask/src/main.rs` (module, command, usage and cwd selection for this command); `xtask/src/deps_inputs.rs` (only the existing producer interface integration and removal of its temporary unused-code expectation); `docs/reference/dependencies.md` (checker contract and Measurements row 0); `docs/controls.md` (DP-02 checker/test ownership); `AGENTS.md` (one Commands line).
- Docs to update: [dependencies.md](../reference/dependencies.md), DP-02 in `docs/controls.md`, one Commands line in `AGENTS.md`.

## Out of scope

- Manifest, lock, crate, feature or pin changes; registry access; capture/replay; CI permissions; shipped behavior; Corpus and seed files.
- Activating `just lock` or `just lock-verify`. No manifest change is admitted before the complete part-three gate exists.

## Preconditions

- Cards merged: `day1-dependencies`.

## Steps

1. Use only existing `serde_json` and std. Implement `cargo xtask deps --check` against the invocation workspace, not `proc::repo_root()` compiled into the binary. Read `cargo metadata --locked --offline` for both graph targets, default features and `net` when that feature exists. Cargo supplies dependency requirements and normal-edge graphs. Count distinct external crate/version pairs, excluding workspace packages and dev/build-only paths; `xtask` counts toward the transitive cap but never the shipped direct budget. Keep the uncapped all-target report separate.
2. Reuse the strict `deps_inputs` producer from `day1-dependencies`; never add a second reader. Propagate its malformed/unsupported-input errors as exit 2 with relative source locations. Cargo remains the semantic parser for dependencies. An absent workspace-dependencies table is valid on the skeleton; later unused workspace pins must still match their rows. Consume every Cargo fact without silent `filter_map` skips; workflow enumeration must propagate entry errors. The tools reader must remain compatible with the documented replay record shapes.
3. Expose no partial `deps --check` command: its only clean exit must mean all five checks ran for every applicable target/profile. Print one `DEPS-n <name>: <what is wrong>` line per finding. DEPS-1 requires a row with the same name and pin for every direct dependency, including a bare member pin before workspace inheritance; DEPS-2 compares every workspace pin; DEPS-3 enforces profile/target direct budgets and transitive caps; DEPS-4 compares each exact 40-hex workflow action SHA with its row; DEPS-5 compares each documented tool pin, including Rust and the lock resolver. Exit 0 clean, 1 findings, 2 for unsupported/unreadable input or failed metadata. Cargo-deny retains bans, licenses and source checks.
4. Tests in `xtask/tests/deps.rs` invoke `env!("CARGO_BIN_EXE_xtask")` in temporary synthetic workspaces. Write `deps_clean_workspace_passes`, `deps_reports_deps_1` through `deps_reports_deps_5`, `deps_uses_the_invocation_workspace`, `deps_rejects_duplicate_or_unsupported_input` and `deps_rejects_failed_metadata`. DEPS-1 covers both a missing row and a mismatched member pin. Assert literal documented exits, code and relevant names. A fault in the invocation workspace must not be masked by a clean build checkout.
5. `deps_counts_normal_target_profile_graph` uses Cargo-process fixtures that distinguish normal/dev/build edges, workspace/external packages, optional net and Linux/Windows edges. The same command consumes those fixtures; retain real-Cargo clean/failure cases. This is the Cargo process seam, not a shipped I/O interface or a mock of checker internals. Independent expected counts must fail if xtask is excluded from transitives, counted as shipped, or a target/profile is skipped.
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

Skip each DEPS check in turn: its named fault case must fail. Use the build checkout instead of invocation cwd: `deps_uses_the_invocation_workspace` must fail. Accept a duplicate tool key: `deps_rejects_duplicate_or_unsupported_input` must fail. Count dev edges or skip a target/profile: `deps_counts_normal_target_profile_graph` must fail. Restore every plant.

## Rollback

Revert the merge commit. The existing manifests and lock remain unchanged; the other two parts cannot become eligible without this checker.

## BLOCKED

The current metadata cannot be read, a required shape cannot be consumed unambiguously, or full DEPS behavior needs a dependency before replay exists. Report the exact input; do not weaken the check or bootstrap it with a manifest change.

# day1-dependencies-inputs

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A complete strict `deps_inputs::read(root)` producer combines source references with workspace declarations, developer-tool pins and separate versioned replay configuration. It supplies `day1-dependencies-checks`, which alone owns the complete DEPS-1 to DEPS-5 command and gate.

## Why

- Configuration reading and final aggregation are a complete seam before Cargo graph measurement and policy decisions. This retains all input requirements after the source-reference provider split under [Size and splitting](README.md#size-and-splitting).
- The full [dependency policy](../reference/dependencies.md) and capture/replay gate remain required before manifest adoption. Readable policy violations are facts, never a parser verdict.

## Scope

- Files: `xtask/src/deps_inputs.rs` (configuration producer, aggregate and module-local tests), `tests/fixtures/deps-inputs/` (public/synthetic immutable inputs).
- Registration lines: `xtask/src/main.rs` (aggregate module and its temporary unused-code expectation; adjust the reference expectation only if actual field use makes it unfulfilled); `docs/reference/dependencies.md` (configuration/aggregate provider description and bounded grammar).
- Docs to update: [dependencies.md](../reference/dependencies.md).

## Out of scope

Reimplementing reference interpretation or its controls; CLI or `just check` activation; DEPS findings, measurements, DP-02, Cargo graph collection, manifests, lock files, new crates, capture/replay, CI permissions, shipped behavior, Corpus and seeds.

## Preconditions

- Cards merged: `day1-dependencies` (`deps_references::read_references`).

## Steps

1. Use existing serde_json and std. Provide the small `deps_inputs::read(root)` interface returning complete located workspace/configuration facts plus the existing reference facts, or a structured read error. Reuse `deps_references::read_references(root)` without another reference parser; propagate its errors and explicit root. Perform no policy comparison, Cargo process or resolution.
2. Consume the complete explicitly supported workspace-pin/declaration and tool/replay configuration shapes in [the provider grammar](../reference/dependencies.md#bootstrap-providers). Unknown, duplicate, unsupported, malformed, ambiguous or unreadable fields/sections refuse rather than disappear. Preserve all declaration fields and source locations. An absent `[workspace.dependencies]` table is valid on the skeleton; Cargo remains the semantic interpreter of dependency requirements and graphs.
3. Keep supported versioned replay configuration separate from developer-tool pins, preserving every field and ordered adjustment. Baseline nightly-only lock settings and complete documented replay records must both read. Do not freeze the incomplete baseline into the contract, grow a second generic TOML parser, silently drop future documented fields, or implement hash/date/path/approval/resolution verification; actual replay semantics remain with the verifier.
4. Module-local tests cross `deps_inputs::read` on complete immutable synthetic roots. Cover current and documented replay modes, cooldown/security adjustments, missing/duplicate/unknown/malformed/unreadable input, explicit invocation-root isolation, source locations and reference-error propagation. Valid missing-row, mismatched-pin and budget-violating declarations remain readable facts. Assert independently authored literal facts/errors; reference interpretation controls stay in their producing module.
5. Put immutable data under `tests/fixtures/deps-inputs/`; keep every executable setup, assertion and mutation counted and follow [Testing](../reference/testing.md). Compile normally. Retain a narrowly justified reference expectation while nested fact fields remain unused; remove it only if actual aggregate field use makes it unfulfilled. The aggregate may have its own narrow expectation; `day1-dependencies-checks` owns removing both module expectations in main when full fact consumption makes them unnecessary. Do not copy or touch facts merely to silence warnings. Register no command, measurement, DP-02 completion or policy gate.

## Acceptance

```sh
cargo nextest run --locked -p xtask deps_inputs::
just check
```

Expected: all selected aggregate/configuration tests execute; supported baseline and replay records retain complete facts; policy faults remain readable; malformed input refuses with a relative location; no dependency, manifest, lock or command/gate change occurs.

## Negative control

Skip malformed/duplicate configuration, drop a replay field or adjustment, use the build checkout instead of the supplied root, suppress a reference read error, or reject a valid policy fault as a parse error: its named aggregate case must fail. Restore every plant. A partial policy command is never an acceptable substitute.

## Rollback

Revert the merge commit. The independent reference provider remains available, while the complete checker remains ineligible without this aggregate.

## BLOCKED

A complete supported shape cannot be read unambiguously, more than 600 counted changed lines are required, or a dependency change or partial policy gate is needed. Preserve the work and propose a real seam; do not weaken the input contract.

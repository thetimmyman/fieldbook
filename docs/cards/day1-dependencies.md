# day1-dependencies

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A strict composed policy-input producer reads workspace declarations, tool pins and replay configuration and combines them with the complete catalog/workflow facts from `day1-dependency-references`, retaining relative source locations. It supplies `day1-dependencies-checks`, which alone owns the complete DEPS-1 to DEPS-5 command and gate.

## Why

- Input reading is a real seam before Cargo graph measurement and policy decisions. Valid declarations that violate policy remain facts; malformed or unsupported input is a read error.
- The full [dependency policy](../reference/dependencies.md) and the capture/replay gate remain required before any manifest adoption. This producer supplies no clean policy result.

## Scope

- Files: `xtask/src/deps_inputs.rs` (producer and module-local tests), `tests/fixtures/deps-inputs/` (public/synthetic immutable inputs).
- Registration lines: `xtask/src/main.rs` (module only and removal of the consumed reference provider's temporary unused-code expectation); `docs/reference/dependencies.md` (provider description).
- Docs to update: [dependencies.md](../reference/dependencies.md).

## Out of scope

CLI or `just check` activation, DEPS findings, measurements, DP-02, Cargo graph collection, manifests, lock files, new crates, capture/replay, CI permissions, shipped behavior, Corpus and seeds.

## Preconditions

- Cards merged: `day1-dependency-references`.

## Steps

1. Reuse `deps_references::read_references(root)` exactly once and re-export its `Source`, `ReadError`, `Row` (including canonical identity) and `ActionReference`; never duplicate or reparse those inputs. Remove the consumed provider's temporary unused-code expectation. Use existing serde_json and std. Provide a small `deps_inputs` interface that takes an explicit invocation root and returns complete documented rows, workspace declarations, tool pins and workflow references with relative source locations, or a structured read error. It performs no policy comparison and no Cargo resolution.
2. Consume complete explicitly supported workspace-pin/declaration and tool/replay configuration shapes, plus the complete reference provider facts. Unknown, duplicate, unsupported, malformed, ambiguous or unreadable input refuses; never skip a failed directory entry or malformed field. Cargo semantic dependency interpretation remains with the complete command. An absent workspace-dependencies table is valid on the skeleton.
3. Preserve supported versioned replay-record configuration separately from developer-tool pins. Do not freeze today's incomplete nightly-only lock configuration into the input contract, add a second generic TOML parser, or silently ignore later documented replay fields. The actual verifier retains replay semantics.
4. Module-local tests cross the producer interface on complete immutable synthetic fixtures. Cover valid current and documented replay shapes, missing/duplicate/unknown/malformed input, failed enumeration, an explicit invocation root and source locations. Valid missing-row, mismatched-pin and budget-violating declarations must remain readable facts for the later policy checks. Assert literal facts/errors, independently of the parser implementation.
5. Put immutable fixture contents under `tests/fixtures/deps-inputs/`; keep executable setup, assertions and mutation logic counted. Compile the module normally. A narrowly justified temporary unused-code expectation is allowed only until the consumer registers it; `day1-dependencies-checks` removes it. No `deps` command, measurement, DP-02 completion or policy gate is registered here.

## Acceptance

```sh
cargo nextest run --locked -p xtask deps_inputs
just check
```

Expected: all selected producer tests execute, valid policy faults are preserved for the consumer, malformed input refuses with a source location, and no dependency, manifest, lock or command/gate change occurs.

## Negative control

Skip a malformed/duplicate input or failed enumeration, read the build checkout instead of the supplied root, or reject a valid policy fault as a parse error: its named producer case must fail. Restore each plant. A partial policy command is never an acceptable substitute.

## Rollback

Revert the merge commit. Existing commands and policy gates remain unchanged; the complete checker remains ineligible.

## BLOCKED

A complete supported shape cannot be read unambiguously, more than 600 counted changed lines are required, or a dependency change or partial policy gate is needed. Preserve the work and propose a real seam; do not weaken the input contract.

# day1-dependencies-checks

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`deps_metadata::decode(bytes)` supplies complete strict Cargo format-1 declaration/resolve facts through read-only collections. It validates shapes and identity joins before construction, preserving every declaration and resolved edge kind/target. It performs no measurement, policy comparison or command. `day1-dependencies-checks-2` retains the collector and the entire DEPS-1 to DEPS-5 command, baseline row 0, DP-02 and gate.

## Why

The measured collector plus its tests and atomic coordination exceeds 600 lines. Decoding/validating Cargo's complete facts is a real seam before target/profile measurement and policy. This first part keeps the original slug; the remaining complete outcome and every enabled-handler control stay in the second part.

## Scope

- Files: `xtask/src/deps_metadata.rs` (decoder and module-local tests), `tests/fixtures/deps-metadata/` (immutable public/synthetic inputs and literal expectations), `docs/cards/day1-dependencies-checks-2.md` (complete remainder).
- Registration lines: `xtask/src/main.rs` (module and temporary unused-code expectation only); `docs/cards/day1-dependencies-2.md` (complete-checker prerequisite), `docs/cards/day1-dependencies-3.md` (checker/baseline provider names), `docs/cards/docs-checks.md` (control-test provider), `docs/cards/drafts/mutation-report.md` (complete-checker prerequisite), `docs/plan/PLAN.md` (the two split rows), `docs/reference/dependencies.md` (decoder/final-provider contract and row-0 provider), and `docs/controls.md` (DP-02 provider cell only).
- Docs to update: the enduring decoder/provider contract in `docs/reference/dependencies.md`.

## Out of scope

Cargo graph collection, a DEPS command, policy/budget decisions, a measurements row, DP-02 completion, just/CI activation, replay/capture, dependencies/manifests/lock, Corpus, seeds and shipped behavior.

## Preconditions

- Cards merged: `workspace-skeleton`.

## Steps

1. Use existing serde_json and std. Decode Cargo format-version 1 bytes, never a Cargo manifest or cfg expression. Require the documented field shapes; package IDs stay opaque nonempty strings. Preserve feature lists, all declared requirements/kinds/nullable targets and every resolved edge kind/nullable target. Do not turn absent or malformed required fields into defaults.
2. Validate unique workspace/package/node identities, every member and node owner, and each resolved edge's package and node. Join each edge kind/target to a matching declaration by target package name, kind and literal target. Do not match the edge's extern alias, evaluate cfg or version requirements, or require inactive declarations to have resolved edges.
3. Only the decoder constructs Metadata; expose its collections read-only. Generic workspace-package selection uses member IDs, ignores external namesakes, returns None when absent and refuses ambiguous member names. No project-name or nonempty-workspace policy is introduced.
4. Module tests assert complete independent literal facts, each independently malformed shape/join, selector behavior, byte/single-byte robustness and interoperability with actual Linux/Windows Cargo output. The real-Cargo case is compatibility evidence, not an independent graph-count oracle. Keep executable setup/assertions/mutations counted; no temp deletion or private stand-in parser.
5. Compile normally with one narrowly justified temporary unused-code expectation until real measurement consumes the facts. Redirect every complete-checker consumer to the remaining card; it must decode every actual metadata response, including the applicability query, and keep all required command tests before its first enabled clean result.

## Acceptance

```sh
cargo nextest run --locked -p xtask deps_metadata
just check
```

Expected: complete validated facts or an error, with no command/gate or manifest/lock change.

## Negative control

Skip a duplicate or a package/node/declaration join, default an absent kind, drop an edge kind/target or a declaration, select an external namesake, or accept a malformed required shape: its named producing-interface case must fail. Restore every plant.

## Rollback

Revert the merge commit. No policy gate was activated; the complete checker remains ineligible.

## BLOCKED

A required Cargo shape cannot be retained unambiguously, the actual complete diff exceeds 600 counted lines, or a dependency/partial policy gate is needed. Preserve the source and split only a complete demonstrated capability; never weaken the decoder or enabled tail.

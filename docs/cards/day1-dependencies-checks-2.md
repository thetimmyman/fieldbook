# day1-dependencies-checks-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`deps_graph::measure(root)` returns complete target/profile measurement facts and every raw member declaration pin, through the single validated Cargo decoder. This is a provider, with no CLI or policy gate. `day1-dependencies-checks-3` owns every DEPS algorithm, the complete command, baseline row 0, DP-02 and gate activation.

## Why

The complete checker exceeds the card-size limit. Cargo measurement is an independently usable capability with its own actual-Cargo controls. Separating it preserves the full final gate; no required command test moves after that gate.

## Scope

- Files: `xtask/src/deps_graph.rs`, `tests/fixtures/deps-graph/`, `xtask/src/deps_metadata.rs`, `tests/fixtures/deps-metadata/` (retain and validate Cargo manifest/declaration paths for member identity).
- Registration lines: `xtask/src/main.rs` (graph module only, no command); `docs/reference/dependencies.md` (provider contract); `docs/cards/day1-dependencies-checks-3.md`, `docs/cards/day1-dependencies-2.md`, `docs/cards/day1-dependencies-3.md`, `docs/cards/docs-checks.md`, `docs/cards/drafts/mutation-report.md`, `docs/plan/PLAN.md`, `docs/controls.md` (complete-checker scope and preconditions, plan order and DP-02 ownership).

## Out of scope

DEPS policy comparison, command registration, clean-gate activation, measurements rows, replay/capture, manifest/lock/dependency changes, shipped behavior, Corpus and seed files.

## Preconditions

- Cards merged: `day1-dependencies-checks` and `day1-dependencies-inputs`.

## Steps

1. Consume `deps_metadata::decode` for every real `cargo metadata --locked --offline --format-version 1` response from the invocation root: Linux, Windows and the separate all-target report, silent defaults and AI only when the actual workspace app has `net`. Absent app is inapplicable; external namesakes cannot select it. Preserve process/decoder errors.
2. Keep every raw normal/dev/build declaration requirement, including inactive optional/target dependencies. Exclude an internal path only by matching the declaration's canonical package name and Cargo-emitted directory to an actual member's manifest directory, never by name alone. The decoder alone validates these origin field shapes; retain conservatively when directory identity cannot match. Add no filesystem re-read, ID parser, alias/cfg or version evaluator.
3. Count active normal closures as distinct external name/version pairs, union the graph targets, exclude members, count xtask transitives but no shipped direct edges from it, and report the uncapped all-target closure separately. Workbook eligibility requires both active shipped normal edges.
4. Bind immutable fixtures through CARGO_MANIFEST_DIR; atomically claim scratch without pre-deletion. Actual Cargo cases cover mixed kinds, targets, optional AI, absent app, external namesakes, inactive pins, internal path exclusion, workbook stage and failed Cargo. Precise malformed origin controls stay with the decoder's existing refusal matrix.
5. Preserve the complete checker tail, its consumers and all process/budget/error controls before its first clean command. Do not enable a partial checker.

## Acceptance

```sh
cargo nextest run --locked -p xtask -E 'test(deps_graph) | test(deps_metadata)'
just check
```

Manifests and the production lock are unchanged. The complete command remains absent.

## Negative control

Drop inactive raw pins, exclude by member name, count dev/build edges, skip Windows, count xtask as shipped, select an external app, use the compiled checkout or swallow failed Cargo: the corresponding literal actual-Cargo case must fail. Restore each plant.

## Rollback

Revert the merge commit. The complete checker cannot become eligible without this provider.

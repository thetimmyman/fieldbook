# day1-dependencies

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A strict source-reference producer returns complete dependency catalog rows and workflow action references with relative source locations. It supplies `day1-dependencies-inputs`; only the later complete checker may report a clean policy result.

## Why

- Reference interpretation is a complete seam before workspace/configuration reading, Cargo graph measurement and policy decisions. Valid declarations that violate policy remain facts; malformed or unsupported input is a read error.
- The full [dependency policy](../reference/dependencies.md) and capture/replay gate remain required before any manifest adoption. The atomic split preserves every remaining input requirement in `day1-dependencies-inputs` and makes the checker wait for it.

## Scope

- Files: `xtask/src/deps_references.rs` (producer and module-local tests), `tests/fixtures/deps-references/` (public/synthetic immutable inputs), `docs/cards/day1-dependencies-inputs.md` (remaining producer).
- Registration lines: `xtask/src/main.rs` (reference module and temporary unused-code expectation only); `docs/reference/dependencies.md` (provider descriptions and bounded input grammar); `docs/plan/PLAN.md` (the two input-provider rows); `docs/cards/day1-dependencies-checks.md` (final-provider prerequisite, reuse and aggregate expectation ownership).
- Docs to update: [dependencies.md](../reference/dependencies.md), [PLAN.md](../plan/PLAN.md), `day1-dependencies-inputs.md` and `day1-dependencies-checks.md`.

## Out of scope

Workspace/tool/replay configuration reading and the final aggregate; CLI or `just check` activation; DEPS findings, measurements, DP-02, Cargo graph collection, manifests, lock files, new crates, capture/replay, CI permissions, shipped behavior, Corpus and seeds.

## Preconditions

- Cards merged: `workspace-skeleton`.

## Steps

1. Use existing serde_json and std. Provide `deps_references::read_references(root)` returning named reference facts or a structured read error, using the explicit invocation root without an environment, cwd or build-checkout fallback. Perform no policy comparison or Cargo resolution.
2. Read the four complete catalog tables and supported workflow action contexts according to the [bounded grammar](../reference/dependencies.md#bootstrap-providers), preserving every cell, canonical identity and relative source location. Keep scalar/code/data contexts distinct from references. Unknown, duplicate, unsupported, malformed, ambiguous or unreadable input refuses; enumerate deterministically and never skip a failed directory entry.
3. Test only the producing interface on complete immutable synthetic roots. `deps_references_complete_facts_from_supplied_root` owns literal rows/references, source locations, policy-fault preservation and scalar/data regressions; `deps_references_refuses_incomplete_and_ambiguous_inputs` owns literal malformed/duplicate/unsupported/enumeration errors. `deps_references_reads_current_sources` reads native inputs; `deps_references_bytes_and_single_byte_mutations_do_not_panic` follows [Testing](../reference/testing.md). Expected data is independent of the parser.
4. Put immutable data under `tests/fixtures/deps-references/`; keep all executable setup, projection, mutation and assertions counted. Compile normally with a narrowly justified temporary unused-code expectation while fact fields still lack a consumer. The aggregate may remove it if its actual use makes it unfulfilled; the complete checker owns removing both provider expectations when it consumes the facts. No command, policy result or gate is registered here.
5. Add the remaining input card and its PLAN row atomically with the checker prerequisite/reuse change. Reference interpretation and its controls remain owned here; configuration, final aggregation and their controls belong to the new card. The checker consumes only that final aggregate.

## Acceptance

```sh
cargo nextest run --locked -p xtask deps_references::
just check
```

Expected: all reference tests execute; complete rows and policy-fault references remain facts; malformed input refuses with a relative location; no dependency, manifest, lock or command/gate change occurs.

## Negative control

Skip a duplicate or failed enumeration, read the build checkout instead of the supplied root, reject a valid policy fault, scan scalar data as mappings, truncate a literal hash suffix, or treat a job name as a data context: its named interface case must fail. Restore every plant. No partial policy command substitutes for these controls.

## Rollback

Revert the merge commit. Existing commands and gates remain unchanged; the configuration aggregate and complete checker stay ineligible without this provider.

## BLOCKED

A complete supported shape cannot be read unambiguously, more than 600 counted changed lines are required, or a dependency change or partial policy gate is needed. Preserve the work and propose a real seam; do not weaken the input contract.

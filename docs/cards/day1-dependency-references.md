# day1-dependency-references

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A complete bounded catalog/workflow-reference producer returns documented rows and literal action references with relative source locations, or a located read error. It is the first real provider of `day1-dependencies`; no dependency-policy command or clean policy verdict is exposed.

## Why

The measured full input-reader candidate exceeded 600 counted lines. Markdown catalog and workflow-reference extraction is a real independent input interface; workspace/tool/replay configuration remains with the composed producer. The complete DEPS-1 to DEPS-5 checker and actual replay/adoption gates remain unchanged and downstream.

## Scope

- Files: `xtask/src/deps_references.rs` (producer and module-local tests), `tests/fixtures/deps-references/` (immutable public/synthetic inputs).
- Registration lines: `xtask/src/main.rs` (module and temporary unused-code expectation only); `docs/reference/dependencies.md` (provider contract).
- Docs to update: `docs/reference/dependencies.md`; the atomic split also updates this card, `day1-dependencies` and its prerequisite, and PLAN's producer order.

## Out of scope

Workspace/tool/replay configuration reading, policy findings, Cargo graph/resolution, CLI or gate activation, measurements, DP-02/DP-04 completion, manifests, lock, crates, capture/replay, CI permissions, shipped behavior, Corpus and seeds.

## Preconditions

- Cards merged: `workspace-skeleton`.

## Steps

1. Use std for `read_references(root) -> Result<References, ReadError>`. Keep `Source` relative with one-based lines; rows retain table, canonical identity and every cell, and workflows retain literal references. The composed producer reuses these types rather than copying them.
2. Consume the four documented complete tables and the explicitly supported block workflow-reference shapes from the invocation root. Unknown, duplicate, incomplete, unsupported or ambiguous input and failed enumeration refuse with a source location. Data/code/scalar bodies cannot become action references. Valid missing-row, mismatched-pin, tag, local and Docker policy faults remain facts for the later checker.
3. Execute module-local tests through this interface on immutable synthetic fixtures: complete literal outputs, malformed/ambiguous refusals, current source readability, deterministic random bytes and single-byte mutations without panic. Expected facts/errors are independent fixture literals.
4. Compile the module normally, with one temporary unused-code expectation until the composed producer consumes it. No command, measurements or partial policy gate is registered. Preserve all other dependency/replay contracts.

## Acceptance

```sh
cargo nextest run --locked -p xtask deps_references
just check
```

All four named tests execute; complete facts and located refusals are observed, and no manifest, lock, dependency, command or gate is changed.

## Negative control

Read the compiled checkout instead of the supplied root; swallow failed workflow enumeration; skip a duplicate identity; reject a valid policy fault; scan scalar bodies as mappings; truncate a literal hash suffix; or treat a keyword job id as data. Each corresponding named native case must fail. Restore each plant and observe green.

## Rollback

Revert the merge after reverting the composed producer/consumers that use this interface. Existing commands and dependency gates otherwise remain unchanged.

## BLOCKED

A complete supported shape cannot be read unambiguously, more than 600 counted changed lines are required, or a dependency change/partial policy gate is needed. Preserve the work and propose a real seam; do not weaken the contract.

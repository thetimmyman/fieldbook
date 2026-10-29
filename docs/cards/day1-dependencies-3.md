# day1-dependencies-3

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The Day-1 runtime/dev dependencies are declared once in `[workspace.dependencies]`, members inherit the entries they use, and the committed `Cargo.lock` builds on the pinned stable toolchain. Standard Cargo and cargo-deny pass. Exact-source MAX/JEV review accepts the pins, actual applicable profile counts and actual 14-day release evidence. No custom dependency CLI, lock verifier or replay helper is introduced.

## Why

- The work-order amendment in [AD-06](../decisions/0001-foundation.md#ad-06-dependency-allowlist) puts application work before custom dependency automation. [Review evidence](../reference/dependencies.md#review-evidence) is the required adoption recipe, including the unchanged cooldown and security-exception review.
- Pins, features and users remain in [dependencies.md](../reference/dependencies.md). `later-dependency-checks` and `later-lock-replay` retain the deferred complete controls; their implementation is not an adoption prerequisite and their controls remain unpassed.

## Scope

- Files: root `Cargo.toml` (only `[workspace.dependencies]`), `crates/{fieldbook,fieldbook-core,fieldbook-calc,fieldbook-render,fieldbook-win}/Cargo.toml`, `xtask/Cargo.toml`, `Cargo.lock`, and `deny.toml` only for an owner-approved licence exception.
- Registration lines: `docs/reference/dependencies.md` (actual Measurements rows 0/1, release-age/exception evidence and any pin/feature reconciliation).
- Docs to update: [dependencies.md](../reference/dependencies.md). Bind the reviewed source/lock and native evidence in the pull request; no new approval-record or configuration format.

## Out of scope

- `proptest` (Day 30), `embed-resource` (R2), later-only runtime rows and developer-tool bumps.
- Shipped behavior, custom commands/recipes, parser changes, baseline helpers, registry snapshots, `.config/lock-inputs/`, new `[lock]` fields and CI changes.
- A different version/feature without reconciling the Pins table and reviewing its actual release evidence. No Corpus/source/status or company permission changes.

## Preconditions

- Cards merged: `workspace-skeleton`. Existing reference/configuration/decoder/measurement/budget providers remain intact; no complete dependency command is required.

## Steps

1. On a clean current-main baseline, record row 0 from actual Cargo metadata/tree outputs using [Counting crates](../reference/dependencies.md#counting-crates); retain any existing historical row. Bind the baseline commit and lock. Historical probe estimates are not current measurements.
2. Declare the Day-1 pins, features and `default-features` once in `[workspace.dependencies]`. Add only the inherited member entries for the documented users. Keep later-only rows unactivated.
3. Retain the intended dev edges: `insta` in core/render/fieldbook; `tempfile` in fieldbook/xtask; `serde_json` in core; `toml` in calc/fieldbook; `zip` in xtask and render tests. Make xtask's `serde_json` inherited and add its path edge to `fieldbook-core` for the complete strings/docs provider. These edges change no shipped dependency semantics.
4. Run the existing `just lock`. Record the actual resolver, command, UTC cutoff, result and complete lock diff in the review evidence. If the pinned nightly rejects the flag/time format, perform the documented standard-Cargo fallback and precise downgrades. Keep the valid nightly-only `[lock]` configuration unchanged; do not invent a mode/cutoff/replay record or helper.
5. Obtain actual primary release-date evidence for every resolved registry package, and relevant tool/action pins. Keep the declared floors equal to the reviewed catalog rows, resolve the newest compatible releases that clear the cutoff, and check every floor and locked version against the 14-day policy. Disclose all young entries; an unresolved nonsecurity young entry blocks adoption until eligible or downgraded. A younger security fix requires the owner's approval and the existing exception table.
6. Build/test from the committed lock, then repeat the build with `--frozen` using the populated cache. Run both cargo-deny shapes. A licence exception requires owner review; no ban, network or source exception is implied.
7. Measure each applicable target/profile from actual Cargo metadata/tree outputs. Review all direct shipped declarations, canonical names/aliases, inactive and unused workspace pins, normal-edge direct budgets and two-target transitive union separately from dev/build edges. Add expanded row 1 (or the next row), retaining baseline/history. Report the uncapped all-target figure separately. MAX/JEV reviews the exact source/lock, raw outputs, pin table, action/tool pins, counts and release dates before merge.
8. Keep `just check`'s existing cargo-deny lines. Do not register a partial `deps --check`, `lock-verify`, custom baseline command or new CI gate. The complete deferred controls cannot be represented as passed by these standard checks.

## Acceptance

```sh
cargo nextest run --locked -p xtask
cargo metadata --locked --format-version 1 --filter-platform x86_64-unknown-linux-gnu
cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
cargo build --workspace --locked
cargo build --workspace --frozen
cargo deny --locked check bans licenses sources
cargo deny --locked --all-features check bans licenses sources
just check
```

Expected: exit 0 with the intended committed lock; cargo-deny reports `bans ok, licenses ok, sources ok`. Repeat applicable metadata/count/build checks with `--features net` only after that feature exists. The required source-bound MAX/JEV review and release-age evidence are recorded; a green build alone does not satisfy them. `--frozen` proves cached committed-lock buildability, not historical registry replay or application readiness.

## Negative control

In an atomically claimed private copy, add banned `tokio`, resolve that copy and run `cargo deny --locked check bans`: it must refuse the banned crate. Change a used requirement so its locked version no longer satisfies it: standard Cargo `--locked` must refuse a required lock update. Restore both plants. In separately labelled review copies, remove a required catalog row, alter a member/workspace pin, falsify a profile count or omit a locked package's release date: MAX/JEV must reject that evidence. Those are review controls, not DEPS command exits; no unimplemented automated negative is claimed green.

## Rollback

Revert the merge. Manifests and lock return to their pre-adoption state. Merged fact/measurement/budget providers remain. Consumers of the newly adopted crates and path edge require this card again.

## BLOCKED

A locked/frozen build or cargo-deny check fails; actual pin/release evidence is missing; an unapproved young release remains; or a direct/transitive count exceeds its current budget/cap. Report the exact package/profile and actual outputs. Do not raise a cap, waive a security exception or replace required evidence with an unimplemented checker.

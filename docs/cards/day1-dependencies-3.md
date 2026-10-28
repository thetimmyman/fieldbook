# day1-dependencies-3

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every Day-1 runtime and dev crate is declared once, in `[workspace.dependencies]` at its pin, each member manifest points at the entries it uses, and `Cargo.lock` was regenerated once by `just lock`. `cargo deny` passes, the crate counts are within the budget, and the Measurements table retains baseline row 0 and adds measured row 1. `cargo xtask deps --check` passes, `just lock-verify` re-resolves the lock to the same file, and `.config/tools.toml` holds the `[lock] cutoff` that `just lock` used.

## Why

- `just lock` rewrites the whole file, so the crates the next cards need go in as one batch ([AD-06](../decisions/0001-foundation.md#ad-06-dependency-allowlist)). The owner does not read each crate: a card that adds one is `auto` when `cargo xtask deps --check`, `cargo deny check` and `just lock-verify` pass and its row in [dependencies.md](../reference/dependencies.md) is in the same pull request ([Dependencies and the lock file](README.md#dependencies-and-the-lock-file)). The checker is provided by `day1-dependencies`; the actual offline verifier is provided by `day1-dependencies-2`. This part captures the production inputs and activates the complete gate before any manifest/lock adoption.
- The pins, features and users of each crate are in [Shipped runtime crates](../reference/dependencies.md#shipped-runtime-crates) and [Test-only and dev-only crates](../reference/dependencies.md#test-only-and-dev-only-crates). The rules are in [Where versions are declared](../reference/dependencies.md#where-versions-are-declared), [Counting crates](../reference/dependencies.md#counting-crates) and [The cooldown](../reference/dependencies.md#the-cooldown).

## Scope

- Files: root `Cargo.toml` (only `[workspace.dependencies]`), `crates/{fieldbook,fieldbook-core,fieldbook-calc,fieldbook-render,fieldbook-win}/Cargo.toml`, `xtask/Cargo.toml`, `Cargo.lock`, `xtask/src/{main.rs,lock_capture.rs}`, `xtask/tests/{lock_capture.rs,lock_bootstrap.rs}`, `xtask/lock-bootstrap.sh`, `justfile` (the recipes `lock`, `lock-verify` and `check`), `.config/tools.toml` (the complete versioned `[lock]` replay configuration), `.config/lock-inputs/` (public bounded registry snapshots and approval records), `.github/workflows/ci.yml` (one step), and `deny.toml` only if the owner agrees to a licence exception in step 5.
- Registration lines: `docs/reference/dependencies.md` (the cooldown/replay contract, Measurements table and Checked by cells of the Budget and Cap rows); `docs/controls.md` (DP-04 activation and its owning tests); `AGENTS.md` (one line of Commands); `xtask/src/{lock_replay.rs,lock_bundle.rs}` (only the capture-facing writer/validation entry points of the recorded format owned by part two).
- Docs to update: [dependencies.md](../reference/dependencies.md), the Measurements section and the "Checked by" cells of the Budget and Cap rows (`cargo xtask deps --check`, DEPS-3), `docs/controls.md` (DP-04 names the full capture/replay gate; DP-02 remains owned by the checker), `AGENTS.md` (Commands: `just lock` and `just lock-verify`, one line).

## Out of scope

- `proptest` (Day 30), `embed-resource` (R2) and every tool in Developer tools.
- Shipped runtime behavior changes. The xtask resolution/capture implementation and its tests are in scope; the complete checker and verifier are reused rather than reimplemented.
- Any version or feature that differs from the Pins table.

## Preconditions

- Cards merged: `day1-dependencies-2`.

## Steps

1. Confirm the existing baseline row 0 from the checker part against a throwaway clean `origin/main` worktree. Preserve that measured row. This part adds the expanded-graph row 1 and owns the initial declarations and resolution; no manifest change from this part merges without complete capture and offline replay.
1a. Make normal `just lock` bootstrap from a known consistent committed baseline, never by running Cargo against edited target manifests to obtain the helper. `xtask/lock-bootstrap.sh` builds xtask from an isolated copy of the committed baseline's source, manifests, lock and pinned stable toolchain using `cargo build --frozen -p xtask`. Locate the toolchain locally before invocation; missing baseline/toolchain/dependencies or inconsistent source refuses with exit 2, with no target mutation or implicit online resolution. Bind any reused binary to tool-source and locked-graph provenance. Run that binary directly against the invocation workspace, passing the target root internally rather than using compiled `proc::repo_root()`. Keep bootstrap build/cache files separate from capture's fresh registry cache and out of the recorded snapshot. For this initial implementation only, build the new serde_json/std capture helper before changing manifests, then invoke it during preparation; the helper stays ignored and uncommitted. After this part merges, the normal committed-baseline recipe works after manifest edits without a mandatory prebuild. The stable-fallback initial resolution is inside capture, never permission for an unrecorded bootstrap Cargo resolution.

2. Add `[workspace.dependencies]` to the root manifest. Write each version once, with the features and `default-features` of the Pins table, for the 11 runtime crates and for the dev crates `insta`, `tempfile` and `zip`.
3. In each member manifest, add `name = { workspace = true }` for the crates in its "Used by" column. Add the dev edges: `insta` in core, render and fieldbook; `tempfile` in fieldbook and xtask; `serde_json` in core; `toml` in calc and fieldbook; `zip` in xtask and, as a dev-dependency, in render. Change the `serde_json` line of `xtask` to the workspace entry. Add `fieldbook-core = { path = "../crates/fieldbook-core" }` to xtask before the initial lock resolution; core-types-and-strings consumes this edge without another manifest edit.
4. Run `just lock`. If the nightly rejects `--publish-time`, follow the fallback in The cooldown and list the young entries in [Young lockfile entries](../reference/dependencies.md#young-lockfile-entries).
5. Run the `cargo deny` command of the Acceptance block. If it names a licence outside the allowlist (the one to watch is `typed-path` below `zip`), stop and write BLOCKED. The owner decides whether `deny.toml` gains an exception.
6. Count again for the silent profile. Direct runtime crates must meet its budget. The two-target transitive count must meet its profile cap and fill row 1's Transitive crates cell. Record the uncapped all-target count separately in Notes, with no equality requirement. `cargo xtask deps --check` checks the capped measurement; AI measurement begins when that feature exists.
7. Add only row 1 for this lock to Measurements, retaining row 0 from `day1-dependencies`. Say which provider measured each row; each later lock-refresh adds the next row.
8. Implement capture for the single recorded-resolution/replay contract in [The cooldown](../reference/dependencies.md#the-cooldown), including the mode, resolver, bounded candidate cache, input digest, cutoff and ordered approved adjustments. `just lock` writes that record; `just lock-verify` replays it without the candidate lock, compares bytes (exit 1 on a difference) and checks cutoff age. Missing resolver or replay material exits 2. Reuse part two’s actual verifier and strict bundle/record format; do not substitute a second checker or compare precomputed output. Capture only consulted public crates.io entries and archives with original identity, hashes and publish dates; retain initial stable candidates before any precise downgrade and all security-override inputs. Add the CI step for a lock, manifest, resolution configuration or replay-input diff; the weekly job uses the same recipe. Preserve the fallback young-entry table and security approval record.
9. Add `lock_bootstrap_ignores_edited_target_manifests`, `lock_bootstrap_missing_or_stale_inputs_refuse_without_mutation` and `lock_bootstrap_and_capture_caches_are_separate` in `xtask/tests/lock_bootstrap.rs` through the bootstrap/lock command. Prove an edited target cannot change the frozen tool build, that missing/stale inputs make no target mutation or online resolution, and that capture digests the actual target inputs. Re-run all DEPS-1 to DEPS-5 process cases from the checker part and `cargo xtask deps --check` on the actually expanded workspace. Do not reimplement or weaken those checks. Add capture process tests in `xtask/tests/lock_capture.rs`: `lock_capture_preserves_initial_candidates`, `lock_capture_production_record_replays_offline`, `lock_capture_replay_survives_time_advance` and `lock_capture_security_override_needs_bound_approval`. The capture output must pass the actual part-two verifier; that verifier is never replaced with a stored digest/descriptor comparison. Retain failure coverage for unsafe paths, missing approval bindings and changed resolution inputs from part two.

10. Keep exactly one `cargo xtask deps --check` in step 4 of `just check`, as part one added. Wire `just lock` and `just lock-verify` to the complete capture/verifier interfaces. Add the same offline verification command to CI for a lock/manifest/resolution-config/replay-input change and to the weekly contract; do not change workflow permissions or allow a declarations-only merge.

11. Run manual registry-dependent replay controls in a scratch checkout: record and replay a stable-fallback resolution, then an approved young security override; both match after the clock advances. Remove the override approval record: verification must fail before comparison. Retain the existing changed-lock negative control. These controls are documented in the pull request and are not ordinary offline Rust tests. Unit tests of replay-record parsing cover missing/duplicate fields, unsafe paths and missing approval bindings.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test deps --test lock_replay --test lock_capture --test lock_bootstrap
grep -nE '^[a-z_-]+ = "[0-9]' crates/*/Cargo.toml xtask/Cargo.toml
cargo build --workspace --locked
cargo deny --locked check bans licenses sources
cargo xtask deps --check
just lock-verify
cargo tree --workspace -e normal --target all --prefix none --no-dedupe --format '{p}' | grep -v ' (/' | grep -v ' ([A-Za-z]:' | sort -u | wc -l
grep -n "cargo xtask deps --check" justfile
just check
```

Expected: the `grep` prints nothing (no member declares a version); the build and `cargo deny` exit 0 with `bans ok, licenses ok, sources ok`; `deps --check` and `lock-verify` exit 0; the `grep -n` of `justfile` prints one line; the silent profile's two-target transitive count meets its cap and equals row 1's Transitive crates cell; the separate all-target figure appears in Notes and has no cap; `just check` exits 0.

## Negative control

In a scratch copy of the repository, add `tokio = "1"` under `[dependencies]` of `crates/fieldbook-core/Cargo.toml` and run `cargo update --workspace`. Then run `cargo deny --locked check bans`. It must exit 2 and print that `tokio` is explicitly banned. Delete the copy. Also put `serde = "1"` in a member manifest of the real tree: the `grep` of the Acceptance block must print that line. Remove it again. Then remove one row from `dependencies.md` in the scratch copy: `cargo xtask deps --check` must exit 1 with `DEPS-1`. Run `just check` in that copy: it must fail at the line `cargo xtask deps --check`. Change one pin in `Cargo.toml` only: it must exit 1 with `DEPS-2`. Change one line of `Cargo.lock` by hand: `just lock-verify` must exit 1.

Change only a workspace path edge or feature in a manifest while retaining the old resolution record and lock: `just lock-verify` must fail its input-digest check before replay. Restore the manifest. This also covers a dependency already present in the lock.

Let bootstrap compile against edited target manifests, reuse mismatched provenance, resolve online or copy its normal Cargo cache into capture: the corresponding bootstrap test must fail. Restore it.

## Rollback

Revert the merge commit. `Cargo.lock` returns to the pre-adoption graph and the manifests to their earlier form. The independently merged checker and offline verifier remain available; only this part’s capture/production activation is reverted. Downstream consumers of the initial dependency set require this part, not the first two alone.

## BLOCKED

- `just lock` fails, or the nightly rejects `--publish-time` and the fallback also fails: write the error and stop.
- `cargo deny` reports a licence, ban or source problem: write the crate, its licence and the crate that pulls it in.
- The direct runtime count or two-target transitive count exceeds its applicable budget: record both measurements and the five largest contributors. The uncapped all-target Notes figure cannot cause this block.
- `cargo xtask deps --check` exits 2 because a table of `dependencies.md` cannot be parsed: write the table name and stop.

# day1-dependencies

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every Day-1 runtime and dev crate is declared once, in `[workspace.dependencies]` at its pin, each member manifest points at the entries it uses, and `Cargo.lock` was regenerated once by `just lock`. `cargo deny` passes, the crate counts are within the budget, and the Measurements table holds rows 0 and 1. `cargo xtask deps --check` passes, `just lock-verify` re-resolves the lock to the same file, and `.config/tools.toml` holds the `[lock] cutoff` that `just lock` used.

## Why

- `just lock` rewrites the whole file, so the crates the next cards need go in as one batch ([AD-06](../decisions/0001-foundation.md#ad-06-dependency-allowlist)). The owner does not read each crate: a card that adds one is `auto` when `cargo xtask deps --check`, `cargo deny check` and `just lock-verify` pass and its row in [dependencies.md](../reference/dependencies.md) is in the same pull request ([Dependencies and the lock file](README.md#dependencies-and-the-lock-file)). This card builds those checks.
- The pins, features and users of each crate are in [Shipped runtime crates](../reference/dependencies.md#shipped-runtime-crates) and [Test-only and dev-only crates](../reference/dependencies.md#test-only-and-dev-only-crates). The rules are in [Where versions are declared](../reference/dependencies.md#where-versions-are-declared), [Counting crates](../reference/dependencies.md#counting-crates) and [The cooldown](../reference/dependencies.md#the-cooldown).

## Scope

- Files: root `Cargo.toml` (only `[workspace.dependencies]`), `crates/{fieldbook,fieldbook-core,fieldbook-calc,fieldbook-render,fieldbook-win}/Cargo.toml`, `xtask/Cargo.toml`, `Cargo.lock`, `xtask/src/{main.rs,deps.rs}`, `xtask/tests/deps.rs`, `justfile` (the recipes `lock`, `lock-verify` and `check`), `.config/tools.toml` (the complete versioned `[lock]` replay configuration), `.config/lock-inputs/` (public bounded registry snapshots and approval records), `.github/workflows/ci.yml` (one step), and `deny.toml` only if the owner agrees to a licence exception in step 5.
- Registration lines: `docs/reference/dependencies.md` (the cooldown/replay contract, Measurements table and Checked by cells of the Budget and Cap rows); `docs/controls.md` (the DP-02 and DP-04 text and the Limits line); `AGENTS.md` (one line of Commands).
- Docs to update: [dependencies.md](../reference/dependencies.md), the Measurements section and the "Checked by" cells of the Budget and Cap rows (`cargo xtask deps --check`, DEPS-3), `docs/controls.md` (DP-02 and DP-04 name `cargo xtask deps --check`, and the Limits line that says no tool checks them goes), `AGENTS.md` (Commands: `cargo xtask deps --check`, one line).

## Out of scope

- `proptest` (Day 30), `embed-resource` (R2) and every tool in Developer tools.
- Using a crate in code. No `use` line is added, so no code changes.
- Any version or feature that differs from the Pins table.

## Preconditions

- Cards merged: `workspace-skeleton`.

## Steps

1. Count the lock as it stands. In a throwaway worktree of `origin/main`, run the counting command of [Counting crates](../reference/dependencies.md#counting-crates), note both numbers for row 0, and remove the worktree.
2. Add `[workspace.dependencies]` to the root manifest. Write each version once, with the features and `default-features` of the Pins table, for the 11 runtime crates and for the dev crates `insta`, `tempfile` and `zip`.
3. In each member manifest, add `name = { workspace = true }` for the crates in its "Used by" column. Add the dev edges: `insta` in core, render and fieldbook; `tempfile` in fieldbook and xtask; `serde_json` in core; `toml` in calc and fieldbook; `zip` in xtask and, as a dev-dependency, in render. Change the `serde_json` line of `xtask` to the workspace entry. Add `fieldbook-core = { path = "../crates/fieldbook-core" }` to xtask before the initial lock resolution; core-types-and-strings consumes this edge without another manifest edit.
4. Run `just lock`. If the nightly rejects `--publish-time`, follow the fallback in The cooldown and list the young entries in [Young lockfile entries](../reference/dependencies.md#young-lockfile-entries).
5. Run the `cargo deny` command of the Acceptance block. If it names a licence outside the allowlist (the one to watch is `typed-path` below `zip`), stop and write BLOCKED. The owner decides whether `deny.toml` gains an exception.
6. Count again for the silent profile. Direct runtime crates must meet its budget. The two-target transitive count must meet its profile cap and fill row 1's Transitive crates cell. Record the uncapped all-target count separately in Notes, with no equality requirement. `cargo xtask deps --check` checks the capped measurement; AI measurement begins when that feature exists.
7. Add row 0 (step 1) and row 1 (this lock) to Measurements. In the sentence above the table, say that this card adds rows 0 and 1 and that each `lock-refresh` card adds the next row.
8. Implement the single recorded-resolution/replay contract in [The cooldown](../reference/dependencies.md#the-cooldown), including the mode, resolver, bounded candidate cache, input digest, cutoff and ordered approved adjustments. `just lock` writes that record; `just lock-verify` replays it without the candidate lock, compares bytes (exit 1 on a difference) and checks cutoff age. Missing resolver or replay material exits 2. Add the CI step for a lock, manifest, resolution configuration or replay-input diff; the weekly job uses the same recipe. Preserve the fallback young-entry table and security approval record.
9. `cargo xtask deps --check` in `xtask/src/deps.rs`: it reads `[workspace.dependencies]` and the member manifests, `cargo metadata --locked --offline`, `.config/tools.toml`, the `uses:` lines of `.github/workflows/*.yml` and the tables of `dependencies.md`, and prints `DEPS-n <name>: <what is wrong>`, one line each. DEPS-1: every direct dependency has a `dependencies.md` row with the same name and pin. DEPS-2: every pin in `[workspace.dependencies]` equals its row's pin. DEPS-3: direct runtime crates at most the budget number in `dependencies.md`, and transitive crates at most the cap number, once for each profile (`cargo metadata` with default features for the silent build, and with `--features net` for the AI build once that feature exists). DEPS-4: every workflow `uses:` is `owner/repo@<40 hex>` and has a row. DEPS-5: every tool in `.config/tools.toml` has a row and a pin. It does not repeat bans, licences and sources (`cargo deny check`) or the cooldown (`lock-verify`). Exit 0 clean, 1 findings, 2 BLOCKED (`cargo metadata` failed, or a table could not be parsed). Raising the budget or the cap is an edit of that cell with a reason in the pull request, not a gate; the owner reads it in the week's merged work. Tests in `xtask/tests/deps.rs`, which run the built binary (`env!("CARGO_BIN_EXE_xtask")`) with a temporary synthetic workspace as the working directory: a clean one passes, and one planted fault for each of DEPS-1 to DEPS-5 exits 1 naming that id.
10. In the `check` recipe of the `justfile`, add `cargo xtask deps --check` to step 4 (Dependency policy), after the two `cargo deny` lines.

11. Run manual registry-dependent replay controls in a scratch checkout: record and replay a stable-fallback resolution, then an approved young security override; both match after the clock advances. Remove the override approval record: verification must fail before comparison. Retain the existing changed-lock negative control. These controls are documented in the pull request and are not ordinary offline Rust tests. Unit tests of replay-record parsing cover missing/duplicate fields, unsafe paths and missing approval bindings.

## Acceptance

```sh
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

## Rollback

Revert the merge commit. `Cargo.lock` returns to the 16 entries of the skeleton and the manifests to their earlier form; `deps --check` and `lock-verify` go with it. Nothing else depends on the new entries yet.

## BLOCKED

- `just lock` fails, or the nightly rejects `--publish-time` and the fallback also fails: write the error and stop.
- `cargo deny` reports a licence, ban or source problem: write the crate, its licence and the crate that pulls it in.
- The direct runtime count or two-target transitive count exceeds its applicable budget: record both measurements and the five largest contributors. The uncapped all-target Notes figure cannot cause this block.
- `cargo xtask deps --check` exits 2 because a table of `dependencies.md` cannot be parsed: write the table name and stop.

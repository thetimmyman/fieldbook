# Toolchain bump runbook

How the pinned Rust toolchain moves to a newer minor release. The rules are AD-02, AD-06 and AD-24 in [ADR 0001](../decisions/0001-foundation.md). A bump is its own card, never part of other work.

| Rule | Value |
|---|---|
| Cadence | Look every 12 weeks. Never bump more often |
| Cooldown | The minor release (`1.NN.0`) must be at least 14 days old. The pin is that minor's newest point release |
| MSRV | Equals the pin: `rust-version` in `Cargo.toml` is the minor (`1.NN`), `channel` in `rust-toolchain.toml` is the full version |
| Who | Owner picks the version and reviews; Codex edits and runs the checks |

A point release of the minor already pinned (for example `1.99.2` after `1.99.1`) needs no 12-week wait and no cooldown wait. It uses steps 3 to 10; the owner approves it.

## Step 1. Check that it is time (owner)

```sh
echo $(( ( $(date +%s) - $(git log -1 --format=%ct -- rust-toolchain.toml) ) / 86400 ))
```

Expected: a number of at least 84, the days since the pin last changed. Below 84, stop.

## Step 2. Choose the target (owner)

Find the newest minor whose first release is at least 14 days old. Replace `NN` with a candidate minor, trying the next minor after the pin first:

```sh
date -u -d '14 days ago' +%F
for p in 0 1 2 3; do curl -fsS "https://static.rust-lang.org/dist/channel-rust-1.NN.$p.toml" 2>/dev/null | awk -v v="1.NN.$p" -F'"' '/^date =/ {print v, $2; exit}'; done
```

Expected: the first line is the cutoff date; each later line is a release of that minor with its date. The minor qualifies when the `1.NN.0` date is on or before the cutoff. The target is the highest listed point release of the newest qualifying minor.

If the `1.NN.0` line is missing or later than the cutoff, that minor is not eligible. Use the pinned minor's newest point release instead, or stop.

Then create the card with `just new-card toolchain-bump-1-NN`, name the target in its Outcome, and set Review to `owner`. Codex does steps 3 to 9 and the owner does step 10.

## Step 3. Edit the two pins (Codex)

In `rust-toolchain.toml` set `channel = "1.NN.P"`. In the root `Cargo.toml` set `rust-version = "1.NN"` under `[workspace.package]`. Then find stragglers:

```sh
git grep -n -F "<old version>"
```

Expected: only places that describe history, if any. Update any other mention (for example the toolchain row in [dependencies.md](../reference/dependencies.md)). Do not touch `Cargo.lock`.

## Step 4. Install and confirm the toolchain (Codex)

```sh
rustup toolchain install
rustc --version
```

Expected: `rustc 1.NN.P (...)`. If rustup cannot find the version, the point release is not published yet; return to step 2.

## Step 5. Re-run the doctor (Codex)

```sh
just doctor
```

Expected: exit code 0, with the toolchain reported at exactly the new pin and every other tool at or above its minimum. If a tool fails its minimum, that is a separate card; do not change unrelated tool pins in `.config/tools.toml` here.

## Step 6. Refresh the lock and run the full check (Codex)

```sh
just lock
cargo build --workspace --locked
just check
```

Expected: exit code 0. New releases often make new lints fire or change formatting. Fix them in this card:

- A new warning from a denied lint group: fix the code. Never add `#[allow]`; a justified exception is `#[expect(..., reason = "...")]`.
- An `#[expect]` that is now unfulfilled is an error: remove it.
- Formatting changes from the new rustfmt: run `cargo fmt` and commit it on its own.

If the fixes pass about 400 changed lines, stop and ask the owner.

## Step 7. Review and stage the lock changes (Codex)

Review the `Cargo.lock` diff and actual resolver/cutoff/release evidence under [Review evidence](../reference/dependencies.md#review-evidence). A manifest or resolution-configuration change requires `just lock` even when dependencies stay the same. Stage new inputs explicitly:

```sh
git add Cargo.toml Cargo.lock rust-toolchain.toml
```

Expected: standard Cargo accepts the committed lock after resolution; exact-source MAX/JEV review accepts the actual pins, profile budgets and release-age evidence. No historical replay is asserted. Unrelated tool pins belong to a separate card.

## Step 8. Re-check lints and hardening flags (Codex)

Lints:

```sh
cargo clippy --workspace --all-targets --locked 2>&1 | grep -iE 'unknown lint|renamed|deprecated|removed'
```

Expected: no output, so every lint named in `[workspace.lints]` still exists with its old meaning. Then read the release notes of the new minor, in particular the compiler and Cargo compatibility notes, and confirm none of the 17 named clippy lints, `unsafe_code = "forbid"` or the `disallowed-*` entries in `clippy.toml` changed behaviour.

Hardening flags (from R2, when `.cargo/config.toml` carries them):

```sh
rustc --target x86_64-pc-windows-msvc -C target-feature=+crt-static -C control-flow-guard=checks --print cfg > /dev/null
```

Expected: exit code 0, so rustc still accepts the flags it checks. The linker flags (`/DEPENDENTLOADFLAG:0x800`, `/Brepro`, `/PDBALTPATH:%_PDB%`) are checked by the full Windows run in step 9, and by BinSkim on the `dist` build once R2 adds it: no new failure compared with the previous release. Check the release notes for any change to control-flow guard in the standard library; the statement given to IT is that it covers first-party code only, so correct the intake sources in `docs/intake/` if the notes change that.

Until R2, `.cargo/config.toml` holds only the xtask alias and this step reduces to the lint check.

If a flag is rejected or its behaviour changed, do not drop it. Stop and ask the owner: this is a CI and release change (the release gate).

## Step 9. Run the full Windows run (Codex)

The `windows` job exits early on a pull request unless it sees code or CI changes, so ask for the full run:

```sh
gh workflow run scheduled.yml --ref card/toolchain-bump-1-NN
gh run list --workflow scheduled.yml --limit 1
gh run watch <run-id> --exit-status
```

Expected: success. This includes the weekly checks (advisories, full-history gitleaks, the lint controls, the cross-OS hash compare) and the Windows run. If the cross-OS compare fails after a bump, the new toolchain changed output; stop and report it, because determinism is an invariant.

## Step 10. Review and merge (owner)

The owner reviews the diff (two pins, small fixes, docs) and merges when `ci-ok` is green.

## What a bump unlocks

When the new pin reaches these versions, do the replacement in the same card as the bump, or in a follow-up card straight after it. Each replaces something temporary.

### From 1.100: the cooldown moves into Cargo (`min-publish-age`)

Replaces the nightly in `just lock` (AD-06).

1. Run the old recipe and keep its result:

   ```sh
   scratch="$(mktemp -d)"
   just lock && cp Cargo.lock "$scratch/Cargo.lock.nightly"
   ```

2. In `.cargo/config.toml` add:

   ```toml
   [registry]
   global-min-publish-age = "14 days"
   ```

3. Change the `lock` recipe in the `justfile` to run `cargo generate-lockfile` on the pinned toolchain, with no nightly and no `-Z` flag.
4. Verify the new setting against actual resolved-package release dates and retain the old lock/output for comparison. Keep the nightly resolver pin until this resolution-only migration passes; there is no present custom verifier or recorded-cutoff equality requirement. Reconcile tools/setup/doctor documentation if the resolver pin is removed. The independent fuzz pin remains.
5. Run the new recipe and compare:

   ```sh
   just lock && diff Cargo.lock "$scratch/Cargo.lock.nightly"
   ```

Expected: inspect any lock difference, then obtain exact-source MAX/JEV pin/profile/release-age review and passing committed-lock Cargo/cargo-deny checks. This migrates cooldown resolution, not a historical replay verifier. If entries younger than 14 days appear, the key was not honoured: check its spelling against the Cargo book for the pinned version, and keep the nightly recipe until it is.

### From 1.101: `trim-paths` replaces the path remaps

Replaces the `--remap-path-prefix` flags for the workspace, vendor and `CARGO_HOME` paths that CI appends through `cargo --config` (AD-20).

1. Find them: `git grep -n -e remap-path-prefix`. They live in the release and rehearsal workflows from R2 on.
2. Set `trim-paths = "all"` in `[profile.dist]` of `Cargo.toml` and delete the remap flags. Keep `/PDBALTPATH:%_PDB%`, which `trim-paths` does not replace.
3. Build the `dist` profile on windows-2025 twice, as the rehearsal does, and search the exe for the three path prefixes the remaps used to hide:

   ```powershell
   findstr /m /l /c:"<workspace path>" /c:"<vendor path>" /c:"<cargo home path>" target\dist\fieldbook.exe
   ```

Expected: no output from `findstr`, and the two exe hashes match. If a path appears, `trim-paths` does not cover that one; restore its remap and note which path in `dependencies.md`.

If the remap flags do not exist yet when 1.101 is pinned, the card that writes the rehearsal job uses `trim-paths` directly.

## Evidence the card records

The pull request description carries all of this; the card's work item gets one comment with the PR number and the `ci-ok` run id.

| Item | Where it comes from |
|---|---|
| Pin before and after | `git diff` of `rust-toolchain.toml` |
| Release date of the minor and the cutoff date | Step 2 output |
| `just doctor` passes at the new pin | Step 5 |
| Reviewed committed lock, actual Cargo/cargo-deny outputs and MAX/JEV pin/profile/release-age evidence | Steps 6 and 7 |
| Lint and flag re-checks, with any fixes | Step 8 |
| Full Windows run id, green | Step 9 |
| `just check @ <pr-head-sha> (PR #N, ci-ok run <id>) -> pass` | The pull request |
| For 1.100 or 1.101: the lockfile diff or the exe hash match | The replacement section above |

The resolver migration follows [Review evidence](../reference/dependencies.md#review-evidence). Retain owner-approved security exceptions and exact updates in the evidence; the valid nightly-only `[lock]` needs no replay record. Complete replay/bootstrap remains deferred.

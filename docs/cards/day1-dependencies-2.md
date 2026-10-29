# day1-dependencies-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The offline lock verifier performs actual Cargo re-resolution from a recorded bounded registry bundle in an empty temporary checkout and isolated Cargo cache, then compares the generated lock byte for byte. It never starts from the candidate lock or today's registry. Production manifests, lock and replay configuration remain unchanged in this part.

## Why

- This part owns the verification interface of [Recorded resolution and replay](../reference/dependencies.md#the-cooldown) before part three can capture and activate a production record. Descriptor/hash validation alone cannot prove the recorded resolution.
- Tests cross the real binary command and fixture-record interface; no product model or network path is added.

## Scope

- Files: `xtask/src/{lock_replay.rs,lock_bundle.rs}`, `xtask/tests/lock_replay.rs`, `tests/fixtures/lock-replay/` (public/synthetic bundles, records, approval records and expected locks).
- Registration lines: `xtask/src/main.rs` (modules and `lock-verify [--record FILE]`); `docs/reference/dependencies.md` (strict record/bundle and verifier contract).
- Docs to update: [dependencies.md](../reference/dependencies.md).

## Out of scope

- Cargo.toml, Cargo.lock and production `.config/tools.toml` changes; online capture/fetching; resolver auto-install during verification; just/CI/weekly activation; new crates; shipped I/O and Corpus changes.

## Preconditions

- Cards merged: `day1-dependencies-checks-2`.

## Steps

1. Implement `cargo xtask lock-verify [--record FILE]` against the invocation workspace. The default is the versioned `[lock]` in `.config/tools.toml`; fixture records use the identical validator/replay path. Preserve replay_schema, exact mode/resolver/cutoff, input digest, snapshot path/hash, bounded entry manifest and ordered adjustments. Digest sorted exact manifest and Cargo-resolution-config bytes, excluding the record itself and Cargo.lock. Consume the entire declared record shape, rejecting unknown/duplicate/unsupported fields with exit 2; do not grow a second generic TOML parser.
2. `lock_bundle` validates repository-relative paths, immutable archive and entry hashes, original crates.io identity, checksums and publish dates before installation. Reject absolute/traversing paths, links, undeclared/duplicate entries, truncation and hash mismatches. Existing system archive/hash tools may be used only behind this xtask interface after validation; no unsafe extraction or unrelated Cargo-home file is accepted. Do not add a dependency before the replay gate exists.
3. Create an empty temporary checkout and isolated empty Cargo cache. Install only validated bundle entries and recorded resolution inputs. Assert that the candidate Cargo.lock is absent before Cargo runs. Locate the exact recorded resolver locally; absence is exit 2 with no auto-install/network fallback. Generate offline with recorded mode/cutoff, then replay ordered precise updates offline. Stable fallback uses captured initial candidates; publish-time uses its recorded cutoff. Preserve original registry identity and use no current registry or normal Cargo cache.
4. Before replay, validate every security adjustment's advisory, owner-approval commit, approval-record path/hash and binding to input digest/candidate/version/cutoff. Missing or changed approval fails before comparison. Preserve old adjustments when time advances; only a fresh resolution may omit an override after it qualifies at the new cutoff. Preserve ordered cooldown adjustments and residual-young-entry fallback handling.
5. Compare actual Cargo output to candidate Cargo.lock byte for byte. Different lock, stale input digest, changed snapshot or insufficient cutoff age is exit 1; malformed records or missing resolver/replay material is exit 2. Compare cutoff with the selected workspace HEAD's committer time, never wall time. Temporary results never overwrite authoritative inputs.
6. Process tests in `xtask/tests/lock_replay.rs` run complete small bounded public/synthetic fixture workspaces with actual Cargo in both modes: `lock_replay_generates_the_expected_lock_without_a_candidate_seed`, `lock_replay_uses_only_the_bundle_offline`, `lock_replay_rejects_changed_input_or_lock`, `lock_replay_rejects_invalid_bundle`, `lock_replay_requires_bound_security_approval`, `lock_replay_is_stable_after_time_advances` and `lock_replay_missing_material_is_exit_2`. Assert distinct condition-specific exits. Include a coherent descriptor/hash fixture whose actual resolution differs; hashing alone cannot pass it. Cover each approval binding, complete supported record parsing, duplicate fields, unsafe paths and mutated records. Fixture resolver setup is explicit and separate; missing prerequisites or zero selected tests never become a skipped success.
7. Keep the current skeleton's `just check` green without activating verification against its incomplete production record. The final report retains real resolver/replay receipts; no fake successful replay or precomputed output is a substitute.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test lock_replay
cargo xtask deps --check
git diff --exit-code -- Cargo.toml Cargo.lock .config/tools.toml xtask/Cargo.toml
just check
```

Expected: both actual replay modes, refusal and time-stability cases execute from complete bundles. No production input change or online access occurs. An absent resolver is a blocker, not a waived test.

## Negative control

Replace actual Cargo replay with descriptor/hash validation: the coherent wrong-resolution fixture in `lock_replay_generates_the_expected_lock_without_a_candidate_seed` must fail. Seed replay with the candidate lock or normal Cargo home: the no-seed/bundle-only case must fail. Accept an unsafe archive entry, changed input/lock, absent resolver or missing security binding: its refusal test must fail. Re-evaluate an old record using today's clock/registry: the time-stability/offline case must fail. Restore every plant.

## Rollback

Revert the merge commit. No authoritative manifest, lock or production record changed. Part three remains ineligible.

## BLOCKED

The resolver or complete fixture is unavailable, real offline replay cannot reproduce either mode, approval cannot be bound, or safe bundle handling requires a dependency change before the verifier exists. Preserve evidence and report the condition; do not substitute stored-descriptor comparison or copy the candidate lock into replay.

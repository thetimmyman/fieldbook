# rehearsal-two-build-compare

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

Two isolated builds of each selected executable profile produce a verified comparison manifest for binary analysis and signing.

## Why

- Executable evidence needs exact build identity and matching debug artifacts, not only a source provenance file.

## Scope

- Files: `xtask/src/two_build.rs` (tests in `mod tests`), `tests/fixtures/release/two-build/` (complete synthetic descriptors and PE/PDB artifacts).
- Registration lines: `xtask/src/main.rs` (two-build command); `.github/workflows/release.yml` (two producing jobs per profile and dependent comparison job); `docs/runbooks/release.md` (producer and verifier procedure); `Cargo.toml` (dist debug information); `.cargo/config.toml` (target reproducibility flags).
- Docs to update: `docs/runbooks/release.md`.

## Out of scope

- Signing, publication and IT authorization.

## Preconditions

- Cards merged: `release-workflow`, `net-stack-spike`.

## Steps

1. `cargo xtask two-build produce --profile PROFILE --out DIR` performs exactly one build and writes `build.toml` (build_schema = 1) plus the executable and raw PDB. Profiles silent, net and bridge use the settled Windows MSVC target and feature sets. Two distinct Windows jobs per profile run this producer; a comparison job downloads their separate artifacts and calls `cargo xtask two-build compare --left LEFT --right RIGHT --out DIR`. A scripted Runner tests producer calls. No credential or private path enters fixtures.

2. Write `comparison.toml` with `comparison_schema = 1`, `commit`, `toolchain`, `image`, `target`, `profile`, `features`, `source_date_epoch` and two `[[build]]` rows. Each row contains `number`, `descriptor`, `descriptor_sha256`, package-relative `exe`, `exe_sha256`, `pdb` and `pdb_sha256` (both PDB fields required for this Windows command). Verify every file hash and require executable hashes equal; require byte-identical raw PDBs with no normalization; before comparison verify each PE CodeView GUID and age against its PDB information stream. Unsafe relative paths, missing files, profile drift or unequal executables refuse output.

3. Provide the parser and verifier to analysis and signing. The shared synthetic fixture covers both build rows and PDBs. Tests call the real parsed command handler through its local scripted Runner: equal builds pass; wrong hash, different profile, missing PDB and changed second build fail before publishing the manifest. Later binary-evidence instrumentation must affect both builds before this comparison, never patch a compared file.

4. Record the exact command and measured result in the release archive. A failed comparison leaves existing evidence untouched and prevents analysis/signing; this card does not publish a release.

5. One BuildPlan in xtask/src/two_build.rs supplies dist, locked dependencies, commit-derived SOURCE_DATE_EPOCH and FIELDBOOK_GIT_SHA, set in the Cargo build process’s environment, with the latter equal to the full commit in build.toml, pinned toolchain and windows-2025 image, x86_64-pc-windows-msvc, profile features, explicit debuginfo=2 and split-debuginfo=packed, stable path remaps and linker /Brepro. Preserve config flags; never replace them through RUSTFLAGS. P5 owns these rehearsal settings; R2 owns the later hardening checks. Archive rebuilds use --offline --frozen. Tests assert the exact build command and environment, including retained flags.

6. Expose `read_build(dir) -> Result<BuildArtifact, CompareError>` and `verify_debug_identity(pe, pdb) -> Result<(), CompareError>` from this module. The latter reads bounded PE debug-directory CodeView RSDS data and the MSF/PDB information stream, checking GUID and age, and refuses absent, malformed or multiple inconsistent identities. `compare(left, right)` verifies safe relative paths, hashes and each identity before comparing commit, toolchain, image, target, features, profile, epoch and artifact bytes. Synthetic minimal PE/PDB pairs cover valid, corrupt and mismatched identities. `substituted_pdb_with_updated_hash_is_refused` changes both PDB and descriptor hash and must fail the actual verifier. Analysis/signing consume this verifier unchanged.

7. `independent_windows_jobs_feed_comparison` checks the workflow matrix, two distinct producers per selected profile, artifact separation, dependency edges and compare invocation. Acceptance also requires one successful unsigned two-job rehearsal through the existing PR/manual workflow; record its comparison in the release archive, without tag or publication.

8. Implement the complete producer/comparison schemas and path bases in the release runbook. After verifying LEFT and RIGHT, stage unchanged copies of both full producer packages under OUT/builds/1 and OUT/builds/2. Verify the copied files and publish comparison.toml last; failure preserves existing evidence. `two_build::tests::relocated_comparison_package_verifies` produces the comparison, moves OUT, removes LEFT and RIGHT, and succeeds through the same verifier used by analysis/signing.

9. `two_build::tests::producer_stamps_the_compiled_commit` uses the scripted Runner to assert `FIELDBOOK_GIT_SHA` in every build environment equals `build.toml.commit`. During the required Windows rehearsal, xtask runs each produced executable with `--version` and compares its commit component with that descriptor; a matching pair of unstamped binaries fails. Keep all process execution in xtask.

## Acceptance

```sh
cargo nextest run --locked -p xtask two_build::
just check
```

Expected: the positive comparison passes and each refusal leaves the previous output unchanged; all commands exit 0 after restoring plants.

## Negative control

In `changed_second_build_is_refused`, change valid second-executable bytes outside its debug identity and update that producer descriptor's executable hash. Both producer packages individually verify; metadata and raw PDBs stay identical. Run the real parsed compare handler and require mismatch refusal with previous output preserved. Temporarily bypass only the final cross-build executable-equality decision (both hash and byte comparisons), retaining individual hashes, PE/PDB identity and raw-PDB equality: `cargo nextest run --locked -p xtask changed_second_build_is_refused` must fail because comparison now succeeds. Substitute a PDB: its binding test must fail. Restore each.

Remove a producer dependency or disable debug-identity verification: the workflow test or `substituted_pdb_with_updated_hash_is_refused` must fail. Restore each.

Leave comparison paths bound to the original producer directories: `two_build::tests::relocated_comparison_package_verifies` must fail. Restore it.

Replace `FIELDBOOK_GIT_SHA` with `GIT_SHA` in the build environment, leaving the former absent: `producer_stamps_the_compiled_commit` must fail. Restore it.

## Rollback

Revert landed consumers in reverse dependency order, including transitive consumers, then revert this card’s merge commit. Find closed consumers’ Preconditions in their retained card text in Git history. Keep the previous accepted release and comparison unchanged; discard only the failed new staging output.

## BLOCKED

A selected profile has no approved build route or two builds differ: record the exact gate in the owner’s tracker and stop.

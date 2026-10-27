# r2-binskim-fuzz-pe

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

The exact R2 executable builds have auditable dependency metadata, binary-analysis and import evidence, and bounded fuzz smoke tests. Intake can verify which bytes each result covers.

## Why

- Evidence for a different executable does not establish the properties of the delivered build. A parser panic or unexpected import must fail the evidence gate before signing and delivery.

## Scope

- Files: `xtask/src/release_evidence.rs`, `xtask/tests/release_evidence.rs`, `fuzz/Cargo.toml`, `fuzz/fuzz_targets/{citation.rs,bridge_request.rs}`, `CHANGELOG.md`.
- Registration lines: `.github/workflows/ci.yml` (fuzz smoke job and ci-ok needs), `docs/reference/testing.md` (the landed fuzz jobs); `.config/tools.toml` (cargo-auditable, BinSkim and cargo-fuzz pins/checksums and the separate fuzz toolchain pin); `xtask/src/main.rs` (release-evidence); `.github/workflows/release.yml` (binary evidence after unsigned comparison); `.github/workflows/scheduled.yml` (fuzz job); this card's entries only.; `xtask/src/two_build.rs` and its workflow invocation (cargo-auditable instrumentation in both producing builds before comparison); `.cargo/config.toml` (production Windows hardening while preserving BuildPlan flags); `crates/fieldbook/build.rs`, `crates/fieldbook/resources/fieldbook.rc` and `crates/fieldbook/resources/fieldbook.manifest` (VERSIONINFO and application manifest); `Cargo.toml`, `crates/fieldbook/Cargo.toml`, `Cargo.lock` and `docs/reference/dependencies.md` (embed-resource build dependency under the existing recipe).; `xtask/src/graph.rs` (cc permitted-parent rule and synthetic graph tests only)
- Docs to update: dependency tool rows, release runbook evidence command and CHANGELOG.md.

## Out of scope

- No signing, new network route or shipped process code. A tool failure is not a waiver. The owner reviews any finding disposition through the release process.

## Preconditions

- Cards merged: `release-workflow`, `rehearsal-two-build-compare`, `net-stack-spike`, `chat-bridge`, `check-citations`.

## Steps

1. Add the three tools at the versions in the dependency reference to the pinned tool manifest, with verified distribution hashes and the existing setup/doctor recipe. If a listed version cannot be obtained or its hash fails, report the exact tool/version and stop; do not silently choose another release. Set `[fuzz] toolchain = "nightly-2026-10-08"` separately from the lock pin and record it in the dependency tool table. No tool becomes a runtime dependency.
2. `cargo xtask release-evidence (--unsigned DIR --out DIR | --self-test | --fuzz-smoke | --fuzz-scheduled)` accepts the two-build comparison manifest, the unsigned silent and selected AI executables and matching PDBs. Verify every input hash before running the tools. The pure plan lists each command and expected output; the executor uses xtask's process edge, with a scripted Runner for tests. Build cargo-auditable metadata into both compared unsigned binaries, never patch it in afterward. `--self-test` uses a temporary synthetic package and scripted tools to run the happy path plus wrong-hash, failed-tool and missing-report cases; it prints `release-evidence: self-test ok` and exits 0 only if each behaves as specified. It needs no real executable evidence and writes no release output.
3. On Windows run BinSkim against each exact executable/PDB and the existing PE import scanner with that build's feature profile. Treat tool failure, missing report, analysis error or unexpected import as failure. Retain full reports outside the repository and a summary containing version, commit, tool versions and full input/output hashes. Signing later records the unsigned-to-signed hash mapping; the evidence remains explicitly about the unsigned compared bytes.
4. The citation target feeds arbitrary bytes through the tolerant lexer after lossy UTF-8 conversion. The Bridge target feeds bytes through read_head, admission and read_body over a bounded in-memory reader and deterministic deadline; neither starts a server. Seed from synthetic valid examples and the known-bad cases. `--fuzz-smoke` reads `Pins::get("fuzz", "toolchain")` and runs `cargo +<pin> fuzz run <target> -- -seed=1 -max_total_time=60` once for each target, citation then bridge_request. `--fuzz-scheduled` uses the same plan with 900 seconds per target. Both propagate a nonzero exit and print the target and result; the successful smoke ends `fuzz: 2 targets passed`. The PR fuzz job runs the smoke with a 10-minute timeout and is required by ci-ok. The scheduled job runs the longer mode with a 40-minute timeout. Each job checks out with pinned actions, full history and no persisted credentials, installs the normal pinned Rust toolchain, the separate fuzz toolchain and pinned cargo-fuzz, then invokes this one command. Tokens remain read-only. Corpus discoveries are stored as failing synthetic regression fixtures only after review, with no data admission.
5. Tests in release_evidence cover `wrong_input_hash_stops_before_tools`, `missing_report_fails`, `analysis_error_fails`, `unexpected_import_fails`, `evidence_names_exact_input_bytes`, `sign_mapping_links_evidence_to_delivery`, `tool_failure_keeps_previous_evidence` and `same_plan_same_summary`. CLI integration tests use a scripted tool executable under a temporary directory, never a real credential or endpoint. Add a deliberate panic to each fuzz target's parser branch in a temporary branch and see the seeded smoke fail, then restore it.
6. The owner runs the exact binary evidence on the release candidate, reviews findings and supplies the signed mapping and verified reports to intake. The release workflow fails before signing if evidence is missing. No delivery follows a failed gate.

7. Consume the versioned comparison contract and shared synthetic fixture from `rehearsal-two-build-compare`. Inspect actual embedded cargo-auditable metadata in each compared executable, checking its identity against the profile; a matching but uninstrumented pair must fail the evidence gate. For SG-04 mark the actual hardening refusal test and feed it a deliberately weakened synthetic executable, not merely a prewritten failure report. The real checker must reject it.

8. Name the real weakened-executable check `weakened_binary_fails_hardening_check` and mark it `// control: SG-04`. It runs the actual hardening checker on the normal fixture and a build with a required flag removed. Bypassing that check must fail the test.

9. Before either producing build, install the production hardening configuration required by the release runbook: static CRT and its required compiler/linker flags, VERSIONINFO and the application manifest. Add embed-resource under the dependency recipe and the existing build-script wrapper allowance; run deps --check, cargo deny and lock verification, documenting the selected pin and features. Reuse shared BuildPlan and preserve debuginfo/path-remap/reproducibility settings; never replace configuration via RUSTFLAGS. Analyze the actual normal release executable for each selected Windows profile and verify its required resources, not just synthetic checker inputs. `weakened_binary_fails_hardening_check` marks SG-04: the normal built fixture passes, removing one required setting produces a real checker rejection, and bypassing that rejection makes the test fail.

10. When embed-resource introduces cc, remove cc from graph.rs’s unconditional BANNED list and add it to the WRAPPED table installed by net-stack-spike, permitting only parent embed-resource. Preserve normal/build dependency traversal and leave deny.toml’s existing wrapper allowance unchanged. `cc_via_embed_resource_is_allowed` accepts the intended graph; `cc_under_another_parent_is_rejected` rejects another parent alone and also alongside the allowed parent.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test release_evidence
cargo xtask release-evidence --self-test
cargo xtask release-evidence --fuzz-smoke
cargo nextest run --locked -p xtask graph::
just check
```

Expected: self-test prints `release-evidence: self-test ok`, smoke prints `fuzz: 2 targets passed`, and each command exits 0; the named positive cases pass and the planted negative cases fail before restoration.

## Negative control

Ignore a scanner exit code: `analysis_error_fails` must fail. Substitute another binary after hashing: `wrong_input_hash_stops_before_tools` must fail. Plant the two parser panics and see the bounded seeded smoke fail. Restore each before the final check.

Bypass the real weakened-build hardening rejection: `weakened_binary_fails_hardening_check` must fail. Restore it.

Remove cc’s parent restriction: `cc_under_another_parent_is_rejected` must fail. Restore it.

## Rollback

Keep the prior archived accepted executable. Revert the new evidence job after saving failed reports outside the repository; no data or deployed executable is rewritten.

## BLOCKED

A required tool or exact-build report is absent, a negative control does not fail, or an import/analysis finding has no owner disposition: record the concrete release gate in the owner's tracker and stop.

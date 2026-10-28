# r2-intake-pack

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

cargo xtask intake assembles and verifies a separately manifested companion containing the 17 artifacts for the exact approved release, without inventing an IT approval or copying company material into the repository.

## Why

- IT must review evidence for the bytes it receives ([the intake question set](../../intake/README.md)); [R2 step 5](../../runbooks/release.md#r2-step-5-hand-over-to-it-owner) is where the pack changes hands. A placeholder or evidence from another build can otherwise appear complete.

## Scope

- Files: `xtask/src/intake.rs`, `xtask/tests/intake.rs`, `tests/fixtures/intake/`, `CHANGELOG.md`.
- Registration lines: `xtask/src/main.rs` (intake); `docs/intake/README.md` (generated artifact paths only).
- Docs to update: intake README, release runbook command and CHANGELOG.md.

## Out of scope

- No change to IT answers, policy permissions or controls evidence. An agent cannot complete artifact 17 on behalf of IT.
- No deployment or sending of the pack.

## Preconditions

- Cards merged: `r2-sign-workflow`, `r2-binskim-fuzz-pe`, `ai-it-story`, `it-company-story`.

## Steps

1. `intake --release DIR --evidence FILE --out DIR [--draft]` reads the verified final executable-package manifest and signed catalog, the fixed 17-row intake inventory and evidence paths supplied outside the repository. Refuse a nonempty output or an output inside the verified package. The intake companion is never a member of that package's checksums or catalog. One typed ARTIFACTS table records id, relative output path, source and required checks; it must have exactly the ids 1 through 17, with no duplicate path or unknown id.
2. For each row, bind version, commit, build profile and payload hashes to the final delivered set. The signed-binary profile, SBOM, vulnerability disposition, network/import capture, code analysis and install diff must name that same build. Copy evidence without rewriting it. Templates may be filled with public build metadata only; missing fields or placeholders are errors. Authorization is either the owner's supplied IT-completed record or an explicit pending record. Pending is permitted only with --draft, watermarks the manifest and exits 1; it never satisfies the release gate.
3. Write `manifest.toml`, a sorted manifest of artifact ids, paths, sizes and full hashes last, binding the final package payload hashes and the hashes of its checksum list and signed catalog. The companion manifest does not hash itself; the complete release archive hashes it. Reopen every output and verify its hash. Fingerprint the manifest and print only that fingerprint and artifact counts. Company deployment evidence stays in the supplied output location; the repository's fixture is synthetic.
4. Tests `all_seventeen_artifacts_match_release`, `missing_artifact_fails`, `wrong_build_evidence_fails`, `companion_binds_final_package`, `companion_output_inside_package_is_refused`, `placeholder_fails`, `pending_authorization_cannot_complete`, `changed_copied_evidence_fails`, `same_inputs_same_manifest` and `private_evidence_is_not_written_to_checkout` use synthetic fixtures. The last test passes an output under a git ancestor with nonpublic evidence and requires refusal before writing.
5. The owner reviews the generated pack against the final release. Then archive-release copies the verified package, edition/originals pair and intake companion together; verify-archive checks the copy and rollback rehearsal before delivery. Record the fingerprint and verification in the owner's tracker. Handover uses the single release runbook procedure. A regenerated pack with a different fingerprint is reviewed again before delivery.

6. `--evidence FILE` reads a versioned TOML descriptor: `evidence_schema = 1`, release `commit`, `signing_manifest_sha256`, and exactly seventeen `[[artifact]]` rows with `id`, `classification`, `authorization_state` (`pending` or `supplied`), optional `authorization_record`, applicable `profiles`, and `[[artifact.files]]` entries with source `path`, intended relative `output` and `sha256`. Relative source paths resolve beside the descriptor; each artifact may contain multiple files. Authorization evidence is supplied for review; a descriptor flag authenticates no approval. VEX rows carry `applicable` and the advisory identifiers; an absent VEX file is allowed only when no advisory requires it. Executable evidence binds through the signing manifest from unsigned analysis to final signed hashes. Validate hashes, identities, profile bindings and output placement before writes; nonpublic evidence may never be written under a Git ancestor. Draft mode labels pending authorization; delivery mode refuses it.

7. A complete synthetic descriptor covers all seventeen IDs, multi-file artifacts and a VEX-not-applicable case. Run the real intake command against it and mutations for wrong build, missing constituent, bad hash, pending authorization, missing applicable VEX and nonpublic Git output. Require refusal before any output for the latter; retain valid signed/unsigned bindings. Skipping a binding check must fail its mutation.

8. Read FIELDBOOK_SECURITY_CONTACT from the environment when filling the contact-route template. Local runs supply it; CI packaging callers forward the owner-provisioned same-named repository secret. Missing or blank input is IN-01 before output; tests use only synthetic text and cover both failures and a filled template. Reuse the edition packaging input contract; never print or commit the real value.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test intake
cargo xtask cards --check
just check
```

Expected: each command exits 0; the named positive cases pass and the planted negative cases fail before restoration.

## Negative control

Allow missing authorization or wrong-build evidence to complete: the matching test must fail. Alter a copied byte after writing its hash: changed_copied_evidence_fails must fail. Restore each.

Bind the companion to another package, or permit its output inside the signed package: the corresponding companion test must fail. Restore each.

Omit the contact variable or accept blank contact text: the intake input tests must fail. Restore each.

## Rollback

Retain the previous accepted intake pack and release. Remove only the new output folder after preserving failed evidence outside the repository. No deployed state changes.

## BLOCKED

A required artifact or IT approval is absent, or evidence does not match the final signed bytes: record the missing artifact in the owner's tracker and stop. Never fill the approval with an agent assertion.

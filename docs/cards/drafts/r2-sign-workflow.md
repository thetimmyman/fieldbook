# r2-sign-workflow

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

A tag-approved unsigned release becomes a verified signed package with checksums for the final executable bytes, ready for intake assembly and the complete release archive.

## Why

- Signing changes bytes. Checksums made before signing cannot describe the delivered executable.
- A failed signing or checksum step must prevent intake assembly; the completed release still needs the durable archive before delivery.

## Scope

- Files: `.github/workflows/sign.yml`, `xtask/src/sign_release.rs`, `xtask/tests/sign_release.rs`, `CHANGELOG.md`.
- Registration lines: `xtask/src/main.rs` (sign-release commands); `docs/runbooks/release.md` (command spellings only).
- Docs to update: release runbook and CHANGELOG.md.

## Out of scope

- No new signer, secret, permission or certificate policy is chosen by the agent. IT and the owner supply the approved route through the existing credential process.
- No tag creation or automatic publication.

## Preconditions

- Cards merged: `release-workflow`, `r2-migrate`, `r2-binskim-fuzz-pe`.

## Steps

1. Follow the ordered R2 signing and durable archive procedures in the release runbook. `sign-release --input DIR --output DIR` accepts only a new output directory and verified unsigned manifests from two matching builds. Its pure planner returns ordered SignExe, WriteSigningManifest, HashPayload, MakeCatalog, SignCatalog, Package and VerifyDelivered steps. Intake and the complete archive follow this verified package; neither is an input to signing. Process execution uses the existing Runner in xtask only.
2. The workflow uses the approved signing environment with its owner gate and tag restriction, read-only repository token and SHA-pinned actions. Fetch the approved unsigned artifacts by the exact tag commit and verify their manifests. A PR runs planner tests with a scripted signer only; it has no signing credentials or environment approval.
3. Sign every selected executable, write the signing manifest described below, then regenerate SHA256SUMS over final executable payload excluding itself, catalog, outer archive and intake companion. Reject an intake path in the signing payload plan, since intake binds these final signatures and hashes. Build the catalog over payload plus SHA256SUMS, sign it, then package. Verify the extracted exact delivery with signatures, hashes and catalog; no post-verification mutation is permitted.
4. `sign_release` tests use a scripted signer that changes executable bytes: `final_checksums_follow_signing`, `catalog_covers_final_manifest`, `unsigned_mismatch_stops_before_sign`, `missing_timestamp_refuses_delivery`, `changed_delivered_byte_fails`, `intake_is_not_in_the_signed_payload` and `sign_failure_leaves_previous_archive_intact`. A real owner-run trial signs synthetic binaries through the approved route and verifies them on a standard-user Windows environment before release use.
5. Keep the exact verified package immutable and supply its manifest/catalog to the intake step. This card performs no handover. After intake assembly, the release runbook archives and verifies the complete set, including the previous accepted release (or its explicit first-release baseline), before the owner hands it over. The archive command and its delivery gate already come from release-workflow.

6. `signing.toml` has `signing_schema = 1`, release `commit` and `[[profile]]` rows with `name`, exact `features`, `comparison_sha256`, `unsigned_exe_sha256`, `signed_exe_sha256`, `pdb_sha256` and `[[profile.reports]]` path/hash entries. After signing, verify these against comparison and analysis inputs, then include the manifest in final checksums and the signed catalog. Keep PDBs separate. Scripted-signer tests prove real command output, refuse substituted unsigned evidence and detect changed signed bytes. Intake verifies this complete chain after signing and stays outside the payload. Mark `changed_delivered_byte_fails` for SG-02; add and mark `non_owner_dispatch_is_refused` for SG-03 through the actual workflow actor check. Disabling that check must fail.

## Acceptance

```sh
cargo nextest run --locked -p xtask --test sign_release --test archive_release
cargo xtask cards --check
just check
```

Expected: each command exits 0; the named positive cases pass and the planted negative cases fail before restoration.

## Negative control

Move HashPayload before SignExe: final_checksums_follow_signing must fail. Skip package verification: `changed_delivered_byte_fails` must fail. Include the intake companion in the signed payload: `intake_is_not_in_the_signed_payload` must fail. Restore each.

## Rollback

Keep the previous archived signed set deployed. Discard the failed new staging folder only; never replace a deployed executable with an unsigned build. Revert the workflow after preserving its evidence.

## BLOCKED

The approved signer/environment or independent archive location is absent, or a signature/catalog/hash check fails: record the specific release gate in the owner's tracker and stop. Never add a permission or bypass approval to make the run pass.

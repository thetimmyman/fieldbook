# edition-zips

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

`cargo xtask edition` (and `just edition`) checks and builds the Corpus at the current commit and writes `out/dist/fieldbook-edition-NNN.zip`, `out/dist/fieldbook-originals-NNN.zip` and the out-of-band `out/dist/SHA256SUMS`, with byte-identical deterministic payloads for the same commit. The browser summary and transfer statement carry measured evidence and are compared separately under the payload comparison rules below. A refusal names its code or control. `just edition` runs `check`, then one stamped build–harness–package operation, so its summary measures the tree it ships.

## Why

- The two zips, their top folders and their contents are [Repository layout](../decisions/0001-foundation.md#repository-layout). Edition ids and numbering are [Identifiers](../reference/corpus-format.md#identifiers) and [Change records](../reference/corpus-format.md#change-records). `edition.toml` and its fingerprint are [Scope and versioning](../reference/corpus-format.md#scope-and-versioning); `build` writes the file, this card ships it.
- The allowlist, path and integrity checks are FB0601, FB0602 and FB0608 in [06: site and edition](../reference/corpus-format.md#06-site-and-edition), controls ED-01 and ED-03; byte-identical output is DT-01 ([control register](../controls.md)). The release side is [AD-08](../decisions/0001-foundation.md#ad-08-ci-design) and [AD-20](../decisions/0001-foundation.md#ad-20-signing-release-pipeline-and-versioning).
- The Tier-0 intake pack, the five source files, their placeholders and the `SHA256SUMS` format are the [intake README](../intake/README.md) ("The 17 artifacts" and "The Tier-0 files and their formats"), [AD-19](../decisions/0001-foundation.md#ad-19-intake-pack-staging) and control IN-01. README.txt carries the notes MW-01 and DR-01 require.
- Design: the [MAP.md](../design/MAP.md) row "About", for the edition data in `edition.toml` that `about.html` shows. [HANDOFF.md](../design/HANDOFF.md) [§2 Routes](../design/HANDOFF.md#routes) (About). No template or style changes. No token group.
- Review is `owner` because this card edits an IT-facing statement under `docs/intake/` ([the three header fields](README.md#the-three-header-fields)).

## Scope

- Files: `xtask/src/{main.rs,edition.rs}` (the tests are in `mod tests` of `edition.rs`), `xtask/Cargo.toml` and `Cargo.lock` (only xtask's `dependencies` list: the `zip` edge from `day1-dependencies-3` and a path edge to `fieldbook-core` for hashing), `justfile`, `docs/intake/edition-readme.txt` (new: the short laptop steps from [transfer.md](../runbooks/transfer.md), and the backup paragraph of step 2), `CHANGELOG.md`.
- Registration lines: the Commands line of `AGENTS.md` (`just edition`); the `Needs` line of Drill 1 in `docs/runbooks/restore-drill.md`; `tests/known-bad/elsewhere.toml`, whose FB0601, FB0602 and FB0608 rows get their `test` key in step 7 (`check-known-bad` wrote the rows).; `xtask/src/allowlist.rs` (Site-profile required paths); shared manifest and zip helpers are consumed unchanged; `docs/intake/README.md` (security-contact input); `SECURITY.md` (packaging input).
- Docs to update: `AGENTS.md` (Commands: `just edition`), `docs/runbooks/restore-drill.md` (Drill 1 needs Microsoft Edge, because `just edition` runs the Edge harness), the justfile header comment (drop `edition` from the recipes still to come), `CHANGELOG.md` (`- Added: just edition builds the edition and originals zips.` under `## Unreleased`).

## Out of scope

- `release.yml` and the tag checks (`release-workflow`); `data/registers.xlsx` and the CSV copies (`registers-xlsx`); the Excel templates (E1).
- Writing the network-silence summary. The Edge harness (`edge-harness`) produces it, and this command invokes it after the stamped build; the harness remains the only summary writer.

## Preconditions

- Cards merged: `new-verbs`, `build-about`, `edition-manifest`, `edge-harness` (the `browser` recipe, which writes `out/harness/network-silence-summary.txt`).

## Steps

1. `edition.rs`: refuse a dirty tree. Take the edition id from the newest `## edition-NNN` heading of `corpus/changes.md`, `SOURCE_DATE_EPOCH` from the commit time and `FIELDBOOK_GIT_SHA` from `HEAD`. Run `fieldbook check --strict` with the commit date as today, then `cargo run --locked -p fieldbook -- build --out out` with both variables set (the program reads `FIELDBOOK_GIT_SHA` when it is compiled, with `option_env!`, so it must be in the environment of that command; `build` replaces `out/edition` and `out/originals` whole). Any finding stops the run with the check's exit code. Remove any previous harness summary before invoking the harness against these completed roots. Require a successful run and its twelve-line summary, matching edition identity and the captured site/originals file hashes. The harness instruments a separate copy. Do not rebuild or change site/original content between measurement and packaging; only the declared packaging companions are added afterward.
2. Stage `out/edition/`, which `build` has just replaced with the site, by adding the Corpus files that `shipped_corpus(tree, quarantined)` lists (no other listing of `corpus/` is used, so a held version is never staged), `ASSISTANT.md`, `README.txt` from `docs/intake/edition-readme.txt`, `TRANSFER-STATEMENT.txt` from `docs/intake/transfer-statement.txt`, and `intake/` with the four Tier-0 statements and `network-silence-summary.txt` read from `out/harness/`. Read `FIELDBOOK_SECURITY_CONTACT` from the invoking environment, rejecting absent or whitespace-only input with IN-01. Fill every placeholder from the table in the intake README; counts come from the staged tree. A missing input or an unfilled `{placeholder}` stops the run, naming IN-01 and the file. `README.txt` carries this backup paragraph, in plain text at 72 columns:

   ```
   BACK UP YOUR WORK
   Your working copies live in Fieldbook\working\. It is the only folder
   that cannot be rebuilt: Fieldbook\edition\ and Fieldbook\originals\
   come from the two zips. Copy working\ to the place your IT policy
   covers after each piece of work and before each new edition. The
   commands, with a hash comparison that uses Get-FileHash, are in the
   transfer runbook under "Back up working". This edition makes no backup
   of its own. After you approve a decision package, use Save As to keep a dated copy of the approved workbook alongside the working copy.
   ```

   `README.txt` also carries this Mark-of-the-Web paragraph, in plain text at 72 columns. The drill of `transfer-and-windows-drill` looks for it by its first line (MW-01):

   ```
   MARK-OF-THE-WEB
   Files that arrive through the approved channel, and files taken
   out of the zips, may carry a Windows marker, the Mark-of-the-Web.
   It is normal and it is IT's control. Leave it in place: do not
   run Unblock-File, do not use the Unblock box in a file's
   Properties, and do not delete the stream. Excel may open a
   workbook in Protected View; that is Excel's own behaviour. The
   transfer runbook explains it under "Mark-of-the-Web".
   ```

   The backup state of the program (`.fieldbook/`) and the `inbox/` folder are never staged.
3. Reuse `edition-manifest`’s `allowlist.rs` with the Site profile: walk both staged trees (`out/edition` and `out/originals`) and raise FB0601 and FB0602 with the path. A path of `REQUIRED_FILES` that the staged edition lacks is FB0601 with the path. `REQUIRED_FILES` is `const REQUIRED_FILES: &[&str]` in `xtask/src/edition.rs`, paths relative to the edition root: `index.html`, `about.html`, `corpus/corpus.toml`, `ASSISTANT.md`, `README.txt`, `TRANSFER-STATEMENT.txt`, `edition.toml`, `intake/ai-development-statement.md`, `intake/data-handling.md`, `intake/integrity-guide.md`, `intake/network-silence.md` and `intake/network-silence-summary.txt`; a later card that ships a fixed file appends its path (`registers-xlsx` appends the six files of `data/`). A version whose `lifecycle` is `quarantined` ships its row, its hash and its reason (in `note`) and nothing else: an original, a folder under `corpus/sources/text/<version-id>/`, an `s/<version-id>.html` page at all (a held version has no page, no link and no search unit) or a search unit that names it raises FB0601 with the path and the version id. The reference's lifecycle table says what a withdrawn version ships. Then write each tree's `SHA256SUMS` with `manifest::write_sums`, exported from `fieldbook-core`; `backup-snapshot` calls the same function, so there is one implementation. Recompute the fingerprint over the staged `corpus/` with `manifest::shipped_corpus` and `manifest::fingerprint` (both from `edition-manifest`, the functions `build` calls to fill `edition.toml`, so there is one rule) and compare it with `edition.toml`, then compare the extraction hashes and every listed file (FB0608).
4. Reuse `edition-manifest`’s `zipdet.rs`: write each zip with entries sorted by path as bytes, the time from `SOURCE_DATE_EPOCH`, fixed permissions and deflate only, under the top folders `edition/` and `originals/`. Then write `out/dist/SHA256SUMS` with the two zip lines. Print one line for each zip with its file name and its size in whole megabytes, such as `fieldbook-originals-001.zip 41 MB` (a zip under 1 MB prints `<1 MB`), so that the owner sees the size before the upload.
5. Add `edition` to `main.rs` and the `edition` recipe to the justfile: `edition: check`, with the body `cargo xtask edition`, which owns stamped build, harness and packaging in that order.
6. Tests in `mod tests` of `xtask/src/edition.rs` (`xtask` is a binary crate, so a test of one of its functions is in the module it tests), on a temporary copy of the fixture with a synthetic `## edition-001` heading and a synthetic summary passed through the function's input: `two_runs_give_identical_zips` (the first build runs in the test; the second runs in a child process, the test binary started again through `std::env::current_exe()` with `--exact edition::tests::build_in_child --ignored`, with `TZ=Pacific/Auckland` and another working directory; the `#[ignore]` test `build_in_child` reads its fixture and output folders from the environment and does nothing without them; the two zips and the two `SHA256SUMS` are byte for byte the same), `zip_top_folders`, `two_summaries_preserve_the_deterministic_payload` (build twice from one commit with two summary files whose `browser:` lines differ: each zip passes its full SHA256SUMS verification; the summary, TRANSFER-STATEMENT.txt (which names the browser) and their derived manifest/hash entries may differ, and every deterministic payload entry is byte for byte the same), `placeholders_all_filled`, `missing_summary_names_in_01`, `missing_required_file_is_fb0601` (delete `ASSISTANT.md` from the staged tree: FB0601 names it), `missing_intake_file_is_fb0601` (delete `intake/data-handling.md` from the staged tree: FB0601 names it), `planted_ps1_is_fb0601`, `long_path_is_fb0602`, `changed_file_is_fb0608`, `renderable_original_stays_txt`, `state_never_ships` (a fixture `.fieldbook/` and `inbox/` beside the fixture Corpus never reach either zip, and a plant of either in the staged tree is FB0601), `held_version_absent` (a copy of the fixture with one version set to `quarantined`, with a reason in `note` and no verified claim on it, ships the row and hash in `sources.csv` and no original, extraction text, `s/` page or search unit for it; a planted original of that version in the staged tree is FB0601, and so is a planted bare `s/<version-id>.html`, an empty file being enough). Add `zip_sizes_are_printed`: the run prints one line `<zip name> <n> MB` for each of the two zips, and a zip of 1,500,000 bytes prints `2 MB`.
7. In `tests/known-bad/elsewhere.toml`, set the `test` key of the FB0601 row to `planted_ps1_is_fb0601`, of the FB0602 row to `long_path_is_fb0602` and of the FB0608 row to `changed_file_is_fb0608`. Add and delete no row: `check-known-bad` wrote them.
8. Mark the tests that carry a control row of [the controls table](../controls.md) with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: `// control: ED-01` on `renderable_original_stays_txt` and on `planted_ps1_is_fb0601`, `// control: IN-01` on `placeholders_all_filled` and on `missing_intake_file_is_fb0601`, and `// control: DT-01` on `two_runs_give_identical_zips`.

9. Define `ATTESTATION_PATHS` once: intake/network-silence-summary.txt and TRANSFER-STATEMENT.txt. `compare_payload` first verifies every full manifest against its own staged tree, then compares all other paths and bytes, ignoring only these two payloads and their derived checksum rows. Missing, extra or changed deterministic files fail. Tests in `mod tests` of `xtask/src/edition.rs`: `compare_payload_accepts_different_browser_versions`, `compare_payload_detects_changed_deterministic_file`, `compare_payload_rejects_corrupted_manifest` and `compare_payload_rejects_missing_attestation`. Export this comparison for release rehearsal; no second exclusion list.

10. Expose `cargo xtask edition-compare --left DIR --right DIR` over compare_payload for the release runbook. It verifies the two zip sets and full internal manifests before comparison; tests in the same module include `edition_compare_accepts_differing_browser_attestations` and `edition_compare_rejects_changed_content_byte`.

11. When the control owner regenerates DT-01, name `two_summaries_preserve_the_deterministic_payload` and `compare_payload_detects_changed_deterministic_file`. Validate full packages and manifests first; the only excluded payloads are the shared two attestation paths and their derived checksum differences, never whole manifests.

12. Use a scripted build/harness runner for `packaged_site_is_the_harness_input` and `stale_harness_summary_is_refused`: compare every measured site/original byte with the packaged counterpart; a failed harness with an old passing summary present produces no package. A changed measured file or mismatched edition identity also refuses. Tests use successful synthetic summaries through the runner, not an unchecked file shortcut.

13. Extend `placeholders_all_filled` with synthetic FIELDBOOK_SECURITY_CONTACT input, absent/empty/whitespace refusal and exact contact substitution. Production values stay out of committed files. Workflow callers bind the owner-provisioned same-named secret only to packaging; their workflow tests prove the binding. Local Acceptance uses an environment value supplied outside the repository.

## Acceptance

```sh
cargo nextest run --locked -p xtask edition::
just edition; echo "exit $?"
just check
```

Expected: the tests pass; `just edition` runs `check`, the site build and the Edge harness, then either builds the zips (`exit 0`, one size line for each zip, and `out/dist/` holds the two zips and `SHA256SUMS`) or, while `corpus/changes.md` has no `## edition-NNN` heading, stops with `exit 1` and a line that names the missing heading; `just check` exits 0.

## Negative control

Remove `ps1` from the refused extensions in `allowlist.rs`. `cargo nextest run --locked -p xtask edition::tests::planted_ps1_is_fb0601` must fail and show that no FB0601 was raised. Restore the file. Then make `allowlist.rs` skip the lifecycle rule: `held_version_absent` must fail. Restore it. Then drop the size line of the originals zip: `zip_sizes_are_printed` must fail. Restore it. Then make `zipdet.rs` write the entry times in the local time zone instead of UTC: `two_runs_give_identical_zips` must fail, because its second build is a child process under `TZ=Pacific/Auckland` and another working directory. Restore it.

Compare the whole manifest across different browser summaries, or ignore a deterministic file: the corresponding comparison test must fail. Restore each.

Compare different attestation bytes as deterministic content: `compare_payload_accepts_different_browser_versions` must fail. Ignore a deterministic file: `compare_payload_detects_changed_deterministic_file` must fail. Restore each.

Rebuild after the harness or reuse an old passing summary after a failed run: `packaged_site_is_the_harness_input` or `stale_harness_summary_is_refused` must fail. Restore each.

## Rollback

Revert the merge commit. `just edition` disappears; no edition has been tagged from it before R1.

## BLOCKED

`fieldbook build` cannot take `SOURCE_DATE_EPOCH` and `FIELDBOOK_GIT_SHA` from the environment of the process xtask starts, or does not write `edition.toml`: set the item to Blocked with a `BLOCKED:` comment that says so, and stop. If `just browser` exits 3 with `Edge not found`, the machine that runs the Acceptance has no Edge: say so in the `BLOCKED:` comment and stop.

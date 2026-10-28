# Release runbook

Fieldbook ships on two streams that never mix:

| Stream | Tag | What it carries | Notes live in |
|---|---|---|---|
| Tool release | `vX.Y.Z` | The `fieldbook` program | `CHANGELOG.md` |
| Edition | `edition-NNN` | The Corpus, built into two zips | `corpus/changes.md` |

An edition never changes the exe, so an approved exe stays approved. The tool follows SemVer below 1.0 until R2 (`v0.1.0` at R1, `v0.2.0` with Handoff and `ask`, `v0.3.x` with `chat`, then `v1.0.0` at R2). Why the streams are separate: AD-20 in [ADR 0001](../decisions/0001-foundation.md).

## Who does what

| Action | Who |
|---|---|
| Choose the version or edition number; review and merge release PRs | Owner |
| Prepare a release PR with `just release` | Codex, on a card the owner opens |
| Draft the `corpus/changes.md` section | Lead, or Codex for upkeep; the owner reviews it |
| Create and push a tag | Owner only (the release gate). Codex and CI never push tags |
| Check the tag, build, write the files | CI (`release.yml`) |
| Dispatch signing (R2) | Owner |

## Before any release

Run these on the machine that will tag, from the repository root.

1. **Be on current `main` with nothing uncommitted.**

   ```sh
   git switch main && git pull --ff-only && git status --short
   ```

   Expected: no output from `git status --short`. If it prints files, commit or discard them first; never tag from a dirty tree.

2. **Check the tools.**

   ```sh
   just doctor
   ```

   Expected: exit code 0. If a tool is missing or too old it names the tool and prints a PATH hint; fix that, then rerun. (`just setup` installs the pinned tools.)

3. **Run the full check.**

   ```sh
   just check
   ```

   Expected: exit code 0. If it fails, nothing is released. Fix the cause on a card.

A green `just check` is not readiness. It shows that the code and the files pass their checks, and nothing more. Readiness is four separate things, each with its own evidence: the content is right and usable (the home gate items 2 to 4 below), the owner has accepted the result (the briefs, and the traced decision package from Day 90), the restore drills pass (edition step 4; drill 4 from Day 1; drill 3 from E1; drill 5 from R2), and IT has authorised the tier (intake artifact 17). Do not tag on a green `just check` alone.

## Tool release (`vX.Y.Z`)

Steps 1, 2 and 7 are Codex's, on a card the owner opens; steps 3 to 6 are the owner's. `just release` arrives with the card `release-workflow`.

### Step 1. Prepare the release commit (Codex)

On a branch named for the card (for example `card/release-0-2-0`):

```sh
just release 0.2.0
```

What it does, in order: it refuses to run on `main`, on a dirty tree, with a version that is not higher than the workspace version, or with nothing under `## Unreleased` in `CHANGELOG.md`; it runs `just check`; it sets the workspace version in `Cargo.toml` and the matching entries in `Cargo.lock`; it turns `## Unreleased` into `## 0.2.0` (no date; the tag carries the time) and opens a new empty `## Unreleased` above it. It then runs `just lock`, reviews resolution under the dependency recipe, runs `just lock-verify` and `just check` on the resulting tree. It does not commit, tag or push.

Expected: exit code 0, and `git status --short` lists the release files (`Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`) and only the replay updates under `.config/tools.toml` and `.config/lock-inputs/`. Review the full dependency/replay diff before staging.

If it fails: it prints the first refusal. An empty `## Unreleased` means the cards since the last release left no changelog lines; write them from the merged cards' titles, have the owner review them, then rerun.

### Step 2. Open the pull request (Codex)

```sh
git add Cargo.toml Cargo.lock CHANGELOG.md .config/tools.toml .config/lock-inputs/
git commit -m "release: 0.2.0"
git push -u origin HEAD
```

Expected: the pull request reaches a green `ci-ok`. The card's Review field is `owner`, so Codex does not merge it.

If `ci-ok` is red, fix the cause in the same branch. Do not edit the version by hand to make a check pass.

### Step 3. Merge (owner)

Review the complete release and replay-input diff, including newly retained files, then merge when `ci-ok` is green.

Expected: `main` now holds the release commit.

### Step 4. Confirm the version on the merge commit (owner)

```sh
git switch main && git pull --ff-only
cargo pkgid -p fieldbook | sed 's/.*[#@]//'
```

Expected: prints the version just released, `0.2.0`. This is the tag check done by hand; CI repeats it.

If it prints another version, the merge is not the release commit. Do not tag; ask Codex for a corrected release PR.

### Step 5. Tag and push (owner)

```sh
git tag -a v0.2.0 -m "fieldbook 0.2.0"
git push origin v0.2.0
```

Push the one tag by name, never `--tags`. Expected: the push succeeds and `release.yml` starts.

If the push is refused, read the message; do not force. A tag that was pushed to the wrong commit and has not been downloaded can be removed with `git push --delete origin v0.2.0` and `git tag -d v0.2.0`, then redone from step 4. If anyone has downloaded it, release the next version instead.

### Step 6. Watch the run (owner)

```sh
gh run list --workflow release.yml --limit 1
gh run watch <run-id> --exit-status
```

Expected: the run succeeds, and the repository's Actions page lists it under `release.yml` with the same run id. Its `tag-check` job refuses a tag that is not on `main` or whose version differs from the workspace version. Its `binary` job rebuilds with the `dist` profile and uploads one artifact named `fieldbook-vX.Y.Z-linux` (`X.Y.Z` is the workspace version in `Cargo.toml`, which `tag-check` has proved equal to the tag) holding the Linux binary and its `SHA256SUMS`. Its `edition` job shows as skipped, because the tag is not an `edition-` tag. `release.yml` also runs on a pull request that changes the workflow and when started by hand from the Actions page. In those runs `tag-check` and `edition` show as skipped and `binary` builds and uploads the artifact under the same name, from the workspace version; a skipped job there is not a failure, and nothing is released, because no tag exists. Treat a build uploaded by such a run as a trial and never hand it to anyone.

If the `tag-check` job fails, delete the tag as in step 5 and start again from step 4. If it fails anywhere else, open a card with the run id; do not retag the same version over a failed run that uploaded files.

### Step 7. Record it (Codex)

After the run succeeds, post the tag and the run id as a comment on the card's work item, and set the item Done. The repository holds no status.

## Edition (`edition-NNN`)

Before local packaging, set `FIELDBOOK_SECURITY_CONTACT` in the invoking environment to the owner-provided contact route; never put the value in a committed file. The owner provisions the same-named repository secret for CI packaging callers. Missing or blank values fail IN-01; tests supply synthetic text. This input also applies to raw editions and intake packaging.

`just edition` arrives with the card `edition-zips` and `release.yml` with the card `release-workflow`. Editions follow the cadence in ADR 0001 (D-17): when content changes, no more often than every two weeks until E1, then as the transfer channel allows.

### Step 1. Merge the content (lead or Codex; owner reviews)

Every content change reaches `main` by pull request with the owner's review. Codex does upkeep only (AD-21); it never imports content and never raises a status or moves a version's `lifecycle` back to `active`.

### Step 2. Write the edition's notes (lead or Codex; owner reviews)

Add a section at the top of `corpus/changes.md`:

```markdown
## edition-NNN

- Added: ...
- Changed: ...
- Verified: ...
```

Use the next number, three digits. No dates. The newest `## edition-NNN` heading names the edition. For a Site release, remove the previous `edition-release.toml` or replace it with exactly `release_schema = 1`, the new `edition = "NNN"` and `kind = "site"`. Keeping the previous raw descriptor fails validation or selects raw; changing only its number does not select Site. Merge by pull request.

### Step 3. Build the edition locally (owner)

```sh
just edition
```

Expected: exit code 0, and `out/dist/` holds `fieldbook-edition-NNN.zip`, `fieldbook-originals-NNN.zip` and `SHA256SUMS` (the two zips' hashes). The edition allowlist (AD-18) passes. `just edition` runs `check`, then one operation stamps and builds the site, runs the Edge harness on that exact completed tree, and packages its unchanged content with the new summary in `out/harness/network-silence-summary.txt`; the machine needs Edge, and without it the harness exits 3 with `Edge not found`.

If the allowlist fails it names the file: an exe, dll, script, macro workbook, remote URL, or an `.html`, `.htm`, `.svg` or `.xhtml` file outside the site. Remove the cause in the Corpus or the renderer; never relax the allowlist to pass. If `just edition` reports a missing `## edition-NNN` heading, step 2 was skipped.

### Step 4. Run the restore drill (owner)

Before E1, run the home drill in [restore-drill.md](restore-drill.md#drill-1-the-home-drill-on-linux-day-1) on these zips. From E1, also dispatch the Windows drill (drill 2) on the commit to be tagged and wait for green.

Also run the [backup drill](restore-drill.md#drill-4-the-backup-drill-on-linux-day-1) (drill 4) in your working copy.

Expected: every step passes. If one fails, fix the cause and rebuild from step 3; do not tag.

### Step 5. Tag and push (owner)

```sh
git tag -a edition-NNN -m "edition NNN"
git push origin edition-NNN
```

Expected: the push succeeds and `release.yml` starts. Failure handling is as in tool step 5.

### Step 6. Watch the run (owner)

```sh
gh run list --workflow release.yml --limit 1
gh run watch <run-id> --exit-status
```

Expected: success. The `tag-check` job refuses a tag that is not on `main` or whose number differs from the newest `corpus/changes.md` heading. The `edition` job builds the site, runs the Edge harness (Edge is preinstalled on the runner image), which writes the network-silence summary, builds the edition on Linux and uploads one artifact named `edition-NNN` with the two zips and `SHA256SUMS`, and its log has one line for each zip with its size in whole megabytes (compare them with the channel limit, as in the transfer runbook, step 3). The `binary` job shows as skipped on an edition tag. The `edition` job runs only on an `edition-` tag: it needs the `## edition-NNN` heading, which a pull request does not have, so a trial run on a pull request or by hand does not build an edition. On release tags the `windows` job also runs its full run, which includes the Windows drill (from E1).

If it fails, read the log, fix on a card, and tag the next number. Do not move or reuse a tag.

### Step 7. Fetch the files and compare with your local build (owner)

Download the exact edition artifact to out/release and verify its published zip hashes. Then run:

```sh
(cd out/release && sha256sum -c SHA256SUMS)
cargo xtask edition-compare --left out/dist --right out/release
```

The comparison opens each package, verifies its full internal manifest, then uses compare_payload and the single ATTESTATION_PATHS list. Expected: both zip hashes and both package manifests pass, and the deterministic payloads match. The browser summary and transfer statement may differ with the browser version; their corresponding checksum rows are verified against each package rather than compared across packages. Missing or changed content blocks sending. Fetch a damaged download again; investigate a deterministic payload mismatch before transfer.

### Step 8. Send it

Follow [transfer.md](transfer.md). Before E1 there is no laptop transfer: the edition is read at home.

## R1: the home gate before the first tags

The owner tags `edition-001` and `v0.1.0` on Day 1, only after the home gate in ADR 0001 passes. That record owns the wording of the eleven items; if this table and it differ, it wins.

| Gate item | Command or evidence | Who |
|---|---|---|
| 1. `check --strict` clean on a fresh clone | `git clone <repository> <empty dir> && cd <dir> && cargo run --locked -q -p fieldbook -- check --strict`; expected exit 0, with the rule set it implements printed | Owner |
| 2. Regression set, zero critical failures | The same `check` output; each row of [Critical failures](../controls.md#critical-failures) has its named test passing | Owner |
| 3. The 15 consequential claims owner-verified | `cargo run --locked -q -p fieldbook -- status`; expected: none of the 15 listed as unreviewed | Owner |
| 4. Five briefs accepted, each one printed page | Print each from the day-one binder; the owner accepts | Owner |
| 5. Secret, marking and identity scans clean | `just check`, then `cargo xtask denylist "$FIELDBOOK_DENYLIST"` (the pre-push hook runs both) | Owner |
| 6. The Day-1 calculation cases CALC-01 to CALC-04 pass ([FM0](../decisions/0001-foundation.md#calculation-core-fm0)) | `cargo nextest run --locked -p fieldbook-calc` | Owner |
| 7. Edge net-log shows no request that is not `file:`, in the tested pages and interactions | The `corpus` job is green on the commit | CI |
| 8. Tier-0 intake pack in the edition | `unzip -l out/dist/fieldbook-edition-001.zip` lists `TRANSFER-STATEMENT.txt`, `edition.toml`, `SHA256SUMS` and `README.txt`; the owner reads the transfer statement | Owner |
| 9. Home drill and backup drill pass on Linux | [Drill 1](restore-drill.md#drill-1-the-home-drill-on-linux-day-1) and [Drill 4](restore-drill.md#drill-4-the-backup-drill-on-linux-day-1) | Owner |
| 10. Question register and day-one list ready | `status` lists the open day-one questions; the owner reviews them | Owner |
| 11. `docs/maintenance.csv` exists | `test -f docs/maintenance.csv` | Owner |

The local pre-push hook supplies its captured ref updates to the denylist. The manual command without --updates checks every local branch and tag, including removed historical content; it is deliberately broader than one push.

When every item passes, do tool release steps 1 to 4 for `0.1.0` (so `edition.toml` records `0.1.0`) and edition steps 1 to 4 for `001`. Then tag both on the same commit and push them in one command: `git push origin v0.1.0 edition-001`. Watch both runs as in the steps above.

If the gate cannot pass in time, the floor in ADR 0001 applies: `edition-001` is the raw files, `SHA256SUMS`, a clean `check` and the transfer statement, with no site.

## R2: what changes for `v1.0.0`

R2 is a tool release with extra preconditions, a signing step and a larger checklist. Editions keep the same process and the same unsigned zips with out-of-band hashes.

Preconditions: the R2 gate in ADR 0001 is met; IT has approved and named the signer route; the rehearsal passed (offline rebuild, matching two-build compare). A mismatch in either blocks `v1.0.0` and nothing else.

Until IT answers IT-27, R2 can carry two signed programs: the silent build and the AI build (the same source with the Cargo feature `net`; the chat listener needs the further feature `bridge`). Each is built twice, signed and listed in the catalog, and the checklist below is run for each, so IT can approve the silent one without the other. If IT-27 is not answered yes, or the AI build's Windows capture and import check (NS-06) fails, R2 ships the silent build only.

The release names them `fieldbook.exe` (the silent build) and `fieldbook-ai.exe` (the AI build, built with `--features net`, plus `bridge` only if IT-32 says yes). `release.yml` renames the AI build's output to `fieldbook-ai.exe` before it is attached, so each program has its own file name and its own hash line. When R2 ships the silent build only, skip every step and row below that names the AI program.

Do tool release steps 1 to 5 for `1.0.0`. Then:

### R2 step 1. Check the release job (owner)

For each program, `release.yml` at R2 builds the exe twice on windows-2025 from the `dist` profile with the hardening flags in `.cargo/config.toml`, and fails unless the two hashes match; the log has one hash line for `fieldbook.exe` and one for `fieldbook-ai.exe`. It attaches each exe, its PDB (kept separately), the CycloneDX 1.5 SBOM, the licence notices and `SHA256SUMS`.

Expected: success, with the two exe hashes of each program equal in the log. If they differ for either program the release is BLOCKED; do not sign.

### R2 step 2. Dispatch signing (owner)

```sh
gh workflow run sign.yml --ref v1.0.0
gh run watch <run-id> --exit-status
```

`sign.yml` checks that the triggering actor is the owner and runs in the `signing` environment, whose tag rule allows only `v*` refs. For each program, it signs the exe (Authenticode, RFC 3161 timestamp, one subject for the product's life). After signing the executables, regenerate final SHA256SUMS over the executable payload (including SBOM and notices), excluding SHA256SUMS itself, the catalog and outer archive. The intake pack is a separately manifested companion, assembled after signing; it is outside this payload and catalog. Make the catalog over that payload plus SHA256SUMS, excluding the catalog itself and the outer archive, then sign it with the same identity. Package only after this; verify signatures, each final checksum and the catalog against the extracted delivered set. Unsigned comparison hashes are evidence, not the final checksum list.

Expected: success, and a signed exe for each program plus `fieldbook-v1.0.0.cat` in the run's artifact. If the actor check or the environment rule refuses, do not work around it; signing is part of the release gate.

### R2 step 3. Verify the signature (owner, any Windows machine)

```powershell
Get-AuthenticodeSignature .\fieldbook.exe | Format-List Status, SignerCertificate, TimeStamperCertificate
Get-AuthenticodeSignature .\fieldbook-ai.exe | Format-List Status, SignerCertificate, TimeStamperCertificate
Test-FileCatalog -Path .\release -CatalogFilePath .\fieldbook-v1.0.0.cat
```

Expected: `Status : Valid` with a timestamp certificate for each program (a silent-only release has no `fieldbook-ai.exe` line), and `Valid` from `Test-FileCatalog`, where `release` is the extracted executable payload with its SHA256SUMS, excluding the catalog, outer archive and intake companion. If either fails, the release is not handed to IT.

### R2 step 4. Run the release checklist (owner, Codex for evidence)

| # | Check | Evidence |
|---|---|---|
| 1 | `just check` is green on the tag commit and the tag equals the workspace version | The `ci-ok` run id |
| 2 | A fresh advisory scan in the release job (`cargo deny check advisories`) has no unwaived known-exploited item | The job log |
| 3 | Two builds on separate jobs match by exe hash, for each program | The job log |
| 4 | SBOM, licence notices, SHA-256 list, signed catalog and two-build evidence are attached | The artifact listing |
| 5 | Packaged smoke test: standard user, no sockets in the silent build; the AI build shows only the test settings' hosts and, if built with `bridge`, during the chat step, one loopback listener; version and hash shown, a newer-schema Corpus refused, uninstall diff, for each program | The PE import check and capture report |
| 6 | Migration rehearsal: the previous tool drills its own snapshot and backs up the restored old tree; the current tool byte-verifies and restores that snapshot (`restore --to`), migrates it and checks the new tree strictly; the previous tool still checks the untouched old tree; the command and its date are in the paragraph under this table, which the card `r2-migrate` writes | The rehearsal report |
| 7 | The owner dispatched `sign.yml` (R2 step 2) | The run id |
| 8 | The previous accepted release set is kept and its rollback is verified for each program; an explicit first release uses the empty-baseline rehearsal below | Where it is kept; the rehearsal result |
| 9 | The intake companion is assembled from the verified signed package and its fingerprint recorded | The fingerprint |
| 10 | `docs/controls.md` is reviewed against this release | The owner's confirmation |

Expected: every row has its evidence as a comment on the release work item. If a row has none, the release waits.

### R2 step 5. Hand over to IT (owner)

After signature verification, run `cargo xtask intake --release <verified-release> --evidence <approved-evidence> --out <new-intake-folder>`. Its companion manifest binds the final payload hashes, checksum list and signed catalog; no intake output is inserted back into the signed package. Follow the durable archive procedure below for the package, edition/originals and intake together, and verify that archive before delivery. Give IT the signed set and this intake companion through the route IT named. IT installs it as a managed copy; the install, uninstall and detection details are in the intake pack. Whether an edition's zips ever join the signed catalog is IT's call at intake; until then editions keep the out-of-band hash.

## Put a source version on hold after issue

Use this when a source version is found to be wrong, unlicensed or not public after an edition that holds it has been issued. Only the owner changes `lifecycle` (the lifecycle table in [the Corpus format](../reference/corpus-format.md#source-vocabularies) says what each state ships). Set the version to `quarantined`, with the reason in `note`, by a pull request the owner reviews. A quarantined version ships only its register row, its hash and its reason: no original, no extracted text, no search entry and no context-pack block. A source on hold is held version by version, so hold every version row of it. Only the owner moves a version back to `active`, with a `note`.

1. **Find the editions that hold it.** Run `cargo xtask edition-impact --source <source-id> --version <number> --archives <verified-archive-root>`. It enumerates edition tags, parses each tagged sources.csv by the separate source_id and version columns, and verifies the corresponding archived edition/originals pair. Payload inspection checks the registered original and extraction hashes/paths, generated source page, search unit and context content as applicable to its Raw or Site profile. It reports containing, absent or unresolved for each tag; missing archives remain unresolved. A retained register row or a later supersedes reference is not evidence of included payload. Resolve every unavailable archive before claiming a complete withdrawal list. Write the results in the pull request.
2. **Issue a replacement edition.** Follow the selected Raw or Site edition recipe with the next number. Expected: the retained register row, hash and reason remain; the original, extraction, source page, search unit and context-pack content are absent. A plain-text citation may retain the identifier.
3. **Tell the laptop.** Send the replacement through [transfer.md](transfer.md), and say that step 11's exception applies: delete every earlier zip that holds the version, and do not roll back to one.
4. **List the printed copies.** Name each printed day-one binder and brief that cites the version, from the edition numbers in step 1 and the footers (product, edition, page id). Take them out of use and reprint from the replacement.
5. **Old copies elsewhere.** What happens to copies that left your hands, if any, follows the employer's policy. Ask IT through the channel it named; do not decide it here.

Expected: the replacement edition is on the laptop, no zip holding the version is left in the delivery folder, and the list of printed copies is written down.

## Log the time

Release work counts as upkeep. Append one row per week to [docs/maintenance.csv](../maintenance.csv): the ISO week (`YYYY-Www`), the minutes, a kind of `upkeep` or `review`, and a short note. `fieldbook status` reports the minutes against the budget in ADR 0001 (D-16).

## Payload comparison

Use the edition command's `compare_payload` and its ATTESTATION_PATHS. First verify each full manifest independently. Compare deterministic entries and their path set; only the browser summary, transfer statement and checksum rows derived from them may differ. Never remove a checksum check merely to make two browser versions compare.

## Durable release archive

For tool delivery, `cargo xtask archive-release --package <verified-package> --edition <edition-set> --intake <intake-folder> --to <approved-archive>` copies the exact final signed package (unsigned before R2), its matching edition/originals pair, intake evidence, manifests, unsigned comparison evidence and previous-version identifier (or the explicit empty baseline for a first release) into a new version/hash folder. At R2 the intake companion has its own manifest bound to the signed package; it is not inserted into the executable catalog. It writes a sorted archive manifest of relative paths, sizes and full hashes last. An existing folder is accepted only if every byte matches; it is never overwritten. The command verifies the copy by reopening every file. The archive is an approved independent location and has retention through the product's supported rollback period; expiring workflow artifacts are transport only.

Download the paired zip artifact and `edition-NNN-evidence` from the same successful Site tag run. The evidence directory holds the tagged baseline, optional mode descriptor, current compatibility manifest and every bound contract/vector/implementation file. Verify tag/commit identity and all hashes; missing material blocks archival.

For a Site edition, use `cargo xtask archive-release --kind site --edition <edition-set> --evidence <downloaded-evidence> --base release-baseline.toml --to <approved-archive>` without --package; retain the exact pair and compatibility/intake evidence through the same collector.

`cargo xtask verify-archive <folder> --restore-to <empty-folder> --previous <previous-archive>` always verifies the current archive; omit --previous only when its stream’s baseline is explicitly empty. For a tool stream it extracts the stored packages without rebuilding and runs the previous tool binding’s packaged smoke and restore rehearsal. For an explicitly empty tool binding (including a first tool after raw editions), verify the current archived package and run its packaged smoke and synthetic restore fixture instead; the result records that no previous tool exists. A declared tool binding cannot choose this exception because its archive is missing. For an edition stream, restore and verify the exact edition predecessor’s pair, dispatching on that archive’s Raw or Site kind. An explicitly empty edition binding inspects the current pair. Preserve each provider’s Raw or Site manifest/content checks, including Site browser evidence, without executable smoke tests for the edition stream. A missing declared edition predecessor refuses. Record the result in the owner's tracker. A missing file, failed hash, required previous executable that is unavailable or same-disk-only copy blocks delivery. Keep at least the currently deployed and previous accepted sets. IT and the owner choose longer retention before deployment. No release deletes an archived set automatically.


## Release baseline

The release PR supplies `release-baseline.toml` in the single [baseline format](../reference/compatibility.md#baseline-descriptor): independently empty-or-bound compatibility, tool and edition tables. Verify each declared binding against its durable archive before tagging. Tag CI retrieves exact compatibility blobs from the compatibility commit in the full-history checkout and runs `cargo xtask release-check <tag> --base release-baseline.toml`. Archive verification selects the tool or edition predecessor for the tag’s stream; missing declared material is a failure, never an empty binding.

## Raw edition floor

Raw delivery uses the same owner tag, artifact comparison and durable archive gates, without an executable or Site provider. The owner commits `edition-release.toml` with exactly `release_schema = 1`, `edition = "NNN"` and `kind = "raw"` in the release PR. The number must match the newest edition heading and owner-created edition-NNN tag. The optional descriptor accepts kind raw or site only; absence means site. Unknown/duplicate fields, a wrong number or unknown kind refuse. The descriptor is read from the tagged commit, never a workflow input. The normal edition workflow skips raw tags; raw-edition.yml handles them independently.

1. Run `cargo xtask edition-raw --out out/dist` locally on the clean release commit. It produces the paired public archives, strict-check/regression evidence and raw intake text. Inspect Markdown, CSV and text, registered originals and both internal/external checksum lists. It ships no site, executable or browser summary and makes no browser-test claim.
2. After the owner merges and tags, raw-edition.yml validates the tag, main ancestry, committed mode and release-baseline.toml, runs edition-raw and uploads fieldbook-edition-NNN-raw. Download that artifact and fieldbook-edition-NNN-raw-evidence from the same tag run. The evidence artifact contains the committed mode/baseline descriptors, current compat/published.toml and all its bound public contract/vector files under their repository-relative paths. Verify those exact hashes and identity. Verify both manifests independently and compare the complete deterministic file/path sets with the local pair. Raw has no browser attestation exclusion. Any difference blocks delivery.
3. Run `cargo xtask archive-release --kind raw --edition <downloaded-set> --evidence <downloaded-evidence> --base release-baseline.toml --to <approved-archive>`. Raw archives contain the two downloaded zips, external SHA256SUMS, archive.toml and evidence/ with exactly the committed mode/baseline descriptors, current published compatibility manifest and every public contract/vector blob it binds. Verify those downloaded bytes against the tag identity and paired edition metadata; a missing or changed vector refuses completion. Site/tool archives retain the same compatibility evidence through the shared collector. Raw intake and clean check/regression reports are inside the edition zip. There is no package or executable requirement. Reopen and verify all bytes before publishing the archive manifest last in a new immutable edition/hash folder.
4. Run `cargo xtask verify-archive <folder> --restore-to <empty-folder> --previous <previous-archive>` (omit previous only for an explicit empty edition binding). Restore the exact archived pair, verify internal and external hashes, edition identity, clean check/regression reports, raw allowlist and original hashes, and inspect representative raw files. For a first delivery inspect the current pair; otherwise also restore and inspect the previous edition binding’s pair. Never rebuild or invoke executable smoke tests for Raw. Missing/damaged evidence or missing previous material blocks delivery. Keep the current and previous accepted sets in the independent archive under the same retention rules as Site.
5. Deliver those verified bytes through the approved channel. Complete the same owner-reviewed baseline-advancement PR, advancing compatibility and edition while retaining tool, using the manifest and vector hashes retained in this archive, before further implementation cards merge. Rollback restores the previous archived pair, except a quarantined payload must never be restored.

The shared archive format is strict TOML: archive_schema = 1; kind = "raw" or "site"; stream = "tool" or "edition" (raw requires edition); tag, commit and baseline_sha256; and sorted [[file]] rows with path, size and sha256. Paths are nonempty slash-separated relative paths beneath the archive root, with no absolute path, dot or parent component, link or duplicate; size is an unsigned byte count and sha256 is lowercase 64-hex. The manifest excludes itself. Every listed byte and the exact provider file set must verify; unknown keys or unlisted files refuse. Site payload verification retains browser and manifest checks; only the tool stream runs packaged executable smoke and restore gates. Shared copying, mode/tag checks and archive validation arrive with edition-manifest; raw-edition and release-workflow register their respective payload verifiers. See [raw-edition](../cards/raw-edition.md) for raw content and lifecycle tests.

### Independent build comparison

Run `cargo xtask two-build produce --profile PROFILE --out DIR` in two distinct Windows jobs per selected profile. Each archives build.toml, the executable and its raw PDB. A dependent job consumes both folders with `cargo xtask two-build compare --left LEFT --right RIGHT --out DIR`. Verify safe paths, recorded identities and hashes, then each PE CodeView GUID/age against the PDB information stream, then require equal executable and raw PDB bytes. No PDB normalization or ignored differences apply. The shared verifier lives in xtask/src/two_build.rs; analysis and signing use it. Preserve both raw artifacts and comparison.toml.

`build.toml` is strict TOML with exactly build_schema = 1, commit (40 lowercase hex), toolchain (the pinned channel), image (the runner image id), target, profile (silent, net or bridge), features (sorted unique feature names; [] / ["net"] / ["bridge"] respectively), source_date_epoch (nonnegative i64), exe, exe_sha256, pdb and pdb_sha256. The Windows target is x86_64-pc-windows-msvc; all three profiles use Cargo dist. exe and pdb are safe relative paths based at the producer folder containing build.toml. Hashes are lowercase 64-hex. Both files are required for this Windows command. No unknown or duplicate keys, missing field or unsupported schema is accepted.

`comparison.toml` contains comparison_schema = 1, the same seven identity fields (commit through source_date_epoch), and exactly two [[build]] rows numbered 1 and 2. Each row has number, descriptor, descriptor_sha256, exe, exe_sha256, pdb and pdb_sha256. All paths are based at the comparison folder. descriptor points to builds/1/build.toml or builds/2/build.toml; the corresponding exe and pdb paths start with that same builds/N prefix. Each whole producer folder is copied unchanged there after validation. Descriptor hashes bind the original producer metadata; verify it again, resolve its file paths relative to its own folder and require the comparison row paths/hashes to name those same copied files. Publish comparison.toml last. Paths use the archive-safe rules above. All identities must match between producers and comparison; both producer packages must pass PE/PDB binding before cross-build byte equality. Moving the comparison folder and deleting the inputs must leave verification possible.

The Windows production target configuration includes `-C target-feature=+crt-static`, `-C control-flow-guard=checks`, `-C link-arg=/DEPENDENTLOADFLAG:0x800`, `-C link-arg=/Brepro` and `-C link-arg=/PDBALTPATH:%_PDB%`. BuildPlan appends path remaps without replacing these flags. The build script embeds VERSIONINFO (ProductName Fieldbook, FileDescription Fieldbook offline reference toolkit, OriginalFilename fieldbook.exe; CompanyName supplied only by the release environment) and an asInvoker, UTF-8 application manifest through embed-resource. No company name is committed. Binary analysis checks these production resources and hardening on each compared profile; CFG coverage is first-party only because the standard library is precompiled. A weakened real binary is the negative control.

The shared BuildPlan uses x86_64-pc-windows-msvc, dist, the selected feature set, explicit debug information and packed PDB output, commit epoch/SHA, path remaps and /Brepro, with the pinned toolchain and windows-2025 image recorded. Do not replace configuration flags through RUSTFLAGS. Archive rebuilding retains --offline --frozen. P5 establishes the rehearsal plan; R2 adds its hardening checks.

After accepting a verified release archive, complete an owner-reviewed follow-up pull request advancing release-baseline.toml’s compatibility and accepted-stream bindings to that accepted tag, commit and archived compatibility hashes, preserving the other stream before any further implementation card merges. The tagged commit retains its previous-release baseline. This follow-up completes the release; `just release` changes the release files and replay inputs listed in Tool release step 1. Ordinary pull-request compatibility checks then protect every newly published binding.

The two-build Windows rehearsal runs each produced executable with `--version`; its commit component must equal the full `build.toml.commit`. The Cargo build environment sets `FIELDBOOK_GIT_SHA` to that commit. Equal binary hashes do not replace this identity check.

# raw-edition

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

`cargo xtask edition-raw --out DIR` produces the paired public raw edition and originals archives, with clean check and regression evidence and no site requirement.

## Why

- The floor delivery needs an executable recipe when renderer or browser work is deferred.

## Scope

- Files: `xtask/src/{edition_raw.rs,raw_release.rs}` (tests in each `mod tests`), `.github/workflows/raw-edition.yml`.
- Registration lines: `xtask/src/main.rs` (edition-raw); shared allowlist Raw profile required paths; `xtask/src/main.rs` (raw-edition-check and Raw archive provider); `docs/runbooks/release.md` (raw tag and archive path); `release-baseline.toml` (owner-reviewed baseline advancement); `edition-release.toml` (owner-selected delivery kind).
- Docs to update: release and transfer runbooks; intake README and raw text variants.

## Out of scope

- Site/browser evidence, executable delivery, creating or pushing tags, signing or new data admission.

## Preconditions

- Cards merged: `edition-manifest`, `search-cli-and-regression`, `check-ci-and-docs`, `calc-cases`.

## Steps

1. Refuse a dirty tree and derive edition ID, commit and source epoch as normal packaging does. Run the built CLI strict check with the commit date, including the regression pass; save its deterministic diagnostic/count report and regression result. Any failure refuses output. Never invoke render, site or browser commands; the raw command has no dependency on their generated files.

2. The edition archive has top folder `edition/` and exactly the public `shipped_corpus` selection, `corpus/eval/regression.toml`, `ASSISTANT.md`, `edition.toml`, `README.txt`, `TRANSFER-STATEMENT.txt`, `intake/{ai-development-statement.md,data-handling.md,integrity-guide.md,network-silence.md}`, `evidence/{check.json,regression.json}` and `SHA256SUMS`. The originals archive has top folder `originals/`, only the public nonquarantined originals selected by registered location/hash, and `SHA256SUMS`; renderable originals retain the existing text-safe naming rule. No state, inbox, site assets or browser summary is permitted. The raw profile is an explicit allowlist, not a relaxed Site profile.

3. Call edition-manifest’s pure EditionInfo constructor/serializer with the public view, complete shipped selection, latest edition heading, commit-derived date/SHA and tool/schema versions; use its fingerprint rule over all shipped Corpus files; mark the package variant as raw in README and transfer text without adding a Corpus schema field. Fixed raw intake variants state that there is no generated site, no browser test claim and no executable in this delivery. They retain the public-only statement, Mark-of-the-Web instructions and Tier-0 working-folder backup/hash recipe. Fill placeholders and refuse any left over. Write both internal checksums and deterministic zips with shared helpers, then an out-of-band two-line SHA256SUMS.

4. The floor drill verifies both archive hashes, expands both into temporary folders, verifies internal manifests and the quarantined exclusions, opens representative raw Markdown/CSV/text in an approved reader and checks originals against registered hashes. It requires the archived clean check/regression reports, but neither rebuilds a site nor launches Edge. The owner reviews the raw transfer statement before delivery. Keep the prior archive pair for rollback.

5. Tests call the parsed command handler in this binary module with renderer/browser invocations forbidden by its local scripted Runner: valid raw packaging succeeds with no site files or browser evidence, preserves regression evidence and is deterministic. Planted script, nonpublic content, held original, missing report, nonclean check or failed regression refuses before publication. A dependency test proves charts, renderer and Edge are not ancestors of this card.

6. Complete the raw tag workflow and durable archive path in the release runbook. raw-edition-check uses the shared edition tag validator and calc-cases baseline/compatibility verifier. Register the Raw archive provider with edition_release, and add the committed mode descriptor when the owner selects raw. raw-edition.yml runs on edition-* tags, on its own pull request and manually, uses pinned actions, read-only contents permission, full history without persisted credentials, pinned tools and bounded jobs. Only a tag whose committed descriptor selects raw runs the delivery job; trial PR/manual jobs use a temporary synthetic tagged repository. The delivery job runs raw-edition-check <tag> --base release-baseline.toml --evidence-out out/raw-evidence, then edition-raw, and uploads exactly the paired archives and external SHA256SUMS as fieldbook-edition-NNN-raw. It separately uploads fieldbook-edition-NNN-raw-evidence, containing the committed mode/baseline descriptors, current compat/published.toml and every public contract/vector blob it binds, at their repository-relative paths. Validate each blob hash before writing evidence and refuse dirty/missing or mismatched material. The evidence artifact is for the durable archive, not the delivered pair. It neither calls nor requires Site handlers. The owner alone creates tags and retrieves the exact workflow artifact.

7. Test the full raw lifecycle through parsed handlers with renderer/browser/executable providers unavailable: tag selection, clean packaging, published artifact file set, downloaded-pair verification and local comparison, durable archival, first-delivery restore/inspection, previous-pair rollback, and baseline advancement. Use a local scripted Runner for workflow commands and actual temporary files for archive handlers. Tests raw_release:: cover wrong mode/heading/ancestry, missing baseline, failed or missing check/regression reports, changed archive bytes, missing previous pair and immutable-folder collision. Static workflow assertions check pinning, permissions, tag selection and command order. Record one successful PR/manual unsigned workflow run before this card completes.

8. Raw archive-release takes --evidence DIR as well as --edition DIR. Verify the downloaded compatibility set and committed descriptors against the tag identity and paired edition metadata, copy them under evidence/, and cover every byte in archive.toml. verify-archive reopens this set; baseline advancement derives new compatibility entries from these retained bytes. Test missing vectors and mismatched manifest hashes, and prove a subsequent ordinary compatibility check protects the newly accepted binding. The same helper from edition_release collects versioned public evidence for Site/tool archival.

9. Use the shared baseline parser’s separate compatibility, tool and edition bindings. Lifecycle tests cover raw edition → first tool and tool → edition update → next tool: retain new compatibility vectors, select the prior executable only from tool, and select raw/rendered pairs from edition. Explicit absence succeeds only for that stream; missing declared predecessors fail. The archive manifest records its stream and the advancement preserves the other stream.

10. Bind `FIELDBOOK_SECURITY_CONTACT` from the owner-provisioned same-named repository secret in every packaging step this card adds, including scheduled, release and Windows calls as applicable. Forward the environment through child edition/intake commands. Tests use synthetic contact text, verify the workflow binding and require IN-01 for missing/blank input; do not print or commit the real value. No token permission changes are needed.

## Acceptance

```sh
cargo nextest run --locked -p xtask edition_raw::
cargo xtask cards --check
cargo nextest run --locked -p xtask raw_release::
just check
```

Expected: raw packaging tests pass with no renderer/browser calls and all refusal cases preserve previous output.

## Negative control

Permit a planted script or invoke site rendering: the corresponding real-command test must fail. Skip the regression gate: the failed-regression case must fail. Restore each.

Skip tag mode validation, allow damaged archived bytes, infer an empty baseline from a missing previous pair, or invoke a Site provider: the matching real-handler lifecycle test must fail. Restore each.

Replace the retained tool binding with the intervening edition binding: the mixed-stream lifecycle tests must fail. Restore it.

## Rollback

Retain and reinstall the previous paired archives; remove only failed staging. Normal site editions keep their own gate.

## BLOCKED

Required check or regression evidence fails, or the owner has not reviewed the raw delivery statement: stop delivery and record the precise gate.

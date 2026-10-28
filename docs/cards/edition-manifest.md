# edition-manifest

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

Raw and rendered editions share one public file selection, fingerprint, checksum, allowlist, data-only public support catalogue and deterministic ZIP implementation.

## Why

- The raw delivery path must be available before any renderer or browser harness; the edition's fixed values and release order are in [Scope and versioning](../reference/corpus-format.md#scope-and-versioning) and [the release runbook](../runbooks/release.md).
- Owner review covers the release gate: this card owns tag validation and archive/release dispatch helpers, as well as the shared public payload used by release packaging.

## Scope

- Files: `crates/fieldbook-core/src/manifest.rs`, `xtask/src/{allowlist.rs,zipdet.rs}` (helper tests stay in each module’s `mod tests`), `crates/fieldbook-core/src/edition.rs`.
- Registration lines: module declarations in core and xtask; `crates/fieldbook-core/src/lib.rs` (edition and manifest modules); `xtask/src/main.rs` (helper modules); `docs/cards/README.md` (packaging recipe); `docs/reference/audit-contract.md` (the new `## Public support verification` section, this card’s clause only); `docs/runbooks/release.md` (the support evidence schema and artifact list, this card’s entries only); `xtask/src/edition_release.rs` (mode descriptor, tag and archive helpers, provider dispatch and tests); `xtask/src/main.rs` (archive-release, verify-archive and edition-impact); `docs/runbooks/release.md` (shared schemas).
- Docs to update: packaging recipe in the cards README.

## Out of scope

- Site rendering, edition admission, signing and delivery.

## Preconditions

- Cards merged: `check-command`, `core-labels`, `core-policy`, `calc-cases` (shared compatibility/baseline parser).

## Steps

1. In pure core, `shipped_corpus(tree, quarantined)` returns sorted path/hash pairs for `corpus/corpus.toml`, `corpus/pages/**`, `corpus/registers/*.csv` and `corpus/sources/text/**`, excluding quarantined text folders, originals and agent instructions. It accepts only the public coherent view; a nonpublic included file is a refusal. Keep the quarantined source row/hash/reason. `fingerprint(entries)` hashes sorted `<sha256>  <path>\n` lines. `write_sums(entries)` returns those same canonical lines; callers provide the selected files and exclude the manifest itself. No I/O enters core.

2. In xtask, the allowlist takes an explicit profile (`Raw` or `Site`) and required paths, validates every staged path and file, rejects executable/script payloads, state, inbox, symlink escape, excessive paths and quarantined payloads, and verifies every listed content hash. The Site profile is used later by edition-zips; do not require an unbuilt renderer. `zipdet` sorts paths as bytes, uses the supplied source epoch, fixed permissions and deflate only. Reuse existing dependency pins and record any new pin under the dependency recipe.

3. Tests use synthetic staged trees: hand-computed fingerprint, exact checksum bytes, public-only selection, quarantined row retained/payload excluded, prohibited file refusal, long path refusal, changed-byte refusal and deterministic ZIP bytes across directories/timezones. Share these helpers with both packaging consumers; no duplicate manifest or allowlist implementation.

4. In pure `edition.rs`, define `BuildInfo { built: Option<CivilDate>, commit: Option<String>, profile: BuildProfile, workbook_epoch: i64 }` with `BuildProfile { Silent, Ai }` and `is_released()` true exactly for a valid date and 40-hex commit. `EditionInfo::from(view: &LabelledView, files: &[(String, String)], heading: Option<&str>, info: &BuildInfo, tool_version: &str, schema_version: u32) -> Result<EditionInfo, MetadataError>` validates the public view and complete shipped Corpus selection, including regression when shipped, and uses the fixed field/key order of corpus-format.md. `to_toml()` returns those bytes with no I/O. workbook_epoch carries the full validated SOURCE_DATE_EPOCH, or the fixed synthetic fallback 2000-01-01T00:00:00Z when absent; it is not serialized into edition.toml and is independent of is_released. Incomplete date/commit means edition 0 and empty built/commit; a released build requires its heading. `edition_metadata_without_renderer` tests exact deterministic bytes, fingerprint and incomplete metadata without linking a renderer.

5. Provide the release runbook’s strict mode and archive schemas in edition_release.rs. Tag validation checks an edition tag against the exact commit’s newest heading, origin/main ancestry and mode descriptor. The shared archive dispatcher accepts Raw or Site provider hooks, registered by their producing cards; an unavailable provider refuses with a named missing capability. Define ArchiveArgs, DeliveryKind, ArchiveManifest, safe relative-path/hash validation, atomic copy and reopen verification here. A provider verifies its payload and restores it for inspection; helpers do not import a renderer, browser or executable module. Tests in edition_release:: exercise synthetic provider hooks, changed copied bytes, unsafe paths, explicit first delivery and a missing previous set. This card registers the command parser once; consumers only register their providers.

6. Implement edition-impact as the release runbook specifies. Tests use temporary tagged repositories and verified synthetic archive pairs: before admission, a payload-bearing edition, a later quarantined edition retaining its register row, and an unavailable archive. A combined version id first appearing in supersedes must not change the result. Classify each tag as containing, absent or unresolved, with a reason. Missing archive evidence never means absent. Use structured CSV fields and payload paths/hashes, never git pickaxe or tag ancestry to infer payload inclusion.

7. Provide collect_release_evidence as a generic helper taking the validated published manifest and its path/hash entries from the compatibility producer. It copies only those public versioned blobs plus the mode/baseline descriptors, validates exact hashes, and preserves repository-relative names below evidence/. No compatibility parser is duplicated here; callers supply the already parsed entries once calc-cases exists. Synthetic helper tests require no renderer or compatibility module dependency.

8. Use the shared baseline parser’s separate compatibility, tool and edition bindings. Lifecycle tests cover raw edition → first tool and tool → edition update → next tool: retain new compatibility vectors, select the prior executable only from tool, and select raw/rendered pairs from edition. Explicit absence succeeds only for that stream; missing declared predecessors fail. The archive manifest records its stream and the advancement preserves the other stream.

9. Define `RenderContext::Public(EditionInfo)` alongside BuildInfo. Its public constructor keeps EditionInfo’s complete public-view/heading validation. Renderers and wrappers carry this context without deriving another metadata value; the company-store card later adds its company variant and view-derived metadata. Public planned paths include edition.toml. Test public context serialization against the same metadata vector.

10. Extend `crates/fieldbook-core/src/manifest.rs` with the data-only public support catalogue defined by [Public support verification](../reference/audit-contract.md#public-support-verification): the supported role and path registrations (the exact root files, the registered page and workbook CSV templates, `eval/regression.toml` and optional `pipeline/rules.toml`), canonical relative-path validation, a strict schema 1 parser and deterministic serializer/selection. Reuse the existing SHA-256, path and manifest helpers and the actual schema/template/layout registrations. The helper is pure: no I/O, no backup or renderer import, no dependency, and no bytes or edition-specific hashes from `corpus/` in a constant. It selects every present recognized support file from one validated public generation and refuses an unsupported or ambiguous registration. Bound input to 1 MiB and 4096 entries; enforce the exact byte rules, sorted paths, duplicate/unknown-key/path refusal, traversal/link/collision refusal and exact-byte hashing before parsing. Both packaging producers consume this one helper; its existing public selection remains the producer of the payload, and this card does not stage it. A content update therefore refreshes the catalogue as data without rebuilding or replacing the executable.

   Tests in `mod tests` of `manifest.rs`: `support_catalogue_literal_bytes_and_hash` (the literal vector above, 194 bytes, its independently known digest, fixed writer bytes, sorted entries, raw-byte hash and a changed comment producing a different digest; the expected literal is not calculated with the encoder under test), `support_catalogue_is_strict_and_complete` (all landed roles, optional absent rules, unknown/duplicate/traversal/link/colliding paths, malformed hashes and both input bounds refuse; no source bytes occur in the output) and `support_catalogue_never_panics_on_200_seeded_inputs` (the shared `tests/support/rng.rs` generator at file level, 200 byte strings plus the valid vector with each byte flipped and once deleted through the strict parser; a value or a typed error, never a panic). Later support producers extend this registration/coverage test under the backup-kind recipe.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core manifest::
cargo nextest run --locked -p xtask allowlist::
cargo nextest run --locked -p xtask zipdet::
cargo nextest run --locked -p fieldbook-core edition::
cargo nextest run --locked -p xtask edition_release::
just check
```

Expected: helper tests pass without rendering a page or launching a browser; all commands exit 0.

## Negative control

Include a quarantined payload or a planted script: the allowlist test must fail. Change one checksum byte: verification must fail. Restore each.

Drop a shipped file from metadata or mark an incomplete build released: `edition_metadata_without_renderer` must fail. Restore each.

Allow a mismatched tag/mode, skip reopened archive verification or treat a missing previous pair as the first delivery: the corresponding edition_release:: test must fail. Infer impact from a retained row or supersedes text: the edition-impact fixture must fail. Restore each.

Replace the retained tool binding with the intervening edition binding: the mixed-stream lifecycle tests must fail. Restore it.

Hash a parsed map or reserialized TOML instead of the exact catalogue bytes, or let a comment change leave the digest unchanged: `support_catalogue_literal_bytes_and_hash` must fail. Restore it. Accept an unknown role, a duplicate or traversing path, or an entry beyond the byte or file-count bound: `support_catalogue_is_strict_and_complete` must fail. Restore it. Put the current support bytes or an edition hash in a constant: `one_executable_accepts_two_supported_editions` must fail. Restore it.

## Rollback

Revert the helper change before its consumers merge; after that migrate both consumers together.

## BLOCKED

A required public classification or original hash is unknown: stop with the exact file and rule; do not invent evidence.

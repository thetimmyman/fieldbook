# core-types-and-strings

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-core::hex::lower` renders bytes as lowercase hexadecimal, and `version` provides the frozen schema maximum and minimum-tool lookup. This complete standard-library capability changes no manifest or lock. Frozen words, chips, JavaScript, docs generation and the literal control remain with `core-types-and-strings-2`.

## Why

- The existing [hex policy](../reference/dependencies.md#in-house-instead-of-a-crate) and [version contract](../reference/corpus-format.md#scope-and-versioning) require one pure implementation. Values and wording do not change.
- The original card exceeds the [size limit](README.md#size-and-splitting). This first part needs only the existing workspace; consumers requiring the complete strings/generator provider wait for part two.

## Scope

- Files: `crates/fieldbook-core/src/{hex.rs,version.rs}`.
- Registration lines: `crates/fieldbook-core/src/lib.rs` (hex/version exports); `docs/cards/core-types-and-strings-2.md` (all remaining original obligations); `docs/cards/{audit-journal.md,check-v0.md,core-ids-and-dates.md,core-schema-pages.md,day1-dependencies-3.md,drafts/assistant-page.md}` (actual remaining-provider references); `docs/plan/PLAN.md` (two-part order); `AGENTS.md` (docs-command provider name); `docs/decisions/0001-foundation.md` (docs-command provider name only); `docs/reference/corpus-format.md` (Strings intro provider name only).
- Docs to update: the named provider/order references only.

## Out of scope

Strings/chips/JS, generators, literal scanning, snapshots/goldens, new dependencies, manifests/lock, all Corpus data and design files. No string-provider consumer becomes eligible here.

## Preconditions

- Cards merged: `workspace-skeleton`.

## Steps

1. Export the pure `hex::lower(&[u8]) -> String`: two lowercase digits per byte, including zero padding. Retain literal empty, 00ff and SHA-256-empty digest tests.
2. Export `version::SCHEMA_VERSION_MAX = 1` and `min_tool_version(schema)`, with schema 1 requiring 0.1.0 and unknown schemas returning None. Retain their native tests; this does not itself implement check/build refusal.
3. Preserve all original words/chips/JS/docs/literals acceptance and negative controls in part two. Rewire only consumers actually needing that complete provider; pure primitive consumers need not wait for it.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core hex::
cargo nextest run --locked -p fieldbook-core version::
just check
```

Expected: all six primitive tests and the public doctest pass; manifests and lock are unchanged. No strings/docs command or application-readiness claim follows.

## Negative control

In a private copy, uppercase hexadecimal digits or drop zero padding: the literal hex tests must fail. Change the schema-1 tool value or return a tool for an unknown schema: the version tests must fail. Restore each plant.

## Rollback

Revert the merge. The primitive exports disappear; the remaining provider and its consumers cannot become eligible without this prerequisite.

## BLOCKED

The frozen schema values or public interface would need to change, or implementation needs a dependency. Keep the existing contract; do not adopt a crate or weaken a control here.

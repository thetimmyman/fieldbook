# zip-advisories

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`Cargo.lock` holds one `zip` at 9.0.5 or later and `rust_xlsxwriter` at 0.101.x, so no open advisory names a locked `zip`, and the deterministic zip writer of `xtask` still gives the same bytes for the same input.

## Why

- The advisories. Five advisories name the locked `zip` 8.6.0: GHSA-m665-v7pm-7grj (critical, fixed in 9.0.5), GHSA-9hj8-73fh-9963, GHSA-hjqx-j4rw-g7xv and GHSA-q7v9-x93c-xqv3 (high, fixed in 9.0.0) and GHSA-8hxf-qr8p-62x4 (moderate, fixed in 9.0.0). `xtask/src/zipdet.rs` reads and writes zips, so the reader issues apply to it.
- Why both move. The graph holds exactly one `zip` ([dependencies.md](../reference/dependencies.md#policy), Duplicate versions), and `rust_xlsxwriter` 0.99.1 needs `zip ^8.3`. Every `rust_xlsxwriter` from 0.100.0 needs `zip ^9.0` with `deflate` only. The newest that clears the 14-day cooldown ([The cooldown](../reference/dependencies.md#the-cooldown)) at the time of writing is 0.101.0; 0.102.0 is younger. `zip` 9.0.5 is older than 14 days, so no cooldown exception is needed and the card is `auto` under [Dependencies and the lock file](README.md#dependencies-and-the-lock-file).
- This is the `rust_xlsxwriter` and `zip` half of the planned replacement row in [Planned replacements](../reference/dependencies.md#planned-replacements), brought forward by the advisories. It takes over the `rust_xlsxwriter` and `zip` half of the former P4 line `rust-xlsxwriter-bump`, which the pull request that added this card removed from PLAN.md. `calamine` is not in the graph yet; `extract-xlsx-sheets` pins the `calamine` that is on `zip` 9 when it adds it, as its own BLOCKED section says.
- Determinism. AD-12 and `edition-zips` require byte-identical zips for equal inputs. `rust_xlsxwriter` stamps the current time unless told otherwise ([AGENTS.md](../../AGENTS.md#traps)); no shipped code writes a workbook yet, so the card adds a test that pins the fixed creation time on the new release.
- The recipe is [Adding a crate, a tool or an action](README.md#adding-a-crate-a-tool-or-an-action) and [Review evidence](../reference/dependencies.md#review-evidence).
- Design: none.

## Scope

- Files: `Cargo.lock`, `xtask/src/zipdet.rs` (only for an API change of `zip` 9; its tests are in its `mod tests`), `xtask/src/edition_release.rs` (only for an API change), `crates/fieldbook-render/tests/xlsx_determinism.rs` (new).
- Registration lines: `Cargo.toml`, the `rust_xlsxwriter` and `zip` lines of `[workspace.dependencies]` (the draft `net-stack-spike` also edits that file); `docs/reference/dependencies.md`, the `rust_xlsxwriter` and `zip` rows under Pins, the note under them, the Planned replacements row, and one new row per profile under Measurements.
- Docs to update: `docs/reference/dependencies.md` as above, and `CHANGELOG.md` (`- Changed: the zip reader and writer move to zip 9, which fixes five published advisories.`).

## Out of scope

- `calamine` and any reader of workbooks.
- A requirement or pin change for any other crate. `just lock` re-resolves the whole lock under the cooldown, so other lock entries may move; Step 2 names each one and the review covers it.
- The `zlib` feature of `rust_xlsxwriter`, and any feature that brings C code into the build graph ([Named bans](../reference/dependencies.md#named-bans)).
- A cooldown exception: if a fix needs a release younger than 14 days, that is the owner's (BLOCKED).

## Preconditions

- Cards merged: none.

## Steps

1. Before any edit, record the SHA-256 of the zip that `deterministic_zip_is_byte_identical_across_insertion_orders_directories_and_timezones` in `zipdet.rs` writes, in the pull request (a scratch print, not committed).
2. Set `rust_xlsxwriter = "0.101.0"` and `zip = { version = "9.0.5", default-features = false, features = ["deflate"] }` in `Cargo.toml`. Run `just lock` and read the whole `Cargo.lock` diff: list in the pull request every lock entry that enters, leaves or changes version, each with its old and new version and its release date ([Review evidence](../reference/dependencies.md#review-evidence) step 4), and confirm one `zip`.
3. Fix any compile error that the `zip` 9 API causes in `zipdet.rs` or `edition_release.rs`. Keep the stored method refused, the entry order sorted and the timestamp from `SOURCE_DATE_EPOCH`.
4. Add `crates/fieldbook-render/tests/xlsx_determinism.rs` with `a_workbook_with_a_fixed_creation_time_is_byte_identical`: write the same one-sheet synthetic workbook twice with a fixed creation time in its document properties and compare the bytes.
5. Gather the evidence of [Review evidence](../reference/dependencies.md#review-evidence) steps 1 to 4: both locked and frozen builds, both cargo-deny shapes, metadata for both graph targets, the transitive counts against the caps of [Policy](../reference/dependencies.md#policy), and the release date of every changed lock entry. Update the rows and append the Measurements rows. In the note under Pins, the sentence that names `rust-xlsxwriter-bump` says instead that `zip-advisories` moved `zip` to 9 and that `calamine` 0.37.0 is the first release on it. Obtain the exact-source MAX/JEV review of step 5, covering `rust_xlsxwriter`, `zip` and every lock entry of Step 2's list, and link it in the pull request.
6. Record the SHA-256 of Step 1's zip again. If it differs, say so in the pull request with the reason (a change of the deflate output); the requirement is equal bytes for equal inputs on one version, which Step 4's test and the Acceptance tests check.

## Acceptance

```sh
cargo nextest run --locked -p xtask zipdet::
cargo nextest run --locked -p fieldbook-render --test xlsx_determinism
cargo tree --locked --workspace -e normal -i zip
cargo deny --locked check advisories bans licenses sources
just check
```

Expected: the tests pass; `cargo tree` prints one `zip v9.` line at its root; `cargo deny` exits 0 and reports no advisory for `zip`; `just check` exits 0.

## Negative control

In the new test, leave the creation time unset in one of the two writes and wait one second between them: `a_workbook_with_a_fixed_creation_time_is_byte_identical` must fail with a byte mismatch. Restore it. Set `zip` back to `8.6.0` in a scratch copy of `Cargo.toml` with `rust_xlsxwriter` at 0.101.0: `cargo tree --workspace` must fail to resolve or show two `zip` versions. Restore it.

## Rollback

Revert the merge commit. The lock returns to `zip` 8.6.0 and `rust_xlsxwriter` 0.99.1, with the advisories open again.

## BLOCKED

The transitive count of a profile goes above its cap, `zip` 9 needs a feature that brings C code into the build graph, `deny.toml` would need a change, or the fix needs a release younger than 14 days: comment `BLOCKED: <what>` on the work item with the count or the crate, push the branch and take the next card. The owner decides a cap or a cooldown exception.

# check-register-links

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` raises FB0207 (a reference to a row or area that does not exist), FB0208 (register supersession that is not reciprocal, names itself or loops), FB0211 (a stored original whose file, row or hash disagree) and FB0214 (a glossary label or alias used twice, ignoring case). One id index over the registers serves this pass and the page and citation passes after it.

## Why

- The codes are in [02: registers and sources](../reference/corpus-format.md#02-registers-and-sources). The references are the id and list columns of [Registers](../reference/corpus-format.md#registers) and the rules across columns that name FB0207 and FB0208. Supersession is in [Supersession](../reference/corpus-format.md#supersession), and the four original-file cases and their check scope are in [Originals](../reference/corpus-format.md#originals).
- A changed original is control ED-03 in [docs/controls.md](../controls.md#edition-and-release). The binary hashes originals as it reads them ([AD-05](../decisions/0001-foundation.md#ad-05-crate-layout-and-enforcement)); core compares, and hashes in memory only the bytes it was given.

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,model.rs,index.rs,register_cells.rs,register_links.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (one stored original, step 5, and fixes a new rule finds), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports broken register references, supersession, original hashes and repeated glossary labels (FB0207, FB0208, FB0211, FB0214).`). `errors.md` does not change.

## Out of scope

- Page ids and page references (`check-pages`, `check-page-links`), and citation keys (`check-citations`).
- The lifecycle of a version under a verified claim, and claims on superseded versions (FB0213, FB0222, `check-promotion`).
- Extraction files and their hashes (FB0212, `extract-and-locators`).

## Preconditions

- Cards merged: `check-claim-cells`.

## Steps

1. Add `index.rs`: an `Index` built once in the model from the typed rows and `corpus.toml`, holding the source versions (with `lifecycle` and the computed superseded state), claims, questions, conflicts, terms and areas, with the line of each row. `check-pages` adds page ids to it later. Expose `Model::index(&self) -> &Index`; the accessor returns the index built by load. Test `index_accessor_returns_resolved_keys`.
2. Add `register_links.rs` with one pass. FB0207 for every id cell and list cell that names a row or an area, and for the rules across columns marked FB0207, at the cell. Point those entries, and the FB0208 and FB0211 entries, of the rules table in `register_cells.rs` at this pass.
3. FB0208 for claims, sources and terms as Supersession gives them. Report a loop once, at its first row in key order, naming its ids in that order.
4. FB0211 for the four cases of Originals. An `Original` entry carries the hash the binary computed; a `Bytes` entry (tests, the known-bad harness) is hashed here with `hash::Sha256Stream`. Report at the row's `sha256` cell, or at the file for a file with no row, and name the version id. Only when `Tree::contains_dir("corpus/sources/public")` is false, skip these checks and add a note that says so. An existing empty folder still raises FB0211 for a row whose original is missing.
5. Give one version of the fixture's second source a stored original: a few lines of plain synthetic text, written for this card and marked synthetic, at `corpus/sources/public/<version-id>.txt`. Change its row's `location` to the file name and its `sha256` to the file's hash. The row counts of the fixture do not change.
6. FB0214: compare every label and alias of the glossary with every other, after lowercasing; report the second use, naming the first term.
7. Unit tests named `check::register_links::...` and `check::index::...`: each code, each original case, a loop of three claims, `absent_originals_directory_has_skip_note`, `empty_originals_directory_reports_missing_file`, and the fixture clean.
8. Move FB0207, FB0208, FB0211 and FB0214 from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture (FB0211: change the stored original's `sha256` cell). If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request. Set `control = "ED-03"` on the FB0211 case and delete the ED-03 row of `[[pending_control]]`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::register_links check::index check::register_cells
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with four new known-bad cases and the fixture still strict clean; `check` on the real Corpus prints `exit 0`; `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini` and append one byte to its stored original. Run `(cd target/nc/corpus-mini && cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"; echo "exit $?")`: it must print an `error[FB0211]` line that names the version id, and `exit 1`. Remove `target/nc/`.

Leave a valid source key out of the returned index: `index_accessor_returns_resolved_keys` must fail. Restore it.

Skip original checks for an empty existing folder: `empty_originals_directory_reports_missing_file` must fail. Restore it.

## Rollback

Revert the merge commit. The four codes go back to `pending.toml`, and the fixture loses its stored original.

## BLOCKED

- The lead's committed registers or originals raise one of these codes: write the code and the row and stop. Codex may not edit `corpus/` before `guard-statuses`.
- An original in the real Corpus would have to be rewritten to pass. Originals are byte-exact: write the version id and stop.

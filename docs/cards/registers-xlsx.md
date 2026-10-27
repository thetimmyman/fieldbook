# registers-xlsx

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build` writes `data/registers.xlsx` and the five CSV copies `data/<register>.csv` into the edition, byte-identical across builds of one commit, and a read-back test proves every workbook cell is a string equal to its register cell.

## Why

- The files, the CSV copy rules and the workbook rules are [Excel copies of the registers](../reference/corpus-format.md#excel-copies-of-the-registers). The workbook is [AD-13](../decisions/0001-foundation.md#ad-13-spreadsheet-outputs); its place in the edition zip is [Repository layout](../decisions/0001-foundation.md#repository-layout).
- Controls XL-01 (a workbook that is only data) and DT-01 (byte-identical output) are in the [control register](../controls.md). `rust_xlsxwriter` and the `zip` dev edge are pinned in [dependencies.md](../reference/dependencies.md) and declared by `day1-dependencies`; this card adds no crate.
- Design: the [MAP.md](../design/MAP.md) rows `#claims`, `#questions`, `#glossary`, `#sources` (the workbook and CSV half) and `#templates` (the `data/registers.xlsx` link in TemplatesList). [HANDOFF.md](../design/HANDOFF.md) section 5 Components (TemplatesList, RegisterTable). No token group: the change adds links, not styles.

## Scope

- Files: `crates/fieldbook-render/src/{lib.rs,xlsx.rs,csv_copy.rs,home.rs}`, `crates/fieldbook-render/templates/index.html`, `crates/fieldbook-render/templates/register.html` (the one register template of `build-registers`; one link line), `crates/fieldbook-render/tests/xlsx.rs`, `crates/fieldbook/src/build.rs`, `xtask/src/edition.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/site.rs` (render calls and Site inserts plus `planned_paths` entries, this card's paths only: `data/registers.xlsx` and the five CSV copies); `crates/fieldbook/tests/leak_outputs.rs` (workbook and CSV surfaces).
- Docs to update: `CHANGELOG.md` (`- Added: data/registers.xlsx and CSV copies of the registers.` under `## Unreleased`).

## Out of scope

- The data-only templates `templates/<name>.xlsx` (E1) and the decision package (Day 30).
- Reading a workbook back into the Corpus. Nothing imports a workbook.

## Preconditions

- Cards merged: `day-one-binder-and-registers-xlsx`, `edition-zips` (`REQUIRED_FILES` in `xtask/src/edition.rs`, which step 4 extends).

## Steps

1. `csv_copy.rs`: write each register's copy with the byte-order mark, line endings and apostrophe guard of the reference, same columns, rows and order.
2. `xlsx.rs`: one sheet per register with the reference's sheet names, the header row built from `Register::columns()`, every cell written as a string, the header row frozen, an autofilter over the used range, no formula, and the creation time set with `DocProperties::set_creation_datetime` from `SOURCE_DATE_EPOCH`. Without it, use `2000-01-01T00:00:00Z`, the workbook-only fallback of the reference. Read BuildInfo.workbook_epoch, populated at the binary boundary by build-v0 from the full validated epoch or fallback; do not populate missing release metadata. Never enable the `zlib` feature.
3. `site.rs`: insert the workbook and five CSV copies through `Site::insert` at `data/registers.xlsx` and `data/<register>.csv`. The binary writes them with all other assets in its one `io::out::replace` call for `<out>/edition`, through `write_outside`; it never replaces a separate data root. `home.rs` and `index.html`: TemplatesList links `data/registers.xlsx` in place of the `not_available` line. The register pages link their CSV copy and the workbook. In `crates/fieldbook-render/src/site.rs` add the six paths to `planned_paths`, so that `planned_paths_equal_the_site_keys` holds (a link is built only to a planned path).
4. `xtask/src/edition.rs`: append the six files of `data/` (`data/registers.xlsx`, `data/claims.csv`, `data/conflicts.csv`, `data/glossary.csv`, `data/questions.csv`, `data/sources.csv`) to `REQUIRED_FILES`, the const of `edition-zips`. Add `missing_workbook_is_fb0601` to `mod tests` of `edition.rs`: delete `data/registers.xlsx` from the staged tree and expect FB0601 to name it (filter `edition::tests::missing_workbook_is_fb0601`).
5. Tests in `crates/fieldbook-render/tests/xlsx.rs`, reading the fixture's workbook back with `zip`: `core_xml_has_the_fixed_time`; `no_formula_macro_external_link_or_hyperlink` (no `<f>` element, no `vbaProject.bin`, no `externalLinks/`, no hyperlink relationship); `every_cell_is_a_string_equal_to_its_register_cell` (the failure names the sheet and the cell, such as `claims!C2`); `header_frozen_with_autofilter`; `every_header_equals_register_columns` (a sheet's header cells equal `Register::columns()` for its register); `readback_flags_a_formula_cell` (a workbook made in the test with `write_formula` in one cell: the read-back that `no_formula_macro_external_link_or_hyperlink` uses reports it and names the sheet and the cell); `two_builds_same_bytes` (the first build runs in the test; the second runs in a child process, the test binary started again through `std::env::current_exe()` with `--exact build_in_child --ignored`, with `TZ=Pacific/Auckland` and another working directory; the `#[ignore]` test `build_in_child` reads its fixture and output folders from the environment and does nothing without them); `csv_copy_has_bom_crlf_and_guard` (a synthetic cell that starts with `=`).
6. Mark the tests that carry a control row of [the controls table](../controls.md) with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: `// control: SC-06` on `csv_copy_has_bom_crlf_and_guard`, `// control: XL-01` on `readback_flags_a_formula_cell` and `// control: DT-01` on `two_builds_same_bytes`.

7. `workbook_without_epoch_has_fixed_creation_time` checks that docProps/core.xml contains exactly 2000-01-01T00:00:00Z and repeated builds match without an epoch. Through the real edition handler and sink in `crates/fieldbook/tests/leak_outputs.rs`, add `delivered_workbook_and_csvs_filter_rows`: read back delivered workbook cells and CSV rows; permitted canaries are present and restricted ones absent. Register both surfaces. Check the edition omission line without a numeric count or added register columns.

8. In the existing real delivered-workbook tests, `delivered_workbook_keeps_full_epoch` reads docProps/core.xml for a non-midnight epoch and requires the exact UTC instant. Pair it with the absent-epoch case; edition metadata remains unreleased when release inputs are incomplete. The timestamp never passes through CivilDate.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test xlsx
just site
ls out/edition/data
T="$(mktemp -d)" && cp out/edition/data/registers.xlsx "$T/first.xlsx" && just site && cmp "$T/first.xlsx" out/edition/data/registers.xlsx && echo same
head -c 3 out/edition/data/claims.csv | od -An -tx1
cargo nextest run --locked -p xtask edition::
cargo nextest run --locked -p fieldbook --test leak_outputs
just check
```

Expected: the tests pass; `ls` prints `claims.csv`, `conflicts.csv`, `glossary.csv`, `questions.csv`, `registers.xlsx` and `sources.csv`; the second build prints `same`; `od` prints `ef bb bf`; the edition tests still pass with the six files required; `just check` exits 0.

## Negative control

In `xlsx.rs`, write any cell that parses as an integer as a number. `cargo nextest run --locked -p fieldbook-render --test xlsx every_cell_is_a_string_equal_to_its_register_cell` must fail and name the `sources` sheet and the cell of a `version` value. Restore the file. Then write the first data cell of the `claims` sheet as a formula (`write_formula`): `cargo nextest run --locked -p fieldbook-render --test xlsx no_formula_macro_external_link_or_hyperlink` must fail and name the sheet and the cell. Restore it. Then make `xlsx.rs` write the creation time in the local time zone instead of UTC from `SOURCE_DATE_EPOCH`: `two_builds_same_bytes` must fail, because its second build is a child process under `TZ=Pacific/Auckland` and another working directory. Restore it.

Use the current clock without an epoch: `workbook_without_epoch_has_fixed_creation_time` must fail. Bypass filtering in workbook or CSV delivery: `delivered_workbook_and_csvs_filter_rows` must fail. Restore each.

Truncate the epoch to midnight: `delivered_workbook_keeps_full_epoch` must fail. Restore it.

## Rollback

Revert the merge commit. The edition loses `data/`, and TemplatesList shows the `not_available` line again.

## BLOCKED

`rust_xlsxwriter` 0.99.1 cannot write a byte-identical file for one creation time (for example it stamps another time inside the package): set the item to Blocked with a `BLOCKED:` comment that says so, and stop. Do not post-process the zip by hand.

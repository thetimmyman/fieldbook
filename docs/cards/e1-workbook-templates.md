# e1-workbook-templates

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build` writes `templates/discovery-log.xlsx`, `templates/assumption-register.xlsx` and `templates/90-day-plan.xlsx` into the edition, and the edition zip requires them. Each is data only: a Read-me sheet, and a data sheet that is an Excel table whose header cells equal the frozen column list of its template, with drop-down lists on the closed columns and synthetic example rows. Two builds of one commit give the same bytes, and a read-back test proves no cell holds a formula.

## Why

- The three files, their content and the bridge are decided: [AD-13](../decisions/0001-foundation.md#ad-13-spreadsheet-outputs) and [Workbooks and templates](../reference/corpus-format.md#workbooks-and-templates). They are data only, with validation lists, an Excel table each, a Read-me sheet that explains "recalculate, then Save as CSV UTF-8" and the drop folder of `file-drop@1`, and synthetic example rows. `decision-package.xlsx` is the only workbook with formulas and is the card `decision-package`, not this one. The sources are `corpus/templates/workbooks/*.csv`, "synthetic sources of the data-only workbooks" ([Layout](../reference/corpus-format.md#layout-of-corpus)); the file check already accepts that folder.
- The edition may carry them: the allowlist of the site edition already permits `templates/<name>.xlsx`, and the zip's layout lists `templates/*.xlsx` "from E1" ([Repository layout](../decisions/0001-foundation.md#repository-layout)). E1 requires that "the three templates ship" ([Gates](../decisions/0001-foundation.md#gates)); the runbooks already copy `edition\templates\discovery-log.xlsx` to a working folder ([transfer](../runbooks/transfer.md#working-copies), [restore drill](../runbooks/restore-drill.md)).
- Controls: XL-01 (a workbook that is only data) and DT-01 (byte-identical output) in the [control register](../controls.md). `rust_xlsxwriter` and the `zip` dev edge are pinned and declared; this card adds no crate. The writer sets a fixed creation time with `DocProperties::set_creation_datetime` and never enables `zlib` ([dependencies.md](../reference/dependencies.md)). The template files are static public synthetic text, not rows of a register: they carry no company record, so they take the same edition view as `registers-xlsx` and add a surface row, per the rule in [Writing a card](README.md#writing-a-card).
- The 90-day plan template uses relative days only and has no date column: the start date "is entered in the browser and is never stored in the Corpus or committed" (O-05 and O-07 in [ADR 0001](../decisions/0001-foundation.md#owner-decisions)).
- No gate applies, so `Review` is `auto`: the columns are not a Corpus object (no `schema_version` bump, the object table of [Scope and versioning](../reference/corpus-format.md#scope-and-versioning) says they are "frozen with the E1 templates"), nothing touches `docs/intake/` or `SECURITY.md`, and no label, policy or egress module changes.
- Lead decisions where the repository is silent. The columns were not written down anywhere (the table says "frozen with the E1 templates"), so this card freezes them, taken from the screens in [HANDOFF.md](../design/HANDOFF.md) (the Assumption row type and the `#assumptions`, `#discovery` and `#plan` routes). (1) Assumption register: `assumption_id`, `text`, `package`, `package_version`, `waits_on_question`, `owner_role`, `source_ref`, `as_of`, `confidence`, `evidence_grade`, `moves`, `revisit_when`, `status`, `drafted_by`. (2) Discovery log: `row_id`, `kind` (`note`, `gap`, `system`, `clause`: the four things the Discovery route lists), `title`, `detail`, `source_ref`, `area`, `owner_role`, `waits_on_question`, `status`, `as_of`, `drafted_by`. (3) 90-day plan: `row_id`, `window`, `item`, `duty`, `proof_test`, `owner_role`, `status`, `drafted_by`. `drafted_by` is `ai` or `owner` ([the object table](../reference/corpus-format.md#scope-and-versioning)); `owner_role` holds a role, never a person; `source_ref` is a key plus locator such as `S-0042-v2, p. 14` or the words `No source yet`. (4) Closed lists: `confidence` is `High`, `Medium`, `Low`; `evidence_grade` is the whole `evidence_grade` vocabulary `A` to `G` ([claim vocabularies](../reference/corpus-format.md#claim-vocabularies)), where HANDOFF shows only A to E, because an assumption may rest on a company value (A to D) and the Corpus vocabulary is the one list a later record type can read; `window` is `Day 1 to Day 30`, `Day 31 to Day 60`, `Day 61 to Day 90` as the checklist page writes them; `status` of an assumption is `draft`, `in_review`, `approved`, `superseded`, and of the other two `open`, `in_progress`, `done`. (5) Example rows are three per template, marked synthetic in `title` or `text`, and the only date in them is the synthetic `2000-01-01`. (6) Pages that link the files (the `Workbook:` line of a `template` page, the Template table) are not built here; no page has such a line today. The owner's delegate accepted this list (A to G, a superset of HANDOFF's A to E) and the `auto` review, because the object table already says the columns are "frozen with the E1 templates": freezing them here is the planned step, not a format change, and no ADR 0002 amendment is needed. (7) The path `templates/workbooks/<stem>.csv` is already a registered text-bearing Corpus file (`manifest.rs` and the file check of `check/files.rs` accept it), so this card adds no manifest or catalogue entry.
- Design: the [MAP.md](../design/MAP.md) rows `#templates` (TemplatesList), `#plan`, `#assumptions` and `#discovery` (the workbook half of each). [HANDOFF.md](../design/HANDOFF.md) section 5 Components (TemplatesList; the Assumption and Template row types). No token group: the change adds files, not styles.

## Scope

- Files: `crates/fieldbook-render/src/{lib.rs,templates_xlsx.rs}`, `crates/fieldbook-render/tests/templates_xlsx.rs`, `xtask/src/edition.rs`, `corpus/templates/workbooks/{discovery-log,assumption-register,90-day-plan}.csv`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/site.rs` (the three inserts and `planned_paths` entries, this card's paths only); `crates/fieldbook/tests/leak_outputs.rs` (the template surface); `docs/reference/corpus-format.md` (the template columns and the Layout of corpus/ rows, this card's rows only); `docs/controls.md` (the template clause, this card's clause only).
- Docs to update: `docs/reference/corpus-format.md` (the three column lists under [Workbooks and templates](../reference/corpus-format.md#workbooks-and-templates), and the E1 row of the object table: `Frozen with the E1 templates` becomes a link to that list), `docs/controls.md` (the `Card` cell of XL-01 and DT-01 gain this slug), `CHANGELOG.md` (`- Added: the three E1 workbook templates in the edition.` under `## Unreleased`).

## Out of scope

- `templates/decision-package.xlsx` (`decision-package`), a rate register, formulas, macros, links and a template that reads a register.
- Reading a filled template back: the bridge is the owner's "Save as CSV UTF-8" and `file-drop@1` in R2. Nothing here imports a workbook.
- The overlay pages, record types and checks for these columns (R2), and any page that links a template.
- The transfer drill and `README.txt` wording about templates (`transfer-and-windows-drill`).

## Preconditions

- Cards merged: `registers-xlsx` (`xlsx.rs` writer conventions, the fixed creation time through `BuildInfo.workbook_epoch`, `REQUIRED_FILES` rows for `data/` and the workbook read-back helpers of `tests/xlsx.rs`), `build-v0`, `edition-zips`, `core-csv` (the reader of the template CSV text).

## Steps

1. The three CSV sources, `corpus/templates/workbooks/<name>.csv`: the header row of the decision (4) lists of Why and three synthetic example rows each, UTF-8, LF, no BOM. The names are exactly `discovery-log`, `assumption-register` and `90-day-plan`.
2. `templates_xlsx.rs`: `pub const TEMPLATES: &[TemplateSpec]` with `TemplateSpec { name, sheet, columns: &[Column] }` and `Column { header, list: Option<&[&str]> }`, the one place the closed lists live; `pub fn render(name: &str, csv: &[u8], workbook_epoch: i64) -> Result<Vec<u8>, TemplateError>`. It reads the CSV with the `core-csv` reader and refuses a header that is not the spec's (a `TemplateError` naming the template and the column). It writes two sheets, `Read me` first and the data sheet second. The data sheet is one `Table` over the header and the example rows (no total row, no calculated column, so no formula), every cell a string, the header frozen, and a `DataValidation` list on each closed column over the table's data rows plus 200 empty rows. The Read-me sheet holds plain strings only: what the file is, that it is data only, the bridge sentence ("recalculate the workbook, because a CSV holds the values displayed and no formula; then Save as CSV UTF-8, and put the file in the drop folder"), the closed lists, and the words `Synthetic example rows: replace them.` The creation time is set as in `xlsx.rs`, with the same fallback.
3. `site.rs`: `render_with_info` reads `corpus/templates/workbooks/<name>.csv` for each `TEMPLATES` entry from `view.tree()` (`get_bytes`, as it already reads `corpus/changes.md`; the tree holds the bytes of every Corpus file except public source originals), inserts `templates/<name>.xlsx` and adds the three paths to `planned_paths`, so `planned_paths_equal_the_site_keys` holds. `build.rs` does not change. A missing or malformed CSV is a `WorkbookError` whose text begins with the tree path `corpus/templates/workbooks/<name>.csv`; the existing mapping in `build.rs` reports every `render_with_info` error as FB0605 with the location `data/registers.xlsx` 1:1, and that mapping and the text of FB0605 stay as they are, so the file the owner must fix is named in the message and not in the location.
4. `xtask/src/edition.rs`: append the three paths to `REQUIRED_FILES`; add `missing_template_is_fb0601` to `mod tests` (delete one staged template, expect FB0601 to name it; filter `edition::tests::missing_template_is_fb0601`; it fails only if the path is in `REQUIRED_FILES`).
5. Tests in `crates/fieldbook-render/tests/templates_xlsx.rs`, using the read-back helpers of `tests/xlsx.rs`: `every_template_header_equals_its_csv` (and the spec, both ways); `each_template_has_a_read_me_sheet_and_one_table`; `no_formula_macro_external_link_or_hyperlink` (no `<f>` element, no `vbaProject.bin`, no `externalLinks/`, no hyperlink relationship; the `<formula1>` of a validation list is allowed and counted separately); `closed_columns_have_the_listed_values` (the validation list text of each closed column equals `TEMPLATES`, and every example row's value is in its list); `read_me_names_the_bridge` (the Read-me text holds `Save as CSV UTF-8` and `drop folder`); `plan_template_has_no_date_column_and_no_calendar_date` (no header named `date` or `start`, and no `YYYY-MM-DD` in the plan's cells); `core_xml_has_the_fixed_time`; `two_builds_same_bytes` (second build in a child process under another `TZ` and directory, as in `registers-xlsx`); `a_wrong_header_is_a_template_error`; `a_missing_template_csv_names_the_file` (`render_with_info` over a view that lacks one CSV returns an error whose text holds `corpus/templates/workbooks/<name>.csv`). Mark `// control: XL-01` on the formula read-back test and `// control: DT-01` on `two_builds_same_bytes`.
6. Through the real edition handler in `crates/fieldbook/tests/leak_outputs.rs`: `delivered_templates_are_public_and_complete` reads the three delivered files back and requires the Read-me text and the example rows to be present and no cell to hold a restricted canary that the fixture's other surfaces filter; register the surface so `output_surfaces_cover_built_sinks` holds.
7. Docs: write the three column lists and the closed values into `corpus-format.md` as the frozen set, and update the object-table row and the controls cells. Run `cargo xtask docs --check`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test templates_xlsx
cargo nextest run --locked -p xtask edition::tests::missing_template_is_fb0601
cargo nextest run --locked -p fieldbook --test leak_outputs
just site
ls out/edition/templates
T="$(mktemp -d)" && cp out/edition/templates/discovery-log.xlsx "$T/first.xlsx" && just site && cmp "$T/first.xlsx" out/edition/templates/discovery-log.xlsx && echo same
just check
```

Expected: the tests pass; `ls` prints `90-day-plan.xlsx`, `assumption-register.xlsx` and `discovery-log.xlsx`; the second build prints `same`; `just check` exits 0.

## Negative control

In `templates_xlsx.rs`, write the first example cell of the assumption register with `write_formula`: `no_formula_macro_external_link_or_hyperlink` must fail and name the sheet and the cell. Restore it. Then change a header in `assumption-register.csv`: `every_template_header_equals_its_csv` and `a_wrong_header_is_a_template_error` must fail. Then drop the `evidence_grade` validation: `closed_columns_have_the_listed_values` must fail. Then add a `start_date` column to the plan: `plan_template_has_no_date_column_and_no_calendar_date` must fail. Then take the creation time from the local clock: `two_builds_same_bytes` must fail. Then leave one template path out of `REQUIRED_FILES`: `missing_template_is_fb0601` must fail. Then make `render_with_info` skip a missing CSV: `a_missing_template_csv_names_the_file` must fail. Restore each.

## Rollback

Revert the merge commit. The edition loses the three files and the three `REQUIRED_FILES` rows; the CSV sources go with it. Nothing else reads them.

## BLOCKED

`registers-xlsx` is not merged, or `view.tree()` does not hold `corpus/templates/workbooks/*.csv` when `render_with_info` runs: stop and ask the lead. A negative control that stops failing stops the card.

# claims-csv-parts

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

`fieldbook build` writes the claims copy for Excel as one file per claims part, `data/claims-<AREA>-<part>.csv`, in the same groups and order as the `claims/<AREA>-<part>.html` pages, and no longer writes `data/claims.csv`, so no edition file grows with the number of claims. The edition check names a claims part page that has no CSV and a CSV that has no page, and the allowlist refuses any `data/*.csv` that is neither a register copy nor a claims part.

## Why

- The need. `data/claims.csv` is the one edition file that grows with the Corpus. Today it is 20,012,178 bytes for 28,927 claims, 76% of the hosted site's 25 MiB per-file limit (`MAX_FILE_BYTES` in `xtask/src/hosted.rs`, 26,214,400 bytes, checked by `cargo xtask hosted-limits` and by `REGISTER_COPY` in `crates/fieldbook-render/src/lint.rs`). About 8,700 more claims break the build with `FB0605`, and the planned Corpus at 20 times Day 1 size (`scale-fixture`) is far past it. The other four CSV copies are small, and `data/registers.xlsx` is 4,814,585 bytes with all five sheets, so the workbook is about four times under the limit and is not changed here.
- The rule that already exists. A claim's part is a function of its id only, `fieldbook_core::ids::claim_part` with `CLAIMS_PER_PART = 500` (part = (number − 1) / 500 + 1, per area). `register::claims_parts` groups the claims by it into the `claims/<AREA>-<part>.html` pages and `ids-split` reuses it for `ids/claims-<AREA>-<part>.html`. This card reuses the same function; it keeps no second copy of the rule.
- The settled wording this card changes. The files and the copy rules are [Excel copies of the registers](../reference/corpus-format.md#excel-copies-of-the-registers), which lists `data/<register>.csv` with `data/claims.csv`; the `FB0605` row of the check catalogue and [ADR 0006](../decisions/0006-edition-search-index.md) (the sentence on the register copies) say each `data/<register>.csv` takes the 25 MiB limit. Both change by a one-sentence edit, an ADR 0006 amendment among them, so this card is `Review: owner`. Gate: not a label, policy or egress module and not the audit contract; the owner reviews it because it changes an accepted decision and the frozen path string `data/<register>.csv`.
- Design: the `#claims` row of [MAP.md](../design/MAP.md) and [HANDOFF.md](../design/HANDOFF.md) section 5 (`RegisterTable`, the claims index). The design names `data/<register>.csv` in the Paths row of section 0 and in `fieldbook_core::strings::PATHS`; the mockup (`docs/design/mockup/index.html`) says "Every build writes data/claims.csv". This card does not edit `docs/design/` or `PATHS`: the conflict is reported in the pull request and needs a design revision card. The one reader-visible change is the "Copy for Excel:" line of `templates/register.html`: the claims index lists a CSV link beside each part, and a part page links its own CSV beside the workbook.
- Lead decisions, each one the repository does not settle:
  1. Name `data/claims-<AREA>-<part>.csv`, with the area code upper case and the part number without leading zeros, so the stem is the one `is_area_part_stem` in `xtask/src/allowlist.rs` already accepts for the part pages.
  2. Remove `data/claims.csv` entirely and add no index CSV. A CSV that lists the parts would be one more file to keep equal to the pages, and an Excel user opens parts by area anyway. The `corpus/registers/claims.csv` source copy stays in the raw zip.
  3. Keep the `claims` sheet of `data/registers.xlsx` whole. The workbook has room; a later card splits it if it nears the limit.
  4. `REQUIRED_FILES` in `xtask/src/edition.rs` drops `data/claims.csv`, and a new check requires the set of `data/claims-*.csv` to equal the set of `claims/*.html` part pages, raising `FB0601` with the name of the one that is missing or extra.
  5. The allowlist refuses a `data/*.csv` that is not a register copy or a claims part. Today it accepts any `data/<name>.csv`.
  6. The 25 MiB register-copy limit in `lint.rs` stays as it is: it already covers any `data/<name>.csv`, a part is about 350 KB, and no lint changes.
  7. A part of fewer rows than 500 (the last of an area) has its own file. A claim that `claim_part` cannot place is not copied, as it has no page now.

## Scope

- Files: `crates/fieldbook-render/src/csv_copy.rs`, `crates/fieldbook-render/src/register.rs`, `crates/fieldbook-render/templates/register.html`, `crates/fieldbook-render/tests/claims_csv.rs`, `crates/fieldbook-render/tests/xlsx.rs`, `xtask/src/allowlist.rs`, `docs/decisions/0006-edition-search-index.md`, `docs/reference/corpus-format.md`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/site.rs` (the part CSV `planned_paths` entries and `Site::insert` calls, and the removal of the `data/claims.csv` ones; no other line); `xtask/src/edition.rs` (the `REQUIRED_FILES` entry and the part check with its two tests, no other line); `crates/fieldbook/tests/leak_outputs.rs` (the CSV read of `delivered_workbook_and_csvs_filter_rows`, which reads each claims part); `crates/fieldbook-render/tests/site_lints.rs` is read only (it uses `data/claims.csv` only as a size example of a register copy and keeps passing).
- Docs to update: `docs/decisions/0006-edition-search-index.md` (one sentence: the claims copy is in parts, each under the same limit), `docs/reference/corpus-format.md` (the `data/<register>.csv` row of Excel copies of the registers and the `FB0605` row), and `CHANGELOG.md` (`- Changed: the claims copy for Excel is one CSV per claims part, data/claims-<AREA>-<part>.csv, in place of data/claims.csv.`).

## Out of scope

- Splitting the `claims` sheet of `data/registers.xlsx`, and the other four CSV copies.
- `docs/design/`, `fieldbook_core::strings::PATHS` and the HANDOFF Paths row: a design revision card changes them after the owner accepts this card.
- Any change to `claim_part`, `CLAIMS_PER_PART`, the part pages or `ids-split`'s pages.
- The `lint.rs` register-copy limit and `xtask/src/hosted.rs`.

## Preconditions

- Cards merged: `search-site-terms` (it changes `docs/reference/corpus-format.md`, which Step 6 edits too), `registers-xlsx` (it wrote `csv_copy.rs`, `data/claims.csv` and the `REQUIRED_FILES` entry), `ids-split` (it made `claim_part` and the part pages the single source of the parts).

## Steps

1. In `crates/fieldbook-render/tests/claims_csv.rs`, write the tests first, on the fixture claims register with synthetic rows added as `tests/registers.rs` does (`make_claim_row`): `claims_part_csvs_equal_the_part_pages` (the `data/claims-*.csv` keys of the built site map one to one to `register::claims_parts`); `claims_part_csvs_hold_the_register_rows_in_order` (the part files, header removed, joined in order equal `csv_copy::rows(view, Register::Claims)`; each file starts with the byte-order mark, has CRLF line endings and the reference header); `a_part_splits_at_500` (`CL-CMP-0500` and `CL-CMP-0501` land in two files); `no_unparted_claims_csv` (the site has no `data/claims.csv`, and `planned_paths` does not hold it); `claims_pages_link_their_csv` (the index lists a CSV link per part and a part page links its own file, and every link is a planned path). Run them: they fail.
2. `csv_copy.rs`: `claims_part_output_path(area_code, part) -> String` and `render_claims_part(view, &ClaimsPart) -> Vec<u8>`, built from `rows(view, Register::Claims)` filtered by `claim_part` of the first cell, with the BOM, CRLF and guard of `render`; `output_path(Register::Claims)` is no longer used for the site. Keep `render` for the other registers.
3. `register.rs` and `templates/register.html`: `ClaimsIndexPartView` and the part page view gain a `csv_href` from `href_if_planned`; the claims index drops its single `csv_href`. `site.rs`: add the part paths and inserts and remove the claims entry of `data/<register>.csv`. Update `tests/xlsx.rs` so `csv_copy_has_bom_crlf_and_guard` and its path test expect four register copies and the part files.
4. `xtask/src/edition.rs`: drop `data/claims.csv` from `REQUIRED_FILES`; add the part check (a function over the staged tree) and the tests `missing_claims_part_csv_is_fb0601` and `stray_claims_part_csv_is_fb0601` (filters `edition::tests::`). `xtask/src/allowlist.rs`: the `data/` rule accepts `registers.xlsx`, the four register copy names and `claims-<stem>.csv` where `is_area_part_stem` holds; add `claims_part_csv_names_are_checked` (accepts `data/claims-CMP-1.csv`; refuses `data/claims.csv`, `data/claims-cmp-1.csv`, `data/claims-CMP-0.csv`, `data/claims-CMP-01.csv`, `data/claims-CMP-1.csv.txt` and `data/sub/claims-CMP-1.csv`, each with `FB0601`).
5. `crates/fieldbook/tests/leak_outputs.rs`: `delivered_workbook_and_csvs_filter_rows` reads the claims parts for the claims register; permitted and restricted canaries still hold.
6. Docs: the three edits of Scope. Run `cargo xtask docs --check` to confirm no generated reference changes.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test claims_csv
cargo nextest run --locked -p fieldbook-render --test xlsx
cargo nextest run --locked -p fieldbook-render --test site_lints
cargo nextest run --locked -p xtask edition::
cargo nextest run --locked -p xtask allowlist::
cargo nextest run --locked -p fieldbook --test leak_outputs
just site
ls out/edition/data
cargo xtask hosted-limits
just check
```

Expected: the tests pass; `ls` prints `claims-<AREA>-<part>.csv` files, `conflicts.csv`, `glossary.csv`, `questions.csv`, `registers.xlsx` and `sources.csv`, and no `claims.csv`; `hosted-limits` exits 0 and names `data/registers.xlsx` as the largest data file; `just check` exits 0.

## Negative control

In `site.rs`, insert `data/claims.csv` again from `csv_copy::render`: `no_unparted_claims_csv` must fail. Restore it. Skip one part file in the site inserts: `claims_part_csvs_equal_the_part_pages` and the edition check `missing_claims_part_csv_is_fb0601` must fail, the second with `FB0601` naming the part. Restore it. Make the allowlist accept any `data/*.csv` again: `claims_part_csv_names_are_checked` must fail. Restore it.

## Rollback

Revert the merge commit. The edition writes `data/claims.csv` again, and the wording of ADR 0006 and corpus-format returns with it.

## BLOCKED

- The owner does not accept the amended wording or the `data/<register>.csv` change: set the item to Blocked with a `BLOCKED:` comment, and stop.
- A test compares `fieldbook_core::strings::PATHS[1]` (`data/<register>.csv`) with the files a build writes, or a script of the site reads `data/claims.csv`: set the item to Blocked with a `BLOCKED:` comment naming the test or script, and stop. Do not edit `PATHS` or `docs/design/`.
- The part pages and the part CSVs cannot share one `claims_parts` order without a change to `claim_part`: set the item to Blocked with a `BLOCKED:` comment, and stop.

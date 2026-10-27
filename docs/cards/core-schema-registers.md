# core-schema-registers

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Rows of `sources.csv` (28 columns), `questions.csv`, `conflicts.csv` and `glossary.csv` parse into typed values. Every vocabulary of those four registers is an enum. A cell that is empty where it must not be, outside its form, outside its vocabulary or too long comes back as a named problem. A round-trip test driven by the register's column list fails when a column has no field.

## Why

- The columns, forms and lengths are in [sources.csv](../reference/corpus-format.md#sourcescsv), [questions.csv](../reference/corpus-format.md#questionscsv), [conflicts.csv](../reference/corpus-format.md#conflictscsv) and [glossary.csv](../reference/corpus-format.md#glossarycsv). The values are in [Source vocabularies](../reference/corpus-format.md#source-vocabularies) and [Other registers](../reference/corpus-format.md#other-registers).
- `check-register-cells` turns these problems into FB0204 and FB0205, so this card decides what counts as one. The five columns after `purpose` (`origin_system`, `origin_stream`, `origin_key`, `doc_kind` and `series`) are the exception: `from_row` reads them as text and judges nothing, because their one check is FB0215 (`r2-pull-and-sync`), which reads `DocKind` and `vocab::stream_id_form` from here.

## Scope

- Files: `crates/fieldbook-core/src/schema/{mod.rs,vocab.rs,rows.rs,sources.rs,questions.rs,conflicts.rs,glossary.rs}`; `crates/fieldbook-core/tests/registers_no_panic.rs`.
- Docs to update: none. The recipe "Adding a column" in [docs/cards/README.md](README.md#adding-a-column) already names the test of step 6, and `docs/reference/corpus-format.md` points to it.

## Out of scope

- The claims register and its vocabularies (`core-schema-claims`).
- Rules across columns and across rows (`check-register-cells`, `check-register-links`).
- Reading a decimal (`calc-cases` adds FB0500 for the `value` column of claims).

## Preconditions

- Cards merged: `core-csv` and `core-schema-pages`.
- The second creates `vocab.rs` and the test `every_enum_matches_the_reference` that step 1 extends.

## Steps

1. In `vocab.rs`, add one enum for each of `source_kind`, `source_role`, `authority_tier`, `family_basis`, `lifecycle`, `question_status`, `work_can_continue`, `conflict_status`, `definition_status`, `doc_kind` and `adoption`, in the style of `core-schema-pages`: `ALL`, `as_str`, parsing. `DocKind::ALL` is the const `DOC_KINDS`: `contract`, `modification`, `quote`, `rate_package`, `eac_package`, `baseline_log`, `schedule_file`, `billing_export`, `receivables_export`, `evms_export` and `other`. Add `vocab::stream_id_form(&str) -> bool` beside `label_form`: `[a-z][a-z0-9-]{0,31}`, the form of a stream id and of a `series`. `Adoption` has the values `proposed`, `prototype_reviewed`, `pending_company_approval`, `company_approved`, `rejected` and `superseded`; `core-schema-claims` uses it for the claims column of the same name. Add the eleven enums to `schema::vocab::every_enum_matches_the_reference`, which compares each `ALL` with the values of its row in the vocabulary tables of the reference; a row is found by the first words of its first cell, so `doc_kind (company store only)` is the row of `DocKind`. Include the overlay values (`answered`, for `source_kind` the three company kinds `vendor_quote`, `company_record` and `company_document`, and for `adoption` `pending_company_approval` and `company_approved`) so that FB0210 can name them, and mark them with `is_overlay()`. `SourceRow::handling` is `Vec<String>`: split a nonempty cell on `|` and retain each value as written; an empty cell becomes `[]`. No label-form or empty-cell CellProblem is made here; FB0410 judges the values. `source_role` also holds the two values the Corpus never uses (`generated_synthesis`, `working_draft`); they parse and `check` refuses them later.
2. In `rows.rs`, write `CellProblem { column, kind }` with kinds `Empty`, `Form`, `CalendarDate`, `Vocabulary` and `TooLong`, and the helpers the four rows share: a text cell with a length limit, a list cell (split on `|`, no spaces, and a `|` in any other cell is a `Form` problem), a bool, an int, a date, a partial date and an id. A malformed or impossible date, or a partial value in a full-date column, is `CalendarDate`; other bad forms are `Form`.
3. Write `SourceRow`, `QuestionRow`, `ConflictRow` and `GlossaryRow`, each with `from_row(&Row) -> Result<Self, Vec<CellProblem>>` and `to_cells() -> Vec<String>`. Fields follow the column names. An empty cell is `None` or an empty list where the column allows it; `QuestionRow.adoption` is `Option<Adoption>`, empty in the Corpus. Text limits: 1,000 characters for a text cell, 300 for `quote`, 400 for a glossary `definition`, 300 for a `boundary_rule`, 40 for a source `tag`. The five columns after `purpose` are text cells of 1,000 characters at most; whether their values are right is FB0215, not this card.
4. Leave rules across columns out. `from_row` checks one cell at a time.
5. Test each row type with a valid synthetic row, and with one row for each problem kind: an empty `title`, `retrieved = "2025-02-30"` and `retrieved = "2025-02"` (`CalendarDate`), `version = "x"`, `source_kind = "blog"`, a 1,001-character `note` (`a_text_cell_over_the_limit_is_too_long`, in `schema::rows`), a `|` in a text cell, and a list with a space around the bar. Test that `from_row` then `to_cells` returns the cells it was given. Test that a source row with `doc_kind = "memo"` and `series = "Bad Name"` still reads (FB0215 judges them), that `DocKind::parse("memo")` is an error, and that `stream_id_form` accepts `commitments` and refuses `Bad Name`, a leading digit and 33 characters.
6. `schema::rows::every_column_survives_from_row_to_cells`, for each of the four registers (`core-schema-claims` adds claims to this same test): take `Register::columns()` from `core-csv`, build a row with a valid synthetic cell in every column from a table in the test (column name to sample, with every optional cell filled), and require `from_row` to succeed and `to_cells` to return exactly those cells. A column with no sample in the test table fails the test and names the column, so adding a column to the reference forces a field here. `sources.csv` has 28 columns, so its table has 28 samples, with `doc_kind` = `quote` and `series` = `commitments` among them.

7. In `tests/registers_no_panic.rs`, `parser_never_panics_on_200_seeded_inputs` includes the shared generator with `#[path = "../../../tests/support/rng.rs"] mod rng;` at file level. Run 200 seeded byte strings and each valid synthetic row with every byte flipped and deleted through all four `from_row` readers, vocabulary parsers and `stream_id_form`; require a typed value or error, never a panic.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core schema::sources schema::questions schema::conflicts schema::glossary schema::vocab schema::rows
cargo nextest run --locked -p fieldbook-core --test registers_no_panic
just check
```

Expected: both succeed; every row type has a test for each problem kind that applies to it, and the round-trip test covers every column of the four registers.

## Negative control

Raise the text limit from 1,000 to 1,001 characters. The test `schema::rows::a_text_cell_over_the_limit_is_too_long` must fail. Restore the line. Remove `answered` from the question statuses: `schema::vocab::every_enum_matches_the_reference` must fail. Restore the line. Remove `evms_export` from `DOC_KINDS`: the same test must fail and name `doc_kind`. Restore the line. Make `SourceRow::to_cells` return an empty cell for one column: `every_column_survives_from_row_to_cells` must fail and name that column. Restore it.

Make a row parser index a missing cell: `parser_never_panics_on_200_seeded_inputs` must fail. Restore it.

## Rollback

Revert the merge commit. Nothing outside `fieldbook-core` uses the module yet.

## BLOCKED

A value in a column table of the reference is not in the vocabulary tables, or the other way round. Write the column and the value and stop (format gate).

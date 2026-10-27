# core-csv

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-core::schema` reads the five registers as tables and writes them back in normal form. The column list of each register is data in the code, and a test requires it to equal the column tables of the reference.

## Why

- The CSV rules, the five column tables and the sort orders are in [Registers](../reference/corpus-format.md#registers) ([AD-09](../decisions/0001-foundation.md#ad-09-corpus-formats-and-the-schema-source-of-truth)).
- `fmt` (a later card) and the normal-form check (FB0122) must agree on one writer, so it lives here once.

## Scope

- Files: `crates/fieldbook-core/src/schema/{mod.rs,table.rs,register.rs}`, `crates/fieldbook-core/tests/csv_no_panic.rs`.
- Docs to update: none needed.

## Out of scope

- Typed rows and vocabularies (`core-schema-registers`, `core-schema-claims`).
- Diagnostics and codes. This card returns errors as values; `check-registers` turns them into FB0200 to FB0203.
- The `fmt` verb (`fmt-command`).

## Preconditions

- Cards merged: `core-schema-pages`.

## Steps

1. Write `register::Register` (`Sources`, `Claims`, `Questions`, `Conflicts`, `Glossary`) with `ALL`, `file_name()` (`sources.csv` and so on) and `columns()`, a constant list of `Column { name, kind, need }`. `kind` is one of `id`, `enum`, `text`, `int`, `date`, `pdate`, `decimal`, `bool`, `list`. `need` is `Yes` or `No`, or `Rule` for every column whose Required cell holds a condition ("by kind", "when verified", "strict, day-one" and the like). `Sources` has 28 entries and ends with `doc_kind` (`enum`, `No`) and `series` (`text`, `No`); `Claims` has 35; its `as_of` is `pdate, Rule`, so an empty cell reaches the cross-column check.
2. Write `table::read(&[u8]) -> Result<Table, TableError>` with `Table { header, rows }` and `Row { line, cols, cells }`. A row keeps the line where it starts and, for each cell, the 1-based column where it starts, counted in code points. The reader strips a byte-order mark, accepts CRLF, and reports as errors: bytes that are not UTF-8 (with the offset), an unbalanced quote, a row whose cell count differs from the header (with both counts) and a line break inside a cell.
3. Write `table::write(&Table) -> String`: LF endings, a cell quoted only when it holds a comma, a double quote or a line break, a double quote doubled, and exactly one final newline.
4. Write `table::normalize(register, bytes) -> Result<String, TableError>`. It reads, puts every cell in Unicode NFC, sorts the rows by the register's key (sources by `source_id`, then `version` as a number; claims, questions and glossary by id, byte by byte; conflicts by the number in `conflict_id`) and writes. A row whose key cannot be read keeps its place after the sorted rows. Add `is_normal(register, bytes) -> bool`.
5. Write the test `columns_match_the_reference` (in `schema::register`). It checks the five `columns()` lists against the column tables of the reference, read with `include_str!` from `docs/reference/corpus-format.md`: names, order, kinds, and `Yes`, `No` or `Rule` from the Required cell (`yes`, `no`, anything else). The test also requires the number of entries of each list to equal the column count in the table of registers (28, 35, 13, 11 and 9).
6. Test the reader and the writer in `schema::table`: `write_is_normal_form` (a file saved by Excel, with a BOM, CRLF, a quoted cell and a doubled quote, normalises to the expected text, and a cell is quoted only when it needs it); `version_10_sorts_after_2` (a source's version 10 sorts after its version 2); `xc_100_sorts_after_xc_99`; `read_errors_carry_their_line` (one case for each read error); `write_then_read_returns_the_same_table`.
7. Write `tests/csv_no_panic.rs` with `random_and_mutated_bytes_never_panic`. It includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 128 from `Rng::new(1)`, and takes the Excel-saved example of step 6 with each of its bytes once flipped and once deleted. Each goes through `table::read`, and an `Ok` table goes through `table::normalize` for every register and `is_normal`. The result is a value or a `TableError` with a line of at least 1, a normalised text reads back to itself, and nothing panics.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core schema::table schema::register
cargo nextest run --locked -p fieldbook-core --test csv_no_panic
just check
```

Expected: both succeed; the run includes `columns_match_the_reference`, `write_is_normal_form`, `read_errors_carry_their_line` and `random_and_mutated_bytes_never_panic`.

## Negative control

Swap two entries in `Register::Claims.columns()`. `schema::register::columns_match_the_reference` must fail and name the column. Restore the file. Then make `write` quote every cell: `schema::table::write_is_normal_form` must fail. Restore the file. Then make `table::read` call `unwrap()` where it reads the next character of a quoted cell: `random_and_mutated_bytes_never_panic` must fail. Restore the file. Then drop `series` from `Sources.columns()`: `columns_match_the_reference` must fail and name it. Restore the file.

## Rollback

Revert the merge commit. Nothing outside `fieldbook-core` uses the module yet.

## BLOCKED

A column table in the reference does not match its own count (28, 35, 13, 11 and 9 columns). Write the register and the count and stop; the owner settles which is right (format gate).

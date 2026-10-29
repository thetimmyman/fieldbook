# check-v0

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-core::check` holds the diagnostic model: a `Code` for every row of the check catalogue, ranges 01 to 09, `Severity`, `Diagnostic` and `Report`, and the text form `path:line:col: error[FB0102]: message` with its `help:` line. `cargo xtask docs` fills `docs/reference/errors.md` between its `errors` markers, and a test requires the codes and severities in the code to equal the catalogue in the reference.

## Why

- The codes, severities and known-bad examples are the [Check catalogue](../reference/corpus-format.md#check-catalogue). The form of a diagnostic is in [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience), and the generated documents are in [AD-22](../decisions/0001-foundation.md#ad-22-agentsmd-and-the-docs-set).
- Every later `check-*` card raises codes through this model, so it comes first and alone.
- Ranges 08 (audit, backup and pipeline) and 09 (model, decide, index, output, mcp and chat) are in the catalogue and in the generated `errors.md`. The verbs that raise them are not this card's: it only gives them a `Code`, a severity and a help line. Their known-bad cases are per-code files, `tests/known-bad/<CODE>.toml` ([Check catalogue](../reference/corpus-format.md#check-catalogue)).

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,code.rs,diag.rs}`, `crates/fieldbook-core/src/lib.rs`, `xtask/src/{main.rs,errors.rs}`, `docs/reference/errors.md` (generated).
- Registration lines: `xtask/src/docs.rs` (one row of `GENERATORS`, built by `core-types-and-strings-2`).
- Docs to update: `docs/reference/errors.md` by `cargo xtask docs`. `AGENTS.md` needs no change.

## Out of scope

- Reading a Corpus and raising any code (`check-run` and the cards after it).
- The command line, JSON and exit codes (`check-command`).
- A word of the catalogue itself. This card copies codes and severities; it does not edit the reference.

## Preconditions

- Cards merged: `corpus-mini-fixture`, `core-types-and-strings-2`.

## Steps

1. Write `Code` with one variant for each code of the catalogue, FB0100 to FB0709 and the used codes of ranges 08 and 09 (a number the catalogue leaves free, such as FB0824 or FB0927, has no variant), reserved codes included, from one macro table that also gives each code its catalogue severity text and a one-sentence `help()` (what to do about it). Add `Code::ALL`, `id() -> &str` (`FB0102`), parsing, and `level() -> Level`, where `Level` is `Error`, `Warning` or `Strict` and is the first word of the severity text of the catalogue row: so FB0382 ("warning, from `r2-records`") and FB0607 ("warning on Linux, error in the `windows` job"; the Windows drill makes it an error itself) are `Warning`, FB0380 and FB0381 ("error, from `r2-records`") are `Error`, and a code marked "warning, error from Day 30" is `Warning` now. A code in range 08 or 09 takes its level from its catalogue row in the same way. Add `Code::severity(strict: bool) -> Severity`: a `Strict` code is `Warning`, or `Error` when `strict` is true, as the [Check catalogue](../reference/corpus-format.md#check-catalogue) says.
2. Write `Severity` (`Error`, `Warning`), `Diagnostic { code, severity, path, line, col, message, help }` and `Report { diagnostics }`. `Diagnostic::new(code, strict, path, line, col, message)` takes `severity` from `code.severity(strict)` and `help` from `code.help()`; no pass sets either itself. A finding about a whole file has line 1 and column 1. `Report` sorts its diagnostics by path, line, column and code, counts errors and warnings, and carries `notes`: plain lines for facts that are not findings, such as a check that was skipped.
3. Write the text form: `path:line:col: error[FB0102]: message`, then a line `help: text`, with no indent. Columns count code points. Write `Report::render_text()` with one blank line between diagnostics, then each note as `note: text`, and a last line `N errors, M warnings` (`clean` when both are zero).
4. In `xtask/src/errors.rs`, add the generator `errors` and register it in `GENERATORS`: parse the catalogue tables out of `docs/reference/corpus-format.md` (section titles from "01:" to "09:", rows with a code in the first cell; a row such as "FB0800 to FB0805" is read as one row for each code the catalogue lists), and fill `docs/reference/errors.md` between its `errors` markers: an introduction (how to read a diagnostic, the four exit codes, the text form), then one table for each range with the columns Code, Severity, Check, Known-bad example and Help. In ranges 08 and 09 the example is the file `tests/known-bad/<CODE>.toml`. `cargo xtask docs --check` compares the file.
5. Test in `xtask`, in `mod tests` of `xtask/src/errors.rs` (`errors::tests::catalogue_matches_the_code`): the set of codes in the reference equals `Code::ALL`, and each severity text equals the one in `Code`. Test in core: `severity_follows_the_level` (FB0230 is a warning without `strict` and an error with it; FB0382 is a warning and FB0380 an error either way), ids parse and print back, `ALL` has no repeats, and a rendered diagnostic matches a literal expected string, including a code point count over a non-ASCII line.
6. Run `cargo xtask docs` once and commit `errors.md`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::
cargo nextest run --locked -p xtask errors::
cargo xtask docs --check
just check
```

Expected: every test passes; `docs --check` exits 0 and prints `docs: up to date`; `just check` exits 0.

## Negative control

Change the severity of FB0118 from strict to error in the macro table: `errors::tests::catalogue_matches_the_code` must fail and name FB0118. Restore the line. Then add a variant with no catalogue row: the same test must fail and name the code. Remove the variant.

## Rollback

Revert the merge commit. `errors.md` goes with it and `docs --check` stops looking for it.

## BLOCKED

The catalogue repeats a code or has a row that is not in a range table. Write the line and stop; the reference is the owner's to change (the format gate).

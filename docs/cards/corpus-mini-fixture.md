# corpus-mini-fixture

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`tests/fixtures/corpus-mini` is a small synthetic Corpus that every negative control copies and breaks. Each file in it parses with the types of `fieldbook-core`, and every row and every page is marked synthetic.

## Why

- The controls in [controls.md](../controls.md) plant one violation in a temporary copy of this fixture and expect one error code ([AD-18](../decisions/0001-foundation.md#ad-18-security-and-confidentiality-guardrails)).
- The fixture grows with the checks: each `check-*` card runs its rules over it and fixes the fixture if a rule finds a fault in it. It must be clean under `--strict`. The date of every run over it is the one line of `TODAY`, beside `corpus/`.

## Scope

- Files: everything under `tests/fixtures/corpus-mini/`, and `crates/fieldbook-core/tests/fixture_parses.rs`.
- Docs to update: none. A one-line `README.md` at the root of the fixture says that it is synthetic and that `TODAY` is the date of every run over it.

## Out of scope

- Extraction files and a stored original (`extract-and-locators` adds them).
- Pages of the other types and the calculation fixture (`build-page-types` and `calc-cases` add them).
- Any real source, name or number.

## Preconditions

- Cards merged: `core-schema-claims`.

## Steps

1. Create the tree under `tests/fixtures/corpus-mini/corpus/`: `corpus.toml` with `schema_version = 1`, `[assistant]`, two areas (`cmp` with the subarea `business-systems`, `est` with `pricing`) and one view; `AGENTS.md` and `ASSISTANT.md` (`rules_version = 1` on its first line) of a few lines; an empty `scan-allow.toml`; `changes.md` with no sections; and the five registers in normal form, each with every column of `Register::columns()` (the 28 columns of `sources.csv`; `doc_kind` and `series` stay empty, as the repository Corpus requires). Beside `corpus/`, add the file `tests/fixtures/corpus-mini/TODAY`: one line `YYYY-MM-DD`, on or after every `as_of` and on or before every `review_by` of the fixture. Choose it after the registers and pages are written. A card that adds dated rows keeps that true, and moves the line only if no date satisfies both bounds. The known-bad harness (`check-known-bad`) and every run of the program over the fixture or a copy of it take their date from this file.
2. Registers. Sources: one synthetic primary source of kind `primary_law` and tier `A` (title "Synthetic Cost Rule, made up for tests", `licence = "Synthetic, no restrictions"`, `location = "metadata-only: made up for tests"`), and a second source with two versions where `v2` supersedes `v1`. Claims: one unchecked consequential fact with a locator and a quote; one assumption; one fact verified by the owner; one superseded claim and its replacement. Questions: one `day_one` question with cells 4 to 8 filled, cells 11 to 13 empty and `status = open` and one ordinary question. Conflicts: one unresolved conflict with its question. Glossary: four terms, one of them with an alias. Every id is unique and every reference resolves.
3. Pages under `corpus/pages/`: a `concept` page (`cmp`) that cites two claims and lists two terms, a `procedure` page (`cmp`) with a prerequisite, and a `brief` page (`est`) with the six brief parts. Each has all required front matter and `status = "draft"`. The concept and procedure pages have the eight parts in order, a Change note whose first entry is `NEW_KNOWLEDGE` and a Worked example that begins with the synthetic label. The brief has only the six brief parts, in order, and no Change note.
4. Write `crates/fieldbook-core/tests/fixture_parses.rs`. It reads each fixture file with `include_str!`. `every_register_row_parses` requires zero problems from `CorpusToml`, from `PageMeta`, and from every row of the five registers; a failure names the register, the line and the column. It also requires the counts: 2 areas, 3 pages, the number of rows in each register. `registers_are_in_normal_form` requires that each register is already in normal form (`is_normal`). Add `today_is_a_civil_date`: `TODAY` holds exactly one line and one final newline, and `CivilDate::parse` reads the line.

5. Add a synthetic source representing trade_press with authority_tier D, explicitly labelled as invented fixture text. It is an eligible public kind in the source-vocabulary table, not a synthetic_fixture/D combination. The fixture must pass register validation; handoff-context later checks default exclusion and explicit inclusion, and handoff-cite-check reports it weak.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test fixture_parses
just check
```

Expected: both succeed.

## Negative control

Empty the `title` cell of one source row. `fixture_parses::every_register_row_parses` must fail and name the register, the line and the column. Restore the file. Then swap two glossary data rows with distinct term ids, preserving their bytes and the header: `fixture_parses::registers_are_in_normal_form` must fail. Restore the file. Then write `2025-02-30` in `TODAY`: `fixture_parses::today_is_a_civil_date` must fail. Restore the file.

## Rollback

Revert the merge commit. Nothing else reads the fixture yet.

## BLOCKED

A rule of the reference cannot be met by synthetic data, for example because two rules contradict each other for one row. Write both rules and stop.

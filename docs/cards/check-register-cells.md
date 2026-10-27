# check-register-cells

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` checks each cell of the five registers and the rules across columns, and raises FB0114 (an invalid calendar date), FB0204 (a cell outside its type, vocabulary, form or length), FB0205 (an empty required cell), FB0206 (a broken rule across columns) and FB0210 (an overlay-only value). The rules across columns are one table in the code, and a test holds that table to the reference.

## Why

- The codes are in [02: registers and sources](../reference/corpus-format.md#02-registers-and-sources). The column tables and the "Rules across columns" of each register are in [Registers](../reference/corpus-format.md#registers), from [sources.csv](../reference/corpus-format.md#sourcescsv) to [glossary.csv](../reference/corpus-format.md#glossarycsv). The overlay values are marked in [Status vocabularies and promotion rules](../reference/corpus-format.md#status-vocabularies-and-promotion-rules).
- `core-schema-registers` and `core-schema-claims` return one `CellProblem` per bad cell and leave the codes to this card.
- Several rules across columns carry another card's code. Listing them all in one table, with the pass that owns each, is how no rule is forgotten.

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,model.rs,register_cells.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports register cells and the rules across columns (FB0114, FB0204 to FB0206, FB0210).`). `errors.md` does not change.

## Out of scope

- FB0306 and FB0307, the locator and quote of a consequential claim (`check-claim-cells`).
- References, supersession, originals and repeated glossary labels: FB0207, FB0208, FB0211 and FB0214 (`check-register-links`).
- Promotion, dates and day-one questions: FB0209, FB0213, FB0220 to FB0222 and FB0230 (`check-promotion`).
- `handling` other than `public`, and text cells that start with a formula character: FB0410 and FB0403 (`check-scans`). The decimal grammar of `value`: FB0500 (`calc-cases`).

## Preconditions

- Cards merged: `check-registers`.

## Steps

1. Add typed rows to the model: run each register's `from_row` on the rows `check-registers` kept, and keep either the typed row or its `CellProblem`s, with the row's line and each cell's column. Later passes read only typed rows. In `check/model.rs`, add `pub struct Located<T> { pub line: u32, pub value: T }` and `Model::claims(&self) -> &[Located<ClaimRow>]`, `sources`, `questions`, `conflicts` and `glossary` with the corresponding row type, in register order. A row with a CellProblem is absent. Test `typed_accessors_keep_register_order_and_skip_bad_rows`.
2. Add `register_cells.rs` with one pass. Map `CalendarDate` to FB0114, `Empty` to FB0205 and `Form`, `Vocabulary` and `TooLong` to FB0204, except: a value marked overlay (`is_overlay()`) is FB0210; an empty cell whose Required entry is a condition is decided by the rule that states the condition, not by FB0205; an empty or malformed `handling` cell is kept and raises neither FB0205 nor FB0204 here (FB0410 judges it). A claim's `area` that differs from its id's area letters is FB0204. The `source_role` values the Corpus never uses are FB0204.
3. Write the rules across columns as one table: one entry for each row of a "Rules across columns" table and each rule sentence under a register, with each predicate's code and owning pass; a row may have more than one predicate. A claims predicate with no explicit code is FB0206, as its caption says. This pass implements the FB0206 predicates, including Grade G notes (literal `replace by ` followed by non-whitespace date or event text; no date parser for free-text events), missing fact/interpretation source_id, version or locator_kind, a nonempty locator for no_fine_locator, superseded_by without superseded adoption, and empty claim `as_of` unless `verification = stale` and `note` begins `as_of unknown`, and the FB0204 part of the sources tier rule. It also implements the questions FB0210 rule: in the repository Corpus any nonempty `answer`, `answered_on` or `adoption`, or `status = answered`, is refused, even if each cell parses. Entries for the codes of other cards are listed with the card's slug and do nothing yet; the sources row marked Convention is listed and does nothing.
4. Test `check::register_cells::rules_match_the_reference`. It reads `corpus-format.md` with `include_str!`. For `sources.csv` and `claims.csv`, the number of table rows equals the number of entries for that register. For every register, the set of codes the reference names in its rules equals the set in the code. A failure names the register.
5. Unit tests named `check::register_cells::...`: one finding for each code, one test for each FB0206 entry with a breaking row and a passing row, and a row whose cell already failed FB0204 that raises no FB0206 from the same cell.
6. Move FB0114, FB0204, FB0205, FB0206 and FB0210 from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture. FB0114 plants `2025-02-30` in a source row's `retrieved` cell; it needs no page pass. If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request.

7. In `check::register_cells`, add `stale_claim_with_unknown_as_of_is_valid`, `empty_as_of_without_stale_reason_is_fb0206` (each missing condition), `question_company_cells_are_fb0210` (one plant for each of the four fields), `question_empty_answer_cells_are_valid`, `impossible_register_date_is_fb0114`, `partial_full_date_is_fb0114` and `partial_claim_date_is_valid`. The tests call the pass through `check::run`; a registry row alone cannot satisfy them.

8. Through `check::run`, add `fact_and_interpretation_need_source_cells` (each missing cell for both kinds, plus valid assumptions), `no_fine_locator_requires_empty_text` (unchecked and non-consequential) and `superseded_claim_requires_superseded_adoption` (valid reciprocal links isolate adoption). Require FB0206 for each isolated fault and none for the valid counterpart.

9. Add `grade_g_requires_replacement_note` through `check::run`: valid date and event notes pass; empty note, missing literal prefix and empty/whitespace-only suffix fail FB0206. Non-G counterparts do not require this prefix. Add `discovery_tier_matches_kind`: every public kind permits D as the source-vocabulary table states, while synthetic_fixture and company kinds remain untiered; disallowed combinations still fail.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::register_cells
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, including `rules_match_the_reference` and five new known-bad cases; `check` on the real Corpus prints `exit 0`; `just check` exits 0.

## Negative control

Remove the entry for "`value` is set" from the claims rules in the code. `check::register_cells::rules_match_the_reference` must fail and name `claims.csv`. Restore it. Then make the overlay test return false: `known_bad::each_case_raises_its_code` must fail for FB0210. Restore it.

Drop the first valid row from an accessor: `typed_accessors_keep_register_order_and_skip_bad_rows` must fail. Restore it.

Disable the question rule or the unknown-date condition: `question_company_cells_are_fb0210` or `empty_as_of_without_stale_reason_is_fb0206` must fail. Map `CalendarDate` to FB0204: `impossible_register_date_is_fb0114` must fail. Restore each.

Disable each of the three source-cell, no-fine-locator-text and adoption predicates in turn: its named check::run test must fail. Restore each.

Disable the Grade G predicate: `grade_g_requires_replacement_note` must fail. Remove its registry entry: `rules_match_the_reference` must fail. Allow D on synthetic_fixture or a company kind: `discovery_tier_matches_kind` must fail. Restore each.

## Rollback

Revert the merge commit. The five codes go back to `pending.toml`, and the model no longer holds typed rows.

## BLOCKED

- The lead's committed registers raise one of these codes: write the code, the file and the line and stop. Codex may not edit `corpus/` before `guard-statuses`.
- A predicate has no code/owning pass, or two predicates contradict each other for one row: write both and stop (format gate).

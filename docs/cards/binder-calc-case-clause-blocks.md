# binder-calc-case-clause-blocks

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A calc, a case and a clause-card page in `day-one.html` carry the block their standalone page prints (the calc table, the Public record and Inference boxes, the clause questions chips), so the binder article holds what `p/<id>.html` holds for that type, and the standalone pages are byte-identical to today.

## Why

- The need. A binder article of a non-brief page renders the same six parts as a checklist or a reference and nothing type-specific except the checklist timeline and the reference rule. A calc page's worked example appears without its table, a case page's How it works appears as plain prose without its record and inference split, and a clause card appears without the questions that settle its applicability. The binder is the print of the whole curriculum (`day-one-binder-and-registers-xlsx`, whose review listed this), so a printed binder lacks content the page has.
- The standalone blocks are in three places: `types::render_typed_eight_part` appends `calc_table::calc_table(&page.body)` to the calc page's Worked example; `templates/types/case.html` holds the `case-split` markup inline, filled from `types::build_case_split`; and `templates/types/clause_card.html` holds the `clause-questions` chips inline, filled from `types::build_clause_calibration_questions`, and wraps the rule in `div.clause-card-rule`. The binder in `day_one.rs` calls none of them.
- [corpus-format.md](../reference/corpus-format.md) page types table: `case` has `How it works` in two subsections, `### Public record` then `### Inference`, "the record and the inference kept apart". [HANDOFF.md](../design/HANDOFF.md) section 12 (print contract): the binder prints the page. The design gives the blocks; nothing new is invented here.
- Lead decision: one template per block, shared by the standalone page and the binder (`parts/case_split.html` and `parts/clause_questions.html`, beside `parts/calc_table.html`, which already is). The standalone templates `include` them, so the two renderings cannot drift. The standalone output is byte-identical, so no snapshot changes; if a `page_types` snapshot differs, the partial's whitespace is wrong and the card is not done.
- Lead decision: in the binder the case subheads are `h4`, not `h3`. The binder's `h3` headings are the six parts (the test `binder_articles_carry_the_whole_page` pins them to `strings::parts()`), and a case's subheads sit inside How it works. The partial takes the heading level as a parameter (`h3` standalone, `h4` binder) and `app.css` gets the `h4` selector beside the `h3` one, with the declarations unchanged.
- Lead decision: binder ids go through `prefix_ids` (`b-<id>-`), as every other binder block does, so the subhead anchors stay unique across the 28 articles and `cite` and `fb:` ids do not collide.
- Lead decision: the clause-card rule keeps its `clause-card-rule` wrapper in the binder, since the wrapper is part of what the standalone page prints.
- Lead decision: the binder holds the `p/<id>.html` block for the same page, not a binder-only summary. Where the binder omits other things (the area, subarea and handling chips and the checklist-groups wrapper, noted in the review of `day-one-binder-and-registers-xlsx`) this card leaves them: they are a different follow-up.
- Design: [MAP.md](../design/MAP.md) rows `CalcTable (worked example)` and `#page-<id>` (the part set); [HANDOFF.md](../design/HANDOFF.md) section 5 (components) and section 12; token groups: none added. Existing rules `.case-split`, `.case-block`, `.clause-questions` and `.calc` carry over; only the screen rule's heading selector gains `h4`. No new token, string or component.
- The three fixture pages (`est-calc-01` at stage 3, `cmp-withhold-case` at stage 4, `cmp-clause-9900-101` at stage 5) are outside `binder_pages` (briefs and stages 1 and 2 only), so the tests set the page's stage to 1 in a test copy of the fixture tree; the fixture files do not change.

## Scope

- Files: `crates/fieldbook-render/src/day_one.rs`, `crates/fieldbook-render/templates/types/case.html`, `crates/fieldbook-render/templates/types/clause_card.html`, `crates/fieldbook-render/templates/parts/case_split.html`, `crates/fieldbook-render/templates/parts/clause_questions.html`, `crates/fieldbook-render/assets/app.css` (the `.case-head h3` screen selector only), `crates/fieldbook-render/tests/print.rs`, `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Changed: the day-one binder carries the calc table, the case split and the clause questions of its pages.`).

## Out of scope

- The area, subarea and handling chips and the checklist-groups wrapper in binder articles, and the Templates list repeating the briefs.
- Page types other than calc, case and clause-card.
- The one-sheet fit of the briefs and the footer text (`binder-brief-one-sheet`, `frame-page-id-hook`).
- The fixtures: the stage override is in the test.

## Preconditions

- Cards merged: `frame-page-id-hook` (`crates/fieldbook-render/tests/print.rs`; it runs first). `build-page-types` and `day-one-binder-and-registers-xlsx` are merged on origin/main.
- `views-and-day-n-2` (shared paths `crates/fieldbook-render/src/day_one.rs` and `crates/fieldbook-render/assets/app.css`) follows this card in the plan and names it in its Cards merged. `binder-brief-one-sheet` follows this card.

## Steps

1. In `day_one.rs`, add `fn render_case_split(view: &CaseSplitView, heading_level: u8) -> String` and `fn render_clause_questions(chips: &[String]) -> String`, each with its Askama struct rendering its partial (infallible like its siblings, `unreachable!` on error). `types.rs` is not edited: `build_case_split` and `build_clause_calibration_questions` are already `pub`. Lead decision: the functions live in `day_one.rs`, not `types.rs`, because only the binder calls them and the standalone templates `include` the partials directly, so `types.rs` (also in the Files of the draft `handoff-new-note`) stays out of this card.
2. Move the `div.case-split` markup of `case.html` into `parts/case_split.html` and the `div.chips.clause-questions` markup of `clause_card.html` into `parts/clause_questions.html`, and `{% include %}` them where they were, with the level `3` for the case heading. Run the `page_types` and `page` tests: the three snapshots must not change.
3. In `day_one.rs`, for a `Case` page, build the How it works part from `build_case_split` and `render_case_split(.., 4)` instead of `build_part_by_name` when `has_sections` is true (the prefix prose before the first subhead stays first, as on the standalone page); for a `Calc` page, append `calc_table::calc_table(&page.body)` to the Worked example body, as `render_typed_eight_part` does; for a `ClauseCard` page, wrap the rule in `div.clause-card-rule` and append `render_clause_questions(&build_clause_calibration_questions(OUTPUT_PATH, page, view, planned))` to Company calibration. Every one of these strings goes through `prefix_ids(.., page.id.as_str())` like the part body.
4. In `app.css`, make the screen rule that names `.case-head h3` name `.case-head h4` too, with the declarations unchanged (no print rule names it).
5. Tests in `tests/print.rs`, each on a test copy of the fixture tree with the page's `stage = N` line replaced by `stage = 1` so the binder lists it: `binder_calc_article_holds_the_calc_table` (the article of `est-calc-01` holds the same `<figure class="panel calc">` block that `p/est-calc-01.html` holds, apart from the id prefix, and holds the caption text), `binder_case_article_holds_the_split` (the article of `cmp-withhold-case` holds `case-record` and `case-inference` boxes, the two subheads as `h4` with `b-cmp-withhold-case-` anchors, and its h3 headings are still the six parts), `binder_clause_article_holds_the_questions` (the article of `cmp-clause-9900-101` holds `clause-questions` with the question chips of the standalone page, and the `clause-card-rule` wrapper). Each also asserts that the standalone page still holds the block, and that ids are unique across the whole binder.
6. Run `just check`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test print binder_calc_article_holds_the_calc_table
cargo nextest run --locked -p fieldbook-render --test print binder_case_article_holds_the_split
cargo nextest run --locked -p fieldbook-render --test print binder_clause_article_holds_the_questions
cargo nextest run --locked -p fieldbook-render --test print --test page_types --test page --test tokens
just site && test -s out/edition/day-one.html
just check
```

Expected: every test command passes with no snapshot change (`git status --short crates/fieldbook-render/tests/snapshots` prints nothing); `just site` and the `test` exit 0; `just check` exits 0.

## Negative control

For each type, remove its block from the binder call in `day_one.rs` (the calc append, the case split, the questions): that type's test must fail with the missing block, and the other two must keep passing. Restore the file. Then change the partial's heading level argument from 4 to 3: `binder_case_article_holds_the_split` must fail (the h3 headings are no longer the six parts). Restore it.

## Rollback

Revert the merge commit. The binder article of those three types holds the six parts without the blocks again, and the standalone templates hold their markup inline again.

## BLOCKED

A precondition has not merged: comment `BLOCKED: binder-calc-case-clause-blocks waits on <slug>`, push the branch and take the next card. A standalone snapshot changes and the whitespace cannot be matched: comment `BLOCKED: binder-calc-case-clause-blocks changed a standalone page` and stop.

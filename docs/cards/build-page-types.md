# build-page-types

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Each page type gets its extra block on `p/<page-id>.html`: a `brief` renders as a BriefSheet with its six parts and the sources block after them; a `checklist` renders its numbered tasks and a PlanTimeline over its relative-day windows; `procedure`, `reference`, `case` and `clause-card` get the blocks HANDOFF gives them. The StageTrack part shows the eight curriculum stages from the `stage` field, with no reading progress. MKT pages render as ordinary pages.

## Why

- The page types, their required content, the `brief` parts and the `stage` field are in [Page files](../reference/corpus-format.md#page-files) and [Strings](../reference/corpus-format.md#strings). Output paths are in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types). MKT subareas are in [Areas and views](../reference/corpus-format.md#areas-and-views).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) rows `#brief-<id>` (BriefSheet), `#plan` (PlanTimeline), `#learn` (StageTrack) and `#competitor-<id>` (the MKT pages only). [HANDOFF.md](../design/HANDOFF.md) [§2 Routes](../design/HANDOFF.md#routes) (`#plan`, `#learn`) and [Page types and what the reader adds for each](../design/HANDOFF.md#page-types-and-what-the-reader-adds-for-each); [§5 Components](../design/HANDOFF.md#5-components) (BriefSheet, PlanTimeline, StageTrack) and [Charts](../design/HANDOFF.md#charts); [§8 Responsive behaviour](../design/HANDOFF.md#8-responsive-behaviour); [§12 Implementation notes](../design/HANDOFF.md#12-implementation-notes) (Testing). Token groups: colour (`--chart-1`, `--chart-neutral`, `--color-warn`), type, space, shape.

## Scope

- Files: `crates/fieldbook-render/templates/types/{brief.html,checklist.html,reference.html,case.html,clause_card.html}`, `crates/fieldbook-render/templates/parts/{plan_timeline.html,stage_track.html}`, `crates/fieldbook-render/src/{lib.rs,page.rs,types.rs,plan.rs,stages.rs}`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/page_types.rs`, `crates/fieldbook-render/tests/snapshots/` (new snapshots), `tests/fixtures/corpus-mini/corpus/pages/` (a `checklist`, a `reference`, a `case`, a `clause-card` and a `template` page, all synthetic), `crates/fieldbook-core/tests/fixture_parses.rs` (the page count only), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: briefs, checklists with their timeline, and the other page types.` under `## Unreleased`).

## Out of scope

- The print contract, the one-page check and `day-one.html` (`day-one-binder-and-registers-xlsx`). CalcTable on `calc` pages (`build-charts`).
- The today line and "Set your start date" (Day 30). The proof-test and progress rows of PlanTimeline: no Corpus field holds them yet.
- CapLadder and CompareMatrix (no Corpus field for a capacity stage; the CPT view is Day 30). Any reading progress or done mark.

## Preconditions

- Cards merged: `build-page`.

## Steps

1. `types.rs`: pick the type's template from `type` and pass the extras to it. Every type keeps the PageHeader, MetaRow and rail of `build-page`.
2. `brief`: BriefSheet with the six parts in the order of the `brief_parts` string, then the sources block. An unfilled part (one that holds the `empty_part` string) shows in italic warn, never hidden.
3. `checklist`: the task items as numbered steps under their `###` group headings. `plan.rs` reads each group heading that names a relative-day window and renders PlanTimeline in its unset variant: one row of windows on a shared day axis, as inline SVG with no `<text>`, no `style` attribute and `role="img"` with a label. A group heading is a window when it is exactly `Day N to Day M` (N and M integers, N at most M); any other heading, including `Day 30 to Day 1`, `Day 1-30` and `Days 1 to 30`, adds no row.
4. `procedure`: the ordered list as numbered steps. `reference`: apply the reference-table convention in the permanent page-type specification; do not infer a date or supersession from arbitrary cells. `case`: Public record and Inference kept apart, each with its evidence label. `clause-card`: the six-row table first, and Company calibration listing its question chips.
5. `stages.rs` and `stage_track.html`: StageTrack with eight columns from the `stage` field of every page, each column linking its pages, and no progress marks. The part is placed on the pages the lead names as curriculum stage pages; until then it is built and tested only.
6. `app.css`: rules for these blocks with the existing tokens. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF.
7. Add five strict-clean fixture pages: checklist, reference, case, clause-card and template (with the required fenced text block). Raise the fixture page count by five.
8. Tests in `tests/page_types.rs`, `mod page_types`: `brief_parts_in_order_then_sources`; `unfilled_brief_part_in_warn`; `checklist_windows_on_one_axis`; `window_grammar_accepts_day_1_to_day_30`; `window_grammar_refuses_reversed_and_other_forms` (`Day 30 to Day 1`, `Day 1-30` and `Days 1 to 30`); `timeline_svg_has_no_text_or_style`; `reference_rows_newest_first`; `case_keeps_record_and_inference_apart`; `stage_track_has_eight_columns_and_no_progress`; `mkt_page_renders_as_a_page` (a synthetic model with an MKT page in `peers`). Golden renders with `insta`: `brief_page_renders`, `checklist_page_renders`, `reference_page_renders`, `case_page_renders`, `clause_card_page_renders` and `template_page_renders` from the fixture. The snapshot tests call `links` and `nav` with a fixed `PlannedPaths::of(...)` (each `p/<page-id>.html` of the fixture and nothing else) and never read `site::planned_paths`, so a later card that registers a path cannot change them. `every_page_type_has_a_render_golden` requires `<type>_page_renders` for every `PageType::ALL` entry except `const GOLDEN_LATER: &[PageType] = &[PageType::Calc]`. Existing snapshots count. `build-charts` removes Calc and records its golden when CalcTable exists. A missing case outside this explicit list fails and names the type.

9. Add `reference_rows_follow_claim_facts`: exactly one distinct claim citation supplies its as_of and superseded_by; repeated citations of that same claim are still one. Sort newest first only when every row has a known date, using first-day comparison and stable authored order on ties. Any undated or ambiguous row leaves the whole table in authored order; rows lacking a unique claim receive no inferred status. Test dated, undated, ambiguous and equal-date tables. No required column or new validation rule is introduced.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render page_types::
cargo nextest run --locked -p fieldbook-core --test fixture_parses
cargo nextest run --locked -p fieldbook build::
cargo nextest run --locked -p fieldbook --test known_bad
cargo nextest run --locked -p xtask literals::
just site
! grep -L 'http-equiv="Content-Security-Policy"' out/edition/p/*.html | grep .
! grep -l -E 'style="| on[a-z]+=|<text' out/edition/p/*.html
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
just check
```

Expected: the tests pass, all six snapshots included, and the fixture stays strict-clean; `just site` exits 0; the guarded lines exit 0; `just check` exits 0.

## Negative control

Remove the Unknowns part from `brief.html`. `page_types::brief_parts_in_order_then_sources` must fail and name Unknowns, and the `brief_page_renders` snapshot must differ. Restore the file. Then add a `<text>` label to the timeline SVG: `page_types::timeline_svg_has_no_text_or_style` must fail. Restore the file.

Remove the template snapshot: `every_page_type_has_a_render_golden` must fail and name template. Restore it.

Sort an undated table or infer status from an ambiguous citation row: `reference_rows_follow_claim_facts` must fail. Restore each.

## Rollback

Revert the merge commit. Every type renders as a plain eight-part page, and the fixture loses its five pages. Revert `day-one-binder-and-registers-xlsx` first if it has merged.

## BLOCKED

- A fixture page of a new type cannot be strict-clean without a register row an earlier card did not add: name the code and add the row only if it is synthetic and in `tests/fixtures/corpus-mini/`.
- The lead has not said how a curriculum stage page is marked, and the StageTrack has nowhere to go: build the part, name the gap in the pull request description, and finish the rest of the card.
- HANDOFF gives a type's block a field the Corpus does not hold: leave it out, name it, and ask the lead.

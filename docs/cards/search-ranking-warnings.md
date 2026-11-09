# search-ranking-warnings

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

`fieldbook check` on the real Corpus reports no `FB0127`: the regression set judges each case on the pages a reader is shown, and each case that still misses is fixed by Corpus content, never by editing a case and never by tuning the ranking.

## Why

- The measurement. `fieldbook check --today "$(git log -1 --format=%cs)"` on the real Corpus reports 17 `FB0127` warnings of the 27 cases (`rg-001`, `rg-003` to `rg-016`, `rg-019` and `rg-023`). Each is a page that the case expects (`expect` ids are page ids). Their ranks in the flat unit list are 96, 329, 1144, 9, 24, 22, 68, 35, 303, 112, 7658, 249, 1412, 746, 11 and 22, and `ctf-financing-overview` of `rg-014` is not in the list at all. A claim row, a section row or a term row of the right area ranks first in the cases sampled: for `rg-004` the first seven rows are `CL-CTF-0951`, `CL-CTF-0002` and other claims about the customary progress payment rate for small business, which is the right answer, and the page `ctf-progress-payments` is at rank 1144.
- The probable cause is the judged list, not the content: one case was sampled, and Step 1 measures all 17. [search.md](../reference/search.md#the-page-view) says two thirds of the units are claim rows, and that the regression set "still judges the unit list". The Corpus holds about 28,800 claims against about 1,700 pages, so a claim that quotes the query words outranks the page that holds them. The edition site never shows those claims as results ([The site index](../reference/search.md#the-site-index)), and the command line shows page rows first, so no reader sees the list the set judges.
- `rg-014` shows a second effect. `fieldbook search --units performance-based payments` finds 6,045 matching units, and `ctf-financing-overview` is not one of them, although the page text says "performance-based payments" several times. The case's expected id is the page lead unit, so a page whose words sit only in its sections misses. A check on the page view, which collapses a section into its page, counts the page.
- The rule of the set is in [search.md](../reference/search.md#regression-set): "nobody tunes the ranking to pass a case", and an agent may not edit an existing case ([AGENTS.md](../../AGENTS.md#hard-stops)). `FB0127` has its fix text in [errors.md](../reference/errors.md): check the case and the Corpus content, and a person decides whether to change the case. This card therefore changes what the set measures and the Corpus, and leaves `corpus/eval/regression.toml` as it is. It is `Review: owner` because it changes what the Day-1 regression gate judges, and the owner decides that.
- Design: none. The card builds nothing a reader sees.

## Scope

- Files: `crates/fieldbook-core/src/eval.rs`, `crates/fieldbook-core/src/check/regression.rs`, `crates/fieldbook-core/src/check/mod.rs` (only if the pass needs the page view), `docs/reference/search.md` (the page-view paragraph and the Regression set section), `docs/reference/corpus-format.md` (the `FB0127` row), `docs/reference/errors.md` (generated), `corpus/pages/` (title, heading and lead-paragraph wording of the pages of the cases that still warn after Step 4, at entry values and nothing else) and `CHANGELOG.md`.
- Registration lines: none.
- Docs to update: `search.md`, the `FB0127` row of `corpus-format.md`, `errors.md` through `cargo xtask docs`, and `CHANGELOG.md` (`- Changed: the regression set judges the page view of search, not the flat unit list.`).

## Out of scope

- Any edit of `corpus/eval/regression.toml`, and any `critical` mark: a person owns both.
- Any change to scoring, tokens, weights or synonyms in `fieldbook-core::search`, or to `search.js` and its goldens.
- Raising `verification`, `applicability` or `adoption` of any claim, and writing `verified_on`.
- The `FB0117`, `FB0221` and `FB0309` warnings. They are about review dates and figures, not ranking.

## Preconditions

- Cards merged: `search-page-view`, `search-cli-and-regression`.
- The owner accepts the change of what the set judges, on the work item or in the pull request (Step 3 says what is asked).

## Steps

1. Measure, in a scratch test that is not committed. For all 27 cases, record the rank of the expected page in the flat unit list (the current figure) and its position among the page rows of `search::page_view` over the same ranked list, so that no case that holds today, a `critical` one above all, would miss under the new judgement. Record, for each case still outside `top` page rows, the page rows above it and why they outrank it (a title match, a shorter page, more matching sections). Put the table in the pull request.
2. Ask the owner on the pull request whether the set judges the page view. Say what changes: an expected id holds when its page is within the first `top` page rows, and a section id of the form `<page-id>#<anchor>` counts for its page; an exact row counts first; `expect_none` is unchanged. Do not start Step 3 before the answer.
3. In `eval.rs`, add the judgement on the page view (a function beside `miss` that takes the `Index` and the `Case`) and call it from `check/regression.rs`. Keep the flat unit list available for `fieldbook search --units`, which does not change. Keep the diagnostic text of `FB0127` and its help. Add unit tests in `mod tests` of `eval.rs`: `a_page_whose_sections_rank_first_holds`, `a_page_below_top_page_rows_misses_and_names_its_rank` and `expect_none_is_unchanged`. Add `mod tests` to `check/regression.rs`, built on the mini Corpus the way the tests of `check/pages.rs` build it (`Tree`, `load`, `run`) with a one-case regression file: `a_case_the_page_view_misses_raises_fb0127` and `a_case_the_page_view_holds_raises_nothing`.
4. Amend `search.md` where it says the set judges the unit list (the page-view paragraph and the Regression set section) and the `FB0127` row of `corpus-format.md`. Run `cargo xtask docs`.
5. Run `fieldbook check --today "$(git log -1 --format=%cs)"` on the real Corpus. For each case that still warns, change the Corpus and not the case: make the page's `title`, a heading or the first paragraph of its lead say the words a person types (for `rg-014`, "performance-based payments" in a heading of `ctf-financing-overview`), or add the missing page. Entry values only; no status change. One pull request per about 400 to 600 changed lines.
6. If a case still warns after content work, stop for that case: comment `BLOCKED: <case id>, <rank among page rows>, <what was tried>` on the work item and leave the case for the owner. Do not edit it.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core eval::
cargo nextest run --locked -p fieldbook-core check::regression::
cargo run -q --locked -p fieldbook -- check --today "$(git log -1 --format=%cs)" --json | grep -cE '"code": "FB012[78]"'
cargo xtask docs --check
just check
```

Expected: the tests pass (the second command runs the two tests of `check/regression.rs`); the third command prints `0` (grep exits 1 for no match, which is the expected result), or names, as `BLOCKED` on the work item, each case that remains; `docs --check` exits 0; `just check` exits 0.

## Negative control

On a temporary copy of the Corpus, replace the five words of the `rg-004` query (`progress`, `payment`, `rate`, `small`, `business`) by a made-up word everywhere in `ctf-progress-payments`: title, lead and every section, since the page view counts a page by its sections as well. `fieldbook check` on the copy must report `FB0127` for `rg-004` (ignore other findings of the copy; deleting the page would give `FB0126`, not `FB0127`). Restore the copy. Then make the judgement count the rank of the first unit of the page instead of its best row in the page view: `a_page_whose_sections_rank_first_holds` must fail, naming the case. Restore it.

## Rollback

Revert the merge commit. The set judges the flat unit list again, the 17 warnings return, and the Corpus wording stays.

## BLOCKED

The owner does not accept the change of what the set judges, or a case still misses after the Corpus content is fixed: comment `BLOCKED: <what>` on the work item with the case id and the measured ranks, and take the next card.

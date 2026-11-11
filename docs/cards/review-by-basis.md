# review-by-basis

- **Runs where:** home, CI Linux
- **Schema impact:** breaking
- **Review:** owner

## Outcome

A page's computed `review_by` counts its interval from the later of its `as_of` and the date of its newest Change note entry, under an accepted ADR 0002 amendment, so a page written from an older source is not overdue the day it is written.

## Why

- The need. 665 of 1,709 pages show "Review overdue" on load. [Dates](../reference/corpus-format.md#dates-supersession-handling-and-labels) says "A page's `review_by` is its `as_of` plus the interval of its volatility", and `as_of` is "the date the page's content describes", which must not be later than the oldest `as_of` of the claims it cites (FB0118). So a page about a rule dated 2026-01-20 reads overdue 90 days later, whenever it was written: `cls-government-property-clause` was drafted on 2026-10-29 (its `NEW_KNOWLEDGE` entry), changed on 2026-10-31, and has been overdue since 2026-04-20.
- `review_by` answers "when should a person look at this page again", and the last time a person or an agent changed the page is the Change note: every page has one, its oldest entry is `NEW_KNOWLEDGE` (FB0137) and each entry carries a date (FB0114). Taking the later of `as_of` and the newest entry date keeps an old page that nobody touched overdue, and stops a new page being overdue at birth. It reads only what the page already holds, so no file needs rewriting and no field is added.
- The format is frozen: ADR 0002 is Accepted, and [Scope and versioning](../reference/corpus-format.md#scope-and-versioning) lists "a meaning changed" as breaking. The header says `breaking` for that reason, although no valid file needs rewriting. The precedent is the five-digit claim continuation, an ADR 0002 amendment folded into `schema_version = 1` before `edition-001`; the owner decides whether this one is folded the same way. This is the format gate, so the card is `Review: owner`.
- A claim's `review_by` (its `as_of` and `verified_on`) is unchanged: a claim is re-checked against its source, and `verify` already moves it.
- Where the notice shows on an unreleased build is `overdue-banner-fold`'s, and it works under either rule.
- Design: none. The card changes a computed date, not what a reader sees beside it.

## Scope

- Files: `crates/fieldbook-core/src/review.rs`, `crates/fieldbook-core/src/check/citation_rules.rs`, `docs/decisions/0002-corpus-format-v1.md` (a new amendment section), `CHANGELOG.md`.
- Registration lines: `docs/reference/corpus-format.md`, the page `review_by` bullet of Dates and the `review_by` row of the computed fields table; `crates/fieldbook-render/src/page.rs`, the one `page_review_by` call; `crates/fieldbook-render/tests/page.rs`, the `review_by_is_computed` expectation.
- Docs to update: the two `corpus-format.md` lines and the amendment; `CHANGELOG.md` (`- Changed: a page's review date counts from its last change note as well as its as-of date.`).

## Out of scope

- The claim rule, `FB0221` and `verify`.
- The interval of each volatility class.
- Any change to a page file or its Change note.

## Preconditions

- The owner accepts the ADR 0002 amendment of Step 1, in this card's pull request or before it.
- Cards merged: none.

## Steps

1. Write the amendment in `docs/decisions/0002-corpus-format-v1.md`: the rule, why, the schema impact and whether it folds into schema version 1, and the measured count of overdue pages before and after at the commit date. Stop for the owner's acceptance (see BLOCKED).
2. In `review.rs`, a function that reads the newest valid Change note entry date of a page body, and `page_review_by` taking it: the base is the later of `as_of` and that date, or `as_of` when the page has no valid entry.
3. Pass the date at the two call sites (`check/citation_rules.rs` for `FB0117`, `render/src/page.rs` for the page).
4. Tests in `review.rs`: `review_by_counts_from_newest_change_note`, `review_by_keeps_as_of_when_later`, `review_by_ignores_invalid_entry_dates`; update `review_by_is_computed` in `fieldbook-render`.
5. Record in the pull request the `FB0117` count of `fieldbook check` at the commit date before and after.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core review::
cargo nextest run --locked -p fieldbook-render --test page
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
just check
```

Expected: the tests pass; `check` exits 0 and reports fewer `FB0117` warnings than before, the figure in the pull request; `just check` exits 0.

## Negative control

Use the oldest Change note entry instead of the newest: `review_by_counts_from_newest_change_note` must fail. Restore it.

## Rollback

Revert the merge commit, including the amendment. `review_by` counts from `as_of` again.

## BLOCKED

Until the owner accepts the amendment: comment `BLOCKED: review-by-basis needs the owner's ADR 0002 amendment`, push the branch with Step 1 only and take the next card.

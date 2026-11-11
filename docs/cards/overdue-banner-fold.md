# overdue-banner-fold

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

On an unreleased build, a page shows no separate review-overdue banner where revision 22 folds it into the build status banner, and a released edition shows the overdue banner exactly as before.

## Why

- The need. 665 of 1,709 pages show "Review overdue" on load. The banner is in every page template (`fb:overdueBanner`, written hidden with the page's `review_by`; `app.js` shows it when the viewer's date has passed). On an unreleased build the build status banner of revision 20 already says the claims are AI-drafted and unchecked, so a second warn banner says the same thing to the same reader.
- The rule that makes so many pages overdue is the owner's: `review-by-basis` proposes it. This card changes only where the notice is shown, as revision 22 (`design-r22-reader-pages`, step 5) decides, and works whichever rule the owner picks.
- The build never reads the clock ([corpus-format.md](../reference/corpus-format.md#what-the-reader-reads), Overdue banner), so the card keeps the browser deciding overdue and computes nothing from today's date.
- Released or unreleased is read from the edition metadata the build status banner reads (`unreleased-status-strip`), not from a new input.
- Design: `MAP.md` rows `#page-<id>` (`MetaRow`) and Build status banner; `HANDOFF.md` section 3 (Unreleased build status) and section 11 (Review date passed) as revision 22 words them; no new token.

## Scope

- Files: `crates/fieldbook-render/src/page.rs`, `crates/fieldbook-render/templates/page.html`, `crates/fieldbook-render/templates/types/`, `crates/fieldbook-render/src/frame.rs`, `crates/fieldbook-render/tests/page.rs`, `crates/fieldbook-render/tests/frame.rs`, `crates/fieldbook-render/tests/snapshots/{page__page__concept_page_renders.snap,page__page__procedure_page_renders.snap,page_types__page_types__brief_page_renders.snap,page_types__page_types__case_page_renders.snap,page_types__page_types__checklist_page_renders.snap,page_types__page_types__clause_card_page_renders.snap,page_types__page_types__reference_page_renders.snap,page_types__page_types__template_page_renders.snap}`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/assets/app.js`, only if the revision moves the date check into the build status banner.
- Docs to update: `CHANGELOG.md` (`- Changed: an unreleased build shows the review notice in its build status banner, not as a second banner on each page.`).

## Out of scope

- The `review_by` rule (`review-by-basis`, owner) and the claim-level review rows of the meta row.
- A released edition's banner.

## Preconditions

- Cards merged: `design-r22-reader-pages`, `fold-company-sections` (it changes the same page files and snapshots; `unreleased-status-strip` is reached through it).

## Steps

1. In `page.rs` and the page templates, omit or keep the page-level banner by the edition metadata, as the revision says.
2. In `frame.rs`, add to the build status banner what the revision says about review dates; if it needs the viewer's date, the banner carries the dates as data and `app.js` decides, as for the page banner today.
3. Tests: in `tests/page.rs`, `unreleased_page_has_no_overdue_banner` and `released_page_keeps_overdue_banner`; in `tests/frame.rs`, `build_status_banner_states_review_rule`. Update the page snapshots and read each diff.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test page
cargo nextest run --locked -p fieldbook-render --test frame
just check
```

Expected: the tests pass; `just check` exits 0.

## Negative control

Omit the banner on a released build too: `released_page_keeps_overdue_banner` must fail. Restore it.

## Rollback

Revert the merge commit. Each page carries its own overdue banner again.

## BLOCKED

Revision 22 or `fold-company-sections` has not merged: comment `BLOCKED: overdue-banner-fold waits on <slug>`, push the branch and take the next card. The banner's sentence needs a change of `templates/base.html`, which `search-palette-design-items` also changes: comment `BLOCKED: overdue-banner-fold needs base.html` and stop.

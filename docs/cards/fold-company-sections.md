# fold-company-sections

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

On a build with no company overlay loaded, every page shows its Company calibration part folded closed under its frozen heading, as revision 22 words it, and the part keeps its place in the part order, the table of contents and print.

## Why

- The need. Company calibration is on 1,703 pages and, in the public Corpus, only lists the company questions that would settle it (for example "How the company defines and reconciles its pools… [@Q-024]"). A reader with no company data can act on none of it, and it sits between the rule and the Sources on every page.
- The part order is frozen and "an empty part shows `Not established.`" ([corpus-format.md](../reference/corpus-format.md#what-the-reader-reads)); revision 22 (`design-r22-reader-pages`, step 4) folds the part without moving or dropping it.
- Whether a company overlay is loaded is already known: the frame's `StoreContext` in `crates/fieldbook-render/src/frame.rs` is `Public` on every build of the repository Corpus. The card reads it and adds no input; it does not change `frame.rs`.
- Design: `MAP.md` row `#page-<id>`; `HANDOFF.md` section 2 (part 6) and the fold component the revision names (section 5, `Fold`); no new token unless the revision adds one. The fold is a native `<details>`, so it works with scripts blocked.

## Scope

- Files: `crates/fieldbook-render/src/page.rs`, `crates/fieldbook-render/src/types.rs`, `crates/fieldbook-render/templates/page.html`, `crates/fieldbook-render/templates/types/`, `crates/fieldbook-render/tests/page.rs`, `crates/fieldbook-render/tests/page_types.rs`, `crates/fieldbook-render/tests/snapshots/{page__page__concept_page_renders.snap,page__page__procedure_page_renders.snap,page_types__page_types__brief_page_renders.snap,page_types__page_types__case_page_renders.snap,page_types__page_types__checklist_page_renders.snap,page_types__page_types__clause_card_page_renders.snap,page_types__page_types__reference_page_renders.snap,page_types__page_types__template_page_renders.snap}`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/assets/app.css`, only if the revision adds a class.
- Docs to update: `CHANGELOG.md` (`- Changed: with no company data loaded, each page's Company calibration part is folded.`).

## Out of scope

- The part's Corpus text and its questions.
- The company overlay build (`r2-overlay`): there the part shows open, as today.
- The "Never confuse" rewrite and the edition dates of the rule table: Corpus work.

## Preconditions

- Cards merged: `design-r22-reader-pages`, `sources-dedupe` (it changes the same page files and snapshots).

## Steps

1. In `page.rs` and `types.rs`, pass whether the build is public (`StoreContext::Public`) to the page model.
2. In `page.html` and `templates/types/`, wrap part 6 in the revision's fold when the build is public; its heading, anchor and table-of-contents entry are unchanged; print opens it as the revision says.
3. Tests in `tests/page_types.rs`: `company_calibration_folds_on_public_build` (each page type), `company_calibration_keeps_part_order`, `company_calibration_open_in_company_context` (a test model with the company context). Update the page snapshots and read each diff.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test page_types
cargo nextest run --locked -p fieldbook-render --test page
just check
```

Expected: the tests pass; `just check` exits 0.

## Negative control

Fold the part in the company context too: `company_calibration_open_in_company_context` must fail. Restore it. Move part 6 after Sources: `company_calibration_keeps_part_order` and the existing `parts_in_frozen_order` must fail. Restore it.

## Rollback

Revert the merge commit. Part 6 shows open again.

## BLOCKED

Revision 22 or `sources-dedupe` has not merged: comment `BLOCKED: fold-company-sections waits on <slug>`, push the branch and take the next card.

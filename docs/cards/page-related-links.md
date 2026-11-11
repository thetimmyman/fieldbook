# page-related-links

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every page's rail lists its `links` pages by title under Related and every page that links to it under Linked from, as revision 22 words them, with the Linked from lists computed once per build from the pages of the view.

## Why

- The need. [corpus-format.md](../reference/corpus-format.md#page-files) says of `links`: "The reader lists them with their titles." No template does. 1,568 pages declare 4,788 related links; on 148 pages none of them appears anywhere in the built page. 442 pages have no inbound body link, and there are no backlinks.
- The rail already has a place for this: `HANDOFF.md` section 3 (Frame, Rail row) holds "TOC, Evidence, Depends on, Linked from, Read first". Revision 22 (`design-r22-reader-pages`, step 1) adds Related and fixes the order, cap and empty states.
- A link target is resolved through `links::page_href` and `render_page_link`, so a page outside the view or not planned renders as text and no page can fail `FB0603`. The view is a `LabelledView`, so a page the destination may not hold never appears in either list ([ADR 0004](../decisions/0004-company-boundary.md)).
- Linked from needs every page's outbound links, so `site.rs` builds the index once and passes it to each page; a per-page scan would be quadratic over 1,709 pages.
- Design: `MAP.md` row `#page-<id>` (`EvidenceRail`, `PageLink`); `HANDOFF.md` section 3 (Rail, Rail fold) and section 5 (`PageLink` and the revision's list component); no new token unless the revision adds one.

## Scope

- Files: `crates/fieldbook-render/src/related.rs` (new: the inbound-link index and the two lists), `crates/fieldbook-render/src/page.rs`, `crates/fieldbook-render/src/types.rs`, `crates/fieldbook-render/src/site.rs`, `crates/fieldbook-render/templates/page.html`, `crates/fieldbook-render/templates/types/`, `crates/fieldbook-render/tests/page.rs`, `crates/fieldbook-render/tests/page_types.rs`, `crates/fieldbook-render/tests/snapshots/{page__page__concept_page_renders.snap,page__page__procedure_page_renders.snap,page_types__page_types__brief_page_renders.snap,page_types__page_types__case_page_renders.snap,page_types__page_types__checklist_page_renders.snap,page_types__page_types__clause_card_page_renders.snap,page_types__page_types__reference_page_renders.snap,page_types__page_types__template_page_renders.snap}`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/lib.rs` (`pub mod related;`); `crates/fieldbook-render/assets/app.css`, only if the revision adds a class.
- Docs to update: `CHANGELOG.md` (`- Added: each page lists its related pages and the pages that link to it, by title.`).

## Out of scope

- Hub pages per standard and new Corpus links: Corpus work in a `kb/` branch.
- Read first (`prereq`) and Depends on, unless revision 22 puts them in this list.
- Popover previews beyond what `render_page_link` already emits.

## Preconditions

- Cards merged: `design-r22-reader-pages`, `frame-unbuilt-entries` (it changes every snapshot this card changes), `page-context-packs` (it changes `page.rs`, `site.rs`, `page.html` and `templates/types/`).

## Steps

1. In `related.rs`, build once per build a map from page id to the sorted, distinct ids of the pages that name it in their body links or in `links`, from the pages of the view; ties and order as the revision says.
2. In `page.rs` and `types.rs`, give each page model the Related list (its `links`, front-matter order) and the Linked from list (from the map, capped as the revision says), each entry a title and an href or text from `render_page_link`.
3. In `page.html` and `templates/types/`, render the two rail sections as the revision says, with their empty states, in every page type that has a rail.
4. Tests in `tests/page.rs`: `related_lists_links_with_titles`, `linked_from_lists_inbound_pages`, `linked_from_is_capped`, `related_target_not_planned_is_text`, `related_lists_are_empty_state_when_none`. Update the page snapshots and read each diff.
5. Run `just site` and record, in the pull request, the Related and Linked from lists of `p/rte-indirect-cost-pools-bases.html`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test page
cargo nextest run --locked -p fieldbook-render --test page_types
just check
```

Expected: the tests pass; `just check` exits 0.

## Negative control

Build the Linked from list from `links` only, not from body links: `linked_from_lists_inbound_pages` must fail. Restore it. Link a target that is not planned: `related_target_not_planned_is_text` must fail. Restore it.

## Rollback

Revert the merge commit. Pages lose the two rail lists; nothing else depends on `related.rs`.

## BLOCKED

A precondition has not merged: comment `BLOCKED: page-related-links waits on <slug>`, push the branch and take the next card.

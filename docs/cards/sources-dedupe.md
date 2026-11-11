# sources-dedupe

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A page's Sources part has one row per distinct cited claim or source version, every citation marker of that claim or version links to that one row, and the row carries one back-reference to each marker, as revision 22 words it.

## Why

- The need. The built edition has 69,466 Sources rows for 31,405 distinct claim-page pairs: 55% repeat a row on the same page. On `p/rte-indirect-cost-pools-bases.html` rows 1 and 2 are both `CL-RTE-0206`. The Sources part is a median 45% of a page's bytes; 299 pages are over 200 KB and the largest is 1.17 MB, which is slow on a phone.
- Today `sources_block.rs` builds "one numbered `Sources` row per `CitationMarker` in exact marker reading order", and `inline.rs` numbers the markers `1..=K`, paired one to one. Revision 22 (`design-r22-reader-pages`, step 2) replaces that with one row per cited target, numbered by first citation, and back-references.
- A held version or a version with no claim keeps its own row kind (`SourceRowKind::Version`); deduplication keys on the target, never merges a claim row with a version row.
- Design: `MAP.md` row `#page-<id>` (`CitationMarker`, `SourcesBlock`); `HANDOFF.md` section 2 (part 7) and section 5 (`SourcesBlock`, `CitationMarker`) as revision 22 words them; no new token unless the revision adds one.

## Scope

- Files: `crates/fieldbook-render/src/sources_block.rs`, `crates/fieldbook-render/src/inline.rs`, `crates/fieldbook-render/src/page.rs`, `crates/fieldbook-render/src/types.rs`, `crates/fieldbook-render/templates/parts/sources_block.html`, `crates/fieldbook-render/tests/page.rs`, `crates/fieldbook-render/tests/page_types.rs`, `crates/fieldbook-render/tests/snapshots/{page__page__concept_page_renders.snap,page__page__procedure_page_renders.snap,page_types__page_types__brief_page_renders.snap,page_types__page_types__case_page_renders.snap,page_types__page_types__checklist_page_renders.snap,page_types__page_types__clause_card_page_renders.snap,page_types__page_types__reference_page_renders.snap,page_types__page_types__template_page_renders.snap}`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/assets/app.css`, only if the revision adds a class for the back-references.
- Docs to update: `CHANGELOG.md` (`- Changed: a page's Sources list each cited claim once, with links back to each place it is cited.`).

## Out of scope

- The claims part pages (`claims/<AREA>-<part>.html`), the evidence rail and the search index.
- Print footnotes, unless the build already writes them; if it does, they follow the same rule.
- Shortening quotes or statements.

## Preconditions

- Cards merged: `design-r22-reader-pages`, `page-related-links` (it changes `page.rs`, `types.rs`, the page tests and the page snapshots).

## Steps

1. In `inline.rs`, number each marker by the first citation of its target in reading order and give each marker an anchor id for its back-reference, as the revision names it.
2. In `sources_block.rs`, build one row per distinct target in first-citation order, carrying the list of its markers' anchors; a claim and a direct version citation stay separate row kinds.
3. In `parts/sources_block.html`, render the back-references as the revision says, with their accessible names. No `style=""`.
4. Tests in `tests/page.rs`: replace `markers_number_in_reading_order_and_match_sources_rows` with `repeated_claim_has_one_row` (a fixture citing one claim three times has one row and three markers with the same number), `back_references_link_each_marker`, `claim_and_version_rows_stay_separate`, and keep `sources_built_only_from_citations`. Update the page snapshots and read each diff.
5. Run `just site` and record in the pull request the row count and byte size of `p/rte-indirect-cost-pools-bases.html` and the largest page, before and after.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test page
cargo nextest run --locked -p fieldbook-render --test page_types
just check
```

Expected: the tests pass; `just check` exits 0.

## Negative control

Key rows by marker again instead of by target: `repeated_claim_has_one_row` must fail. Restore it. Drop one back-reference: `back_references_link_each_marker` must fail. Restore it.

## Rollback

Revert the merge commit. Sources return to one row per marker.

## BLOCKED

Revision 22 or `page-related-links` has not merged: comment `BLOCKED: sources-dedupe waits on <slug>`, push the branch and take the next card.

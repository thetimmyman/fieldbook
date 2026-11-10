# design-r22-reader-pages

- **Runs where:** home
- **Schema impact:** none
- **Review:** owner

## Outcome

Revision 22 of `docs/design/` is accepted, and it specifies the seven reader-page changes of Steps 1 to 7, each with its wording, position, states and tokens, so that the cards that build them invent nothing.

## Why

- The need. The hosted edition is now read as a reference by its owner, with no company data loaded, and an audit of the built `main` found pages that are hard to read: related pages are never listed, Sources repeat themselves, about 26 lines of every frame say "is not available until", every page carries an empty Company calibration part, 665 pages show "Review overdue" on load, and the top-bar build link is under the target size. Each fix changes what a reader sees, so the design comes first ([PLAN.md](../plan/PLAN.md#stop-and-ask): do not invent; do not edit `docs/design/` from a build card).
- Related pages. [corpus-format.md](../reference/corpus-format.md#page-files) says of the `links` field "The reader lists them with their titles", and 1,568 pages declare 4,788 such links; no template lists them. `HANDOFF.md` section 3 (Frame, the Rail row) already names a rail that "Holds TOC, Evidence, Depends on, Linked from, Read first", and none but TOC and Evidence is built. 442 pages have no inbound body link, so a Linked from list is the only way to reach them from a page.
- Sources. `HANDOFF.md` section 2 (Page template, part 7) gives `SourcesBlock` "one numbered row per citation marker". The built site has 69,466 rows for 31,405 distinct claim-page pairs, so 55% are repeats; the Sources part is a median 45% of a page's bytes, and the largest page is 1.17 MB.
- Unbuilt navigation. `nav.rs` renders every entry whose target is not in the build as `.not-available` text with its landing line (20 sidebar entries and the Assistant, Work and Plan tabs).
- Company calibration. Part 6 is on 1,703 pages and only lists company questions; with no company overlay loaded it is noise for this reader. The part order is frozen ([corpus-format.md](../reference/corpus-format.md#what-the-reader-reads), Part order), so the revision may fold the part, never drop or move it.
- The overdue banner. A page's `review_by` is `as_of` plus its interval ([Dates](../reference/corpus-format.md#dates-supersession-handling-and-labels)); `review-by-basis` proposes the rule change to the owner. Until then, on an unreleased build the build status banner of revision 20 already says the claims are unchecked, and a second warn banner on most pages says it again.
- The build link. `narrow-width-check` measured `a.build-date-link` at 23.19px tall at 375px, under `--target-min` (24px; section 0, WCAG 2.5.8 row; section 5, Chip row), and exempts it until a follow-up card fixes it.
- Outbound links on source pages. The owner decided that `s/` pages may link to the official publisher of each cited source and each cited section, for a fixed list of publishers, and nowhere else (recorded on the pull request that added this card); D-20 and `HANDOFF.md` section 11 (External URL: "never a link") say otherwise today. `source-location-links` builds it and amends ADR 0001 and the intake statements.
- This is a revision of the interface lane in a pull request of its own; the owner accepts each revision ([AGENTS.md](../../AGENTS.md#authority)), so the card is `Review: owner`.
- Design: this card is the design. It follows the `openui-design` skill at design time: a study for each changed layout family (the reader rail and the frame's sidebar), implemented in the mockup.

## Scope

- Files: `docs/design/HANDOFF.md`, `docs/design/MAP.md`, `docs/design/README.md`, `docs/design/MANIFEST.txt`, `docs/design/mockup/index.html`.
- Registration lines: none.
- Docs to update: the four design files; `MANIFEST.txt` is re-hashed.

## Out of scope

- Any Rust, template, CSS or script change: each item has its own build card (`frame-unbuilt-entries`, `page-related-links`, `sources-dedupe`, `fold-company-sections`, `overdue-banner-fold`, `build-date-target`).
- The `review_by` rule itself (`review-by-basis`), and building the source-page links, the ADR 0001 amendment and the intake edits (`source-location-links`).
- Any frozen string: a fixed sentence the revision adds is fixed template text (section 0), not a frozen string.

## Preconditions

- Design revision 21 (the `app.js` budget and stalled motion, its own design pull request) has merged, so this revision is numbered 22 and builds on its text.
- Cards merged: none.

## Steps

1. Related and Linked from (section 3 Rail row, section 5 a `RelatedList` or the existing `PageLink` in a rail section): the Related section lists the page's `links` with their titles in front-matter order; Linked from lists every page whose body or `links` names this page, with titles, in a stated order and with a stated cap and overflow line for pages with many inbound links; the empty state of each; where both sit when the rail folds below 1040px; and whether Read first (`prereq`) is in the same revision.
2. Sources (section 2 part 7, section 5 `SourcesBlock` and `CitationMarker`): one row per cited claim or source version, numbered in order of first citation; every marker of the same claim shows that row's number; the row carries one back-reference per marker, with their labels and accessible names; print footnotes follow the same rule.
3. Unbuilt navigation (section 3 Frame, section 5 `Sidebar`, `NavItem`, `SectionTabs`): on a build where an entry's target is not built, the entry is hidden or collapsed into one disclosure, with its wording; the rule for a tab; that no landing line is lost from the app map or the about page.
4. Company calibration (section 2 part 6): with no company overlay loaded (the public `StoreContext`), part 6 is folded closed under its frozen heading, with the fold's summary wording, and stays in the part order, in the table of contents and in print.
5. The overdue banner on an unreleased build (section 3 Unreleased build status, section 11 Review date passed): whether the page-level banner is suppressed when the build status banner shows, and what the build status banner then says about review dates, given that the build never reads the clock and `app.js` decides overdue in the browser.
6. The build link (section 5, the `Chip` row; section 0, the targets row, WCAG 2.2 AA 2.5.8, with the `--target-min` token): the element under target is `a.build-date-link`, the linked `Chip` around the header's build date, not the `.build-chip` store-context chip; the revision says which element carries the 24px target at every width, and that no other top-bar control changes size.
7. Outbound links on source pages (section 11 External URL; section 5 `LocatorView`; the `MAP.md` Source drawer row; section 12, the Links bullet ('External URLs are text with a copy button (D-20)'); section 16, the checklist item 'No page contains `href="http`'; the `MAP.md` row 'CSP, no `style=""`, no external URL'). Each of these states the same `s/` official-publisher exception, so revision 22 keeps no rule that contradicts its own section 11: on an `s/` page only, the version's official location and each section locator may be a link to the official publisher's copy, for the publishers the build names; the link's look, its accessible name (it says it opens the publisher's site), whether it opens in a new tab, and the locator text beside it; every other external URL stays text with a copy button.
8. A revision paragraph at the top of `HANDOFF.md`, the README's current revision, the MAP rows that name the build cards in Out of scope, the study record in section 13, and `MANIFEST.txt` re-hashed.

## Acceptance

```sh
(cd docs/design && sha256sum -c MANIFEST.txt)
typos docs/design
just check
```

Expected: each file prints `OK` and `sha256sum` exits 0; `typos` exits 0; `just check` exits 0.

## Negative control

Change one byte of `HANDOFF.md` after hashing: `(cd docs/design && sha256sum -c MANIFEST.txt)` must print `HANDOFF.md: FAILED` and exit 1. Restore it.

## Rollback

Revert the merge commit. The design returns to revision 21, and the build cards above stay blocked on their precondition.

## BLOCKED

Revision 21 has not merged: comment `BLOCKED: revision 22 waits on revision 21` and push the branch. An item needs a token, a component or a change to the frozen part order that the design does not give: comment `BLOCKED: <item> needs <what>` and leave the other items in the revision.

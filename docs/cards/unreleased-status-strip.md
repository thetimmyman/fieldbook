# unreleased-status-strip

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every page of an unreleased build shows one status strip that says the build is unreleased, is not an edition, and that its claims are AI-drafted and unchecked, with the counts computed from the claims register at build time and never written into the code or a page.

## Why

- The need. A hosted knowledgebase shows the Corpus to readers who do not run `fieldbook`. The register today holds 28,857 claims; every one has `verification = unchecked`, 28,839 have `drafted_by = ai` and 18 `lead`, and none has `verified_by_role`. A reader of any page sees none of that: the header chip says "unreleased build" only when the build has no date or commit, and the page itself carries no status. The honest status must be on every page and must stay true when the register changes, so it is computed.
- The hosted read-only edition (ADR 0007, proposed) deploys the build output unchanged and calls itself an unreleased build of the main branch whose claims are AI-drafted and unchecked; it names this strip as the planned card that makes every page say so. Because the hosted pages are byte-identical to the pages of any other build of the same commit, the strip is decided at build time, not by the host.
- The wording the work item names, pending the design: "Unreleased build · not an edition · claims are AI-drafted and unchecked", with the counts beside it (for example "28,839 of 28,857 claims AI-drafted, 28,857 unchecked" for the register above; the build computes the figures, the card writes none). The design gives the final Banner wording and kind.
- What `unreleased` means is not settled in code. `BuildInfo::is_released` in `crates/fieldbook-core/src/edition.rs` is true when the build has a `SOURCE_DATE_EPOCH` date and a 40-hex commit, and the program takes the commit from the compile-time `FIELDBOOK_GIT_SHA`. A hosted build of a commit on the main branch, built by a tool that was compiled with the commit, meets both and reads as a released edition with a date, although no edition tag exists. Step 1 settles the predicate; the owner owns tags and releases, so the answer is the owner's.
- Related and not changed here: 665 pages carry a passed `review_by` and 308 claims a passed `review_by` at the commit date (`FB0117`, `FB0221`). For example `cls-government-property-clause` has `as_of = 2026-01-20` and a passed review date of 2026-04-20, so a page written from an older source reads overdue the day it is written. Whether that banner and this strip say the same thing twice is the interface lane's question; the owner owns the review rule.
- Decisions: [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) (the header and `unreleased_build`), [corpus-format.md](../reference/corpus-format.md#strings) (`unreleased_build` is frozen), [ADR 0004](../decisions/0004-company-boundary.md) (public-only views). The strip counts public claims only, from the `LabelledView` the renderer already holds.
- Design: `MAP.md` rows `Banner` and the global frame (`AppShell`, `TopBar`, `BuildChip`); `HANDOFF.md` section 3 (frame), section 5 (`Banner`: always an icon, a bold first sentence and what to do next) and the `Unreleased build` row of the states table; token groups: the info or neutral banner colours as the revision names them. The strip is rendered with scripts blocked, in the `banners` slot of `templates/base.html`, with no `style=""` attribute.

## Scope

- Files: `crates/fieldbook-render/src/frame.rs`, `crates/fieldbook-render/src/meta.rs` (the one place the counts are computed, once per build, from the claims of the view), `crates/fieldbook-render/templates/base.html`, `crates/fieldbook-render/assets/app.css` (only if the revision adds a class), `crates/fieldbook-render/tests/frame.rs`, `crates/fieldbook-render/tests/site.rs`, `crates/fieldbook-core/src/edition.rs` (only for the predicate of Step 1), `CHANGELOG.md`.
- Registration lines: none.
- Docs to update: `docs/reference/corpus-format.md` (the strip's wording, if the revision adds a fixed string; the Strings table and `strings.rs` change together with their goldens), and `CHANGELOG.md` (`- Added: every page of an unreleased build states that the build is not an edition and counts its AI-drafted, unchecked claims.`).

## Out of scope

- Any edit of `docs/design/`: the revision is the interface lane's, accepted by the owner.
- Raising `verification` or setting `verified_on`: nothing here changes a claim.
- The overdue-review banner and the `review_by` rule.
- A strip on a released edition. A tagged edition keeps its header and shows no strip.
- Any script: the strip is static HTML and the counts are not read in the browser.

## Preconditions

- A `docs/design/` revision, accepted by the owner, that gives the Banner wording, kind and position for an unreleased build. Revision 19 has merged, so this is the next revision, in a pull request of its own.
- Cards merged: `build-frame`, `build-about`.

## Steps

1. Settle the predicate with the owner on the pull request: an unreleased page is one whose build is not an edition built by the edition release workflow. Show how `BuildInfo::is_released` answers for a hosted build of a main-branch commit. If it says "released", add the smallest input that makes the answer "unreleased" there (for example an explicit release mark that only the release workflow sets), refuse to default it to released, and add a test in `edition.rs`. Do not tag, release or change a workflow.
2. In `meta.rs`, compute once per build, from the public claims of the view: the number of claims, the number with `drafted_by = ai`, and the number with `verification = unchecked`. No count is a constant; a register with no claim gives no claim sentence.
3. In `frame.rs` and `base.html`, render the strip in the `banners` slot of every page when the build is unreleased, with the revision's wording, the three counts formatted with thousands separators, and the frozen `unreleased_build` text. A claim sentence appears only for a count above zero, and the word "unchecked" is not printed when the unchecked count is zero.
4. Tests in `tests/frame.rs`: a fixture whose claims are 3 AI-drafted and 1 lead-drafted, all unchecked, shows "3 of 4" and "4"; a fixture with one verified claim shows the unchecked count of the others; a fixture with no claims shows no claim sentence; a released build shows no strip. In `tests/site.rs`: every page of the fixture build has exactly one strip, and the strip text equals the counts recomputed from `claims.csv` of the fixture.
5. Run `just site` and record the strip text of `index.html`, one area page and one claim-heavy page in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core edition::
cargo nextest run --locked -p fieldbook-render --test frame
cargo nextest run --locked -p fieldbook-render --test site
just site && grep -c 'Unreleased build' out/edition/index.html
just check
```

Expected: the tests pass; the `grep` prints `1` (use the first words of the accepted wording if they differ); `just check` exits 0.

## Negative control

Hard-code the three counts in `meta.rs`: the fixture test that changes `claims.csv` and expects different figures must fail. Restore it. Make a released build render the strip: the "released build shows no strip" test must fail. Restore it.

## Rollback

Revert the merge commit. Pages lose the strip and the header chip is as before; `edition.rs` returns to its earlier predicate.

## BLOCKED

The design revision is not accepted, or the owner has not answered Step 1: comment `BLOCKED: the strip needs the accepted design revision` or `BLOCKED: what makes a build unreleased`, push the branch with the card unchanged and take the next card.

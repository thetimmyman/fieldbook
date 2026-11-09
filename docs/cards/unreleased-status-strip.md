# unreleased-status-strip

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Every page of an unreleased build shows one status strip that says the build is unreleased, is not an edition, and that its claims are AI-drafted and unchecked, with the counts computed from the claims register at build time and never written into the code or a page.

## Why

- The need. A hosted knowledgebase shows the Corpus to readers who do not run `fieldbook`. The register today holds 28,857 claims; every one has `verification = unchecked`, 28,839 have `drafted_by = ai` and 18 `lead`, and none has `verified_by_role`. A reader of any page sees none of that: the header chip says "unreleased build" only when the build has no date or commit, and the page itself carries no status. The honest status must be on every page and must stay true when the register changes, so it is computed.
- The hosted read-only edition (ADR 0007, proposed) deploys the build output unchanged and calls itself an unreleased build of the main branch whose claims are AI-drafted and unchecked; it names this strip as the planned card that makes every page say so. Because the hosted pages are byte-identical to the pages of any other build of the same commit, the strip is decided at build time, not by the host.
- The wording and the counts are the design's. Revision 20 of `docs/design/` names this a build status banner: a `Banner` in its warn variant, the first item of the frame's banner slot, with fixed sentences, agreement rules and four counts that the section "Unreleased build status" of `HANDOFF.md` defines: M (every claim row of the register the build renders), A (`drafted_by` is `codex` or `ai`), U (`verification` is `unchecked`) and S (`verification` is `primary_source_supported`). The build computes them and the card writes none. A claim that an agent drafts in the repository is `codex` and one pasted in from an assistant is `ai`; both count as AI-drafted. For the register above the build would say 28,839 of 28,857 (18 rows are `lead`'s) and 28,857 of 28,857 unchecked.
- What `unreleased` means is already settled, and this card does not change it. [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) (Determinism) says a build with `SOURCE_DATE_EPOCH` and a 40-hex commit is a released build, and `EditionInfo::from` in `crates/fieldbook-core/src/edition.rs` also requires an `edition-NNN` heading for it; otherwise it writes `edition = "unreleased"` and number 0. The banner reads that same edition metadata, as the header's `unreleased_build` chip does (revision 20 says so), so the two cannot disagree. The hosted build runs `just site`, which sets `SOURCE_DATE_EPOCH` and no commit, so it is already unreleased. If the owner wants a build that carries a commit but no edition tag to count as unreleased, that amends AD-12 and the edition build: a separate `Review: owner` card whose scope holds the `edition-zips` and `release-workflow` paths, not this one.
- Related and not changed here: 665 pages carry a passed `review_by` and 308 claims a passed `review_by` at the commit date (`FB0117`, `FB0221`). For example `cls-government-property-clause` has `as_of = 2026-01-20` and a passed review date of 2026-04-20, so a page written from an older source reads overdue the day it is written. Whether that banner and this strip say the same thing twice is the interface lane's question; the owner owns the review rule.
- Decisions: [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) (the header and `unreleased_build`), [corpus-format.md](../reference/corpus-format.md#strings) (`unreleased_build` is frozen), [ADR 0004](../decisions/0004-company-boundary.md) (public-only views). The strip counts public claims only, from the `LabelledView` the renderer already holds.
- Design: `MAP.md` rows `Banner` and the global frame (`AppShell`, `TopBar`, `BuildChip`); `HANDOFF.md` section 3 (the frame and "Unreleased build status"), section 5 (`Banner`: always an icon, a bold first sentence and what to do next) and the `Unreleased build` row of the states table; token groups: the warn banner colours the revision names, no new token. The banner is static HTML in the `banners` slot of `templates/base.html`, a region named "Build status", with no `style=""` attribute, and reads the same with scripts blocked.

## Scope

- Files: `crates/fieldbook-render/src/frame.rs`, `crates/fieldbook-render/src/meta.rs` (the one place the counts are computed, once per build, from the claims of the view), `crates/fieldbook-render/templates/base.html`, `crates/fieldbook-render/assets/app.css` (only if the revision adds a class), `crates/fieldbook-render/tests/frame.rs`, `crates/fieldbook-render/tests/site.rs`, `CHANGELOG.md`.
- Registration lines: `docs/reference/corpus-format.md`, a Strings table row, only if the revision makes a fixed string of the banner's wording and `crates/fieldbook-core/src/strings.rs` with its goldens.
- Docs to update: `docs/reference/corpus-format.md` (the banner's wording, only if the revision makes a fixed string of it; the Strings table and `strings.rs` change together with their goldens), and `CHANGELOG.md` (`- Added: every page of an unreleased build states that the build is not an edition and counts its AI-drafted, unchecked claims.`).

## Out of scope

- Any edit of `docs/design/`: the revision is the interface lane's, accepted by the owner.
- Raising `verification` or setting `verified_on`: nothing here changes a claim.
- The overdue-review banner and the `review_by` rule.
- A strip on a released edition. A released edition keeps its header and shows no strip.
- The meaning of `unreleased`, `edition.rs`, the edition build and every workflow: AD-12 settles the predicate and the release gate is the owner's.
- The print footer's edition slot that revision 20 words: the build writes no print footer yet (no template carries "Check the current edition"), so the card that builds it reads the frozen `unreleased_build` text.
- Any script: the strip is static HTML and the counts are not read in the browser.

## Preconditions

- Revision 20 of `docs/design/`, which gives the banner's wording, kind and position, has merged: it is the interface lane's pull request, accepted by the owner. If it changes before it merges, follow the merged text.
- Cards merged: `build-frame`, `build-about`.

## Steps

1. In `meta.rs`, compute once per build, from the claims register of the view the page is rendered for, the four counts of the revision: M, A (`drafted_by` in {`codex`, `ai`}), U and S (see Why). No count is a constant; a register with no claim has M = 0 and gives no count sentence.
2. In `frame.rs` and `base.html`, render the banner as the first item of the `banners` slot of every page of the site when the build's edition metadata says `unreleased` (the value the header chip already reads, not a new input), with the revision's sentences and agreement rules, digits with comma thousands separators, and the link to `claims.html` relative to the page. No close control, no script, no `style=""`. A released edition renders nothing.
3. Tests in `tests/frame.rs`: a fixture whose claims are 2 `ai`, 1 `codex` and 1 `lead`, all unchecked, shows "3 of 4 claims are AI-drafted" and "4 of 4 are unchecked"; a fixture with one `primary_source_supported` claim shows the supported sentence and the unchecked count of the others; a fixture with no claims shows "No claims recorded." and no supported sentence; a fixture with one claim uses the singular noun; a released build shows no banner. In `tests/site.rs`: every page of the fixture build has exactly one banner, and its counts equal the counts recomputed from `claims.csv` of the fixture.
4. Run `just site` and record the banner text of `index.html`, one area page and one claim-heavy page in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test frame
cargo nextest run --locked -p fieldbook-render --test site
just site && grep -c 'This is an unreleased build, not an edition.' out/edition/index.html
just check
```

Expected: the tests pass; the `grep` prints `1` (use the first sentence of the merged revision if it differs); `just check` exits 0.

## Negative control

Hard-code the four counts in `meta.rs`: the fixture test that changes `claims.csv` and expects different figures must fail. Restore it. Count only `drafted_by = ai` as AI-drafted: the fixture with a `codex` row must fail. Restore it. Make a released build render the banner: the "released build shows no banner" test must fail. Restore it.

## Rollback

Revert the merge commit. Pages lose the banner and the header chip is as before.

## BLOCKED

Revision 20 has not merged: comment `BLOCKED: the strip needs the accepted design revision`, push the branch with the card unchanged and take the next card. Or the renderer cannot tell an unreleased build from the edition metadata it already reads: comment `BLOCKED: build status source` with what it reads, and leave `edition.rs` alone.

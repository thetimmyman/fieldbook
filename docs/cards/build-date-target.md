# build-date-target

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The top-bar build link is at least 24px tall at 375px, and the narrow-width check holds it to that floor with no exemption.

## Why

- The need. `narrow-width-check` measured `a.build-date-link`, the linked `Chip` around the header's build date, at 23.19px tall at 375px. The design's floor for a linked chip is `--target-min` (24px; `HANDOFF.md` section 0, the targets row, WCAG 2.2 AA 2.5.8, and section 5, the `Chip` row). That card was told to report, not fix, so it exempts this one link from its control-height case and prints a note on every run.
- Revision 22 (`design-r22-reader-pages`, step 6) names which element carries the target; the fix is one rule in `assets/app.css` and the removal of the exemption from the harness.
- Design: `MAP.md` row AppShell, TopBar; `HANDOFF.md` section 5, the `Chip` row (a chip that is a link is at least `--target-min` tall outside running text), and section 0, the targets row, with the `--target-min` token; no new token. The element is `a.build-date-link`, not the `.build-chip` store-context chip of `BuildChip` ("Public data only" or "Company storage", from 1440px), which this card does not touch.

## Scope

- Files: `xtask/harness/narrow.js`, `xtask/harness/narrow-probe.js`, `xtask/src/harness.rs`.
- Registration lines: `crates/fieldbook-render/assets/app.css`, the `.build-date-link` rule.
- Docs to update: none.

## Out of scope

- Any other top-bar control, and any template: the link's markup is `frame-unbuilt-entries`'s file.
- Widths other than 375px.

## Preconditions

- Cards merged: `narrow-width-check`, `design-r22-reader-pages`, `corpus-job-speed` (it changes `xtask/src/harness.rs`).

## Steps

1. In `app.css`, give the element revision 22 names a minimum block size of `var(--target-min)` without changing the chip's own 22px pill.
2. In `narrow.js` and `narrow-probe.js`, remove the exemption of `a.build-date-link` and its note, so the link is measured against the linked-chip floor of 24px; in `harness.rs`, replace the test `narrow_probe_exempts_only_build_date_link_chip` with `narrow_probe_exempts_nothing`.
3. Run `just site && just browser` and record the narrow lines in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p xtask harness::
just site && just browser
just check
```

Expected: the tests pass; `just browser` exits 0 with `harness: narrow: narrow/narrow.html: 3 cases, 0 failed` and no `known design failure` note; `just check` exits 0.

## Negative control

Remove the new `app.css` rule in the temporary site build: `just browser` must fail with `top_bar_buttons_keep_the_control_height` naming the build link. Restore it.

## Rollback

Revert the merge commit. The link is 23px again and the harness exemption returns with it.

## BLOCKED

A card under Cards merged has not merged: comment `BLOCKED: build-date-target waits on <slug>`, push the branch and take the next card. CSS alone cannot reach 24px without changing the template: comment `BLOCKED: build-date-target needs topbar.html` and stop.

# source-ext-link-style

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

A reader can identify and activate a publisher link beside a source locator or in the official location row. The link has an accent underline, a target at least 24px tall and a trailing 13px icon.

## Why

[HANDOFF.md](../design/HANDOFF.md), section 5, specifies the publisher link under LocatorView and the underline under PageLink. The Type, Space and Border tokens supply the measurements.

## Scope

- Files: `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/tokens.rs`, `docs/plan/PLAN.md`, `CHANGELOG.md`.
- Docs to update: the plan line and the changelog.

## Out of scope

Publisher selection, markup, outbound-link checks and navigation.

## Preconditions

- Cards merged: `source-location-links`.

## Steps

1. Pin the publisher link and icon tokens in the renderer tests.
2. Apply the existing tokens to publisher links on source pages.
3. Inspect a built source page on desktop and narrow screens in light, dark and print.

## Acceptance

```sh
cargo test --locked -p fieldbook-render --test tokens
just site
just check
```

The token tests pass. The link wraps at narrow widths, carries an underline, fits its icon and keeps the keyboard focus ring.

## Negative control

Remove the publisher link styling. The publisher-link token test must fail.

## Rollback

Revert the card to restore the previous publisher-link presentation.

## BLOCKED

Stop if the source link requires a change to publisher admission or navigation.

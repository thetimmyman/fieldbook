# claims-register-split

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The claims register is no longer one page. `fieldbook build` writes one claims page per area and a `claims.html` index that links to them, each of which loads in the page harness in under 60 seconds, and the harness's register-page exception (300 seconds) is removed, so every page of the edition has the same 60-second limit.

## Why

- The page harness and its limits are in [AD-08](../../decisions/0001-foundation.md#ad-08-ci-design). The register pages have 300 seconds there as an owner exception, because the single `claims.html` is about 88 MB and takes about 260 seconds to load headless. This card ends the exception.
- Register pages and their routes are in [AD-12](../../decisions/0001-foundation.md#ad-12-site-generation-and-page-types); the register template is `crates/fieldbook-render/src/register.rs`.
- Design: the [MAP.md](../../design/MAP.md) register rows (`claims.html`) and the sidebar link to the claims register. The per-area page reuses the one generic register template; a change to its look is a request to the interface lane. Token groups: none.

## Scope

- Files: `crates/fieldbook-core/src/ids.rs` and `crates/fieldbook-core/src/lib.rs` (the one claim-part route function), `crates/fieldbook-core/src/search/units.rs` (the claim result link), `crates/fieldbook-render/src/register.rs`, `crates/fieldbook-render/src/links.rs`, `crates/fieldbook-render/src/nav.rs`, `crates/fieldbook-render/src/site.rs`, `crates/fieldbook-render/src/page.rs` and `crates/fieldbook-render/src/sources_block.rs` (comments only), `crates/fieldbook-render/templates/register.html`, `crates/fieldbook-render/assets/search.js` (the same arithmetic), `crates/fieldbook-render/tests/` (registers, frame, search and the snapshots), `crates/fieldbook/tests/build.rs` (the FB0603 anchor test on a part page) and `crates/fieldbook/tests/leak_outputs.rs`, `xtask/src/harness.rs` (remove `REGISTER_LIMIT` and `REGISTER_PAGES`, or shrink the list to the pages that still need it, with their tests), `docs/decisions/0001-foundation.md`, `docs/reference/search.md`, `docs/reference/corpus-format.md` (route text only), `CHANGELOG.md`.
- Registration lines: none.
- Docs to update: AD-08 (drop or narrow the exception), `CHANGELOG.md` (`- Changed: the claims register is an index page with one page per area part (500 claim numbers each), and every claim link, search result and All IDs entry points at its part.`).

## Out of scope

- Splitting the other registers; they keep one page each unless the harness shows one past 60 seconds, which is then a card of its own.
- Any change to the register CSVs, the claim schema or search.
- A new layout or component for the register pages.

## Preconditions

- Cards merged: `edge-harness`.

## Steps

1. In `register.rs`, render the claims register as one page per area part (`claims/<AREA>-<part>.html`, part = (number - 1) / 500 + 1, so one area never makes one huge page), and `claims.html` as an index of the areas with their claim counts and links to every part. Links from claim chips, popovers and source pages to a claim point at its area page with the claim's anchor.
2. In `build.rs`, write the area pages and the index, deterministically (sorted by area code).
3. Tests in `crates/fieldbook-render/tests/registers.rs`: every claim appears on exactly one area page; the index links every area page; a claim link resolves to an existing page and anchor.
4. In `xtask/src/harness.rs`, remove `claims.html` from `REGISTER_PAGES`; if no register page then needs more than 60 seconds, remove `REGISTER_LIMIT`, `REGISTER_PAGES` and their tests, and the AD-08 sentence; otherwise narrow the list and the sentence to the pages that remain.
5. Run `just site && just browser` and record the slowest page's load time in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test registers
cargo nextest run --locked -p xtask harness::
just site && just browser
just check
```

Expected: the tests pass; `just browser` exits 0 with its summary line and every page within 60 seconds, the claims pages among them; `just check` exits 0.

## Negative control

Render all claims onto the index page again: `just browser` must fail with `claims.html: not done within its limit of 60 seconds`. Restore it.

## Rollback

Revert the merge commit. `claims.html` is one page again and the harness exception must be restored with it.

## BLOCKED

An area's claims page alone still takes more than 60 seconds, or a claim link cannot be kept stable across the split: `BLOCKED: <page> loads in <n> s` or `BLOCKED: claim links`, and stop for the lead to choose a finer split.

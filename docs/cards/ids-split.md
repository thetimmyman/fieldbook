# ids-split

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`ids.html` is an index of the seven id groups with their counts, each group but claims is its own page under `ids/`, and the claims group is split by the claims register's part rule into `ids/claims-<AREA>-<part>.html` (500 claim numbers a part), so no id-list page in the built edition is over 3 MB.

## Why

- The need. `ids.html` is 13.5 MB. Every page links to it (the search trigger without scripts) and the search page sends a reader there when search fails, which is the worst moment to load 13.5 MB on a phone. Most of it is the 28,857 claim ids.
- The claims register was split the same way by `claims-register-split`: `claims.html` became an index and each area part its own page, and `xtask/src/allowlist.rs` admits the part pages by a strict name rule (`is_claims_part_page`). This card follows that shape: `ids.html` stays the entry page, so every existing link to it, and the design's role for it ([ADR 0006](../decisions/0006-edition-search-index.md), `HANDOFF.md` section 10), are unchanged.
- The routes are an output table row of [corpus-format.md](../reference/corpus-format.md) (the `ids.html` row), extended as `claims-register-split` extended the claims row; the design's Id list is a route without a mockup, so no design revision is needed (`MAP.md` row Id list, `HANDOFF.md` section 2 Routes).
- One page per area would not meet 3 MB: in the audit build of `main`, the CMP area alone holds 9,485 claim ids in 4,191,644 bytes of `ids.html`, so `ids/claims-CMP.html` would be about 4.0 MiB. The claims register's part rule (`fieldbook_core::ids::claim_part`, `CLAIMS_PER_PART` = 500) gives parts of about 0.22 MB, and a part depends only on the claim id, so an id's page never moves when other claims are added.
- The groups and their order are those of `ids::render_page` today: pages, sources, versions, claims, questions, conflicts, terms. A group page holds the same entries, anchors and titles the group has now.
- Design: `MAP.md` row Id list; `HANDOFF.md` section 2 (Routes); no new token, string or component. The index page lists each group as a link with its count.

## Scope

- Files: `crates/fieldbook-render/src/ids.rs`, `crates/fieldbook-render/templates/ids.html`, `crates/fieldbook-render/src/site.rs`, `crates/fieldbook-render/src/nav.rs`, `crates/fieldbook-render/tests/registers.rs`, `crates/fieldbook-render/tests/search_index.rs`, `crates/fieldbook/tests/leak_outputs.rs`, `crates/fieldbook-render/tests/snapshots/registers__registers__ids_page_renders.snap`, `xtask/src/allowlist.rs`, `docs/reference/search.md`, `CHANGELOG.md`.
- Registration lines: `docs/reference/corpus-format.md`, the `ids.html` row of the site output table.
- Docs to update: that row; `CHANGELOG.md` (`- Changed: the ID list is an index with one page per group, and claims in parts of 500 per area.`).

## Out of scope

- The search index and its size (ADR 0006).
- The browser harness: it opens every built page already, so `xtask/src/harness.rs` is unchanged; if a run shows it is not, the card is BLOCKED (see below).
- The claims register pages.

## Preconditions

- Cards merged: `page-related-links` (it changes `site.rs`), `raw-edition` (it changes `xtask/src/allowlist.rs`).

## Steps

1. In `ids.rs`, keep one function that builds the seven groups; render `ids.html` as the index and one page per other group at `ids/<slug>.html`, the claims group at `ids/claims-<AREA>-<part>.html`, with the area and part from `fieldbook_core::ids::claim_part` (no second copy of the rule), each in the site frame with its own crumbs. The claims group has no page of its own: under its count, the index lists its part pages, each with its first and last claim id.
2. In `site.rs`, add the new paths to the planned paths and insert each page.
3. In `allowlist.rs`, admit `ids/<slug>.html` for the six other slugs and `ids/claims-<AREA>-<part>.html` by a rule that mirrors `is_claims_part_page` (prefix `ids/claims-`, no further `/`, a three-letter upper-case area, a `-`, a part with no leading zero, `.html`), with unit tests of accepted names (`ids/claims-CMP-1.html`, `ids/claims-CMP-19.html`) and refused ones (`ids/claims-cmp-1.html`, `ids/claims-CMP-01.html`, `ids/claims-CMP.html`, `ids/claims-CMP-1/x.html`).
4. Tests in `tests/registers.rs`: `ids_index_lists_groups_with_counts`, `ids_group_pages_hold_every_id` (the union of the group pages equals the ids of the view), `ids_claims_split_by_area` (each claim id is on the page `claim_part` names and on no other, and no part page holds more than `CLAIMS_PER_PART` ids). Update the ids snapshot and read the diff.
5. Run `just site`, then record in the pull request the size of `ids.html` and the largest `ids/` page.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test registers
cargo nextest run --locked -p xtask allowlist::
just site && test -d out/edition/ids && find out/edition/ids.html out/edition/ids -size +3M | wc -l
just check
```

Expected: the tests pass; the `test -d` passes and the `find` line prints `0` (a missing `ids/` folder fails the line); `just check` exits 0.

## Negative control

Leave one group out of the group pages: `ids_group_pages_hold_every_id` must fail. Restore it. Admit `ids/claims-cmp-1.html` (lowercase area) in `allowlist.rs`: its refusal test must fail. Restore it.

## Rollback

Revert the merge commit. `ids.html` holds every id on one page again.

## BLOCKED

A precondition has not merged: comment `BLOCKED: ids-split waits on <slug>`, push the branch and take the next card. `just browser` does not open the new pages without a harness change: comment `BLOCKED: ids-split needs a harness change` and leave `harness.rs` alone.

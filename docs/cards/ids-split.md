# ids-split

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`ids.html` is an index of the seven id groups with their counts, each group is its own page under `ids/`, and the claims group is one page per area, so no id-list page in the built edition is over 3 MB.

## Why

- The need. `ids.html` is 13.5 MB. Every page links to it (the search trigger without scripts) and the search page sends a reader there when search fails, which is the worst moment to load 13.5 MB on a phone. Most of it is the 28,857 claim ids.
- The claims register was split the same way by `claims-register-split`: `claims.html` became an index and each area part its own page, and `xtask/src/allowlist.rs` admits the part pages by a strict name rule (`is_claims_part_page`). This card follows that shape: `ids.html` stays the entry page, so every existing link to it, and the design's role for it ([ADR 0006](../decisions/0006-edition-search-index.md), `HANDOFF.md` section 10), are unchanged.
- The routes are an output table row of [corpus-format.md](../reference/corpus-format.md) (the `ids.html` row), extended as `claims-register-split` extended the claims row; the design's Id list is a route without a mockup, so no design revision is needed (`MAP.md` row Id list, `HANDOFF.md` section 2 Routes).
- The groups and their order are those of `ids::render_page` today: pages, sources, versions, claims, questions, conflicts, terms. A group page holds the same entries, anchors and titles the group has now.
- Design: `MAP.md` row Id list; `HANDOFF.md` section 2 (Routes); no new token, string or component. The index page lists each group as a link with its count.

## Scope

- Files: `crates/fieldbook-render/src/ids.rs`, `crates/fieldbook-render/templates/ids.html`, `crates/fieldbook-render/src/site.rs`, `crates/fieldbook-render/tests/registers.rs`, `crates/fieldbook-render/tests/snapshots/registers__registers__ids_page_renders.snap`, `xtask/src/allowlist.rs`, `CHANGELOG.md`.
- Registration lines: `docs/reference/corpus-format.md`, the `ids.html` row of the site output table.
- Docs to update: that row; `CHANGELOG.md` (`- Changed: the ID list is an index with one page per group, and claims one page per area.`).

## Out of scope

- The search index and its size (ADR 0006).
- The browser harness: it opens every built page already, so `xtask/src/harness.rs` is unchanged; if a run shows it is not, the card is BLOCKED (see below).
- The claims register pages.

## Preconditions

- Cards merged: `page-related-links` (it changes `site.rs`), `raw-edition` (it changes `xtask/src/allowlist.rs`).

## Steps

1. In `ids.rs`, keep one function that builds the seven groups; render `ids.html` as the index and one page per group at `ids/<slug>.html`, the claims group at `ids/claims-<AREA>.html` per area code, each in the site frame with its own crumbs.
2. In `site.rs`, add the new paths to the planned paths and insert each page.
3. In `allowlist.rs`, admit `ids/<slug>.html` for the seven slugs and `ids/claims-<AREA>.html` by the same strict rule as `is_claims_part_page`, with unit tests of accepted and refused names.
4. Tests in `tests/registers.rs`: `ids_index_lists_groups_with_counts`, `ids_group_pages_hold_every_id` (the union of the group pages equals the ids of the view), `ids_claims_split_by_area`. Update the ids snapshot and read the diff.
5. Run `just site`, then record in the pull request the size of `ids.html` and the largest `ids/` page.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test registers
cargo nextest run --locked -p xtask allowlist::
just site && find out/edition/ids.html out/edition/ids -size +3M | wc -l
just check
```

Expected: the tests pass; the `find` line prints `0`; `just check` exits 0.

## Negative control

Leave one group out of the group pages: `ids_group_pages_hold_every_id` must fail. Restore it. Admit `ids/claims-cmp.html` (lowercase) in `allowlist.rs`: its refusal test must fail. Restore it.

## Rollback

Revert the merge commit. `ids.html` holds every id on one page again.

## BLOCKED

A precondition has not merged: comment `BLOCKED: ids-split waits on <slug>`, push the branch and take the next card. `just browser` does not open the new pages without a harness change: comment `BLOCKED: ids-split needs a harness change` and leave `harness.rs` alone.

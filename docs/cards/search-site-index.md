# search-site-index

- **Runs where:** home, CI Linux, CI Windows
- **Schema impact:** none
- **Review:** owner

## Outcome

The site receives a deterministic public search index from the pure ranking provider.

## Why

- CLI search and regression must remain available when site delivery is deferred.
- Design: use the existing Search row of [MAP.md](../design/MAP.md) and the site-index contract in [search.md](../reference/search.md#the-site-index); this card changes data serialization only, with no token groups.

## Scope

- Files: `crates/fieldbook-render/src/search_index.rs`, `crates/fieldbook-render/tests/search_index.rs`.
- Registration lines: render module declaration; `site.rs` render call, Site insertion and planned path for `assets/search-index.js`.; `crates/fieldbook/tests/{build.rs,leak_outputs.rs}` (delivered search-index assertions on the existing edition SURFACES row)
- Docs to update: none.

## Out of scope

- Ranking, CLI search, browser search controls and palette.

## Preconditions

- Cards merged: `search-ranking-and-index`, `build-site-lints`, `build-about`.

## Steps

1. Implement `search_index::write(&Index) -> String` from the reference: compact JSON with fixed key order between `window.FB_INDEX = ` and `;`. Render from the public view, insert in Site and register its planned path. The existing delivery pipeline writes it; the size lint still rejects more than 3 MB.

2. Tests cover prefix/version, 160-code-point snippets, UTF-16 term order, equal bytes across builds, absence of nonpublic units and label IDs, and no quarantined unit or canary text. Run planned-path equality. The pure ranker and the general index shape remain unchanged.

3. Add `build_search_index_uses_the_permitted_view` through build-page’s real render/delivery harness. Require successful delivery, reopen assets/search-index.js and assert permitted IDs, snippets and postings are present while forbidden and quarantined IDs, text and unique terms are absent. Resolve postings through the delivered unit table; do not search for numeric unit IDs as text. Extend the existing edition SURFACES assertion.

4. Add `search_section_links_resolve_to_rendered_headings` through the actual build/delivery harness: reopen the delivered index and page HTML and require every section fragment to equal a rendered heading id, including duplicate/Unicode headings. A two-sentence Purpose confirms the page-lead snippet uses its first paragraph under the 160-code-point rule, while summaries use its first sentence.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test search_index
cargo nextest run --locked -p fieldbook-render planned_paths_equal_the_site_keys
just site
cargo nextest run --locked -p fieldbook --test build --test leak_outputs
just check
```

Expected: tests pass, the site includes the index and its size checks pass.

## Negative control

Let the edition keep every unit: `edition_index_holds_public_units_only` must fail. Include a quarantined source: `held_version_not_in_index` must fail. Restore each.

Bypass filtering before inserting the search index into Site: `build_search_index_uses_the_permitted_view` must fail while permitted presence stays required. Restore it.

Change a fragment in either the renderer or index consumer: `search_section_links_resolve_to_rendered_headings` must fail. Restore it.

## Rollback

Revert the site serialization; CLI search continues working.

## BLOCKED

The public index contract conflicts with interface-owned guidance: report the exact conflict and stop that portion.

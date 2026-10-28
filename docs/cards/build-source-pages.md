# build-source-pages

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build` writes one `s/<version-id>.html` for every source version row that is not held (`lifecycle` of `quarantined`), and copies every stored original of those versions into the sibling `originals/` folder under its shipped name, so a renderable original arrives as `<version-id>.<ext>.txt`. A `p.` or `pp.` citation of a version whose shipped original is a PDF links to that original at `#page=<first page>`; every other locator kind keeps its `s/` anchor, and a held version stays plain text with its hold reason.

## Why

- The `s/` page and its contents are set in [The `s/` page](../reference/corpus-format.md#the-s-page) and the `s/` row of [Site output paths](../reference/corpus-format.md#site-output-paths). The shipped name of an HTML or SVG original is [Renderable originals](../reference/corpus-format.md#renderable-originals), control ED-02 in the [control register](../controls.md).
- The link to an original, the content security meta and the no-`style=""` rule are [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types). The two folders `out/edition/` and `out/originals/` are in [Repository layout](../decisions/0001-foundation.md#repository-layout).
- The link target of each locator kind is the Locator kinds table of [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance): `p.` and `pp.` go to the original at its first page, and `sec.`, `sheet`, `rec`, `ext` and none go to the `s/` page. The shipped original is chosen by the same selection that copies it, so the link and the file cannot disagree.
- Design: the [MAP.md](../design/MAP.md) row "Source drawer" (SourceDrawer, LocatorView, Excerpt, AnswerMeta), built as the page itself so it works with scripts blocked. [HANDOFF.md](../design/HANDOFF.md) section 5 Components (those four rows, `CitationChip` and `Unavailable`), section 6 States and interactions (citation chip), section 11 Edge cases (metadata-only source and held version cited), and section 12 Links. Token groups: colour (the status tokens for the variant banners), type (`--font-mono` for ids and hashes), layout (`--drawer-w`).

## Scope

- Files: `crates/fieldbook-render/templates/source.html`, `crates/fieldbook-render/templates/parts/unavailable.html`, `crates/fieldbook-render/src/{lib.rs,source.rs,cite.rs}`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/source.rs`, `crates/fieldbook/src/build.rs`, `crates/fieldbook/tests/build.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-render/src/site.rs` (this card's paths in `planned_paths` and its calls and `Site` inserts in `render` only); `crates/fieldbook-render/src/links.rs` (retain lifecycle eligibility before source-page fallback); `crates/fieldbook-render/src/page.rs` and `crates/fieldbook-render/templates/page.html` (the located-citation link target and held citation rendering); `crates/fieldbook-render/tests/page.rs` (held reason cases); `crates/fieldbook/tests/leak_outputs.rs` (source-page and originals assertions on the existing edition SURFACES row)
- Docs to update: `CHANGELOG.md` (`- Added: source pages and the originals folder.` under `## Unreleased`).

## Out of scope

- Extracted text, section anchors and the locator highlight (`extract-and-locators`, `quote-match-and-anchors`). Until then every page says the version is not extracted, and every locator kind other than a PDF page or range keeps its `s/` page anchor. The citation chip's JavaScript drawer enhancement is unchanged.
- The originals zip and its `SHA256SUMS` (`edition-zips`).
- The source register page `sources.html` (`build-registers`).
- The site of a company store (the overlay site). It reads that store's folders, takes its originals from `sources/org/` and writes under the data root only, so nothing it makes can reach an edition (`r2-company-ids` builds its index, `r2-records` its record pages). This card builds the edition.

## Preconditions

- Cards merged: `build-registers`.

## Steps

1. In `source.rs`, build a `SourcePage` for each version row that is not held (step 5): the row in register column order, the `sha256` in mono, the shipped name of the original, the claims that cite this version (linked to `claims.html#<id>`) and the pages that cite it (linked to `p/<page-id>.html`). Add `shipped_name(version_id, ext)`. It appends `.txt` exactly for the extensions in Renderable originals and keeps every other name. Read `LabelledView::sources` and `LabelledView::tree` for source versions and extraction text.
2. Link a stored original to the sibling `originals/` folder by its shipped name, with the locator text beside the link. A metadata-only version has no link; it shows the lock chip of HANDOFF section 11. A superseded version shows the stale-version variant with a link to the version that supersedes it.
3. Add `parts/unavailable.html`, the `Unavailable` panel, with the `not_available` line from `fieldbook_core::strings`. It is a `<section class="unavailable">`. `build-charts` reuses it. LocatorView, Excerpt and AnswerMeta render this panel where their data is missing; on Day 1 that is always for LocatorView and Excerpt.
4. In `app.css`, lay out the page as the drawer at its full width, capped at `var(--drawer-w)` on wide screens. Use the existing tokens. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF.
5. `render` adds the `s/` pages to the `Site`; in `render/src/site.rs`, add their paths to `planned_paths`, so that `planned_paths_equal_the_site_keys` holds. Then copy each stored original from `corpus/sources/public/` to `<out>/originals/<shipped name>` byte for byte, where `<out>` is the `DIR` of `build --out DIR`. The originals go in the same write set that `build-v0` passes to `Journal::begin_export`, each with its SHA-256, and are written by a second call of `io::out::replace` for `<out>/originals`, with the final `Allowed` and `Audited`. Include their exact bytes, hashes, marker and directory operations in the build Delivery before its final check; a view-selection token cannot copy an original. Hash each copy while writing it and stop with exit 3 if the hash differs from the row (the comparison is a pure function in `build.rs`). The edition's originals are the public originals of the repository Corpus; no other folder is read. Pass `shipped_name(version_id, ext)` of every stored original that is copied to `lint` as the `OriginalNames` of `build-site-lints`. A version named in `quarantined` (the ids whose `lifecycle` is `quarantined`; `build-about` later passes the same list to `shipped_corpus`) gets no `s/` page, no version row on its source page and no link; the source page of a wholly held source is omitted; the omission is listed in the build report: `build` prints one line `held <version-id>` for each. Its original is not copied and its path is not added to `planned_paths`, so a link to it is plain text. Before begin, include `BuildRoot::Originals` in `dirs::check_build_roots`; the binary test `a_foreign_originals_folder_is_not_deleted` plants originals/keep.txt without a build marker, requires exit 2, intact bytes and no event.
6. Tests in `tests/source.rs`: `one_page_per_version` (the fixture's version rows); `renderable_original_ships_as_txt` (`.html`, `.htm`, `.svg`, `.xhtml` gain `.txt`; `.pdf` and `.txt` do not); `metadata_only_has_no_link`; `superseded_version_links_forward`; `missing_parts_show_unavailable`; `every_page_has_the_csp_meta_and_no_style_attribute`; `held_version_has_no_page` (a synthetic model with one `quarantined` and one `active` version of the same source: only the active one has a page and no page links to the held one; with every version held the source has no page); `version_links_point_at_s_pages_once_planned` (`links` with a fixed `PlannedPaths::of(...)` that includes `s/<version-id>.html` of the fixture, and never `site::planned_paths`: a version chip links to that page, and with the same set minus that path it links to its `sources.html` row); `a_renderable_original_link_resolves_to_its_txt_name` (a synthetic `Site` whose `s/` page links to `originals/<version-id>.html.txt`, with the `OriginalNames` made from `shipped_name(version_id, "html")`: `lint` reports no FB0603; with `<version-id>.html` in the set instead it reports FB0603). Tests in `crates/fieldbook/tests/build.rs` (`mod build`, the file of `build-v0`), each on its own temporary copy of the fixture made by `temp_copy`, with its own state folder and `--out` folder: `originals_are_copied_byte_for_byte_under_shipped_names` (every stored original of the copy is in `$T/originals/<shipped name>` with the same bytes; a `.html` original is `.html.txt`), `originals_are_in_the_write_set` (the `write.begin` of that build counts the files of `edition/` and `originals/` together) and `a_held_version_prints_held_and_ships_nothing` (edit the copy's version row to `lifecycle = quarantined`: `held <version-id>` is printed, and there is no `$T/edition/s/<id>.html` and no file of it in `$T/originals`). The exit-3 hash comparison cannot be reached through the binary, because `check` has compared each hash with its row (FB0211). `original_hash_mismatch_is_an_error` is a unit test of the pure comparison in `build.rs`: bytes whose hash differs from the row's give an error that exits 3, and equal bytes give `Ok`. No `World` double is added for it.

7. The version-link fallback tests apply only to non-quarantined versions. Before selecting s/ or the register anchor, keep held versions as plain text in citations, register cells and ids.html, even when sources.html is planned. Test an active linked version alongside the held one; retained metadata does not make the held row a link destination. The located-citation rule of step 10 is applied first and never applies to a held version.

8. For a citation of a held version, render its escaped source-row `note` as the hold reason beside the plain-text citation, as HANDOFF specifies. In `tests/page.rs`, `held_citation_shows_reason_without_link` renders an actual reader page citing a held version and an active comparison version: require the escaped reason, no source-page or register-fallback link for the held citation, and the active link.

9. Add `build_source_pages_and_originals_use_the_permitted_view` through the real render/delivery harness established by build-page. Require successful delivery, permitted source-page canary text and byte-exact permitted originals, with forbidden-label source text and originals absent from both outputs. Extend the existing edition SURFACES assertion. Preserve the public-only Corpus preflight; a preflight rejection alone does not test these branches.

10. Extend the citation renderer entry point in this card to accept shipped-original availability; the earlier `build-page` implementation and its snapshots supply an empty availability input until this card lands, preserving their version-page links. Give the renderer the availability of the version it cites, from the same selection that step 5 uses for delivery and for `OriginalNames`: the version is not held, and its stored original's shipped name. A citation whose locator kind is `pdf_page_box` (`p.` or `pp.`) and whose version has a shipped PDF original links to `../../originals/<shipped name>#page=<first page>` — the path from a page under `edition/p/` or `edition/s/` to `<out>/originals/` — with the locator text beside the link; a `pp.` range uses its first page. Every other locator kind (section, sheet, record, extension, none) keeps its `s/<version-id>.html` anchor, a metadata-only version keeps its `s/` link and its lock, and a held version keeps the plain text and hold reason of step 8. The `originals/` folder is not a `Site` key and no path in `planned_paths` names it, so this link never depends on the site's path list and the fixed `PlannedPaths` of the `build-page` snapshots is unchanged. Add `pdf_citation_links_to_shipped_original_page` in `crates/fieldbook/tests/build.rs` through the real render/delivery harness: build a temporary copy of the fixture, then read the delivered `edition/p/<page-id>.html` and require the delivered relative href `../../originals/<shipped name>#page=<first page>` for a single-page citation and for a range citation, and require the locator text beside the link. Add a separate page whose only citation is `[@CL-...]`, backed by a ClaimRow whose own source version and `p.` or `pp.` locator resolve to the shipped PDF; assert that claim’s own Sources-row href and locator text. The claim case must contain no direct source citation, so a correctly routed direct citation cannot mask broken claim routing. Require a metadata-only version's citation to keep its `s/` href, and a held version's citation to be plain text carrying the escaped hold reason with neither an `originals/` nor an `s/` href. Keep the fixture's original bytes, hashes and names fixed, and do not read `site::planned_paths` in the test.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test source
cargo nextest run --locked -p fieldbook build::
cargo build -q --locked -p fieldbook && B="$PWD/target/debug/fieldbook" && C="$(mktemp -d)" && cp -R tests/fixtures/corpus-mini/. "$C" && T="$C/out"
(cd "$C" && "$B" build --out "$T") && ls "$T/edition/s"
ls "$T/originals"
! grep -L 'http-equiv="Content-Security-Policy"' "$T"/edition/s/*.html | grep .
! grep -l 'style="' "$T"/edition/s/*.html
grep -c 'class="unavailable"' "$T"/edition/s/*.html
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p xtask literals::
just site
cargo nextest run --locked -p fieldbook-render --test page
cargo nextest run --locked -p fieldbook --test build --test leak_outputs
cargo nextest run --locked -p fieldbook --test build pdf_citation_links_to_shipped_original_page
just check
```

Expected: the tests pass; `ls` prints one `.html` file per version row of the fixture, and the `ls` of `$T/originals` prints one file per stored original of the fixture; the two `grep` lines guarded by `!` exit 0 (every page has the meta, no page has a `style` attribute); `grep -c` prints at least `1` for every page; the diff line exits 0 (no new token, no raw colour); the literal test passes; the located-citation test reads a delivered page and passes; `just site` and `just check` exit 0.

## Negative control

In `shipped_name`, drop `.html` from the renderable list. `cargo nextest run --locked -p fieldbook-render --test source renderable_original_ships_as_txt` must fail and name the `.html` case. Restore the file. Then make `build.rs` skip the originals loop: `build::originals_are_copied_byte_for_byte_under_shipped_names` must fail. Restore the file. Then build the `WriteSet` from `edition/` alone: `build::originals_are_in_the_write_set` must fail. Restore the file. Then copy the original of a held version: `build::a_held_version_prints_held_and_ships_nothing` must fail. Restore the file. Then make the comparison accept any hash: `original_hash_mismatch_is_an_error` must fail. Restore the file.

Skip the Originals root check: `a_foreign_originals_folder_is_not_deleted` must fail. Restore it.

Remove only the hold reason: `held_citation_shows_reason_without_link` must fail. Restore it.

Bypass source-page filtering, then separately bypass original selection after valid preflight: `build_source_pages_and_originals_use_the_permitted_view` must fail in each case while permitted presence stays required. Restore each.

Redirect a located PDF citation to its `s/` page, then select the range's last page instead of its first, then link the citation of a held version: `pdf_citation_links_to_shipped_original_page` must fail on its `p.`, `pp.` and held case respectively. Restore each.

## Rollback

Revert the merge commit. `build` stops writing `s/` pages and copying originals, and a located citation goes back to its `s/` anchor. Later cards link to these pages, so revert any that have merged first, or FB0603 fails on their links.

## BLOCKED

`fieldbook build` has no `--out DIR` flag, or `build-registers` does not link claims by `claims.html#<id>`. Set the item to Blocked with a `BLOCKED:` comment naming the gap, and stop; do not add the flag here.

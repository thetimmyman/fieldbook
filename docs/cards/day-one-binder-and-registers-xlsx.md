# day-one-binder-and-registers-xlsx

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build` writes `day-one.html`, the day-one binder, and `assets/app.css` carries the print contract, while `cargo xtask print-count <pdf>` reports FB0607 for a printed brief of more than one page.

## Why

- `day-one.html` and what it holds are its row in [Site output paths](../reference/corpus-format.md#site-output-paths). The print contract is [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) and the Print bullet of [Site output paths](../reference/corpus-format.md#site-output-paths); the footer line is the `print_check` string in [Strings](../reference/corpus-format.md#strings).
- FB0607 and its severity are in [06: site and edition](../reference/corpus-format.md#06-site-and-edition); it is listed in `tests/known-bad/elsewhere.toml` under this card. Control WB-05 is in the [control register](../controls.md). The page count is the PDF's `/Count`, as the [restore drill](../runbooks/restore-drill.md) reads it.
- Design: the [MAP.md](../design/MAP.md) rows "Print" and `#templates` (TemplatesList), and the print half of `#brief-<id>` (BriefSheet). [HANDOFF.md](../design/HANDOFF.md) section 5 Components (BriefSheet, TemplatesList), section 8 Responsive behaviour (print), section 12 Implementation notes (print contract). Token groups: type, space.

## Scope

- Files: `crates/fieldbook-render/templates/{day-one.html,index.html}`, `crates/fieldbook-render/src/{lib.rs,day_one.rs,home.rs}`, `crates/fieldbook-render/assets/app.css`, `crates/fieldbook-render/tests/print.rs`, `crates/fieldbook/src/build.rs`, `xtask/src/{main.rs,print_count.rs}`, `xtask/tests/print_count.rs`, `xtask/tests/fixtures/synthetic-one-page.pdf` (new, synthetic), `CHANGELOG.md`.
- Registration lines: the Commands line of `AGENTS.md` (`cargo xtask print-count`); `tests/known-bad/elsewhere.toml`, whose FB0607 row gets its `test` key in step 6 (`check-known-bad` wrote the row); `crates/fieldbook-render/src/site.rs` (`planned_paths` entries, this card's paths only: `day-one.html`).; `crates/fieldbook-render/src/site.rs` (binder render call, Site insertion and planned path); `xtask/src/edition.rs` (the `day-one.html` entry in Site `REQUIRED_FILES` and its module-local test, this card’s entries only); `crates/fieldbook/tests/leak_outputs.rs` (binder surface); `crates/fieldbook-render/tests/frame.rs` (`built_entries_still_link` compares the HTML-escaped label, because the now-built `Templates & tools` nav entry renders its ampersand as `&#38;`). `xtask/tests/archive_release.rs` (its synthetic Site stage gains a synthetic `day-one.html`, which the Site `REQUIRED_FILES` entry now requires). `crates/fieldbook-render/src/page.rs` (the binder reuses the page's part builder, metadata row, notices, conflict boxes and change note: visibility changes and the extraction of the metadata row and superseded links, with no change to the page output).
- Docs to update: `AGENTS.md` (Commands: `cargo xtask print-count`), `CHANGELOG.md` (`- Added: the day-one binder and print styles.` under `## Unreleased`).

## Out of scope

- `data/registers.xlsx` and the CSV copies (`registers-xlsx`). Until then TemplatesList shows the `not_available` line in their place.
- Printing to PDF. Edge prints; the Windows release job runs the count (`transfer-and-windows-drill`). The Excel templates (E1).

## Preconditions

- Cards merged: `quote-match-and-anchors` (it reaches `calc-day1`, whose seeded generator `tests/support/rng.rs` step 5 includes).

## Steps

1. `day_one.rs` and `day-one.html`: the brief pages, then the pages of stages 1 and 2, in that order, each in its own `<article>` with the page's parts. A Corpus with none shows the empty-state line. `site.rs` calls the separately callable binder renderer with the build’s `LabelledView` and `BuildInfo`, inserts `day-one.html` into `Site`, and registers it in `planned_paths`. The existing binary pipeline writes that entry with the edition root. Later `binder@1` and `site@1` partition their outputs without duplicate paths.
2. `app.css`: one `@media print` block with the `@page` rule and body padding of the contract, a footer per printed page (product, edition, page id and the `print_check` string), citations as footnotes, status words printed as text, no `file:` address printed, a page break before each `<article>`, and BriefSheet at its sheet width. Hide navigation and buttons in print.
3. `home.rs` and `index.html`: add TemplatesList with the briefs, a link to `day-one.html` and the `not_available` line for `data/registers.xlsx`.
4. `print_count.rs`: read the PDF's trailer, the catalog and the page-tree root, and print `pages: N`. If `N > 1`, print an FB0607 line and exit 1. A file it cannot read exits 3. It uses no PDF crate.
5. Tests: in `tests/print.rs`, `day_one_lists_briefs_then_stages` (the fixture's brief), `one_page_rule_in_print_block`, `footer_has_edition_and_page_id`, `every_page_has_the_csp_meta_and_no_style_attribute` and `no_file_url_in_built_pages` (build the fixture site; no page and not `app.css` holds the text `file:`); in `xtask/tests/print_count.rs`, which run the built binary (`env!("CARGO_BIN_EXE_xtask")`) on a PDF given as the argument, `one_page_pdf_passes` and `two_page_pdf_is_fb0607` (the two-page fixture from `extract-and-locators`), `not_a_pdf_exits_3` and `parser_never_panics_on_200_seeded_inputs` (the file includes the generator with `#[path = "../../tests/support/rng.rs"] mod rng;` at file level, writes 200 files of 0 to 128 bytes from `Rng::new(1)` and `xtask/tests/fixtures/synthetic-one-page.pdf` with each 8th byte once flipped and once deleted to a temporary folder, and runs the binary on each; every exit code is 0, 1 or 3, never 101 or a signal).
6. In `tests/known-bad/elsewhere.toml`, set the `test` key of the FB0607 row to `two_page_pdf_is_fb0607`. Add and delete no row.
7. Mark the tests that carry a control row of [the controls table](../controls.md) with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: `// control: WB-05` on `no_file_url_in_built_pages`.

8. `delivered_binder_filters_articles` runs the real edition handler and reads the delivered binder: permitted article canaries appear, forbidden ones do not. Register the binder surface; omission wording has no numeric count because it is an edition payload, even when printable.

9. Add `day-one.html` to the Site `REQUIRED_FILES` in `xtask/src/edition.rs`, after the paths that card already requires and without adding it to the Raw profile’s requirements. The provider already exists through quote-match -> extract-and-locators -> release-workflow -> edition-zips, so this adds no prerequisite. Add `edition::tests::missing_binder_is_fb0601` in that module: first validate a healthy synthetic Raw-profile stage with no `day-one.html` and require success, using the shared profile/allowlist helper without a Raw-packager prerequisite; then stage a healthy Site payload, delete only `day-one.html`, and require FB0601 naming it. Raw editions do not gain a binder requirement.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test print
cargo nextest run --locked -p xtask --test print_count
just site
grep -c '@page' crates/fieldbook-render/assets/app.css
! grep -L 'http-equiv="Content-Security-Policy"' out/edition/day-one.html out/edition/index.html | grep .
! grep -l 'style="' out/edition/day-one.html out/edition/index.html
cargo xtask print-count xtask/tests/fixtures/synthetic-two-pages.pdf; echo "exit $?"
! git diff origin/main -- crates/fieldbook-render/assets/app.css | grep -E '^\+.*(--[a-z0-9-]+ *:|#[0-9a-fA-F]{3,8}\b)'
cargo nextest run --locked -p xtask literals::
cargo nextest run --locked -p xtask edition::tests::missing_binder_is_fb0601
cargo nextest run --locked -p fieldbook-render planned_paths_equal_the_site_keys
cargo nextest run --locked -p fieldbook --test leak_outputs
just check
```

Expected: both test runs pass; `just site` exits 0; `grep -c` prints `1`; the guarded lines exit 0; `print-count` prints `pages: 2`, an FB0607 line and `exit 1`; the literal test passes; `edition::tests::missing_binder_is_fb0601` passes for a healthy Raw-profile stage without the binder, then refuses the healthy Site stage after deleting only the binder; `just check` exits 0.

## Negative control

In `print_count.rs`, change the limit from one page to two. `cargo nextest run --locked -p xtask --test print_count two_page_pdf_is_fb0607` must fail. Restore the file. Then emit `file:///C:/x` in the print footer: `cargo nextest run --locked -p fieldbook-render --test print no_file_url_in_built_pages` must fail. Remove it. Then make `print_count.rs` `unwrap()` on the trailer: `cargo nextest run --locked -p xtask --test print_count parser_never_panics_on_200_seeded_inputs` must fail. Restore it.

Remove the binder insertion while retaining its planned path: `planned_paths_equal_the_site_keys` must fail. Restore it.

Render the binder from the unrestricted model: `delivered_binder_filters_articles` must fail. Restore it.

Bypass the Site required-file check, or add the binder to the Raw requirements: `edition::tests::missing_binder_is_fb0601` must fail. Restore it. Require a new source or prerequisite for the binder: the existing quote-match -> extract-and-locators -> release-workflow -> edition-zips chain must show no new edge. Restore it.

## Rollback

Revert the merge commit. `day-one.html` and the print rules go; pages still print with the browser's defaults.

## BLOCKED

A print rule cannot be met without a `style=""` attribute or an inline script (for example to number footnotes): set the item to Blocked with a `BLOCKED:` comment that says so, and stop.

# build-site-lints

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`build` lints the rendered site in memory before it writes a file. A link or anchor that does not resolve is FB0603, a page that breaks a site rule is FB0604, and a script or data file over budget is FB0605. Any of them stops the build with exit 1 and nothing written.

## Why

- The codes and their checks are in [06: site and edition](../reference/corpus-format.md#06-site-and-edition). The site rules come from [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) (CSP, no `style=""`, links, external URLs) and the budgets from [AD-15](../decisions/0001-foundation.md#ad-15-ui-technology-and-design-system). The paths a link may reach are in [Site output paths](../reference/corpus-format.md#site-output-paths).
- The controls are in [Site, workbooks and print](../controls.md#site-workbooks-and-print).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) row "CSP, no `style=""`, no external URL". [HANDOFF.md §12 Implementation notes](../design/HANDOFF.md#12-implementation-notes) (CSP, Links, the budgets). No token group.

## Scope

- Files: `crates/fieldbook-render/src/{lib.rs,lint.rs}`, `crates/fieldbook-render/tests/site_lints.rs`, `crates/fieldbook/src/build.rs`, `crates/fieldbook/tests/build.rs`.
- Registration lines: `tests/known-bad/elsewhere.toml` (the `test` key of the rows FB0603, FB0604 and FB0605 only).
- Docs to update: none. The codes already exist in `errors.md`.

## Out of scope

- FB0601, FB0602 and FB0608 on the edition and its zips (`edition-zips`). FB0607, one-page briefs (`day-one-binder-and-registers-xlsx`).
- The Edge harness and the net-log (`edge-harness`).
- Warning above the search index target (`search-ranking-and-index`).

## Preconditions

- Cards merged: `build-v0`.
- Its Scope shares no path with `build-tokens`, so the two may run together. It shares `render/src/lib.rs` with `build-frame`, so not beside that card.

## Steps

1. `lint.rs`: `lint(&Site, &OriginalNames) -> Vec<Diagnostic>`, a pure function over the paths and bytes `render` returns and the shipped names of the originals: `pub struct OriginalNames(pub BTreeSet<String>)` holds the names that `originals/` will contain (an HTML or SVG original is `<version-id>.<ext>.txt`). Read HTML with a small hand-written scanner of tags and attributes; no new crate.
2. FB0603: every `href` and `src` with no scheme, after `#fragment` is split off, resolves against the page's folder to a path in the `Site`. A directory link must name `index.html`. Resolve and validate the target first. An ordinary HTML fragment must equal an id in that page. A shipped PDF original may use `#page=N` for a positive integer N; search.html may use its documented `#q=<encoded query>` state. Neither exception allows a missing destination. Locator checks own PDF page counts, which OriginalNames does not contain. A link into the sibling `originals/` folder resolves against the `OriginalNames` the binary passes in: the shipped names (originals are not part of the site), so an HTML or SVG original is `<version-id>.<ext>.txt`. This card passes the empty set, because the `s/` pages and the `originals/` folder arrive with `build-source-pages`, which passes the shipped names. Report the page, the link and the line.
3. FB0604, one finding per breach: an `href="http`; a `style="` attribute; an `on…=` attribute; a page without exactly one CSP meta equal to `render::csp::CSP_STATIC`; a `<script>` that is inline or whose `src` is not a file under `assets/`.
4. FB0605 uses constants in `lint.rs`: `APP_JS: u64 = 30_000`, `SEARCH_JS = 10_000`, `ASSIST_JS = 20_000`, `SCRIPTS_TOTAL = 64_000` and `DATA_FILE = 3_000_000` (decimal bytes). Scripts are `assets/app.js`, `assets/search.js`, `assets/strings.js` and `assets/assist.js`; only files that exist count. Each named script has its individual limit, except strings.js which counts only in the total. Any other .js file counts as script unless it is `assets/search-index.js` or `assets/assist-data.js`: those two are data, each held to DATA_FILE. Report path, bytes and budget. The company search-data working-set limit is checked by its company card, not here.
5. `build.rs`: run the lint after `render` and before `Journal::begin_export`, so a refused build appends no write event. Any finding prints its diagnostic, writes nothing and exits 1.
6. Tests in `render/tests/site_lints.rs`, `mod site_lints`, each on a small synthetic `Site`: one test that raises each check of steps 2 to 4 (`dead_link_is_fb0603`, `style_attribute_is_fb0604`, `script_over_budget_is_fb0605`), one that passes clean, `fragment_must_exist`, `directory_link_needs_index_html`, `originals_link_resolves_by_name`, `budget_counts_all_scripts`, `remote_script_is_fb0604` (a `<script>` whose `src` is a remote address) and `external_href_is_fb0604` (an `<a>` whose `href` starts with `http`), each of which plants that one breach and expects FB0604 and nothing else. In `fieldbook/tests/build.rs`: `the_fixture_site_is_lint_clean` and `a_lint_finding_writes_nothing` (no file and no `write.begin`). `budgets_match_the_reference` reads the FB0605 row with `include_str!` and checks the five limits and the script/data classification.
7. Known-bad. The rows for FB0603, FB0604 and FB0605 are already in `tests/known-bad/elsewhere.toml` with the card `build-site-lints` (`check-known-bad` wrote them); set their `test` keys to `dead_link_is_fb0603`, `style_attribute_is_fb0604` and `script_over_budget_is_fb0605`. This card adds no row. `every_code_is_accounted_for` must pass.
8. Mark the tests that carry a control row of the controls table with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: WB-02 on `remote_script_is_fb0604` and on `external_href_is_fb0604`, and WB-03 on `script_over_budget_is_fb0605`.

9. Add `html_scanner_never_panics_on_200_seeded_inputs` in `tests/site_lints.rs`: use the shared `Rng::new(1)` for 200 byte strings and flip and delete each byte of valid HTML with multibyte text, tags and quoted attributes. Decode lossily; the scanner returns parsed values or a typed error with positions within the input, never a panic.

10. In `site_lints`, add `pdf_page_and_search_query_fragments_are_valid` and `fragment_exceptions_still_require_targets`: valid PDF #page=1 and encoded search state pass; #page=0, a noninteger selector, missing original and missing ordinary HTML anchor raise FB0603.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render site_lints::
cargo nextest run --locked -p fieldbook build::
just site
! grep -rl -E 'style="|href="http' out/edition
just check
```

Expected: the tests pass; `just site` exits 0 because the fixture and the lead's Corpus build lint-clean; the guarded `grep` exits 0; `just check` exits 0.

## Negative control

In `lint.rs`, turn off the `style="` rule. `site_lints::style_attribute_is_fb0604` must fail. Restore the file. Then add `<a href="missing.html">` to `base.html`: `build::the_fixture_site_is_lint_clean` must fail with FB0603 and name `missing.html`. Restore the file. Then add `<script src="https://example.invalid/x.js"></script>` to `base.html`: `build::the_fixture_site_is_lint_clean` must fail with FB0604. Restore the file.

Count search-index.js as a script or change DATA_FILE: `budgets_match_the_reference` must fail. Restore each.

Make the scanner assume every attribute has a closing quote: `html_scanner_never_panics_on_200_seeded_inputs` must fail. Restore it.

Require HTML ids for the valid PDF/query cases, or skip destination validation for them: the fragment tests must fail. Restore each.

## Rollback

Revert the merge commit. `build` writes without linting. Revert `search-ranking-and-index` first if it has merged.

## BLOCKED

- A page that an earlier card renders breaks a lint, and the fix is in a template that card owns: name the code, the page and the template, and stop.
- The 06 range and AD-12 disagree on a rule: name both lines and ask the lead.

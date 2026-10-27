# build-v0

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook build [--out DIR]` refuses a Corpus with errors (FB0600) and otherwise writes a minimal edition into `DIR/edition` (`DIR` is the parent folder and defaults to `out`): `index.html` from one base template with the CSP meta, `assets/strings.js` and `assets/app.css`. Two builds of one commit are byte-identical, and a test proves it (FB0606). The build appends one `write.begin` and one `write.end` to the audit journal (AL-01). `just site` builds into `out/edition`, and the CI `corpus` job runs `build`.

## Why

- Output paths, the CSP meta (rung 1), the header and the determinism rules are in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) and [Site output paths](../reference/corpus-format.md#site-output-paths). The verb and its flags are in [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience). The CI job is in [AD-08](../decisions/0001-foundation.md#ad-08-ci-design).
- FB0600 and FB0606 are in [06: site and edition](../reference/corpus-format.md#06-site-and-edition). The controls are in [Site, workbooks and print](../controls.md#site-workbooks-and-print) and [Determinism and arithmetic](../controls.md#determinism-and-arithmetic).
- The `unreleased_build` string and `strings::js()` are owned by [Strings](../reference/corpus-format.md#strings).
- The edition is a destination whose row is fixed to `public` ([ADR 0004](../decisions/0004-company-boundary.md#6-destinations)). The check that keeps a label out of it is FB0410, which makes FB0600 stop the build; the token is what `io::out` needs in order to write outside the data root, and the `Audited` token from `Journal::begin_export` is what every `io::atomic` write needs (AL-01).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) row "CSP, no `style=""`, no external URL". The `index.html` it writes is an empty shell that `build-home-and-areas` fills. [HANDOFF.md](../design/HANDOFF.md) [§12 Implementation notes](../design/HANDOFF.md#12-implementation-notes) (relative links, `<time>` dates) and [§0 Frozen strings](../design/HANDOFF.md#frozen-strings). No token group: `build-tokens` fills `app.css`.

## Scope

- Files: `crates/fieldbook-render/src/{lib.rs,site.rs,meta.rs,csp.rs}`, `crates/fieldbook-render/templates/base.html`, `crates/fieldbook-render/assets/app.css` (a one-line placeholder comment), `crates/fieldbook-render/tests/site.rs`, `crates/fieldbook/src/{lib.rs,cli.rs,build.rs}`, `crates/fieldbook/src/io/{out.rs,dirs.rs}`, `crates/fieldbook/tests/build.rs`, `justfile`, `.github/workflows/ci.yml`, `docs/reference/cli.md` (generated), `README.md`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook/tests/cli.rs` (remove `build` from `NOT_BUILT`, this card's verbs only); `tests/known-bad/elsewhere.toml` (the `test` key of the rows FB0600 and FB0606 only); `crates/fieldbook/src/io/mod.rs` (`mod dirs;` and `mod out;`, this card's entries only), `crates/fieldbook/tests/add.rs` (the raw writer scan includes folder operations, this card's entries only); `crates/fieldbook/tests/leak_outputs.rs` (edition surface rows and their negative plants, this card's entries only); `AGENTS.md` (Commands: the build and site commands, this card's entries only); `crates/fieldbook/tests/chips.rs` (remove only the parseable `fieldbook build` PENDING entry).
- Docs to update: `cli.md` by `cargo xtask docs`; `AGENTS.md` (Commands: `fieldbook build` and `just site`); `README.md` (the quick start line about `fieldbook build`); the `justfile` header comment (`site` has arrived); `CHANGELOG.md` (`- Added: the build command.` under `## Unreleased`).

## Out of scope

- The frame, the tokens and every page but `index.html` (`build-frame`, `build-tokens`, `build-page` and later).
- The site lints FB0603 to FB0605 (`build-site-lints`). `edition.toml` and the edition name in the header (`build-about`). `--since` (`edition-002`).
- The Edge harness (`edge-harness`) and zips (`edition-zips`).

## Preconditions

- Cards merged: `edition-manifest`, `calc-cases`, `core-policy`, `audit-journal`, `add-new-and-edition` (the shared test and writer seams).
- `check-ci-and-docs` has merged, so the `corpus` job and `cargo xtask docs` exist.

## Steps

1. `render::site`: `pub struct Site(BTreeMap<String, Vec<u8>>)` holds a relative output path and its bytes, with `Site::new()`, `insert(&mut self, path: &str, bytes: Vec<u8>)` (the path is relative with no `..`; a repeat is a bug and panics in debug builds) and `files(&self) -> &BTreeMap<String, Vec<u8>>`. `render(&LabelledView, &BuildInfo) -> Result<Site, MetadataError>` (it reads no clock or unrestricted model) writes `index.html`, `assets/strings.js` (from `strings::js()`) and `assets/app.css` (included with `include_str!`).
2. `render::meta` re-exports the core edition BuildInfo and BuildProfile from edition-manifest. The binary reads SOURCE_DATE_EPOCH and compile-time FIELDBOOK_GIT_SHA at the I/O boundary, validates them and supplies the core value. Preserve the full epoch in BuildInfo.workbook_epoch, with the fixed workbook fallback when absent; do not derive it from the calendar date or the clock. It adds no second released predicate. The header shows the date in a time element only when is_released is true; otherwise it shows unreleased_build.
3. `render::csp` holds both constants: `CSP_STATIC` (`connect-src 'none'`) and `CSP_BRIDGE` (`connect-src 'self'`), the strings of [Site output paths](../reference/corpus-format.md#site-output-paths) in the Corpus format. `CSP_STATIC` is on every page written to a Corpus or an edition; `CSP_BRIDGE` is only for the chat page that `fieldbook chat` renders at run time and never writes to a file, so `build` uses `CSP_STATIC` alone. `base.html` (askama) writes it as the meta tag, links the assets relative to the page, and has an empty `<main>`. No `style` attribute.
4. `fieldbook::build`: add `build [--out DIR]` to the clap `Cli` and remove `build` from the `NOT_BUILT` list of `verbs_match_ad17` in `crates/fieldbook/tests/cli.rs`. Remove the exact `fieldbook build` chip from PENDING; keep `fieldbook build --overlay` pending. `DIR` is the parent folder and the default is `out` under the root that `io::find_root` found: the site is written to `DIR/edition` (the folder is replaced as a whole), and later cards write `DIR/originals` and `DIR/dist`. Take the shared store lease and call `check::load` non-strict with the same `today` as `check`, and refuse on any error with FB0600 and exit 1, printing the errors; nothing is written and no journal event is appended. Keep the immutable checked snapshot, then release the read lease before opening the journal for writing. Nothing written depends on `today`. Run `dirs::check_build_roots(dir, &[BuildRoot::Edition])` before `Journal::open`; `build-source-pages` adds Originals. A foreign root is refused before any journal event. Then take the exclusive store lease and confirm the snapshot generation is unchanged; if it changed, release and reload before planning output, so a relabel during the transition cannot publish stale permissions. Open the journal only after that check, retain the lease through `write.end`, and capture `now` and `shipped`, and set `let eff = EffectivePolicy::floor(false)` once for this build. Select the edition view with `egress::check(&eff, &Dest::Edition, None, None, now, shipped)`. The labels are the constant `public` set, because FB0410 has passed and the `edition` row admits nothing else, and the selected view supplies the content accessors. `World::now()` is the method that `audit-journal` added, the only clock seam; its type is the `Now` of `audit-journal`. `policy-load` later replaces the `floor(false)` value with the run's `EffectivePolicy`. Build `LabelledView::new(&model, &selection, &ReadableStores::corpus())` and call `render(&view, &info)`. Render validates the shared core EditionInfo even while this card emits only the shell. Propagate MetadataError as exit 1 before replacement or write.begin; About later adds its presentation, not a new error path. Prepare one `Delivery` for the exact site bytes, parent marker and all directory operations; `build-source-pages` adds the originals before this approval. Scan the rendered text for markings. Call `egress::check(&eff, &Dest::Edition, Some(&labels), Some(&delivery), now, shipped)` and use only that final token at sinks. A denial appends one refusal and writes no output. Then `Journal::begin_export("build", &WriteSet { labels, files }, &allowed, dir)` with the label set `public` and every path of the `Site` under `DIR/edition` with its SHA-256; it returns the `Audited` token. Write with `io::out::replace(root: &Path, files: &BTreeMap<String, Vec<u8>>, allowed: &Allowed, audited: &Audited) -> Result<(), Error>`, called once for `DIR/edition` with `site.files()`; `build-source-pages` calls it again for `DIR/originals`. Repeat the root check before journal begin and again before removal. An absent root is valid; an existing root needs the regular parent marker `.fieldbook-out` containing exactly `fieldbook build output\n`. Otherwise exit 2, delete nothing and append no event, with `refusing to replace <root>: it was not made by fieldbook build; remove it yourself or choose another --out`. Write the marker through `write_outside` in the same write set. `replace` repeats the check and removes only the selected root through `dirs::remove_outside`, never DIR itself. It then writes every file through `io::atomic::write_outside` (it creates the parent folders). Then call `Journal::end`. An I/O error exits 3.
5. `justfile`: `site` runs `SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)" cargo run -q --locked -p fieldbook -- build --out out` (the site is `out/edition`). `ci.yml`: in the `corpus` job, after `check`, run the downloaded binary's `build --out out` with the same variable.
6. Tests. `render/tests/site.rs` (`mod site`): `csp_matches_the_reference` (reads `CSP_STATIC` from `corpus-format.md` with `include_str!`); `csp_constants_differ_only_in_connect_src` (splits both constants on `; ` and compares: only the `connect-src` directive differs); `every_html_has_one_csp_meta`; `no_style_attribute`; `unreleased_without_date`; `unreleased_without_commit` (a date and no commit is the unreleased form); `released_with_date_and_commit` (the method and rendered metadata agree); the missing-date and missing/invalid-commit cases assert is_released is false; `strings_js_is_strings_js`. `fieldbook/tests/build.rs` (`mod build`), spawning the binary as `tests/cli.rs` does. Each test in `build::` runs on its own temporary copy of `tests/fixtures/corpus-mini` (made by `temp_copy(fixture: &Path) -> tempfile::TempDir` in `crates/fieldbook/tests/support/mod.rs`, which `tests/add.rs` uses too), with its own state folder. `two_builds_are_identical` uses two distinct explicit output parents inside that copy: the first child process runs from its root with TZ=UTC, the second from its subfolder with TZ=Pacific/Auckland. Keep build metadata identical; compare relative edition paths and bytes, excluding journal state. The tests: `refuses_a_corpus_with_errors` (FB0600, exit 1, nothing written); `builds_the_fixture`; `build_out_is_the_parent_folder` (`build --out "$T"` writes `$T/edition/index.html` and no `index.html` in `$T`); `a_build_appends_one_write_begin_and_one_write_end` (the journal in the state folder gains one `write.begin` with the label `public` and one `write.end`, and `refuses_a_corpus_with_errors` shows that a refused check appends neither); `two_builds_are_identical` (the second build is a child process with `TZ=Pacific/Auckland` and another working directory and must still match; any byte that differs fails with FB0606 and the path); `a_rebuild_drops_a_page_the_corpus_no_longer_has` (build into `$T`, plant `$T/edition/p/gone.html` as the build of an earlier Corpus with that page would have left it, build again into `$T`: the file is gone and the folder equals a fresh build). Mark the tests that carry a control row of the controls table with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: DT-01 on `two_builds_are_identical`.
7. Run `cargo xtask docs` and commit the regenerated `cli.md`.
8. Known-bad. The rows for FB0600 and FB0606 are already in `tests/known-bad/elsewhere.toml` with the card `build-v0` (`check-known-bad` wrote them); set the `test` key of FB0600 to `refuses_a_corpus_with_errors` and of FB0606 to `two_builds_are_identical`. This card adds no row. `every_code_is_accounted_for` must pass.

9. Create `io/dirs.rs`: `BuildRoot { Edition, Originals }`, `Removable { BuildRoot(BuildRoot), PartialSnapshot, Snapshot }`, `check_build_roots(dir: &Path, roots: &[BuildRoot]) -> Result<(), Error>` and `remove_outside(audited: &Audited, allowed: &Allowed, path: &Path, what: Removable) -> Result<(), Error>`. Initially only BuildRoot operations are valid; backup adds its snapshot cases. The check is on canonical whole path components and the marker, and removal repeats it. Every mutating helper takes Audited and either RootPath or Allowed and returns OutOfScope outside its permitted path. Extend `io_writes_only_through_atomic` to cover raw `remove_dir_all`, `create_dir`, `create_dir_all` and `rename`; allow only `atomic.rs`, `journal.rs` and `dirs.rs` for folder operations. Add `a_foreign_edition_folder_is_not_deleted` and `a_second_build_removes_a_stale_file` in `tests/build.rs`; the first checks exit 2, unchanged keep.txt and no journal event.

10. Follow the output recipe for edition: register its real handler and final sink in `SURFACES`, prove successful public-shell delivery and check destination/payload binding. The empty shell claims no page-filter coverage; build-page adds title/body canaries and build-frame tests filtered navigation data; each built route tests rendered counts when it first displays them. Persisted output metadata keeps the input ItemRefs needed by `labels::effective_item`; reopening after source relabelling re-evaluates them. Run `output_surfaces_cover_built_sinks` and `real_outputs_keep_permitted_canaries_and_refuse_others`.

11. In `tests/build.rs`, `build_binds_every_output` runs a real build and checks every file and marker; `changed_build_bytes_or_target_are_refused` changes a planned byte or path after approval and requires refusal before that write. `public_shell_delivery_succeeds` verifies the actual shell, marker and files; unauthorized delivery refuses before creating a destination. Page canaries are tested by build-page when content is rendered.

12. Add `released_build_without_heading_preserves_output` in `tests/build.rs`: use valid release date/commit, a valid changes file without an edition heading, and existing owned output. The real build handler returns exit 1, preserves every output byte and emits no write.begin. Incomplete release metadata still yields the unreleased shell. All render callers and tests unwrap successful Result values through their normal test result handling.

13. Carry the metadata producer’s RenderContext through site rendering and planned-path computation, initially Public(EditionInfo) only. The public constructor and metadata refusal remain unchanged. Later local rendering extends the context, rather than calling the public constructor with company data.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render site::
cargo nextest run --locked -p fieldbook build::
just site
test "$(grep -c 'http-equiv="Content-Security-Policy"' out/edition/index.html)" = 1
! grep -rl 'style="' out/edition
cargo xtask docs --check
cargo nextest run --locked -p fieldbook --test leak_outputs
cargo nextest run --locked -p fieldbook --test chips
just check
```

Expected: the tests pass; `just site` exits 0; the CSP line count is 1; no file holds `style="`; `docs --check` prints `docs: up to date`; `just check` exits 0.

## Negative control

In the binary's output preparation, before final delivery approval, write the canonical output folder's path into the header bytes. `build::two_builds_are_identical` must fail with FB0606 and name `index.html`. Restore the file. Then, at that same binary preparation seam, write the value of the `TZ` variable into the header: the same test must fail. Restore the file. Then let `build.rs` go on to render when `check` reports an error: `build::refuses_a_corpus_with_errors` must fail. Restore the file. Then write the site straight into `DIR`: `build::build_out_is_the_parent_folder` must fail. Restore the file. Then call `Journal::begin_export` after `io::out::replace`: `build::a_build_appends_one_write_begin_and_one_write_end` must fail. Restore the file. Then make `replace` skip the removal: `build::a_rebuild_drops_a_page_the_corpus_no_longer_has` must fail. Restore the file.

Skip the marker check: `a_foreign_edition_folder_is_not_deleted` must fail. Restore it.

Temporarily bypass only the final sink’s delivery-binding comparison. First obtain valid delivery-bound `Allowed` and matching `Audited` tokens, then alter the bytes or target at the sink seam. Run byte and path cases separately with every other prerequisite valid: each refusal assertion in `changed_build_bytes_or_target_are_refused` must fail because the altered write succeeds; `build_binds_every_output` must fail too. Mutating before `begin_export` is insufficient because its independent check can reject first. Restore the sink comparison.

Restore the removed build chip in PENDING: the chip parser test must fail. Restore the list.

Bypass metadata failure: `released_build_without_heading_preserves_output` must fail. Restore it.

## Rollback

Revert the merge commit. `build`, `just site` and the CI build step go. Revert every later `build-*` card first.

## BLOCKED

- The lead's Corpus has an error, so the CI build refuses: write the code and the file and stop. Do not edit `corpus/`.
- The `corpus` job does not exist, or does not download the binary: stop; `check-ci-and-docs` owns it.
- Two builds differ for a reason this card cannot remove, such as an askama or crate default: name the byte and the crate and ask the lead.

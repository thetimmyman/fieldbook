# edge-harness

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`cargo xtask harness [--edition <dir>]` and `just browser` open every page and every renderable file of the edition in headless Edge with a net-log. They print the page count and `0 non-file requests`, record CSP violations and resource entries per page, and prove on a negative page that a remote script is blocked and reported. The `corpus` CI job runs it.

## Why

- The harness, its flags, the selftest and the negative page are in [AD-08](../decisions/0001-foundation.md#ad-08-ci-design). Home gate item 7 is in [Gates](../decisions/0001-foundation.md#gates).
- Controls: NS-03 and WB-01, and the harness half of ED-02, in [controls.md](../controls.md).
- Edge is the version preinstalled on the runner image and is not pinned ([Developer tools](../reference/dependencies.md#developer-tools)). Nothing is installed.
- Design: the [MAP.md](../design/MAP.md) row Edge from `file://`, cmd and PowerShell command chips, the headless Edge half; the Windows half is `transfer-and-windows-drill`. [HANDOFF.md](../design/HANDOFF.md) §13 Design evidence (Not yet verified: the AD-12 CSP in Edge) and §12 Implementation notes (CSP). Token groups: none.

## Scope

- Files: `xtask/src/{main.rs,harness.rs}`, `xtask/harness/{selftest.js,negative.html}`, `xtask/tests/fixtures/harness/{netlog-file-page-https.json,netlog-browser-own.json,dom-with-violation.html,dom-without-selftest.html,dom-nested-with-violation.html,dom-nested-without-selftest.html,edge-version.txt,netlog-file-page-preconnect.json,netlog-file-page-navigation.json,netlog-file-page-isolation-key.json,netlog-file-page-site-for-cookies.json,dom-text-viewer.html,dom-text-sniffed-as-html.html}` (new, synthetic), `xtask/Cargo.toml` (path edges to `fieldbook-core` and `fieldbook-render`), `Cargo.lock`, `.github/workflows/ci.yml`, `justfile`.
- Registration lines: the Commands line of `AGENTS.md` (`just browser` and `cargo xtask harness`).
- Scope expansion (the owner's Chromium fallback): the AD-08 harness paragraph of `docs/decisions/0001-foundation.md` and the Developer tools row of `docs/reference/dependencies.md`.
- Docs to update: `AGENTS.md` (Commands: `just browser` and `cargo xtask harness`), the `justfile` header comment (`browser` has arrived).

## Out of scope

- `search.js`, `search.html`, the golden wrappers (`golden_wrap`) and running the search goldens in Edge (`search-js`); the shortcut test (`search-palette`).
- The `windows` job and the print check on Windows (`transfer-and-windows-drill`).
- Any change to a built page to suit the harness.

## Preconditions

- Cards merged: `check-ci-and-docs`, `build-v0` and `build-about` (`edition.toml`, whose `edition` field the summary copies).
- The `corpus` job and `just site` exist.

## Steps

1. `harness.rs`: `cargo xtask harness [--edition DIR]`; `DIR` defaults to `out/edition`, and the renderable `.txt` originals are read from the `originals` folder beside it (`out/originals` by default; `build` writes both). Find Edge (`microsoft-edge` on Linux, `msedge.exe` on Windows); if none, exit 3 with "Edge not found". The finder is `pub(crate) fn find_edge() -> Result<PathBuf, String>` (its error is the text `Edge not found`), and the AD-08 flags are `pub(crate) const EDGE_FLAGS: [&str; 6]` (`--headless=new`, `--disable-background-networking`, `--disable-component-update`, `--disable-sync`, `--no-default-browser-check` and `--disable-features=OptimizationHints,MediaRouter`), both in `xtask/src/harness.rs`, for later drill steps. Copy the edition to `out/harness/edition/`, copy `xtask/harness/selftest.js` into that harness-owned folder once, at its root, and add one script element (a `<script>` whose `src` is the reference from that page to the script) as the first child of `<head>` in each copied HTML page, so the reference resolves inside the copy from any depth: `selftest.js` from a page at the copy's root, `../selftest.js` from `p/<id>.html`, and one more `..` for each further folder level. The reference comes from one pure function `pub(crate) fn selftest_src(page: &Path, root: &Path) -> Result<String, String>` (the copied page's path and the harness root; a page outside the root is an error), so that the rule is testable without Edge. The edition itself is never changed. The fix stays inside the copy: never weaken the CSP, never allow a remote script, and never inject into a `.txt` original. The path edges change the manifest: follow the shared [Review evidence](../reference/dependencies.md#review-evidence), run `just lock`, review and commit `Cargo.lock`, and obtain the actual Cargo/cargo-deny and MAX/JEV pin/profile/release-age evidence; leave the valid nightly-only lock configuration unchanged. The pull request records this Scope expansion before the card's last commit.
2. Run `--dump-dom --log-net-log=<file>` with exactly the flags of `EDGE_FLAGS` (step 1), once per copied HTML page and once per renderable `.txt` original. Never pass `--allow-file-access-from-files`. Run pages one at a time, each with a limit of 60 seconds; a page that is not done in that time fails the run and is named. A `.txt` original is opened unchanged from the `originals` folder, with no injected script and no copied shell around it: it loads as text, so the navigation and net-log checks apply to it, while the DOM marker of step 3 is required only on HTML pages, which alone can execute the selftest.
3. `selftest.js` is synchronous. It listens for `securitypolicyviolation` and, at `load`, writes the violations and `performance.getEntriesByType("resource")` into one `<pre id="fb-selftest">` as JSON of this shape: `{"violations": [{"directive": "script-src", "blocked": "https://example.invalid/x.js"}], "resources": [{"name": "file:///.../app.css", "initiator": "link"}]}`, where `directive` and `blocked` are strings from the event and `name` and `initiator` are strings from the resource entry. The harness reads it from the dumped DOM. An HTML page with any violation, or with a missing selftest result, fails; the CSP and the missing-selftest refusal are unchanged, and both apply at every depth of the copy, so a page in `p/` that cannot load `../selftest.js` is a missing result and fails rather than passing. A `.txt` original has no selftest and is judged by its navigation and net-log result alone.
4. Parse the net-log. Count requests whose initiator is a `file:` page and whose URL is not `file:`. Ignore the browser's own traffic. Any such request fails and names the page and URL.
5. `negative.html` loads `https://example.invalid/x.js` under the edition's CSP. Pass only if the net-log or the selftest shows the load blocked and reported. A negative page that reports nothing fails the run.
6. Print `harness: edge <version>` (the version from `--version` of the browser that ran; Edge is not pinned) as the first line of every run, before the first page opens. Then print `harness: <n> pages, <m> files, 0 non-file requests, negative page blocked (edge <version>)` and exit 0; exit 1 on any failure. On success, also write `out/harness/network-silence-summary.txt` in the form of [network-silence.md section 4](../intake/network-silence.md#4-how-to-read-network-silence-summarytxt): exactly these twelve lines in this order, each `name: value`, and never the `SYNTHETIC EXAMPLE` line of that section: `edition:` (the `edition` field of the `edition.toml` in the tree under test, so `unreleased` for the unstamped `just site` build and `edition-NNN` for a stamped one), `browser: Microsoft Edge <dotted version from --version>`, `options:` (every command-line option used), `pages loaded:`, `text-renamed originals loaded:`, `requests started by those loads:`, `requests to file: URLs:`, `requests to any other scheme or host:`, `self-test: pass|fail`, `negative page, requests blocked:`, `negative page, requests reported:` and `verdict: pass|fail`. A pure `summary_text` function in `harness.rs` builds the text from the counts, so a test can read it. `edition-zips` reads the file there and maps `{edge_version}` to the dotted version in `browser:`, `{n_pages_checked}` to `pages loaded:`, `{n_text_originals_checked}` to `text-renamed originals loaded:` and `{n_nonlocal_requests}` to `requests to any other scheme or host:`; this card writes no placeholder. Add `just browser` (`cargo xtask harness`) and the harness step to the `corpus` job after the build.
7. Tests in `mod tests` of `xtask/src/harness.rs` (`xtask` is a binary crate), over the recorded fixtures of `xtask/tests/fixtures/harness/`, read with `include_str!`: `https_request_from_file_page_fails` (`netlog-file-page-https.json`: one `https:` request from a `file:` page fails and names the page and the URL), `browser_own_request_is_ignored` (`netlog-browser-own.json`: the browser's own traffic is not counted), `selftest_src_is_relative_to_each_copied_page` (the pure function over the copied page paths of a harness root: `index.html`, `p/<id>.html` and `p/x/<id>.html` give `selftest.js`, `../selftest.js` and `../../selftest.js`, each joined onto its page's own folder resolves to the one harness-owned script, and a page outside the root is an error), `nested_page_gets_the_relative_selftest_reference` (the copy-and-inject step of step 1 on a temporary nested page: the copied `p/` page holds a `<script>` whose `src` is `../selftest.js`, and that reference resolves to the harness-owned file, so no page points at a `p/selftest.js` that was never copied), `selftest_violation_fails` (`dom-with-violation.html`: a selftest result with a violation fails; the nested case `dom-nested-with-violation.html` fails too and names the nested page), `missing_selftest_result_fails` (`dom-without-selftest.html`; the nested case `dom-nested-without-selftest.html` fails and names the nested page), `first_line_matches_version_pattern` (the first line of a run for the recorded `--version` output of `edge-version.txt`, `Microsoft Edge 131.0.2903.51`, matches `^harness: edge [0-9][0-9.]*$`), `summary_line_ends_with_same_version` (the summary line ends with the same version), `summary_has_the_twelve_lines_in_order` (a summary built from fixed counts and the recorded version has these twelve names in this order and no `SYNTHETIC EXAMPLE` line, and its `edition:` line equals the `edition` field of the `edition.toml` text it was given, `unreleased` in one case and `edition-007` in the other) and `summary_matches_attestation_format` (checks the permanent twelve-line contract in `docs/intake/network-silence.md` section 4: all names and counts, the `browser:` value `Microsoft Edge <dotted version>` and `verdict: pass`, using a hand-written matcher).

## Acceptance

```sh
cargo nextest run --locked -p xtask harness::
just site && just browser
just check
```

Expected: the tests pass; `just browser` exits 0, prints `harness: edge <version>` as its first line and then its summary line, with `0 non-file requests` and `negative page blocked`, and `out/harness/network-silence-summary.txt` exists with its twelve lines, `edition: unreleased` among them; `just check` exits 0. The `corpus` job log of the pull request shows the same line.

Dependency acceptance also requires the actual committed-lock Cargo/cargo-deny and exact-source MAX/JEV pin/profile/release-age evidence of the shared dependency recipe; no deferred command is assumed present.

## Negative control

The negative page is the control. Also, on the branch, make `selftest_src` return `selftest.js` for every page: `selftest_src_is_relative_to_each_copied_page` and `nested_page_gets_the_relative_selftest_reference` must fail, the second naming the nested page. Restore it. Change `negative.html` to load `x.js` from the same folder: the run must fail with "negative page: nothing blocked". Restore it. Drop the `requests to file: URLs` line from `summary_text`: `summary_has_the_twelve_lines_in_order` must fail. Restore it. Write `browser: Edge <version>` in place of `browser: Microsoft Edge <version>`: `summary_matches_attestation_format` must fail. Restore it. Emit `verdict: ok`: `summary_matches_attestation_format` must fail. Restore it. If the real negative page ever reports no blocked request, the card is BLOCKED.

## Rollback

Revert the merge commit. The `corpus` job stops running Edge and `just browser` is gone. NS-03 and WB-01 fall back to the site lints until it returns.

## BLOCKED

The negative page reports no blocked request, a page makes a request outside `file:`, or Edge rejects the CSP. These are stop-and-ask cases: write which page and which rule, and stop. The CSP rungs in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) are the owner's choice. A new CI permission also stops the card for the owner.

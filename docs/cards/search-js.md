# search-js

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`assets/search.js`, a classic script of at most 10 KB, answers a query from `window.FB_INDEX` with exactly the list and scores Rust gives. `search.html#q=` shows the results, and with scripts blocked it shows one line that links to `ids.html`. While the index loads, a Skeleton bar shows after 300 ms and not before. `cargo xtask harness` writes each golden file as a classic-script wrapper (`golden_wrap`), and the Edge harness runs every tokenize case (query side) and every ranked case through `search.js` and times 10 lookups.

## Why

- The engine, the loader, the search page and the no-script rule are in [The site index](../reference/search.md#the-site-index) and [Query procedure](../reference/search.md#query-procedure). The drift traps are in [Where Rust and JavaScript can drift](../reference/search.md#where-rust-and-javascript-can-drift); the budgets in [Budgets](../reference/search.md#budgets) and FB0605.
- Script rules are in [AD-15](../decisions/0001-foundation.md#ad-15-ui-technology-and-design-system) and WB-03 in [controls.md](../controls.md). This card owns the golden wrappers and what each kind asserts ([How the vectors are loaded](../../tests/golden/README.md#how-the-vectors-are-loaded)); `edge-harness` owns the harness that opens them.
- Design: the [MAP.md](../design/MAP.md) row Search (CommandPalette, SearchTrigger), the `search.html#q=` half; the palette is `search-palette`. [HANDOFF.md](../design/HANDOFF.md) §10 Search (result row), §6 States and interactions (no results) and §11 Edge cases (scripts blocked) and Index loading, and the §5 Skeleton row (the MAP row Skeleton). Token groups: colour (`--color-mark` for hits), type, space, shape and line (`--radius-sm`), motion (`--dur-pulse`).
- The search page shows [the page view](../reference/search.md#the-page-view), not the flat unit list; `tests/golden/search/page-view.json` holds `search.js` to the Rust view.

## Scope

- Files: `crates/fieldbook-render/assets/search.js`, `crates/fieldbook-render/templates/search.html`, `crates/fieldbook-render/src/search_page.rs`, `crates/fieldbook-render/assets/app.css` (the search page rules only), `crates/fieldbook-render/tests/search_page.rs`, `xtask/src/{main.rs,golden_wrap.rs,harness.rs}`, `xtask/harness/search_goldens.js`.
- Registration lines: `crates/fieldbook-render/src/lib.rs` (the search_page module line); `crates/fieldbook-render/src/site.rs` (render calls and Site inserts plus `planned_paths` entries, this card's paths only: `search.html` and `assets/search.js`). `docs/reference/search.md` (the site-index `v` row, unsupported-version fallback wording only).
- Docs to update: `CHANGELOG.md` (`- Added: search from the keyboard in the edition.` under `## Unreleased`). The search page has no command or verb, and `cargo xtask harness` is already in `AGENTS.md`.

## Out of scope

- The palette, the search trigger and the shortcuts (`search-palette`).
- Filters by kind and the HANDOFF metadata fields that the index does not carry (see BLOCKED for what to do if they are asked for).
- "Ask" on no results, beside "Not established in the library." (Day 30, `assistant-page`).

## Preconditions

- Cards merged: `search-site-index`, `edge-harness` (the harness) and `search-ranking-and-index` (the index builder that a ranked wrapper calls).

## Steps

1. `search.js`: normalise, the query-side lexer, binary search over `terms` by UTF-16 code units, integer sums, the last-token prefix at half impact, the exact class and the sort. Word characters use `/[\p{Alphabetic}\p{N}]/u`. No `fetch`, no module, no float in a score.
2. The loader adds one `<script src>` for `search-index.js` on the first focus of a search input, Ctrl K or `/`, or when `search.html` opens with a query in its hash. An index whose `v` is not 1 is not used: the loader disables ranking for that page and keeps the static ID-list fallback, showing `Search is unavailable for this index version. Use the ID list.` with the `ids.html` link, as the interface lane specifies in [Fixed template text](../design/HANDOFF.md). This is unfrozen fixed template text; it adds no frozen-string key or seed change. Scripts are running, and the existing scripts-blocked line stays reserved for blocked scripts. While the script loads, a Skeleton bar (one `<div>` with the class `skeleton`, marked `aria-hidden="true"`) is added by `search.js` only after `SKELETON_DELAY_MS`, a constant of 300, and removed when the index is ready, so a load that finishes sooner shows nothing. The delay is set with `setTimeout` and cleared with `clearTimeout`, and the index loads by appending a `<script>` to `document.head` and setting its `onload`, so that the harness can drive all three (step 7).
3. `search.html`: an input outside any `<form>`, results updated in place, Enter sets `#q=` with `encodeURIComponent`, the page reads the hash on load and on `hashchange`. It shows the first 50 results and the total. Each result row follows §10 with the fields the index has: the title with hits marked in `<mark>`, the snippet, the kind, status and dates, and a link only when its kind’s optional link form exists. A result without a link remains a readable title/snippet/metadata row, never href="" or a fabricated destination. Text goes in with `textContent` only. `search.js` exports `window.FB_SEARCH = { load, rank, render, run }`. `load() -> Promise<Index>` loads the ordinary site index; `rank(index, text, limit) -> { hits, total }` returns ordered hits with scores and unit metadata, using the explicit index; `render(hits, text, list)` renders exactly that ordered hit list through textContent. `run(text, limit, list)` is the existing convenience wrapper over the ordinary index and returns the total after rendering. Each row is `<li role="option" data-id="<unit id>" data-kind="<kind>">` with its link inside. Search calls run; the palette selects prefix-first hits and passes them to render without reranking. Assistant supplies its own permitted index to rank. `site::render` adds the page and asset to Site; in `crates/fieldbook-render/src/site.rs` add `search.html` and `assets/search.js` to `planned_paths`, so that `planned_paths_equal_the_site_keys` holds.
4. No results shows the abstention phrase read from `window.FB_STRINGS`, never a literal. With scripts blocked the page shows the §11 scripts-blocked line and a link to `ids.html`.
5. `app.css`: the result list and `mark` rules, using `var(--color-mark)` and the type and space tokens, and the `.skeleton` rule from the §5 Skeleton row: a 12px bar with `var(--radius-sm)`, shimmering from `var(--color-surface-2)` to `var(--color-border)` over `var(--dur-pulse)`, and still under `prefers-reduced-motion`. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF. No `style=""`.
6. `golden_wrap.rs` (`cargo xtask harness` calls it before it opens the search page): for each `*.json` under `tests/golden/` except `tests/golden/json/` (the report goldens, which a Rust test compares and no page reads), write `out/harness/golden/<path>.js` that assigns it to `window.FB_GOLDEN[<path>]` unchanged. For a ranked file, also write the index Rust builds from its `units` and `glossary`, in the form of `search-index.js`.
7. `harness.rs` and `search_goldens.js`: open a generated page that loads the golden wrappers, each ranked index and `search.js`; run the query-side tokenize cases and every ranked case; write the outcomes into the DOM. The harness fails on any difference and names the case id. It times 10 lookups on the fixture's index and prints the p95, advisory. The case `skeleton_waits_300ms` runs in a page of its own and replaces `setTimeout`, `clearTimeout` and the insertion of the index `<script>` with recorders (the script is recorded and not inserted). It requires: after the input's first focus, one timer of 300 ms is set and the page holds no `.skeleton`; after the recorded timer function runs, there is one `.skeleton`; after the case assigns `window.FB_INDEX` and calls the recorded script's `onload`, there is none; and when `onload` is called before the timer function, the timer is cleared and no bar appears. A version-mismatch case, `unsupported_index_version_keeps_the_id_list_line`, loads a `search-index.js` whose `v` is 2 and requires the index-unavailable line with its `ids.html` link, no ranked results, and no scripts-blocked line; no automatic migration or second fetch fallback is added. No DOM library is added.
8. `tests/search_page.rs`: the page carries the CSP meta once, has no `style=` attribute, links `ids.html`, and loads `search.js` with `src`; the static page holds no `skeleton` element; and `search_js_is_a_classic_script_within_budget` reads `search.js` with `include_str!` and requires at most 10,000 bytes, no `fetch(`, `eval(` or `new Function`, and no line that starts with `import` or `export`. Mark the tests that carry a control row of the controls table with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: put `// control: WB-03` directly above `search_js_is_a_classic_script_within_budget`, and none above `golden_wrap`. In `golden_wrap`, a unit test `golden_wrap`: a wrapper holds its JSON unchanged, and a ranked wrapper's index equals the one `fieldbook_core::search` builds from the same `units`.

9. Add ordinary non-seed cases with unequal-scoring versions sharing an unversioned ID or `origin_key`. Exact-class results use natural ID order alone; other results retain matched-count, summed-impact and natural-ID order. Share expected ranks between Rust, browser and CLI tests. Restoring score-first order inside the exact class must fail.

10. The real exported API is exercised by `ordered_hits_render_without_reranking`: a supplied hit list renders in exactly that order, and a separately passed permitted index cannot expose a unit from the ordinary site index. Keep absent-link rendering checks here. Do not implement palette selection inside `render`; the palette's one-character prefix case, deduplication, eight-hit cap and non-navigating Enter belong to its own controller tests in `search-palette`.

11. The shared result renderer treats an absent link as non-navigating: a rendered row without a link keeps its text and metadata and has no href. The palette controller, not `render`, owns Enter, the eight-hit cap, deduplication and the final Search for row; its `shortcuts.js` cases cover linked-result keyboard navigation and the non-navigating absent-link row. Future record views supply their link through UNIT_KINDS, not a renderer special case.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test search_page
cargo nextest run --locked -p xtask golden_wrap::
just site && just browser
grep -c 'http-equiv="Content-Security-Policy"' out/edition/search.html
grep -c 'style="' out/edition/search.html
grep -c 'var(--color-mark)' crates/fieldbook-render/assets/app.css
just check
```

Expected: the tests pass; `just browser` prints every tokenize and ranked case and `skeleton_waits_300ms` passing, `0 non-file requests` and the advisory p95; the greps print `1`, `0` and at least `1`; `just check` exits 0, so FB0605 holds `search.js` within 10 KB.

## Negative control

Make `search.js` skip the prefix match of the last token. `just browser` must fail and name ranked case ids, among them the seed case for the prefix. Restore it. Then change one byte inside a golden JSON file's wrapper output in `golden_wrap.rs` (for example, trim a trailing newline): `golden_wrap` must fail and name the file. Restore it. Then set `SKELETON_DELAY_MS` to 0: `just browser` must fail and name `skeleton_waits_300ms`. Restore it. Then add a call to `fetch` to `search.js`: `search_js_is_a_classic_script_within_budget` must fail. Remove it.

Rerank the explicit hit list or ignore the index argument: `ordered_hits_render_without_reranking` must fail. Restore each.

## Rollback

Revert the merge commit. `search.html`, `search.js`, the Skeleton rule and the golden wrappers go; `ids.html` still stands in for search, and `just browser` still runs the `edge-harness` checks.

## BLOCKED

A ranked case passes in Rust and fails in Edge on a character the drift table covers. Name the case and the character, and stop. If a reviewer asks for the §10 metadata line or filters the index lacks, stop and ask: it changes the index format in search.md.

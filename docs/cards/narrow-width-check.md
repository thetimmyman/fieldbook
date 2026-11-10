# narrow-width-check

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`just browser` loads a built page in a frame 375px wide and fails when the page scrolls sideways or a top-bar button is smaller than the control height, so the CSS below 768px has an automatic check.

## Why

- The need. The top bar was measured by hand in headless Chromium from 500 to 1600px when `search-palette` merged; 375px, the width the design names, was not measured, and the harness has no check of the CSS at any width: its shortcut pages carry a stub frame and load no `app.css`. A hosted knowledgebase is read on phones, so the narrow width is the one that matters most.
- The browser flags are fixed. AD-08 and `EDGE_FLAGS` in `xtask/src/harness.rs` allow `--headless=new`, `--dump-dom` and the net-log, and no window size. The viewport therefore cannot be set from outside; the page under test has to be framed at 375px. A page framed from a `file:` URL has an opaque origin, so the parent cannot read it: a probe script runs inside the frame and posts its result to the parent.
- What the harness already does. `prepare_copy` copies the built edition and injects the self-test script into every page, and `inject_script` and `selftest_src` do the injecting. This card adds one more injected script to one copied page, so the page measured is the built `index.html` with its own `assets/app.css` and the real top bar, and the net-log stays at zero non-file requests.
- Decisions: [AD-08](../decisions/0001-foundation.md#ad-08-ci-design) (the browser options and the harness), AD-15 (no byte budget is touched). Design: none built. The card builds nothing a reader sees; it checks `HANDOFF.md` section 8 (Responsive behaviour), the "Below 768px" row ("No page scrolls sideways at 375px") and the `Button` row (`--control-h`, 32px).

## Scope

- Files: `xtask/src/harness.rs`, `xtask/harness/narrow.html`, `xtask/harness/narrow.js`, `xtask/harness/narrow-probe.js`.
- Registration lines: none.
- Docs to update: none. The harness is described in AD-08 and does not change.

## Out of scope

- Any change to `app.css`, a template or `app.js`: the card reports a failure and does not fix it. A failure it finds is a separate card.
- A viewport option for the browser: the flags stay as AD-08 fixes them.
- Widths other than 375px, print and zoom.

## Preconditions

- Cards merged: `edge-harness`, `search-palette`, `claims-register-split` (it edits `harness.rs`, and its pull request changes the list of pages the harness opens).

## Steps

1. In `harness.rs`, add one narrow page next to the shortcut pages: `prepare_narrow_page(copy, harness)` writes a copy of the built `index.html` from the harness copy of the edition with `narrow-probe.js` injected beside the self-test (`inject_script`), and `narrow/narrow.html`, filled from `xtask/harness/narrow.html`, which holds one `<iframe width="375" height="800" src="...">` of that copy, a CSP that allows `frame-src 'self'` and its own script, and the same result list and summary elements as the other case pages. Its required case ids go in a `NARROW_CASES` constant. Run it through `run_case_pages` and `judge_case_page`, so its summary line is `harness: narrow: narrow/narrow.html: N cases, 0 failed`.
2. `narrow-probe.js` runs in the frame after load and posts one message to the parent with the measures; `narrow.js` in the parent turns the message into cases. The cases: `page_does_not_scroll_sideways_at_375` (`document.documentElement.scrollWidth` is at most `innerWidth`, and `innerWidth` is 375); `top_bar_does_not_scroll_sideways_at_375` (the top bar's `scrollWidth` is at most its `clientWidth`); `top_bar_buttons_keep_the_control_height` (every button and link in the top bar is at least 32px tall, and an icon button at least 32px wide). The cases fail, naming the element, and never pass because the probe did not run: a missing or late message is a failed case.
3. Unit tests in `mod tests` of `harness.rs` (they run as `harness::`): `narrow_page_frames_the_copy_at_375` (the generated page holds the iframe at width 375 and the CSP `frame-src 'self'`) and `narrow_probe_is_injected_once` (the copied page holds the probe script once, beside the self-test).
4. Run `just site && just browser` and record the three case ids and the narrow summary line in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p xtask harness::
just site && just browser
just check
```

Expected: the tests pass; `just browser` exits 0 with the line `harness: narrow: narrow/narrow.html: 3 cases, 0 failed`; `just check` exits 0.

## Negative control

In `app.css` of the temporary site build, add a rule that makes the top bar 400px wide at every width: `just browser` must fail, naming `top_bar_does_not_scroll_sideways_at_375` or `page_does_not_scroll_sideways_at_375`. Restore it. Then remove the injected probe script from the copied page: `top_bar_buttons_keep_the_control_height` must fail as a missing result, not pass. Restore it.

## Rollback

Revert the merge commit. The harness loses the narrow page and its three cases.

## BLOCKED

The frame does not report a measure because the browser refuses the framed `file:` page under the AD-08 flags: comment `BLOCKED: narrow frame under the AD-08 flags` with the net-log line, and stop. Do not add a browser flag; AD-08 is the owner's.

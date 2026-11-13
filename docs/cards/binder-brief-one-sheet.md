# binder-brief-one-sheet

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Each of the six real briefs prints on one US Letter sheet, both as its article in `day-one.html` and as its own page `p/<id>.html`, with the unreleased-build banner counted, so `cargo xtask print-count` reports `pages: 1` for each and the sixth printed binder page is the sixth brief's footer.

## Why

- The need. The first edition's Windows release job will count the pages of each printed brief and fail on more than one (`FB0607`, an error on Windows and a warning on Linux; see the FB0607 row and the Print bullet of [corpus-format.md](../reference/corpus-format.md)). Printed in headless Chromium from a build of `main`, the binder articles of the six briefs run to 2, 2, 3, 2, 3 and 3 Letter sheets, and the standalone pages to 2, 3, 3, 2, 3 and 3. Each brief is 431 to 455 words and 11 to 25 cites, inside the 400-word rule of `FB0138`, so the Corpus is not at fault. The Sources footnotes are: each row prints a claim chip, a status chip, a source chip, the full claim statement and a "Never verified" line, and the rows spill after the body. The first article also carries the binder head and the unreleased banner.
- The design wants a brief on one sheet with its footnotes. [HANDOFF.md](../design/HANDOFF.md) section 12 (print contract) and its Print row of the build-status banner say a brief prints on one Letter page, the banner counts inside that budget, and a brief that does not fit with the banner fails. The mockup's footnote is a short line, not a statement paragraph. Print is a layout family, not a token table: the Print tokens are `--color-bg`, `--shadow-raise` and `--shadow-pop`, and what hides in print is a layout rule.
- Lead decision: fix the density in print CSS, not by trimming the Corpus. The briefs meet `FB0138`, and a cut would remove cited text to fit a printer. A measured experiment, with existing tokens only, brought two of the six to one sheet and shortened the rest by levers that each keep every status word and locator: hide the claim statement under `.sheet` in print (it sits one click away on screen and on the source page), lay each footnote row out inline, keep `.sources-list` in two columns, drop the chip borders and icons in footnotes while keeping the status words and locators, tighten the list and part margins, hide the binder head in print and set the sheet text one step down.
- Lead decision: the rules apply under `.sheet`, so both the binder article and the standalone `p/<brief>.html` fit one sheet. The Windows drill (the draft `transfer-and-windows-drill`, step 4) prints standalone pages with Edge and counts them with `print_count`, so fitting only the binder would pass here and fail there.
- Lead decision: if no layout of existing tokens gets all six to one sheet with the banner, this card is BLOCKED with the measured page counts, and the choice between a design revision of the footnote type and a Corpus trim is the owner's. This card does not invent a token or a smaller type size than the Type table has.
- Design: [MAP.md](../design/MAP.md) rows `#brief-<id>`: BriefSheet and "Print: footnotes, one-page briefs"; [HANDOFF.md](../design/HANDOFF.md) section 12 (print contract) and the Print table of section 4; token groups Type, Space and Print, used as they stand. No new token, string or component. Nothing a reader sees on screen changes: every rule sits inside the last `@media print` block.
- The check that stays is the Edge count in the Windows drill. This card does not edit that draft or `harness.rs`; it leaves the Chromium loop in Acceptance as the Linux evidence.

## Scope

- Files: `crates/fieldbook-render/assets/app.css` (the last `@media print` block only), `crates/fieldbook-render/tests/print.rs`, `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Changed: each of the six briefs prints on one Letter sheet.`). No design document changes.

## Out of scope

- Corpus text: the six briefs are not edited, and no cite is dropped.
- The harness and the Windows release job: the Edge count of every printed brief belongs to `transfer-and-windows-drill`.
- The footer text on an ordinary page (`frame-page-id-hook`) and the calc, case and clause blocks of the binder (`binder-calc-case-clause-blocks`).
- Pages of the binder that are not briefs: only the briefs have a one-page rule.

## Preconditions

- Cards merged: `day-one-binder-and-registers-xlsx` (the binder, the print block and `print-count`; merged on origin/main).
- Beside: `views-and-day-n` and `views-and-day-n-2` (both open, later in the plan; shared path `crates/fieldbook-render/assets/app.css`, a registration file, so no precondition path is needed). Each edits other rules of the file; whichever merges second rebases and keeps both sides.
- Tools at home: `chromium` and `pdftotext` on the path, and a built `just site`. CI Linux runs the tests only.

## Steps

1. Build with `just site`, print each of the six standalone briefs and the binder with `chromium --headless=new --no-sandbox --no-pdf-header-footer --print-to-pdf=FILE URL`, and write the page counts in the pull request before the change (`cargo xtask print-count FILE`).
2. In the last `@media print` block of `app.css`, add rules under `.sheet` (the class the six brief articles and the standalone sheet share): the claim statement hidden; each `.source-row` inline with its number, body, chips and meta; footnote chips without border, padding or icon, their status words and locators kept; `.sources-list` in two columns; list items, parts, h3 and paragraphs with the nearest Space token; `.binder-head` hidden. Use `var(--...)` tokens only. `token_use_lint` in `tests/tokens.rs` (no raw number in a token-only property) must stay green; it is run in Acceptance.
3. Measure after each rule and keep a rule only if it lowers a count. If a brief is still over one sheet with every rule in, stop: BLOCKED (below).
4. In `tests/print.rs`, add `brief_one_sheet_rules_keep_status_words_and_locators`: read `app.css`, take the final print block as `one_page_rule_in_print_block` does, assert that no rule in it sets `display: none` on `.source-chips` or `.chip` (the status words and locators live in the chips, and `status words always print`), that the statement rule (`.source-stmt`) is under `.sheet` only, and that the block contains no raw pixel or rem value outside `var(` (the token rule, restated for the new lines). Add `one_sheet_rules_are_under_sheet`: every new selector begins with `.sheet` or `.binder-head`, so no ordinary page's print changes.
5. Run `just check`, then the Acceptance loop, and record the final page counts and the binder page count in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test print brief_one_sheet_rules_keep_status_words_and_locators
cargo nextest run --locked -p fieldbook-render --test print one_sheet_rules_are_under_sheet
cargo nextest run --locked -p fieldbook-render --test tokens token_use_lint
just site
mkdir -p "$TMPDIR/one-sheet"
for id in cmp-brief-audit-discovery cmp-brief-rule-changes cmp-cas-gaap-conformance-2026 est-brief-certified-data-threshold fnd-brief-first-meeting fnd-brief-research-reconciliation; do chromium --headless=new --no-sandbox --no-pdf-header-footer --print-to-pdf="$TMPDIR/one-sheet/$id.pdf" "file://$PWD/out/edition/p/$id.html" 2>/dev/null; cargo xtask print-count "$TMPDIR/one-sheet/$id.pdf" || echo "FAIL $id"; done
chromium --headless=new --no-sandbox --no-pdf-header-footer --print-to-pdf="$TMPDIR/one-sheet/binder.pdf" "file://$PWD/out/edition/day-one.html" 2>/dev/null
for i in 1 2 3 4 5 6; do pdftotext -f $i -l $i "$TMPDIR/one-sheet/binder.pdf" - | grep -c "Check the current edition"; done
just check
```

Expected: the three test commands pass; the `for` loop prints `pages: 1` six times and no `FAIL`; the second loop prints `1` six times (each of the first six printed binder pages ends with the footer, so each brief took one sheet; the seventh page is the first page that is not a brief); `just check` exits 0. The loop runs at home only; CI Linux runs the tests.

## Negative control

In a temporary edit, delete the new `.sheet` statement rule from the final print block of `app.css`, run `just site` and the first loop: at least one brief must print `FB0607` and the loop prints `FAIL <id>`. Then add `.sheet .source-chips { display: none; }` to the block: `brief_one_sheet_rules_keep_status_words_and_locators` must fail. Restore the file.

## Rollback

Revert the merge commit. The briefs print on two to three sheets again, and the Windows release job's count would fail on them.

## BLOCKED

A precondition has not merged: comment `BLOCKED: binder-brief-one-sheet waits on <slug>`, push the branch and take the next card. A brief is still over one sheet with every token-only rule in: comment `BLOCKED: binder-brief-one-sheet, <id> prints <n> sheets`, list the counts, and leave the Corpus and the design alone; the owner chooses a footnote-type design revision or a Corpus trim.

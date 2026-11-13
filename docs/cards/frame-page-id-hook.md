# frame-page-id-hook

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

The print footer of an ordinary reader page (`p/<id>.html`) shows the bare page id, as the binder footer does, and never the route `p/<id>.html`: the built page's frame carries `data-page-id="<id>"` and the print rule reads it.

## Why

- The need. The footer of a page printed from `p/est-rate-window-brief.html` reads `p/est-rate-window-brief.html · Check the current edition`, because the print rule prints `attr(data-route)`. The design's footer is the sample edition, the page id and the check line: the mockup's `printPageId(h)` strips the route to the bare id, and HANDOFF section 12 says the footer holds "the page id". The binder footer already prints the bare id (`print-foot` in `day-one.html`), so the same page prints two different footers.
- [HANDOFF.md](../design/HANDOFF.md) section 12 (print contract) and the Print row of the build-status banner; [corpus-format.md](../reference/corpus-format.md) Print bullet ("The footer shows product, edition, page id and 'Check the current edition'. ... No `file:` URL prints"). The design already specifies the id, so this card needs no design revision.
- Lead decision: derive the id in `frame.rs` from `output_path` when it has the form `p/<id>.html` (one path segment after `p/`), not a new `FrameInput` field. `FrameInput` has eleven call sites (area, page, ids twice, day_one, register twice, home, source, about, search page); a field would touch all of them for one page type, and the page route is already the one place that names the id.
- Lead decision: a route that is not a reader page (the home page, an area, a register, the ids index, a source page, the about and search pages) keeps `attr(data-route)` in print. The design does not give those pages a page id, and a footer must show something that names the sheet. A card that wants another footer for them is a design revision.
- Lead decision: `data-route` stays on the frame. `app.js` reads it (`.app[data-route]`) to resolve the current route, so the new attribute is added beside it and the old one is not renamed.
- Design: [MAP.md](../design/MAP.md) rows "Print: footnotes, one-page briefs" and `#brief-<id>`: BriefSheet; [HANDOFF.md](../design/HANDOFF.md) section 12; no token group (the Print tokens are not touched); no new string: the footer's words are the existing `strings::print_check` text. The screen does not change.

## Scope

- Files: `crates/fieldbook-render/src/frame.rs` (the render function and its template struct only; `FrameInput` is unchanged), `crates/fieldbook-render/templates/base.html` (the one `div.app` line), `crates/fieldbook-render/assets/app.css` (the last `@media print` block, the `.app::after` rule), `crates/fieldbook-render/tests/print.rs`, `crates/fieldbook-render/tests/frame.rs`, `CHANGELOG.md`.
- Registration lines: the nine page snapshots in `crates/fieldbook-render/tests/snapshots/` (`page__page__concept_page_renders.snap`, `page__page__procedure_page_renders.snap` and the `page_types__page_types__{brief,calc,case,checklist,clause_card,reference,template}_page_renders.snap` files): the one `div.app` line of each gains ` data-page-id="<id>"`. Other cards own these snapshots for their own page content, so this card only adds that attribute.
- Docs to update: `CHANGELOG.md` (`- Changed: a printed page's footer shows the page id, not its route.`). No reference document changes: the Print bullet already says "page id".

## Out of scope

- The binder footer and the one-sheet fit of the briefs (`binder-brief-one-sheet`).
- Footers of the pages that are not reader pages: they keep the route.
- Any new `FrameInput` field and any change to the call sites.
- The registers' snapshots (`registers__*`): their routes are not `p/<id>.html`, so they are byte-identical and stay untouched. If one changes, the card is BLOCKED.

## Preconditions

- Cards merged: `binder-brief-one-sheet` (`crates/fieldbook-render/tests/print.rs`; it runs first, since both edit the print tests and the last print block). `build-frame` and `day-one-binder-and-registers-xlsx` are merged on origin/main.
- Beside: `views-and-day-n` (open, later in the plan; shared paths `crates/fieldbook-render/src/frame.rs` and `crates/fieldbook-render/assets/app.css`). This card runs first; `views-and-day-n` rebases onto it and names `frame-page-id-hook` in its Cards merged when it is next edited. The nine snapshots also appear in the Files of the drafts `assistant-page` and `handoff-new-note`, which are later cards and rebase on this one.

## Steps

1. In `frame.rs`, add `fn page_id_of(output_path: &str) -> Option<&str>`: `Some(id)` when the path is `p/` + one non-empty segment without `/` + `.html`, else `None`. Add its value to the template struct that `render_page` fills, as `page_id: Option<&'a str>`. Unit tests in `mod tests` of `frame.rs`: `page_id_of_reads_reader_routes` (`p/est-rate-window-brief.html` gives `Some("est-rate-window-brief")`), `page_id_of_refuses_other_routes` (`index.html`, `day-one.html`, `ids/claims-CMP-1.html`, `p/a/b.html`, `p/.html`, `s/x.html` give `None`).
2. In `base.html`, write `{% if let Some(id) = &page_id %} data-page-id="{{ id }}"{% endif %}` on the `div.app` tag, after `data-route`. Nothing else on the line changes.
3. In the last `@media print` block of `app.css`, add `.app[data-page-id]::after` with `content: attr(data-page-id) "\0000a0\0000b7\0000a0Check the current edition";` under the existing `.app::after` rule, and keep the `.app:has(.binder)::after` hide. The footer's other rules (position, size, colour) stay shared by both selectors. No `file:` text, and no raw number outside `var(`.
4. Change `footer_has_edition_and_page_id` in `tests/print.rs`: its assertion that the final print block contains `attr(data-route)` stays (the fallback), and it adds that the block contains `attr(data-page-id)`. Replace its `data-route="p/est-rate-window-brief.html"` expectation by `data-page-id="est-rate-window-brief"` and `data-route="p/est-rate-window-brief.html"` both present on the built page.
5. Add `ordinary_page_footer_is_the_bare_id` in `tests/print.rs`: build the site from the fixture, and for `p/est-rate-window-brief.html` and `p/cmp-system-review.html` assert that the `div.app` tag has `data-page-id` equal to the id from the file name; that the final print block orders the `[data-page-id]` rule after the `data-route` rule (so it wins); that the text a reader sees as the footer (the attribute the rule prints, plus the check line) holds no `/` and no `.html` and no `file:`. For `index.html` and `ids.html` assert no `data-page-id` attribute on the frame.
6. Add `frame_has_page_id_only_on_reader_pages` in `tests/frame.rs`: render the frame with `output_path` `p/x.html` and with `index.html` and compare the two `div.app` tags.
7. Update the nine snapshot files named under Registration lines by hand, from the spec: the `div.app` line gains ` data-page-id="<id>"` after `data-route` (changing a snapshot recipe in [README.md](README.md#changing-a-golden-or-snapshot): run the tests, read each `.snap.new`, rename only when the diff is that one attribute).
8. Run `just check`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render frame::tests::
cargo nextest run --locked -p fieldbook-render --test print footer_has_edition_and_page_id
cargo nextest run --locked -p fieldbook-render --test print ordinary_page_footer_is_the_bare_id
cargo nextest run --locked -p fieldbook-render --test frame frame_has_page_id_only_on_reader_pages
cargo nextest run --locked -p fieldbook-render --test page --test page_types --test registers
just site && grep -c 'data-page-id=' out/edition/p/est-rate-window-brief.html
just check
```

Expected: every test command passes (a snapshot that differs by more than the one attribute fails the last test command); the `grep -c` line prints `1`; `just check` exits 0.

## Negative control

Remove the new `.app[data-page-id]::after` rule from the print block: `ordinary_page_footer_is_the_bare_id` must fail with the route in the footer. Restore it. Make `page_id_of` return `Some` for `index.html`: `page_id_of_refuses_other_routes` must fail. Restore it.

## Rollback

Revert the merge commit. The footer of an ordinary page shows its route again, and the nine snapshots go back with it.

## BLOCKED

A precondition has not merged: comment `BLOCKED: frame-page-id-hook waits on <slug>`, push the branch and take the next card. A registers or home snapshot changes: comment `BLOCKED: frame-page-id-hook changed a non-page snapshot` and stop.

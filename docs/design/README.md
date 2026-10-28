# Fieldbook design package

The interface design for Fieldbook: the handoff spec, a clickable mockup of the whole application, and a map from the mockup to the build.

| File | What it is |
|---|---|
| `HANDOFF.md` | The spec: routes, layout, tokens, components, states, AI integration, search, edge cases, evidence and acceptance checklist (revision 17) |
| `mockup/index.html` | A single self-contained HTML file. No network requests, synthetic data only |
| `MAP.md` | Each mockup route and component, its AD-12 output path, its section in `HANDOFF.md` and the card that builds it, by slug |
| `MANIFEST.txt` | SHA-256 of every file in this folder |

## Current revision 17

Stale summaries now read as the source contracts already say: the chip rule names the verbs of the AD-17 table, and its R2 examples are `fieldbook build --overlay` and `fieldbook sync`, tagged as whole commands; the `#feeds` landing is the public `Pull` stream `later-feeds`, whose snapshots are `sources.csv` origin rows; the sample brief id carries the area code `mkt`; the browser-storage summaries name the reference-only named context packs that ADR 0001's AD-15 storage rule already permits; `SourcesBlock` states the two numbered-row variants the build already has, a claim citation and a direct source-version citation; search names its unsupported index version state instead of the scripts-blocked line; and `PageActions` keeps Copy context pack on ORG pages under the complete-payload check and separate clipboard permission, with the selectable manual text as the fallback. The mockup changes only two sample command payloads, the command tag rule, and one containing-block property on the existing table wrapper, so tokens, component geometry and the existing layouts are unchanged and the existing study is reused. This revision's own evidence is the prototype check in section 13 of `HANDOFF.md`: copy buttons by pointer and keyboard at two widths in both themes, the blocked-copy feedback from a clipboard replaced in memory, the tag boundaries, page, body and `#main` scroll widths against their client widths with the wide table scrolling only inside its own wrapper, and no request outside the page. No Rust, Windows, print or full assistant check is claimed, and the earlier prose-only receipts stay historical.

## Revision 16 (historical)

Section 5 names each component's mockup root selector in one table, so a class the mockup gives to a component is read from that mapping instead of from the component's name; the mockup's selectors are unchanged and utility, descendant and variant classes are excluded. A `p.` or `pp.` citation of a version whose shipped original is a PDF links to that original at `#page=<first page>`, and a page range at its first page; every other locator kind keeps its `s/` anchor, and a held version stays plain text with its escaped hold reason. Tokens, component geometry and the revision 15 layouts are unchanged: no layout family moved, so the existing study is reused. Source checks and the browser render are recorded separately in section 13 of `HANDOFF.md`.

## Revision 15 (historical)

Purpose summaries and search snippets follow their separate source contracts; About metadata comes from edition-manifest. Assistant and drawer Stop invoke the same adapter cancellation method. The existing chat layout study is reused because no visible layout changes. Assistant copying checks complete payloads against independent clipboard permission and uses the existing dialog for selectable manual fallback. Home admission chips carry public confirmation; record results have no broken page link. The shared frame shows the selected public or company store context and effective labels, including permitted local marked content. About shows the compiled profile and posture. Assistant search stays destination-filtered; Stop discards the whole reply. Pricing landings follow P7, and desk commands and backup verification states match their reports. Existing tokens, compact tables and component geometry remain. Earlier notes are history; operative rules are in HANDOFF.md.

## What changed in revision 10 (historical)

The copy-paste Handoff is replaced by a chat built in, beside a live context panel (`HANDOFF.md` section 9). The Assistant page and the Ask drawer are one chat: your question, a thread note for each context change, the streamed and checked answer, and actions that land it in the app. The panel shows what the model reads, with a reason per item, pin, remove and add, a meter against the adapter's budget, and saved context packs; below 1040px of main width and in the drawer it is a sheet. One status line names the adapter and its state (connected, answering, no model here, connecting, error), and its capabilities switch controls, never the layout. With no model, Copy context pack and a paste area keep any outside assistant usable. The cite-check adds one status, "outside context", for a real item the answer did not read. The step card, step bar and mode switch are gone, and the Assistant labels in Frozen strings change (ADR 0002). `MAP.md` rows for Today, the Assistant page and the Ask drawer follow, and a row for the model adapter waits on the architecture contract.

## Revisions 6 to 9

Revision 9 settled the token tables, fixed template text and search order, and took in a design-system audit; its notes are at the top of `HANDOFF.md`. Revisions 6, 7 and 8 arrived together, after revision 5. The notes at the top of `HANDOFF.md` list each one's changes: 6 is a design critique pass (one 768px breakpoint, phone layouts, 24px targets), 7 makes the Ask flow quieter (one step card, closed folds), and 8 adds section tabs with a contextual sidebar and small sourced charts. `MAP.md` adds the new components, the `#areas` route and the two chart tokens. ADR 0001 rulings are unchanged.

## Precedence (AD-15)

[ADR 0001](../decisions/0001-foundation.md) governs formats, tools and delivery. This package governs layout, components and visual design. On a conflict, stop and ask the owner.

## Open the mockup

Open `mockup/index.html` in Edge or Chrome, from disk or from any static file server. Routes are hash links (`#today`, `#page-cmp-withholding`, `#states`). `?` lists the keyboard shortcuts. The mockup sets no start date; use the Day chip or Today's "Set your start date" to see Day N.

## The mockup is a reference, not shipped code

Codex ports it into askama templates and `assets/app.css`, `app.js`, `search.js` and `assist.js`, one card at a time, as `MAP.md` lists. The build emits one HTML file per page (AD-12), follows the CSP, and has no `style=""` attributes and no inline scripts. The mockup's hash routes, tabs, inline styles and sample records do not carry over.

Change this folder only by pull request.

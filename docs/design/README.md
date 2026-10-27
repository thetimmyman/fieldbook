# Fieldbook design package

The interface design for Fieldbook: the handoff spec, a clickable mockup of the whole application, and a map from the mockup to the build.

| File | What it is |
|---|---|
| `HANDOFF.md` | The spec: routes, layout, tokens, components, states, AI integration, search, edge cases, evidence and acceptance checklist (revision 15) |
| `mockup/index.html` | A single self-contained HTML file. No network requests, synthetic data only |
| `MAP.md` | Each mockup route and component, its AD-12 output path, its section in `HANDOFF.md` and the card that builds it, by slug |
| `MANIFEST.txt` | SHA-256 of every file in this folder |

## Current revision 15

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

# home-area-placeholders

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

On `index.html` and every `a/<area>.html`, each "not available until" placeholder is either the wording or omission the design gives for that section, or is listed for a design revision and left as it is, so a reader of the hosted site sees no placeholder that names an internal document.

## Why

- The producers, found by `grep -rn render_unavailable crates/fieldbook-render/src`. In the body of the page:
  - `area.rs`: "The process list is not available until Release 3.", "The procedure list is not available until Release 3.", "The module card is not available until Release 3.", "History is not available until a later release.", and "The template list is not available until E1." (only where the area has no template page).
  - `home.rs` (and again at the top of every area page, through `render_three_up`): "Continue working is not available until the ORG overlay, Release 2 or later."
  - `home.rs`: "Next up is not available until ADR 0001 decides its order rule." This names an internal decision record to a reader, and [ADR 0001](../decisions/0001-foundation.md) has no order rule for it (a search for "next up" and "unblocks the most" finds none).
- What the design gives ([HANDOFF.md](../design/HANDOFF.md) section 2 and section 5, rows of [MAP.md](../design/MAP.md) `ProcessDetails`, `Timeline` and `ModuleCard`):
  - History: the empty state "Nothing recorded yet. History fills as pages and registers change."
  - Next up (`NextUpRow`): "With no rows the list is left out." The order rule "depends on ADR 0001", and ADR 0001 gives none, so the list has no rows to show yet.
  - Processes: "Process records arrive in Release 3, with the first observed process."
  - Procedures: "Attestation arrives with procedure records in Release 3; there is no command before then", and a procedure with no attestation reads "Not attested".
  - Module: "Module status arrives in Release 3 (`modules.csv`)."
  - Continue working: the AD-09 landing line, which the page already shows. It stays.
  - Templates: nothing for an area with no template page. `TemplatesList` is the list of briefs, `data/registers.xlsx`, the Excel templates and the day-one binder, and it has no empty state.
- The count a reader meets. In a built edition, `index.html` holds 28 occurrences of "is not available until" and an area page 32 (`a/mna.html` and `a/cmp.html`). The frame accounts for 26 of them on every page: 20 sidebar entries (the unbuilt Work, Plan, Assistant and View entries, `templates/frame/sidebar.html`) and the three top-bar tabs Assistant, Work and Plan, which carry the line twice each, in a `title` attribute and in an sr-only span (6 occurrences, `templates/frame/topbar.html`). The rest are the body lines above: 2 on `index.html` and 6 on an area page. The frame is the design's and is not changed here; Step 4 lists its density for the interface lane.
- A conflict to put to the owner, as [the design README](../design/README.md#precedence-ad-15) requires. The frozen string `not_available` ([corpus-format.md](../reference/corpus-format.md#strings)) is "<Thing> is not available until <release or trigger>." and the three process, procedure and module lines of the design have a different shape ("... arrive in Release 3 ..."). The current wording is the frozen string and conforms to AD-09. Whether the design's wording replaces it is the owner's decision; this card does not decide it.
- Design: `MAP.md` rows `ProcessDetails`, `Timeline`, `NextUpRow`, `ModuleCard` and `ThreeUp`; `HANDOFF.md` sections 2 (the Process, Procedure, History event and Module card rows) and 5 (`NextUpRow`, `ThreeUp`, `TemplatesList`); token groups: none new. Its Acceptance checks the frozen strings, an empty-state line or "Unavailable" where data is missing, no `style=""` and the CSP meta.

## Scope

- Files: `crates/fieldbook-render/src/area.rs`, `crates/fieldbook-render/src/home.rs`, `crates/fieldbook-render/templates/index.html`, `crates/fieldbook-render/tests/home.rs`, `CHANGELOG.md`.
- Registration lines: none.
- Docs to update: `CHANGELOG.md` (`- Changed: the home and area pages no longer show a placeholder that names an internal document.`).

## Out of scope

- A History timeline built from git: the `fieldbook` program starts no process (only `xtask` does), and an edition has no git history. It needs a design for what an edition shows and a card of its own.
- The Templates empty state, the Continue working panel on area pages, the density of five placeholders on every area page, and the frame's 26 occurrences (the 20 sidebar entries in `templates/frame/sidebar.html` and the Assistant, Work and Plan tabs of the top bar, 6 occurrences, in `templates/frame/topbar.html`): the design does not give them. They need a design revision first, put in a pull request of its own to `docs/design/` by the interface lane.
- Any change to the frozen string `not_available`, to `strings.rs` or to its goldens.
- Process, procedure, module and Continue working records (Release 3 and the ORG overlay).

## Preconditions

- Cards merged: `build-home-and-areas`, `build-charts` (it extends `area.rs`) and `registers-xlsx` (it extends `home.rs` and `index.html`, and follows `day-one-binder-and-registers-xlsx`), so that no two cards in flight share those files.
- For Steps 3 and 4: the owner's answer to the conflict above, on the work item or in the pull request.

## Steps

1. History (`area.rs`, `history_html`): replace the `render_unavailable` call with the design's empty state, as a `<p class="muted">` line (the empty-state form `home.rs` already uses). It is one constant in `area.rs`, outside `strings`.
2. Next up (`home.rs`, `index.html`): the design leaves a list with no rows out. Render no `#next-up` panel while there are no rows, keep the `Briefs` panel in its place, and remove the text that names ADR 0001 from the page. Keep the function that would build the rows so the rule can be added later without changing the template.
3. Ask the owner whether the process, procedure and module lines take the design's wording or keep the frozen string. If the answer is "the design's", change those three constants in `area.rs` and add the "Not attested" cell the design names only if a procedure row exists (none does yet). If it is "keep", change nothing here and say so on the work item.
4. Write on the work item the four items the design does not give (the Templates empty state, Continue working repeated on area pages, the five placeholders per area page, and the frame's 26 occurrences: 20 sidebar entries and the three top-bar tabs, 6 occurrences), each with the page it shows on and the section or frame file it names, so the interface lane can take them.
5. Update the tests that name the changed lines: `empty_sections_show_their_line` and `index_lists_day_one_questions_and_briefs` in `tests/home.rs` (the area tests are in that file too). A new test in `tests/home.rs` builds the fixture site and asserts that no page of it contains `ADR 0001`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test home
just check
```

Expected: the tests pass; every `a/<area>.html` of the fixture has a History section with the design's line and no "History is not available"; `index.html` has no `Next up is not available` and no `ADR 0001`; `just check` exits 0.

## Negative control

Restore the `render_unavailable("Next up", "ADR 0001 decides its order rule")` call in `home.rs`: the new "no page names ADR 0001" test must fail, naming `index.html`. Restore the change.

## Rollback

Revert the merge commit. The pages show the AD-09 placeholders again.

## BLOCKED

The owner has not answered the wording conflict: do Steps 1, 2, 4 and 5 for the sections the design settles, leave the process, procedure and module lines as they are, and comment `BLOCKED: process, procedure and module wording needs the owner's answer on the not_available conflict`.

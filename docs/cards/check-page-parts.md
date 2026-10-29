# check-page-parts

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` splits every page body into its parts and raises FB0130 to FB0133 (the part headings, a blank part, text under `## Sources`, a Worked example without the synthetic label) and FB0137 (the Change note). The parts, with their lines, are kept for the later body and citation passes.

## Why

- The codes are in [01: pages, corpus.toml and files](../reference/corpus-format.md#01-pages-corpustoml-and-files). The page types and what each part holds are in [Page files](../reference/corpus-format.md#page-files). The part headings are the `parts` and `brief_parts` keys of [Strings](../reference/corpus-format.md#strings), and `fieldbook-core::strings` is their only source in the code.
- The Change note form is in [Change records](../reference/corpus-format.md#change-records); the change classes are in [Page vocabularies](../reference/corpus-format.md#page-vocabularies). The synthetic label is in [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance).

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,model.rs,page_parts.rs}`, `crates/fieldbook-core/src/schema/page_types.rs` (one assertion added), `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports part headings, blank parts, the Sources and Worked example parts and the Change note (FB0130 to FB0133, FB0137).`). `errors.md` does not change.

## Out of scope

- What a part must hold for its page type, raw HTML, images, outside links, brief length and defined terms (`check-page-body`).
- Citations, in the Sources part or anywhere else (`check-citations`).
- `note` pages. Under schema 1 a `note` is FB0108 (`check-pages`) and offers no parts.

## Preconditions

- Cards merged: `check-page-links`.

## Steps

1. Add `page_parts.rs`. Write `split_parts(page) -> Parts`: read the body with `check::markdown`, and cut it at each level-2 heading. Each part keeps its heading text, the line of the heading and the event range up to the next level-2 heading. Store the `Parts` of each page in the model, so `check-page-body` and `check-citations` read the same split. A heading inside a code block is not a heading; `pulldown-cmark` already says so. Store the parsed parts on PageEntry and expose `PageEntry::parts(&self) -> &Parts`; renderers use that accessor. Test `parts_accessor_returns_the_parsed_parts`.
2. FB0130 at the first heading that breaks the rule: the level-2 headings are not exactly `strings::parts()` (the eight-part types) or `strings::brief_parts()` (a `brief`), in order, with none missing, repeated or extra; or a level-1 heading is present; or a heading is deeper than level 3. A missing part is reported at the heading that stands where it should be, or at the end of the body.
3. FB0131 at the heading of a part other than `Sources` with nothing but whitespace before the next level-2 heading or the end of the body. A part that holds only the `empty_part` string is not blank. `Sources` is checked only by FB0132.
4. FB0132 at the first line of text under `## Sources`.
5. FB0133: a `## Worked example` part that holds anything other than the `empty_part` string, and whose first block is not a paragraph that is exactly the synthetic label. Keep the label in one constant in `page_parts.rs`, with a test that pins its text. It is not a `strings` key yet; see the pull request note in step 9.
6. FB0137, for the eight-part types only (a `brief` has no Change note): the part holds no entry; a line of the part is not `- CLASS, YYYY-MM-DD: sentence.` with the sentence ending in a full stop; the class is not a change class the Corpus accepts (`MATERIAL_CONTRACT_CHANGE` is overlay only and is refused); or the last entry, the oldest, is not `NEW_KNOWLEDGE`. Read the entries as source lines, one entry per line, at their lines.
7. An entry date that has the form but is not a real calendar date is FB0114, as [Dates](../reference/corpus-format.md#dates) says, not FB0137.
8. Report one finding per fault: a missing `## Change note` heading is FB0130 alone, and a blank Change note is FB0131 alone.
9. Unit tests named `check::page_parts::...`: each code; a `brief` with its six parts and no Change note raises nothing; a `###` subsection inside a part raises nothing; a `####` heading raises FB0130; a Worked example that holds `Not established.` raises nothing; an entry dated `2025-02-30` raises FB0114; and `schema::page_types::every_type_is_complete` (from `core-schema-pages`) now also requires that each type's template holds the parts of its type in order. In the pull request, say that the synthetic label is not a `strings` key.
10. If the fixture's `brief` page has a `## Change note`, remove it: the brief parts are six. Say so in the pull request.
11. Move the five codes from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture. If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request.

12. Tests in `check::page_parts`: `sources_heading_alone_is_valid`, `sources_text_is_fb0132` and `parts_never_panic_on_200_seeded_inputs`. Include the shared RNG at file level for tests; feed 200 byte strings and byte flips/deletions of a valid multibyte body with a Change note through `split_parts` and its date reader. Event ranges stay within the event list, positions stay within the page and nothing panics.

13. Expose heading anchors on Parts for every level-2/3 heading, keyed by event position. One pure `page_parts::heading_anchors` producer owns the algorithm in [search.md](../reference/search.md#page-heading-anchors); renderer and search consume its stored anchors. Test `heading_anchors_are_unique_and_deterministic` with Unicode, punctuation-only text, repeated headings, and a heading whose base already equals an allocated suffix. No filesystem or renderer dependency is added.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::page_parts
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with five new known-bad cases; `check` on the real Corpus prints `exit 0`; `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini` and swap the `## Purpose` and `## The rule` headings of its `concept` page. Run `(cd target/nc/corpus-mini && cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"; echo "exit $?")`: it must print an `error[FB0130]` line at the first swapped heading, and `exit 1`. Remove `target/nc/`.

Return empty parts: `parts_accessor_returns_the_parsed_parts` must fail. Restore it.

Remove the Sources exemption: `sources_heading_alone_is_valid` must fail. Make the part reader panic on malformed text: `parts_never_panic_on_200_seeded_inputs` must fail. Restore each.

Remove collision handling or return an empty anchor: `heading_anchors_are_unique_and_deterministic` must fail. Restore it.

## Rollback

Revert the merge commit. The five codes go back to `pending.toml`, and the model no longer holds parts.

## BLOCKED

The lead's committed pages raise one of these codes: write the code, the file and the line and stop. Fix the content in a pull request of its own (GS-02 judges it), then continue.

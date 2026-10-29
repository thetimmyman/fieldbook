# check-page-body

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` raises FB0134 when a page type's required content is missing, FB0135 for raw HTML, an image or a link to an address outside the Corpus, FB0138 when a `brief` is longer than 400 words, and FB0139 when a page defines a glossary term in bold.

## Why

- The codes are in [01: pages, corpus.toml and files](../reference/corpus-format.md#01-pages-corpustoml-and-files). The required content of each type and the body rules are in [Page files](../reference/corpus-format.md#page-files). The citation forms are in [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance); the calculation case form is in [Identifiers](../reference/corpus-format.md#identifiers).
- A placeholder or guidance comment left from a template reads as raw HTML, so FB0135 is what stops a half-filled page (Templates, in Page files).

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,page_body.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports required content by page type, raw HTML, images, outside links, long briefs and terms defined on a page (FB0134, FB0135, FB0138, FB0139).`). `errors.md` does not change.

## Out of scope

- Whether a citation is well formed or resolves (`check-citations`), and the number-unit lint (FB0309, `check-citation-rules`).
- Whether the calculation case named by `Fixture:` exists. A repository test of the calculation cards does that.
- The `Workbook:` line of a `template` page (E1).

## Preconditions

- Cards merged: `check-page-parts`.

## Steps

1. Add `page_body.rs` with one pass over the pages that have `Parts`, and register its module and pass in `check/mod.rs`. Read events only through `check::markdown`, so tables and task lists are on and nothing else is.
2. FB0134, at the heading of the part, for the rules of the Page types table:
   - `procedure`: `How it works` holds an ordered list of two or more items at its top level.
   - `reference`: `The rule` holds a table.
   - `case`: the level-3 headings of `How it works` are exactly `Public record` then `Inference`.
   - `clause-card`: the first block of `The rule` is a table whose six body rows are named, in the first column and in this order, as the Page types table names them; each value cell is the `empty_part` string or holds a claim key. Keep the names in one `CLAUSE_CARD_ROWS` constant in `page_body.rs`; `clause_card_row_names_match_the_reference` pins them against the Page types table in `corpus-format.md`.
   - `calc`: `Worked example` holds a line `Fixture: <id>`, and the id has the calculation case form. The form only.
   - `checklist`: `How it works` holds only task items and level-3 headings, and no `YYYY-MM-DD` date.
   - `template`: `The rule` holds a fenced code block whose info string is `text`.
   - `brief`: `Evidence` is a list in which every item holds a claim key or a source version key; `Unknowns` is a list in which every item holds a question key or is the `empty_part` string.
   - `concept`: nothing.
3. A "key" in step 2 is text that `[@` opens and that the id types of `core-ids-and-dates` parse as that kind of id. This is a test of presence. `check-citations` replaces it with the citation lexer and owns every fault of form.
4. FB0135 at each raw HTML block or inline (comments included), each image, and each link of `markdown::body_links` whose target has a scheme or climbs out of `corpus/`. An autolink with a scheme is an outside link. An address written as plain text raises nothing.
5. FB0138: a `brief` body of more than 400 words, a warning at the first line of the body. A word is a run of characters between whitespace in the text and code events of the body; headings count, front matter does not. Pin the count with a test of a brief of exactly 400 words and one of 401.
6. FB0139: trim the strong-span text and remove one terminal colon, if present, then trim again and compare without regard to case with the `label` and aliases of each glossary row. Without that internal colon, require `:` immediately after the span. A match is a warning at the span.
7. Unit tests named `check::page_body::...`: each rule of step 2 met and broken; `<!-- todo -->`, `<b>`, an image and `https://` and `../../../x.md` links for FB0135; a plain-text address; the 400 and 401 word briefs; both `**Material weakness:**` and `**Material weakness**:` against a synthetic glossary row, and a bold word that is not a term.
8. Move the four codes from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture. If a case of an earlier card now also raises one of these codes (a created page, for example), add it to that case's `also` and say so in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::page_body
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with four new known-bad cases; `check` on the real Corpus prints `exit 0` (warnings may print); `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini` and add a line `<!-- todo -->` under `## Purpose` of its `concept` page. Run `(cd target/nc/corpus-mini && cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"; echo "exit $?")`: it must print an `error[FB0135]` line at that line, and `exit 1`. Remove `target/nc/`.

Compare the internal-colon span without removing its colon: the `**Material weakness:**` case must fail. Restore it; the non-term bold-word case stays clear.

## Rollback

Revert the merge commit. The four codes go back to `pending.toml`.

## BLOCKED

- The lead's committed pages raise FB0134 or FB0135: write the code, the file and the line and stop. Fix the content in a pull request of its own (GS-02 judges it), then continue.
- A rule of the Page types table cannot be read as a test without a choice the reference does not make: write the type and the rule and stop.

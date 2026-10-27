# check-page-links

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` raises FB0110 to FB0113 (unknown page ids in `prereq`, `links` and supersession; a `prereq` cycle or a prerequisite at a later stage; unknown `terms`; unknown `questions` or `conflicts`), FB0115 (page supersession that is not reciprocal, names itself or loops) and FB0136 (a relative link in a page body that resolves to no file in the Corpus).

## Why

- The codes are in [01: pages, corpus.toml and files](../reference/corpus-format.md#01-pages-corpustoml-and-files). The fields and their rules are in [Page files](../reference/corpus-format.md#page-files); page supersession is in [Supersession](../reference/corpus-format.md#supersession); page links are in [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance).
- The `Index` of `check-register-links`, with the page ids `check-pages` added, resolves every id here.

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,markdown.rs,page_links.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports unknown page, term, question and conflict ids, prerequisite cycles, page supersession and broken page links (FB0110 to FB0113, FB0115, FB0136).`). `errors.md` does not change.

## Out of scope

- A link to an address outside the Corpus, raw HTML and images (FB0135, `check-page-body`). A link that climbs out of `corpus/` is FB0135, not FB0136.
- Citation keys in the body (`check-citations`).
- Splitting the body into parts (`check-page-parts`).

## Preconditions

- Cards merged: `check-pages`.

## Steps

1. Add `page_links.rs` with one pass over the pages that have a `PageMeta`.
2. FB0110 to FB0113 at the line of the field in the front matter, one finding for each unknown id, naming it.
3. FB0111: build the `prereq` graph; report a prerequisite at a later stage at the `prereq` line, and each cycle once, at the page whose id sorts first, naming the cycle's ids in sorted order so the message is stable.
4. FB0115 as Supersession gives it for pages: missing reciprocity in either direction, a page that names itself, a loop. Report a loop once, as in step 3.
5. Add `markdown.rs` with `pub struct Ev<'a> { pub event: Event<'a>, pub line: usize, pub col: usize }` and `pub fn events(body: &str, body_start_line: usize) -> Vec<Ev<'_>>`. It reads a body with `pulldown-cmark`, with the extensions of Body rules in [Page files](../reference/corpus-format.md#page-files) (pipe tables and task lists) and nothing else. `line` and `col` are 1-based in the page file, not in the body: `body_start_line` is the page-file line of the body's first line, and `col` counts code points. `check-page-parts` and `check-page-body` use it, so every body is read one way. In it, write `body_links(body: &str, body_start_line: usize) -> Vec<BodyLink>`, where `BodyLink { target: String, line: usize, col: usize }`.
6. FB0136: a target with no scheme, after its `#fragment` is dropped, targets the current page file when its path is empty; otherwise it resolves against the page's folder; when it stays inside `corpus/` and names no file in the `Tree`, raise FB0136 at the link. Keep the pulldown-cmark trap of [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance) in mind: `[@A][@B]` is not a link here, because no definition exists, and `check-citations` reads it.
7. Unit tests named `check::page_links::...` and `check::markdown::...`: each code; `check::markdown::lines_are_counted_in_the_page_file` (a body that starts at line 7 gives its first event line 7, not 1); a cycle of three pages and its stable message; a prerequisite with a later stage; `check::page_links::fragment_only_link_targets_current_page` for `#purpose`; a passing `other.md#purpose`; `missing.md#purpose` raises FB0136; a link that leaves `corpus/` and raises nothing here.
8. Move the six codes from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture. If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request.

9. `check::markdown::events_never_panic_on_200_seeded_inputs` includes the shared RNG at file level for tests. Read 200 byte strings and single-byte flips/deletions of a valid body with multibyte text, links and headings through `events` and `body_links`; positions remain valid page lines and code-point columns, and nothing panics.

10. For diagnostics on a successfully parsed front-matter field, use `PageMeta::field_location(key)` from the page parser. Do not infer a line by searching the raw text; comments and reordered keys keep their parser-recorded positions.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::page_links check::markdown
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with six new known-bad cases; `check` on the real Corpus prints `exit 0`; `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini` and make two of its pages list each other in `prereq`. Run `(cd target/nc/corpus-mini && cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"; echo "exit $?")`: it must print one `error[FB0111]` line that names both ids, and `exit 1`. Remove `target/nc/`.

Make `events` slice at an invalid UTF-8 boundary: `check::markdown::events_never_panic_on_200_seeded_inputs` must fail. Restore it.

Remove the empty-path branch: `check::page_links::fragment_only_link_targets_current_page` must fail. Restore it. FB0136 checks file existence, not fragment existence; the Corpus boundary and FB0135 ownership remain unchanged.

## Rollback

Revert the merge commit. The six codes go back to `pending.toml`.

## BLOCKED

The lead's committed pages raise one of these codes: write the code, the file and the line and stop. Codex may not edit `corpus/` before `guard-statuses`.

# search-site-terms

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

Search on the site finds glossary terms by their labels and aliases: a query of "wrap rate" or "fully burdened rate" on `search.html` lists the term `wrap_rate` with a link to its row in `glossary.html`, and `assets/search-index.js` stays within the size plan of the amended ADR 0006.

## Why

- The gap: the site index holds page and section units only ([The site index](../reference/search.md#the-site-index)), so a term the glossary defines is never a site result, while `fieldbook search` finds it. The amendment that `adr-0006-glossary-units` adds to [ADR 0006](../decisions/0006-edition-search-index.md) admits term units and sets the size plan; this card builds it.
- Term units already exist in `fieldbook-core::search` with their fields, aliases and link ([Document units](../reference/search.md#document-units)); the site serializer filters them out. Rust remains the one specification of the ranking, and `search.js` must agree with Rust over the index it is given ([Where Rust and JavaScript can drift](../reference/search.md#where-rust-and-javascript-can-drift)).
- `Review: owner`: the card changes which units the edition index holds, which ADR 0006 sets, and the FB0605 row's wording.
- Design: `MAP.md` rows for `search.html` and the palette; `HANDOFF.md` section 10 (the result row and the page view's other rows, which revision 19 says are always empty on the edition). The revision this card needs must say how a term row looks on the edition. No new token.

## Scope

- Files: `crates/fieldbook-render/src/search_index.rs`, `crates/fieldbook-render/assets/search.js`, `crates/fieldbook-render/tests/search_index.rs`, `crates/fieldbook-render/tests/search_page.rs`, `docs/reference/search.md` (The site index, Budgets), `docs/reference/corpus-format.md` (the FB0605 row), `CHANGELOG.md`.
- Docs to update: `search.md` and the FB0605 row as above; `CHANGELOG.md` (`- Added: site search finds glossary terms by their labels and aliases.`).

## Out of scope

- Claim, source, question and conflict units on the site.
- Any change of tokens, weights, synonyms or sort keys.
- The size reduction or index split of the amendment's plan, if the owner chose (a) or (c): that is a card of its own, written when the amendment is accepted.
- Any edit of `docs/design/`.

## Preconditions

- Cards merged: `adr-0006-glossary-units`, `search-glossary-aliases`, `search-palette-design-items` (it also changes `search.js`).
- The draft `r2-company-ids` also changes `search_index.rs`. If it has left `drafts/` and is in flight, this card waits for it to merge.
- A revision of `docs/design/` after revision 21 that shows a term row on the edition's search page and palette has merged: it is the interface lane's pull request, accepted by the owner.

## Steps

1. In `search_index.rs`, admit units of kind `term` beside `page` and `clause`, with the term's link to its `glossary.html` row. Keep the serialization order and the URL stripping.
2. In `search.js`, render a term row as the design revision says, in the page view's other rows, with `textContent` only. Keep `search.js` within 10 KB.
3. Tests in `tests/search_index.rs`: `the_edition_index_holds_term_units_with_aliases` (a fixture term with an alias is in `docs`, and its alias tokens have postings) and `the_edition_index_holds_no_claim_units`. In `tests/search_page.rs`: a term row has its link and no `innerHTML`.
4. Run `just site` and record the size of `assets/search-index.js` against the amendment's figure, and the result rows for "wrap rate" and "T&M", in the pull request.
5. Update `search.md`, the FB0605 row and `CHANGELOG.md`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-render --test search_index
cargo nextest run --locked -p fieldbook-render --test search_page
just site && grep -c '"wrap_rate","term"' out/edition/assets/search-index.js
just browser
just check
```

Expected: the tests pass; the `grep` prints `1`; `just browser` exits 0 with the Rust and JavaScript agreement checks passing; `just check` exits 0.

## Negative control

Filter term units out of the site index again in `search_index.rs`: `the_edition_index_holds_term_units_with_aliases` must fail. Restore it. Admit claim units as well: `the_edition_index_holds_no_claim_units` must fail, and `just site` must fail with FB0605 on the real Corpus. Restore it.

## Rollback

Revert the merge commit. The site index holds pages and sections only again; the amendment stays accepted and unbuilt.

## BLOCKED

The design revision has not merged, or the built index is over the amendment's cap: comment `BLOCKED: <what>` on the work item with the measured size and take the next card. Never raise the cap in code beyond what the amendment states.

# adr-0006-glossary-units

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

[ADR 0006](../decisions/0006-edition-search-index.md) carries an accepted amendment that lets the edition search index hold glossary term units with their aliases beside page and section units, and that states the measured size of that index and the plan for staying inside its cap.

## Why

- The gap. Search on the site never finds a glossary term: the index holds page and section units only ([The site index](../reference/search.md#the-site-index)). A reader who types "wrap rate" finds no page that says it, and the term `wrap_rate`, whose alias is "fully burdened rate", is not a result there. The command line finds the term, because `fieldbook search` indexes every unit.
- The size. The index was about 14.7 MB against a 15 MB cap when ADR 0006 was accepted, about 2% headroom. Term units add bytes, and so does every new page of the Corpus cards (`kb-rates-practitioner`, `kb-hub-pages`, `kb-fpa-core`). At about 8.6 KB per page, the headroom holds about 35 more pages. The amendment must say what happens before the cap is reached, not after.
- ADR 0006 is the owner's decision, so its amendment is `Review: owner`. It changes no Corpus file and no format: FB0605 keeps its meaning.
- Design: none here. `search-site-terms` builds the reader-facing rows after a design revision.

## Scope

- Files: `docs/decisions/0006-edition-search-index.md`.
- Docs to update: the ADR only. `search.md`, `corpus-format.md` (the FB0605 row) and `docs/design/` follow in `search-site-terms` and its design revision.

## Out of scope

- Any code, and any change to `search-index.js`.
- Query-side synonym expansion: [Synonyms](../reference/search.md#synonyms) already expand glossary aliases in both directions on the index side.
- Adopting SQLite or tantivy (`later-sqlite-tantivy`).

## Preconditions

- Cards merged: none.

## Steps

1. Measure on the current `main`, in a scratch change that is not committed: build the site index with and without term units of kind `term` (fields as [Document units](../reference/search.md#document-units) gives them, aliases included) and record both sizes of `assets/search-index.js` in bytes, and the gzipped sizes, in the pull request. Count the pages each planned Corpus card adds (`kb-rates-practitioner`, `kb-hub-pages`, `kb-fpa-core`) and estimate their bytes from the measured mean per page.
2. Write the amendment as a section `## Amendment: glossary term units` at the end of the ADR, with the status line updated. It says: which kinds the edition index holds (`page`, `clause`, `term`); the measured sizes of Step 1; and the size plan, one of these as the owner chooses on the pull request: (a) keep the 15 MB cap and pay for the terms and the planned pages from the index itself, by a named reduction measured in Step 1 (for example, a shorter snippet for section units); (b) raise the cap to a stated figure, with the load time that `edge-harness` measures on the largest file; (c) split the index into two files loaded on first focus, each under its own cap. Recommend one, with the figures.
3. State the trigger that reopens the decision: the file within 300 KB of its cap fires the same review as reaching it.
4. Ask the owner on the pull request to accept the amendment. Do not merge before the answer.

## Acceptance

```sh
rg -n '^## Amendment: glossary term units' docs/decisions/0006-edition-search-index.md
typos docs/decisions/0006-edition-search-index.md
just check
```

Expected: `rg` prints one line; `typos` exits 0; `just check` exits 0. The pull request holds the Step 1 table and the owner's acceptance.

## Negative control

Rename the amendment heading to `## Glossary term units`: the first Acceptance command must exit 1 and print nothing. Restore it. Add a misspelt word such as `amendmnet` to the section: `typos` must exit 2 and name the line. Restore it.

## Rollback

Revert the merge commit. The index holds page and section units only, as before; `search-site-terms` cannot start.

## BLOCKED

The owner does not accept the amendment, or Step 1 shows that term units alone push the file over 15 MB with no reduction that keeps every page: comment `BLOCKED: <what>` on the work item with the figures and take the next card.

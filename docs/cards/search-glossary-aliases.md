# search-glossary-aliases

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook check` on the real Corpus runs six new regression cases, one for each of the queries "T&M", "wrap rate", "fringe pool", "subcontract flowdown", "FAR 31.205" and "fixed price incentive", and reports none of them as a miss, because the glossary carries the terms and aliases that a reader types.

## Why

- The measurement. Twenty everyday queries were run with `fieldbook search` on `main`; fourteen found the right page and six did not. "T&M" ranked `mna-additional-information` first: `&` separates tokens ([Word characters](../reference/search.md#word-characters)), so the query is `t` and `m`, and no glossary row joins those to "time and materials". "wrap rate" ranked `cmp-alcoholic-beverages` third: no page says it, and the glossary row `wrap_rate` came 11th. "fringe pool", "subcontract flowdown", "FAR 31.205" and "fixed price incentive" ranked clause prescriptions and unrelated pages above an explanation.
- The mechanism exists. [Synonyms](../reference/search.md#synonyms) appends the tokens of every other member of a glossary set to a unit that contains one member, on the index side, in both directions, for the command line and the site alike. A glossary row labelled "time-and-materials contract" with the alias `T&M` makes a query of `t m` find the pages that say "time and materials". The ranking is not changed: the regression rule is that "nobody tunes the ranking to pass a case" ([Regression set](../reference/search.md#regression-set)).
- The pages the cases expect come from the Corpus cards: the wrap-rate and pool pages of `kb-rates-practitioner`, the cost-principles overview and the contract-type pages of `kb-hub-pages`.
- An agent may append a regression case by commit, and never edits one or sets `critical` ([Adding a regression case](README.md#adding-a-regression-case)).
- Design: none. The card changes Corpus data only.

## Scope

- Files: `corpus/registers/glossary.csv`, `corpus/eval/regression.toml` (appended cases only).
- Docs to update: none.

## Out of scope

- Term units in the site index: `search-site-terms`.
- Query-side expansion, a stop-word list, stemming or any change to `fieldbook-core::search`.
- Editing, reordering or removing an existing regression case.

## Preconditions

- Cards merged: `kb-hub-pages`, `search-ranking-warnings` (the set then judges the page view).

## Steps

1. For each of the six queries, pick the page that answers it, from the pages `kb-rates-practitioner` and `kb-hub-pages` added or an existing page (for "subcontract flowdown", the page that explains flowdown of subcontract clauses). Record the query, the page id and its rank today in the pull request.
2. Add or extend glossary rows at entry values, with `definition_status` and `source_ref` as [corpus-format.md](../reference/corpus-format.md) asks: a time-and-materials contract with the aliases `T&M` and `time and materials`; a fixed-price incentive contract with `FPI` and `FPIF`; a flowdown clause with `flow-down` and `flow down`; the FAR cost principles with `FAR 31.205` and `selected costs`; `fringe pool` as an alias of `fringe_benefits`; keep `wrap_rate` and its alias. Each row's definition is one sentence a public source supports.
3. Append six cases, `rg-028` to `rg-033`, with `expect` the page ids of Step 1 and the default `top`.
4. Run `fieldbook check`. A case that still misses gets no ranking change: improve the page's title, heading or lead wording under `kb-hub-pages` rules in a follow-up, or leave it (see BLOCKED).

## Acceptance

```sh
cargo run -q --locked -p fieldbook -- search 'T&M'
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)" --json | grep -cE '"code": "FB012[678]"'
cargo xtask guard-statuses origin/main HEAD
just check
```

Expected: the first page row of the search is the time-and-materials page of Step 1; the `grep` prints `0` (grep exits 1 for no match, which is the expected result); the guard exits 0; `just check` exits 0.

## Negative control

In a temporary copy of the Corpus, delete the alias `T&M` from the new glossary row: `fieldbook check` on the copy must report `FB0127` for the case of "T&M". Restore it. Commit a change to the `query` of `rg-001` on a scratch branch: `cargo xtask guard-statuses origin/main HEAD` must fail and name `corpus/eval/regression.toml`. Drop the scratch commit.

## Rollback

Revert the merge commit. The six cases and the glossary rows go; the pages stay.

## BLOCKED

A case still misses after its page says the words a reader types: comment `BLOCKED: <case id>, <rank among page rows>, <what was tried>` on the work item, remove that case from the pull request rather than weaken it, and take the next card.

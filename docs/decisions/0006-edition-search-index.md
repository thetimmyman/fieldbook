# Edition search index

Status: accepted by the owner. It supersedes the edition index budget in AD-11, the data-file line of AD-15 for `assets/search-index.js`, and the edition index figure in the `later-sqlite-tantivy` row of AD-24, all in [ADR 0001](0001-foundation.md). It decides nothing else.

## Trigger

`later-sqlite-tantivy` in AD-24 fires on an edition index above 3 MB. Measured on the real Corpus, an index of every unit is about 34.7 MB and an index of page and section units alone is about 14.7 MB. FB0605 would refuse either.

## Decision

The edition's site search index, `assets/search-index.js`, holds page and section units only, the kinds `page` and `clause`. Its cap is 15 MB, and the 1 MB target is dropped. Every other data file keeps its 3 MB cap. The trigger fired and the owner chose this over adopting SQLite or tantivy; the `later-sqlite-tantivy` figure for the edition index is now 15 MB.

`fieldbook search` still searches every unit. Claims, sources, questions, conflicts and terms stay reachable on the site through the register pages and `ids.html`, and are not search results there. The overlay site index and its shards are unchanged. The specification is in [search.md](../reference/search.md#the-site-index).

## Effect

- A result list on the site and on the command line can differ, because the two indexes have different units, and so different document frequencies and field lengths. Over the 24 positive regression cases, with results grouped by page, the expected page is in the first 5 for 18 cases on a pages-only index and 19 on the full index, and in the first 10 for 21 on both.
- The headroom is small: about 14.68 MB against 15 MB, about 2%. Growth in pages or sections can reach the cap soon, and that fires the same trigger again.
- Load time for a file of this size has not been measured. `edge-harness` and `search-js` measure it.
- FB0605 is a lint on built site output and changes no Corpus file, so there is no schema change and no ADR 0002 amendment. The code keeps its meaning, a data file over its byte budget; only the threshold for this file moves.

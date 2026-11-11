# Edition search index

Status: accepted by the owner. It is amended by [glossary term units](#amendment-glossary-term-units), which is accepted by the owner-delegated judge and JEV review, as in [ADR 0007](0007-hosted-edition.md). It supersedes the edition index budget in AD-11, the data-file line of AD-15 for `assets/search-index.js`, and the edition index figure in the `later-sqlite-tantivy` row of AD-24, all in [ADR 0001](0001-foundation.md). It decides nothing else.

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

## Amendment: glossary term units

Status: accepted by the owner-delegated judge and JEV review, with size plan (a): section snippets of at most 100 code points in the site index only.

### Why

Search on the site never finds a glossary term: the index holds page and section units only, so a reader who types "wrap rate" finds no page that says it, and the term `wrap_rate`, whose alias is "fully burdened rate", is not a result. `fieldbook search` finds it, because the command line indexes every unit. The new Corpus pages (`kb-rates-practitioner`, `kb-hub-pages`, `kb-fpa-core`) also grow the index. This amendment says what the index holds and how it stays inside its cap, with figures.

### Decision

The edition's site search index holds page leads and page sections of kind `page` or `clause` (a lead is `page`, or `clause` for a clause card; a section takes its page's kind), and glossary term units of kind `term`. A term unit has the fields the [document units](../reference/search.md#document-units) table gives it: the `term_id` in the id field, the `label` as the title, the aliases in the headings field and the definition and boundary rule in the body. Synonym expansion from the aliases is unchanged. Claims, source versions, questions and conflicts are still not in the site index. As before, N, document frequencies, average field lengths and impacts are those of the kept unit set, and `fieldbook search` still builds its index over every unit.

This changes no Corpus file and no format: FB0605 keeps its meaning, and `window.FB_INDEX` keeps `"v":1`, because `kind` is a string and a new kind needs no new `v`. The builder change and the reader-facing rows belong to `search-site-terms`, after a design revision. The search reference (`search.md`) and the FB0605 row of `corpus-format.md` change in that card, not here.

### Measured sizes

Measured on `main` at the time of writing, in a scratch build that was not committed, with only the list of kinds changed (`EDITION_KINDS` with `term` added) and the rest of the build the same. The cap is the FB0605 constant, 15,000,000 bytes.

| Index | Units | Bytes | Gzipped | Headroom to the cap |
|---|---|---|---|---|
| Pages and sections (today) | 15,369 | 14,679,806 | 4,653,341 | 320,194 (2.1%) |
| Plus glossary terms | 15,519 (150 term units) | 14,768,781 | 4,695,653 | 231,219 (1.5%) |

The 150 term units add 88,975 bytes (0.6%), about 593 bytes each, and 42,312 gzipped bytes. Terms alone fit under the cap. The pages the Corpus cards plan add more than the headroom holds, once they land:

| Planned pages | Pages | Estimated bytes |
|---|---|---|
| `kb-rates-practitioner` | 4 | 34,360 |
| `kb-hub-pages` (1 overview, 8 contract types, 19 Cost Accounting Standards) | 28 | 240,520 |
| `kb-fpa-core` | 3 | 25,770 |
| Total | 35 | 300,650 |

The estimate uses the measured mean of 8,590 bytes per page (14,679,806 bytes over 1,709 pages, each lead with its sections). A hub or practitioner page with more sections than a clause card costs more, so the estimate is a floor. With terms and these pages the index is about 15,069,431 bytes, 69,431 over the cap. Without terms it is 14,980,456 bytes, 19,544 under it. So the planned content crosses the 14,700,000-byte trigger line below either way, and passes the cap with terms; the plan below has to cover both.

The parts of the file, measured on the pages-and-sections index: the `docs` array is 4,361,093 bytes (of which snippets 1,643,123, the `parent` page title repeated on every section 893,314, ids 839,401, titles 293,931), the `terms` array 604,456 and the `post` postings 9,714,206.

### Size plan

Options measured or costed for the cap:

| Option | Effect | Cost |
|---|---|---|
| (a) Keep 15 MB; shorter snippets on section units | The cut applies to section units, ids `<page-id>#<anchor>`, of either kind. Section snippets cut to 120 code points save 289,771 bytes; to 100 save 453,508; to 80 save 622,598. Page-lead and term snippets stay at 160 | No format change; section rows in a result list show less text. The command line keeps 160 |
| (b) Raise the cap | A stated figure, for example 20 MB | The load time of a file this size is not measured; `edge-harness` and `search-js` measure it. Every page of the edition waits on a larger script on first search, and the zip grows by the gzipped delta only |
| (c) Split into two files loaded on first focus | Each file under its own cap | A format change (`v` 2), a loader in `search.js` and new rules for N and document frequencies across files; a card of its own |

**Decision: (a), with section snippets of at most 100 code points in the site index only.** With terms, the 35 planned pages and 100-character section snippets the index is about 14,615,923 bytes (15,069,431 less 453,508), 384,077 bytes under the cap, about 44 more pages at the mean. It needs no format change and no new measurement of load time, and it leaves (b) and (c) available when the next trigger fires. A 120-character cut is not enough: it leaves about 14,779,660 bytes, over the trigger line below. The cut is made after the snippet is built, like the removal of URL schemes, and the snippets of page leads and term units and the command line's are unchanged. A cut keyed on the kind `clause` would be wrong: it would save 539,967 bytes and shorten the leads of 1,692 clause cards, not the measured 453,508 bytes on section units. The cut is a card of its own, which lands before or with the term units on the site; it is not part of `search-site-terms`. Cutting is the cheapest saving because it touches no posting, so no rank changes except through terms.

The larger saving, the `parent` page title that every section unit repeats (893,314 bytes), would need a format change to refer to the page by position, so it is held for option (c) or a `v` 2.

### Trigger

The decision is reopened, with the same review as reaching the cap, when `assets/search-index.js` is within 300,000 bytes of the cap: at 14,700,000 bytes or more. The index is 14,679,806 bytes today, so it is 20,194 bytes under that line, and with terms (14,768,781 bytes) it is over it. This amendment is that review for that figure. It also covers the planned pages of `kb-rates-practitioner`, `kb-hub-pages` and `kb-fpa-core` while the cut lands. Under the decision the projected index is 14,615,923 bytes, below the line, and the next review is due when the index, built with the cut, reaches 14,700,000 bytes. A card that adds pages states the index size it measures in its pull request.

### Effect

- A site result list can include a glossary term, reached by the term's own `kind`. The search page and the palette may offer a term row only after the design revision and `search-site-terms`.
- Term units change N, document frequencies and average field lengths, so impacts move slightly. Impacts and the order of pages move slightly. Regression cases are only appended (`search-glossary-aliases` appends six), and no existing case changes.
- The edition still holds public units only, and no label reaches it.
- Rollback: revert the merge commit. The index holds page and section units only, as before; `search-site-terms` cannot start.

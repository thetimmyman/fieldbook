# Search

This is the specification of Fieldbook search: the tokenizer, the document units, the ranking, the site index, the command-line search, the golden vectors and the budgets. It extends AD-11 in [ADR 0001](../decisions/0001-foundation.md). Rust is the only specification of the tokenizer and the ranking; the JavaScript on the site only normalises the query, looks terms up and adds integers.

Cards that build it: `search-core-and-cli` (the tokenizer and the seed tokenize goldens), `search-ranking-and-index` (BM25F, the in-memory index, query function and ranked goldens), `search-site-index` (site serialization and `search-index.js`), `search-cli-and-regression` (`fieldbook search` and the regression runner), `search-page-view` (the page view and `--units`), `search-js` (`search.js`, `search.html`), `search-palette` (the palette) and `edge-harness` (the Edge harness that holds `search.js` to the Rust results). The seed goldens are written before the card that passes them starts. Notes on later releases are marked.

## What AD-11 fixes and what this document decides

AD-11 fixes the outline. This document fixes the details that a golden vector has to pin. Those details are decided here, and ADR 0001 does not state them.

| AD-11 fixes | Decided in this document |
|---|---|
| NFKC, then lowercase; U+2010 to U+2015 and U+2212 fold to `-` | Soft hyphen is dropped; no other folding; the word-character definition |
| An identifier lexer keeps clause numbers, claim ids and source versions whole | The exact shape of each identifier; boundaries; near-miss behaviour; every other hyphen, dot and underscore separates words, except in decimal numbers |
| The index side also emits ancestors down to part.section | Which identifiers have ancestors, the emission order, and that source versions have the source id as their ancestor |
| A clause alias such as `7012` is its own token and ranks below an exact id | Aliases are emitted only on the index side, only for a four-digit hyphen suffix, into the field that holds the clause number. A bare `7012` matches because the query token equals the alias. "Exact id" is a sort class |
| No stemming; synonyms come only from glossary aliases | How synonyms are applied: on the index side, into the body field |
| Documents are page sections and register rows | The page lead unit, which fields each unit fills, the snippet and the link of each kind. Which units the edition's site index holds: page leads and page sections only (the owner's amendment of AD-11; see [The site index](#the-site-index)) |
| BM25F with k1 = 1.2, b = 0.75, weights 6, 4, 2, 2, 1; impacts `round(score * 1000)` | The formula, the IDF, a deterministic logarithm, the minimum impact of 1, field lengths |
| Sort: exact id, matched tokens, summed impact, id | What exact id means, natural id order, prefix scoring, limits |
| `window.FB_INDEX`; 160-character snippets (100 for a section unit); a size cap on the edition index, now 15 MB with no target (the owner's amendment of AD-11); prefix match of the last token; `search.html#q=`; palette; `ids.html` | The array layout, term order, prefix length and cap, the palette rules |
| `fieldbook search` prints the top 10; an exact id prints the full record | Output format, `--json`, exit codes |
| Seed goldens: 40 tokenize, 10 ranked, 30 citation, 20 cite-check | File formats, coverage plan, how the Edge harness uses them |
| Regression set: 20 to 30 questions, Rust only | The file format and what counts as a failure |

## Overview

| Piece | Where | When |
|---|---|---|
| Normaliser, lexer, synonym expansion, units | `fieldbook-core`, module `search` | Card `search-core-and-cli` |
| BM25F, index builder, query function | `fieldbook-core`, module `search` | Card `search-ranking-and-index` |
| Serialisation of the edition's page and section units to `assets/search-index.js`, under the 15 MB cap | `fieldbook-render` | Card `search-site-index`; the cap by card `search-index-budget` |
| `fieldbook search` | `fieldbook` | Card `search-cli-and-regression` |
| The page view above `query` | `fieldbook-core`, module `search::pages`, and `fieldbook` | Card `search-page-view` |
| Lexer shapes 6 to 8 and the test `lexer_covers_every_family` | `fieldbook-core`, module `search` | Card `search-core-and-cli`, which needs the `FAMILIES` table of `core-ids-and-dates` |
| The company index: the stores, labels in results, text windows, the overlay site index | `fieldbook-core`, module `search`, and `fieldbook` | Card `r2-company-ids`; record versions by card `r2-records` |
| `assets/search.js` (the engine), `search.html` | `fieldbook-render` assets and templates | Card `search-js` |
| The palette | `fieldbook-render` assets and templates | Card `search-palette` |
| The Edge harness: loads the golden wrappers and runs `search.js` over them | `xtask` | Card `edge-harness` |
| Goldens | `tests/golden/search/` | Tokenize seeds before `search-core-and-cli`, ranked seeds before `search-ranking-and-index`; extensions by Codex |
| Regression set | `corpus/eval/regression.toml`, run by `fieldbook check` | Written by the lead with the Day-1 content; the runner arrives with `search-cli-and-regression` |
| Context packs select their top 8 hits with the same query function | `fieldbook-core::assist` | Day 30 |

Nothing about search is stored between runs. The CLI and `build` construct the index in memory: `build` from the Corpus, the CLI from the Corpus and, on a machine that has a company store, from that store as well ([The company index](#the-company-index)). The edition index, `assets/search-index.js`, contains public units only, and of those only page and section units (the kinds `page` and `clause`); no label ever reaches it. The command line builds its index over every unit, so the same query can give a different list on the site and on the command line, and different impacts, because the unit count N and the average field lengths differ between the two indexes ([The site index](#the-site-index)). The company index adds units, three lexer rows, labels in results, an `Authz` filter and its own size budget, and changes no rule of the tokenizer, the ranking or the query procedure. The overlay site index is not changed by the edition's page-and-section restriction.

## Tokenizer

The tokenizer has two sides that share one lexer. The index side turns text into the tokens stored in the index. The query side turns what the user typed into the tokens looked up.

### Normalisation

Applied in this order to every string, on both sides:

1. Unicode NFKC.
2. Lowercase (full Unicode case mapping; `str::to_lowercase` in Rust, `toLowerCase()` in JavaScript). NFKC is not applied again.
3. Fold U+2010 to U+2015 and U+2212 to `-` (U+002D). NFKC has already turned U+2011 into U+2010 and the fullwidth and small hyphen-minus forms into `-`.
4. Delete U+00AD (soft hyphen).

Nothing else is folded. Accents are kept (`é` stays `é`). There are no stop words, no stemming and no spelling correction.

### Word characters

A **word character** is an ASCII letter or digit, or a character above U+007F that has the Unicode property Alphabetic or is a number (category N). In Rust that is `char::is_alphanumeric`; in JavaScript it is `/[\p{Alphabetic}\p{N}]/u`. Every other character separates tokens: spaces, punctuation, `_`, `'`, `’`, `@`, `$`, `%`, `&`, `§`, `/`, brackets and `-` outside the shapes below.

### The lexer

The lexer scans the normalised text from left to right. At each word character it tries the shapes below in order, takes the first that matches, emits one token and continues after it. A shape matches only if the character after it is not a word character (the **boundary**); if the longest match fails the boundary, the shape does not apply and the next one is tried. There is no backtracking to a shorter match. A token always starts where the previous character is not a word character.

| # | Shape | Pattern (ASCII, after normalisation) | Example |
|---|---|---|---|
| 1 | Source version | `s-[0-9]{4}-v[0-9]+` | `s-0042-v2` |
| 2 | Source | `s-[0-9]{4}` | `s-0042` |
| 3 | Claim | `cl-[a-z]{3}-[0-9]{4}` | `cl-cmp-0011` |
| 4 | Question | `q-[0-9]{3}` | `q-027` |
| 5 | Conflict | `xc-[0-9]+` | `xc-14` |
| 6 | Company claim | `cl-x[a-z]{2}-[0-9]{6,10}` | `cl-xes-000012` |
| 7 | Company versioned id | `[a-z]{2}-[0-9]{6,10}-v[0-9]+` | `so-000042-v2`, `cm-000012-v2` |
| 8 | Company id | `[a-z]{2}-[0-9]{6,10}` | `so-000042`, `qo-000003`, `xo-000002` |
| 9 | Clause number | `[0-9]{1,4}\.[0-9]{3}(-[0-9]+)?(\([a-z0-9]{1,4}\))*` | `15.404-1(a)(4)`, `252.204-7012`, `15.404(a)` |
| 10 | Number | `[0-9]+` then any number of `.[0-9]+` or `,[0-9]{3}` not followed by a digit; the commas are removed from the token | `1,000,000.50` becomes `1000000.50` |
| 11 | Word | A maximal run of word characters | `contractor`, `15th`, `v2` |

- Shapes 1 to 5 are the Corpus ids. The conflict shape accepts one or more digits; `check` is stricter and reports a malformed id separately. Shapes 6 to 8 are the company ids of [ADR 0004](../decisions/0004-company-boundary.md): a writer-digit prefix and six sequence digits make six to ten digits. They are three generic rows, not one row per family, so neither this table nor `search.js` carries the family list. The test `lexer_covers_every_family` reads the `FAMILIES` table in `fieldbook-core` and fails if the example id of any family is not lexed whole by the shape this table names for it, or if a family prefix would be taken by a shape in rows 1 to 5 (the prefix `xc` is refused for that reason).
- A clause number is the part and section (`252.204`, with a three-digit section), an optional hyphen suffix, and any number of paragraph groups. It covers FAR, DFARS and CAS numbering.
- A decimal such as `7.5` is a number. A decimal with a three-digit fraction, such as `1.250`, has the clause shape and is lexed as a clause number; it has no ancestors and no alias, so the only effect is that it stays whole.
- `-`, `.`, `_`, `'` and `,` inside anything else separate words. `cost-type` gives `cost` and `type`; `contractor's` gives `contractor` and `s`; `material_weakness` gives `material` and `weakness`; `pp. 14–15` gives `pp`, `14` and `15`. The same rule on both sides means a hyphenated query still matches.
- Digits from other scripts are word characters and form words.
- The shapes are a const table, `LEXER_SHAPES`, in `fieldbook-core::search`: one row per shape, in the order above, with its name, its pattern text and its example. The lexer reads the table, and the test `lexer_shapes_match_the_reference` fails if a row differs from the table above. A new shape changes the tokens of every indexed document and every golden, so it is a format change: a row in the const and in this table, golden vectors in Rust and JavaScript, and a new `v` of the index. A part, contract or quote number needs none of that: the `Natural keys` rule of [the company index](#the-company-index) ranks an exact key first.

### The index side adds tokens

After a clause number or a source version, the index side emits extra tokens into the same field. The query side emits none of them.

| Token | Extras, in this order |
|---|---|
| Clause number | Its ancestors from the longest to the shortest, stopping at the part and section; then, if the hyphen suffix has exactly four digits, the suffix as the **alias** |
| Source version | The source id (`s-0042-v2` gives `s-0042`) |
| Company versioned id | The unversioned id (`so-000042-v2` gives `so-000042`) |
| Everything else | None |

Ancestors of a clause number are made by dropping paragraph groups from the right one at a time, then the hyphen suffix. The part and section (`15.404`) is the last ancestor; the part alone (`15`) is never emitted.

| Index-side input | Tokens |
|---|---|
| `15.404-1(a)(4)` | `15.404-1(a)(4)`, `15.404-1(a)`, `15.404-1`, `15.404` |
| `252.204-7012` | `252.204-7012`, `252.204`, `7012` |
| `252.204-7012(b)(2)` | `252.204-7012(b)(2)`, `252.204-7012(b)`, `252.204-7012`, `252.204`, `7012` |
| `15.404(a)` | `15.404(a)`, `15.404` |
| `52.215-2` | `52.215-2`, `52.215` (the suffix has one digit, so no alias) |
| `15.404` | `15.404` |
| `s-0042-v2` | `s-0042-v2`, `s-0042` |
| `so-000042-v2` | `so-000042-v2`, `so-000042` |
| `cl-xes-000012` | `cl-xes-000012` |
| `qo-000003` | `qo-000003` |
| `cl-cmp-0011` | `cl-cmp-0011` |

Why this shape:
- **Ancestors** let a query for `15.404` or `252.204` find every clause below it, while a query for the leaf finds only the leaf.
- **The alias** lets a bare `7012` find `252.204-7012`. People say "DFARS 7012". The alias is a plain token, so it scores like any token in its field and is found only by someone who types it. Four digits is the DFARS numbering of the 7000 series; a shorter suffix such as `-2` would collide with every number in the Corpus. CAS standards are reached through a glossary alias (below).
- **The source id** lets `search S-0042` list every version of the source. A company versioned id does the same: `search SO-000042` lists every version of the source or record.

### The query side

`query_tokens` runs the lexer on the trimmed, normalised query and then:
1. removes duplicate tokens, keeping the first occurrence;
2. keeps at most the first 12 tokens.

It does not emit ancestors or aliases, and it does not expand synonyms. A query with no tokens returns no results.

There is no phrase search, no operators (`AND`, `OR`, `-term`), no wildcards and no field search. Quotation marks separate tokens like any other punctuation.

### Worked examples

Synthetic inputs. "Index" is the index-side token list; "Query" is the query-side list.

| Input | Index | Query |
|---|---|---|
| `DFARS 252.204–7012 (Safeguarding)` (en dash) | `dfars`, `252.204-7012`, `252.204`, `7012`, `safeguarding` | `dfars`, `252.204-7012`, `safeguarding` |
| `FAR 15.404-1(a)(4)` | `far`, `15.404-1(a)(4)`, `15.404-1(a)`, `15.404-1`, `15.404` | `far`, `15.404-1(a)(4)` |
| `[@S-0042-v2, p. 14]` | `s-0042-v2`, `s-0042`, `p`, `14` | `s-0042-v2`, `p`, `14` |
| `Q-027; XC-14` | `q-027`, `xc-14` | `q-027`, `xc-14` |
| `see 252.204−7012.` (minus sign, full stop) | `see`, `252.204-7012`, `252.204`, `7012` | `see`, `252.204-7012` |
| `7012` | `7012` | `7012` |
| `pp. 14–15` | `pp`, `14`, `15` | `pp`, `14`, `15` |
| `$1,000,000.50 and 7.5%` | `1000000.50`, `and`, `7.5` | `1000000.50`, `and`, `7.5` |
| `contractor’s O&M cost-type` | `contractor`, `s`, `o`, `m`, `cost`, `type` | the same |
| `ﬁle １２３` (ligature, fullwidth digits) | `file`, `123` | `file`, `123` |
| `con­tractor` (soft hyphen inside) | `contractor` | `contractor` |
| `material_weakness` | `material`, `weakness` | `material`, `weakness` |
| `3.14159 15th 1.5x` | `3.14159`, `15th`, `1`, `5x` | the same |
| `XC-5 Q-1234` | `xc-5`, `q`, `1234` | the same |
| `SO-000042-v2 and CM-000012` | `so-000042-v2`, `so-000042`, `and`, `cm-000012` | `so-000042-v2`, `and`, `cm-000012` |
| `SO-00042` | `so`, `00042` | the same |
| `ref ab-123456` | `ref`, `ab-123456` | the same |
| `cost cost Cost` | `cost`, `cost`, `cost` | `cost` |

`XC-5` is a conflict id by shape. `Q-1234` is not a question id (three digits are required), so it is a word and a number. `1.5x` fails the boundary for shapes 9 and 10, so the word shape takes `1`, the dot separates, and `5x` is a word. `SO-00042` has five digits, so shapes 6 to 8 do not apply: it is the word `so` and the number `00042`. `ab-123456` has the company id shape whatever it means, because the lexer is one set of shapes and carries no family list; a public page that needs the two parts separate writes a space. A natural key from a source system (a quote, part, project or bid number) has no shape of its own: it is found through the exact class below.

### Synonyms

Synonyms come only from the glossary. For each `glossary.csv` row, the `label` and every value in `aliases` (separated by `|`) form one synonym set. Each member is lexed to tokens (query-side lexing without removing duplicates).

On the index side, for each field of each unit except `id`: if the field's token sequence contains a member as a contiguous run, the tokens of every other member of the set are appended to the unit's **body** field, once per set per unit. Sets are applied in glossary order (sorted by `term_id`), and members in the order `label`, then `aliases`.

Appending to the body field, with weight 1, means a synonym match never outranks a literal match in a heavier field. The query side does nothing: searching `ice` finds units that say "independent cost estimate" because the index side added `ice` to them, and the reverse works too.

Acronyms go in `aliases` (AD-09). A CAS number is reached the same way: a glossary row labelled `9904.401` with the alias `cas 401` makes `cas 401` find units that contain `9904.401`.

### Where Rust and JavaScript can drift

| Trap | Rule |
|---|---|
| Unicode versions differ between Rust and the browser | The goldens use only characters whose properties have not changed for years; a failure on a newer character is a bug report against this table |
| Sorting strings | JavaScript compares UTF-16 code units. The index is sorted in UTF-16 order (`encode_utf16()` comparison in Rust), never byte order |
| Floating point | JavaScript never computes a float. It adds integers |
| Word characters | `char::is_alphanumeric` in Rust and `[\p{Alphabetic}\p{N}]` in JavaScript are the same set; do not use `\w` or `\p{L}` |
| Logarithms | See [Ranking](#ranking) |

## Page heading anchors

`check::page_parts::heading_anchors` assigns anchors in document order to level-2 and level-3 headings. Use visible Markdown text (Text and Code events), Unicode lowercase; retain alphanumeric characters, collapse whitespace and hyphen runs to `-`, remove other characters and trim edge hyphens. Use `section` if empty. Allocate the first unused base, then base-2, base-3 and so on using one set, including collisions with literal suffixed headings. Parts retains each anchor with its heading event position; rendering and search use that same value, never a second slugger. Source-extraction anchors are unchanged.

## Document units

An index **unit** is one thing a result can point at. Each unit has five **fields**, which are token lists, and a few **display** values that are stored as text and are not searched.

| Unit | Built from | `id` field | `title` field | `headings` field | `claim` field | `body` field |
|---|---|---|---|---|---|---|
| Page lead | A page's front matter and the text before its first `##` heading | The page id | The page `title` | | | Text before the first `##` |
| Page section | Each `##` section of a page | | | The `##` text and any deeper headings in the section | | The section text |
| Source version | A `sources.csv` row | `S-0042-v2` | `title` | | | `publisher`, `tag`, `note` |
| Claim | A `claims.csv` row | `claim_id` | | | `statement` | `quote`, `note` |
| Question | A `questions.csv` row | `question_id` | `question` | | | `why_it_matters`, `default_assumption`, `answer` |
| Conflict | A `conflicts.csv` row | `conflict_id` | `topic` | | | `positions`, `resolution`, `rationale` |
| Term | A `glossary.csv` row | `term_id` | `label` | `aliases` | | `definition`, `boundary_rule` |
| Record version | A row of a company record file (`records\<type>.csv`), one unit per version | The version id and the lexed `origin_key` | The `title` column named by the type in `types.toml` | | | The type's `search` columns, joined with a space (empty if it names none) |
| Text window | One text file (`p0001.txt` and on) of a company source version that is not `quarantined` | | The source version's `title` | | | The text of the file |

- The `id` field holds the id lexed by the index-side lexer: `cmp-withholding` gives `cmp` and `withholding`; `material_weakness` gives `material` and `weakness`; `S-0042-v2` gives `s-0042-v2` and `s-0042`. A section unit has an empty `id` field, so searching a page id does not match every section of the page. A company id is lexed whole (`SO-000042-v2` gives `so-000042-v2` and `so-000042`). In the company store the `id` field of a source version or a record version also holds the lexed `origin_key`, so a part or quote number typed on its own ranks that record first through the weight of the `id` field. A text-window unit has an empty `id` field, as a section unit does.
- A section unit's id is `<page-id>#<anchor>`, with the anchor from the shared core heading producer below, also used by the renderer.
- Page text means the Text and Code events of the parsed Markdown, separated by one space at every block boundary and line break. Link and image destinations, raw HTML and front matter are left out. Citations such as `[@S-0042-v2, p. 14]` stay as written, so a search for `S-0042` finds the pages that cite it.
- Multi-value register cells (separated by `|`) are joined with a space.
- Page types: every type is indexed. The `note` type arrives at Day 30.
- Which units the edition's site index keeps is decided in [The site index](#the-site-index): page leads and page sections only. The table above is the unit set of the in-memory index that the command line builds.
- Record versions have no fixed fields of their own: the type in `types.toml` names one `title` column and a list of `search` columns, so a new record type needs no change to this document or to the code that builds units. The units of the company store are built by the cards `r2-company-ids` (text windows, labels) and `r2-records` (record versions).
- The `kind` values are a const table, `UNIT_KINDS`, with one row per kind (`page`, `clause`, `source`, `claim`, `question`, `conflict`, `term`, `record` and `text`) and its optional link form from the table below. A missing link is None/null, never an empty href or a guessed page. `kind` is a string in the index, so a new kind needs no new `v`: it needs a row in the const, a row in the tables of this section and the test `unit_kinds_match_the_reference` passing. The kind `record` carries every company record type, so a new record type adds no kind, and the overlay site index uses `record` and `text` in the same format as the edition.
- Labels. Every unit carries, in memory, the labels of what it was built from. The `docs` arrays of the edition index are unchanged: the edition holds `public` units only, so a label there would say nothing. See [The company index](#the-company-index).
- After the fields are built, synonym expansion runs (above), and then the token lists are final. A **field length** is the number of tokens in the final list, extras and synonyms included.

### Display values and the result line

| Unit | Display `title` | `parent` | `kind` | `status` | `as_of` | `effective_from` | `verified_on` | Snippet comes from |
|---|---|---|---|---|---|---|---|---|
| Page lead | `title` | | `page`, or `clause` for the type `clause-card` | `status` | `as_of` | | | The first paragraph of the first section |
| Page section | The section heading | The page `title` | The page's kind | The page's `status` | The page's `as_of` | | | The first paragraph of the section |
| Source version | `title` | | `source` | `lifecycle` | `as_of` | `effective_from` | | `note`, else `publisher` |
| Claim | The claim id | | `claim` | `verification` | `as_of` | `effective_from` | `verified_on` | `statement` |
| Question | `question` | | `question` | `status` | | | | `why_it_matters` |
| Conflict | `topic` | | `conflict` | `status` | | | | `resolution`, else `positions` |
| Term | `label` | | `term` | `definition_status` | | | | `definition` |
| Record version | The `title` column | The type name | `record` | `lifecycle` | `as_of` | `effective_from` | | The first of the type's `search` columns |
| Text window | The source version's `title` | The version id | `text` | The version's `lifecycle` | The version's `as_of` | The version's `effective_from` | | The first paragraph of the file |

Partial dates (`2025`, `2025-11`) are stored as written. An empty value is the empty string.

**Snippet.** Collapse runs of whitespace to one space and trim. If the text has at most 160 code points, use it. Otherwise take the first 159 code points, cut back to the last space if there is one, trim, and append `…` (U+2026). A snippet is at most 160 code points. The edition's site index cuts the snippet of a section unit (an id with `#`) further, to at most 100 code points, by the same rule with 100 in place of 160: if the built snippet has more than 100 code points, take the first 99, cut back to the last space if there is one, trim, and append `…`; the cut comes before the removal of URL schemes. Page leads and terms keep their 160, and the command line's snippets are not cut.

**Links.** The path is relative to the edition root. Pages in subfolders add `../`; `search.js` works that out from its own location.

| Kind | Id form | Link |
|---|---|---|
| `page`, `clause` | `<page-id>` | `p/<page-id>.html` |
| `page`, `clause` | `<page-id>#<anchor>` | `p/<page-id>.html#<anchor>` |
| `source` | `S-0042-v2` | `s/S-0042-v2.html` |
| `claim` | `CL-CMP-0011` | `claims/CMP-1.html#CL-CMP-0011` |
| `question` | `Q-027` | `questions.html#Q-027` |
| `conflict` | `XC-14` | `conflicts.html#XC-14` |
| `term` | `material_weakness` | `glossary.html#material_weakness` |
| `record` | `CM-000012-v2` | No link until `later-record-views` ships; then `r/<type>.html#CM-000012-v2` |
| `text` | `SO-000042-v2#p0001` | `s/SO-000042-v2.html` |

The register anchors are the ids as written. The part of a claim is (number - 1) / 500 + 1, derived from the id alone, so `CL-CMP-0501` links to `claims/CMP-2.html#CL-CMP-0501`. Any result filter by kind is the UI lane's and filters on `kind` after ranking; it does not change ranking. The command line has no such filter: its `--json` output only carries `kind` on each hit. The edition site's index holds the kinds `page` and `clause` only, so the edition offers no filter for a claim, source, question, conflict or term, and no result list there promises a hit of those kinds; whether it filters pages from sections is left to the design pull request. The filters for the other kinds can apply only where the index holds them, which is a company site.

## Ranking

BM25F with k1 = 1.2 and b = 0.75 for every field. Field weights:

| Field | `id` | `title` | `headings` | `claim` | `body` |
|---|---|---|---|---|---|
| Weight | 6 | 4 | 2 | 2 | 1 |

For N units, a token t, a unit d and a field f:

```text
n_t          = number of units whose final token lists contain t in any field
idf(t)       = ln(1 + (N - n_t + 0.5) / (n_t + 0.5))
avgl_f       = total tokens in field f over all units / N        (a field with avgl_f = 0 is skipped)
tf'(d,t)     = sum over f in [id, title, headings, claim, body], in that order, of
               weight_f * tf(d,f,t) / (1 - b + b * length(d,f) / avgl_f)
score(d,t)   = idf(t) * tf'(d,t) * (k1 + 1) / (tf'(d,t) + k1)
impact(d,t)  = max(1, round(1000 * score(d,t)))                  (round half away from zero)
```

An **impact** is stored for every pair (t, d) with tf' above zero. The minimum of 1 means a match is never lost to rounding.

**Determinism.** The builder must give identical impacts on every platform and in every build:
- The sum over fields runs in the fixed order above. Units and terms are processed in index order.
- It uses only IEEE-754 addition, subtraction, multiplication and division, which are exact across platforms. It never calls `f64::ln`, `powf`, `powi` or `exp`: libm results can differ in the last bit between Linux and Windows, and a one-bit difference can flip a rounded integer.
- `ln` is written by hand in `fieldbook-core`. For x at least 1: divide x by 2 until it is below 2, counting the divisions as e; let m be the result and z = (m - 1) / (m + 1); then `ln(x) = e * LN_2 + 2 * (z + z^3/3 + z^5/5 + ... )`, summing the terms for exponents 1, 3, 5, … 29 in that order (15 terms) and using the constant `std::f64::consts::LN_2`. Reference values: `ln(2) = 0.6931471805599453`, `ln(3) = 1.0986122886681098`, `ln(10) = 2.302585092994046`. A unit test checks the function to within 1e-12 against a table of values.
- `f64::ln`, `powf`, `powi` and `exp` are added to `disallowed-methods` in the root `clippy.toml` by card `search-core-and-cli`, so the rule is enforced by clippy.

### Query procedure

Both Rust (`query`) and JavaScript (`search.js`) follow it.

A `quarantined` source version has no search unit, so its title, `note` and text are not in the index (see the held versions rule in [corpus-format.md](corpus-format.md#sources)). A superseded or `withdrawn` version stays a unit, with its `lifecycle` and `effective_from` shown.

1. `s` is the normalised, trimmed query. The tokens are `query_tokens(s)`.
2. For each token, collect a score per unit:
   - **Exact:** the impact of the term equal to the token, if the term exists.
   - **Prefix, last token only:** if the token has at least 2 UTF-16 code units, take the first 64 terms (in index order) that start with the token and are not equal to it. Each unit's score is the larger of its exact impact and half of its impact for any prefix term (integer division by 2). A unit counts once for the token.
3. For each unit: `matched` is the number of tokens it scored for, and `sum` is the total of its token scores.
4. A unit is in the **exact class** if `s` equals its folded id, or equals its unversioned id when the unit is a source version or a record version, or, in the company store, equals the folded `origin_key` of a source version or a record version. The exact class is included even when it has no scores (a section id has an empty `id` field).
5. Sort exact-class units first, ordered by natural ID alone. Sort other units by `matched`, larger first, then `sum`, larger first, then the unit number, smaller first. Unit numbers are assigned in natural id order, below.
6. Return the first units up to the limit a surface asks for, and the total count: 50 on the site, 10 with `--units` on the CLI, every unit for [the page view](#the-page-view). A surface that cuts the list says so (see the search page and the CLI below).

**Natural id order.** Compare folded ids as sequences of runs, a run being a maximal group of digits or a maximal group of non-digits. A digit run sorts before a non-digit run. Digit runs compare by numeric value, then by length (shorter first); non-digit runs compare in UTF-16 order. A shorter sequence sorts before a longer one that starts the same way. So `s-0042-v2` sorts before `s-0042-v10`.

Because the exact class is the first sort key, an exact id always ranks above an alias hit, a prefix hit and any text hit.

### The page view

Two thirds of the units are claim rows, and a long page has many sections, so the flat list buries the page a reader wants. The page view is a pure step above `query`: it takes the index and the full ranked list (a limit of every unit) and groups it for a reader. It changes no score, no order and no token. Rust (`search::pages`) is the specification for the JavaScript mirror, and [`page-view.json`](../../tests/golden/search/page-view.json) holds both to it.

The view has three lists, in this order:

1. **Exact rows.** Every exact-class hit, in query order, ungrouped and unchanged. A page whose lead or section is exact is marked seen, so it does not repeat: its other matching units are absorbed by the exact row.
2. **Page rows.** Every remaining hit of kind `page` or `clause`, collapsed to its page id (the part of the id before any `#`). A page takes the place of its first hit in rank order. A row carries the page id, the display values of the page's lead, the id and the snippet of its best-ranked unit, and how many of its units matched. If the index holds no lead for the page, the row uses the display values of the best unit.
3. **Other rows.** Every remaining hit of any other kind, a later kind included, flat and in rank order. There is no closed list of kinds, and nothing is dropped: exact rows, the units of the page rows, the absorbed units and the other rows add up to the total.

The command line shows every exact row, at most 10 page rows, then at most 5 other rows; when there is no page row, other rows fill to 10.

The [regression set](#regression-set) judges this view: an expected page holds when its row is within the first `top` page rows.

### Worked example

Three synthetic units, numbered in natural id order, N = 3.

| Unit | Id | `id` field | `title` | `headings` | `claim` | `body` |
|---|---|---|---|---|---|---|
| 0 | `CL-CMP-0011` | `cl-cmp-0011` | | | `the contracting officer may withhold 5 percent of payments` (9 tokens) | |
| 1 | `cmp-withholding` | `cmp`, `withholding` | `payment`, `withholding` | | | `withholding applies to business system deficiencies` (6 tokens) |
| 2 | `cmp-withholding#the-rule` | | | `the`, `rule` | | `withholding is permitted when a significant deficiency exists` (8 tokens) |

Average field lengths: `id` 1, `title` 0.6667, `headings` 0.6667, `claim` 3, `body` 4.6667.

The token `withholding` appears in units 1 and 2, so n = 2 and `idf = ln(1.6) = 0.470004`.

| Unit | Field terms of tf' | tf' | score | impact |
|---|---|---|---|---|
| 1 | `id` 6 / 1.75 = 3.42857; `title` 4 / 2.5 = 1.6; `body` 1 / 1.214286 = 0.82353 | 5.8521 | 0.85806 | 858 |
| 2 | `body` 1 / 1.535714 = 0.65116 | 0.6512 | 0.36372 | 364 |

A field term is `weight * tf / (0.25 + 0.75 * length / avgl)`.

Queries over these units:

| Query | Result (unit: matched, sum) | Why |
|---|---|---|
| `withholding` | `cmp-withholding`: 1, 858; section: 1, 364 | |
| `withhold` | `CL-CMP-0011`: 1, 863; `cmp-withholding`: 1, 429; section: 1, 182 | `withhold` is exact in unit 0. `withholding` matches as a prefix at half impact (858 / 2 and 364 / 2) |
| `payment withholding` | `cmp-withholding`: 2, 2091; section: 1, 364 | Two matched tokens first |
| `cmp-withholding` | `cmp-withholding`: 2, 2456; section: 1, 364 | Exact class first |
| `cmp-withholding#the-rule` | the section first: 3, 1641 | The exact class is first, and it retains the ordinary token scores (`withholding` 364, `the` 414, `rule` 863) |
| `the rule` | section: 2, 1277; `CL-CMP-0011`: 1, 414 | |

The exact class is a sort key, not a score reset. `cmp-withholding#the-rule` matches three query tokens and reports 3, 1641. The page lead matches `cmp` and `withholding` (2, 2456): its sum is higher but it matches fewer tokens. This example does not isolate exact-class precedence. To see a zero-score exact unit, give an exact-ID unit whose searchable fields contain none of the query tokens beside a scored competitor: the exact unit still sorts first with 0, 0.

## The company index

On a machine with a company store ([ADR 0004](../decisions/0004-company-boundary.md)), `fieldbook search` and the tool server's `search` read that store as well. The tokenizer, the ranking formula and the query procedure above do not change. This section states what differs. The site index in the next section is the edition's and stays public: the edition is built from the Corpus alone.

| Rule | Detail |
|---|---|
| One index over the stores | The command builds one in-memory index over every store it reads (`corpus`, then `overlay`), with one N, one average field length per field and one idf. The result is one ranked list in which the exact class is still the first sort key, so an exact company id and an exact clause id both come before any text hit. Nothing is written. A second index merged afterwards is not built: two idf tables would rank the same term differently in each |
| `--store NAME` | Repeatable. Limits the index to the named stores before anything is counted. `--store corpus` gives the list a Corpus-only index gives on the same Corpus (a test) |
| Ids do not collide | A company id has six to ten digits and a company area starts with `x`, so no two stores hold the same id (FB0416 and FB0417 keep the shapes apart). Unit numbers are assigned in natural id order over the union |
| Labels in memory | A unit carries the labels of what it was built from; a unit built from several things carries the union. A result shows its labels. A label is never a ranking input |
| Who may read a store | Before a store is opened, `Authz::allow(Read)` asks whether the logon token may read it. A store whose row names `readers` is opened only for a token in that group, and a store that is not opened adds no unit and no term, so its words are not in the idf. The default adapter `policy-readers@1` allows a store whose row names no `readers`. The same set of stores (`ReadableStores`) feeds search, the context pack, the tool server and the overlay site, so one check cuts all four. A later adapter that reads a source system's own access list waits for `later-source-access` |
| Results that leave | A result printed on the screen is not egress. A result returned to a tool client (public units only in version 1), or put into a context pack, a brief or an export, passes `LabelledView` ([policy.md](policy.md)): a unit whose labels the destination does not allow is not in the list. A pack, an export and a print end with "Left out (policy): N records". A model request, a tool reply and the clipboard end with the fixed line "Some records were omitted." and no number (`shows_left_out_count`), because a count would show how many restricted records matched. A test plants one canary term in a unit of each label and reads every destination kind |
| Natural keys | A quote or part number as the source system writes it is the `origin_key` of a company source version or record. The `id` field holds it (rows above) and a query equal to the folded key is in the exact class, so the versions of one record list in version order. No lexer shape is added for it |
| Text windows | One unit per text file of a company source version, so the text of a company document is searchable. A `csv-rows` window is one such file. The Corpus has no such units and the edition does not gain them. A `quarantined` version has none, as in the Corpus |
| Overlay site index | `fieldbook build --overlay` writes the company site and its `search-index.js`, in the format of the next section, on demand into `state\site\overlay\`, a derived folder inside `state` and not inside the store. It carries the units of the stores the run may read, and the union of their labels is recorded with it. It is refused (FB0412) when the `holds` of `state` do not include every label it carries, it is not built for a store whose row names `readers`, backup does not copy it, `clean --site` removes it, and it is never copied to the edition. At the floor (a company store and no policy) it carries `marked` pages, because `state` holds `marked` there; it is a derived copy in the same per-user tree and is never exported or backed up. Company indexes are partitioned by stable unit-id hash into classic-script shards of at most 8 MB. The 24 MB cap is the browser working set, not total historical storage. The edition keeps its own caps (15 MB for `search-index.js`, 3 MB for any other data file) because it travels as a zip; the overlay site stays on the laptop |
| Goldens | Company cases run over the synthetic store `tests/fixtures/company-mini` with a synthetic policy. The seed files are `seed-tokenize-company.json` and `seed-ranked-company.json` (see [Seed coverage](#seed-coverage)) |

## The site index

`assets/search-index.js` is one classic script that assigns one global:

```text
window.FB_INDEX = {"v":1,"docs":[...],"terms":[...],"post":[...]};
```

| Key | Content |
|---|---|
| `v` | The index format, `1`. `search.js` refuses any other value, disables index use and shows the static ID-list fallback with `Search is unavailable for this index version. Use the ID list.`; scripts remain running |
| `docs` | One array per unit in natural id order. The position is the unit number. Each is `[id, kind, title, parent, snippet, status, as_of, effective_from, verified_on, exact_key]`, all strings. exact_key is the unmodified NFC origin_key for company source/record units and empty otherwise; compare it before tokenization |
| `terms` | Every distinct term, in UTF-16 code-unit order |
| `post` | Parallel to `terms`. Each entry is a flat array `[unit, impact, unit, impact, ...]` of integers, units ascending |

Worked-example fragment (the unit numbers are those of the table above):

```text
"terms":[..., "withhold", "withholding", ...], "post":[..., [0,863], [1,858,2,364], ...]
```

- **Section snippets.** The snippet of a section unit in the site index has at most 100 code points (the cut of [Display values and the result line](#display-values-and-the-result-line)); the snippet of a page lead has at most 160, as on the command line. The index format stays `"v":1`, and the units, tokens, postings and impacts, so the ranking, are those of the index before the cut. The cut saves about 450,000 bytes of the file (card `search-section-snippets`; [ADR 0006](../decisions/0006-edition-search-index.md) size plan (a)).
- **No remote URL.** The edition allows no `http://` or `https://` in an asset (ED-01, FB0601), and a snippet or title can quote one from Corpus text. So the site index removes every `http://` and `https://` (ASCII case-insensitive, repeated until none is left) from each `docs` string after the snippet is cut: `listed at https://example.org/x` is delivered as `listed at example.org/x`. Terms, postings and impacts are not affected. The command line's snippets keep the text as written.
- The serialisation is compact JSON with the keys in the order shown and no spaces. `BTreeMap` and sorted vectors only, so equal inputs give equal bytes.
- **Which units the edition index holds.** Page leads and page sections only: the units of kind `page` and `clause`, whose ids are `<page-id>` and `<page-id>#<anchor>`. It holds no claim, source version, question, conflict or term unit. The reason is size. Measured on the real Corpus, an index of every unit is about 34.7 MB, and one of pages and sections alone is about 14.7 MB, against the former 3 MB cap. The command line is not affected: `fieldbook search` builds its index over every unit.
- **What the site then shows.** Search on the site finds pages and sections. A claim, source version, question, conflict or term is not a search result there. The page view (card `search-page-view`) groups the ranked units into exact rows, page rows and other rows; on the site its other rows, those of every kind but `page` and `clause`, are always empty. Another kind is reached only by an exact id scan, which is done in `ids.html` and the id-list pages under `ids/` that it links to (every id with its link) and the register pages, not through the index. The palette and the search page must not promise a claim hit.
- **Results can differ from the command line.** The site index and the command line's index hold different unit sets, so N, document frequencies and average field lengths differ, and so do the impacts and the order of the pages they rank. Rust remains the one specification of the ranking, and `search.js` agrees with Rust over the index it is given. Compare lists and scores only over the same unit set.
- Size of the edition's index: **cap 15 MB, no target.** `build` fails the edition with FB0605 when `assets/search-index.js` is above 15,000,000 bytes; every other data file keeps its 3 MB cap. The earlier 1 MB target is dropped: the real Corpus's pages and sections do not reach it. Before the section-snippet cut the file held 14,828,125 bytes; after it, with 13,788 section units cut to at most 100 code points, it holds 14,370,347 bytes, 457,778 bytes less and 629,653 bytes under the cap. A growing Corpus still reaches the cap and then the reopening in [Budgets](#budgets). `cargo xtask scale` prints the size. The overlay site's index has its own budget (shards at most 8 MB and browser working set at most 24 MB; see [The company index](#the-company-index)).
- The index is not loaded with a page. `search.js` adds one `<script src>` for it on the first focus of the search input, Ctrl K or `/`, and runs the pending query when it loads. There is no `fetch` and no module, because file:// allows neither.
- `search.js` holds the engine: normalise, the query-side lexer, binary search over `terms`, the integer sums, the sort and the loader. `search.js` also builds the result rows for the search page and the palette, through `window.FB_SEARCH.run`; the palette itself is in `app.js`. `search.js` stays within 10 KB.
- Result views build text with `textContent` only. Snippets and titles come from Corpus text and are never inserted as HTML.

**Search page.** The search input is not inside a `<form>`. Typing updates the results in place. Enter sets the address to `search.html#q=<terms>`, where `<terms>` is `encodeURIComponent` of the trimmed query; `search.html` reads that value on load and on `hashchange`, and a value that does not decode is an empty query. The page shows the first 50 results and the total count, over the page and section units of the edition index. When the total is more than 50, it shows "Showing 50 of N" above the list. A query with no results shows the abstention phrase (see [corpus-format.md](corpus-format.md) for the frozen strings).

**Palette.** Opened by Ctrl K or `/`. Empty input has its own empty state. While the folded, trimmed input is nonempty and has no whitespace, the palette lists folded ID-prefix matches first in unit order, excluding section units, then fills with nonduplicate ranked results up to eight total. The test is on whitespace, not on the token count, so `q-001,q-002` is one ID prefix that matches nothing and its ranked results follow. On the edition the index holds page ids only, so an ID-prefix match is a page id and the palette promises no claim, source, question, conflict or term hit; `ids.html` and the id-list pages under `ids/` that it links to list those ids. Input with whitespace uses ranked results only. Reuse the shared ranking and row renderer. Enter opens a highlighted row only when it has a link; a record without a page remains visible with its text and metadata and does not navigate. The last row is “Search for <text>”, linking to `search.html#q=`. The palette’s looks are the UI lane’s.

**Without scripts.** Every page works without JavaScript. `ids.html` and the id-list pages under `ids/` that it links to list every id with its link. `ids.html` stands in for search, and on the edition it is also the way to an id that the page-and-section index does not hold. `search.html` says that search needs scripts and links to `ids.html`.

## The command line

`fieldbook search <query>` builds the index in memory from the Corpus and, on a machine with a company store, from that store as well ([The company index](#the-company-index)), and runs `query`, then lists the results as [the page view](#the-page-view). Words after `search` are joined with single spaces, so quoting is optional; put `--` before a word that starts with `-` (`fieldbook search -- -5% margin`), because the command line reads it as an option otherwise. It reads the stores' files and writes nothing.

```text
fieldbook search [--json] [--units] [--store NAME]... <query>...
```

Plain output is two lines per row: the exact rows, then the page rows, then the other rows after the one-line heading `Also matched:`, which is left out when no row comes before them. Ranks run on across the lists. A limit never cuts an exact row, so an exact unversioned source ID, record ID or natural key lists every version in natural order. At most 10 page rows and then at most 5 other rows follow; with no page row, other rows fill to 10. Plain and JSON list the same rows. A snapshot test pins the format:

```text
1. cmp-withholding (page) Payment withholding for business-system deficiencies
   draft · as of 2025-11-01 · Withholding applies to business system deficiencies…
```

The first line is rank, id, kind and display title. The second is the non-empty values among `status`, `as_of`, `effective_from`, `verified_on` (the last two prefixed "effective" and "verified"), then the snippet, joined with ` · `. On a page row these are the page lead's values and the best unit's snippet; when the best unit is a section, `section <id> (<heading>)` comes before the snippet:

```text
2. cmp-withholding (page) Payment withholding for business-system deficiencies
   draft · as of 2025-11-01 · section cmp-withholding#purpose (Purpose) · Withholding applies to business system deficiencies…
```

Output before the list is never printed, and the "Showing" line comes after it. When a limit cuts the view, the last line gives the rows shown out of each list that has rows: `Showing 10 of 12 pages and 5 of 7 other results.`, or `Showing 10 of 12 pages.`, or `Showing 10 of 14 results.` when only other rows match. When a source or record version is among the other rows, the line ends with ` fieldbook search <source id> lists every version.`.

- **`--units`.** Prints the flat unit list instead: two lines per unit in rank order, at most 10. An exact unversioned source ID, record ID or natural key uses `max(10, eligible exact-class count)`, so every eligible version appears in natural order. When truncated, the last line is `Showing D of N. fieldbook search <source id> lists every version.`, with the actual displayed count D.

- **Exact id.** When exactly one unit is in the exact class, it is printed in full instead of its two lines: the register row as `column: value` lines in column order, skipping empty values; for a page, `path:` then the front-matter fields in schema order. The other results follow as usual. A source id such as `S-0042`, a company id such as `SO-000042` or `CM-000012`, or a natural key (`origin_key`) puts every version in the exact class, so the versions are listed in version order. A record version prints its envelope and its type's columns the same way, one `column: value` line each.
- **No results.** Prints the abstention phrase and exits 1.
- **`--json`.** One object with `report_version` 1, `query`, `tokens` (the query tokens), `view` (`pages`, or `units` with `--units`), `total` (the matching units before any grouping or cut), and `results`: an array of objects with `rank`, `id`, `kind`, `title`, `parent`, `status`, `as_of`, `effective_from`, `verified_on`, `labels` (an array of label ids; `["public"]` for a Corpus unit), `store` (`corpus` or `overlay`), `matched`, `score` (the summed impact) and `snippet`. A page row's `id` is the page id, its `matched`, `score` and `snippet` are those of its best unit, and it adds `best_unit` (that unit's id) and `matched_units` (how many of the page's units matched). No other row has those two keys.
- **Exit codes.** 0 when there is at least one result, 1 when there is none, 2 for usage errors, 3 for I/O errors, as for the other commands.
- **Labels.** When any result carries a label other than `public`, the second line of every result starts with its labels in square brackets (`[vendor-quote]`), and a result that carries only `public` shows none. A list of public units only prints exactly as before, so the Corpus snapshot test stays valid. What the command prints is not egress.
- The Corpus is found as for the other commands. The CLI never reads or writes the clipboard.

## Golden vectors

Goldens hold Rust and JavaScript to one specification. They live in `tests/golden/search/`. Citation goldens live in `tests/golden/citations/` and Handoff goldens in `tests/golden/handoff/`; they are specified with the citation grammar and are listed here only for their counts.

**The seed rule.** Files named `seed-*.json` hold the reference cases, written before the code they check. Any author, an agent included, may add a new seed file in a pull request that changes nothing else. An existing seed file is never changed by an agent: CI rejects such a commit (`guard-statuses`, GS-02). Agents add cases in other files in the same directory. If a seed case fails, the code or this document is wrong; the agent does not change the seed. It marks the card BLOCKED, names the case id, and asks. A change to a seed is a change to this document, made by the lead or the owner in one pull request with `Review: owner`.

| Kind | Directory | Seed file | Seed cases | Total required | Asserted by |
|---|---|---|---|---|---|
| tokenize | `tests/golden/search/` | `seed-tokenize.json` | 40 | At least 200 | Rust, both sides; the Edge harness, the query side |
| ranked | `tests/golden/search/` | `seed-ranked.json` | 10 | At least 30 | Rust; the Edge harness, over an index built by Rust |
| tokenize and ranked, company | `tests/golden/search/` | `seed-tokenize-company.json`, `seed-ranked-company.json` | 12 and 4 | Counted in the totals above | Rust; the Edge harness, the query side of the tokenize cases |
| page-view | `tests/golden/search/` | none: `page-view.json` is an ordinary file | | At least 8 | Rust; the Edge harness, from `search-js` |
| citation | `tests/golden/citations/` | `seed-*.json` | 30 | | Rust; the Edge harness (Day 30) |
| cite-check | `tests/golden/handoff/` | `seed-*.json` | 20, one of them an answer with a good citation and a broken one, which exits 1 and prints "1 of 2 citations do not resolve" | | Rust; the Edge harness (Day 30) |

Codex's files are named `tokenize-<topic>.json` and `ranked-<topic>.json`. Case ids in seed files are `tok-s-001` and `rk-s-01` onwards; Codex's cases use `tok-001` and `rk-01` onwards. Ids are unique across the directory. The company seed files use `tok-sc-001` and `rk-sc-01` onwards.

### Formats

JSON, UTF-8, one object per file. Tokenize file:

```json
{
  "kind": "tokenize",
  "version": 1,
  "cases": [
    {
      "id": "tok-s-001",
      "side": "index",
      "input": "DFARS 252.204–7012",
      "tokens": ["dfars", "252.204-7012", "252.204", "7012"],
      "note": "en dash folds to a hyphen; ancestor and alias are emitted"
    }
  ]
}
```

`side` is `index` (the token list as stored, duplicates kept) or `query` (the list after removing duplicates and capping at 12). Inputs with non-ASCII characters are written with `\u` escapes so a reader can see them.

Ranked file. The index is built from `units`, so a case does not depend on the Corpus:

```json
{
  "kind": "ranked",
  "version": 1,
  "units": [
    {
      "id": "CL-CMP-0011",
      "kind": "claim",
      "fields": { "id": "CL-CMP-0011", "title": "", "headings": "", "claim": "The contracting officer may withhold 5 percent of payments", "body": "" },
      "display": { "title": "CL-CMP-0011", "parent": "", "snippet": "", "status": "unchecked", "as_of": "2025-11-01", "effective_from": "", "verified_on": "" },
      "exact_key": ""
    }
  ],
  "glossary": [ { "label": "independent cost estimate", "aliases": ["ice"] } ],
  "cases": [
    {
      "id": "rk-s-01",
      "query": "withhold",
      "expect": ["CL-CMP-0011", "cmp-withholding", "cmp-withholding#the-rule"],
      "scores": [863, 429, 182],
      "impacts": [ { "unit": "cmp-withholding", "token": "withholding", "impact": 858 } ]
    }
  ]
}
```

`expect` is the complete result list in order, up to 10. `scores` is optional and gives the summed impacts in the same order. `impacts` is optional and pins individual postings. A unit may carry `exact_key`, the unmodified NFC origin key; it defaults to `""` and is compared with the folded query before tokenization. A case with no results has `"expect": []`.

How each side runs a ranked file:
- **Rust** builds the index from `units` and `glossary`, runs `query` and compares.
- **The harness** (`cargo xtask harness`) builds the same index with the Rust code, writes it and the file as generated `.js` wrappers under `out/` (file:// cannot fetch JSON), loads both in headless Edge, runs `search.js` and compares. Generated wrappers are never committed.

### Seed coverage

The writer of the seeds covers at least the following.

| Seed file | Group | Cases |
|---|---|---|
| `seed-tokenize.json` | Normalisation: NFKC (fullwidth digits, a ligature, a superscript), lowercase of a non-ASCII letter, every folded dash (U+2010, U+2011, U+2013, U+2014, U+2212), soft hyphen, accents kept, empty and whitespace-only input | 8 |
| | Words and numbers: punctuation, apostrophe, underscore, hyphenated compound, decimal, thousands separator, a comma that is not one, digits against letters, `%` and `$` | 8 |
| | Identifier shapes: each of the six Corpus shapes (the five ids and the clause number), paragraph groups, a group of five characters, an unbalanced parenthesis, a shape followed by a letter, and the near misses `xc-5`, `q-1234`, `cl-cmpx-0011`, `s-004` | 12 |
| | Index-side extras: a long ancestor chain; part and section only; a four-digit alias; a two-digit suffix with no alias; no suffix; the source id; a claim id with no extras; a repeated clause number | 8 |
| | Query side: duplicates removed, the cap of 12, no extras, surrounding whitespace | 4 |
| `seed-ranked.json` | An exact claim id first | 1 |
| | `S-0042` lists versions in natural order, `v2` before `v10` | 1 |
| | A bare `7012` finds the clause card through the alias, and a title hit outranks a body hit | 1 |
| | `252.204` finds a descendant through an ancestor | 1 |
| | The last token is a prefix; a prefix-only hit scores half | 1 |
| | Matched tokens outrank summed impact | 1 |
| | A title hit outranks a body hit; a heavier field wins | 1 |
| | A synonym through a glossary alias ranks below a literal hit | 1 |
| | A section id is in the exact class with no scores | 1 |
| | No match, and a query with only punctuation, give an empty list | 1 |
| | `impacts` pinned for at least five postings, including those in the worked example | across the cases |
| `seed-tokenize-company.json` (new seed files, so the seeds above stay unedited; the lead writes the tokenize file before the card `search-core-and-cli` starts and the ranked file before `r2-company-ids`) | Company shapes: each of the three; six digits and ten digits; five and eleven digits as near misses; a versioned id with its extra; `cl-xes-000012` against `cl-cmp-0011`; `xc-000042`, which the conflict shape takes; `ab-123456` as the cost of the generic shape; `SO-00042` | 12 |
| `seed-ranked-company.json` | A company id first; `SO-000042` lists its versions in natural order; a record found by its `origin_key`; a text-window hit ranks below a title hit | 4 |

## Regression set

`corpus/eval/regression.toml` holds the questions that search must keep answering. The lead writes 20 to 30 of them with the Day-1 content, negatives included; the set then grows by promotion from the question log (`ask-promote`). It runs in Rust only, as part of `fieldbook check`, against the index built from the Corpus. The lead writes the file, and the program writes to it when the owner or lead accepts a promoted question at `fieldbook queue review`. The program never sets `critical`; a person does. The guard bars an agent from editing the file, as it bars the seed goldens. A promoted case keeps the query text the person approved, and that text passes the marking and identity scans like any committed file.

```toml
# Synthetic example
[[case]]
id = "rg-001"
query = "7012"
expect = ["cmp-dfars-252-204-7012"]
top = 3
critical = true

[[case]]
id = "rg-002"
query = "quantum accounting"
expect_none = true
```

| Key | Meaning |
|---|---|
| `id` | Unique, `rg-NNN` |
| `query` | The text a person would type |
| `expect` | Unit ids, usually page ids, each judged on [the page view](#the-page-view): an id holds when its page is within the first `top` page rows, or when an exact row stands for it. A section id `<page-id>#<anchor>` counts for its page. Each id must exist in the Corpus |
| `top` | Default 5: how many leading page rows an expected page may appear in |
| `expect_none` | The result list must be empty. Write the query so that none of its tokens appears anywhere in the Corpus |
| `critical` | Default false. A miss on a critical case is an error; a miss on any other case is a warning |

**What a case judges.** A case judges what a reader is shown, the page view of the full ranked list, not the flat unit list: claim rows, which are two thirds of the units, never push a page out of the count, and a page found only by its sections counts by its best section. Exact rows are never cut, so an expected page that an exact row stands for holds. An expected id that is not a page or a clause (a claim or another kind) is judged on the exact rows, then on the other rows of the view. `expect_none` is unchanged: the query must return no unit at all. The flat list stays available as `fieldbook search --units`, and ranking, scores and tokens are untouched.

The gate "zero critical failures" at Day 1 counts the errors from this set. Recording the questions as cases is the lead's or the owner's work, and the question log lists the candidates; nobody tunes the ranking to pass a case.

## Budgets

| Budget | Value | Gate |
|---|---|---|
| Query time, exact identifier or keyword | p95 at most 100 ms (NFR-F-01 in ADR 0001) | The harness times 10 lookups per run, advisory in CI; gated on the owner's laptop at E1 |
| Index size, edition | Cap 15 MB for `search-index.js`, no target; 3 MB for any other data file; page and section units only | FB0605 fails the edition above the cap; `cargo xtask scale` prints the size |
| Index size, overlay site | Shards at most 8 MB; browser working set at most 24 MB | Search every shard; bounded top-k merge |
| Company index build time | At most 5 s for 50,000 units on the laptop | The harness builds a generated synthetic store of that size; advisory in CI, gated on the owner's laptop at E1. Over the limit is a trigger for `later-sqlite-tantivy` |
| `search.js` | 10 KB; all scripts together 64 KB | A size check in CI (AD-15) |
| Index loading | Only on first focus, Ctrl K or `/`; never with a page | The harness |
| Snippet | At most 160 code points; at most 100 for a section unit of the site index | The index builder |
| Query tokens | At most 12 | `query_tokens` |
| Prefix | At least 2 UTF-16 code units; at most 64 terms | `query` |
| Results | 50 on the site; on the command line 10 page rows and 5 other rows, or 10 other rows with no page row, and 10 units with `--units`; a surface says so when it cuts | `query`, and the page view above it |
| Agreement | Rust and JavaScript give identical lists and scores on every ranked case and identical tokens on every query-side tokenize case | The harness (Day 1) |

The index, `check` and `build` budgets that are not about search are in AD-17.

**Related units for a page.** A page's context pack takes its related units from `query` on the page title, at most 8, with the page's own units left out (card `page-context-packs`). The rank order above is the only order it uses.

**Reopening this design.** An effective-as-of filter only if a clause query shows a case where the version in force at a past date is not within the first 10 results; until then a source id lists every version in order. SQLite or tantivy only if search p95 exceeds 100 ms on the laptop, the edition index exceeds 15 MB, the browser working set exceeds 24 MB, building the in-memory company index takes more than 5 seconds, or a full `check` exceeds 60 seconds. The persistent adapter then sits behind the existing `IndexPort` and is built from the company store; its callers do not change. WebAssembly in the site only if the JavaScript query side misses the p95 budget while the goldens pass (AD-24).

## Company browser growth

The company site writes a manifest with shard paths/hashes and global document-frequency/length statistics. Each shard uses the same integer scoring and exact-first ordering as the full index. The classic-script loader searches every shard sequentially with those shared statistics, retains bounded top-k results with deterministic id ties, then releases shard data. It uses local script loading on file: pages, not fetch or modules. It never silently skips historical shards. Interrupted or failed loads show incomplete search and no complete-result claim. Label filtering happens before shard construction and again for live Bridge views; a policy/content generation change rebuilds or invalidates the index.

The R2 workload is five years (60 periods) with 10,000 observation memberships per period and 50,000 extraction windows, including repeated values, late vintages and restrictive source relabelling. The harness compares full-index and all-shard rankings and historic exact-id retrieval, measures total disk bytes, peak browser working set and p95 query time, and tests a missing shard. Search remains available for all history; persistent engine adapters can replace the CLI engine without removing this browser contract. The public edition keeps its own caps: 15 MB for the search index, which holds page and section units only, and 3 MB for any other data file.

Browser search exports load() for the ordinary index, rank(index, text, limit) returning scored hits and total, render(hits, text, list) preserving the supplied order, and run(text, limit, list) as their ordinary-index wrapper. The palette combines ID-prefix and ranked hits before rendering. Assistant ranking passes its own permitted index. Exact-key metadata survives company shard serialization; token-colliding keys such as `quote-a` and `quote a` remain distinct.

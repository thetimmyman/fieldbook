# Golden vectors

Golden vectors are JSON files of inputs and the exact outputs the program must give. They hold two implementations to one specification: the Rust code and the JavaScript on the site. The tokenizer and the citation lexer are written once in Rust and ported to JavaScript (`search.js`, and `assist.js` from Day 30), and the vectors are what keeps the two from drifting (AD-11, AD-10).

The behaviour they pin is specified in [search.md](../../docs/reference/search.md) (tokenizer and ranking) and [corpus-format.md](../../docs/reference/corpus-format.md) (citation grammar, locators, the check catalogue). The vectors never decide behaviour. If a vector and the specification disagree, one of them is wrong, and the lead or the owner fixes it.

## Folders

| Folder | Kind | Seed file | Seed cases | Total required | Wired in by |
|---|---|---|---|---|---|
| `search/` | tokenize | `seed-tokenize.json` | 40 | At least 200 | Card `search-core-and-cli` (Rust); card `search-js` (Edge, query side) |
| `search/` | tokenize | `seed-tokenize-company.json` | 12 (ids `tok-sc-001` onwards) | Counted in the 200 above | Card `search-core-and-cli` (Rust); card `search-js` (Edge, query side) |
| `search/` | ranked | `seed-ranked.json` | 10 | At least 30 | Card `search-ranking-and-index` (Rust); card `search-js` (Edge) |
| `search/` | ranked | `seed-ranked-company.json` | 4 | Counted in the 30 above | Card `r2-company-ids` (Rust) |
| `citations/` | citation | `seed-citations.json` | 30 | No minimum | Card `check-citations` (Rust, the lexer); the Day-30 Handoff cards (Edge); card `r2-company-ids` for `citations-company.json` (Rust) |
| `handoff/` | cite-check | `seed-*.json` | 20 | No minimum | The Day-30 Handoff cards (Rust and Edge) |
| `context/` | context | `cases.json` | 0 (written with `handoff-context`) | At least 6 | Card `handoff-context` (Rust); card `assistant-page` (Edge) |
| `assist/` | marking | `scan.json` | 0 (written with `assistant-page`) | At least 12 | Card `assistant-page` (Rust and Edge) |

- `seed-ranked.json` is written by the lead after card `corpus-mini-fixture` merges and before `search-ranking-and-index`. The `handoff/` seeds are written by the lead before Day 30. `seed-tokenize-company.json` is written by the lead before card `search-core-and-cli`, and `seed-ranked-company.json` before card `r2-company-ids`. They are new seed files, so the seeds above stay unedited.
- Codex adds cases beyond the seeds in other files (see [Adding cases](#adding-cases)).
- `citations/citations-company.json` pins the company key shapes (`key_kind` `company`). It is a new file and the seed is not touched: the public shapes do not change, so every old citation stays valid and the seed cases must still pass. The JavaScript port of the lexer reads it when the interface lane builds the overlay site's assistant page.

## Ownership

| Files | Written by | Codex |
|---|---|---|
| `seed-*.json` in any folder | The lead or the owner | May read them. Never edits, renames or deletes one |
| Every other `*.json` in these folders | Codex, or anyone | Adds and edits |

- Changing or deleting an existing non-seed case follows the recipe "Changing a golden or snapshot" in [docs/cards/README.md](../../docs/cards/README.md#changing-a-golden-or-snapshot).
- CI rejects a commit by the agent author that touches a `seed-*` file. The check is `guard-statuses` in `ci-ok`; its first form, shipped by `workspace-skeleton`, rejects any such commit by path (AD-21).
- If a seed case fails, the code or the specification is wrong. Codex does not change the seed. It sets the card's work item to Blocked with a comment "BLOCKED: <what>" that names the case id, pushes the branch and takes the next card whose paths are disjoint. The owner clears it by answering on the item or in the pull request.
- A change to a seed is a change to the specification it pins. The lead or the owner makes both in one pull request marked `Review: owner`. When the id grammar gains a form, the seed stays so that old citations stay valid, the new form gets seeds in a new file, and the specification change is made in the same reviewed pull request.

## Adding cases

| Kind | File name | Case ids |
|---|---|---|
| tokenize | `tokenize-<topic>.json` | Seed `tok-s-001` onwards; others `tok-001` onwards |
| ranked | `ranked-<topic>.json` | Seed `rk-s-01` onwards; others `rk-01` onwards |
| citation | `citations-<topic>.json` | Seed `cit-s-001` onwards; others `cit-001` onwards |
| cite-check | `cite-check-<topic>.json` | Fixed when its seed is written |

- A case id is unique across its folder.
- Every expected value must follow from the specification, not from what the code prints today. A case whose expected value the specification does not settle is not added; the question goes to the lead.
- Write each non-ASCII character in `input` and in expected strings as a `\u` escape, so a reader can see which character it is. Notes are plain ASCII.
- Files are UTF-8 without a byte-order mark, LF line endings, one final newline.

## How the vectors are loaded

**Rust.** Tests read the JSON with `serde_json`. A loader rejects a file whose `kind` or `version` it does not know and a case with a field it does not know. A test runs every case and reports every failing case id, not just the first.

**The Edge harness.** A page opened with `file://` cannot fetch JSON, so `cargo xtask harness` turns each golden file into a generated classic script under `out/` and loads it with a `<script src>`:

```js
window.FB_GOLDEN = window.FB_GOLDEN || {};
window.FB_GOLDEN["search/seed-tokenize.json"] = { "kind": "tokenize", "version": 1, "cases": [ ... ] };
```

The key is the path of the file below `tests/golden/`, and the value is the file's JSON unchanged. Generated wrappers are never committed. For a ranked file the harness also writes the index that Rust built from the file's `units`, as `search-index.js` would; the JavaScript then answers each query from that index and the harness compares the lists.

| Kind | Rust asserts | The harness asserts |
|---|---|---|
| tokenize | Both sides | The query side only. The JavaScript has no index side |
| ranked | Every case | Every case, over the index built by Rust |
| citation | Every case, in the Rust lexer | Every case, in the JavaScript port (Day 30) |
| context | Every case | Every case, in the JavaScript port, over an `assist-data.js` that Rust writes from `corpus-mini` |
| marking | Every case, in `scan::marking` | Every case, in the JavaScript port |
| cite-check | Every case | Every case, in the JavaScript port, with the case's `context` as a set of ids or none |

## Formats

JSON, UTF-8, one object per file. Every file has `kind` (string) and `version` (integer, `1`) and a `cases` array. A case has an `id`, the fields below, and an optional `note`: one plain sentence saying what the case pins. Loaders ignore the text of a `note`.

### Tokenize

```json
{
  "kind": "tokenize",
  "version": 1,
  "cases": [
    {
      "id": "tok-s-028",
      "side": "index",
      "input": "FAR 15.404-1(a)(4)",
      "tokens": ["far", "15.404-1(a)(4)", "15.404-1(a)", "15.404-1", "15.404"],
      "note": "Ancestors from the longest to the shortest, ending at the part and section"
    }
  ]
}
```

| Field | Type | Meaning |
|---|---|---|
| `id` | string | The case id |
| `side` | `"index"` or `"query"` | Which tokenizer runs. `index` gives the list as stored: duplicates kept, ancestors and aliases added. `query` gives the list looked up: duplicates removed, at most 12 tokens, nothing added |
| `input` | string | The text, before normalisation. May be empty |
| `tokens` | array of strings | The exact token list, in order. May be empty |

The rules are in [Tokenizer](../../docs/reference/search.md#tokenizer).

### Ranked

```json
{
  "kind": "ranked",
  "version": 1,
  "units": [
    {
      "id": "CL-CMP-0011",
      "kind": "claim",
      "fields": {"id": "CL-CMP-0011", "title": "", "headings": "", "claim": "The contracting officer may withhold 5 percent of payments", "body": ""},
      "display": {"title": "CL-CMP-0011", "parent": "", "snippet": "", "status": "unchecked", "as_of": "2025-11-01", "effective_from": "", "verified_on": ""},
      "exact_key": ""
    }
  ],
  "glossary": [{"label": "independent cost estimate", "aliases": ["ice"]}],
  "cases": [
    {
      "id": "rk-01",
      "query": "withhold",
      "expect": ["CL-CMP-0011", "cmp-withholding", "cmp-withholding#the-rule"],
      "scores": [863, 429, 182],
      "impacts": [{"unit": "cmp-withholding", "token": "withholding", "impact": 858}]
    }
  ]
}
```

The index is built from the file's `units` and `glossary`, so a case does not depend on the Corpus.

| Field | Type | Meaning |
|---|---|---|
| `units[].id` | string | The unit id as written (`CL-CMP-0011`, `cmp-withholding#the-rule`) |
| `units[].kind` | string | The unit kind: `page`, `clause`, `source`, `claim`, `question`, `conflict`, `term`, `record` or `text`, matching `UNIT_KINDS` |
| `units[].fields` | object | Five strings, `id`, `title`, `headings`, `claim` and `body`. Each is text; the builder lexes it with the index-side tokenizer. An empty string is an empty field |
| `units[].display` | object | Seven strings that are stored and not searched: `title`, `parent`, `snippet`, `status`, `as_of`, `effective_from`, `verified_on` |
| `units[].exact_key` | string, optional | The unmodified NFC origin key, or `""` when the unit has none (the default). Compared with the folded query before tokenization, so `quote-a` and `quote a` stay distinct |
| `glossary[]` | objects | `label` (string) and `aliases` (array of strings): the synonym sets |
| `cases[].query` | string | The text typed |
| `cases[].expect` | array of strings | The complete result list, as unit ids in rank order, up to 10. `[]` for no results |
| `cases[].scores` | array of integers, optional | The summed impact of each result, in the same order |
| `cases[].impacts` | array of objects, optional | Pins single postings: `unit` (id), `token` and `impact` (integer) |

The example shows one unit; a file lists every unit its cases need. The ranking is in [Ranking](../../docs/reference/search.md#ranking), and its worked example gives numbers that can be checked by hand.

### Citation

```json
{
  "kind": "citation",
  "version": 1,
  "cases": [
    {
      "id": "cit-s-001",
      "mode": "strict",
      "input": "[@S-0042-v2, p. 14]",
      "expect": {
        "ok": true,
        "cites": [
          {"key": "S-0042-v2", "key_kind": "version", "locator": {"kind": "pdf_page_box", "text": "p. 14"}}
        ]
      }
    },
    {"id": "cit-s-020", "mode": "strict", "input": "[@CL-CMP-0011, p. 3]", "expect": {"ok": false, "code": "FB0305"}}
  ]
}
```

| Field | Type | Meaning |
|---|---|---|
| `mode` | `"strict"` or `"tolerant"` | `strict` is the reader that `check` uses on the Corpus. `tolerant` is the reader for pasted answers (Day 30) |
| `input` | string | The text read. It is one bracket pair, or a sentence with bracket pairs in it |
| `expect.ok` | boolean | `true` when every citation in the input is accepted. `false` when the reader reports a finding or rejects the input |
| `expect.cites` | array | The citations accepted, flat and in the order read, for `ok: true`. Always `[]` for a rejection by the tolerant reader. Absent for a strict failure |
| `expect.code` | string | Strict failure only: the `FB03xx` code, from the [citations part of the check catalogue](../../docs/reference/corpus-format.md#03-citations). A strict case has exactly one finding |
| `expect.canonical` | string | Tolerant `ok: true` only: the input rewritten in the canonical form |

Fields of each entry of `cites`:

| Field | Type | Meaning |
|---|---|---|
| `key` | string | The id as written |
| `key_kind` | `"version"`, `"claim"`, `"question"`, `"conflict"` or `"company"` (a key that matches a company shape of the const `FAMILIES` table) | The nonterminal of the grammar the key matched |
| `locator` | object or `null` | `null` when the citation has no locator. Otherwise `kind` and `text` |
| `locator.kind` | string | `pdf_page_box` (`p.`, `pp.`), `text_section` (`sec.`, `§`, `para.`), `sheet_cells` (`sheet`) or `record_key` (`rec`) |
| `locator.text` | string | The locator as written, prefix included (`pp. 14-15`, `§ 15.404-1`). In a tolerant case it is the canonical text |
| `non_canonical` | boolean | Tolerant cases only: `true` when the reader had to repair the citation. It is the "non-canonical" flag in the output of `cite-check` |

The grammar and the locator kinds are in [Syntax at a glance](../../docs/reference/corpus-format.md#syntax-at-a-glance).

What a citation vector pins, and what it leaves to other checks:
- It pins reading: keys, locators, the canonical form, the repairs of the tolerant reader, and the findings `FB0300`, `FB0301`, `FB0302` and `FB0305`.
- It does not pin resolution: whether an id exists (`FB0303`), whether a locator is in the extraction (`FB0304`), or whether a claim may lack a locator (`FB0306`). Those need a Corpus and are tested with the `corpus-mini` fixture.
- A key and a locator are taken as written. The lexer does not check that the area letters are an area of the Corpus.

### Cite-check

The cite-check format is fixed when the lead writes its seed, before Day 30. This section is extended in the same pull request. Whatever the format, a case pins the exit code and the summary line: `cite-check` exits 1 when any citation does not resolve and prints "N of M citations do not resolve". "Resolves" means the applicable reference, locator and direct quotation checks passed, not that the answer is correct. Ordinary fact/evidence vectors are defined in [Citation checks](../../docs/reference/citation-check.md#shared-cases); the seed files are not rewritten to add them. A case may carry `context`, a list of unit ids. The check then runs with that context and may give `outside_context`; a case with no `context` runs with none and never gives it. At least three of the seeds carry a `context`: one reference that exists and is outside it, one that is outside both the Corpus and the list (`broken`, because the first status that applies wins), and one that is inside it. A verdict names its status in one of the six words `broken`, `broken_locator`, `outside_context`, `superseded`, `weak`, `verified`.

### Context

`kind` is `context`. A case has `id`, `question` (string), `scope` (`all`, `area:<code>` or `page:<id>`), `budget` (integer), `include` (an object of three booleans: `tier_d`, `superseded`, `contradicted`), `edits` (an object of four lists of ids: `pinned`, `removed`, `added`, `suggested`), `items` and `text`. Each of `items` is `{ unit, origin, why, via?, included, stale }`, in order, and `text` is the exact text of `ContextPack::to_text()`. The cases run on `corpus-mini`; the harness gets its units from an `assist-data.js` that the same Rust function writes for the build.

### Marking

`kind` is `marking`. A case has `id`, `text` and `expect`, the list of shapes the marking scan finds in `text` (empty for a lookalike that must not match). The strings are synthetic.

# Corpus format v1

This is the specification of the Fieldbook Corpus: the files in `corpus/`, the ids, the citation syntax, the frozen strings and the checks. [ADR 0002](../decisions/0002-corpus-format-v1.md) freezes it at F0 as schema version 1. This file at the accepting commit is the frozen v1 specification. A company store reads the same formats. What differs there (label lists in `handling`, company id shapes, the origin and classification columns, company-only values) is marked in each section. The store's own files are in [company-store.md](company-store.md), and the policy file is in [policy.md](policy.md).

How to read it:
- "Error" and "warning" are `fieldbook check` severities. An error makes `check` exit 1. A warning never changes the exit code, except that `check --strict` turns the rules marked `strict` in the [Check catalogue](#check-catalogue) into errors.
- A rule marked "convention" is one no check can see. A rule marked with a code (`FB0114`) is checked.
- Releases are written as in [ADR 0001](../decisions/0001-foundation.md): R1 (Day-1 Corpus, tool `v0.1.0`), E1 (first transfer), Day 30 (`v0.2.0`: Handoff, `ask`, the review queue and, if its spike passes, the AI build), R2 (`v1.0.0`, signed Toolkit). Where something arrives later than R1, the text says when.
- Frozen means what [Scope and versioning](#scope-and-versioning) lists: changing it needs a decision record. Check codes FB0800 and up and the chips of the [Verb map](#syntax-at-a-glance) are living: cards add them under the recipes in [docs/cards/README.md](../cards/README.md).
- The commands are those of AD-17 in ADR 0001. Their flags are in the generated `cli.md`.
- Every example value is synthetic.

## Scope and versioning

**Schema version.** `schema_version` is an integer. It is set in `corpus/corpus.toml` and copied into `edition.toml`. It is `1` in this specification. Both `check` and `build` refuse a Corpus whose `schema_version` is newer than the tool supports (FB0100).

**What v1 freezes** (AD-09, AD-10):

| Frozen in v1 | Where it is specified |
|---|---|
| Page front matter, the page types and their parts | [Page files](#page-files) |
| The five registers: columns, order, vocabularies | [Registers](#registers) |
| Ids, the citation syntax and the locators | [Identifiers](#identifiers), [Syntax at a glance](#syntax-at-a-glance) |
| The frozen strings | [Strings](#strings) |
| `corpus.toml`, `extraction.toml`, `scan-allow.toml` and `edition.toml` | [Areas and views](#areas-and-views), [Sources](#sources), [Dates, supersession, handling and labels](#dates-supersession-handling-and-labels), below |
| The layout of `corpus/` | [Layout of corpus/](#layout-of-corpus) |
| The label list in `handling`, the origin, `doc_kind` and `series` columns of `sources.csv`, the `field` locator, the structured form of `applies_to`, the company id shapes, the company-only vocabulary values and the `csv-rows` extraction method | [Handling and labels](#handling-and-labels), [Registers](#registers), [Identifiers](#identifiers), [Status vocabularies and promotion rules](#status-vocabularies-and-promotion-rules), [Extraction](#extraction) |

Check codes are stable. A check's message may be improved without a version change. The `--json` report has its own `report_version`.

**Changes.** Every change to the files or to what `check` accepts is a schema change. It needs a decision record and the owner's approval (the format gate in AD-21), and it raises `schema_version`.

| Schema impact | Meaning | Migration |
|---|---|---|
| none | The accepted files and the meaning of every field are unchanged | None; `schema_version` is unchanged |
| additive | Everything valid before is still valid: a new optional field or column, a new vocabulary value, a new page type, a new check | Stamp-only when existing readers accept both shapes; an exact-header change needs an explicit preserving migration |
| breaking | Some valid file needs rewriting: a field or column removed, renamed or reordered, a meaning changed, a rule tightened, an id form or frozen string changed | Rewrites the files |

The pre-`edition-001` [ADR 0002 amendment](../decisions/0002-corpus-format-v1.md#amendment-five-digit-claim-continuation-pre-edition-001) folds five-digit public claim continuation (`10000` to `99999`) and numeric-within-area `claims.csv` ordering into `schema_version = 1` before any reader or edition ships: every four-digit claim id and its relative sort order stay unchanged, so no existing file needs rewriting.

The pre-`edition-001` [ADR 0002 amendment on the review date basis](../decisions/0002-corpus-format-v1.md#amendment-review-date-basis-pre-edition-001) changes the meaning of a page's computed `review_by` (it counts from the later of `as_of` and the newest Change note date) and folds into `schema_version = 1` the same way: no file is rewritten.

**How a bump ships.**
- Before R2, the bump ships with a one-off migration, `cargo xtask migrate-<from>-to-<to>`, in the same pull request. It runs once, at home, and is deleted by a later card. A test runs it on the `corpus-mini` fixture and compares the result with a committed expected tree.
- From R2, bumps are batched into planned tool releases. `fieldbook migrate` rewrites a Corpus or a company store in place, is idempotent, and is the only command that changes `schema_version`.
- The executable refuses a Corpus whose `schema_version` is newer than it knows (FB0100). An edition with the same `schema_version` never needs a new executable (invariant 4 in ADR 0001).
- `note` pages arrive at Day 30 and make the version 2. The objects that follow are in [Object types beyond v1](#object-types-beyond-v1).

**`edition.toml`** is written by `build` into the root of the edition, and `cargo xtask edition` ships it in the edition zip. It is not hand-written and not in git.

| Field | Type | Meaning |
|---|---|---|
| `edition` | string | `edition-007`, or `unreleased` for a build without `SOURCE_DATE_EPOCH` or without a 40-hex commit |
| `number` | integer | The number of the newest `## edition-NNN` heading in `changes.md`, which the tag repeats; `0` when unreleased |
| `built` | string | The UTC date of the commit as `YYYY-MM-DD`; empty when unreleased |
| `tool_version` | string | The version of `fieldbook` that built it, such as `0.2.0` |
| `min_tool_version` | string | The oldest tool version that supports `schema_version` |
| `schema_version` | integer | Copied from `corpus.toml` |
| `commit` | string | 40 lowercase hex characters; empty when unreleased |
| `fingerprint` | string | SHA-256 over the Corpus files that ship, as 64 lowercase hex characters |
| `[counts]` | table of integers | `pages`, `source_versions`, `claims`, `questions`, `conflicts`, `terms`, `originals` |
| `[ids]` | table of arrays | The sorted ids of `pages`, `source_versions`, `claims`, `questions`, `conflicts` and `terms`; `changes.html` diffs them against the previous edition (`build --since`) |

- The fingerprint is the SHA-256 of the text made from one line per file under the edition's `corpus/` folder, `<sha256 hex>  <path relative to the edition root>`, sorted by path as bytes, LF endings, a final newline.
- `min_tool_version` comes from one table in `fieldbook-core`: the first tool version that supports each schema version. It is `0.1.0` for schema 1.
- `edition.toml` has a fixed key order and no clock reading, so two builds of the same commit give the same bytes.

## Syntax at a glance

Everything an author can write inline, with what the reader makes of it. The citation grammar is exactly as in AD-10; the sections after the table explain it.

| Syntax | Example | Renders as |
|---|---|---|
| Source-version citation, page | `[@S-0042-v2, p. 14]` | A numbered marker that links to the page's Sources part. The Sources row names the version and the locator and links to `originals/S-0042-v2.pdf#page=14` |
| Page range | `[@S-0042-v2, pp. 14-15]` | As above, with the link to the first page |
| Source-version citation, section | `[@S-0107-v1, sec. (e)(1)]`, `[@S-0107-v1, § 15.404-1]`, `[@S-0107-v1, para. 3]` | A numbered marker. The Sources row links to the `s/S-0107-v1.html` page at the section anchor |
| Source-version citation, sheet | `[@S-0200-v1, sheet Rates!B2:D9]` | A numbered marker linking to the `s/` page |
| Source-version citation, record | `[@S-0300-v1, rec 48213]` | A numbered marker linking to the `s/` page |
| Company citation | `[@SO-000042-v1, rec 14]`, `[@CL-XES-000012]`, `[@CM-000012-v2]` | As the public forms. The company shapes exist only in a company store (see [Identifiers](#identifiers)) |
| Company citation, record field | `[@CM-000012-v3, field burden_eng]` | A numbered marker. The Sources row names the record version, the column and the calculation run that produced the figure. Company store only |
| Source-version extension locator | `[@S-0042-v2, ext structured_path %2Fitems%2F0]` | The source row and preserved locator; unavailable until its resolver is built |
| Source-version citation, no locator | `[@S-0042-v2]` | A numbered marker. Allowed only where the statement is not consequential |
| Claim citation | `[@CL-CMP-0011]` | A numbered marker. The Sources row shows the claim, its verification, the source version, the locator, the quote and the verified date |
| Question reference | `[@Q-027]` | A neutral chip with the id, linking to `questions.html#Q-027`. It points; it never supports |
| Conflict reference | `[@XC-14]` | A neutral chip with the id, linking to `conflicts.html#XC-14`. It points; it never supports |
| Several citations | `[@CL-CMP-0011; @S-0042-v2, pp. 14-15]` | One numbered marker per citation, side by side. Separate with a semicolon and one space |
| Page link | `[accounting-system clause](../business-systems/cmp-acct-system.md)` | A link to `cmp-acct-system.html`. The target is the real relative path to the page file, so the link also works in a Markdown viewer |
| Glossary term | Front matter `terms = ["material_weakness"]`; the text says "material weakness" | The first occurrence of the term's label or an alias in the body becomes a link to `glossary.html#material_weakness`. Later occurrences stay plain |
| Precise wording | `> Precise: The contractor shall maintain …` | A blockquote that the reader shows under the plain paragraph before it when the wording toggle is on. It always prints |
| Sources part | `## Sources` | The heading alone. `build` fills it from the page's citations |
| Empty part | `Not established.` | The text of a part that has no content yet. Never leave a part other than Sources blank |
| Synthetic label | `Synthetic example. The numbers are made up.` | The first paragraph of every Worked example part that has content |
| Change note entry | `- SOURCE_REFRESH, 2025-11-01: Replaced the threshold after the notice.` | A class chip, the date and the sentence |

**Citation grammar** (Pandoc style; the writer is strict):

```ebnf
citation = "[" , cite , { "; " , cite } , "]" ;
cite     = "@" , key , [ ", " , locator ] ;
key      = version | claim | question | conflict | company ;
version  = "S-" , digit , digit , digit , digit , "-v" , digits ;
claim    = "CL-" , area , "-" , ( digit , digit , digit , digit
                                | nonzero , digit , digit , digit , digit ) ;
question = "Q-" , digit , digit , digit ;
conflict = "XC-" , digit , digits ;
company  = "CL-X" , capital , capital , "-" , cnum
         | prefix , "-" , cnum , [ "-v" , digits ] ;   (* company store only *)
prefix   = capital , capital ;
cnum     = digit , digit , digit , digit , digit , digit , { digit } ;
locator  = "p. " , digits
         | "pp. " , digits , "-" , digits
         | ( "sec. " | "§ " | "para. " ) , label
         | "sheet " , name , "!" , range
         | "rec " , record-key
         | "field " , column                    (* company store only *)
         | "ext " , extension-kind , " " , extension-payload ;
(* label, name, range, record-key and column are text without ";" or "]" *)
```

- `digit` is `0` to `9`. `nonzero` is `1` to `9`. `digits` is one or more digits. `area` is the capitals of an area code (`CMP`). A public claim number has four digits (`0001` to `9999`) or five digits from `10000` (never a leading `0` in five digits). The conflict form needs at least two digits (`XC-14`), which is how `check` is stricter than the lexer in [search.md](search.md). `capital` is `A` to `Z`. A `cnum` has six to ten digits: the writer's digits, then six sequence digits. The lexer reads the company forms generally (a company claim, a company versioned id and a company id), so a new record type needs no lexer change. `check` accepts only the prefixes in the `FAMILIES` table of `fieldbook-core`, and only in a company store: a company form in a file of the repository Corpus is an error (FB0416).
- A claim, question or conflict key takes no locator. A locator belongs to a source version. The one exception is `field`, which belongs to a record version and exists only in a company store; its `column` is a column name of the record's type, of the form `[a-z][a-z0-9_]*`.
- Examples: `[@S-0042-v2, p. 14]`, `[@CL-CMP-0011]`, `[@Q-027]`, `[@XC-14]`, `[@CL-CMP-0011; @S-0042-v2, pp. 14-15]`.
- A `sheet` locator is `sheet NAME!D214` for one cell or `sheet NAME!A2:F9` for a range of two different corners (`A1` to `XFD1048576`, capitals, no `$`). It is split at the last `!`. A sheet name is written with three escapes, so that any name can be cited: `%25` for `%`, `%3B` for `;` and `%5D` for `]`, with the hex in capitals. A name matches exactly, after Unicode NFC. `rec NAME!213` names data row 213 of that sheet, with the name written the same way.
- Rejected forms: `[S-0042@v2 p.14]`, `{CL-CMP-0012}` and `[[…]]`. None is a canonical citation, and the tolerant reader does not accept them.

**Locator kinds.** The kind is written by the prefix. The claims register stores the locator as written, prefix included (`p. 14`), next to its kind.

| Written locator | Kind in `claims.csv` | Link target | What `check` resolves |
|---|---|---|---|
| `p.`, `pp.` | `pdf_page_box` | `originals/S-0042-v2.pdf#page=14`, with the locator text beside the link | The page number is within the page count, and the quote is on that page (or those pages) |
| `sec.`, `§`, `para.` | `text_section` | The `s/` page, at the section anchor | The label is in the extraction's `[locators]` table, and the quote is on the page it maps to |
| `sheet` | `sheet_cells` | The `s/` page, at the first row's window | Where the extraction is `xlsx-sheets`: the sheet exists, the cell or range lies within it, and the quote is in the cells it names (FB0311, FB0308). For any other extraction the source version exists and the cells are not read |
| `rec` | `record_key` | The `s/` page, at the row's window | Where the extraction is `csv-rows`: data row N exists, and the quote is in that row's text (FB0311, FB0308). Where it is `xlsx-sheets`, the key is `NAME!N`: the sheet exists, data row N of that sheet exists, and the quote is in that row's text. For any other extraction the source version exists and the record is not read |
| `field` | `record_field` | The record's page when `later-record-views` exists; until then none, and the locator text is shown | Company store only (FB0210 elsewhere). The record version exists, has that column, and the cell is not empty (FB0311). The quote is the cell's text and is not matched against an original |
| `ext KIND PAYLOAD` | `ext:KIND` | The source row with locator text, no fabricated anchor | A registered resolver checks the payload; an unsupported kind yields unavailable, never a verified locator |
| none | `no_fine_locator` | The `s/` page | Allowed only on a claim whose `consequential` is `false` |

**Extension locators.** `extension-kind` is `[a-z][a-z0-9_]*`, at most 64 ASCII bytes, excluding the built-in kind names. `extension-payload` is nonempty UTF-8, encoded bytewise: ASCII letters, digits, `-`, `.`, `_` and `~` stay literal; every other byte is `%HH` with uppercase hex. Decode once, then re-encode to require the same canonical text. Invalid escapes, encoded unreserved bytes, invalid UTF-8, empty payload and a full locator over 1,000 characters are refused. No normalization changes payload bytes. The claims kind is `ext:<kind>` and its locator cell holds the whole `ext <kind> <payload>` form; a different kind in the two cells is invalid. Both strict and tolerant readers preserve this canonical form unchanged.

`schema::locator` owns the append-only registry of extension names and meanings. Its `LocatorKind::Extension(kind)` and `Locator::Extension { kind, payload }` preserve a valid unknown kind and decoded payload; `Unsupported { kind, payload }` is the resolver result until a matching implementation exists. Formatting re-encodes those same bytes. Unsupported is an unavailable fact, not a fine resolved locator or a verified quote; it cannot support promotion. The registered built-in forms still use their original parsers and reject malformed input. Adding a registry row and resolver changes no existing meaning and needs no Corpus schema bump.

**Strict writer, tolerant reader.**
- The writer (everything in `corpus/`, and every citation the tool prints) uses only the canonical form above. `check` rejects anything else in the Corpus (FB0301).
- The tolerant reader is for pasted answers only (Day 30, `cite-check`). It also accepts a missing `@`, a missing comma, Unicode dashes, `p.14` and `§15.404-1`, and reports each of them as "non-canonical". It also reads `[@<area>-note-<nnn>]` as an answer-only reference: the form is never a Corpus citation, `check` keeps rejecting it with FB0300, and `cite-check` preserves its normal precedence: absent is `broken`, visible but outside a supplied context is `outside_context`, and visible in-context (or checked without a context) is `weak`.
- One lexer in `fieldbook-core` reads both. The JavaScript port in `assist.js` (Day 30) is held to it by shared goldens.
- `pulldown-cmark` reads `[@A][@B]` as a reference link. Put the citations inside one pair of brackets, or leave text between them. `check` flags `][@` (FB0302).
- **Citation attempt.** A pair of square brackets in a page body is read as a citation when it is empty, holds an `@`, or starts with the shape of a key, such as `S-42` or `CL-CMP-1`. Other brackets are prose. Code spans and fenced code are not read. FB0300 reports an attempt whose brackets are unbalanced or empty or whose key is not an id.

**What a number cites.** A number, threshold, date or rule in prose cites a claim, not a source version. A source-version citation supports a statement that is not consequential, or points the reader to a passage. A question or conflict reference is neutral: it says "this is open" or "this is disputed", and it is never evidence. `cite-check` treats it the same way (AD-18).

## Strings

`fieldbook-core::strings` is the single source of every string below. `build` writes it to `assets/strings.js`, which assigns one object, `window.FB_STRINGS`, with the keys shown. A test fails on any of these literals appearing elsewhere in the code. The card `core-types-and-strings-2` makes `cargo xtask docs` generate this section from the module; until it merges, this hand-written section is the source and the code must match it exactly.

<!-- strings:begin -->

**Frozen in v1.** Changing any of these is a breaking schema change.

| Key | String | Value |
|---|---|---|
| `abstention` | Abstention phrase | "Not established in the library." Matched case-insensitively; final punctuation optional |
| `empty_part` | Empty part | "Not established." |
| `refusal` | Refusal (`cite-check`) | "Do not use this answer." |
| `parts` | The eight parts, in order | Purpose · The rule · How it works · Worked example · Never confuse · Company calibration · Sources · Change note |
| `brief_parts` | `brief` parts | Question · Recommendation · Evidence · Options · Unknowns · Next action |
| | `note` | No fixed parts |
| | Page links | Plain relative `.md` links, rewritten to `.html` |
| | Terms | Listed in front matter `terms` |
| | Precise wording | A blockquote that starts `Precise:` |
| `paths` | Paths | `data/registers.xlsx`, `data/<register>.csv`, `templates/<name>.xlsx`, `day-one.html`, `assistant.html`, `map.html`, `ASSISTANT.md` |
| `rounding` | Rounding | "Each extension rounds half away from zero to cents; totals sum the rounded extensions." |
| `origin_labels` | Origin labels | Quoted, Computed, Judgment. FB0380 (missing computed or judgment evidence) and FB0381 (missing or inconsistent quoted evidence) are reserved and become active with the card `r2-records` |
| `binary` | Binary | `fieldbook` |

- **Part headings** are level-2 headings with exactly these words: `## Purpose`, `## The rule`, `## How it works`, `## Worked example`, `## Never confuse`, `## Company calibration`, `## Sources`, `## Change note`. `check` enforces them, in this order, on every page type except `brief` and `note`. `## Sources` is written as the heading alone; `build` fills it.
- **`brief` headings** are `## Question`, `## Recommendation`, `## Evidence`, `## Options`, `## Unknowns`, `## Next action`. A `brief` has no Sources heading; `build` appends the sources block after its last part. `note` pages (Day 30) have no fixed parts.
- **Abstention.** A search or an answer that finds nothing says the abstention phrase, never "No results". `cite-check` accepts the phrase in any case, with or without the final full stop.
- **Frozen words (AD-23).** edition, transfer statement, intake pack, day-one binder, context pack. "Pack" is never used alone. The repository README's "Words we use" defines them.

**Verb map.** The site shows a command only as one of these chip strings. They live in `strings` (key `chips`). A CI test replaces each placeholder in angle brackets with a valid sample value and parses every chip with clap's `try_get_matches_from`. The samples for `<folder>`, `<n>`, `<proposal-id>` and `<word>` are `inbox`, `1`, `P-0007` and `duplicate`. A chip names a file, never a pipe, and uses double quotes only, so it runs unchanged in cmd and in PowerShell Constrained Language Mode. Its tag is "At home", or "R2 · laptop" for a chip that exists only from R2. Adding a chip is additive; changing or removing one is a content change that needs a card and updated goldens. Home `new` requires `--confirm-public` for supplied text; its chips include the flag, which the owner runs only after confirming that text is public. `add` refuses a public-marked source without `--confirm-public` (FB0410, exit 1); the `add` chips are completed with the flag by the person who confirms that the file is public.

| Intent | Chip string | From |
|---|---|---|
| Start a page | `fieldbook new page --confirm-public <area>-<slug> --type <type> --subarea <subarea>` | Day 1 |
| Add a question | `fieldbook new question --confirm-public --area <a> "<text>"` | Day 1 |
| Start a claim | `fieldbook new claim --confirm-public --area <a> --source <version-id> --at "<locator>" --quote "<text>"` | Day 1 |
| Start a conflict | `fieldbook new conflict --confirm-public --area <a>` | Day 1 |
| Start a glossary term | `fieldbook new term --confirm-public --area <a>` | Day 1 |
| Add a source file | `fieldbook add <file>` | Day 1 |
| Add a new version of a source | `fieldbook add <file> --source <source-id>` | Day 1 |
| Check the Corpus | `fieldbook check` | Day 1 |
| Format the files | `fieldbook fmt` | Day 1 |
| Rebuild the site | `fieldbook build` | Day 1 |
| Find an id or a word; list the versions of a source | `fieldbook search <query>` | Day 1 |
| See what needs work | `fieldbook status` | Day 1 |
| Write the desk pages | `fieldbook status --page` | Day 1 |
| Record that the owner verified a claim | `fieldbook verify <claim-id> --on YYYY-MM-DD` | Day 1 |
| Add every file in a folder | `fieldbook add <folder>` | Day 1 |
| Verify the audit journal | `fieldbook audit verify` | Day 1 |
| Show recent activity | `fieldbook audit show` | Day 1 |
| Back up the working data | `fieldbook backup` | Day 1 |
| Check a backup | `fieldbook backup verify` | Day 1 |
| List backups | `fieldbook backup list` | Day 1 |
| Prove a backup restores | `fieldbook restore --snapshot <n> --drill` | Day 1 |
| Build a context pack | `fieldbook context "<question>"` | Day 30 |
| Check a pasted answer | `fieldbook cite-check <file>` | Day 30 |
| Save an answer as a note | `fieldbook new note --ai-drafted --confirm-public --area <a> --from <file>` | Day 30 |
| Propose sources from a folder | `fieldbook add <folder> --propose` | Day 30 |
| Review proposals | `fieldbook queue review` | Day 30 |
| Accept a proposal | `fieldbook queue accept <proposal-id>` | Day 30 |
| Reject a proposal | `fieldbook queue reject <proposal-id> --reason <word>` | Day 30 |
| Flag the last question | `fieldbook queue flag` | Day 30 |
| Ask a question through the gate | `fieldbook ask "<question>"` | Day 30 |
| See the state of each AI port | `fieldbook ai status` | Day 30 |
| See the policy in force | `fieldbook policy show` | Day 30 |
| Put a decision to the model | `fieldbook decide <file>` | Day 90 |
| Build the overlay site, then print from Edge | `fieldbook build --overlay` | R2 |
| See what the company data sources would add | `fieldbook sync --dry-run` | R2 |

Three things have no chip. A link between pages is front matter, and `check` resolves it. The regression set runs inside `check`. Opening an original is a link to the file at its locator. The flags of each command are in the generated `cli.md`.

**The `new` verbs.** Each of `new page`, `new question`, `new claim`, `new conflict` and `new term` writes one page or one register row in its entry state and prints the page's path or the row's id. The chip supplies `--confirm-public`; it is the typed public act (ADR 0004), so the verb refuses without it (FB0410), refuses marked text even with it (FB0400), and a machine with a company store refuses the typed act (FB0822). A flag left off is asked for on standard error only when standard input is a terminal; otherwise the verb exits 2 and writes nothing. A value with a line break or another control character exits 2 and writes nothing, because a register cell cannot carry one. The verbs do not run `check`, so a cell that no flag fills keeps its entry value or stays empty. `new claim` takes the date and volatility from its source version and sets `drafted_by` from the state's role. `new term` takes `--id <term_id>`, or the next `<area>_term_<n>`. A request is admitted before an interrupted original is resumed, so a changed request that would be refused is refused, not matched against the pending write.

These verbs have no chip, and `NO_CHIP` in `crates/fieldbook/tests/chips.rs` lists them: `audit export`, `queue list`, `queue show`, `queue ingest`, `queue pin`, `ai test`, `policy check`, `label add`, `label reapply`, `label show`, `mcp` and `chat`. The test `every_verb_has_a_chip` fails on a clap verb that is in neither this map nor `NO_CHIP`. The test `verbs_match_ad17` compares the verbs of the clap definition with the fenced `verbs` block under the AD-17 table in [ADR 0001](../decisions/0001-foundation.md#ad-17-authoring-experience).

**Other fixed wording.** `strings` also holds these. The last column names the decision in [ADR 0001](../decisions/0001-foundation.md) that settles each. A change is a content change that needs a card and updated goldens, not a schema change.

| Key | Wording | Decision |
|---|---|---|
| `cite_statuses` | broken · broken locator · outside context · superseded · weak · unchecked locator · unchecked quote · quote mismatch · verified, in precedence order | AD-18 |
| `verified_text` | "Citation resolves at this locator" | AD-18 |
| `non_canonical` | "non-canonical" | AD-10 |
| `left_out_budget` | "Left out (budget)" | AD-18 |
| `left_out_policy` | "Left out (policy)" | ADR 0004 |
| `pack_labels` | "Labels: <ids>", the ids sorted and joined by a comma and a space. It replaces the earlier handling line of a pack | ADR 0004 |
| `not_available` | "<Thing> is not available until <release or trigger>." | AD-09 |
| `unreleased_build` | "unreleased build" | AD-12 |
| `print_check` | "Check the current edition" | AD-12 |
| `not_remembered` | "Not remembered in this browser" | AD-15 |
| `set_start_date` | "Set your start date" | AD-15 |
| `set_start_date_plan` | "Set a start date to mark today" | AD-15 |
| `day_n_of_90` | "Day N of 90", with N the day count the page script computes | AD-15 |
| `day_n_today` | "Day N of 90 · the line marks today", with N the day count the page script computes | AD-15 |
| `tag_at_home` | "At home" | AD-17 |
| `tag_r2_laptop` | "R2 · laptop" | AD-17 |
| `assistant_labels` | "Ask" · "Send" · "Stop" · "Copy context pack" · "Check citations" · "Save as note" · "Copy as draft note" · "This session only" | AD-15 |
| `ai_note_label` | "AI-drafted · unreviewed" · "AI-drafted · reviewed" | AD-15 |

<!-- strings:end -->

**Fixed wording outside `strings`.** Two more pieces of fixed wording are not `strings` keys: the synthetic label (`Synthetic example. The numbers are made up.`) and the six row names of a `clause-card` (see [Page files](#page-files)). Each is one constant in the check code, pinned by a test of its text. Changing one is a content change that needs a card and updated goldens.

`verified_text` says that a citation resolves: the id exists and the locator is in the source. It does not say that the answer is right, and the wording of the answer is not checked.

## Layout of corpus/

```text
corpus/
  AGENTS.md  ASSISTANT.md  README.md
  corpus.toml
  changes.md  scan-allow.toml
  pages/<area-slug>/<subarea>/<page-id>.md
  registers/        sources.csv claims.csv questions.csv conflicts.csv glossary.csv
  sources/public/   S-0042-v2.pdf ...          byte-exact originals
  sources/text/<version-id>/   extraction.toml  p0001.txt ...
  templates/        <type>.md   (workbooks/*.csv from E1)
  eval/regression.toml
  pipeline/rules.toml   (optional)
```

| Path | What it holds | Written by | In the edition zip |
|---|---|---|---|
| `AGENTS.md` | Content rules for agents (AD-22) | Hand | No |
| `ASSISTANT.md` | The assistant rules, `rules_version = 1` (AD-18). Prepended to every context pack | Hand | Yes, at the edition root as `ASSISTANT.md` |
| `README.md` | How to read the raw files with no tool | Hand | No |
| `corpus.toml` | `schema_version`, the areas, the views and `[assistant]` | Hand | Yes |
| `changes.md` | One `## edition-NNN` section per edition, newest first; no dates | Hand | No; it renders as `changes.html` |
| `scan-allow.toml` | Allowances for the marking scan | Hand | No |
| `pages/<area-slug>/<subarea>/<page-id>.md` | One page per file | Hand | Yes |
| `registers/*.csv` | The five registers | Hand; `fmt` normalises | Yes |
| `sources/public/<version-id>.<ext>` | Byte-exact originals, binary in git | `fieldbook add` | No; the originals zip |
| `sources/text/<version-id>/` | `extraction.toml` and one text file per page, `p0001.txt` and on | `cargo xtask extract`, committed | Yes; not for a `quarantined` version |
| `templates/<type>.md` | One page template per v1 page type | Hand | No |
| `templates/workbooks/*.csv` | Synthetic sources of the data-only workbooks | Hand | No; `fieldbook build` writes `templates/<name>.xlsx` from them (E1) |
| `eval/regression.toml` | The search regression questions, run by `check` | Hand | No |
| `pipeline/rules.toml` | Optional. The intake classification rules, each with an `example` that `check` replays (FB0826) | Hand; the lead writes it | No |

**Three kinds of data.**
- **Hand-written:** pages, registers, `corpus.toml`, `changes.md`, `scan-allow.toml`, the templates, the rule files and `pipeline/rules.toml`.
- **Written by a command and committed:** originals and their register rows (`add`), extraction (`cargo xtask extract`), and any file `fmt` has normalised.
- **Computed, never stored:** a page's `claims` and `review_by`, a source version's "superseded" state, extraction status, and every count. Writing a computed field is an error.

Generated output is in `out/`, which git ignores: the edition, the originals and the zips.

The state folder (`.fieldbook/` at home, `state\` on the laptop) is not part of the Corpus: `check` never reads it, git ignores it, and no edition contains it. A company store is not part of the repository Corpus either. On the laptop the content root is the company store (`overlay\`), which holds the same kinds of file with company ids and labels. Its extra files and its layout are in [company-store.md](company-store.md).

**Rule files.** `ASSISTANT.md` has at most 40 lines, and its first line is `rules_version = 1`. `AGENTS.md` here is `corpus/AGENTS.md`, and it has at most 40 lines. The repository's own `AGENTS.md` is not a Corpus file: control AG-01 holds it to 100 lines. `check` reports a rule file out of form (FB0125). `ASSISTANT.md` ships in every edition and is prepended to every context pack. `AGENTS.md` never ships.

**File rules.** `fieldbook fmt` normalises them; `check` reports a file that is not normalised (FB0122).
- UTF-8 without a byte-order mark, LF line endings, Unicode NFC, no trailing whitespace, exactly one final newline.
- `.gitattributes` sets `* text=auto eol=lf` and marks `corpus/sources/public/**` binary.
- `fmt` never parses front matter and writes it back, because the TOML serializer drops comments. It removes a byte-order mark and converts line endings in the whole file. It reports any other front-matter fault, such as trailing whitespace or text that is not NFC, and leaves the text alone.
- For registers that `table::read` accepts, `check` raises FB0122 at 1:1 exactly when `table::is_normal` is false. No separate line-level check applies; a trailing space in a cell is data and is preserved. Unreadable registers receive their parse diagnostic instead. Other text under `corpus/` follows the rules above, except `sources/`, whose bytes are hashed instead (FB0211, FB0212).

**Path rules** (FB0123, FB0602).
- A path is at most 150 characters from the edition root. A test prefixes a 100-character root and requires every path to stay under 260.
- No two paths differ only by case. No Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`), no trailing dot or space, none of `<>:"/\|?*` and no control character. Names are NFC.
- An id is at most 64 characters. A page file is `<page-id>.md`; an original is `<version-id>.<ext>` with a lowercase extension of one to five letters or digits.

## Areas and views

The 13 areas and the 3 views are data in `corpus/corpus.toml`, not code. The areas appear in the order of the file.

| Code | Folder (`slug`) | Name | Subareas (folders) |
|---|---|---|---|
| `fnd` | `foundation` | Foundation | `evidence`, `glossary`, `questions`, `fixtures`, `systems`, `onboarding` |
| `mkt` | `market` | Markets | `taxonomy`, `feeds`, `ledgers`, `company`, `peers`, `programs` |
| `est` | `estimating` | Estimating | `pricing`, `wbs-analogy`, `estimating-system`, `escalation` |
| `sup` | `supply-chain` | Supply chain | `sourcing-evidence`, `procure-to-pay`, `supplier-identity` |
| `rte` | `indirect-rates` | Indirect rates | `pools-bases`, `rate-lifecycle`, `absorption`, `variance` |
| `prg` | `program-finance` | Program finance | `baseline`, `eac`, `evms`, `risk-opportunity` |
| `fpa` | `fpa` | FP&A | `planning-chain`, `backlog`, `kpi`, `forecast-vintage` |
| `ctf` | `contract-finance` | Contract finance | `funding`, `billing-payment`, `financing`, `cash` |
| `cmp` | `compliance` | Compliance | `regulatory-framework`, `cas-allowability`, `business-systems`, `audit-retention`, `trade-cyber` |
| `cls` | `close-revrec` | Close and revenue | `close`, `revenue`, `inventory-wip`, `fixed-assets`, `group-bridge` |
| `mna` | `mna` | M&A | `screening`, `valuation`, `deal-gates`, `diligence` |
| `opm` | `operating-model` | Operating model | `decision-rights`, `cadence`, `corrective-action`, `talent` |
| `ind` | `industrialization` | Industrialization | `capacity`, `ramp`, `investment-case`, `cash-returns` |

Market has no `cases` subarea: its company, peer and program pages are in `company`, `peers` and `programs`.

**`[[area]]` fields.**

| Field | Type | Rule |
|---|---|---|
| `code` | string | Three lowercase letters; unique. The prefix of every page id; the `area` of every register row; its capitals appear in claim ids. In a company store every code starts with `x`, and in the repository Corpus none does (FB0100) |
| `slug` | string | The folder under `pages/`; lowercase letters, digits and hyphens, starting with a letter; unique |
| `name` | string | Short display name |
| `summary` | string | One line for the area header |
| `subareas` | array of strings | The only folders allowed under the area folder; the same character rule as `slug`; unique within the area |

**Views.** A view is a filter over the pages of one area. It owns no pages, ids or folders. Competitor and public program are not objects: they are Market pages in the subareas `peers` and `programs`.

| Code | Name | Filter (`area`, `subarea`) |
|---|---|---|
| `CPY` | Company profile | `mkt`, `company` |
| `CPT` | Competitors | `mkt`, `peers` |
| `PGM` | Programs | `mkt`, `programs` |

A view's `code` is three capital letters and never equals an area code in capitals. Its `filter` names an existing area and one of that area's subareas. The reader shows the eyebrow "View over MKT", which is "View over " and the filter's area code in capitals; no field holds it. A view keeps its route and sidebar entry, and builds its sections by filtering the area's pages.

**Changing the data.** The owner may add an area, a subarea or a view in `corpus.toml` without a schema change, because the tool reads them as data. Removing or renaming one is a breaking change: ids are never renamed (see [Identifiers](#identifiers)). `check` reports a page whose area or subarea is not in `corpus.toml` (FB0106, FB0107) and a malformed entry (FB0100).

## Page files

A page is one Markdown file with TOML front matter. `fieldbook-core` reads the front matter with the `toml` crate and `deny_unknown_fields`, so an unknown field is an error (FB0102).

**Shape of the file.**
- UTF-8 without a byte-order mark, LF line endings, NFC text (see [Layout of corpus/](#layout-of-corpus)).
- The first line is `+++`, then the TOML front matter, then a line that is `+++`, then one blank line, then the body.
- The path is `corpus/pages/<area slug>/<subarea>/<page-id>.md`. The id equals the file stem (FB0105). The id's first segment is the area code (FB0106). The folders are the area's `slug` and one of its `subareas` (FB0107).
- A page has no `#` heading. The reader draws the title from the front matter.

Synthetic front matter:

```toml
+++
id = "cmp-withholding"
title = "Payment withholding for business-system deficiencies"
area = "cmp"
subarea = "business-systems"
type = "concept"
stage = 3
readiness = "PUBLIC-NOW"
status = "draft"
as_of = 2025-11-01
handling = ["public"]
terms = ["significant_deficiency"]
volatility = "V1"
+++
```

**Front matter fields**, in the order `new page` writes them. `check` does not require the order. Vocabularies are defined once, in [Status vocabularies and promotion rules](#status-vocabularies-and-promotion-rules).

| Field | Type | Required | Allowed values and rule |
|---|---|---|---|
| `id` | string | yes | The page id form in [Identifiers](#identifiers). Equals the file stem. Unique in the Corpus |
| `title` | string | yes | A plain noun phrase of 1 to 120 characters. No markup characters (`<`, `>`, `*`, `[`, `]` or a backtick) and no final full stop |
| `area` | string | yes | An area `code` in `corpus.toml`. Equals the first segment of `id` |
| `subarea` | string | yes | One of that area's `subareas`. Equals the folder |
| `type` | string | yes | `concept`, `procedure`, `reference`, `case`, `clause-card`, `calc`, `checklist`, `template` or `brief`. `note` joins at Day 30 with schema 2 |
| `stage` | integer | yes | 1 to 8: the curriculum stage in which a reader meets the page |
| `readiness` | string | yes | `PUBLIC-NOW`, `SYNTHETIC-NOW`, `MAP-ONBOARDING`, `POLICY-GATED` or `CONTRACT-SPECIFIC` |
| `status` | string | yes | `draft`, `reviewed` or `stable`. The maturity of the page, never of a claim |
| `as_of` | date | yes | A TOML local date such as `2025-11-01`: the date the page's content describes. Not later than the oldest `as_of` among the claims the page cites (FB0118, strict) |
| `handling` | array of strings | yes | Label ids, sorted, none repeated, such as `["public"]`. The repository Corpus accepts only `["public"]` (FB0410). A company store accepts the ids its policy lets that store hold (FB0411, FB0412). See [Handling and labels](#handling-and-labels) |
| `terms` | array of strings | no | Glossary term ids the page uses. Each exists (FB0112) |
| `prereq` | array of strings | no | Page ids to read first. Each has a `stage` no later than this page's. The graph has no cycle (FB0110, FB0111) |
| `questions` | array of strings | no | `Q-nnn` ids the page depends on (FB0113) |
| `conflicts` | array of strings | no | `XC-nn` ids the page depends on (FB0113) |
| `links` | array of strings | no | Page ids of related pages that another area owns. Each exists (FB0110). The reader lists them with their titles |
| `volatility` | string | no | `V1`, `V2` or `V3`. When absent, the most volatile class among the cited claims, or `V3` when the page cites none |
| `supersedes` | array of strings | no | Page ids this page replaces (FB0110, FB0115) |
| `superseded_by` | array of strings | no | Page ids that replace this page. The replacing page lists this one in `supersedes` (FB0115) |
| `origin` | string | note only | `ai_drafted`. Required on a `note` (FB0709) and refused on any other type as an unknown field (FB0102) |
| `reviewed` | boolean | note only | `false` when `new note` writes the page; the owner sets `true` after reading it |
| `question` | string | note only | The question the answer was given for, on one line |
| `context_fingerprint` | string | note only | The SHA-256 in hex of the ids of the context, sorted, one per line with a final line feed; 64 lowercase hex characters |

**Computed fields.** Writing either one is an error (FB0109).

| Field | Computed from | Meaning |
|---|---|---|
| `claims` | The citations in the body | The claim ids the body cites, sorted, once each. The reader shows them as the page's evidence |
| `review_by` | `as_of`, the newest Change note date and `volatility` | The date by which a person should look at the page again; see [Dates, supersession, handling and labels](#dates-supersession-handling-and-labels) |

**Page types.** Eight types carry the eight parts of [Strings](#strings), in order. A `brief` carries the six `brief` parts. A `note` (Day 30) has no fixed parts. The parts are level-2 headings. `check` reports a missing, misspelt, repeated, reordered or extra `##` heading, and any `#` heading (FB0130).

Every type has a row here and a template, `corpus/templates/<type>.md`. A test, `schema::page_types::every_type_is_complete`, loops over the page types in `fieldbook-core`. For each it requires that the template exists, parses with unknown fields refused and names the type, and that this table has a row for the type. Two more tests loop the same way: one requires that each template holds its parts in order (FB0130), and `every_page_type_has_a_render_golden` requires a golden for each render.

| Type | Parts | Use | Required content beyond the parts (FB0134) | The reader adds |
|---|---|---|---|---|
| `concept` | Eight | Define an idea and its boundaries | None | Nothing |
| `procedure` | Eight | A public method done in steps | `How it works` is an ordered list of two or more steps | Numbered steps |
| `reference` | Eight | A lookup of values, rules or lists | `The rule` contains a table with a header row | Dated rows, newest first, with a superseded row struck through and labelled |
| `case` | Eight | A public case, read for what it shows | `How it works` has two subsections, `### Public record` then `### Inference` | The record and the inference kept apart |
| `clause-card` | Eight | One clause or provision of a regulation | `The rule` begins with a table of six rows named, in the first column and in this order, Instrument, Codified citation, RFO citation, Edition date, Text status and Applies to. Each value cites a claim or says `Not established.` | The six-row table first |
| `calc` | Eight | A calculation `fieldbook-calc` can check | `Worked example` has a line `Fixture: <case id>`, such as `Fixture: CALC-01`. `check` verifies the form; a repository test verifies that the case exists (`calc-cases`) | A calculation table with a pass or fail footer |
| `checklist` | Eight | Tasks to run through | `How it works` holds only task items (`- [ ] text`) and `###` group headings. Plans use relative days ("Day 1 to Day 30"); a `YYYY-MM-DD` date in `How it works` is an error | Numbered steps and a timeline |
| `template` | Eight | A fill-in form shown as text | `The rule` holds a fenced `text` block with the blank form. A line `Workbook: templates/<name>.xlsx` may point to the fillable copy (E1) | Nothing |
| `brief` | Six | A one-page decision paper | `Evidence` is a list in which every item cites a claim or a source version. `Unknowns` is a list in which every item cites a question or says `Not established.` About 400 words at most, so that it prints on one page (FB0138). A word is a run of characters between whitespace in the text and code of the body; headings count and front matter does not | The print layout |

To add a page type, follow "Adding a page type" in [docs/cards/README.md](../cards/README.md#adding-a-page-type).

**What each part holds** (the eight-part types).

| Part | Holds |
|---|---|
| `Purpose` | One or two sentences: what the page is for |
| `The rule` | The rule, definition or fact. A claim citation follows every number, threshold, date and clause number |
| `How it works` | The steps, formula or mechanism |
| `Worked example` | A synthetic example with its known answer. Its first paragraph is `Synthetic example. The numbers are made up.` (FB0133). Numbers here need no claim |
| `Never confuse` | The neighbours the topic is mistaken for, with their glossary terms |
| `Company calibration` | The questions that settle how the topic applies to the company, as `[@Q-nnn]` references. Nothing about the company is stated as fact |
| `Sources` | The heading alone. `build` fills it from the citations (FB0132) |
| `Change note` | One entry per line, newest first: `- CLASS, YYYY-MM-DD: sentence.` The class is one of the change classes. The oldest entry, on the last line, is `NEW_KNOWLEDGE` (FB0137). The newest-first order is a convention; no check reads it. An entry date that is not a real date is FB0114 |

A `brief` holds: `Question` (what is being decided, in one sentence), `Recommendation` (what to do, first), `Evidence` (the claims it rests on), `Options` (the alternatives considered), `Unknowns` (what is not established, with its questions) and `Next action` (one sentence that names a role and what would make you revisit the decision, a number of days or an event).

**Body rules, every type.**
- CommonMark with pipe tables, task lists and fenced code blocks. Footnotes, definition lists and strikethrough are not used.
- Parts are `##`. A part may use `###`. Nothing deeper (FB0130).
- A part other than `Sources` is never blank. A part with nothing to say holds `Not established.` (FB0131). `Sources` stays empty for the build to fill (FB0132).
- No raw HTML (comments included), no images and no link to an address outside the Corpus (FB0135): a link with a scheme, or a relative link that climbs out of `corpus/`. Write an external address as plain text; `build` shows it with a copy button.
- Link to another page by its real relative `.md` path. A link inside `corpus/` whose target does not exist is an error (FB0136).
- A paragraph is one line. Do not hard-wrap.
- Terms: list the glossary term in `terms` and use its label or an alias in the text. Do not define it again; a bold `**term**:` for a glossary term is flagged (FB0139).
- Numbers: cite a claim after each. A figure with no claim citation outside `Worked example` is flagged by the number-unit lint (FB0309).

**Templates.** `corpus/templates/<type>.md` holds one template per v1 page type. `new page <area>-<slug> --type <t>` copies it. A template is not a page: it sits outside `pages/`, and its front matter holds quoted placeholders such as `"<title>"`.
- `new page` asks which of the area's subareas the page belongs to, or takes `--subarea <s>`, and writes the file there. It writes `id`, `area` and `as_of` (today, as a TOML date) and replaces `<today>` wherever it appears. It leaves every other placeholder for the author.
- A placeholder that is left in the front matter has the wrong type or is outside the vocabulary of its field, so `check` reports it (FB0102, FB0104, FB0107). A placeholder in the body is text in angle brackets outside code, which CommonMark reads as raw HTML, so `check` reports it too (FB0135).
- A template shows every required field, every required part in order, one guidance line per part as an HTML comment, and `Not established.` as the body where the type needs no structure. A type that needs a structure (FB0134) shows it with placeholders. Optional fields appear as commented lines.
- Raw HTML is an error in a page (FB0135). The author deletes each guidance line, and a page that still holds one fails `check`.

A reference-page table row with exactly one distinct claim citation uses that claim's `as_of` and `superseded_by` for display. Repeated citations of the same claim count once. Other rows have no inferred date or supersession. Sort newest first only when every row has a known date, using the first day of a partial date and retaining authored order on ties; otherwise keep the entire table in authored order. Display dates at their original precision. This rendering convention adds no required cells or rejection rule.

## Identifiers

An id never encodes meaning that can change. A claim id carries its area because areas are stable; it carries no status and no date. Ids are compared exactly, letter case included.

| Object | Form | Pattern | Example | Allocated by |
|---|---|---|---|---|
| Area code | Three lowercase letters | `^[a-z]{3}$` | `cmp` | The owner, in `corpus.toml` |
| Page | `<area>-<kebab>`, at most 64 characters | `^[a-z]{3}(-[a-z0-9]+)+$` | `cmp-withholding` | The author, with `new page` |
| Source | `S-` and four digits | `^S-[0-9]{4}$` | `S-0042` | `add` |
| Source version | The source id, `-v`, an integer from 1 | `^S-[0-9]{4}-v[1-9][0-9]*$` | `S-0042-v2` | `add` |
| Claim | `CL-`, the area in capitals, four digits, or five digits from 10000 | `^CL-[A-Z]{3}-([0-9]{4}\|[1-9][0-9]{4})$` | `CL-CMP-0011`, `CL-CMP-10000` | `new claim` |
| Question | `Q-` and three digits | `^Q-[0-9]{3}$` | `Q-025` | `new question` |
| Conflict | `XC-` and two or more digits | `^XC-[0-9]{2,}$` | `XC-14` | `new conflict` |
| Term | snake_case, at most 64 characters | `^[a-z][a-z0-9]*(_[a-z0-9]+)*$` | `material_weakness` | `new term` |
| Calculation case | A prefix from the table below, a hyphen, two or three digits | `^[A-Z]{2,4}-[0-9]{2,3}$` | `CALC-01` | `calc-cases` and the lead |
| Error code | `FB` and four digits | `^FB[0-9]{4}$` | `FB0102` | The [Check catalogue](#check-catalogue) |
| ADR | Four digits and a slug | `^[0-9]{4}-[a-z0-9]+(-[a-z0-9]+)*$` | `0002-corpus-format-v1` | Numbers reserved in PLAN.md |
| Edition | `edition-` and three or more digits | `^edition-[0-9]{3,}$` | `edition-007` | `cargo xtask edition` |
| Tool release | `v` and three integers | `^v[0-9]+\.[0-9]+\.[0-9]+$` | `v0.2.0` | The owner's tag |
| Proposal (local) | `P-` and four digits | `^P-[0-9]{4}$` | `P-0007` | `add --propose` |
| Asked question (local) | `A-` and four digits | `^A-[0-9]{4}$` | `A-0012` | `ask` |
| Company source | `SO-` and a company number | `^SO-[0-9]{6,10}$` | `SO-000042` | `add`, `sync` |
| Company source version | The company source id, `-v`, an integer from 1 | `^SO-[0-9]{6,10}-v[1-9][0-9]*$` | `SO-000042-v2` | `add`, `sync` |
| Company claim | `CL-X`, two capitals, a company number | `^CL-X[A-Z]{2}-[0-9]{6,10}$` | `CL-XES-000012` | `new claim` |
| Company question | `QO-` and a company number | `^QO-[0-9]{6,10}$` | `QO-000003` | `new question` |
| Company conflict | `XO-` and a company number | `^XO-[0-9]{6,10}$` | `XO-000002` | `new conflict` |
| Record, record version | `CM-` and a company number; the version adds `-v` and an integer | `^CM-[0-9]{6,10}(-v[1-9][0-9]*)?$` | `CM-000012`, `CM-000012-v2` | `sync` |
| Company term | `x_` and snake_case | `^x_[a-z0-9]+(_[a-z0-9]+)*$` | `x_vendor_tier` | `new term` |

A company area code is three lowercase letters starting with `x` (`xes`), and a company page id starts with it (`xes-vendor-quotes`). The rows above exist only in a company store (FB0416, FB0417).

**Calculation case prefixes.** A case id is a prefix from this table, a hyphen and two or three digits. A prefix is used only by the module named in its row, and the first card that adds a case with a reserved prefix changes its state to "now". The const `FAMILIES` holds every prefix, so that FB0401 does not take a case id for a tracker key. The test `case_prefixes_match_the_reference` compares this table with the `prefixes` of the case row of `FAMILIES`, and the test `case_files_use_a_current_prefix` checks that every case file begins with a prefix whose state is Now.

| Prefix | Cases for | State |
|---|---|---|
| `CALC` | The core calculation rules: the Day 1 cases CALC-01 to CALC-04, and CALC-05 and CALC-06 from Day 60 to 75 | Now |
| `QA`, `FC` | The lead's reference-case families. Each family is given its meaning by the first card that adds a case | Now |
| `EST` | Estimating: bid cost build and price build-up | Reserved for `later-calc-functions` |
| `QTE` | Quotes: normalisation to a comparable price | Reserved for `later-quotes` |
| `SCH` | Schedules: slip in calendar and working days | Reserved for `later-schedule-review` |
| `RTE` | The indirect rate book: pools, bases, absorption and escalation | Reserved for `later-rate-book` |
| `EAC` | Estimate at completion and estimate to complete | Reserved for `later-finance-eac-evm` |
| `EVM` | Earned-value indices | Reserved for `later-finance-eac-evm` |
| `CTF` | Contract finance: billing, funding and cash | Reserved for `later-contract-finance` |

Two digits allow 99 cases a prefix and three allow 999, so a family that outgrows 99 needs no second scheme. A case id is never renamed once an edition holds a page that cites it.

**Company numbers.** A company number is the writer's digits followed by exactly six sequence digits. Writer 0 has no extra digits: `SO-000042`. Writer 1 gives `SO-1000042`. A writer index runs from 0 to 9999 and is set in the store's `store.toml`. A sequence runs from 000001 to 999999 for each family and writer. Company ids sort by length, then byte by byte. Public sources use four sequence digits, public claims four and then five from 10000, questions three, and conflicts at least two with no four-digit cap. Company families have distinct prefixes (`SO`, `QO`, `XO`, `CM`), company claim areas start with `x`, and company terms with `x_`. The complete prefix and pattern distinguish the stores; digit count alone does not. One constant in `fieldbook-core`, `FAMILIES`, lists every family. The id parsers read it, and a test compares it with the table above, with the lexer and with the prefixes the tracker-key scan exempts.

The last two rows (before the company rows) are local to the state folder (`.fieldbook/` at home, `state\` on the laptop). A proposal records the `writer` of the store it was made for, so a queue copied from another device cannot be mistaken for this one's. They are not part of the Corpus: they never appear in a Corpus file, and the id lexer does not read them.

**Allocation.**
- `new` and `add` choose the next free number: one more than the highest number ever used in that series, retired rows included. In a company store a series is per family and per writer: the allocator reads only the rows of that family whose number carries the store's writer digits, and `sync` takes numbers the same way, so two writers cannot hand out the same id. `check` reports a repeated id (FB0203). A claim series is per area (`CL-CMP-0012` follows `CL-CMP-0011`). A claim series that reaches 9999 continues with five digits: `CL-CMP-10000` follows `CL-CMP-9999`. A five-digit number never starts with 0, so each number has one spelling. Until the card `add-new-and-edition` lands them, the lead allocates by hand, with `sha256sum` and a row in `sources.csv` for a source.
- Source numbering starts at `S-0001`. A source's first version is `v1`; each later version of the same source is one more.
- An id has one spelling everywhere. The version id `S-0042-v2` is the file stem of its original (`S-0042-v2.pdf`), the name of its extraction folder, the name of its site page (`s/S-0042-v2.html`) and the key in a citation (`[@S-0042-v2, p. 14]`).
- A register id is also the anchor of its row on the register page: `claims/CMP-1.html#CL-CMP-0011` (a claim is on the part page of its area and number: part = (number - 1) / 500 + 1), `sources.html#S-0042-v2`, `questions.html#Q-027`, `conflicts.html#XC-14`, `glossary.html#material_weakness`.

**Never reused, never renamed.**
- From the first edition that contains an id, the id keeps its meaning for ever. It is not reused for another object and not renamed. Before `edition-001` exists no reader holds an id, and the lead may correct one by hand.
- A row is never deleted. To retire one, keep it and point forward:

| Object | How it is retired |
|---|---|
| Page | `superseded_by` names the replacing page. The file stays. A page with no replacement stays too, and its Change note says why it no longer applies |
| Source version | `lifecycle` becomes `withdrawn`, or a later version's `supersedes` names it. The original stays |
| Claim | `superseded_by` names the replacing claim, and `adoption` becomes `superseded` |
| Term | `superseded_by` names the replacing term |
| Question | `status` becomes `withdrawn` |
| Conflict | The row stays, with its resolution or its question |

**Reports never enter the Corpus (D-6).** A research report, a chat answer or any other AI-generated text is not a source and has no `S-` id. A statement found in one is re-sourced to a primary document and enters as an unchecked claim that cites that document. The report is not cited.

## Registers

Five CSV registers live in `corpus/registers/`. Each has a fixed header: the columns below, in this order. A company store has the same five, with the same columns, in its own `registers\` folder.

| File | One row is | Key | Columns | Sort order |
|---|---|---|---|---|
| `sources.csv` | One version of one source | `source_id` and `version` | 28 | `source_id`, then `version` as a number |
| `claims.csv` | One claim | `claim_id` | 35 | The area, then the number as a number |
| `questions.csv` | One question for the company | `question_id` | 13 | `question_id` |
| `conflicts.csv` | One disagreement between statements | `conflict_id` | 11 | `conflict_id` as a number |
| `glossary.csv` | One term | `term_id` | 9 | `term_id` |

Other ids sort byte by byte, which is also numeric order because their digits have fixed width. Claim ids do not have a fixed width once a series passes 9999, so `CL-CMP-9999` sorts before `CL-CMP-10000`. Acronyms are glossary `aliases`; there is no separate acronym register.

**CSV rules, all five** (RFC 4180).
- UTF-8 without a byte-order mark, LF line endings, Unicode NFC. A header row read by name. Rows sorted as above.
- `check` is tolerant on read: it strips a byte-order mark, accepts CRLF and says "save as CSV UTF-8" for any other encoding (FB0202). `fmt` rewrites the file in normal form. Excel saves a BOM and CRLF, so run `fieldbook fmt` after an edit in Excel.
- For valid single-line Corpus register cells, a cell is quoted only when it holds a comma or a double quote; a double quote inside a cell is doubled. `fmt` applies this.
- A writable Corpus register cell is one line. A line break inside a cell is an error (FB0201); company-record cells follow their separate rules below.
- An empty cell is empty. Do not write `n/a`, `none`, `-` or `unknown`; each column below says what empty means.
- A **list** column holds values separated by `|`, with no spaces around it. A `|` is not allowed in any other cell of a register (FB0204). In a company-store record file a `|` in a text cell is literal.
- Dates are ISO 8601: `YYYY-MM-DD`. A **pdate** column also accepts `YYYY-MM` and `YYYY`, and the tool never fills in the missing parts. Booleans are `true` and `false`. Decimals follow the grammar `^-?(0|[1-9][0-9]*)(\.[0-9]+)?$` (FB0500).
- A text cell is at most 1,000 characters (FB0204). `quote` is at most 300. In `glossary.csv` a `definition` is at most 400 and a `boundary_rule` at most 300.
- A text cell of a register that people type does not start with `=`, `+`, `-`, `@`, TAB or CR (FB0403). A decimal such as `-5` is not a text cell and may start with `-`. A company-store record file is loaded from company exports, which hold such text and line breaks, so it is escaped and not refused ([the record file rules](company-store.md#typestoml-and-records)).
- Types used in the tables: `id` (a form in [Identifiers](#identifiers); a company store holds the company forms and the repository Corpus only the public ones), `enum` (a vocabulary in [Status vocabularies and promotion rules](#status-vocabularies-and-promotion-rules)), `text`, `int`, `date`, `pdate`, `decimal`, `bool`, `list`.
- A cell or pair of cells that breaks a rule in the "Rules across columns" tables is FB0206. A cell outside its type, vocabulary or length is FB0204. A required cell that is empty is FB0205. A reference to a row that does not exist is FB0207.

To add a column, follow "Adding a column" in [docs/cards/README.md](../cards/README.md#adding-a-column). A column added after the freeze is an amendment of ADR 0002 and needs the format gate.

### sources.csv

| # | Column | Type | Required | Values and form | Empty means |
|---|---|---|---|---|---|
| 1 | `source_id` | id | yes | `S-nnnn` | |
| 2 | `version` | int | yes | 1, 2, ... | |
| 3 | `tag` | text | no | A short label of at most 40 characters for people and search | No label |
| 4 | `title` | text | yes | The title as published | |
| 5 | `publisher` | text | yes | The body that published it | |
| 6 | `source_kind` | enum | yes | `source_kind` | |
| 7 | `source_role` | enum | yes | `source_role` | |
| 8 | `authority_tier` | enum | yes, except fixtures and company kinds | `A` to `D`; the kind limits the tiers | A synthetic fixture or a company kind has no tier |
| 9 | `licence` | text | yes | The terms of reuse in a phrase, such as "US Government work, public domain" | |
| 10 | `source_family` | text | no | A snake_case key shared by sources that come from one original | Unknown. A blank family never means independent |
| 11 | `family_basis` | enum | with a family | `family_basis` | No family |
| 12 | `location` | text | yes | Where the original comes from, as plain text. A version whose bytes are not in git starts with `metadata-only: ` | |
| 13 | `sha256` | text | yes | 64 lowercase hex characters: the SHA-256 of the original's bytes | |
| 14 | `published` | pdate | no | The date printed on the document | The document prints no date |
| 15 | `effective_from` | pdate | no | When the text takes effect | Not stated, or not applicable |
| 16 | `as_of` | pdate | yes | The date the data describes | |
| 17 | `retrieved` | date | yes | The day the original was read | |
| 18 | `lifecycle` | enum | yes | `lifecycle` | |
| 19 | `supersedes` | list | no | Source version ids this version replaces | Replaces nothing |
| 20 | `volatility` | enum | yes | `volatility` | |
| 21 | `handling` | list | yes | Label ids separated by `|`, sorted. `public` in the repository Corpus; in a company store the labels the policy lets the store hold (see [Handling and labels](#handling-and-labels)) | |
| 22 | `note` | text | no | Anything a reader needs: why a date is empty, why the version exists | |
| 23 | `purpose` | text | no | One line saying which open question or job the source answers. Added before the freeze, so every other column keeps its number | Unassigned |
| 24 | `origin_system` | text | no | The id of the company system the source was pulled from, as in `systems.toml` and the policy's `[[source]]` table. Written only by `sync` or by a person, never by a model or a rule | Added by hand, or not from a company system |
| 25 | `origin_stream` | text | with a system | The stream of that system | As `origin_system` |
| 26 | `origin_key` | text | with a system | The system's own key for the item, at most 200 bytes, as `sync` read it. It finds "the same item again", where `sha256` finds "the same file again" | As `origin_system` |
| 27 | `doc_kind` | enum | no | `doc_kind`: what the file is. Empty in the repository Corpus. In a company store `sync` sets it from the stream, or a person sets it at review; a classifier may suggest it | Not yet classified |
| 28 | `series` | text | no | A name of the form of a stream id (`[a-z][a-z0-9-]{0,31}`) that joins the sources of one vintage stream; in a vintage stream it is the stream id. Empty in the repository Corpus | Not part of a series |

Rules across columns:

| Rule | Check |
|---|---|
| `source_family` is set exactly when `family_basis` is set | FB0206 |
| `authority_tier` is empty exactly when `source_kind` is `synthetic_fixture` or a company kind, and otherwise one of the tiers the kind allows | FB0206, FB0204 |
| A `published`, `effective_from` or `as_of` that is empty for a reason the cell cannot show says why in `note` | Convention |
| Each id in `supersedes` is an existing version, not this one, and the chain has no loop | FB0207, FB0208 |
| `origin_system`, `origin_stream` and `origin_key` are set together, or all three are empty. A set `origin_system` names a system the store declares | FB0215 |
| For ordinary documents, each version of an `origin_key` after the first names the one it replaces in `supersedes`, and a revision stream’s `as_of` does not go back. Validated record-load observation sources retain empty `supersedes` and original dates; observation manifests select current evidence. Missing or malformed observation bindings do not qualify for this exception. Two sources of one stream never share an `origin_key`; in a vintage stream the key includes the `as_of` | FB0216 |
| `doc_kind` is empty or a word of the `doc_kind` vocabulary, `series` is empty or has the form of a stream id (`[a-z][a-z0-9-]{0,31}`), and both are empty in the repository Corpus | FB0215 |
| A stored original `corpus/sources/public/<version-id>.<ext>` (in a company store, `sources/org/<version-id>.<ext>`) exists and its bytes hash to `sha256`. A `metadata-only` version has no such file | FB0211 |
| A claim that is `primary_source_supported` rests on a version whose `lifecycle` is `active` | FB0213 |
| A `quarantined` version says why in `note` | Convention |

A version is **superseded** when another row's `supersedes` names it. That state is computed and never stored.

### claims.csv

| # | Column | Type | Required | Values and form | Empty means |
|---|---|---|---|---|---|
| 1 | `claim_id` | id | yes | `CL-AAA-nnnn`, or five digits from `10000` | |
| 2 | `area` | enum | yes | An area code. Equals the id's area letters in lowercase | |
| 3 | `kind` | enum | yes | `claim_kind` | |
| 4 | `statement` | text | yes | One plain sentence a page can cite. No markup | |
| 5 | `as_of` | pdate | unless stale with unknown date | The date the data describes | Unknown only when `verification` is `stale` and `note` begins `as_of unknown` |
| 6 | `source_id` | id | by kind | The source the claim rests on. In a company store, a record id (`CM-nnnnnn`) when `locator_kind` is `record_field`, and `version` is then the record's version | An assumption has no source |
| 7 | `version` | int | by kind | The version of that source | As `source_id` |
| 8 | `locator_kind` | enum | by kind | `locator_kind` | As `source_id` |
| 9 | `locator` | text | with a fine kind | As written in a citation, prefix included: `p. 14`, `pp. 14-15`, `sec. (e)(1)`, `§ 15.404-1`, `para. 3`, `sheet Rates!B2:D9`, `rec 48213`, and in a company store `field burden_eng` | `no_fine_locator` |
| 10 | `quote` | text | if consequential | Verbatim from the original at the locator, at most 300 characters | No quote |
| 11 | `consequential` | bool | yes | `true` when a wrong value could change a decision, a price, a filing or a compliance position: thresholds, rates, dates, clause numbers, amounts, and everything on a brief | |
| 12 | `drafted_by` | enum | yes | `drafted_by` | |
| 13 | `verification` | enum | yes | `verification` | |
| 14 | `applicability` | enum | yes | `applicability` | |
| 15 | `adoption` | enum | yes | `adoption` | |
| 16 | `data_origin` | enum | yes | `data_origin` | |
| 17 | `volatility` | enum | yes | `volatility` | |
| 18 | `value` | decimal | no | The number the claim states, in the decimal grammar | The claim states no number |
| 19 | `unit` | enum | with a value | `unit` | |
| 20 | `effective_from` | pdate | no | When the fact or rule starts to be true in the world | Not stated |
| 21 | `effective_to` | pdate | no | When it stops | Still in force at `as_of` |
| 22 | `instrument` | enum | no | `instrument` | Not a regulatory claim |
| 23 | `citation_codified` | text | promotion rule | The codified number, notice citation or statute section | Allowed only by the instrument table below |
| 24 | `citation_rfo` | text | cross-column or promotion rule | The model-text, deviation or proposed-rule identifier | Allowed only by the instrument table below |
| 25 | `edition_date` | pdate | no | The clause or provision date printed on the text, as `YYYY-MM` | None printed |
| 26 | `text_status` | enum | no | `text_status` | Not regulatory |
| 27 | `applies_to` | text | no | The contract date and type test. Either prose, such as "fixed-price DoD contracts entered after 2025-01-01", or a structured cell of pairs `key=value` separated by a semicolon and one space, such as `event=award_date; entered_from=2025-01-01; contract_type=fixed_price`. The keys are `event`, `entered_from`, `entered_to`, `contract_type`, `fiscal_year`, `calendar` and `basis`, and each appears once. `event` is the date the fact is keyed on, one of `award_date`, `solicitation_date`, `clause_date`, `cdrl_date`, `query_date` or `event_date`. `entered_from` and `entered_to` are pdates. Any other value is one word of lowercase letters, digits and underscores. A cell with no `=` is prose and is not read. A cell with `=` that breaks this form is FB0204 | No such test |
| 28 | `evidence_grade` | enum | for assumptions | `evidence_grade` | The claim is not an input to a calculation |
| 29 | `verified_on` | date | when verified | The day a person checked the claim against the original | Not checked |
| 30 | `verified_by_role` | enum | when verified | `verified_by_role` | Not checked |
| 31 | `supersedes` | list | no | Claim ids this claim replaces | Replaces nothing |
| 32 | `superseded_by` | list | no | Claim ids that replace this claim | Not replaced |
| 33 | `conflict_id` | id | no | The `XC-nn` conflict this claim is part of | In no conflict |
| 34 | `question_id` | id | no | The `Q-nnn` question that settles it | No question |
| 35 | `note` | text | no | Anything a reader needs | |

Rules across columns (FB0206 unless another code is named):

| If | Then |
|---|---|
| `kind` is `fact` or `interpretation` | `source_id`, `version` and `locator_kind` are set (FB0206); the version exists (FB0207) |
| `kind` is `assumption` | `source_id`, `version`, `locator_kind` and `locator` are empty, `evidence_grade` is set and `verification` is `unchecked` |
| `locator_kind` is `no_fine_locator` | `locator` is empty (FB0206); `consequential` is `false` (FB0306) |
| `locator_kind` is another kind | `locator` is set and begins with that kind's prefix (`p.`/`pp.`, `sec.`/`§`/`para.`, `sheet`, `rec`, `field`) |
| `locator_kind` is `record_field` | `source_id` is a record id and `version` is that record's version (FB0207), and the claim is in a company store (FB0210) |
| `evidence_grade` is `G` | `note` starts with the literal `replace by ` and has non-whitespace date or event text after it (FB0206) |
| `consequential` is `true` | `quote` is set (FB0307) |
| `value` is set | `unit` is set. A `unit` without a `value` is also an error |
| `verification` is `unchecked` | `verified_on` and `verified_by_role` are empty |
| `verification` is `primary_source_supported` or `contradicted` | `verified_on` and `verified_by_role` are set |
| `verification` is `unresolved` | `question_id` is set |
| `verification` is `contradicted` | `conflict_id` is set |
| `verification` is `stale` | `superseded_by` or `conflict_id` is set, or `note` begins `review passed` or `as_of unknown` |
| `as_of` is empty | `verification` is `stale` and `note` begins `as_of unknown` |
| `instrument` is `class_deviation`, `rfo_model_text` or `proposed_rule` | `citation_rfo`, `effective_from` and `text_status` are set (dual numbering) |
| `text_status` is set | `instrument` is set |
| `superseded_by` is set | `adoption` is `superseded` (FB0206); each named claim lists this one in `supersedes` (FB0208) |
| `conflict_id` is set | The conflict's `claim_ids` lists this claim (FB0207) |

The promotion rules, which decide who may set `verification`, `applicability`, `adoption`, `verified_on` and `verified_by_role`, are in [Status vocabularies and promotion rules](#status-vocabularies-and-promotion-rules) (FB0209).

### questions.csv

A question is an ask for the company. The Corpus holds the asks; answers are company data and live in the company store, so the answer columns stay empty in the repository Corpus. A quick capture (`new question --confirm-public --area <a> "<text>"`) fills only the required cells.

| # | Column | Type | Required | Values and form | Empty means |
|---|---|---|---|---|---|
| 1 | `question_id` | id | yes | `Q-nnn` | |
| 2 | `area` | enum | yes | An area code | |
| 3 | `question` | text | yes | The ask in company-neutral words | |
| 4 | `owner_role` | text | strict, day-one | A role title, never a person's name | Not yet named |
| 5 | `blocks` | text | strict, day-one | What stays blocked until it is answered | Not yet written |
| 6 | `why_it_matters` | text | strict, day-one | One sentence | Not yet written |
| 7 | `default_assumption` | text | strict, day-one | What the Corpus assumes until the answer arrives | Not yet written |
| 8 | `work_can_continue` | enum | strict, day-one | `work_can_continue` | Not yet decided |
| 9 | `day_one` | bool | yes | `true` for the questions to ask in the first days | |
| 10 | `status` | enum | yes | `question_status`: `open` or `withdrawn` in the Corpus | |
| 11 | `answer` | text | no | Empty in the Corpus | No answer held here |
| 12 | `answered_on` | date | no | Empty in the Corpus | No answer held here |
| 13 | `adoption` | enum | no | Empty in the Corpus | No answer held here |

Rules across columns: a row with `day_one` set to `true` has cells 4 to 8 filled (FB0230, strict). A value in `answer`, `answered_on` or `adoption`, or a `status` of `answered`, is a company-store-only value, refused in the repository Corpus (FB0210).

### conflicts.csv

| # | Column | Type | Required | Values and form | Empty means |
|---|---|---|---|---|---|
| 1 | `conflict_id` | id | yes | `XC-nn` | |
| 2 | `area` | enum | yes | An area code | |
| 3 | `topic` | text | yes | A short title | |
| 4 | `positions` | list | yes | Two or more competing statements, separated by a vertical bar | |
| 5 | `resolution` | text | when resolved | What the Corpus uses | Unresolved |
| 6 | `rationale` | text | when resolved | The rule or reason that decides it | Unresolved |
| 7 | `evidence` | list | when resolved | Claim ids and source version ids that support the resolution | Unresolved |
| 8 | `resolved_on` | date | when resolved | The day it was resolved | Unresolved |
| 9 | `status` | enum | yes | `conflict_status` | |
| 10 | `claim_ids` | list | yes | The claims in dispute | |
| 11 | `question_id` | id | when unresolved | The question that settles it | Resolved |

Rules across columns: a `resolved` row has cells 5 to 8 filled and an `unresolved` row has `question_id` filled (FB0206). Each id in `claim_ids` and `evidence` exists (FB0207). Resolve a conflict by the first of four rules that decides it: (1) a claim checked against primary text beats one that was not; (2) a correction from primary text beats a secondary summary; (3) where two checks disagree, the better-sourced one wins and `rationale` names the other; (4) where no source settles it, the status is `unresolved` and a question is raised instead of a guess.

### glossary.csv

| # | Column | Type | Required | Values and form | Empty means |
|---|---|---|---|---|---|
| 1 | `term_id` | id | yes | snake_case | |
| 2 | `label` | text | yes | The words as a reader writes them: `material weakness` | |
| 3 | `definition` | text | yes | One sentence, at most 400 characters | |
| 4 | `boundary_rule` | text | yes | What the term is not, or the neighbour it must not be added to. At most 300 characters | |
| 5 | `owner_area` | enum | yes | One area code. The owner writes and changes the definition | |
| 6 | `source_ref` | list | for public references | Claim ids and source version ids behind the definition | No source row |
| 7 | `definition_status` | enum | yes | `definition_status` | |
| 8 | `aliases` | list | no | Synonyms and acronyms. They feed search and the first-occurrence link | None |
| 9 | `superseded_by` | id | no | The term that replaces this one | Not replaced |

Rules across columns: a `public_reference` row has `source_ref` filled (FB0206). A label or alias is unique across the whole glossary (FB0214). A page never restates a definition: it lists the term in `terms`. Add a term only when it is used in two or more areas, is ambiguous between areas, or is regulated.

## Excel copies of the registers

The Corpus sources of truth are the CSV files in `corpus/registers/`. `build` writes copies for Excel into the edition. The tool never reads these copies back. A workbook that is a source, and not one of these copies, is read by the `xlsx-sheets` method ([Extraction](#extraction)); a system that exports only CSV goes through `csv-rows` (save as CSV UTF-8, then `file-drop@1` or `add`). Macros are never read or run.

| File | When | What it is |
|---|---|---|
| `data/<register>.csv`, `data/claims-<AREA>-<part>.csv` | Day 1 | Plain CSV exports for Excel: `data/sources.csv`, `data/questions.csv`, `data/conflicts.csv` and `data/glossary.csv` (one file each), and one `data/claims-<AREA>-<part>.csv` per claims part (`claims/<AREA>-<part>.html`) in place of `data/claims.csv` |
| `data/registers.xlsx` | Day 1 | One workbook with one sheet per register |

**The CSV copies.** The claims copy is one file per claims part (`data/claims-<AREA>-<part>.csv`), in the same groups and order as `claims/<AREA>-<part>.html`; the other four registers are one file each. Same columns, same rows, same order as the source (for claims, across the part files taken together in order). A byte-order mark, CRLF line endings, and a leading apostrophe on any text cell that starts with `=`, `+`, `-`, `@`, TAB or CR. A decimal is not a text cell and gets no apostrophe. Because `check` refuses such text cells in the source, the apostrophe is a second guard for hand-edited copies. If Excel mangles a copy, use Data > From Text/CSV.

In the edition, both copies hold only the rows the edition keeps. A kept row's cell that names a record the edition leaves out (a claim in `supersedes`, a source version in `evidence`, a question in `question_id`, and so on) is written without that reference, and the copies carry no count or note of what was left out.

**`registers.xlsx`.** Sheets are named `sources`, `claims`, `questions`, `conflicts` and `glossary`.
- Every cell is a string, including dates, numbers and booleans. There are no formulas, no macros and no links.
- The header row is frozen and has an autofilter.
- A workbook cell holds at most 32,767 characters. A register cell longer than that, such as a long joined list, stops the build with FB0605; the message names the register, the row id and the column. `check` limits each text cell (FB0204) and each item of a text list, but not the joined length of a list cell, so a strict-clean Corpus can reach this.
- The file carries a fixed creation time taken from `SOURCE_DATE_EPOCH`, so two builds of the same commit give the same bytes. A test reads `docProps/core.xml` back.
- The workbook is read-only by design: to change a register, edit the CSV and run `fieldbook check`.

The data-only templates (`templates/<name>.xlsx`, E1) and the decision package (Day 30) are in the [Contract for the reader UI](#contract-for-the-reader-ui).

Without SOURCE_DATE_EPOCH, workbook creation time is the fixed synthetic instant `2000-01-01T00:00:00Z`. The binary passes the resolved instant into the pure renderer. This fallback never fills edition release metadata.

## Status vocabularies and promotion rules

Each vocabulary is defined here once. A value marked "company store" is reachable only in a company store, never in the repository Corpus; the Corpus refuses it (FB0210). To add a vocabulary value, follow "Adding a vocabulary value" in [docs/cards/README.md](../cards/README.md#adding-a-vocabulary-value).

### Four grading fields and four state fields

The grading fields answer different questions and are never collapsed into one.

| Axis | About | Question | Field | Register |
|---|---|---|---|---|
| Source | A source version | Who published it, in what role, with what default weight? | `source_kind`, `source_role`, `authority_tier`, `licence`, `source_family` | `sources.csv` |
| Support | An input to a calculation | How good is the evidence for this input, for this purpose? | `evidence_grade` | `claims.csv` |
| Verification | A claim | Was it checked against a primary source, and with what result? | `verification`, `verified_on`, `verified_by_role` | `claims.csv` |
| Origin | A value | How did the value come to exist? | `data_origin` | `claims.csv` |

The state fields sit beside them and answer other questions: `applicability` (does it apply to the company), `adoption` (is it approved), `kind` (what sort of statement), `lifecycle` (can the source be relied on).

- A letter in one field is never compared with a letter in another. Authority tier `A` to `D` and evidence grade `A` to `G` are unrelated scales.
- A high tier does not make a claim true, applicable or approved. A primary source can be wrong, dated or inapplicable.
- The order of sources breaks a tie between conflicting claims. It is never a score.
- Licence and handling belong to the source and are separate from its weight.
- Categorical values are primary. A weighted score is never the only grade.
- Two sources that agree corroborate each other only when both have a `source_family`, the families differ, and both rows have `family_basis` set to `reviewed`. A blank family means unknown, never independent. A single source, however authoritative, supports a narrow claim and is described as single-source. Prose does not say "independently corroborated" (FB0402): it names the sources. Nothing in v1 counts agreement; the first feature that does must use this rule and carry a golden.

### Page vocabularies

| Field | Value | Meaning |
|---|---|---|
| `readiness` | `PUBLIC-NOW` | Written from public sources and usable before any company answer |
| | `SYNTHETIC-NOW` | Needs made-up, labelled example data to be useful; uses no company data |
| | `MAP-ONBOARDING` | Maps what the owner will learn at onboarding; depends on company answers, which the page lists as questions |
| | `POLICY-GATED` | Depends on a company policy that is not public: approvals, limits, thresholds |
| | `CONTRACT-SPECIFIC` | Depends on the terms of a contract the company holds, such as the clauses it incorporates |
| page `status` | `draft` | Being written; may hold open items |
| | `reviewed` | A person other than the author read it against its claims. The lead or the owner sets it |
| | `stable` | The owner accepted it. A later change adds a Change note entry |
| `handling` | label ids | The labels of a page, a source version or a record at creation. A company label id matches `[a-z]{2,4}-[0-9]{4}` (an opaque code). The repository Corpus accepts only `public` (FB0410) |
| | `public`, `unlabelled`, `marked`, `owner-supplied` | The four built-in ids. A company store adds the ids of its policy's label table. See [Handling and labels](#handling-and-labels) |
| `volatility` | `V1` | Volatile: thresholds, clause numbers, feed status, prices, benchmarks. Review by a named trigger or every 90 days |
| | `V2` | Periodic: peer financials, annual indexes. Review when the next filing or release appears, and at most every 180 days |
| | `V3` | Stable: definitions, arithmetic, standards text. Review every 365 days |

| `stage` | Name |
|---|---|
| 1 | Vocabulary and evidence rules |
| 2 | The contract world |
| 3 | Cost structure |
| 4 | Program control |
| 5 | Cash and billing |
| 6 | Accounting |
| 7 | Planning and governance |
| 8 | Capital and growth |

| Change class | Used when |
|---|---|
| `NEW_KNOWLEDGE` | A source or fact with no earlier row. New rows enter `unchecked` |
| `SOURCE_REFRESH` | The same source in a new version. The new version row is added, its claims are re-checked and the old version stays |
| `CLAIM_CONFLICT` | Two sources or rows disagree. A conflict row is added |
| `COMPATIBLE_EXTENSION` | Detail is added and no existing claim changes. No re-verification of the old row |
| `MATERIAL_CONTRACT_CHANGE` | Company store only: a company contract term or clause changes a calculation. Affected snapshots are re-run into new ones |

### Claim vocabularies

| Field | Value | Meaning |
|---|---|---|
| `claim_kind` | `fact` | A statement a source supports: a rule, a number, a date |
| | `assumption` | A value the author chose, such as a placeholder rate in a worked example. It has no source |
| | `interpretation` | How a source's text is read, for example agency guidance |
| | `recommendation` | What to do |
| `verification` | `unchecked` | Entry value. No one has checked the claim against a primary source |
| | `primary_source_supported` | A person read the original at the locator and it supports the claim |
| | `contradicted` | Primary text for the claim's own scope and date conflicts with it. It always has a conflict row |
| | `stale` | The claim was right for its date and the instrument has since changed, or its review date passed with no re-check |
| | `unresolved` | Two primary sources disagree, or none can be reached. It always has a question |
| `applicability` | `general_reference` | Holds in general and makes no claim about the company: definitions and public methods |
| | `prototype_assumption` | A stand-in used in an example until a company value arrives |
| | `pending_company_confirmation` | Entry value for facts. It may apply to the company; only the company can confirm |
| | `confirmed_applicable` | A named role at the company confirmed it. Company store only |
| | `not_applicable` | Known not to apply: replaced text, or out of scope |
| `adoption` | `proposed` | Entry value. Offered as the method or number to use |
| | `prototype_reviewed` | The lead or the owner reviewed it for use in prototypes and examples |
| | `pending_company_approval` | Waiting for the company to approve it. Company store only |
| | `company_approved` | The company approved it, with an approval record. Company store only |
| | `rejected` | Considered and not adopted |
| | `superseded` | Replaced by another claim |
| `data_origin` | `observed` | Read directly from a source |
| | `estimated` | Computed or judged by the author from other numbers |
| | `inferred` | Concluded from facts that do not state it |
| | `unknown` | Entry value. The origin is not established |
| `drafted_by` | `owner`, `lead`, `codex` | The role that wrote the row |
| | `ai` | Text produced by an assistant outside the repository and pasted in (Day 30) |
| `verified_by_role` | `owner`, `lead` | The role that checked the claim. Never a person's name |
| `locator_kind` | `pdf_page_box`, `text_section`, `sheet_cells`, `record_key`, `no_fine_locator`, and in a company store `record_field`; extensions use `ext:<kind>` | Written by the prefixes in [Syntax at a glance](#syntax-at-a-glance) |
| `text_status` | `final` | The text in force |
| | `deviation_text` | Class-deviation or model-deviation text that differs from the codified text |
| | `proposed` | A proposed rule; not final |
| `instrument` | `far_codified`, `dfars_codified`, `cas` | Codified FAR, codified DFARS, CAS (48 CFR 99xx) |
| | `rfo_model_text` | Revolutionary FAR Overhaul model deviation text |
| | `class_deviation` | A DoD class deviation, with its number in `citation_rfo` |
| | `proposed_rule` | A proposed rule, with the FAR case or docket in `citation_rfo` |
| | `federal_register_notice`, `statute` | A notice, or a statute |
| `unit` | A currency code of three capital letters from ISO 4217, such as `USD`; or `percent`, `basis_points`, `days`, `business_days`, `weeks`, `months`, `years`, `hours`, `count`, `ratio`, `each`, `per_100`, `per_1000` or `per_dozen` | The unit of `value`. The words other than currencies are the variants of `Unit` in `fieldbook-calc`, and a conversion exists only inside one basis family. The test `unit_words_match_the_reference` compares them with this cell and with the time words of the number-unit lint (FB0309). `fieldbook-calc` takes `USD` until the first non-USD source brings exchange rates. ISO 4217 remains the intended currency meaning; automated format validation checks three ASCII capital letters, without checking ISO-list membership |
| `evidence_grade` | `E` | An official public benchmark |
| | `F` | An industry secondary source |
| | `G` | Management judgment. `note` begins with the literal `replace by ` followed by non-whitespace date or event text |
| | `A`, `B`, `C`, `D` | Company store: company actual; company committed; company estimate, unapproved; parent or analogous operation, adjusted |

A calculation shows the lowest grade among its inputs. Until onboarding every rate in the Corpus is `E`, `F` or `G`. A better value does not edit the old one: it arrives as a new claim that supersedes it.

### Source vocabularies

| `source_kind` | Examples | Tiers allowed | Use rule |
|---|---|---|---|
| `primary_law` | Statutes, FAR, DFARS and CAS text, Federal Register notices, class-deviation text, eCFR | A, D | Operative text for an instrument and a date |
| `official_data` | Federal award and entity records, contract-award APIs, trade and price statistics | A, D | Evidence of what was reported, with coverage and lag |
| `company_filing` | Audited annual reports and securities filings of peers and the parent | A, D | What the filer said; never another company's practice |
| `official_announcement` | Army and DoD releases, daily contract announcements | A, D | An alert with a threshold, not a complete history |
| `standard_or_guidance` | NIST publications, audit-agency manuals, accounting standards | A, B, D | Authority on method; check the licence before storing an excerpt |
| `company_statement` | Peer or parent press releases, investor decks, unaudited releases | B, C, D | What was said; flag unaudited and alternative measures |
| `licensed_database` | Commercial defense and trade databases | B, D | The finding and a locator, not the data, unless the licence allows |
| `strategic_dataset` | Arms-transfer data, customs and trade flows | C, D | Evidence of flows, not of prices |
| `trade_press` | News and trade press | C, D | Corroboration only; never the sole support for a number |
| `vendor_documentation` | ERP vendor manuals and release notes | B, D | Behaviour of a named release; the customer's tenant may differ |
| `synthetic_fixture` | Made-up test data | none | Never evidence for a real claim |
| `vendor_quote` | A supplier's quotation or proposal letter as received, with its validity period | none | What the supplier offered, for a date. Company store only |
| `company_record` | A record or an export from a company system: a ledger, a rate table, a schedule, an order, a time report | none | What the system held at its as-of date. Company store only |
| `company_document` | A company policy, contract, specification, memo or procedure | none | What the document says. Company store only |

Tier D is discovery-only for the public kinds above; it never supplies primary support. Synthetic and company kinds stay untiered.

`ai_generated_report` and `internal_record` are not values of `source_kind`. A report never enters the Corpus (see [Identifiers](#identifiers)), and a record from a company system is a `company_record`, which lives only in a company store.

| Field | Value | Meaning |
|---|---|---|
| `source_role` | `primary_evidence` | The original text of the thing it describes |
| | `accepted_interpretation` | A reading by a body whose reading is accepted, such as agency guidance |
| | `synthetic_fixture` | Made-up data for tests and worked examples |
| | `unknown` | Not yet reviewed |
| | `generated_synthesis`, `working_draft` | Not used in the Corpus: they describe reports, which never enter |
| `authority_tier` | `A` | Official: law, official data, official announcements, filings |
| | `B` | Manufacturer, licensed or standards body |
| | `C` | Strategic or trade-flow dataset, trade press |
| | `D` | Discovery only. Left out of context packs unless named |
| | (empty) | A `synthetic_fixture` or a company kind has no tier, because the scale ranks public publishers. For a company kind the original is the evidence |
| `family_basis` | `unknown`, `inferred`, `reviewed` | How sure we are that sources share one original. Only `reviewed` counts towards corroboration |
| `lifecycle` | `active` | May be relied on |
| | `quarantined` | On hold. Only its row ships, with the hash and the reason in `note`: no original, extracted text, `s/` page, search unit or context-pack block. A quarantined version cannot support a verified claim (FB0213). A hold is per version row: to hold a whole source, hold every version. Moving an existing source version’s `lifecycle` back to `active` is prohibited by `AGENTS.md`; `guard-statuses` does not enforce it |
| | `withdrawn` | The publisher or the owner withdrew it. Kept for the record |
| `doc_kind` (company store only) | `contract`, `modification` | A contract or task order, and a change to one |
| | `quote` | A supplier's quotation or proposal letter |
| | `rate_package` | A set of rates for a period, with their bases |
| | `eac_package`, `evms_export` | An estimate-at-completion package for a project, and an earned-value export |
| | `baseline_log` | A baseline or a log of changes to one |
| | `schedule_file` | A project schedule |
| | `billing_export`, `receivables_export` | A billing or invoice export, and a receivables export |
| | `other` | Classified, and none of the words above. An empty cell means not yet classified. The words are the const `DOC_KINDS`; adding one is a vocabulary change with a known-bad case |

### Other registers

| Field | Value | Meaning |
|---|---|---|
| `question_status` | `open` | Asked and not answered |
| | `withdrawn` | No longer asked |
| | `answered` | Company store only |
| `work_can_continue` | `yes`, `partly`, `no` | Whether work can proceed on the default assumption |
| `conflict_status` | `resolved`, `unresolved` | Whether a rule or a source settles it |
| `definition_status` | `public_reference` | A regulatory or standards term, with a citation in `source_ref` |
| | `general_practice` | Accepted finance or engineering usage |
| | `design_convention` | A modelling rule this project chose; a decision can change it |
| | `pending_company_calibration` | The company must confirm or replace it |

### Promotion rules

**Entry values.** A new claim enters with `verification` set to `unchecked`, `applicability` to `pending_company_confirmation`, `adoption` to `proposed` and `data_origin` to `unknown`, whatever the source says about itself. `verified_on` and `verified_by_role` are empty. A claim is never pre-labelled supported.

**Reaching `primary_source_supported`.** All of these hold (FB0209):
1. A source-version claim requires `source_role = primary_evidence`, and either `authority_tier` is `A` or `B` or `source_kind` is a company kind (`vendor_quote`, `company_record` or `company_document`: no tier, company store only). A record-field claim instead meets the record evidence branch below; source authority fields do not exist on a record.
2. The claim has an available fine locator. A consequential claim also has a `quote` that matches. An unsupported extension locator is unavailable.
3. `as_of` is set. A regulatory claim (one with an `instrument`) also has `effective_from` and the numbering cells in the table below.
4. A number has a `value` and a `unit`: a claim with either one has both.
5. A person has read the original at the locator: `verified_on` and `verified_by_role` are set.
6. The cited source or record version's `lifecycle` is `active` (FB0213). Supersession remains FB0222; stale computed inputs remain FB0382, not a silent lifecycle change.

**Record evidence (R2).** The claim cites one existing, nonempty field with `record_field`, and its value/unit, when present, agree with that field. Each numeric field must pass the immutable per-field evidence check in [Financial work](financial-work.md#evidence-and-calculation-runs): Quoted has a resolved, matching quotation or cell; Computed has a resolvable matching stored run and its inputs; Judgment has a nonempty rationale. A populated cell, row-level basis or rationale alone does not establish an observed fact. Computed and Judgment claims require `data_origin = estimated`. Support for Computed establishes that the named method and inputs yield the recorded result; support for Judgment establishes the documented choice. Neither establishes that an estimate will occur, that its premises are correct or that the company approved it. The human verifier checks that the statement carries that limited meaning; the program checks origin, evidence and matching value without claiming to judge prose. A non-numeric field supports only the text stored in the record. Conditions 2 to 5, consequential-claim role limits and the active-version requirement apply to both branches. A missing, mismatched or unavailable evidence chain refuses promotion.

`r2-records` extends the shared promotion eligibility function consumed by both `check` and `verify::plan`; neither may substitute its own record rule.

**Numbering at promotion.** The required cell is nonempty for `primary_source_supported` (FB0209). The other citation cell may be empty. These are evidence-identification rules, not a claim that one instrument governs another.

| Instrument | Required citation cell | Cell that may be empty |
|---|---|---|
| `far_codified` | `citation_codified` | `citation_rfo` |
| `dfars_codified` | `citation_codified` | `citation_rfo` |
| `cas` | `citation_codified` | `citation_rfo` |
| `federal_register_notice` | `citation_codified` | `citation_rfo` |
| `statute` | `citation_codified` | `citation_rfo` |
| `rfo_model_text` | `citation_rfo` | `citation_codified` |
| `class_deviation` | `citation_rfo` | `citation_codified` |
| `proposed_rule` | `citation_rfo` | `citation_codified` |

With no instrument, both citation cells may be empty. The cross-column FB0206 rule still requires `citation_rfo`, `effective_from` and `text_status` for the last three instruments at every verification state; promotion does not replace that rule.

**Other results.**
- `contradicted` needs primary text that conflicts with the claim for the same scope and date. It always creates a conflict row.
- `stale` means the claim was right for its date and the instrument has since changed, or its review date passed with no re-check. When the claim's own `as_of` is unknown, use `stale` and write `as_of unknown` in `note`.
- `unresolved` is the honest state when two primary sources disagree or none can be reached. It always carries a question id.
- Editing the `statement`, `value`, `unit`, `source_id`, `version`, `locator_kind`, `locator` or `quote` of a claim that is not `unchecked` voids the check, whoever the author is. The same commit sets `verification` back to `unchecked` and clears `verified_on` and `verified_by_role`, and the claim is verified again. This reset obligation applies to every author. Automated enforcement is narrower: `guard-statuses` rejects an agent-authored evidence edit whose resulting `verification` is not `unchecked`; it does not inspect non-agent commits.

**Applicability and adoption.** The three fields `verification`, `applicability` and `adoption` are independent. A claim can be supported, pending company confirmation and proposed at once. Supported is not applicable. `confirmed_applicable` and `company_approved` are reached only in a company store, by an answer from a named role at the company or by an approval record. Neither can be reached by editing a page.

**Who may do what.**

| Role | May | May not |
|---|---|---|
| Owner | Verify any claim. Only the owner verifies a consequential claim (`verified_by_role = owner`). Accept a page as `stable`. Settle a conflict. Accept or reject a proposal at a terminal (`queue accept`, `queue reject`) | Write a company value into the repository Corpus |
| Lead | Write content. Verify a claim that is not consequential (`verified_by_role = lead`). Set a page to `draft` or `reviewed`. Set `applicability` to `general_reference`, `prototype_assumption` or `not_applicable`, and `adoption` to `prototype_reviewed`, `rejected` or `superseded`. Propose sources with `add --propose`, and accept or reject a proposal at a terminal | Verify a consequential claim. Write a company value into the repository Corpus |
| Agent (Codex, or an assistant) | Add public sources and their originals, by hand or with `add` and `add --propose`. Draft claims, quotes, questions and pages at entry values, with `drafted_by` naming the drafting role. Accept or reject a proposal of public material into the repository Corpus. Append a regression case. Add a new seed golden in a pull request of its own. Upkeep: formatting, links, typos | Raise a status. Set `verified_on` or `verified_by_role`. Set a page to `stable`. Rewrite or delete an original. Accept into a company store, or run `label add`, `restore` or a pruning `backup`, or edit the journal or `.fieldbook/`. Change an existing seed golden or regression case, or mark a case `critical` |

A **raise** is setting `verification`, `applicability` or `adoption` to a value other than its entry value, or any change to `verified_on` or `verified_by_role`. Setting a field back to its entry value is not a raise. Moving an existing source version’s `lifecycle` back to `active` is prohibited by `AGENTS.md`; `guard-statuses` does not enforce it. `guard-statuses` treats a commit as an agent's when its author name ends in `(agent)` or it carries a `Co-authored-by` trailer (GS-02). A consequential claim verified by anyone but the owner is an error (FB0209).

**Actor.** Every action also has an actor: the operating-system principal that ran it (`os:DOMAIN\user`, or `local` in tests), the role above, and the way it came in (`cli`, `chat` or `mcp`). The audit journal records all three on every event. The registers still record a role and never a person: `verified_by_role` and `drafted_by` stay roles. The three roles are the only ones in version 1. More than one holder of a role, a role table and approval bands wait for the Later rows `later-accounts` and `later-roles`. The `Authz` port may narrow what an actor sees or does (FB0807); it never widens a role.

## Sources

A source is a document, a data file or an export. In the repository Corpus every source is a public document. A company store holds company sources too: vendor quotes, records from company systems and company documents (see [Source vocabularies](#source-vocabularies)). Each version of a source is kept as the bytes the publisher or the system released, with a SHA-256 and one row in `sources.csv`. Reports never enter the Corpus (see [Identifiers](#identifiers)).

**Suggestions from the add pipeline.** The review queue may suggest `title`, `publisher`, `source_kind`, `authority_tier` (within FB0204), `licence`, `published`, `effective_from`, `volatility`, `tag` and `purpose`, and in a company store `doc_kind` (`SuggestField::DocKind`). It never suggests `source_role` (it stays `unknown`), `source_family`, `family_basis`, `as_of`, `lifecycle`, `handling`, `origin_system`, `origin_stream`, `origin_key` or `series`. A source whose `doc_kind` was wrong is moved by a `reclassify` proposal, which changes `doc_kind` and `series` of a version and nothing else. Each suggested value is shown with the quote or id it rests on, and a person may change any value before accepting. A source enters the Corpus only through an interactive accept, with the entry values of this reference.

### Originals

- **Where.** `corpus/sources/public/<version-id>.<ext>`, such as `S-0042-v2.pdf`. The file name is the version id and a lowercase extension. Git stores it as binary.
- **Public only in the Corpus.** In the repository Corpus an original is a public document. Nothing from inside the company, and nothing whose licence forbids keeping a copy, enters `sources/public/`. A company store keeps its originals in `sources/org/<version-id>.<ext>`, named and hashed the same way (FB0211), labelled by the version's `handling`, under the same rules for size and licence. Vendor quotes, material under a non-disclosure agreement, specifications and contracts are held there as received.
- **Byte-exact.** No tool rewrites an original: `fmt`, `build` and extraction only read it. A changed byte, a missing file or a file with no row is an error (FB0211).
- **Size.** An original of up to 25 MB is stored in git. A larger one, or one the licence does not let us keep, is recorded without its bytes: the row holds the hash and the location, and `location` starts `metadata-only: `. Claims on it carry a locator and a quote, and `check` cannot match the quote. A consequential claim on such a version raises a warning (FB0223), so that no one mistakes a stored quote for a checked one.
- **Licence.** `licence` states the terms in a phrase. A claim quotes at most 300 characters of an original. A licensed database is cited by the finding and its locator and is not copied.
- **Check scope.** The original-file checks run where `corpus/sources/public/` exists. An edition's copy of `corpus/` has no originals (they travel in the originals zip), and `check` there skips these checks and says so.
- **Held versions.** A `quarantined` version ships its register row only, with the hash and the reason in `note`. Its original, extracted text, `s/` page, search unit and context-pack block are left out, no page links to it, and a citation of it is plain text. The edition allowlist fails an edition that holds one of them. `withdrawn` is not settled yet: a withdrawn version keeps its text and shows a chip until the owner decides whether it ships as a row only too.

| Situation | Check |
|---|---|
| A row without a `metadata-only: ` location has no file at `sources/public/<version-id>.<ext>` | FB0211 |
| A file in `sources/public/` has no row | FB0211 |
| A file's bytes do not hash to the row's `sha256` | FB0211 |
| A `metadata-only` row has a file | FB0211 |

### Adding a version

| Step | What happens | Who |
|---|---|---|
| 1 | `fieldbook add <file> --confirm-public` copies the file to `sources/public/`, hashes it and appends a row for a new source (the next `S-nnnn`, version 1). `add <file> --source S-0042 --confirm-public` appends the next version of that source. `--metadata-only` hashes the file, does not copy it and prefixes `location` | Lead, at home |
| 2 | The lead completes the row and replaces the file name in `location` with the address the original came from | Lead |
| 3 | `cargo xtask extract <version-id>` writes the extraction and the lead adds the locators | Lead, at home |
| 4 | `fieldbook check` | Lead |

- `add` fills the columns it can know: `source_id`, `version`, `sha256`, `retrieved` (the day it runs), `lifecycle` (`active`), `handling` (`public` in the Corpus, only when the person gives `--confirm-public`: without the flag the file is `unlabelled` and `add` stops with FB0410 and writes nothing, and marked text is refused with FB0400 with or without the flag; in a company store, the labeller's result) and `location` (the file name). The row is not valid until the lead fills the required cells that remain (FB0205).
- `add` is idempotent. A file whose hash is already in the register adds nothing, and `add` prints the version it matches.
- In a company store `add` copies the file to `sources/org/`, runs the labeller (see [Handling and labels](#handling-and-labels)) and refuses the file when a label it gets is one the store may not hold (FB0412). `sync` does the same for each pulled item and writes a proposal, never a register row ([company-store.md](company-store.md)).
- A new version is a new row. The earlier row and its original stay. When the new version replaces the earlier one, its `supersedes` names the earlier version id, and the earlier version is then superseded (computed).
- A claim that still holds is moved to the new version: `version` changes, `verification` returns to `unchecked`, and the claim is checked again. A claim whose statement changes is a new claim that supersedes the old one. Either way the change class is `SOURCE_REFRESH`. `check` warns about a claim that rests on a superseded version (FB0222).

### Extraction

`cargo xtask extract <version-id>` runs at home only, and its output is committed. CI checks the hashes and never re-extracts. A change of the extraction tool's version opens a re-extraction card (`extract-and-locators`).

| Original | Method | Output in `sources/text/<version-id>/` |
|---|---|---|
| PDF | `pdftotext -layout`, one text file per page | `p0001.txt`, `p0002.txt` and on, in page order |
| HTML, XHTML, SVG | A tag stripper that keeps the text | One file, `p0001.txt` |
| Plain text | Copied | One file, `p0001.txt` |
| A table saved as CSV (UTF-8, or UTF-16 with a byte-order mark) | `csv-rows`, run by `extract` and by `sync` (core, no new crate) | `p0001.txt`, `p0002.txt` and on: one file per window of data rows, each with the header line first |
| A workbook (`.xlsx`, `.xlsm`) | `xlsx-sheets`, run by `extract` and by `sync` (core, the crate `calamine`) | `p0001.txt`, `p0002.txt` and on: one file per window of data rows of one sheet, each with that sheet's header line first, the sheets in tab order |
| Other input types | Supplied text, with the byte-exact original retained | Unavailable locators remain unchecked until a resolver is built |
| Text a person supplies for a version the program could not extract | `supplied` (home extraction fallback; laptop `add --text` from R2) | One text file per page, as supplied |

`extraction.toml` sits beside the text files. Its values here are synthetic.

```toml
tool = "pdftotext"
tool_version = "0.0.0"
method = "pdftotext-layout"
page_count = 3
thin_pages = [3]
original_sha256 = "<64 lowercase hex characters>"

[pages]
p0001 = "<64 lowercase hex characters>"
p0002 = "<64 lowercase hex characters>"
p0003 = "<64 lowercase hex characters>"

[locators]
"(e)(1)" = 2
"15.404-1" = [2, 3]
```

| Key | Meaning |
|---|---|
| `tool`, `tool_version`, `method` | What produced the text. `method` is `pdftotext-layout`, `html-strip`, `plain`, `csv-rows`, `xlsx-sheets` or `supplied` |
| `page_count` | The number of text files. For a PDF it also equals the original's page count. At most 9,999, because a page file is named `pNNNN.txt`; a run with more pages refuses and writes nothing |
| `thin_pages` | The numbers of the pages whose text has under 40 characters once whitespace is trimmed, ascending; `[]` when there are none. `extract` writes it. An image-only page shows up here: its text file is empty, and its hash still matches |
| `original_sha256` | Equals the version row's `sha256` |
| `[pages]` | The SHA-256 of each text file, keyed by file stem |
| `[method_data]` | Optional metadata for additive methods, validated by that method’s registered key schema. Existing root keys keep their meanings and positions; unsupported methods retain this table but supply no verified facts |
| `[locators]` | Section label to the page where it begins, or `[first, last]`. A label is written as in a citation, without its `sec.`, `§` or `para.` prefix |
| `unit` | What a text file stands for: `page` (the default when the key is absent) or `window`. `csv-rows` and `xlsx-sheets` write `window`, and their `thin_pages` is always `[]`, because a table has no image-only page |
| `window_rows` | The table methods only (`csv-rows`, `xlsx-sheets`): the number of data rows in a window, at least 1. The default is 50 |
| `row_count` | The table methods only: the number of data rows in the original, the header excluded; for `xlsx-sheets`, the sum over the sheets. For `csv-rows` a `rec N` locator needs `1 <= N <= row_count`; for `xlsx-sheets` it needs `1 <= N <= rows` of its sheet |
| `values` | `csv-rows` writes `as-written`: the characters in the file. `xlsx-sheets` writes `cached`: the value the workbook last stored in the cell, never a formula and never the display |
| `values_only` | `true` when the stream that supplied the file is a workbook saved as CSV (`workbook = true` in `systems.toml`); absent means `false`. A CSV holds displayed values only, so a stale cached value and a formula are gone (R-13). The `s/` page, the context pack and the review queue say "values only (saved from a workbook)" for such a version. Never written for `xlsx-sheets`, which reads the workbook itself and says what it knows in `flags` |
| `flags` | `xlsx-sheets` only; absent means none. A list of words in this order. `formulas`: some cells are formulas, and the text shows the result last saved. `calc-manual`: calculation was manual when the workbook was saved, so a result may be stale (only with `formulas`). `uncached-formulas`: some formulas had no saved result, and their cells are empty (only with `formulas`). `external-links`: the workbook links to other files, and the links were not followed. The `s/` page and the title of each text window carry a sentence for every flag |
| `[sheets]` | `xlsx-sheets` only: one table for each worksheet, `[sheets."NAME"]`, in byte order of the name, with `first` and `last` (the window numbers of the sheet; the sheets in tab order cover `1` to `page_count`), `rows` (its data rows), `columns` (the most cells in a row) and `hidden = true`, which is written only for a hidden sheet |

- `extract` writes `[locators]` for the labels it recognises. The lead adds the others by hand, and a later extraction keeps every label already present.
- A recognised label looks like a section number: a digit anywhere (`3`, `4.2`, `15.404-1`), a parenthesised sub-item (`(a)`, `(e)(1)`), an upper-case Roman numeral (`IV`) or a single upper-case letter (`A`). A line that opens with `section`, `sec.` or `§` and then continues in prose (`Section is …`, `§ applies …`) proposes nothing, and a heading word such as `Agencies` or `Affected` is never a locator. The lead adds a section heading of another shape by hand.
- A label matches exactly, after Unicode NFC. There is no fuzzy match.
- The section anchor on the `s/` page is `sec-` and the label with each run of characters outside `a-z` and `0-9` replaced by one hyphen, lowercased and trimmed of hyphens: `(e)(1)` gives `sec-e-1`. Two labels with one anchor are an error (FB0212).
- **`supplied`.** The text was supplied by a person for a version the program could not extract; it can be attached at home by the extraction fallback or on the laptop by intake. `extraction.toml` records `tool = "supplied"` and `tool_version = "1"`. A supplied extraction is not checked against the original: FB0223 treats it like a thin page for a consequential claim, and the `s/` page says "text supplied by hand; not checked against the original". Supplied text that holds a `handling` line whose value is not public is refused before any page is written, so a company label never reaches the repository Corpus.
- **`csv-rows`.** The original is a CSV file. The extractor reads UTF-8, or UTF-16 with a byte-order mark; any other encoding is not text, and the message says "save as CSV UTF-8". The first row is the header. The data rows are cut into windows of `window_rows` rows, and each window is one text file (`p0001.txt` on) that holds the header line and then the window's rows. A row is one line: its cells in file order separated by a tab, with a tab or a line break inside a cell written as a space. A `rec N` locator is arithmetic: data row N is in window `ceil(N / window_rows)`, on line `((N - 1) mod window_rows) + 2`. The quote of a claim at `rec N` must occur in that line (FB0308), after the normalisation described under Quote matching below, and N must not exceed `row_count` (FB0311). A table of more than 100,000 data rows, or an original over 32 MiB, is not extracted (`TooLarge`); the original is still kept and cited by hash. Only values reach the text: CSV holds no formula and no macro. The method is a row of the `METHODS` list in `fieldbook-core`, and a test, `methods_only_grow`, fails if a method is removed or renamed.
- **`xlsx-sheets`.** The original is a `.xlsx` or `.xlsm` workbook. Each worksheet, in tab order, is a table of its own: its row 1 is the header and its later rows are data rows. A window never mixes two sheets, so the windows of a sheet follow those of the sheet before it. A `rec NAME!N` locator is arithmetic: data row N of a sheet whose first window is `first` is in window `first + (N - 1) div window_rows`, on line `((N - 1) mod window_rows) + 2`. Blank rows are kept, so that the arithmetic holds. A row is one line: its cells from column A to its last non-empty cell, separated by a tab, with a tab or a line break inside a cell written as a space. The quote of a claim at `sheet NAME!A2:F9` must occur in the text of the cells that range names, each row's cells joined by a space and the rows joined by a space (FB0308), and the sheet, the row and the column must exist (FB0311). The text holds the value the workbook stored in each cell, never a formula and never the display: a stored `41.2` is `41.2` whatever its number format, a whole number has no decimal point, a boolean is `TRUE` or `FALSE`, an error cell is its text (`#N/A`), and a date is ISO text (`2023-07-15`, with a time after a `T` when it has one). Both date systems are read, and serial 60 is `1900-02-29`, the day Excel counts. Nothing is evaluated or run. Bounded size-accounting inflation measures every ZIP entry. Semantic extraction, including dependency reads, never accesses macro, drawing or embedded-object payloads and never follows external targets; its reader boundary is observed in tests. A hidden sheet is read and marked in `[sheets]`. The `flags` say what the extractor could tell about the values. A `.xls`, a `.xlsb` and a password-protected workbook are refused with a message that says to save a copy as `.xlsx`. A workbook over 32 MiB, one that unpacks to over 256 MiB, or one with over 5,000,000 cells, over 100,000 data rows in all or over 9,999 windows is not extracted (`TooLarge`); the original is still kept and cited by hash. A figure read from a workbook can still be wrong where nobody looks, when calculation was manual (R-13): `calc-manual` is how the page says so.
- A missing or extra text file, a hash that does not match, an `original_sha256` that differs from the row, a `page_count` that differs from the files, or a `thin_pages` list that differs from the files is an error (FB0212).
- **Extraction status** is computed, never stored: the version has no extraction (metadata-only, a type with no extractor, or not yet extracted), or it is extracted with or without thin pages. `status` and the `s/` page show it. A version with thin pages shows "N pages have little or no text; read the original" and names the pages. `check` cannot match a quote or scan for a marking on a thin page, and there is no OCR in v1. A consequential claim raises FB0223 when its version is metadata-only, its extraction lists thin pages, or its extraction method is `supplied`. Missing extraction alone does not raise FB0223.
- **Quote matching.** `check` finds a claim's quote in the text of the pages its locator names. Both sides are normalised first: Unicode NFKC, straight quotes for curly ones, a plain hyphen for every dash and minus, one space for each run of whitespace. Case is kept. A hyphen at the end of a line is tried kept and removed. Copy a quote from the extracted text, not from a screen reader view of the original. The fixtures that pin this are those of `quote-match-and-anchors`.

### Renderable originals

`.html`, `.htm`, `.svg` and `.xhtml` originals ship in the originals zip as `<version-id>.<ext>.txt`, with the same bytes and hash, so Edge shows them as text and runs nothing. The register keeps the true extension, and a section locator in such an original links to the `s/` page.

### The `s/` page

`build` writes one page per source version, `s/<version-id>.html`. It shows the register row, the extracted text with the section anchors, and the claims and pages that cite the version. A source version that has no extraction shows the row only. A `quarantined` version has no page, and the source page of a source whose every version is held is left out too.

## Dates, supersession, handling and labels

### Dates

Every date is an ISO 8601 calendar date, `YYYY-MM-DD`. There are no times and no time zones. A date that is not a real calendar date is an error (FB0114).

| Field | Where | Form | Meaning |
|---|---|---|---|
| `as_of` | Page | date | The day the page's content describes |
| `as_of` | Source version, claim | pdate | The day the data describes |
| `published` | Source version | pdate | The date printed on the document |
| `effective_from` | Source version, claim | pdate | When the text takes effect, or the fact starts to be true |
| `effective_to` | Claim | pdate | When the fact stops being true |
| `edition_date` | Claim | pdate | The clause or provision date printed on the text, such as `2025-11` |
| `retrieved` | Source version | date | The day the original was read |
| `verified_on` | Claim | date | The day a person checked the claim against the original |
| `resolved_on` | Conflict | date | The day the conflict was resolved |
| `review_by` | Page, claim | computed | The day to look at it again |
| Change note date | Page | date | The day the change was made |

**Partial dates.** A pdate column also accepts `YYYY-MM` and `YYYY`. The tool never fills in the missing parts. For comparison a partial date is its first day, so `2025-11` is `2025-11-01`. It is shown as written.

**Today.** `check` reads the calendar date once, in the executable's I/O layer, and passes it to the core. `--today YYYY-MM-DD` replaces it, so a run can be repeated. `fieldbook-core` and `build` read no clock: the edition takes its time from the commit through `SOURCE_DATE_EPOCH`.

**Review dates.** A volatility class sets how long a page or claim goes without a second look. Days are added; months are not.

| Class | Interval | Typical subject |
|---|---|---|
| `V1` | 90 days | Thresholds, clause numbers, feed status, prices, benchmarks |
| `V2` | 180 days | Peer financials, annual indexes |
| `V3` | 365 days | Definitions, arithmetic, standards text |

- A claim's `review_by` is the later known value of its `as_of` and its `verified_on`, plus the interval of its `volatility`; if both are absent, `review_by` is unknown. Date comparisons use known dates only, including the oldest cited date for FB0118.
- A page's `review_by` is the later of its `as_of` and the date of its newest valid Change note entry, plus the interval of its volatility: the page's `volatility` field, else the most volatile class among the claims it cites, else `V3`. A valid entry has a known class, a real calendar date and a sentence; any other line is not read here (FB0114 and FB0137 report it), and a page with no valid entry counts from its `as_of`. The order of the entries is not read: the newest date wins. [ADR 0002](../decisions/0002-corpus-format-v1.md#amendment-review-date-basis-pre-edition-001) records the change.
- For a known base date, a computed `review_by` past `9999-12-31` saturates at that date. This rule does not fill missing date parts or change the calculation core's checked `add_days` behavior.
- Four findings depend on today's date: a page or claim `as_of` later than today (FB0116, FB0220) and a page or claim `review_by` that has passed (FB0117, FB0221). They are warnings, and they never fail `check`, not even under `--strict`: a commit that passes today must not fail next month. `status` lists them as next actions.

### Supersession

A superseded object is never deleted. It stays, points to what replaced it, and the reader marks it.

| Object | Fields | Reciprocal | What follows |
|---|---|---|---|
| Page | `supersedes`, `superseded_by` | Yes (FB0115) | The reader links the old page to its replacement |
| Source version | `supersedes` on the new row | No. A version is superseded when another row names it; the state is computed | The old original stays. A claim resting on it warns (FB0222) |
| Claim | `supersedes`, `superseded_by`, and `adoption` set to `superseded` | Yes (FB0208) | A page that cites it warns (FB0310) |
| Term | `superseded_by` | No | The reader links the old term to the new one |
| Question | `status` set to `withdrawn` | | |

- Each id named exists (FB0110, FB0207). No id names itself, and no chain loops (FB0115, FB0208).
- A page that cites a superseded or withdrawn source version, a quarantined version or a superseded claim gets a warning (FB0310). `build` marks the citation.
- Context packs (Day 30) leave out quarantined versions always, and superseded versions, superseded claims, contradicted claims and Tier-D sources unless `--include` names them. One exception: a selected claim brings its conflict row and every claim in that conflict's `claim_ids`, shown as contradicted even though contradicted claims are excluded by default. A claim and its conflict are kept or dropped together, and what is dropped is listed under `left_out_budget`.

### Change records

Two logs have two jobs and stay separate.

| Log | Holds | Form |
|---|---|---|
| A page's `## Change note` | Why the page changed, for that page's readers | `- CLASS, YYYY-MM-DD: sentence.` Newest first. The oldest entry is `NEW_KNOWLEDGE` (FB0137) |
| `corpus/changes.md` | What an edition added, changed and verified | `## edition-NNN`, newest first, then bullets `- Added: …`, `- Changed: …`, `- Verified: …`. No dates: the tag carries the date (FB0124) |

- The class is one of the four change classes the Corpus accepts: `NEW_KNOWLEDGE`, `SOURCE_REFRESH`, `CLAIM_CONFLICT`, `COMPATIBLE_EXTENSION`. `MATERIAL_CONTRACT_CHANGE` is company-store only (R2).
- `changes.md` renders as `changes.html` from `edition-002`, next to the list of ids added since the earlier edition (`build --since <edition>`).
- The file opens with a `#` title and one line that says how entries are written. The sections follow. A file with no section yet is valid.
- The newest `## edition-NNN` heading names the edition. The [release runbook](../runbooks/release.md) says who writes it and when.

### Handling and labels

`handling` is a field of every page and every source version, and of every record in a company store. Its value is a set of label ids. In a page's front matter it is an array, `["public"]`. In a register cell it is the ids sorted and joined by `|`. A company label id matches `[a-z]{2,4}-[0-9]{4}` (`np-0007`). It is an opaque code because ids are written to the journal, and a name would tell a reader which counterparty a record concerns; the loader refuses any other shape (FB0413). The company defines its labels in its policy file ([policy.md](policy.md)). Four ids are built in: `public` (a public source or a synthetic example), `unlabelled` (no label could be found), `marked` (a marking or a source's own label was found and no rule maps it) and `owner-supplied` (text the owner typed or pasted at a prompt, for that one request: no page, source version or record carries it, and only the logs in `state` do). A set is sorted, has no repeats and is never empty, and `public` is dropped beside any other label. A thing derived from others (extracted text, a search chunk, a pack, an export, a backup, a calculation) carries the union of the labels of its inputs. The full rules are in [ADR 0004](../decisions/0004-company-boundary.md).

The `handling` cell holds the labels given at creation, and it is never rewritten. A later change is a line in the store's ledger `labels.csv` (`seq`, `item_id`, `kind`, `labels`, `origin`), which only grows. The labels in force for an item are its cell plus the ledger's `add` lines for its id, read by one function that every check, search, pack and backup uses ([the label ledger](company-store.md#the-label-ledger)).

In the repository Corpus the only value is `public`. Any other value is an error (FB0410), because the Corpus is public by definition. In a company store each id is checked against the label table of the policy that is loaded: an id the table does not hold is a warning (FB0411) and the record is treated as unresolved, and a set the store may not hold is refused at intake (FB0412).

| Store | Where it lives | What it holds | Rule |
|---|---|---|---|
| `corpus` | The repository, every edition, and the shipped edition on the laptop | `public` only | Public sources and synthetic examples. No pointer rows: even a pointer discloses, and review refuses it |
| `overlay`, the company store | The data root on a company device, in `overlay\` | The labels its `[[store]]` row lists in `holds`. With no policy: `public`, `unlabelled` and `marked` | Never committed to a source repository, never in an edition. Real figures, cost builds, full documents and pointers are all allowed in it |
| `working` | The data root | As the floor, unless the policy widens it: `public` and `unlabelled`, and `marked` when a company store is on the machine and no policy is loaded | The owner's drafts and notes |
| `state` | The data root | `public`, `unlabelled` and `owner-supplied`, and `marked` when a company store is on the machine and no policy is loaded, unless the policy widens it. `owner-supplied` is valid in the `holds` of `state` and of no other store | The queue, the journal, the settings, the question log and call records. Items in the inbox and queue are admitted by the store they are bound for |

- **Fieldbook carries labels; it does not classify.** The labeller reads a source's own label field, the file's markings and the policy's rules, and unions what it finds. A built-in banner shape (the FB0400 list in [Scans](#scans)) that no policy rule maps makes the file `marked`; the company's `[[marking]]` rules map text to its own labels. Fieldbook never decides by meaning that a document is controlled, never lowers a label and never removes or edits a marking. It makes no compliance claim: which stores qualify for which labels, and the controls around them, are the company's.
- **How `public` is given.** Only by an act: the edition build, a policy `[[source]]` row for a stream whose folder lies inside that row's `root`, or, on a machine with no company store, a person’s typed confirmation for home `add` or `new` (the flag `--confirm-public`), which the journal records. A candidate for the Corpus with no banner and no act is `unlabelled`, and `corpus-add` refuses it with FB0410 (`reason` `unlabelled`).
- **Who changes a label.** A person at a console. `label add` needs an interactive console on both input and output and a typed confirmation that names the number of items, and redirected input or output is refused (FB0822); a terminal and role setting do not prove human identity. On the laptop preset the `role` word in `settings.toml` is ignored for this verb, and a store row that names `changers` (a Windows group) needs the logon token in it. It adds a company label to a page, a source version or a record. It never adds `public`, `unlabelled`, `marked` or `owner-supplied`. Each change appends a `label.change` event with the before and after sets. Version 1 has no `label remove`. `label reapply` re-runs the labeller under the current policy and can only add.
- **Where data goes.** A request leaves a store only through a destination the policy names, only with labels that destination allows, and only after the journal holds the decision (see [policy.md](policy.md)). Nothing in the product writes a record with a non-public label, its extraction, its original or any text derived from it under a git work tree (FB0415), into an edition, or to the home side. The test for a git work tree, a synced folder or a network drive is on the folder itself (`World::place`), not on its name. `backup` copies every root of every configured store to a target the owner or IT names, which is a destination row with its own labels. A snapshot is labelled by exclusion: a file of no known kind adds `unlabelled`, and a snapshot that holds `marked` is refused (FB0901), as is one that holds `unlabelled`, except that on a machine with a company store and no policy the folder the owner recorded as company storage takes `unlabelled`. `restore` fills an empty folder; a restore onto a second machine stamps `restored_from`, and no id is issued there until `init --overlay --adopt` records that the old machine is retired or gives a new writer (FB0836).
- **One vocabulary.** Prose says "label" and "company store". The words of the old two-zone model are listed in [ADR 0004](../decisions/0004-company-boundary.md), and the docs lint `retired_terms_absent` fails on any of them outside the decision records that define them and the settings card that refuses the removed keys.
- The company store's files and layout are in [company-store.md](company-store.md). Estimating, pricing and the rate book are record types in it, designed in a later decision record.

### Scans

`check` runs these scans over committed files. Each has its own code in the 04 range of the [Check catalogue](#check-catalogue). The pattern lists are the v1 lists; the code holds them and a test keeps both equal. A pattern may be added by a card with the owner's review and a line here. A scan only refuses and an allowance can pass a false match, so adding a pattern is not a schema change. A pattern is never removed in v1.

| Scan | Code | Reads | Catches | Allowance |
|---|---|---|---|---|
| Markings | FB0400 | Every committed text file under `corpus/` of the repository Corpus, extracted text included. `add` into the Corpus also reads the text of the file being added. A company store is not read for it: there a marking becomes a label | The banner shapes below | `scan-allow.toml` |
| Identity shapes | FB0401 | Hand-written files of the repository Corpus: pages, registers, `corpus.toml`, `changes.md`, `scan-allow.toml`, templates and rule files. Not `sources/`. A company store is not read for it: a vendor contact or a staff name there is ordinary company data under the store's labels | The identity shapes below | None |
| Claim language | FB0402 | Page bodies, except `Precise:` blockquotes and fenced code, and the `statement` column of `claims.csv`. Never a quote | `CMMC compliant`, `NIST certified`, `FIPS compliant`, `EVMS compliant`, `audit-ready`, `DCAA approved`, `independently corroborated`, `independently confirmed`, `multiple independent sources`, in any case. The last three make prose name the sources instead | None |
| Formula triggers | FB0403 | Every cell of the five registers that people type. A company-store record file escapes such a cell instead ([the record file rules](company-store.md#typestoml-and-records)) | A text cell that starts with `=`, `+`, `-`, `@`, TAB or CR | None |
| Label | FB0410 | `handling` of every page and source version in the repository Corpus | A value other than `public` | None |
| Store labels | FB0411, FB0412 | `handling` of every page, source version and record in a company store, against the loaded policy | An id the label table does not hold (a warning); a set the store may not hold (an error) | None |
| Secrets | none | Staged changes (pre-commit), the PR range, and the full history weekly | Keys and tokens | None. This scan runs outside `check` |

A PDF original is scanned through its extraction, which is committed text (`extract-and-locators`). A thin page may be an image that holds a marking the scan cannot see, so `status` names thin pages and the owner reads the original there. The personal-name denylist is a local pre-push hook and is not part of `check`.

**Banner shapes** (FB0400 and the labeller). Capital letters, as printed on a marking. Lower-case running text does not match. In the repository Corpus a match is an error. In a company store the same shapes are the built-in marking rules of the labeller: a match that no `[[marking]]` rule maps makes the file `marked`, and a match the policy maps gives the labels the rule names. The text itself is never changed.

| Shape | Matches |
|---|---|
| Controlled information | `CUI//` and `CONTROLLED UNCLASSIFIED INFORMATION` |
| Dissemination | `NOFORN` and `FOUO` |
| Distribution statements | `DISTRIBUTION STATEMENT B` to `DISTRIBUTION STATEMENT F`. Statement A, public release, does not match |
| Export control | `EXPORT CONTROLLED`, `EXPORT-CONTROLLED`, `ITAR CONTROLLED`, and the phrase `technical data whose export is restricted` in any case |
| Company and procurement | `PROPRIETARY`, `COMPANY CONFIDENTIAL`, `SOURCE SELECTION INFORMATION`, `CONTRACTOR BID OR PROPOSAL INFORMATION` |

**Identity shapes** (FB0401).

| Shape | Matches | Does not match |
|---|---|---|
| Email address | `name@host.tld` | A `noreply` or `no-reply` local part, and the host `users.noreply.github.com` |
| Phone number | A leading `+` and at least nine digits; a parenthesised area code and seven digits; three, three and four digits joined by hyphens | A dotted clause number such as `252.204-7012` |
| Private IPv4 address | Four dotted decimal fields in `10/8`, `172.16/12`, `192.168/16`, `127/8` or `169.254/16` | A public address |
| Internal host name | A name that ends `.lan`, `.local`, `.internal` or `.home` | |
| Home-directory path | `C:\Users\` or `/home/` or `/Users/` followed by a plain name | A placeholder in angle brackets or percent signs |
| Tracker key | Two to ten capitals, a hyphen and one to six digits, with no letter, digit or hyphen on either side | Every prefix in the const `FAMILIES` table (`Q`, `XC`, `CL`, `FB`, `CALC`, `QA`, `FC`, the reserved case prefixes `EST`, `QTE`, `SCH`, `RTE`, `EAC`, `EVM` and `CTF`, and the company prefixes); the control families `AC AT AU CA CM CP IA IR MA MP PE PL PM PS PT RA SA SC SI SR`; and `DD`, `SF`, `ISO`, `AS`, `FIPS`, `RFC`, `BIS`, `DS`, `EEO`, `EIA`, `FSPM`, `HSPD`, `LS`, `MAR`, `NQA`, `SHA`, `VETS`, `WH`. No tracker key prefix is exempt |

**`scan-allow.toml`.** The file holds no allowance in v1. It exists so that a public document which prints a marking in its own text, such as a regulation that quotes a banner, can be kept. The file belongs to the repository Corpus. A company store has none: a public document that prints a banner there is `marked` until the policy maps the marking (the Later row `later-banner-allowance`).

```toml
[[allow]]
sha256 = "<64 lowercase hex characters>"
match = "<the matched text, exactly>"
reason = "<why this public text may stay>"
```

- An empty file, or one that holds only comments, is valid and allows nothing.
- `sha256` is the hash of the file that holds the match. An edit to that file changes the hash and ends the allowance.
- `match` is the matched text as the scan reports it. `reason` is a sentence and is required. Only FB0400 reads this file.
- A malformed entry, an entry without a reason, or two entries with the same `sha256` and `match` is an error (FB0404). An entry that matches nothing is a warning (FB0405), because the file changed or the text left.

## Check catalogue

Every check has a code: `FB` and four digits. A code never changes meaning and is never reused. The first two digits give the range.

| Range | Covers | Raised by |
|---|---|---|
| 01 | Pages, `corpus.toml`, rule files and the other files under `corpus/` | `check` |
| 02 | Registers and sources | `check` |
| 03 | Citations | `check` |
| 04 | Scans, labels and stores | `check`, `add`, `sync`, the labeller and the policy loader, and the browser from Day 30 |
| 05 | Calculation | `fieldbook-calc` and its tests |
| 06 | Site and edition | `build`, `cargo xtask edition` and tests |
| 07 | Handoff (Day 30) | `context`, `cite-check`, `new note` |
| 08 | Audit, backup, pipeline and sync | `audit verify`, `audit export`, `backup verify`, `restore`, `add`, `queue` and `sync`; `check` for FB0826 |
| 09 | Model, decide, index, output, MCP and chat | `ask`, `decide`, `ai test`, `mcp`, `chat` and the adapters |

| Severity | Meaning |
|---|---|
| error | The command exits 1 |
| warning | Reported. It never changes the exit code |
| strict | A warning, and an error under `check --strict`. The strict rules are FB0118, FB0230 and FB0320 |
| warning, error from Day 30 | A warning in `v0.1.0` and an error from `v0.2.0` |
| from Day 30, from R2 | The check arrives with that release. Before it, nothing raises the code |
| reserved | The code is allocated and nothing raises it yet |

- `docs/reference/errors.md` is generated from this catalogue by `cargo xtask docs`, and the card `check-v0` adds it. A test keeps the `Code` enum in `fieldbook-core` equal to this catalogue: the same codes, severities and one-line meanings.
- Each known-bad example below is a negative fixture. For ranges 01 to 07 a test runs `check` on it and expects exactly that code. For ranges 08 and 09 the fixture is a file of its own (see [Known-bad files](#known-bad-files)). Examples are synthetic.
- The diagnostic form, the exit codes and the `--json` report are in AD-17 of [ADR 0001](../decisions/0001-foundation.md). `docs/reference/cli.md` (generated) prints the strict rules by code.
- The codes added for the company boundary arrive with the card that raises them. Until that card merges, a code is reserved: nothing raises it.
- A code decided in process (FB0807, FB0918, FB0925) is decided by the card named here; its known-bad file or row goes with the card that holds the verb.

| Codes | Raised by the card |
|---|---|
| FB0411, FB0412, FB0415 | `core-labels` |
| FB0413, FB0807, FB0918, FB0925 | `core-policy` |
| FB0414, FB0418, FB0419 | `policy-load` |
| FB0806 | `audit-export` |
| FB0311 | `extract-csv-rows` (a `rec` locator), `extract-xlsx-sheets` (a `sheet` locator, and a `rec` locator on a workbook) and `r2-records` (a `field` locator) |
| FB0416, FB0417, FB0420, FB0836 | `r2-company-ids` |
| FB0808 | `r2-state-and-journal` |
| FB0504 | `calc-cases` (a case) and `r2-records` (a `calc_run` cell) |
| FB0215, FB0216, FB0830 to FB0835 | `r2-pull-and-sync` |
| FB0240, FB0241, FB0380 to FB0382 | `r2-records` |

### 01: pages, `corpus.toml` and files

| Code | Severity | Check | Known-bad example |
|---|---|---|---|
| FB0100 | error | `corpus.toml` is missing or is not decodable TOML, an area or view is malformed (unknown key, repeated `code` or `slug`, a view filter that names no area or subarea), or `schema_version` is newer than the tool supports; in a company store also, `store.toml` is missing, has an unknown key, a `writer` outside 0 to 9999 or a `store_uid` that is not a version 4 UUID. A physical read failure is not this finding: the file reader exits 3 with the path | `schema_version = 9` |
| FB0101 | error | A page has no front matter, or the `+++` fences are missing or unbalanced | A first line of `---` |
| FB0102 | error | The front matter is not valid TOML for the schema: a syntax error, an unknown field, or a value of the wrong type | `titel = "…"`, or `stage = "3"` |
| FB0103 | error | A required front-matter field is missing | No `as_of` |
| FB0104 | error | A front-matter value is outside its vocabulary or form | `status = "final"` |
| FB0105 | error | `id` is not the page id form, differs from the file stem, or is used twice | File `cmp-withholding.md` with `id = "cmp-withhold"` |
| FB0106 | error | `area` is not an area code in `corpus.toml`, or differs from the id's first segment or the area folder | `id = "cmp-x"` with `area = "est"` |
| FB0107 | error | `subarea` is not one of the area's subareas, or differs from the folder | `subarea = "cash"` under `cmp` |
| FB0108 | error | `type` is not a page type of this schema version | `type = "note"` under schema 1 |
| FB0109 | error | A computed field is written | `review_by = 2026-01-01` |
| FB0110 | error | A page id in `prereq`, `links`, `supersedes` or `superseded_by` does not exist | `prereq = ["cmp-nope"]` |
| FB0111 | error | The `prereq` graph has a cycle, or a prerequisite has a later `stage` | Two pages that list each other |
| FB0112 | error | A `terms` id is not in `glossary.csv` | `terms = ["no_such_term"]` |
| FB0113 | error | A `questions` or `conflicts` id is not in its register | `questions = ["Q-999"]` |
| FB0114 | error | A date is not a real calendar date, or is partial where a full date is required | `as_of = 2025-02-30` |
| FB0115 | error | Page supersession is not reciprocal, names the page itself, or loops | A lists B in `superseded_by`; B does not list A |
| FB0116 | warning | A page's `as_of` is later than today | `as_of` a year ahead |
| FB0117 | warning | A page's `review_by` has passed | A `V1` page last dated 200 days ago |
| FB0118 | strict | A page's `as_of` is later than the oldest `as_of` among the claims it cites | A page dated 2025-11-01 that cites a claim dated 2025-06-01 |
| FB0120 | error | A text file other than a register is not valid UTF-8 | A Windows-1252 byte in a page |
| FB0121 | error | A file or folder sits where the layout does not allow it | `pages/compliance/notes.txt`, or a page directly in an area folder |
| FB0122 | error | An accepted register is not in `table::is_normal` form (reported at 1:1), or another text file outside `sources/` has a byte-order mark, CRLF, non-NFC text, trailing whitespace or no single final newline. Register cell whitespace is preserved; unreadable registers get parse diagnostics. `fmt` fixes form | A register saved by Excel |
| FB0123 | error | A path breaks the path rules: over 150 characters, two paths that differ only by case, a reserved name, a trailing dot or space, a forbidden character, an id over 64 characters | A 160-character path |
| FB0124 | error | `changes.md` is malformed: a `##` heading that is not `## edition-NNN`, numbers not newest first, a bullet that does not start `- Added:`, `- Changed:` or `- Verified:`, or a date | `## edition-1` |
| FB0125 | error | A rule file is out of form: `ASSISTANT.md` is missing, longer than 40 lines, or does not begin with `rules_version = <integer>`; or `corpus/AGENTS.md` is longer than 40 lines | A 41-line `ASSISTANT.md` |
| FB0126 | error | `eval/regression.toml` is malformed, repeats a case `id`, or names an expected unit id that does not exist | `expect = ["cmp-nope"]` |
| FB0127 | warning | A regression case misses: an expected page is not within the first `top` page rows of the search page view, or an `expect_none` query returns results | A non-critical case whose page is the sixth page row |
| FB0128 | error | A regression case marked `critical` misses | A critical case whose answer is absent from the top 3 |
| FB0130 | error | The part headings are not exactly the type's parts in order, a `#` heading is present, or a heading is deeper than `###` | `## Rule` for `## The rule` |
| FB0131 | error | A part other than Sources is blank | `## Purpose` followed at once by another heading |
| FB0132 | error | The `## Sources` part holds text | A sentence under `## Sources` |
| FB0133 | error | A Worked example that has content does not begin with the synthetic label | A worked example that starts with a table |
| FB0134 | error | A page type's required content is missing | A `procedure` with one step |
| FB0135 | error | Raw HTML (comments included), an image, or a link to an address outside the Corpus | `<!-- todo -->` |
| FB0136 | error | A relative link does not resolve to a file in the Corpus | `[text](missing.md)` |
| FB0137 | error | The Change note is missing, an entry is malformed or has an unknown class, or the oldest entry (the last line) is not `NEW_KNOWLEDGE` | `- UPDATE, 2025-11-01: text.` |
| FB0138 | warning | A `brief` is longer than 400 words | A 600-word brief |
| FB0139 | warning | A bold `**term**:` defines a glossary term on a page | `**Material weakness**: a deficiency …` |

### 02: registers and sources

| Code | Severity | Check | Known-bad example |
|---|---|---|---|
| FB0200 | error | A register file is missing, or its header row is not exactly the columns of this specification, in order | `claims.csv` without `claim_id` |
| FB0201 | error | CSV syntax: an unbalanced quote, a row with the wrong number of cells, or a line break inside a cell | A `statement` that wraps onto a second line |
| FB0202 | error | A register is not valid UTF-8. The message says "save as CSV UTF-8" | A Windows-1252 file |
| FB0203 | error | A key is malformed or used twice: `claim_id`, `question_id`, `conflict_id`, `term_id`, or the pair `source_id` and `version` | Two rows for `CL-CMP-0011` |
| FB0204 | error | A cell is outside its type, vocabulary, form or length, or has a vertical bar outside a list column | `verification = "checked"` |
| FB0205 | error | A required cell is empty | A claim without `statement` |
| FB0206 | error | Cells break a rule across columns | `value = 5` with no `unit` |
| FB0207 | error | A reference does not resolve: a source version, claim, question, conflict, term or area that does not exist | `source_id = S-9999` |
| FB0208 | error | Supersession in a register is not reciprocal, names the row itself, or loops | A claim whose `superseded_by` names a claim that does not list it in `supersedes` |
| FB0209 | error | A promotion rule is broken: a status is `primary_source_supported` without every condition, or a consequential claim was verified by the lead | A verified claim with no `verified_on` |
| FB0210 | error | A company-store-only value is present in the repository Corpus | `applicability = "confirmed_applicable"`, or `source_kind = "vendor_quote"` |
| FB0211 | error | A source original does not match its row: a missing file, a file with no row, a hash that differs, or a metadata-only row that has a file | One byte changed in `S-0042-v2.pdf` |
| FB0212 | error | An extraction does not match its original: a missing or extra text file, a hash that differs, a `page_count` or `thin_pages` list that differs, table keys that do not fit the method or the files, or two labels with one anchor | `page_count = 4` with three text files |
| FB0213 | error | A claim that is `primary_source_supported` rests on a source version that is `quarantined` or `withdrawn` | Verified claim on a withdrawn version |
| FB0214 | error | A glossary label or alias is used twice, ignoring case | Two terms with the alias `EAC` |
| FB0215 | error | An origin cell is wrong: `origin_system`, `origin_stream` and `origin_key` are not all set or all empty, the system is not declared in `systems.toml`, the stream is not one of that system's streams, or the key is over 200 bytes; or `doc_kind` is not a word of `DOC_KINDS`, or `doc_kind` or `series` is set in the repository Corpus | `origin_system` set with no `origin_key`; `doc_kind = "memo"` |
| FB0216 | error | A series is inconsistent: an ordinary document revision of one `origin_key` does not name the version it replaces, in a revision stream its `as_of` goes back, or two sources of one stream share an `origin_key` | A second version of a key with an empty `supersedes`  Ordinary document predecessor and monotonic-date checks exempt only valid record-load observation bindings; missing or malformed bindings refuse. |
| FB0220 | warning | A claim's `as_of` is later than today | `as_of` a year ahead |
| FB0221 | warning | A claim's `review_by` has passed | A `V1` claim last checked 200 days ago |
| FB0222 | warning | A claim rests on a source version that a later version supersedes | A claim on `S-0042-v1` when `S-0042-v2` supersedes it |
| FB0223 | warning | A consequential claim cites a source version whose extraction is metadata-only, has pages with little or no text, or whose extraction method is `supplied` | A rate claim on a metadata-only version |
| FB0230 | strict | A question with `day_one` set to `true` has an empty cell among `owner_role`, `blocks`, `why_it_matters`, `default_assumption` and `work_can_continue` | A day-one question with no default assumption |
| FB0240 | error | A company store's `types.toml` is invalid: an unknown key, a repeated `(name, revision)` pair or a repeated column name within one revision, a column named like an envelope column, a column named like the currency cell of a money column, a column kind the reader does not have, an `enum` with no `values`, a `title` or `key` column that does not exist or is a money or list column, a `search` or `show` column that does not exist, a `ref` to a type that is not defined, or a file in `records\` that no type names | A column of kind `price` |
| FB0241 | error | A record version breaks a rule of the record file that the cell checks do not cover: a version number that skips, a version after the first whose `supersedes` is not the version before it (or names a version of another record), a `type` that is not the file's, an `origin_key` that is not the joined key columns, two records of one stream with one `origin_key`, or, when a stream is loaded, a column the type does not have or lacks, or a natural key twice in one file | Version 3 of a record with no version 2 |

### 03: citations

| Code | Severity | Check | Known-bad example |
|---|---|---|---|
| FB0300 | error | A citation cannot be read: the brackets are unbalanced or empty, or a key is not an id | `[@S-42]` |
| FB0301 | error | A citation is not in the canonical form | `[S-0042-v2 p.14]` |
| FB0302 | error | Two citations touch as `][@` | `[@CL-CMP-0011][@CL-CMP-0012]` |
| FB0303 | error | A citation names an id that does not exist | `[@CL-CMP-9999]` |
| FB0304 | error | A locator does not resolve in the extraction: a page beyond `page_count`, or a section label that is not in `[locators]` | `[@S-0042-v2, p. 400]` for a 30-page original |
| FB0305 | error | A locator follows a claim, question or conflict key | `[@CL-CMP-0011, p. 3]` |
| FB0306 | error | A consequential claim has no fine locator | A threshold with `no_fine_locator` |
| FB0307 | error | A consequential claim has no quote | A rate claim with an empty `quote` |
| FB0308 | warning, error from Day 30 | A claim's quote is not found in the extracted text at its locator. The error applies to a consequential claim. A quote on any other claim stays a warning | A quote with one word changed |
| FB0309 | warning, error from Day 30 | The number-unit lint: a figure outside the exempt places has no claim citation in its sentence. A sentence ends at `.`, `!` or `?` followed by whitespace or the end of the block; a decimal point does not end one | "The threshold is 15 percent." with no citation |
| FB0310 | warning | A page cites a superseded or withdrawn source version, a quarantined version or a superseded claim | A page that cites a claim with `superseded_by` set |
| FB0311 | error | A `rec` locator names a data row that the version's table extraction (`csv-rows` or `xlsx-sheets`) does not have, a `sheet` locator names a sheet, a cell or a range that its `xlsx-sheets` extraction does not have, or a `field` locator names a column that the record version does not have or whose cell is empty | `rec 400` for a table of 300 rows |
| FB0320 | strict | A `stable` page cites a consequential claim whose `verification` is not `primary_source_supported` | A stable page on an unchecked claim |
| FB0380 | error, from `r2-records` | A numeric field lacks its computed inputs or a resolvable matching stored run, or its judgment evidence lacks a rationale; row defaults and display cells do not replace field evidence | A computed field with a run hash but no stored run |
| FB0381 | error, from `r2-records` | A quoted numeric field lacks one typed evidence pointer or available target/text, or its value, unit or quotation disagrees with the evidence. Invalid cell and field locators use FB0311 | A quoted field whose document quotation does not match |
| FB0382 | warning, from `r2-records` | A record version with computed field evidence rests on an input that is superseded or withdrawn, so the figure looks current and is not. `check` and `status` share one function (`rests_on`) and so give the same warning | A Computed record whose input source version is withdrawn |

**Notes on the 03 range.**
- **Locators and extraction.** FB0304 and FB0308 need the extraction of the cited version. A version with no extraction (a metadata-only version, a type with no extractor, a version not yet extracted) is not resolved or matched, and `status` lists it. FB0306 and FB0307 apply either way.
- **Number-unit lint.** A *figure* is a number with a currency sign or code, a percent sign, a unit word (`days`, `weeks`, `months`, `years`, `hours`, `dollars`, `million`, `billion`, `thousand`, `percent`) or a `K`, `M` or `B` suffix. A figure is exempt inside `Worked example`, a `Precise:` blockquote, `Change note`, a code block and a citation. Any other figure needs a claim citation in its sentence. From Day 30 the finding is an error for every uncited figure.
- **Quote matching** is described under [Sources](#extraction).

### 04: scans, labels and stores

The shapes and the scopes are in [Scans](#scans).

| Code | Severity | Check | Known-bad example |
|---|---|---|---|
| FB0400 | error | A marking shape is in a committed text file of the repository Corpus, or in the file `add` is given for it, and no allowance covers it. A company store has no such error: a marking there becomes a label | A page that contains a controlled-information banner |
| FB0401 | error | An identity shape is in a hand-written file of the repository Corpus | A personal email address in a page |
| FB0402 | error | A claim-language phrase is in prose or in a claim statement | "The program is CMMC compliant." |
| FB0403 | error | A text cell in a register that people type starts with `=`, `+`, `-`, `@`, TAB or CR | A `note` cell of `=1+1` |
| FB0404 | error | A `scan-allow.toml` entry is malformed, has no `reason`, or repeats another entry's `sha256` and `match` | An entry with an empty `reason` |
| FB0405 | warning | A `scan-allow.toml` entry matches nothing | An entry for a file whose text has changed |
| FB0410 | error | A page or source version of the repository Corpus has a `handling` other than `public`, or home `add` or `new` receives text without `--confirm-public`, so it is `unlabelled`; the message names the flag and says to rerun the command with it only if the supplied text is public | `handling = ["np-0007"]`; `add` with no `--confirm-public` |
| FB0411 | warning | A page, source version or record in a company store carries a label id that the policy's label table does not hold. It is treated as unresolved | `handling = ["np-9999"]` with no such label |
| FB0412 | error | A store may not hold the labels of what it is given. Intake (`add`, `new`, `sync`, `queue accept`) is refused | A record labelled `np-0007` offered to a store that holds `public` only |
| FB0413 | error | The policy file is invalid (including a label id that is not of the form `[a-z]{2,4}-[0-9]{4}`, and an example file loaded as the policy), or its `revision` is lower than the highest recorded. Egress is closed | A policy that lists `unlabelled` in an `allow`; a `revision` that went down |
| FB0414 | error | The policy file is read from a place the owner can edit: the data root, the Corpus or, on the laptop layout, a git work tree | `--policy` pointing into the data root |
| FB0415 | error | On the laptop layout, a store, the state folder or a drop folder lies in a git work tree, or fails the place test (a sync root, a reparse point, or a network drive where the rule needs a local one). On any layout, a record with a label other than `public` would be written under a git work tree, and at home `.fieldbook/` is not named in an ignore file of the checkout | `--state` set to a folder inside a checkout |
| FB0416 | error | A company id shape, or a company area, is in a file or register of the repository Corpus | `CL-XES-000012` in a public page |
| FB0417 | error | A row of the company store defines an id of a public shape | `S-0042` in the company `sources.csv` |
| FB0418 | error | The policy file or its folder can be written by someone who is not an administrator. Egress is closed | A policy file that any user of the machine can change |
| FB0419 | error | On the laptop preset, a flag names a state, overlay or working folder other than the fixed laptop folders | `--state` on the laptop preset |
| FB0420 | error | A store folder lacks the not-content-indexed attribute | A restored `overlay\` folder |

### 05: calculation

Raised by `fieldbook-calc` and its tests (`calc-day1`, `calc-ops`, `calc-cases`). `check` raises FB0500 for a `value` cell and for a `money` or `decimal` cell of a record in a company store, FB0501 for a record cell outside the range of the calculation core, and FB0504 for a `calc_run` cell that names a function the core does not have.

| Code | Severity | Check | Known-bad example |
|---|---|---|---|
| FB0500 | error | A decimal does not follow the decimal grammar (a thousands separator, an exponent, a currency sign, a leading zero or a trailing dot), or a rounding-policy scale is outside its permitted range | `1,000.00` |
| FB0501 | error | A result is outside the range the calculation type holds | A product of two very large amounts |
| FB0502 | error | A calculation mixes currencies or units, or a money value has no currency | `USD` added to `EUR` |
| FB0503 | error | A calculation case gives a result that differs from its expected value | `CALC-01` expects 1230.50 and gets 1230.49 |
| FB0504 | error | A calculation case, or a `calc_run` cell of a company-store record, names a function that is not in `CALC_FNS` | A case with `fn = "build_up@9"` |

### 06: site and edition

Raised by `build`, `cargo xtask edition` and the tests of the build and edition cards.

| Code | Severity | Check | Known-bad example |
|---|---|---|---|
| FB0600 | error | `build` was run on a Corpus that has errors, and refuses | `build` after an FB0103 |
| FB0601 | error | The edition holds a file the allowlist refuses: an exe, dll, ps1, bat, cmd, vbs, lnk or xlsm file, a remote URL in an asset, an `.html`, `.htm`, `.svg` or `.xhtml` file outside the site, or any file of a `quarantined` version beyond its register row; or the staged edition lacks a file the edition must hold (`REQUIRED_FILES` in `xtask/src/edition.rs`) | A planted `.ps1`; the original of a quarantined version; an edition with no `data/registers.xlsx` |
| FB0602 | error | An edition path breaks the path rules: over 150 characters from the edition root | A 151-character path |
| FB0603 | error | A link in the built site does not resolve to a built file or anchor | A link to a page that has no output |
| FB0604 | error | A built page breaks a site rule: no `<a href="http:">` or `<a href="https:">`, except on `s/` to a listed official publisher over `https:`; a `style="` attribute; an inline event handler; a script that is not a file in `assets/`; no content security policy; or a policy that is not equal to `CSP_STATIC` | An `href="http` in a page |
| FB0605 | error | A script or data file is over budget: `app.js` 32 KB, `search.js` 11 KB, `assist.js` 20 KB, 64 KB of scripts in all, 15 MB for `search-index.js`, or 3 MB for any other data file of the edition, except that `data/registers.xlsx`, each `data/<register>.csv` and each `data/claims-<AREA>-<part>.csv` are downloads no page loads and have the hosted site's 25 MiB per-file limit. The edition's `search-index.js` holds page, section and glossary term units only. The company browser loads shards of at most 8 MB with at most 24 MB of live search data, as search.md specifies. Scripts are app.js, search.js, strings.js and assist.js; strings.js counts only in the 64 KB total. search-index.js and assist-data.js are data, not scripts. Other .js files count as scripts. A KB is 1,000 bytes. A register cell too long for a workbook cell (over 32,767 characters) also stops the build with FB0605, naming the register, the row id and the column | A 70 KB `app.js`; a joined list in one `claims` cell over 32,767 characters |
| FB0606 | error | Two builds of one commit differ | A timestamp in an output file |
| FB0607 | warning on Linux, error in the `windows` job | A `brief` prints on more than one page | A brief that runs onto a second page |
| FB0608 | error | Edition integrity: a file is missing from `SHA256SUMS` or differs from it, the fingerprint differs from `edition.toml`, or an extraction hash differs | A zip with one file changed |

### 07: Handoff

All of these arrive at Day 30. A `cite-check` citation takes the first applicable status from `cite_statuses`. The fact, precedence and exit rules are in [Citation checks](citation-check.md#status-and-exit). A question or conflict reference is neutral. "Verified" means every applicable reference, locator and direct quotation check passed; it does not establish that the answer is correct. `cite-check` prints "N of M citations do not resolve" and exits 1 for unresolved citations or when nothing verifies, except an answer solely of abstentions.

| Code | Severity | Check | Known-bad example |
|---|---|---|---|
| FB0700 | error | A question given to `context`, or typed into the composer, carries a marking shape that the labels of the request do not cover. The copy is refused | A question that contains a controlled-information banner |
| FB0701 | error | `cite-check` finds a broken citation: the id does not exist | `[@CL-CMP-9999]` in an answer |
| FB0702 | error | `cite-check` finds a broken locator | A page that the original does not have |
| FB0703 | warning | A citation names a superseded version or claim | A citation of `S-0042-v1` after `v2` replaced it |
| FB0704 | warning | A citation is weak: the source is Tier D, the cited text is a note, or the claim is unchecked, unresolved or contradicted | A citation of an unchecked claim |
| FB0705 | warning | A sentence has no citation. The exact abstention phrase is accepted | A figure with no citation |
| FB0706 | warning | A citation is accepted by the tolerant reader but is not canonical | `[CL-CMP-0011]` |
| FB0707 | error | Nothing in the answer verifies. The output says "Do not use this answer." and the command exits 1 | An answer whose every citation is broken |
| FB0708 | error | `new note` refuses a note whose text fails FB0400 to FB0403 when the note is bound for the repository Corpus. In a company store the note takes the store's default labels, and a marking in it is labelled, not refused | A note with a personal email address |
| FB0709 | error | A note's front matter is incomplete (no `question`, `reviewed` or `context_fingerprint`), its `context_fingerprint` is not 64 lowercase hex characters, or its `origin` is not `ai_drafted` (schema 2) | A note with no `question` |

The seed cases for `cite-check` include one answer with a good citation and a broken one. It exits 1 and prints "1 of 2 citations do not resolve".

### 08: audit, backup and pipeline

Raised by `audit verify`, `audit export`, `backup verify`, `restore`, `add`, `queue` and `sync`. FB0826 is raised by `check`. The audit and backup codes arrive at Day 1 and so does FB0821 (`add <folder>`); the other queue and pipeline codes arrive at Day 30. FB0824 is free. The known-bad case of each code is a file of its own (see [Known-bad files](#known-bad-files)).

| Code | Severity | Raised when | Known-bad example |
|---|---|---|---|
| FB0800 | error | The hash chain of the audit journal is broken at a line | One byte of an earlier line changed |
| FB0801 | error | An event has a kind or a field outside the vocabulary, a free-text value, or no `actor`, `role`, `store` or `via` | An event with a free-text field |
| FB0802 | error | The journal is shorter than the expected length, or its head differs from the expected head | Lines cut from the end of a journal that has a recorded head |
| FB0803 | warning | A write has a begin event and no end event, or a model call has an `ai.begin` and no `ai.call` of the same `tx` | A `write.begin` with no `write.end`; an `ai.begin` with no `ai.call` |
| FB0804 | warning | A torn last line was moved aside and recovered, or `audit verify --repair` set a broken journal aside as `journal.broken-<seq>` and started a new chain | A last line cut in half; a changed byte in an earlier line, repaired |
| FB0805 | warning | The recorded time goes backwards | An event dated earlier than the one before it |
| FB0806 | error | `audit export` cannot use its target: the policy has no `[audit] sink`, the folder is unusable, or a file there would be overwritten with different bytes | A target file that exists with other bytes |
| FB0807 | error | An `Authz` adapter denies the act. The `refuse` event carries a reason from a closed list | An adapter that denies `Export` to the actor |
| FB0808 | warning | A store recorded in the previous `journal.open` is not there. `store.missing` is appended | A store folder that was moved or deleted |
| FB0810 | error | A snapshot file is missing, or its bytes differ from `SHA256SUMS` | One byte of a snapshot file changed |
| FB0811 | error | `backup.toml` or the manifest is inconsistent | A manifest that lists a file the snapshot does not hold |
| FB0812 | error | A backup target is unusable, or lies inside a folder it backs up | A target inside the state folder |
| FB0813 | warning | A backup target is under a synced location and is not declared synced | A target in a synced folder with no `kind = "synced"` |
| FB0814 | warning | There is no verified backup yet, or the newest verified backup is older than `warn_after_days` (printed by `status`, `backup`, `backup verify` and `backup list`) | A snapshot with no `backup.verify`; a verified backup older than the limit |
| FB0815 | error | A restore is refused: the destination is not empty, or the snapshot failed to verify | A restore into a folder that holds a file |
| FB0816 | error | A restore drill failed: its check, journal or fingerprint comparison | A drill whose restored fingerprint differs |
| FB0820 | error | A proposal's file no longer matches its recorded hash | A file in the queue changed after it was proposed |
| FB0821 | error | A file type is not allowed as an original (executable or active content; a macro-enabled Office file is refused in the repository Corpus and stored inert in a company store) | A `.ps1` file in the folder given to `add` |
| FB0822 | error | An accept, reject, label change or typed public act is refused. An accept or reject: no terminal, or the declared role is not owner or lead. A label change: the program has no interactive console on both input and output, the typed confirmation does not name the number of items, the logon token is not in the store's `changers` group, or the label asked for is `public`, `unlabelled`, `marked` or `owner-supplied`. On the laptop preset the `role` word is ignored for a label change. A typed public act (`PublicAct::typed`) is refused when its confirmation is missing (a library check: home `add` or `new` without `--confirm-public` is FB0410) or the machine has a company store. On the laptop preset, a logon token that cannot be read. A denial by the `Authz` port is FB0807 | `queue accept` with standard input from a file; `label add` from a script |
| FB0823 | warning | A model suggestion was dropped: a value outside the vocabulary, or a quote or id not found | A suggested `source_kind` that is not in the vocabulary |
| FB0825 | warning | A proposal has no extraction, so its quotes cannot be checked | A proposal for a PDF with no text layer |
| FB0826 | error | A rule in `corpus/pipeline/rules.toml` cannot be read, has a repeated `id`, has no `example` or no `expect`, has a `set` value outside the vocabulary, or does not reproduce its own `example`. Raised by `check` | A rule with no `example` |
| FB0830 | error | `systems.toml` is invalid (an unknown key, including one in a stream `row` table, a secret-like key, a stream with no `versioning`, a repeated id, a `load` that names no type or a type with no `key`, a `currency` without `load`), or `sync` names something the layout cannot use | A stream with no `versioning` |
| FB0831 | warning | A pulled item, or a row of a loaded file, is not proposed: `older` (a version, or a record row, older than the one held), `duplicate` (the bytes are held under another key) or `unmatched` (the name does not fit the stream's template). `sync` prints it | A revision that predates the held version |
| FB0832 | warning | A cursor is unreadable or newer than the listing. The stream is listed again from the start | A cursor file with a bad line |
| FB0833 | error | A pulled key or path leaves its stream folder, or is empty | A symbolic link in a drop folder |
| FB0834 | error | A `Pull` adapter breaks its contract: it writes to its source, or lists in a different order twice. Raised by the contract test only | An adapter that renames a file in the drop folder |
| FB0835 | error | A stream folder lies outside the `root` of the policy `[[source]]` row that labels it | A drop folder outside the row's `root` |
| FB0836 | error | A restored store has not been adopted, so no id is issued | `sync` on a machine that holds a restored `overlay\` |

### 09: model, decide, index, output, MCP and chat

Raised at run time by `ask`, `decide`, `ai test`, `mcp`, `chat` and the adapters. They arrive with those verbs: Day 30, and Day 90 for `decide` and `chat`. An error here fails the verb that raised it, with the exit code of AD-17 (0 clean, 1 findings, 2 usage, 3 I/O or network), and a warning never changes the exit code. Exit 2 is usage; the 09 refusals that name Exit 2 below are usage-class (AD-17). The known-bad case of each code is a file of its own (see [Known-bad files](#known-bad-files)). Codes that need an endpoint run against `ScriptedHttp`.

| Code | Severity | Raised when | Known-bad example |
|---|---|---|---|
| FB0900 | error | Settings error in `[model]`, `[search]`, `[mcp]` or `[chat]`: unknown key, unknown adapter, secret-like key name, bad order. Exit 2 | A key named `api_key` under `[model]` |
| FB0901 | error | Labels refused: a label of the request is outside the destination's allowed set, or the request is `unlabelled` or `marked`. Exit 2 | A request labelled `np-0007` to an endpoint that allows `public` |
| FB0902 | error | A marking shape in an outbound prompt that the request's labels do not cover. Exit 2 | A question that contains a controlled-information banner |
| FB0903 | error | The audit journal could not be appended before sending; nothing was sent. Exit 3 | A journal folder that cannot be written |
| FB0904 | error | The audit journal could not be appended after the reply; the output was discarded. Exit 3 | A journal that fails after the send |
| FB0905 | error | The endpoint rejected the credentials. Exit 3 | A scripted 401 reply |
| FB0906 | error | The endpoint rejected the request, or redirected. Exit 3 | A scripted 404 reply, or a 3xx reply |
| FB0907 | error | Prompt too large for the endpoint or the limit. Exit 2 | A prompt over `max_prompt_chars` |
| FB0908 | error | The endpoint is unreachable or failing after retries. Exit 3 | Three scripted 503 replies |
| FB0909 | error | Reply unusable: cut off, or invalid after one repair. Exit 1 | A reply that fails the schema twice |
| FB0910 | error | The endpoint's certificate is not trusted by this machine. Exit 3 | A scripted certificate failure |
| FB0911 | error | The CPU lacks features the TLS provider needs. Exit 2 | A test that reports a missing CPU feature |
| FB0912 | error | This build has no network adapter, or `fieldbook chat` was run in a build without feature `bridge`. Exit 2 | A route to an endpoint on the silent build; `chat` on a build without `bridge` |
| FB0913 | error | `base_url` is neither https nor loopback. Exit 2 | An `http://` address on another host |
| FB0914 | error | The key variable is unset or empty. Exit 2 | A `key_env` that names an unset variable |
| FB0915 | error | The call budget for this run is used. Exit 2 | The call after `max_calls_per_run` |
| FB0916 | error | The Handoff ticket is missing or used, or the reply is empty. Exit 2 | `ask --resume` run twice for one ticket |
| FB0917 | error | An AI proposal's provenance does not resolve in the journal. Exit 1 | A proposal with a forged provenance reference |
| FB0918 | error | No approval for this destination: the policy has no row for it, the row has expired, the URL differs from the row's, or the client is not listed. Exit 2 | An endpoint whose row expired yesterday |
| FB0925 | error | A model call with a non-public label is refused because the policy requires a recent export (`require_fresh_export`, on unless IT turns it off) and the journal was not exported within `max_unshipped_hours`. Exit 2 | A call the day after the last `audit export` was a week old |
| FB0919 | error | Reply larger than the reply cap. Exit 3 | A reply over `max_reply_bytes` |
| FB0920 | error | Decision spec invalid. Exit 2 | A spec with one option |
| FB0921 | error | Decision pick not among the options; the reply is dropped. Exit 1 | A pick that names no option |
| FB0922 | error | Confidence out of range; the reply is dropped. Exit 1 | A confidence of 1200 |
| FB0923 | warning | Evidence dropped: an id not in the pool, or a quote not found. Reported in the decision report | A cite whose quote is not in the unit |
| FB0924 | warning | No evidence: the pick was forced to `unknown`. Reported in the decision report | A pick whose every cite was dropped |
| FB0930 | warning | An index adapter returned an id that is not in the Corpus; the id is dropped | A scripted index that returns `cmp-nope` |
| FB0931 | error | An output adapter produced different bytes twice. Raised by the contract test only | An adapter that writes a timestamp |
| FB0940 | error | MCP: malformed request. A JSON-RPC error | A request that is not JSON |
| FB0941 | error | MCP: unknown tool. A JSON-RPC error | A call to a tool that does not exist |
| FB0942 | error | MCP: a result carries a label the `mcp` destination does not allow, withheld. A tool error | A unit labelled `np-0007` asked for with only `public` allowed |
| FB0943 | error | MCP: invalid arguments. A tool error | A search call with no query |
| FB0944 | error | MCP: audit unavailable, the call refused. A tool error | A journal that cannot be appended during a call |
| FB0950 | error | The page cannot reach the Bridge: the process has gone. The page keeps its pack and offers Handoff | `fieldbook chat` killed while the page is open |
| FB0951 | error | A request has no token or a wrong one, a `Host` other than `127.0.0.1:<port>`, or an `Origin` that is neither absent nor the same origin. HTTP 403; nothing is sent to any model | `GET /api/status` with a foreign `Origin` |
| FB0952 | error | The Bridge was asked to bind any address except 127.0.0.1. No command line can ask for one; the guard in `bind` raises it. A listener on the company network is a separate build under its own decision record (the Later row `later-hosted-service`), not a setting of the Bridge. Exit 2 | A test asks for `0.0.0.0` |
| FB0953 | warning | `fieldbook chat` ended after `[chat] idle_minutes` with no request. Exit 0 | A test sets `idle_minutes = 1` and a fake clock |
| FB0954 | error | A second `fieldbook chat` on the same state folder. Exit 2 | Two sessions against `tests/fixtures/state-mini` |
| FB0955 | error | A request over 256 KiB (HTTP 413) or one that did not finish within 5 s (HTTP 408) | A 300 KiB body to `POST /api/send` |

### Known-bad files

For ranges 08 and 09 each code has `tests/known-bad/<CODE>.toml` with the fields `verb`, `args`, `state` and `expect`. `state` is a folder under `tests/fixtures/`, for instance `state-mini`, and `expect` is the code and the exit status. The harness runs the verb against a copy of that folder passed as `--state`. When the verb also finds the Corpus by walking up from the working directory, such as `add`, the harness runs it in a separate temporary copy of `tests/fixtures/corpus-mini`, so the case never runs in, or writes to, the real Corpus. A code that needs an endpoint runs against `ScriptedHttp`, and a code that needs a socket runs against a loopback test server in `xtask`. A code that no verb of the binary raises (a scripted index, a planted adapter, a page script) has `code` and `in_process = "<crate>/<test file>/<test name>"` in place of `verb`, `args`, `state` and `expect`; `tests/known_bad_pointers.rs` requires the named test to exist. A code that needs an endpoint may name `http = "http.toml"`, a file inside the case's fixture tree that scripts the endpoint, and the harness then runs the verb in process over `ScriptedHttp`. The test `every_code_has_a_known_bad_file` fails on a code of this catalogue with no file.

To add a check code, follow "Adding a check code" in [docs/cards/README.md](../cards/README.md#adding-a-check-code).

## Contract for the reader UI

This section lists what the reader and the build take from the Corpus and what the build emits. It adds no field and no rule. It restates AD-12, AD-13, AD-15 and AD-17 of [ADR 0001](../decisions/0001-foundation.md), and where the two differ the ADR governs and this section is corrected. The UI lane's design files ([docs/design](../design/README.md)) own layout, components and visual design. This file owns formats, tools and delivery (AD-15). On a conflict between them, the implementer stops and asks the owner.

### What the Corpus does not carry

The reader may look for these. None is in the Corpus, and the right column says what it uses.

| Not in the Corpus | Why | The reader uses |
|---|---|---|
| A front-matter `summary` | A page states its purpose once, in the Purpose part | The first sentence of the Purpose part for area lists and popovers; search snippets follow [search.md](search.md#document-units), using the first paragraph of the first section for a page lead |
| `review_by` or `claims` in front matter | Both are computed, and writing either is an error (FB0109) | `review_by` from `as_of`, the newest Change note date and `volatility`; `claims` from the page's citations |
| A start date for Day N | The build injects no date (AD-15) | A date the owner enters in the browser |
| Evidence labels PF, CS, OD, RR, INF and UNK | They are not Corpus values | `claim_kind`, `verification` and `data_origin`, as written |
| Evidence grades A to D | They are company-store values (FB0210). The Corpus holds E, F and G | `evidence_grade`, as written |
| Competitor and public program as object types | They are market pages in the subareas `peers` and `programs` | The views CPT and PGM |

### What the reader reads

- **Strings.** Every frozen string comes from `window.FB_STRINGS` in `assets/strings.js`. The reader does not carry a frozen literal of its own, and a test fails on one in the code.
- **Status chips.** A status chip has an icon and a word, so a status never rests on colour alone, and its word prints. The UI lane's design files choose the labels. The chip's title holds the raw vocabulary value, such as `primary_source_supported`.
- **Overdue banner.** The build writes the banner hidden, with the page's `review_by`. `app.js` shows it when the viewer's own date is past that day. The build never reads the clock. An unreleased build writes no such banner: its build status banner says each page shows its review date and none is flagged, and the meta row keeps the date as text.
- **Part order.** The reader draws the parts in the order of the page type. It adds nothing between them that the page does not hold, and an empty part shows `Not established.`
- **Page type blocks.** The reader adds a table, a numbered list or a print layout for the types that carry one (see [Page files](#page-files)). It adds no other content.
- **Links.** Where each token links is in [Syntax at a glance](#syntax-at-a-glance).

### Site output paths

One HTML file per page. A `#` is used only for anchors and for `search.html#q=`. The paths are those of AD-12; "Arrives" is the release whose edition first holds the file.

| Output path | Page | Arrives |
|---|---|---|
| `index.html` | Home: day-one list, briefs, areas, search, edition and as-of | Day 1 |
| `a/<area>.html` | Area by subarea and stage. A section without data shows its empty-state line | Day 1; views at Day 30 |
| `p/<page-id>.html` | Page in the frozen part order, with Sources built from the citations. The page context pack in `<details><pre>` is a Could item (card `page-context-packs`) | Day 1 |
| `s/<version-id>.html` | Source version: row, hash, the original at its locator, section anchors, citing claims, extraction status | Day 1 |
| `claims.html`, `claims/<AREA>-<part>.html` | The claims index, one entry per area part with its id range and count; and one register page per area part of 500 claim numbers, with an anchor per row | Day 1 |
| `sources.html`, `questions.html`, `conflicts.html`, `glossary.html` | Registers, from one generic template, with an anchor per row | Day 1 |
| `ids.html`, `ids/<slug>.html`, `ids/claims-<AREA>-<part>.html` | The id index: the seven id groups with their counts. One page per group but claims (`<slug>` is `pages`, `sources`, `versions`, `questions`, `conflicts` or `terms`), each holding every id of the group with its link; the claims group in one page per area part of 500 claim numbers, by the claims register's part rule, so no id-list page is over 3 MB | Day 1 |
| `search.html` | Results for `#q=` | Day 1 |
| `about.html` | Tool version, edition, commit, the label statement (an edition holds `public` only), status counts, stale and unreviewed items; the build profile (silent or AI) and the AI position as text. A built page cannot know the store, the policy or the connected company data sources: `fieldbook policy show` and `fieldbook sync --dry-run` print them on the laptop | Day 1 |
| `day-one.html` | The day-one binder: the `brief` pages and the pages of stages 1 and 2, ready to print | Day 1 |
| `changes.html` | `corpus/changes.md` plus the ids added since the previous edition (`build --since`) | `edition-002` |
| `assistant.html` | The Assistant: Handoff on a page opened from a file, the Bridge when served by `fieldbook chat` (AD-15) | Day 30 |
| `map.html` | The App map: layers, releases, the stores and their labels; the AI layer as text | Day 30 |
| `assets/app.css`, `app.js`, `search.js`, `search-index.js`, `strings.js` | Styles, scripts, the search index and the frozen strings | Day 1 |
| `assets/assist.js`, `assist-data.js` | The Handoff script and its data | Day 30 |

- **Content security.** Every page written to a Corpus or an edition carries one meta tag: `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'none'; form-action 'none'; base-uri 'none'`. That string is `CSP_STATIC`, a constant of `render::csp`. There is no `style=""` attribute, no inline event handler, and no `<a href="http:">` or `<a href="https:">`, except on `s/` to a listed official publisher over `https:` (FB0604). Every other external address is text with a copy button. The chat page that `fieldbook chat` serves from memory carries the same policy except `connect-src 'self'`; it is never written to a file (AD-15).
- **Links.** Directory links name `index.html`. An original links to the sibling `originals/` folder, with the locator text beside the link. A page works without JavaScript; the budgets are in FB0605.
- **Print.** `@page { size: Letter; margin: 0 }` with body padding. The footer shows product, edition, page id and "Check the current edition". Citations print as footnotes, and status words always print. No `file:` URL prints. A `brief` prints on one page (FB0607).
- **Header.** The edition and its build date are written into every page at build. Without `SOURCE_DATE_EPOCH` or without a commit the page says "unreleased build".
- **Build status.** On an unreleased build, the one the header chip says is unreleased, every page opens with a banner that says "This is an unreleased build, not an edition.", with the build's counts of AI-drafted and unchecked claims (and, once a claim is checked, of supported claims) and a link to the claims register. The build computes the counts from the claims register it renders (`BuildStatus` in `render::meta`) and writes the same numbers into every page; no page or program text holds a count. A released edition shows no banner.
- **The zips.** The layout of the edition zip and the originals zip is in the repository layout section of ADR 0001. The edition zip holds `corpus/` without originals and without `AGENTS.md`.

### Workbooks and templates

The Day-1 copies, `data/<register>.csv`, `data/claims-<AREA>-<part>.csv` and `data/registers.xlsx`, are in [Excel copies of the registers](#excel-copies-of-the-registers). The other workbooks follow AD-13.

| File | From | Content |
|---|---|---|
| `templates/discovery-log.xlsx`, `templates/assumption-register.xlsx`, `templates/90-day-plan.xlsx` | E1 | Data only, with validation lists. Each is an Excel table whose headers equal the company-store columns frozen with it. A Read-me sheet explains the "Save as CSV UTF-8" bridge to the company store and says to recalculate the workbook first, because a CSV holds the values displayed and no formula. The bridge is one connector of the `Pull` port (`file-drop@1`): the file goes into a drop folder and `sync` proposes it. The example rows are synthetic |
| `templates/decision-package.xlsx` | Day 30, after spike S2 | The only workbook with formulas. Its columns include the trace columns claim id, source version and locator |
| A rate register | On request | Public rates only. The company's own rate book and indirect-rate build-ups are record types of the company store, built after the knowledge base (the build order in ADR 0004) |

#### Template columns

The three E1 templates freeze these columns. They are not a Corpus object, so `schema_version` does not change. The header row of `corpus/templates/workbooks/<name>.csv` and the header cells of the Excel table are exactly this list, and `fieldbook build` refuses a CSV whose header differs. Every cell is a string. `owner_role` holds a role, never a person. `source_ref` is a key and a locator such as `S-0042-v2, p. 14`, or the words `No source yet`. A closed column has a drop-down list over its rows and 200 empty rows below them.

| Template | Columns, in order |
|---|---|
| `assumption-register` | `assumption_id`, `text`, `package`, `package_version`, `waits_on_question`, `owner_role`, `source_ref`, `as_of`, `confidence`, `evidence_grade`, `moves`, `revisit_when`, `status`, `drafted_by` |
| `discovery-log` | `row_id`, `kind`, `title`, `detail`, `source_ref`, `area`, `owner_role`, `waits_on_question`, `status`, `as_of`, `drafted_by` |
| `90-day-plan` | `row_id`, `window`, `item`, `duty`, `proof_test`, `owner_role`, `status`, `drafted_by` |

| Closed column | Values | Used by |
|---|---|---|
| `confidence` | `High`, `Medium`, `Low` | Assumption register |
| `evidence_grade` | `A` to `G`, the whole [`evidence_grade` vocabulary](#claim-vocabularies). An assumption may rest on a company value (`A` to `D`), and the Corpus list is the one a later record type can read | Assumption register |
| `kind` | `note`, `gap`, `system`, `clause` | Discovery log |
| `window` | `Day 1 to Day 30`, `Day 31 to Day 60`, `Day 61 to Day 90` | 90-day plan |
| `status` | `draft`, `in_review`, `approved`, `superseded` | Assumption register |
| `status` | `open`, `in_progress`, `done` | Discovery log, 90-day plan |
| `drafted_by` | `ai`, `owner` | All three |

The plan has no date column: the start date is entered in the browser and is never stored in the Corpus or committed. Each template has three example rows, marked `Synthetic example:` in `text`, `title` or `item`, with lower-case keys (`example-1`) and the synthetic date `2000-01-01` as their only date.

- **Writer.** The `rust_xlsxwriter` crate writes every workbook with a fixed creation time. A test reads `docProps/core.xml` back.
- **Decision package.** INDEX/MATCH lookups only. Every formula cell carries the value cached by `fieldbook-calc`, including the self-check cell, whose cached value is "OK". The file asks Excel to recalculate on load, and its Read-me explains the save prompt that follows. A header block holds the edition number, the tool version and the as-of date, so that a filled workbook says which edition it was built from. A unit test reads it back, as it reads `docProps/core.xml`. A zero denominator shows "n/a: zero denominator", never a masked blank. A unit test checks the XML parts: no macros, external links, web hyperlinks, DDE, volatile functions or dynamic arrays.
- **Preview label.** Until spike S2 passes, the decision package is labelled "preview". S2 opens a test workbook on the corporate laptop, as a downloaded file with the Mark-of-the-Web, Protected View included, and requires that it recalculate with no repair prompt.
- **Working copies.** A workbook the owner fills in lives in the `working\` folder of the data root (`%LOCALAPPDATA%\Fieldbook\working\` on the laptop), never inside `edition\`. The roots come from `Layout`; a synced or shared folder is not a live root (FB0415).

### Command chips

The site writes nothing and runs nothing. Where a change is needed, it shows the command as a chip: text with a Copy button. The chip strings, their argument shapes and the release each arrives in are the [Verb map](#strings). The flags of each command are in the generated `cli.md`, and the verbs are those of AD-17.

- **Form.** A chip is one command, exactly as the verb map gives it, with `fieldbook` first. It has no pipe, no redirection, no environment variable and no single quote, so it runs unchanged in cmd and in PowerShell Constrained Language Mode.
- **Placeholders.** The reader fills a placeholder only with an id, an area code in lowercase or a page type taken from the page it is on. A text placeholder such as `<text>` stays for the owner to type. Text from a page, a register or an answer never enters a command. One quoting function builds every command the site shows.
- **Tag.** Each chip carries "At home", or "R2 · laptop" if it exists only from R2.
- **Clipboard.** Copy puts the text on the clipboard. If the clipboard is refused, the command stays selectable. The command-line tool never reads or writes the clipboard.
- **Not yet built.** Where the object a chip acts on does not exist in the edition, the page shows the landing line (`not_available`) instead of the chip.

## Object types beyond v1

Version 1 freezes the page types, the five registers and `edition.toml`. Every later object type lands with a named release, an activation trigger and a `schema_version` bump (see [Scope and versioning](#scope-and-versioning)). Until it lands, the site shows the landing line `<Thing> is not available until <release or trigger>.` (key `not_available`), and no command creates it. Question ids in the last column are Corpus questions, defined with the Day-1 content.

| Object | Lands | Trigger, or where it lives meanwhile |
|---|---|---|
| `note` page type: `origin = "ai_drafted"`, `reviewed`, `question` and `context_fingerprint`. `new note` writes `reviewed = false` | Day 30 (schema 2) | Never a source for a claim |
| Overlay columns for assumptions, decisions and discovery notes (`drafted_by` is `ai` or `owner`) | E1 | [Frozen with the E1 templates](#template-columns) |
| Decision package, assumption and discovery records | R2, company store | Read from CSV saved from the templates, through the `file-drop@1` adapter of the `Pull` port |
| Audit request | R2, company store | Q-014 |
| Company process and procedure records | R3 | The first observed process |
| Module status | R3 | `modules.csv` |
| Competitor, competitor move | Not an object | Market pages in the subarea `peers`; Q-019 |
| Public program of record | Not an object | Market pages in the subarea `programs`; `programs.csv` once the owner names programs |
| Cycle | Not an object | Q-006 |
| Feed | Not an object | A stream of the `Pull` port whose policy row labels it `public` (the Later row `later-feeds`); its snapshots are sources with the origin columns and a `series` |
| Thread | Not an object | A saved answer is a note |
| History | Not an object | Generated from git |
| Pointer: a record that names a document or a system record and holds no copy | The company store, with the first record cards | One record form among others, for an item that stays in its system of record. Full records and synced copies are its peers |
| Rate set, indirect-rate build-up, vendor quote, estimate, bid cost build, price volume, project, company program record, compliance matrix | The company store, after the record cards, in the owner's order: the rate book and pricing first, then estimate at completion and earned value | Each is a block of the company's `types.toml` on the record envelope. Q-002 and the company's choice of system of record decide the order within a group |

- **The company store.** Company records carry the labels of the company's table in `handling`. They live in company storage, never in a source repository, an edition or git (ADR 0004). A context pack or a tool reply may carry a company label only when the company's policy lets that destination receive it. The record envelope, `types.toml` and the `Register` port are in [company-store.md](company-store.md); the columns of each record type are the company's own `types.toml`, not part of this file. Until a record module lands, the Corpus holds only the asks (`questions.csv`), never the answers.
- **Figure origin (R2).** Every nonempty money, decimal or integer field has immutable evidence with basis Quoted, Computed or Judgment, as [Financial work](financial-work.md#evidence-and-calculation-runs) defines. Quoted evidence resolves a stored cell, record field or located document quotation and agrees with its value and unit. Computed evidence names inputs and a matching stored calculation run even when no display `calc_run` column exists. Judgment evidence gives its rationale. The row's basis and `source_ref` are an authoring default and aggregate lineage, not substitutes for field evidence. A page cites one figure with the `field` locator. FB0380 and FB0381 are reserved in the [Check catalogue](#check-catalogue) and become active with the card `r2-records`.
- **Bid and proposal pricing.** Fieldbook computes and drafts bid and proposal pricing from the rate book and the bid cost builds, in the company store, once the rate book exists. A bid's cost build, price volume and estimate copy are records with an origin label on every figure and a version for every build. The company decides which estimating system is the system of record and who approves a price. Where the estimate stays in that system, the store holds a pointer to it beside the owner's own analysis. Fieldbook makes no claim that an output meets any rule; the company's contracts and finance owners decide that. The designs are in a later decision record.

## Compatible schema evolution

An optional CSV column is stamp-only only when the existing reader accepts both headers without reinterpreting values. Exact-header readers require an explicit migration step. Rehearsal checks old-header/new-reader fixtures and verifies every preserved value. Company record files use immutable type revisions, as described in [Financial work](financial-work.md#observations-and-revisions), and never rewrite old rows to add a column.

Note pages at schema 2 persist `inputs = [{ store, id }]` as sorted ItemRef values, including an empty list when there were no inputs. Their effective labels include those dependencies. This field is required for AI-drafted notes and belongs to no other page type. The note planner and schema parser share its round-trip fixture.

CSV extraction bounds both original bytes and total emitted UTF-8 to 32 MiB. Count repeated headers, cells, separators and line endings before allocation; an excess produces no partial extraction.

## Additive input formats

Adding an extraction method or locator kind needs no format amendment or Corpus schema bump. Existing names and meanings never change. The fixed extraction envelope reserves optional `[method_data]` for new method-specific keys, validated by that method’s registered key schema. Existing root keys keep their location and meaning. Unknown method metadata is retained as unavailable by a reader that lacks that method, never interpreted as verified extraction. Existing methods still reject unknown keys. The locator registry likewise preserves unknown extension data as unavailable; only an implemented resolver can establish a match. `methods_only_grow` checks old rows and their meanings, permits appended rows, and does not require an older build to understand a future row.

| Input | Route in the current plan | Original |
|---|---|---|
| CSV | `csv-rows`, then supplied text if needed | Kept byte-exact |
| xlsx | `xlsx-sheets`, immediately after CSV; reads stored cell values directly | Kept byte-exact |
| xls | Supplied text, or a separately retained converted xlsx/CSV extraction input | Kept byte-exact; conversion never replaces it |
| XML, JSON, YAML | Supplied text; the structured-path extension below is Later | Kept byte-exact |
| Text PDF | `pdftotext-layout` at home, or supplied text | Kept byte-exact |
| Scanned PDF | Supplied transcription or OCR text; unavailable text is never verified | Kept byte-exact |
| Email (.eml, .msg) | Supplied body text; each attachment follows its own type’s route | Message and attachments each kept byte-exact |
| doc, docx | Supplied text | Kept byte-exact |
| txt, md | `plain`, or supplied text if decoding fails | Kept byte-exact |
| Other types | Supplied text with the unchanged original | Kept byte-exact |

“Supplied” means `cargo xtask extract --source VERSION --text FILE` at home or the laptop’s supplied-text intake when built. The home command attaches text only to an already admitted version, without replacing its original. It is a documented current route, not a claim that every extractor already exists. Supplied text retains its warning and can support a checked quote only under the existing evidence rules.

| Reserved locator kind | Stage | Meaning |
|---|---|---|
| `structured_path` (`ext:structured_path` in claims) | Later | A typed selector: JSON uses JSON Pointer; XML uses a namespace-bound element/attribute path; YAML uses a normalized mapping/sequence path. The implementing method pins escaping, namespaces and duplicate-key behavior and supplies round-trip and missing-path tests. No unresolved selector verifies a claim |
| `email_part` (`ext:email_part` in claims) | Later | A stable MIME part identifier within a byte-exact message. The body is one source version; each attachment is its own source, linked by a source-row `note` naming the parent message version and part identifier until the typed container resolver lands; its future `[method_data]` retains that version, original hash and part identifier. Part IDs refer to stored message bytes, never a mutable mailbox position |

A new input method follows the single extraction-method recipe. Its fixture retains the original, its resolver validates the new locator and an unsupported-reader test keeps the locator unavailable. Adding a type does not reopen the frozen meanings of existing evidence.

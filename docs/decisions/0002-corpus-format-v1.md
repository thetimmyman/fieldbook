# ADR 0002 Corpus format v1

Status: Accepted (F0; amended before edition-001 for five-digit claim continuation and the review date basis)

## Context

The lead writes the Day-1 content before the toolkit exists (H8 in [ADR 0001](0001-foundation.md)). That is safe only if the format is frozen first. Every page, register row and id written now must stay valid when `check`, `build` and `search` arrive. This record freezes the format at F0, before any content pull request, and it merges before ADR 0001. It decides AD-09 (the Corpus formats and the schema source of truth) and AD-10 (ids, citations and frozen strings), which ADR 0001 leaves to it. [ADR 0004](0004-company-boundary.md) is accepted with this record. It adds what company data needs from the format (a list of label ids in `handling`, the origin and classification columns, the `field` locator, company id shapes, and table extraction), and because this record is still Proposed those changes are made here, before the freeze, and not as amendments.

## Decision

**[corpus-format.md](../reference/corpus-format.md) is the specification. The file at the commit that accepts this record is the frozen version 1 specification.** The summary below is not a second copy: where it and the specification differ, the specification governs and this record is corrected.

| AD | Choice | Why | Risk and mitigation |
|---|---|---|---|
| **AD-09 Formats** | Pages are Markdown with TOML front matter between `+++` fences. The five registers are RFC 4180 CSV files in `corpus/registers/`. The sources register carries a `purpose` column (column 23), three origin columns (24 to 26: `origin_system`, `origin_stream`, `origin_key`) and two classification columns (27 and 28: `doc_kind`, `series`), all optional and all appended before the freeze, so every other column keeps its number. `purpose` is one line saying which open question or job the source answers. The origin columns say which company system and stream a source was pulled from, and under which key; they are empty for a source added by hand. `doc_kind` says what kind of document the source is (a contract, a quote, a rate package and the like) and `series` which run of exports of one document it belongs to. Both are empty until `sync` or a person sets them, and they are empty for every source in the repository Corpus. Originals are byte-exact files with a SHA-256: public files in the Corpus, and company files in a company store. `handling` is a set of label ids from the company's label table, written as an array in front matter and as a `|` list in a register cell. In the Corpus its only value is `public`; a company store holds the labels its policy lists ([ADR 0004](0004-company-boundary.md)). The serde types in `fieldbook-core` are the one schema, read with `deny_unknown_fields`. `schema_version = 1` is set in `corpus/corpus.toml`. The 13 areas and the 3 views are data in that file. A company store has its own `corpus.toml`, and its area codes start with `x`. Version 1 has nine page types: concept, procedure, reference, case, clause-card, calc, checklist, template and brief. `note` joins at Day 30 and makes the version 2. Every later object type gets a release, a trigger and a schema bump. | A real parser with typed dates. CSV opens in Excel. One schema source. One owner per topic. | **Risk:** an Excel round trip damages a CSV. **Mitigation:** Excel gets copies; `check` names the fault; `fmt` restores the form. |
| **AD-10 Ids, citations, strings** | Ids: page `cmp-withholding`, source `S-0042`, version `S-0042-v2`, claim `CL-CMP-0011` (four digits, or five from 10000 once an area reaches 9999), question `Q-025`, conflict `XC-14`, term `material_weakness`, error `FB0102`, edition `edition-007`, tool `v0.2.0`. A company store adds shapes with six to ten digits that cannot collide with a public id or with another writer's ids: source `SO-000042`, claim `CL-XES-000012`, question `QO-000003`, conflict `XO-000002`, record `CM-000012-v2`. Its areas start with `x` and its glossary terms with `x_`. Citations follow the Pandoc style: `[@S-0042-v2, p. 14]`, `[@CL-CMP-0011]`, `[@Q-027]`, `[@XC-14]`, several joined by `; `. In a company store a citation may also name one figure of a record: `[@CM-000012-v3, field burden_eng]`. The writer is strict, and a tolerant reader serves pasted answers only. Every frozen string lives in `fieldbook-core::strings`, and `build` writes it to `assets/strings.js`. The Assistant page's eight labels (Ask, Send, Stop, Copy context pack, Check citations, Save as note, Copy as draft note, This session only) are the key `assistant_labels` of the other fixed wording; the render crate writes them into the page and no script holds one. | A documented syntax that parses through `pulldown-cmark`. One source for every string the reader and the tool show. | **Risk:** a mistyped key, or the reader and the tool drifting apart. **Mitigation:** `check` resolves every key; a CI test parses every command chip; a test fails on a frozen literal outside `strings`. |

**Also frozen:** the nine templates in `corpus/templates/`, the rule files `corpus/ASSISTANT.md` (`rules_version = 1`) and `corpus/AGENTS.md`, the check codes `FB0100` to `FB0709` and what each means, and the layout of `corpus/`.

**Decided in the specification where ADR 0001 is silent.** The owner confirms these by accepting this record.

| Topic | Decision | Section |
|---|---|---|
| Grading | A claim keeps verification, applicability, adoption and data origin as separate fields. No score combines them. | [Status vocabularies and promotion rules](../reference/corpus-format.md#status-vocabularies-and-promotion-rules) |
| Promotion | A new claim enters `unchecked`. Codex never raises a status. The lead may verify a claim that is not consequential. Only the owner verifies a consequential claim. | Same |
| Overlay-only values | A value that records the company's own confirmation or approval is refused in the Corpus. | Same |
| Reports | A research report or AI-generated text is never a source. A statement from one is re-sourced to a primary document. | [Identifiers](../reference/corpus-format.md#identifiers) |
| Ids | An id is never reused or renamed from the first edition that contains it. Public and company ids have different shapes and separate number spaces: a company id has six to ten digits (the writer's digits, then six sequence digits) and is allocated only inside company storage, so it cannot collide with an edition id or with another writer's id. | Same |
| Review dates | V1, V2 and V3 mean 90, 180 and 365 days. `review_by` is computed, never written. | [Dates, supersession, handling and labels](../reference/corpus-format.md#dates-supersession-handling-and-labels) |
| Quotes | A consequential claim carries a quote of at most 300 characters. The quote match warns on Day 1 and is an error for a consequential claim from Day 30. The number-unit lint warns on Day 1 and is an error from Day 30. | [Check catalogue](../reference/corpus-format.md#check-catalogue) |
| Scans | `check` refuses markings, personal identity and claim language in public files. It never classifies. Allowances are keyed by SHA-256 and apply to the repository Corpus only. On company intake a marking becomes a label and is not refused, and a company store keeps no allowance file in git ([Handling and labels](../reference/corpus-format.md#handling-and-labels)). | [Scans](../reference/corpus-format.md#scans) |
| Strict rules | `check --strict` turns three warnings into errors: FB0118, FB0230 and FB0320. | Check catalogue |
| `applies_to` | A claim's `applies_to` is prose, or pairs `key=value` joined by `; ` with the keys `event`, `entered_from`, `entered_to`, `contract_type`, `fiscal_year`, `calendar` and `basis`. The key `event` says which date the rule is keyed on, so a rule is found by the date it applies to. A cell with no `=` is prose and is not read; a cell with `=` that breaks the form is FB0204. | [claims.csv](../reference/corpus-format.md#claimscsv) |
| Citing a record figure | In a company store a citation may end with `field <column>`, as in `[@CM-000012-v3, field burden_eng]`. The claim row has `locator_kind = record_field`, `source_id` is the record id and `version` is the record's version. The quote is the cell's text. The repository Corpus refuses it (FB0210). | [Pointers and figure origin](../reference/company-store.md#pointers-and-figure-origin) |
| Extraction | `extraction.toml` records the tool, the hashes and the section locators. CI checks the hashes and never re-extracts. The extraction method is one of `pdftotext-layout`, `html-strip`, `plain`, `csv-rows`, `xlsx-sheets` and `supplied` (the list only grows, and the `extraction.toml` section of the specification is the current one); `csv-rows` splits a table into fixed windows of rows with the header repeated, so that a `rec N` locator resolves (FB0311); `xlsx-sheets` reads the stored cell values of a `.xlsx` or `.xlsm` workbook and cuts each sheet into windows the same way, so that a `sheet` locator and a sheet-qualified `rec` locator resolve (FB0311); `supplied` (text a person supplied for a version the program could not extract) is written by the home extraction fallback and by laptop intake at R2, and a supplied extraction is not checked against the original (FB0223). | [Sources](../reference/corpus-format.md#sources) |

## Consequences

**What this gives.**
- The lead writes the Day-1 content against a fixed target. The first `check` run (`check-command`) validates content that already exists.
- Codex builds the serde types, `strings`, the checks and the site from one file. Tests tie the file to the code: `errors.md` equals the check catalogue, the Strings section is generated, and each known-bad example is a negative fixture.
- A citation or an id written now still resolves in every later edition.

**What it costs.**
- The freeze comes early. A mistake found after content exists costs a migration (see Scope and versioning in the specification).
- Until the toolkit lands, the lead checks pages by eye against the templates, and `check` cannot yet catch an error.
- Version 1 holds what R1 needs and the seams that company data needs: the label list in `handling`, the origin and classification columns, the `field` locator, the company id shapes and table extraction ([ADR 0004](0004-company-boundary.md)). The record envelope is fixed by ADR 0004, and a new record type is a block in the company store's `types.toml` with no schema bump. Processes, bids and the other work objects wait for their release; a new page type or register column is still a schema bump.
- The company-side format edits cost nothing now. After the freeze each of them would be a breaking change with a migration.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| YAML front matter | Typing surprises (`no`, `1.0`, bare dates) and indentation faults. TOML has typed dates and maps onto the serde types without ambiguity. |
| TOML or JSON registers | The owner cannot edit them in Excel. |
| A separate schema file, such as JSON Schema | Two sources drift. The serde types are the schema, and `check` is their test. |
| A byte-order mark in the source CSV | It makes diffs noisy and breaks plain tools. The Excel copies get one. |
| Wiki links, `{CL-CMP-0012}` and `[S-0042@v2 p.14]` | Only the Pandoc style parses through `pulldown-cmark`. Page links stay plain relative `.md` links, which any editor previews. |
| CPY, CPT and PGM as areas | One owner per topic. They are views over Markets. |
| Writing `review_by` and `claims` in front matter | They go stale. The build computes both. |
| Ids that carry a status or a date | The meaning changes and the id cannot. |
| Freezing every object type now | Version 1 needs only pages, registers and `edition.toml`. |
| Freezing after the content is written | Every page and row would need a migration. |
| `handling` as a closed pair, `public` or `org` | Two values cannot tell a vendor quote under a non-disclosure agreement from a controlled file. An open label table keeps the format fixed and lets the company define the rest. |
| One number space for the Corpus and the company store | Ids would collide the first time two writers or two devices allocate. Separate shapes and writer digits make a collision impossible by construction. |

## Reopen trigger

A change to the frozen format is a new ADR that supersedes or amends this one, names the trigger that fired, updates the specification and raises `schema_version` (the format gate). Any of these fires it:

- Content needs something the format cannot say, and the same workaround appears in three content pull requests [J].
- A check or scan refuses public text it should accept, and `scan-allow.toml` cannot express the exception.
- IT or the company requires a marking or a field that the format, the label table or the company store cannot carry. (This fired before the freeze for labels, company ids, origin and classification columns, the `field` locator and table extraction. [ADR 0004](0004-company-boundary.md) applied the change in this record.)
- A dependency change leaves the format unreadable.

## Amendment: five-digit claim continuation (pre-edition-001)

- **Trigger:** Content needs something the format cannot say (reopen trigger 1): the compliance (`CMP`) claim series reached `CL-CMP-9999` during Day-1 public clause-card drafting while compliance sections remain to be cited.
- **Decision:** A public claim series continues with five digits from `10000` to `99999` (`^CL-[A-Z]{3}-([0-9]{4}|[1-9][0-9]{4})$`, with no leading zero in five-digit numbers so every number has one canonical spelling) in both the Identifiers table and the citation grammar, and `claims.csv` sorts by area and then by claim number as an integer (`CL-CMP-9999` before `CL-CMP-10000`). Company claims (`^CL-X[A-Z]{2}-[0-9]{6,10}$`) remain disjoint by area prefix and digit width.
- **Versioning and migration:** Every existing four-digit `CL-AAA-0001` to `CL-AAA-9999` claim id remains valid and keeps the identical relative order under `(area, number)` as under byte order. Because neither `edition-001` nor any `fieldbook-core` identifier, CSV or citation reader has landed yet, this amendment is folded into `schema_version = 1` before the first reader or edition ships, and no existing register row or page citation needs rewriting.

## Amendment: review date basis (pre-edition-001)

- **Trigger:** None of the listed triggers fits exactly, so the owner's acceptance also decides that this record may be amended for it. The computed meaning of `review_by` misleads readers: it counted a page's interval from `as_of`, "the date the page's content describes", so `FB0117` marks a page overdue the day it is written whenever the page describes an older source. At the commit date, 665 of 1,709 pages raise `FB0117`; under this amendment 6 do (`fieldbook check --strict --today` the commit date: 994 warnings before, 335 after). The six are the six `brief` pages, which have no Change note part and keep counting from `as_of`.
- **Decision:** A page's `review_by` is the later of its `as_of` and the date of the newest valid entry in its `## Change note` part, plus the interval of its volatility. A valid entry has a known change class, a real calendar date and a sentence; an entry that is not valid is not read (`FB0114` and `FB0137` still report it), and a page with no valid entry counts from its `as_of`. The order of the entries is not read. The new rule reads only what the page already holds: no file is rewritten and no field is added. A claim's `review_by`, `FB0221` and `verify` are unchanged.
- **Why the Change note:** `review_by` answers "when should a person look at this page again". The last time a person or an agent changed the page is its Change note: every page of the eight-part types has one, its oldest entry is `NEW_KNOWLEDGE` (`FB0137`) and each entry carries a date (`FB0114`). A page nobody has touched stays overdue, and a page of the eight-part types written today from an older source is not overdue at birth.
- **Schema impact:** Breaking by the definition in [Scope and versioning](../reference/corpus-format.md#scope-and-versioning), because a meaning changes: the same page can have an earlier `review_by` before this amendment than after it. No valid file needs rewriting, and no value that a Corpus holds changes except the computed `review_by`.
- **Versioning and migration:** As for the five-digit claim continuation, this amendment is folded into `schema_version = 1` before `edition-001`: no edition, tag or release has shipped, so no reader of an earlier meaning exists. The alternative, a bump to `schema_version = 2`, would refuse every Corpus the tool reads today for a computed date; it is not needed until an edition has shipped under the earlier meaning.

## Additive input support

New extraction methods and locator kinds are additive extensions and need no format amendment or Corpus schema bump. Existing names, meanings and stored evidence never change. The common extraction envelope stays fixed; method-specific metadata is validated against that method’s registered key schema. An older build reports an unsupported method or locator as unavailable and never treats it as verified. Page types and register-column changes keep their existing versioning rules.

Structured paths into JSON, XML and YAML and email-part locators are reserved additive kinds, described with their Later rows and current supplied-text route in the input-format reference. An email body is one source version; each attachment is a separate source linked to that message. Every original remains byte-exact.

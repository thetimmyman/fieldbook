# The company store

This is the reference for the company store of [ADR 0004](../decisions/0004-company-boundary.md): its folder, the files that describe it, how data reaches it from the company's data sources, how typed records are kept and which checks run on it. The labels and the policy that decide what it may hold are in [policy.md](policy.md) and [Handling and labels](corpus-format.md#handling-and-labels). Every id, name and value in the examples is synthetic.

Cards that build it: `core-labels` (store admission, the label ledger), `r2-overlay` (empty store initialization and the local site), `r2-company-ids` (`store.toml`, `init --overlay --writer`, the id families, the company index), `extract-csv-rows` (the `csv-rows` method), `extract-xlsx-sheets` (the `xlsx-sheets` method), `r2-pull-and-sync` (`systems.toml`, `file-drop@1`, `sync`), `r2-records` (`types.toml`, record files, the `Register` port) and `it-company-story` (what IT is told).

## What it is

The company store is the folder `overlay\` under the data root on the laptop. It uses the formats of the Corpus ([Corpus format](corpus-format.md)) with the differences below, so that every verb, check and view that reads a Corpus reads it. It holds what the company's policy lets it hold, under labels. The source repositories never hold any of it.

| Matter | Repository Corpus | Company store |
|---|---|---|
| Store id | `corpus` | `overlay` |
| Holds | `public` only | What `[[store]]` lists for `overlay`; with no policy, `public`, `unlabelled` and `marked`. Never `owner-supplied`, which is request-only |
| Ids | Four-digit shapes (`S-0042`, `CL-CMP-0011`) | Six to ten digits (`SO-000042`, `CL-XES-000012`) |
| Areas and terms | Any code but one starting with `x` | A code starting with `x`; terms start with `x_` |
| Source kinds | The public vocabulary | Adds `vendor_quote`, `company_record` and `company_document`, with no authority tier |
| Originals | `sources/public/` | `sources/org/` |
| Marking scan | FB0400 refuses a banner; `scan-allow.toml` allows a public document that prints one | A marking becomes a label; no `scan-allow.toml` (Later row `later-banner-allowance`) |
| Label changes | `handling` is edited in the file | `handling` is never rewritten. A change is a line in `labels.csv` ([below](#the-label-ledger)) |
| Records | None | Typed record files, below |
| Where a git work tree may hold it | Yes, if `public` | Never (FB0415) |

## Layout

```text
%LOCALAPPDATA%\Fieldbook\
  edition\            public. The shipped edition. Never edited on the laptop
  originals\          the owner's local copies of originals
  working\            the owner's notes and drafts
  overlay\            the company store (content root on the laptop)
    corpus.toml       areas: every code starts with x
    store.toml        store_uid, writer, restored_from
    labels.csv        the label ledger, append only
    systems.toml      systems and streams
    types.toml        record types
    registers\        the five registers, company ids only
    records\          one CSV per record type
    pages\            pages of the company areas
    sources\org\      originals, named <version-id>.<ext>
    sources\text\<version-id>\    extraction.toml and the text files
    evidence\         per-field evidence, <version-id>.toml
    runs\             durable runs, <full-sha256>.toml
    observations\<system>\<stream>\    <source-version>.toml
  state\              inbox, staging, queue, journal, settings.toml, backup.toml
    sync\<system>\<stream>.toml        cursors
    drop\<system>\<stream>\            default drop folders
    site\overlay\                      derived: the company site and its index, built on demand
```

- The live store, the state folder and every drop folder may not lie inside a git work tree (FB0415). The test is on the folder and not on its name: the live store and the state folder must be local, with no reparse point and no sync root; a drop folder may be on the network but is not a sync root. A company share is valid as a drop source, which is read only, and as a backup target. It is not a live store. The journal and its lock stay on local disk.
- Fieldbook marks new store folders as not content-indexed, and `check` fails when a store folder lacks the mark (FB0420), because a copy, a restore or some tools drop it. It adds no encryption of its own: device encryption and access control are the company's.
- Backup covers the new files, because `layout.roots()` lists the roots and these files sit under two of them. It labels a snapshot by exclusion: a file that fits no known kind adds `unlabelled` (ADR 0004, section 6). `r2-records` registers evidence, runs, observations and every record revision as known kinds with their current effective input labels; none is a label-free control. The derived `site\` folder is not copied.
- A restore onto a second machine while the first is still in use would give two stores that issue the same id for good, so a restored store does not allocate ids until it is adopted ([below](#storetoml)).
- A store row that names `readers` is not opened for a person outside that group. No search, index, site or pack is built from it for them. The company site and its index are not built for such a store.
- Existence is metadata. A refusal names no label and no title.

`fieldbook init --overlay [--writer N]` creates the store. It writes `store.toml` with a random `store_uid`, an empty `corpus.toml`, empty registers with their headers and the empty folders. It first resumes matching initialization recovery using the original persisted store UID and transaction; fresh initialization refuses a completed store, an unrelated pending operation, a nonempty unrecognized folder or a location that FB0415 refuses.

## `store.toml`

```toml
store_uid = "0f3c9a52-7d1e-4b86-a3c4-5e2f19d8b7a0"
writer = 0
```

| Key | Rule |
|---|---|
| `store_uid` | A random UUID minted by `init --overlay`. It lives with the data, so a restored store keeps it. `journal.open` lists the `store_uid` of every store the run opened, and the manifest of `audit export` carries it, so exports from many laptops are attributable. It is never a path |
| `writer` | An integer from 0 to 9999, default 0. It is the first digits of every company id this store allocates ([Identifiers](corpus-format.md#identifiers)): writer 0 gives `SO-000042`, writer 1 gives `SO-1000042`. `journal.open` records it |
| `restored_from` | Written by `restore --to`: the `store_uid` of the store the files were restored from. Absent in a store that was never restored |

The parser denies unknown keys. A company store with no `store.toml`, or one that does not parse, is FB0100. Allocation reads only the rows of that family and writer, so the overlay and the edition no longer share a number space.

**Adopting a restored store.** A store whose `store.toml` has `restored_from` allocates no id (FB0836) until `fieldbook init --overlay --adopt` records one of two things: `--retired`, which says the old machine is retired and keeps the writer, or `--writer N`, which gives this store a new writer. The adopt step removes `restored_from`, and it appends a journal event. The failure this prevents: restoring onto a new laptop while the old one still runs gives two stores that both issue `SO-000043`, and ids are never reused or renamed, so the collision cannot be repaired. The program has this guard because a line in a runbook can be missed.

## `systems.toml`

The file names the company data sources and their streams. It has a closed key set (FB0830) and holds no secret and no label: the owner edits it, and only the company's policy says what a source may hold and where it may read from.

```toml
systems_schema = 1

[[system]]
id = "finance-exports"
adapter = "file-drop@1"
scope = "monthly commitments exports"
note = "Saved by the finance office as CSV UTF-8"

[[system.stream]]
id = "commitments"
versioning = "vintage"
name = "{key}-{as_of}"
as_of = "name"
load = "commitment"

[[system.stream]]
id = "vendor-quotes"
versioning = "revision"
name = "{key}"
as_of = "retrieved"
path = "D:\\Company\\Quotes"
```

| Key | Where | Rule |
|---|---|---|
| `systems_schema` | top | `1`, required |
| `id` | `[[system]]` | `[a-z][a-z0-9-]{0,31}`. The id the policy's `[[source]]` table uses. The row's `root` binds the id to a folder tree: a stream whose folder lies outside it is refused by `sync` (FB0835). A system with no `[[source]]` row in a loaded policy labels its items `unlabelled` |
| `adapter` | `[[system]]` | A `Pull` adapter id with its version (`file-drop@1`) that is a `Built` row of the registry |
| `scope`, `note` | `[[system]]` | Text, at most 200 code points each. Shown by `sync --dry-run` |
| `id` | `[[system.stream]]` | `[a-z][a-z0-9-]{0,31}`, unique within the system |
| `versioning` | stream | `"revision"` or `"vintage"`. **Required, no default.** A revision stream makes new bytes for a known key a new version of the same source. A vintage stream makes each dated file its own source in a series, and a late vintage is valid |
| `name` | stream | A template matched against the file name without its last extension. It holds literal text, `{key}` (required) and `{as_of}`. `{as_of}` matches `YYYY-MM-DD`, `YYYYMMDD` or `YYYY-MM`. `{key}` takes the shortest run of one or more characters that lets the rest match. The matcher is a pure function with no regex crate |
| `as_of` | stream | `"name"` (the template must hold `{as_of}`) or `"retrieved"` (it must not; the date is the day the file was fetched; only a `revision` stream may use it). **Required, no default** |
| `path` | stream | Optional. An absolute folder (a drive or a UNC share path). Default `state\drop\<system>\<stream>\`. A relative path is FB0830. A symbolic link, a junction, `..` or an empty key is refused (FB0833) |
| `load` | stream | Optional. A record type name from `types.toml` that has a `key`. The stream's files become records ([below](#loading-a-stream-into-records)). The file is a CSV with commas, as the `csv-rows` method reads it. A name that is not a defined type, or a type with no `key`, is FB0830 |
| `mode`, `scope`, `period`, `event_key`, `control_totals` | stream | With load only; observation semantics and required keys are in [Financial work](financial-work.md#observations-and-revisions). Independent of source versioning |
| `currency` | stream | Optional, with `load` only: three capitals. The currency of every `money` cell of a loaded file whose header has no `<name>_ccy` cell. A file that has the cell uses the cell |
| `workbook` | stream | Optional boolean, default false. `true` says the CSV files of the stream were saved from a workbook, so their cells are displayed values only. A `.xlsx` or `.xlsm` file in the stream is read as a workbook by `xlsx-sheets` and needs no mark, because its extraction says what it knows in `flags`. `sync` then writes `values_only = true` into the `extraction.toml` of each CSV source version it proposes from the stream, and the source page, the context pack and the review queue say "values only (saved from a workbook)" beside that source. A value that is not a boolean is FB0830 |
| `[system.stream.row]` | stream | Optional table of the cells every item of the stream shares: `source_kind` (`vendor_quote`, `company_record` or `company_document`), `publisher`, `licence`, `volatility` (`V1`, `V2` or `V3`), `tag`, `purpose` (text cells, the limits of `sources.csv`), `doc_kind` (a word of the vocabulary of the same name in [sources.csv](corpus-format.md#sourcescsv); absent means not yet classified) and `title` (a template of `{key}` and `{as_of}`, default `{key}`). `sync` writes each into the proposal as a value from the stream (`by = origin`); a classifier cannot replace it, and a person can edit it at review. `series` is not a key: `sync` writes the stream id into the `series` cell of each source of a vintage stream and leaves it empty in a revision stream. A key outside this list is FB0830 |

A network adapter later names its credential by a name in the Windows credential store in the same table; the file never holds the value, and the environment variable is for the home preset only.

## `sync`

`fieldbook sync [--system ID] [--stream ID] [--full] [--dry-run] [--json]` is stage 0 of the intake pipeline. It writes proposals only and nothing under the content root.

1. It reads `state\sync\<system>\<stream>.toml` for the cursor and lists from it. `--full` ignores the cursor and lists the stream from the start, which is how a stream is checked again after a restore; the hash and key rules make that safe. A lost or unreadable cursor costs one full re-list (FB0832).
2. It plans each item with a pure function, below.
3. It fetches the items that need proposals, copies each to `state\inbox\<system>\<stream>\`, runs the file-type allow-list (FB0821) and the labeller, and runs the same pipeline as `add --propose`.
4. It appends `sync.begin` (with optional `root_sha256` and the policy hash, as [the audit contract](audit-contract.md#source-root-fingerprint) defines) and `sync.end` to the journal and updates the cursor. Before step 1, it checks that the stream's folder lies inside the `root` of the policy's `[[source]]` row (FB0835).

| Outcome | When | Result |
|---|---|---|
| `Present` | The hash is held under this key, by a waiting or rejected proposal, or by a source added by hand | Nothing written |
| `NewSource` | Unseen key. In a vintage stream the key includes the `as_of`, so a late vintage is a new source | A `new_source` proposal |
| `NewVersion` | Known key, new bytes, not older | A `new_version` proposal |
| `Older` | Revision stream and `as_of` earlier than the current version's | Reported, not proposed (FB0831) |
| `Duplicate` | The hash is held under another key | Reported, not proposed (FB0831) |
| `Unmatched` | The name does not fit the stream's template | Reported, not proposed (FB0831) |

The `origin_key` of a pulled source is the `{key}` of the name in a revision stream and `{key}@{as_of}` in a vintage stream, with the date written `YYYY-MM-DD` or `YYYY-MM` as the name gives it, so `20260930` and `2026-09-30` name the same source. A month stays a month: the tool never fills in the missing parts of a date. A file over 64 MB, or one that changed after it was listed, is skipped for the run with a plain line and counted in `skipped`; a subfolder is not read, so a stream has one folder; a link is refused (FB0833). A stream with `load` builds a batch of records from each new file ([below](#loading-a-stream-into-records)).

`Older` has a concrete failure behind it: someone drops last month's file into a share again, and without the rule it would supersede this month's.

A proposal is state, not evidence. `queue accept` stays interactive, in a terminal, with the owner or lead role. `queue accept --all --system S [--stream T]` accepts every waiting proposal of one stream after one interactive confirmation that lists them; each is still re-checked (FB0820) and journaled on its own. `sync --dry-run` prints the store, the policy and the sources it would use, then for each file its key, `as_of` and planned outcome, and writes nothing, so a wrong name template shows on the first run. Fieldbook installs no scheduler. A task the company already runs may call the verb.

Incremental by cursor, idempotent by hash and key: a damaged cursor cannot cause a duplicate. `fetch` never writes to the source. `file-drop@1` never moves or deletes a file in a drop folder.

## Provenance and extraction

- `sources.csv` has five more columns after `purpose`. `origin_system`, `origin_stream` and `origin_key` (columns 24 to 26) are empty for a source added by hand; only a puller or a person writes them, never a model or a rule (FB0215). `doc_kind` and `series` (columns 27 and 28) say what the file is and which vintage stream it belongs to. `sync` sets `doc_kind` from the stream's `row` table and `series` from the stream id, a person may change either at review, and both are empty until then. The classifier may suggest `doc_kind` (one `rules.toml` row per kind) and never `series`. A source version that was classified wrongly is moved by a `reclassify` proposal, which changes `doc_kind` and `series` of that version and nothing else: the original, its hash, its ids and its labels stay (FB0215 for a word that is not in the vocabulary).
- Three `source_kind` values exist only in this store: `vendor_quote`, `company_record` and `company_document`. They carry no tier. A claim on such a version is checked against its original like any other.
- The `csv-rows` method (`extract-csv-rows`) cuts a CSV into windows of `window_rows` data rows with the header repeated. `rec N` is the 1-based data row. Its window is `ceil(N / window_rows)`. A claim's quote must occur in that row's text (FB0308), and `N` must not exceed `row_count` (FB0311). It reads UTF-8 and UTF-16 with a byte-order mark; anything else is "save as CSV UTF-8". A CSV saved from a workbook holds the values on display and no formula, so a manual-calculation setting or a stale cached value can have put a wrong figure in it. A stream that says `workbook = true` has `values_only = true` written into each CSV extraction record, and the page, the pack and the queue say so.
- The `xlsx-sheets` method (`extract-xlsx-sheets`) reads the cell values of a `.xlsx` or `.xlsm` file directly, so a workbook need not be saved as CSV first. It cuts each sheet into windows of `window_rows` data rows with the sheet's header repeated. `sheet NAME!D214` names a cell or a range and `rec NAME!N` the 1-based data row of a sheet ([Corpus format](corpus-format.md#extraction)). The text holds the value the workbook last stored in each cell, never a formula and never the display, and the extraction says in `flags` when some cells are formulas, when calculation was manual, when a formula had no stored result and when the workbook links to other files. Nothing is evaluated or followed. A `.xls`, a `.xlsb` and a password-protected workbook are refused with a message that says to save a copy as `.xlsx`. A system that exports only CSV goes through `csv-rows`.
- Word documents, PDF and project files arrive through `file-drop@1` today: they are preserved, hashed and proposed, and their text is supplied by hand until their extractors exist (Later rows `later-docx-import`, `later-pdf-native-ocr`, `later-project-reader`).

## `types.toml` and records

`types.toml` defines the record types the store holds. The repository holds fixture types only; the company's real types stay in company storage.

```toml
types_schema = 1

[[type]]
name = "commitment"
title = "description"
search = ["description", "supplier"]
show = ["supplier", "amount"]
key = ["po_number", "line"]
basis = "quoted"
scope = { entity = "entity" }

[[type.column]]
name = "entity"
kind = "ref"
to = "entity"
required = true

[[type.column]]
name = "po_number"
kind = "text"
required = true

[[type.column]]
name = "line"
kind = "int"
required = true

[[type.column]]
name = "description"
kind = "text"
required = true

[[type.column]]
name = "supplier"
kind = "ref"

[[type.column]]
name = "amount"
kind = "money"
required = true

[[type]]
name = "entity"
title = "name"
key = ["name"]
scope = "global"

[[type.column]]
name = "name"
kind = "text"
required = true
```

| Key | Where | Rule |
|---|---|---|
| `types_schema` | top | `1`, required |
| `name` | `[[type]]` | `[a-z][a-z0-9-]{0,31}`; unique with revision |
| `revision` | type | Positive integer, default 1; definitions and earlier revision files are immutable |
| `scope`, `period` | type | Column mapping or explicit global scope, and optional period reference, as Financial work specifies |
| `title` | type | The column shown as the record's title in results and lists. Required. Not a `money` or `list` column, because a title is one value |
| `search` | type | Optional. Columns whose text is added to the record's body in the search index ([search.md](search.md#the-company-index)). The `title` column is always indexed |
| `show` | type | Optional. The columns a record view prints, in order, as an array. The default is the `title` column and then the `key` columns. A name that is not a column of the type is FB0240. Record views are the Later row `later-record-views`; until it is built the key is checked and not read |
| `key` | type | The columns that form the natural key. The `origin_key` of a record loaded from a stream is these values joined with `/`, each value escaped first (`%` becomes `%25` and `/` becomes `%2F`), so a part number that holds `/` is an ordinary key. Not a `money` or `list` column, because a key needs one spelling |
| `basis` | type | The default `basis` of a version: `quoted`, `computed` or `judgment`. Default `quoted` |
| `name` | `[[type.column]]` | `[a-z][a-z0-9_]{0,31}`; not the name of an envelope column; unique within the type |
| `kind` | column | `text`, `int`, `date`, `pdate`, `decimal`, `bool`, `list`, `enum`, `money`, `ref` or `calc_run` |
| `values` | column | For `enum`: the allowed words |
| `required` | column | Default false. An empty required cell is FB0205 |
| `to` | column | For `ref`: a type name, or `source` for a source version. Optional; without it the reference may name any record or source version. A name that is not a defined type is FB0240 |

The register reader's cell rules apply to every kind that the registers have ([CSV rules](corpus-format.md#registers)), with one difference for record files, which are loaded from company exports and not typed. Real exports hold line breaks in a description, a `|` in a text cell, and text that starts with `-`, `+`, `=` or `@`. A record file escapes these and does not refuse them. A line break in a cell is stored as one space (the `csv-rows` method does the same, so a quote still matches its row). A `|` is literal in a text cell, and only a list column splits on it. A text cell that starts with `=`, `+`, `-` or `@` is stored as it came, and is neutralised where it could do harm: on an export to CSV or a workbook it is written with a leading single quote, and in a page it is shown as text. A text cell is at most 1,000 characters (FB0204). FB0403 still applies to the registers that people type. Three kinds are new.

| Kind | Cells | Rule |
|---|---|---|
| `money` | Two: `<name>` and `<name>_ccy` | A decimal under the register grammar (FB0500; FB0501 outside the range of the calculation core) with at most six fraction digits, and a currency of three capitals (FB0204). Both or neither (FB0206) |
| `ref` | One | The id of a record version or a source version, as `to` allows. It must resolve (FB0207) |
| `calc_run` | One | The run that produced a figure: `<function id>#<16 hex digits of the run hash>`, as in `build_up@1#3fa9c27e01b4d85a`. A cell of another shape is FB0204, and a function that is not in `CALC_FNS` is FB0504. Every computed figure requires resolvable stored run evidence (FB0380), whether or not its type has a display calc_run column. The code that computed the figure writes it, and a model never does |

Type revisions and their file names follow [Financial work](financial-work.md#observations-and-revisions). Add an optional column in a new immutable revision; retain every old header, definition and row. New versions use the latest revision.

### The envelope

Every record file `records\<type>.csv` or `records\<type>@<revision>.csv` begins with the same 15 columns, then the type's columns in the order `types.toml` lists them. Rows are sorted by `record_id` (by length, then bytes, as company ids sort), then `version` as a number.

| # | Column | Meaning |
|---|---|---|
| 1 | `record_id` | `CM-000012`. The logical record |
| 2 | `version` | Integer from 1. The version id is `CM-000012-v2` |
| 3 | `type` | The type name; it must be the file's type |
| 4 | `as_of` | The date the data describes |
| 5 | `effective_from` | Start of the period the facts apply to |
| 6 | `effective_to` | End of that period; empty means open |
| 7 | `lifecycle` | The sources vocabulary; "superseded" is computed, not stored |
| 8 | `supersedes` | Version ids this one replaces |
| 9 | `basis` | `quoted`, `computed` or `judgment` |
| 10 | `source_ref` | A `|` list of typed source or record pointers for aggregate lineage ([below](#pointers-and-figure-origin)); each numeric field has its own evidence sidecar |
| 11 to 13 | `origin_system`, `origin_stream`, `origin_key` | As above. For a record the key is the natural key joined |
| 14 | `handling` | The labels the labeller gave the version when it was created, a `|` list. Never rewritten. The effective labels include this cell, the ledger ([below](#the-label-ledger)) and all persisted inputs through `effective_item` |
| 15 | `note` | Free text |

- A record version is immutable. A change of figures is a new version with `supersedes`. A re-import that finds equal values reuses the version and still records observation membership. The `Register` port has `read` and `plan_append`, and no way to rewrite a row. `plan_append` writes nothing: it returns the new text of the type's file as one member of the write set that the queue's writer applies. It is also the one place a lock on a closed period is consulted, when a lock record exists. `check` finds a hand edit only where it breaks a rule below.
- An approval, a lock and a hold are record types with a `ref` to the version they cover. They are never a column on the priced row, because approving a price would then write a new version and the id that cites it would move.
- Cell forms: `version` is an integer from 1; `as_of` is a partial date (`YYYY`, `YYYY-MM` or `YYYY-MM-DD`); `effective_from` and `effective_to` are full dates, so that a period has an exact edge; `supersedes`, `source_ref` and `handling` are `|` lists; a type's columns follow the kinds above.
- There is no `snapshot_id` column. Observations and issued snapshots are immutable manifests defined in Financial work.
- Dated rules, quotes and rate sets are record types, not new stores.

### Pointers and figure origin

Typed source and record pointers, field evidence and durable runs follow [Financial work](financial-work.md#evidence-and-calculation-runs). A figure pointer identifies a CSV cell, workbook cell, record field or located document quotation. Each nonempty money, decimal or integer field has quoted, computed or judgment evidence; the record basis supplies only an authoring default. Missing facts never count as verified evidence.

A page or claim cites a record figure with `field <column>`, for example `[@CM-000012-v3, field burden_eng]`. FB0311 reports a missing or empty field. A claim uses `locator_kind = record_field`, the record id and version, and compares its quote with that field. The repository Corpus refuses record-field locators (FB0210). Promotion uses the separate [record evidence branch](corpus-format.md#promotion-rules), not source authority fields. Rendered computed figures name their stored run; a model never recomputes them.

`rests_on` walks all persisted field and run inputs to the originals. `check` and `status` share it for FB0382 when an input is superseded or withdrawn; effective labels walk the same input graph. Missing references and cycles stay unresolved.

### The label ledger

`labels.csv` is one append-only file in the store folder. A record row, a source row and a page keep the labels the labeller gave them, and a later change never rewrites them.

| Column | Meaning |
|---|---|
| `seq` | A number from 1 that only goes up |
| `item_id` | The id of the page, source version or record version the line is about |
| `kind` | `add` in version 1. A `remove` line is the only thing a later `label remove` needs |
| `labels` | A `|` list of label ids |
| `origin` | `person` (from `label add`), `reapply` (from `label reapply`) or `intake` (a correction made while a batch was staged) |

The effective labels of an item are its own cell plus the `labels` of every `add` line for its id. A person or intake ledger classification resolves this item's own `unlabelled`; ordinary unions retain it. Effective labels also union the effective labels of persisted inputs through `labels::effective_item`; a missing input or cycle is unresolved. A line with `origin = reapply` is the one line that also replaces `marked` by its labels, because `label reapply` is the only act that may resolve a marking; a line from a person, or from intake, leaves `marked` in place. One function reads this, and every view, index, check and export uses it. For a promoted question or regression case, the typed queue.accept ids map to the accepted proposal in queue/done; its persisted input pairs remain dependencies after the asked row expires. A missing referenced proposal is unresolved. Keep the proposal while the target exists. The line carries no name of a person: the journal's `label.change` event has the actor, the before set and the after set. `label add` and `label reapply` append lines and rewrite nothing, and a failed write leaves the ledger as it was.

### Loading a stream into records

A stream with `load = "<type>"` builds records. The file is a CSV with commas, read as the `csv-rows` method reads it. Each cell is trimmed of leading and trailing spaces and put in Unicode NFC before the cell rules apply, because an export pads cells and a padded cell would otherwise make every row a new version each month. A date is written `YYYY-MM-DD`, a boolean `true` or `false`, and a decimal by the register grammar; a cell in another form is refused, and the message says which form the column takes. A `calc_run` cell is checked for its shape (FB0204) and for a function in `CALC_FNS` (FB0504). The load does not recompute the figure. It receives each supplied figure’s evidence from the versioned [load-evidence companion](financial-work.md#load-evidence-input); missing or ambiguous evidence refuses before staging.

The header names every `required` column of the type, a `money` column by its two cells (`amount` and `amount_ccy`, or `amount` alone when the stream sets `currency`), and may name any other column of the type and `effective_from`, `effective_to`, `basis`, `lifecycle` and `note`. A name that is not one of those, a repeated name, or a missing required column is FB0241, and the message names the column. A column that the header leaves out keeps the current version's value (empty for a new record), so a type may gain an optional column without every export changing at once.

`sync` fills `type`, `as_of` (the stream's), `lifecycle` (`active`, or the file's cell), `basis` (the file's cell, else the type's), `source_ref` (the stored source version and `rec N`), the three `origin_*` cells (the key is the type's `key` columns, each escaped, joined with `/`) and `handling` (the labels of the file, which include the stream's source labels). A row is matched to a record by `origin_system`, `origin_stream` and `origin_key`, so one part number in two systems stays two records. Each data row has an outcome: New creates a record; Unchanged reuses an equal value version; Historical retains a late observation without making it current; New version appends a changed value. Every distinct accepted source creates an observation, including an all-Unchanged load. For a load stream, dispatch to the batch planner before document-only Older or duplicate-hash shortcuts; only the same accepted observation identity and hash is idempotent. [Financial work](financial-work.md#observations-and-revisions) defines snapshot, event and upsert modes, period membership, omissions, control totals and historical selection. A byte-identical repeat of the same observation is idempotent. Event-key collisions with changed values and duplicate natural keys in a file are structural errors. Source and observation evidence is retained independently of whether a value changes.

If every row validates, `sync` stages the batch and proposes `new_record_batch`. Any structural, scope, control-total or evidence error refuses the entire batch before staging or proposal. A cell that is long, multi-line, or starts with `-` is not an error. If any row has a structural error, nothing is proposed for the batch: `sync` prints the first five failing rows with their lines and codes, skips the file for the run and counts it in `skipped`, so the cursor does not move and the file is reported again until it is fixed. A ledger loaded halfway misleads. The proposal lists the counts of new records, new versions, unchanged and historical rows, and the natural key of each change. A person accepts the whole batch; per-row edits are the Later row `later-batch-edit`. The plan is made again at accept, and if it differs from the one proposed (a record changed in between) the proposal is stale (FB0820) and nothing is written.

The accept writes the original, the extraction, record, run, evidence and observation files and, last, the source row and committed generation. Running the accept again after a crash completes it: the ids are allocated in the same order, and rows already written for this source version are recognised and left alone. A single record by hand is a one-row file in a stream, which keeps one door and one audit trail.

## Checks on a company store

| Check | Runs on a company store? | Notes |
|---|---|---|
| FB0100 | Yes | `corpus.toml` or `store.toml` is missing or malformed. A company area code that does not start with `x` is also FB0100 |
| FB0200 to FB0214 (registers) | Yes | With the company id shapes. FB0210 does not apply to the three company source kinds here; the repository Corpus raises it for them |
| FB0200 to FB0207 on record files | Yes | A record file is read as a register: its header is the 15 envelope columns and the type's columns in order, then syntax, encoding, keys (`record_id` with `version`), cell types, required cells, cross-column rules (an `effective_to` before `effective_from`, a money cell without its currency or the reverse) and references (`source_ref` pointers, `ref` cells). A figure that breaks the decimal grammar is FB0500 |
| FB0215 | Yes | An origin cell is wrong: not all three set or all empty, a system or stream not declared in `systems.toml`, or a key over 200 bytes. Also a `doc_kind` that is not in the vocabulary or a `series` that does not have the form of a stream id |
| FB0216 | Yes | A series is inconsistent: an ordinary document revision of an `origin_key` does not name the version it replaces; in an ordinary document revision stream its `as_of` goes back; or two sources of one stream share an `origin_key`  Ordinary document predecessor and monotonic-date checks exempt only valid record-load observation bindings; missing or malformed bindings refuse. |
| FB0240 | Yes | `types.toml` is invalid: an unknown key, a repeated `(name, revision)` pair or a repeated column name within one revision, a column named like an envelope column or like the currency cell of a money column, a kind the reader does not have, an `enum` with no `values`, a `title` or `key` column that does not exist or is of kind `money` or `list`, a `search` or `show` column that does not exist, a `ref` to an undefined type, or a file in `records\` that no type names |
| FB0241 | Yes | A record version breaks a rule of the record file that the cell checks do not cover: a version number that skips, a version after the first whose `supersedes` is not the version before it (or names a version of another record), a `type` that is not the file's, an `origin_key` that is not the joined key columns, two records of one stream with one `origin_key`, or, when a stream is loaded, a column the type does not have or lacks, or a natural key twice in one file |
| FB0308, FB0311 | Yes | Locators, including `rec`, `sheet` and `field`; FB0311 also reads the `rec N` of a record's `source_ref` |
| FB0380, FB0381 | Yes | Figure origin of record versions ([above](#pointers-and-figure-origin)) |
| FB0382 | Yes | A computed record version rests on a superseded or withdrawn input. A warning |
| FB0504 | Yes | A `calc_run` cell names a function that is not in `CALC_FNS` ([above](#loading-a-stream-into-records)) |
| FB0400 | No | A marking is a label here, and the banner scan runs on the repository Corpus. The labeller reads markings at intake |
| FB0401 | No | The identity scan looks for personal emails, phone numbers, internal hosts and tracker keys in a repository that goes outside the company. A company store holds names, emails and phones by design, so the scan does not run on it. On the repository Corpus its tracker-key row exempts every prefix in `FAMILIES`, so a company id written in a public page is raised by FB0416 and never by FB0401 |
| FB0402, FB0403 | Yes | Claim language (FB0402) runs on the pages of both stores. FB0403 reads the registers that people type. A record file is loaded from an export and escapes instead (above) |
| FB0410 to FB0420 | Yes | Labels, admission, policy, shape and place rules. FB0416 and FB0417 keep company and public shapes apart. FB0418 and FB0419 are raised when the policy is read, and FB0420 when a store folder lacks the not-content-indexed mark |
| FB0808 | Yes | A store recorded in the previous `journal.open` is missing. A warning |
| FB0820, FB0821 | Yes | Re-check at accept; file-type allow-list at intake |
| FB0830 to FB0836 | Yes | `systems.toml`, planner reports, cursors, paths, the adapter contract, a stream outside its source `root` (FB0835) and a restored store that has not been adopted (FB0836) |

The repository Corpus refuses the other direction: a company-shaped id, a company-only `source_kind` or a label other than `public` in a public file or register fails (FB0416, FB0210, FB0410).

## Search

`fieldbook search` reads every store the command can read, with labels beside each result and company documents searchable by window. The rules, the overlay site's index and its budget are in [The company index](search.md#the-company-index).

## What the store does not do

- It does not encrypt. Device encryption and access control are the company's.
- It is not shared. A personal store holds what its owner could see when the data was fetched, because it was fetched with the owner's own access. A second reader is given read access to the folders or, for a store issued to a role logon, is added to the group that the store row names in `readers`. A shared store, accounts and source access rules inside one store are Later rows (`later-shared-store`, `later-accounts`, `later-source-access`).
- It is a working copy. The company's records system is the record, and each figure in the store names its source.
- It does not decide what a document means. It reads the declared labels and the markings the policy names.

Validated record loads in a revision stream allocate source versions in arrival order while preserving source dates. Their source rows have empty `supersedes` and never retire an earlier observation’s source row. A validated observation binding exempts these rows from the document predecessor and monotonic-date parts of FB0216; missing or malformed bindings do not. Observation manifests select current evidence by observation as-of within stream, scope and period. A late load remains historical and does not supersede a newer observation. This exception to backward-date rejection applies only to validated loads; ordinary document revisions keep the FB0216 rule. Record value dates remain immutable, including when a newer observation reuses an unchanged value.

Without `--stream`, batch acceptance infers the stream only when exactly one has waiting proposals for the system. Multiple streams require explicit selection before confirmation or writes; none is a no-op. Each confirmation covers one stream.

For load streams, pair `<input filename>.observation.toml` before candidate matching. It is metadata for that input, never a standalone CSV candidate. Listing/cursor and fetch checks include its hash; the original source hash remains the data file hash. Retained supersession facts enter acceptance replanning and plan_sha256. An orphan, malformed or changed companion refuses the load item without cursor advancement.

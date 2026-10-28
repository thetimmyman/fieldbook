# ADR 0004 The company boundary

Status: Proposed (the owner accepts at F0, with ADR 0002)

Reference documents: [the policy file](../reference/policy.md), [the company store](../reference/company-store.md), [search: the company index](../reference/search.md#the-company-index) and [handling and labels](../reference/corpus-format.md#handling-and-labels).

## Context

ADR 0001 was written for a personal tool. It assumed public sources and synthetic data, two zones (`PUB` in git, `ORG` by pointer), controlled data refused, one user and no pricing. The owner has since decided the following.

- Fieldbook runs on the company laptop, on the company network, as an extension of company infrastructure. It will later become a company application with more than one user.
- It may hold what the owner is authorised to access: vendor quotes, data under a non-disclosure agreement, proprietary data and, where the company's own system is authorised for it, controlled unclassified information and export-controlled data.
- It syncs from the company's data sources: the company data book, vendor quotes, estimating, transactions, project schedules, files and specifications.
- It computes and drafts bid and proposal pricing. Company records hold real figures and bid cost builds.
- Company records go to a model only once the company approves that endpoint. Until then the AI works over public, synthetic and owner-pasted text.
- The build order is the knowledge base first, then estimating, pricing and the indirect rate book, then estimate at completion and earned value, then the rest.

This record decides how, not whether. Several early choices no longer hold.

| Early choice | Why it breaks |
|---|---|
| Two data classes, `PUB` and `ORG` | A quote under an NDA, a competition and a controlled category each need their own handling. Two classes cannot tell them apart |
| "The tool refuses; it never classifies" | Refusing every file with a banner refuses the main use of a company store |
| Company records "held by pointer, never copied" | Search, extraction, calculation and the AI need the text and the figures |
| "Nothing returns from the laptop" | True for the home side and GitHub. Company data now also arrives from company sources |
| One user, one writer, a role that is a declared word | A company needs to know who did what, and a second writer must not mint the same id |
| Ids with four digits and one number space | Company files and records pass 9,999, and two writers collide |
| The data class comes from the folder | A company needs several kinds of data in one store, so labels must travel with the record |

What does not change: files first; deterministic output; a silent build with no network code; one model gate; one hash-chained journal; the review queue; ports in a registry with no plugin loader; and no claim of compliance with any scheme.

## Decision

### 1. The boundary

Fieldbook is an internal application inside the company boundary. It is one control among the company's, not the boundary itself. It has three parts.

| Part | Rule |
|---|---|
| Development | The source repositories hold code, public sources and synthetic data. Nothing company-owned is ever written to them. Example policy files and fixtures use made-up ids and names |
| Operation | Company data lives in company storage, under the data root. Each store holds only the labels the policy lets it hold, or the floor when there is no policy |
| Egress | Data leaves a store only through a destination the policy names, only with labels that destination allows, and only after the journal has the decision |

The one-way flow stays true in the form that matters: nothing company-owned goes to the home side or to GitHub. Company data now arrives from company sources into company storage. That inflow stays inside the boundary.

### 2. Labels

A **label** is an id from a table the company defines. A record carries a **set** of labels. Labels have no order and no "highest".

**Shape.** A company label id matches `[a-z]{2,4}-[0-9]{4}` (`np-0007`, `bid-0003`). It is an opaque code, not a name, so that the id in the journal tells a reader nothing about a counterparty. The loader refuses any other shape (FB0413). The human title, which may name a counterparty, is never journaled. The four built-in ids below are the only other ids.

**Four ids are built in.**

| Id | Meaning | Who may assign it | Held by | May go to |
|---|---|---|---|---|
| `public` | A public source original or a synthetic example | An act, never a default and never a bare say-so: the edition build; a policy `[[source]]` row for a stream whose folder lies inside that row's `root`; or, on a machine with no company store, a person's typed confirmation for home `add` or `new`, which the journal records with who and when | Every store | Any destination whose row lists it |
| `unlabelled` | No label could be found | Fieldbook, at intake. A candidate for the repository Corpus with no banner and no act is `unlabelled`, and `corpus-add` refuses it | Stores whose `holds` lists it. With no policy: `overlay`, `working`, `state` | Nowhere, except the floor B backup route of section 3. A policy row that lists it is invalid |
| `marked` | A marking or a source's own label was found and no rule maps it | Fieldbook, at intake | Stores whose `holds` lists it. With no policy: `overlay`, `working` and `state` on a machine with a company store, and none on a machine without one | Nowhere. A policy row that lists it is invalid |
| `owner-supplied` | Text the owner typed or pasted at a prompt, for that one request. **Request-only** | Fieldbook, at the prompt | No page, source version, record, original or note carries it. Only logs do: the question log and call records, which live in `state`. It is valid in the `holds` of `state` and of no other store, and is refused in a `default`, a `[[source]]` and a `[[marking]]` row (FB0413 in the file), and by `label add` and `label reapply` (FB0822) | Rows that list it. With no policy: Handoff only |

**Rules for a set.**

- A set is sorted, has no duplicates and is never empty. Empty becomes `unlabelled`.
- The rule is **subset**: a request may go to a destination only if every label it carries is in that destination's allowed set.
- `public` is dropped when any other label is present.
- A derived union preserves `unlabelled`. Only an explicit ledger classification of that item resolves it.
- `marked` is dropped only by `label reapply`, when the current policy maps the marking. No person can drop it.
- A derived thing carries the **union** of its inputs' labels: extracted text, a search chunk, an index, a context pack, a calculation, an export, a backup, a call record and an audit event. A model may propose an added label. It may never propose a removed one.
- A machine-derived record from only `public` inputs stays `public`. A note the owner writes does not become `public` because it cites public claims. It takes the store default.

**Where a label is stored.** A page (`handling` in its front matter), a source version (`handling` in `sources.csv`) and a record (`handling` in its envelope) carry the labels the labeller gave them at creation, in a cell. That cell is never rewritten. Every later change is a line in the store's append-only ledger, `labels.csv` (`seq`, `item_id`, `kind`, `labels`, `origin`). The **effective** labels of an item are its cell, its ledger additions and the effective labels of its persisted input references. The pure `labels::effective_item` walks that graph; a missing input or a cycle is unresolved. A restrictive source change therefore restricts old notes, calculations, packs and call records on their next read. A line written by `label reapply` is the one line that also takes `marked` away: it replaces `marked` by its labels. `kind` is `add` now. A `remove` line is the only thing `label remove` needs later, so adding it is not a format change. Every other unit has computed labels. A claim carries those of the source version it cites, and a claim with no source takes the store default. A question, a conflict and a glossary term take the store default, and a question or conflict that names claims or pages also carries their union. `label add` accepts a page, a source version or a record. To change a claim's labels, change its source version's.

**Where labels come from, in order. The results are unioned.**

1. The source system's own label field, mapped through `[[source.map]]`. A native label with no map becomes `marked`.
2. The file's markings: the built-in banner shapes (the FB0400 list) and the policy's `[[marking]]` rules. A built-in shape with no rule becomes `marked`.
3. The policy's `[[source]]` default and path-prefix rules, for a stream whose folder lies inside the row's `root` (section 8).
4. The store default, for text the owner writes in that store. A public-only store default or global marking rule is invalid (FB0413); without a public act, text remains unlabelled. Root-bound public source rules and derivation from only public inputs retain their existing meaning.
5. Nothing found: `unlabelled`.

Fieldbook tightens on any marking. It loosens only on the company's rules. It never infers a label from what a document means.

**Who may change a label.** `label add` adds a company label to a page, a source version or a record. It may resolve `unlabelled` by choosing company labels. It can never add `public`, `unlabelled`, `marked` or `owner-supplied`. It is a person's act at a console: the program needs an interactive console on both input and output and a typed confirmation that names the number of items. A redirected pipe is refused (FB0822). An agent is forbidden to invoke approval commands; terminal checks do not distinguish a person from a program using a PTY. On the laptop preset the `role` word in `settings.toml` is ignored for this verb. If the store row names `changers` (a Windows group), the logon token must be in it. Each change appends a `label.change` event with the before and after sets. Version 1 has no `label remove`. `label reapply` re-runs the labeller under the current policy and can only add.

**What a person cannot do.** A person cannot assign `public`, except by the act named in the table: a pricing workbook labelled `public` by hand so that a model will read it is the failure. A person who wants to release a document follows the company's release process, and the document returns through a source that the policy marks public, from a folder inside that source's `root`.

### 3. The company policy file

The policy is one file owned by the company's IT or security function. Fieldbook reads it and never writes it.

**Where it is read from.** The preset is decided by what is on the machine, never by the name of a folder. `Layout::decide` picks the **Laptop** preset when a company store (`overlay\store.toml`) exists under the data root, or the machine file exists, or the state folder lies under the user's local application data. Anything else is **Home or CI**.

| Preset | Path | Can the owner redirect it? |
|---|---|---|
| Laptop | `%ProgramData%\Fieldbook\policy.toml`. The folder is asked of Windows by its known-folder call. It is not read from an environment variable | No. The release binary has no flag that names a policy or moves a laptop folder. A machine file wins over everything else |
| Home and CI | None: the floor applies | Only in a test build (feature `test-hooks`), which accepts `--policy FILE`, `--program-data DIR` and `--local-app-data DIR` |

The path comes from `Layout::policy_path()`. One function, `PolicySource::read()`, returns the bytes. A central source later replaces that function and nothing else. A flag that names a state, overlay or working folder other than the fixed laptop folders is refused on the Laptop preset (FB0419). It would start a state folder with no record of the policy. `policy check FILE` reads a draft, in any release build, and installs nothing.

A policy inside the data root or the Corpus is refused (FB0414), and so is one inside a git work tree. Those are places the owner can edit. Before the file is read, the program asks the operating system who can write the file and its folder. If anyone but administrators and the system account can, the state is Closed (FB0418). So the owner's write access is checked, not assumed. What the program cannot see is an administrator who writes the file. For that, the [designated egress events](../reference/audit-contract.md#kinds-and-fields) and outward-write events carry the policy hash, and `policy.change` records changes to the observed policy. `policy show` prints the path, the hash and the result of the writer check, and the first line of every AI verb prints `policy: <state> <first eight hex digits>`. The hash in an exported journal shows a different file.

**States.**

| State | How it arises | What Fieldbook does |
|---|---|---|
| Floor | No policy file is found, and no policy has been loaded here before | The built-in floor policy, below |
| Loaded | The file is found, passes the writer check, parses, and its `revision` is not lower than the highest recorded | Use it. If its hash differs from the last recorded one, append `policy.change` |
| Closed | A file is present and unreadable, invalid, writable by a non-administrator (FB0418) or an example file; or its `revision` is lower than recorded; or a policy was loaded here before (a revision is recorded) and the file is now missing | Every destination is denied except the audit sink. `check`, `build`, `search` and `status` still run. `policy.change` is appended with `outcome = absent` or `invalid` |

The failure `revision` prevents: someone puts back an older, more permissive file. IT can still roll back. A roll-back is a new revision with old content.

**Floor policy.** There are two floors, chosen by one fact: whether a company store is on the machine.

- Labels: the four built-ins.
- Stores: `corpus` holds `public` only. `overlay`, `working` and `state` hold `public` and `unlabelled`, and `state` also holds `owner-supplied` (its logs). On floor B, `overlay`, `working` and `state` also hold `marked`. A store with no row in a loaded policy uses its floor row without `marked`: under a loaded policy only a `[[store]]` row makes a store hold `marked`. Files already held stay where they are, `check` reports each one whose store no longer holds its labels (FB0412), and IT either lists `marked` in the store rows or maps the marking in a `[[marking]]` row so that `label reapply` resolves it.
- **Floor A, no company store on the machine** (Home, CI, the first run): Handoff allows `public` and `owner-supplied`. A model endpoint named in `settings.toml` allows `public`. MCP, Bridge, print and clipboard allow `public`. Export and backup allow `public` and `owner-supplied`.
- **Floor B, a company store on the machine and no policy.** This is a laptop IT has not reached yet. The laptop is company infrastructure and the files on it are ones the owner can already open, so Fieldbook holds them: `overlay`, `working` and `state` hold `marked` as well as `unlabelled`, and the owner can read, search, calculate and draft over them in place, in the per-user folders on the laptop. What leaves is another matter, and for that the floor is the most closed one. Nothing marked leaves by any door, and nothing unlabelled leaves except by the one backup route below. No model endpoint receives anything. Handoff allows `public` and `owner-supplied`. MCP, Bridge, print and clipboard allow `public`. Export is refused: it needs a policy row with a path. Backup has one narrow route: a target that the owner marked `company_storage = true` in its `[[backup.target]]` table of `settings.toml`, that the place test finds local or on the network. Through it a snapshot may hold `public`, `unlabelled` and `owner-supplied`, never `marked`, because a backup target is a new place with its own readers. A snapshot that would hold `marked` is refused (FB0901) with the count, so one marked file stops the tool's own backup until a policy maps the marking and `label reapply` has run. The journal records `dest`, `labels` and `policy = floor`. The failure this prevents: a laptop that holds company data and cannot be backed up by the tool the company asked for, so the owner copies the folder by hand to a place no one checked.
- **Why floor B holds `marked`.** A rule that refused a `marked` file until IT wrote a policy would aim at the wrong place. The failure to prevent is a marked file reaching a place no company rule has approved: a model, an export, a tool client, a backup, a print or the clipboard. The floor still stops that. Every way out passes `egress::check`, which denies `marked` at step 2 before it reads any row, and anything derived from a marked file carries `marked` too (the union rule), so a draft, a pack or a call record built from one is denied at the same door. What the floor allows is only the owner's own files sitting in the owner's own per-user folders on a laptop the company runs. Refusing to hold them would protect nothing and would cut local search over files the owner can already open.
- The fixed rows (section 6) apply as always.

Permission lists are `LabelAllow` values: they may be empty and keep `public` beside other ids. Data labels are `LabelSet` values and use the derivation rules above. A permission intersection never uses data normalization.

**Narrow only.** The effective allow set of a destination is the policy row intersected with the owner's `settings.toml` list. Every settings key declares a merge rule: a list intersects, a switch is AND, a number is the minimum, a word is the policy's. A key with no merge rule fails a test. `ai status` and `policy show` print both sets and which side cut each label.

**Schema, version 1.** The parser denies unknown keys. A newer policy may carry a restriction an older program cannot enforce, so an unknown key makes the whole file invalid and closes egress.

| Table | Keys | Meaning |
|---|---|---|
| top | `policy_schema`, `revision`, `example` | Schema 1. `revision` is the company's own counter and must not go down. `example = true` marks the shipped example, which no program loads |
| `[[label]]` | `id`, `title`, `banner`, `restricted` | A label. The title is shown to people and never journaled. The banner is printed on pages, exports and prints. `restricted = true` needs a reader group wherever the label is held |
| `[[marking]]` | `match`, `labels` | Text found in a file maps to labels |
| `[[source]]` | `id`, `root`, `default`, `[[source.rule]]` (`prefix`, `labels`), `[[source.map]]` (`native`, `labels`) | A company data source: the folder tree it may read, its default labels, path rules and its own classification field mapped to labels |
| `[[store]]` | `id`, `holds`, `default`, `readers`, `changers`, `hold` | What a store may hold, the labels given to notes the owner writes in it, the Windows group that may open it, the group that may run `label add` on it, and a hold that refuses purge and label changes |
| `[[destination]]` | `kind`, `name`, `url`, `allow`, `expires`, `keep_calls_days_max`, `ref`, `clients`, `paths` | Where labels may go (section 6) |
| `[audit]` | `sink`, `max_unshipped_hours`, `require_fresh_export` | The folder `audit export` writes to, how old the last export may be, and whether a model call with a non-public label needs a recent export. IT sets the last key (a boolean). It is on when the key is absent |

**Rules the loader enforces (all FB0413).**

- `unlabelled` or `marked` in any `allow`: invalid. (A store's `holds` may list them.)
- A destination of kind `git`, `edition` or `audit`: invalid. They are fixed rows.
- A store row for `corpus`: invalid. The repository Corpus is public by definition.
- An `allow`, `holds`, `default` or `labels` id that is not in the table or the built-ins: invalid. A company label id that does not match `[a-z]{2,4}-[0-9]{4}`, or that is a built-in id: invalid.
- `owner-supplied` in a `holds` other than that of `state`, in a `default`, or in a `[[source]]` or `[[marking]]` row: invalid. It is request-only.
- A restricted label in a store's `holds` when that row has no `readers`: invalid. `readers`, `changers` or `hold` on `corpus`: invalid.
- A `[[source]]` row without an absolute `root` (or the word `drop`, which means the default drop folder of that system): invalid.
- A non-public label in an `mcp` or `clipboard` row: invalid in version 1. The tool server serves `public` only, because an `mcp` row approves labels and not an endpoint (section 6).
- A `model` row that allows a non-public label, when the export requirement is on (`require_fresh_export` is true or absent) and `[audit]` has no `sink` or no `max_unshipped_hours`: invalid. With the requirement off neither is needed.
- `require_fresh_export` that is not a boolean: invalid.
- A model row without a `url`, or with a `url` that is not https or loopback: invalid.
- A `paths` key on any kind but `export` and `backup`: invalid.
- `example = true`: the file is refused whole. `policy check` prints `warning` and the line number for each value that still looks like a placeholder: a host ending `.example`, a path starting `C:\Example` and a ref starting `CHG-EXAMPLE-`.
- An unknown key or table, or a `policy_schema` the program does not know: invalid.

`policy show` prints the hash, the revision, the path and the writer check, every label (id and title), every rule, every row, the fixed rows marked "built in", the advisory kinds marked "advisory", and the effective table after the owner's narrowing. `policy check FILE` validates a draft with no side effects, so IT can test a file before it installs it.

### 4. Stores and admission

A **store** is a root of the data layout. Its id is the root name: `corpus` (the repository Corpus at home, the edition on the laptop), `overlay`, `working` and `state`. The **company store** is the `overlay\` folder. A store's id comes from where the folder is, never from a setting.

A record is written to a store only if every label it carries is in that store's `holds`. Otherwise the write is refused (FB0412). `state` is a transit store: an item in `state\inbox`, `staging` or the queue is admitted by the `holds` of the store it is bound for, so the policy lists a label once. The `holds` of `state` itself governs its logs, the question log and call records, and the derived site, which `build --overlay` refuses to write when `state` does not hold a label the site carries. In the repository Corpus the same refusal keeps its old codes (FB0400 for a marking, FB0410 for the `handling` cell) and `scan-allow.toml` keeps its meaning.

**SC-02 becomes three checks.**

| Check | Runs | Reads | Result |
|---|---|---|---|
| **Labeller** | At intake: `add`, `new`, a sync, a question typed at a prompt | The file's text and metadata, the source's own label, the policy's rules | A label set. The same bytes and policy give the same labels. It never lowers a label and never edits or removes a marking |
| **Store admission** | Before a record is written | The label set and the store's `holds` | Admit, or refuse (FB0412) |
| **Egress** | At every boundary (section 5) | The labels of everything outbound, the destination row, the system clock, the age of the last audit export; and for text, the marking shapes over the final text | Allow, or refuse |

The repository Corpus is the same machine with the strictest store: `corpus` holds `public` only, so a banner there fails admission as it does today.

**Company store layout.**

```text
%LOCALAPPDATA%\Fieldbook\
  edition\            public. The shipped edition. Never edited on the laptop
  originals\          the owner's local copies of originals
  working\            the owner's notes and drafts
  overlay\            the company store (content root on the laptop)
    corpus.toml       areas: every code starts with x
    store.toml        store_uid, writer, restored_from
    labels.csv        the label ledger (append only)
    systems.toml      systems and streams
    types.toml        record types
    registers\        the five registers, company ids only
    records\          one CSV per record type
    pages\  sources\org\  sources\text\<version-id>\
  state\              inbox, staging, queue, journal, settings.toml, backup.toml
    sync\<system>\<stream>.toml      cursors
    drop\<system>\<stream>\          default drop folders
    site\overlay\                    derived: the company site and index, on demand
```

- **Git, synced and network folders (FB0415).** The test is on the folder, not on its name. `World::place(path)` reports whether the folder is a reparse point or a junction, whether it carries the cloud-file attributes of a sync client or is registered as a sync root, and whether its drive is a network drive or a UNC path. On the laptop preset the live store, the state folder and every drop folder may not lie inside a git work tree. The live store and the state folder must be local, with no reparse point and no sync root. A drop folder may be local or on the network but not a sync root. An export or backup path from the policy is tested the same way, and the result is journaled with the write. A sync-root target is valid only when the row's `paths` names it. At home no row exists, so a backup target in a sync root is the warning FB0813, and a one-line notice when the target says `kind = "synced"`. On the home layout the state folder is `.fieldbook/` beside the checkout, which is inside a work tree by design. There it holds `public` and `owner-supplied` items only (the question log is `owner-supplied`), `check` fails (FB0415) when `.fieldbook/` is not named in an ignore file of the checkout, and on every layout a page, source version, record, original or extraction with any other label is refused under a `.git` ancestor. A question-log row, a queue file and the journal are not labelled items, and the journal holds ids only. The failure it prevents: a developer points `--state` at a checkout, runs `git add -A`, and publishes company figures. A company share is valid as a drop source (read only) and as a backup target. It is not a live store. The journal and its lock stay on local disk.
- Fieldbook marks new store folders not content-indexed and adds no encryption of its own. Device encryption and access control are the company's. The mark is a file attribute that a copy, a restore or some tools drop, so `check` fails (FB0420) when a store folder lacks it.
- Backup covers the new files, because `layout.roots()` lists the roots and the new files sit under two of them. It labels them by exclusion (section 6).
- Existence is metadata. A refusal names no label and no title.
- Search is never cut. Local search runs over every store the person may read, in one in-memory index over those stores with one idf, and shows each result's labels. A store whose row names `readers` is not opened for a person outside that group, so nothing from it enters the index, the site or a pack. Results shown on screen are not egress. Results returned to a tool client are. A company document's text is searchable by window. The company site and its index are a second copy of company text, so they are derived and on demand: `build --overlay` writes them to `state\site\overlay\`, not for a store that names `readers`; backup does not copy them; `clean --site` removes them; and the leaver steps list them. Under the floor the site includes `marked` pages, because `state` holds `marked` there and the site is a derived copy in the same per-user tree, never exported or backed up; leaving them out would cut the site's search. The rules are in [search.md](../reference/search.md#the-company-index); the edition's index stays public-only.
- **Restore guard.** A restore of a backup onto a second machine, while the first is still in use, would give two writers with the same index and two stores that issue the same id for good. So `restore --to` stamps `store.toml` with `restored_from = <the old store_uid>`, and the program refuses to allocate an id (FB0836) until `init --overlay --adopt` records one of two things: that the old machine is retired (the writer is kept), or a new `--writer N`. `store_uid` is written to `journal.open` and to the manifest of `audit export`, so exports from many laptops are attributable. A second-device step beyond that is deferred.
- **Missing stores.** `journal.open` lists the `store_uid` of every store the run opened. At start, a store listed in the previous `journal.open` that is not there appends `store.missing` and `check` warns (FB0808). A store row with `hold = true` also refuses purge and label changes. The company's records system, not a Fieldbook store, is the record (section 16).

### 5. Egress

One pure function in `fieldbook-core` makes every egress decision:

`egress::check(policy, dest, labels, delivery, now, shipped) -> Decision`

`labels` is `Option<&LabelSet>` and `delivery` is `Option<&Delivery>`. With both absent, check only destination availability (state, row, expiry, URL, client and path) and return its complete narrowed `LabelAllow` for view selection; no data labels are invented. With labels present and no delivery, also check the data labels and model freshness, but return no sink authority. With both present, check the complete delivery and bind its exact operations. A delivery without labels is FB0902 (`mismatch`). An empty permission set stays empty and selects no content. Selection never converts permissions to data labels. The order below applies to a data preflight or final delivery; the audit destination retains its existing Closed-state exception.

`now` is read from the system clock by one function, and no flag or variable reaches it in a release build. `shipped` is the age of the last `audit.export`, from the journal. The decision is made in this order.

1. Policy state is Closed: deny (FB0413).
2. Any label is `marked` or not in the table: deny (FB0901). A label `unlabelled` is denied too, with one exception: a `backup` to a target with `company_storage = true`, on a machine with a company store and no policy (floor B, section 3), goes on to steps 3 and 4, where the floor row decides. A loaded policy has no such exception, because no row may list `unlabelled`.
3. No row for the destination, or the row is expired, or the model URL differs from settings, or the tool client is not listed: deny (FB0918).
4. The labels are not all in the row's `allow` intersected with the owner's list: deny (FB0901).
5. A marking in the outbound text maps to a label outside the request's set: deny (FB0902). The record was mislabelled.
6. A `model` call with a non-public label, the requirement on (`[audit] require_fresh_export`, which is on when the key is absent), and the last export older than `[audit] max_unshipped_hours`, or none yet: deny (FB0925). The journal is evidence only once it is off the machine. With the requirement off this step never denies.
7. Otherwise an `Allowed` with private destination, labels, policy hash and delivery binding. `delivery = None` is preflight-only and cannot reach a sink. A delivery binds canonical path or endpoint/reply route, operation, exact payload hashes and lengths, and marking results; the final writer or transport checks the binding against the actual operation and bytes.

The caller selects a view, renders the bytes, scans them and checks a concrete delivery, then appends the journal event before sending. A token for a view cannot write.  Every outbound API takes the `Allowed` token: the model send, the tool-server reply, the export writer, the pack writer, the backup writer. Only `egress::check` makes one. The rule is in the types, not in a list. `io::atomic` has two doors: `write_in_root(RootPath, ..)` for a path under the data root, where a `RootPath` is made in one file, `io/root.rs`, from the `Layout`; and `write_outside(&Allowed, path, ..)`. A test fails if any other public writer exists, so there is no list of outside writers to keep. That is a design aid, not a boundary: a new dependency, a macro or an unsafe block can still reach the operating system. So the release has a second, separate check: a deny list in `cargo-deny`, and a scan of the built binary's imported names for network, process-start and clipboard calls outside the gate crates. An edit to the deny list, the scan list or the policy crates is owner-reviewed. The IT answers call the token a design aid and the binary scan the check. This extends the `Audited` token from "no write without a journal entry" to "no egress without a decision".

**Views.** `PublicView` becomes `LabelledView`, built for one destination from an `Allowed` token. A record is in it only if all of the record's labels are in the allowed set. A record outside is left out whole, never redacted. The by-construction property survives: a pack builder given a view for `handoff` cannot hold a record `handoff` may not have.

**Left-out count.** Existence is metadata, and a count tells a reader how many restricted records matched a query. So the count is a property of the destination kind: `shows_left_out_count` in `DEST_KINDS`. It is true for `handoff`, `export` and `print`, which the owner reads: the pack ends with `Left out (policy): N records`, and names none of them. It is false for `model`, `mcp`, `bridge`, `clipboard` and `edition`: they get the fixed line `Some records were omitted.` with no number (an edition is read by anyone who gets a copy). The failure it prevents: an answer that sounds complete because the company records it needed were silently dropped, and, for a tool client, a count that shows how many restricted records mention a competitor. The pack header `Handling: PUBLIC` becomes `Labels: <ids>` plus the banners of those labels.

**Gate order.** The model gate is the same single gate. Its step 3 is the egress check and its step 4 is the mismatch scan.

### 6. Destinations

| Kind | Covers | Row matched by | What Fieldbook enforces | What it cannot enforce |
|---|---|---|---|---|
| `model` | An HTTPS model endpoint named in settings | `name`, and `url` equal to `base_url` | Allow set, URL binding, expiry, call-record retention cap, journal before send, a fresh audit export for non-public labels unless IT turned the requirement off | What the operator keeps or trains on. That is a contract |
| `handoff` (advisory) | A pack file the owner carries to an approved assistant | Optional `name` | Allow set. The pack's first line prints its labels. Records outside the set are left out and counted | Which assistant the owner pastes into |
| `mcp` | Results returned over `fieldbook mcp` | The kind; optional `clients` | `public` only in version 1. Journal before the reply. The `clients` list (hash of the client name) | Who the client really is. `clientInfo` is self-declared. The client sends what it gets to a model that nobody approved. So no company label is served |
| `bridge` | The chat Bridge listener on the loopback address | The kind | Whether the listener starts at all. Allow set for what the page shows. `Cache-Control: no-store` on the page and every `/api` route. Not offered for a non-public label when the machine has more than one interactive logon | Another program of the same user that reads the port |
| `export` | Any file written outside the data root | `paths` prefix on the canonical path | Allow set, path prefix, the place test, the label banner and metadata in the file | What happens to the file after it lands |
| `backup` | A backup target | `paths` prefix | Allow set, path prefix, the place test, the existing refusals (FB0812, FB0813) | The target's own access control |
| `print` (advisory) | The print view of a page | The kind | Whether the page renders a print form, and a banner on every printed page | The browser's own print |
| `clipboard` (advisory) | The copy controls in pages | The kind | `public` only in version 1. Whether the page shows them | Text selection and copy by hand |
| `audit` | The journal export | Fixed | Journal lines only, no labelled content | |
| `git` | A source repository work tree | **Fixed: `public` only** | Refuses labels outside public/owner-supplied under a `.git` ancestor (FB0415); owner-supplied is confined to ignored, never-committed home State and its restore drills, never Corpus content or commits; laptop live, State and drop roots stay outside Git | |
| `edition` | The public edition that ships | **Fixed: `public` only** | The build refuses any other label | |

Writes inside the data root are store writes. Writes outside it are exports. If more than one export row's `paths` contains the target, the allowed set is the union of those rows.

**Advisory kinds.** Once a pack is shown, printed or copied, the program has no say. So `policy.md` and `policy show` mark `handoff`, `print` and `clipboard` as advisory: a row is a statement of what the page offers, not a barrier. A pack is journaled as `ai.pack`, which records what was put in the pack and not where it went. The assistant page and its data file carry the policy hash and an expiry time, and the page hides copy and print once the time passes, so a saved page does not outlive a policy change. In version 1 a clipboard row can list `public` only.

- **Handoff.** The default route. It sends nothing. The pack builder takes a view for `handoff`.
- **Tool server.** `fieldbook mcp` keeps five tools and no write tool. Its library is a `LabelledView` for `mcp`, and the destination is `public` only (the table says why). A refused call names no label and no title. `shown N of M` counts permitted hits only. A tool client gets the fixed omitted line and no count. A listed `clients` row makes an unlisted client a refused session. The row can be reopened for company labels (`later-mcp-company-data`) when it can bind the launching program by its path and publisher, read from the process, and names the endpoint that IT approved for that program.
- **Chat Bridge.** Sends go through the one gate. The page shows the label banner of the pack. The listener is a destination of its own (`bridge`), so IT can turn it on apart from the model: with a policy loaded and no `bridge` row the listener does not start (FB0918). The allow set is the row's, narrowed by the owner's list, and it is cut to `public` when `World::interactive_logons()` counts more than one interactive logon on the machine, because another logon can reach the loopback port and the per-run token is then the only barrier. The page and every `/api` route carry `Cache-Control: no-store`, so the browser keeps no copy of company text.
- **Export.** Every export file carries its label set where the format has a place: a banner line and a meta tag in HTML, a banner cell and a document property in a workbook, a manifest in a zip, the `handling` column in a register CSV, and a first line in a pack. This is so the company's own tools can read it. Fieldbook does not claim to apply the company's sensitivity-label scheme.
- **Print and clipboard.** A page for a non-public label set renders a banner on every printed page, and hides the print form or the copy control unless the row allows the set. They are controls, not blocks, and the design says so.
- **Backup.** A snapshot copies every root of every configured store (`layout.roots()`). It is labelled by exclusion. Every file it copies must be of a known kind with a declared label: a page, a source version, a record, an original, extraction text, a queue proposal, a question-log line, a call record, a Handoff pack (labelled by its saved labels and current effective inputs from its ticket), the label ledger, an issued-snapshot manifest. A file that fits no kind (a stray file in a root, anything in `working` that is not a page or an original, anything in a derived folder) adds `unlabelled`. Derived folders (the site, the index, and `pages/` and `drill/` of the state folder) are not copied. A `pages/` folder of the Corpus or of a content root (`corpus/pages/`, `overlay\pages\`) holds hand-written pages and is copied. Corpus support is text-bearing. A selected known support path has public provenance only when its exact current hash matches either the last completed public write for that path in admitted verified journal history, or the same path's entry in a retained catalogue whose exact digest has a successful admitted raw-verification binding. A catalogue, checksum list or edition fingerprint beside the selected tree grants no authority by itself. Pending intents, failed verification, unknown paths, mismatched hashes and unavailable evidence never grant provenance. Apply current markings and applicable label rules after resolving that fact; the edition act cannot erase a restrictive or unresolved label. Missing provenance contributes `unlabelled` and the existing destination and floor rules decide the result. It does not make every backup universally refuse: the pre-existing narrow floor-B route remains exactly as specified. The catalogue is public edition evidence at `edition/evidence/corpus-support.toml`, written as data by both packaging producers and never compiled into an executable or treated as a copy of the bytes ([Public support verification](../reference/audit-contract.md#public-support-verification)). Only a validated known text-free control schema adds no label: the journal, locks, sync cursors, backup metadata and metadata-only reports or saved packs. Validated settings are text-bearing owner-supplied metadata: union `owner-supplied`, markings over the exact copied bytes (including comments), and the State default labels supplied by a Loaded policy. Do not add the no-policy floor State default to validated settings. Unknown or malformed kinds retain their parser errors and `unlabelled` treatment. Filename patterns do not establish this. The local backup verification cache carries its canonical target path as labelled owner-supplied request metadata, plus marking and applicable State default labels; it is never a text-free control or a journal path. Text-bearing metadata such as an ask sidecar, import report or schema-bearing Handoff ticket carries its saved labels and input references, evaluated through `effective_item`; an unknown or malformed kind adds `unlabelled`. The set is checked once against the target. With a policy loaded, a snapshot that is not only `public` needs a target under a `backup` row's `paths`, and every label in it must be in that row's `allow`. With a policy loaded, a snapshot that holds `unlabelled` or `marked` items is refused (FB0901) with the count, because no row may list those two labels. The way out for `unlabelled` is a real label: the policy's `[[source]]` default rules give it at intake, and `label add` gives it by hand. The way out for `marked` is a `[[marking]]` rule in the policy that maps the marking, and then `label reapply`; `label add` leaves `marked` in place. With no policy, floor A allows `public` and `owner-supplied`, so the home Drill 4 (which copies the question log) works. Floor B has the one narrow route of section 3, which takes `unlabelled` and never `marked`; a store that holds `marked` (as floor B lets it) is therefore not backed up by the tool until a policy maps the marking and `label reapply` has run, and the refusal names the count. The label-leak matrix plants a canary in `working\`, in the site folder and in a stray file, and asserts that the snapshot is refused or labelled for each. `backup.toml` has `labels` in place of `zone` and `roots` for the roots copied; the target's `company_storage` key is in `settings.toml`. A `restore --to` outside the data root is an export.

### 7. Model endpoints and the AI before any approval

**How the company approves an endpoint.** It adds one `[[destination]] kind = "model"` row. The row names the endpoint, binds its URL, lists the labels it may receive, may carry an expiry and may cap how long call records are kept. It is the company's change, made under the company's change control. Fieldbook records two things about it.

- `policy.change`, when the file's hash first differs.
- Every use: `ai.begin` carries `dest`, `labels` and `policy` (a Handoff pack writes `ai.pack` with the same three), and `ai.call` pairs with its begin by `tx`.

So "who approved what" is the company's change record, and "under which policy did this call run" is in the journal.

**All of these must hold before a byte is sent.**

1. The owner named the endpoint in `settings.toml`, and its key variable (`key_env`) is set.
2. The policy has a row for it, not expired, with `url` equal to `base_url`.
3. Every label the request carries is in the row's `allow` intersected with the owner's list.
4. No marking in the final text maps to a label outside the request's set.
5. If the request carries a non-public label and the export requirement is on (it is by default, and IT can turn it off, section 3): the policy has an `[audit] sink`, and the last `audit export` is not older than `max_unshipped_hours` (FB0925). A journal that stays on the machine is not evidence of what left it.
6. The journal accepted `ai.begin`.

The owner cannot approve an endpoint by editing settings. Raising `allow` in `settings.toml` does nothing the policy has not already allowed.

The model key stays in the environment variable that `key_env` names. It is the owner's own key for an endpoint the company approved, and it is never in a file, the journal, a log or an error line. A connector credential is different and worth more, so it comes from the credential store (section 8). If IT-38 asks for the credential store for model keys as well, the `Secrets` port serves the model key with no change to the gate.

**Route by coverage.** Labels have no order, so a route is chosen by coverage. `[model] order = ["corp-gateway", "handoff"]` replaces `default`, `fallback` and `[model.route]` of the settings file. With no `order`, the order is `["handoff"]`. The gate picks the first entry whose effective allow set covers the whole request. A request that none covers is refused. A policy refusal never falls back. An availability failure of the chosen route falls back to Handoff only if `handoff` is in `order` and the Handoff row covers the request.

**The AI before approval.** The AI works over public, synthetic and owner-pasted text from the first day. Company records go to a model only once a policy row lists their labels. The first approval IT is asked for is the smallest one: the silent build with Handoff. The AI build has one network feature, `net`, which holds the model client. A company connector is built behind its own feature `net-pull`, added by the owner-reviewed card `later-company-source-read`; that card moves the model client behind `net-model` and keeps `net = ["net-model"]`, so IT can approve a read-only connector and refuse a model, or the reverse, with a separate allow-list request for each. The Stage C seams are types only, with no network code before their trigger.

| Build and state | Public content | Owner-pasted question | Company records |
|---|---|---|---|
| Silent build | Makes no connection at all | Not applicable | Local search, build and calculators only |
| AI build, no policy, floor A (no company store) | To a settings-named endpoint, as `public` | Handoff only | None exist |
| AI build, no policy, floor B (a company store) | Handoff, and public tool results | Handoff only | Never leave. They stay in the store as `unlabelled`, `marked` or their source label. No model endpoint receives anything |
| AI build, policy loaded, no `model` row | Handoff if its row allows | Handoff if its row allows | Never leave until a row lists their labels |
| AI build, policy loaded, row for the endpoint | As the row says | As the row says | As the row says |

**Call records.** A call record holds the prompt and the reply, so it is content. It carries the request's labels and lives in `state`. It is written only if the `holds` of the `state` row lists those labels and the row's `keep_calls_days_max` is above zero. If either fails, the call proceeds and no record is kept, with a warning. The existing purge is capped by the row.

**Labelling by a model.** Asking a model to label a document would send the document to a model before it has a label. `unlabelled` is allowed nowhere, so the design closes that door. The deterministic classifier rules may propose labels by path or keyword, as proposals a person confirms.

### 8. Sync: the Pull port

One port, `Pull`, replaces the two ports the end-product document proposes (a public feed and a system-of-record read). Both list items and fetch their bytes. They differ only in the labels of what they read, and that belongs to the policy's `[[source]]` row.

```rust
pub struct ItemRef { key: Option<OriginKey>, name: String, size: u64, stamp: String, as_of: Option<PartialDate> }
pub struct Listing { items: Vec<ItemRef>, next: Cursor, complete: bool }
pub struct Fetched { bytes: Vec<u8>, ext: String, observed: CivilDate, native: Option<String> }

pub trait Pull {
    fn id(&self) -> &'static str;                                  // "file-drop@1"
    fn list(&self, s: &StreamCfg, from: &Cursor) -> Result<Listing, PullError>;
    fn fetch(&self, s: &StreamCfg, item: &ItemRef) -> Result<Fetched, PullError>;
}
```

- `stamp` is an adapter-owned change token. `Cursor` is an opaque string of at most 4 KiB. `key` is none when the name does not fit the stream's template (the planner reports `Unmatched`). `complete` is true when the listing is the whole stream, and `sync` journals it as `scope = full`, or `incremental` when it is false. `native` is the source's own classification text for the policy's `[[source.map]]`; a folder has none.
- List, then fetch. The hash, not the stamp, decides whether bytes are new.
- Contract suite P: listing is sorted and deterministic; a second listing from the returned cursor yields nothing new; an empty cursor lists everything; a key, where there is one, is non-empty and has no `..` (`sync` refuses one over 200 bytes, FB0215); `fetch` never writes to the source; an item that changed between list and fetch is skipped for the run; oversize items fail with `TooLarge`; every error names the adapter.
- Incremental by cursor, idempotent by hash and key. A lost or damaged cursor costs one full re-list (FB0832). It cannot cause a duplicate.

**`file-drop@1`** is the first adapter. It needs no API call, makes no connection and adds no crate. Each stream has a folder. Fieldbook reads it, never writes to it, and never moves or deletes a file in it. A stream's `path` may name a company share. A symbolic link, a junction, `..` or an empty key is refused (FB0833). Each item passes the file-type allow-list (FB0821) and the labeller. The key and the date come from a name template such as `{key}-{as_of}`, matched by a pure function with no regex crate. A stream declares `as_of = "name"` or `as_of = "retrieved"`. There is no silent default.

**Planner.** A pure function decides, per item, one of:

| Outcome | When | Result |
|---|---|---|
| `Present` | The hash is held under this key, by a waiting or rejected proposal, or by a source added by hand | Nothing written |
| `NewSource` | Unseen key. In a vintage stream the key includes the `as_of`, so a late vintage is a new source | A `new_source` proposal |
| `NewVersion` | Known key, new bytes, not older | A `new_version` proposal |
| `Older` | Revision stream and `as_of` is earlier than the current version's | Reported, not proposed (FB0831) |
| `Duplicate` | The hash is held under another key | Reported, not proposed (FB0831) |
| `Unmatched` | The name does not fit the stream's template | Reported, not proposed (FB0831) |

`Older` has a concrete failure behind it. Someone drops last month's file into a share again. Without the rule it would supersede this month's.

**`systems.toml`** has a closed key set, begins with `systems_schema = 1` and holds no secrets. An unknown key is an error (FB0830). Each `[[system]]` has `id`, `adapter`, `scope` and `note`. Each `[[system.stream]]` has `id`, `versioning` (`"revision"` or `"vintage"`, required, no default), `name`, `as_of`, an optional `path`, an optional `load` record type with an optional `currency` for its money cells, and an optional `[system.stream.row]` table. The `row` table holds the cells every pulled item shares, so a proposal needs no typing: `source_kind` (`vendor_quote`, `company_record` or `company_document`), `publisher`, `licence`, `volatility`, `tag`, `purpose` and `title` (a template of `{key}` and `{as_of}`, default `{key}`). Each is set by `sync` as a value from the stream, never from the file or a model, and a classifier cannot replace it; a person can edit it at review. A system's `id` is the id the policy's `[[source]]` table uses. The file holds no label and no secret: the owner edits it, and only the company says what a source may hold and where it may read from. The policy row's `root` binds the id to a folder tree. A stream's folder (its `path`, or the default drop folder) must lie inside that root, compared by whole path components, or `sync` refuses the stream (FB0835) and journals a `refuse`. `sync.begin` records the optional canonical-root fingerprint and policy hash, never the path; [the audit contract](../reference/audit-contract.md#source-root-fingerprint) defines its bytes and absent-row case. The failure it prevents: the owner repoints a source id whose default is `public` at any folder, and everything in it arrives as public. A network adapter later names its credential by a name in the Windows credential store.

**`sync` is stage 0 of the intake pipeline and writes proposals only.** `fieldbook sync [--system ID] [--stream ID] [--full] [--dry-run] [--json]` lists from the cursor (`--full` lists from the start and ignores the cursor, which is how a stream is checked again after a restore), plans, fetches, copies each new item to `state\inbox\<system>\<stream>\`, and runs the same pipeline as `add --propose`. It writes no file under the content root. A proposal is state, not evidence. A person accepts: `queue accept` stays interactive, in a terminal, with the owner or lead role. `queue accept --all --system S [--stream T]` accepts every waiting proposal of one stream after one interactive confirmation that lists them. Each is still re-checked (FB0820) and journaled on its own. `--dry-run` lists the key, `as_of` and planned outcome of each file and writes nothing, so a wrong name template shows on the first run. Fieldbook installs no scheduler. A task the company already runs may call the verb.

**Network adapters later.** A web, database or report adapter has `Needs::Net`. It is built only with the feature `net-pull`, which its own card adds (a separate allow-list request: that card gives the model client its own feature `net-model`), passes a `GuardedPull` gate that is the model gate pointed inward, and has its own destination kind. Its card is owner-reviewed and edits the network documents. The company approves the channel first. The adapter reads its credential from the Windows credential store through a `Secrets` port. The environment variable is for the home preset only. A secret never appears in `systems.toml`, `settings.toml`, the journal or an error line.

### 9. Provenance, extraction and records

**Provenance.** Three columns are appended to `sources.csv`: `origin_system`, `origin_stream` and `origin_key` (columns 24 to 26). They are empty for a source added by hand. Origin is written only by a puller or by a person, never by a model or a rule. The failure it prevents: a model that can suggest `origin_system` could claim an export came from the finance system when it came from an email.

**Document kind and series.** Two more columns, `doc_kind` and `series` (columns 27 and 28, so 28 in all), say what a file is and which run of files it belongs to. `doc_kind` is a word of the closed const `DOC_KINDS`: `contract`, `modification`, `quote`, `rate_package`, `eac_package`, `baseline_log`, `schedule_file`, `billing_export`, `receivables_export`, `evms_export` and `other`. Empty means not yet classified. `series` is a name that joins the sources of one vintage stream. Both cells are empty in the repository Corpus (FB0215). In a company store `sync` sets `doc_kind` from the stream's `row` table and `series` from the stream (see "Revision or vintage"), and a person may change either at review (a `series` has the form of a stream id and is checked as such, FB0215). The classifier may suggest `doc_kind` (`SuggestField::DocKind`, by one row per kind in `rules.toml`) and a person confirms it, as with every suggested field; it never suggests `series`. A file whose kind was wrong is moved by a `reclassify` proposal, a new proposal kind that changes `doc_kind` and `series` of a source version and nothing else: the original, its hash, its ids and its labels stay. The failure it prevents: every company source arrives as `company_document` with free tag text, and the backlog is then re-keyed by hand. Adding a kind is a vocabulary change with a known-bad case.

**Revision or vintage.** A stream declares which. In a **revision** stream, new bytes for a known key are a new version of the same source and `supersedes` is set. In a **vintage** stream, each dated file is its own source in a series, and a late vintage is valid. A wrong guess cannot be silent: a wrong `vintage` on a revision stream would leave two current quote letters side by side. A vintage stream writes its stream id into the `series` of each source it proposes, so the sources of one series are found by one column and none supersedes another.

**Company source kinds.** Three `source_kind` values exist only in a company store: `vendor_quote`, `company_record` and `company_document`. They carry no authority tier, because the A to D scale ranks public publishers, and the repository Corpus refuses them (FB0210, as it refuses every company-only value). A claim on a version of a company kind is checked against its original like any other, since the original is the primary evidence of what the quote or record says. A person still reads the original at the locator; Fieldbook never raises a claim on its own. Adding a kind is a vocabulary change with a known-bad case.

**Table extraction.** Two methods split a table into fixed windows of rows with the header repeated: `csv-rows` for a CSV and `xlsx-sheets` for a workbook. `rec N` is the 1-based data row, and its window is arithmetic: `ceil(N / window_rows)`. The quote of a claim must occur in that row's text (FB0308). `extraction.toml` gains `unit`, `window_rows`, `row_count`, `values`, `values_only`, `flags` and `[sheets]`. The methods become an append-only const `METHODS`, with the test `methods_only_grow`. `csv-rows@1` is a core extractor with no new crate. It reads UTF-8 and UTF-16 with a byte-order mark. Anything else is `NotText` ("save as CSV UTF-8"). `xlsx-sheets@1` reads a `.xlsx` or `.xlsm` workbook directly, right after CSV, and a workbook need not be saved as CSV first (owner decision D-24 in [ADR 0001](0001-foundation.md)). Each worksheet has its own windows. The text holds the value the workbook stored in each cell and never a formula, and nothing in the file runs or is followed. `flags` say what the extractor could tell about those values: formulas, manual calculation, formulas with no saved result and links to other files (R-13). The citation `sheet NAME!D214` names cells and `rec NAME!213` names a data row. The reader is `calamine`, with `zip` as a second direct crate, and the card has `Review: owner`. An old `.xls`, a binary `.xlsb` and a password-protected workbook are refused with a message that says to save a copy as `.xlsx`. The CSV route stays for a system that exports only CSV. Documents, PDF and project files arrive through `file-drop@1` today: they are preserved, hashed and proposed, and their text is supplied by hand until their extractors exist. A CSV saved from a workbook holds displayed values only: a formula, a manual-calculation setting and a stale cached value are gone (R-13). A stream whose files are saved that way says `workbook = true` in `systems.toml`; `sync` then writes `values_only = true` into the `extraction.toml` of each source it proposes, and the source page, the context pack and the review queue say "values only (saved from a workbook)" beside that source. The workbook templates, whose rows are loaded as CSV registers, tell the person to recalculate and then save as CSV UTF-8.

**Typed records.** `types.toml` in the company store (it begins with `types_schema = 1`) defines record types with the column kinds the register reader has, plus `money` (a decimal and a currency cell), `ref` (an id of a record or source version) and `calc_run` (the function and the run that produced a figure, written `<function id>#<16 hex digits of the run hash>`, as in `build_up@1#3fa9c27e01b4d85a`; a cell of any other shape is FB0204, and a function that is not in `CALC_FNS` is FB0504). A type names its `title` column, optional `search` columns, the `key` columns of its natural key, a default `basis`, and optionally `show`, the columns a record view prints and in what order (the default is `title` and then the `key` columns; a column that does not exist is FB0240), so search, sync and the record pages need no code per type. Every record file begins with one 15-column envelope. The full file specifications are in [the company store](../reference/company-store.md).

| # | Column | Meaning |
|---|---|---|
| 1 | `record_id` | `CM-000012`. The logical record |
| 2 | `version` | Integer from 1. The version id is `CM-000012-v2` |
| 3 | `type` | A type name |
| 4 | `as_of` | The date the data describes |
| 5 | `effective_from` | Start of the period the facts apply to |
| 6 | `effective_to` | End of that period; empty means open |
| 7 | `lifecycle` | The sources vocabulary; "superseded" is computed, not stored |
| 8 | `supersedes` | Version ids this one replaces |
| 9 | `basis` | `quoted`, `computed` or `judgment` |
| 10 | `source_ref` | Typed source or record pointers for aggregate lineage; numeric fields have their own evidence |
| 11 to 13 | `origin_system`, `origin_stream`, `origin_key` | As above. For a record the key is the natural key joined |
| 14 | `handling` | The label set |
| 15 | `note` | Free text |

A record version is immutable, and so is its `handling` cell: a label change is a line in the ledger, never a rewrite (section 2). A change of figures is a new version with `supersedes`. Equal values reuse their immutable version but retain observation membership and source evidence. A late load is retained as historical and does not replace the current observation. Snapshot, event and upsert semantics, type revisions and exact scoped lock checks are defined in [Financial work](../reference/financial-work.md). A `Register` port reads records and plans the appending of new ones, which the queue's writer applies as part of one write set; its default adapter is `csv-folder@1`, and a server store is the only expected second adapter. `Register::plan_append` is the one place that a lock on a closed period is consulted when a lock exists. An approval identifies its exact target versions through its validated `targets` list; a hold refers to its covered version. A `period_lock` carries scope and period plus a ref to its authorizing approval, and a `reopen` refers to that exact lock and a new approval. These are separate immutable records, never an approval column on the priced row, so approving a price does not move the id that cites it. An issued snapshot is an ordinary row with a `manifest_sha256` and one `snapshots/<manifest_sha256>.toml` carrier; the carrier and its canonical identity are in [Financial work](../reference/financial-work.md#issued-snapshot-carrier-and-canonical-identity). One core function, `rests_on(version)`, walks aggregate lineage and all field-evidence and stored-run inputs through record and source versions; `status` and `check` use it to warn on a version with computed field evidence whose inputs are superseded or withdrawn (FB0382). A stream with `load = "type"` also builds a batch of records. Real exports hold line breaks in a description, a `/` in a part number, a leading `-` or `+`, and a `|` in a text cell, so the cell rules escape and do not refuse. Key columns are joined with an escaped form (`%2F` for `/`, `%25` for `%`). A line break in a text cell is loaded as one space. A leading `=`, `+`, `-` or `@` is kept, and neutralised where it can do harm: on a CSV or workbook export and in a page. A structural, evidence, scope or control-total error stops the batch. If every row validates, `sync` stages it and proposes `new_record_batch`. If any row has a structural error, nothing is proposed for the batch: `sync` prints the failing rows, skips the file and leaves the cursor where it was. A ledger loaded halfway misleads. The batch is accepted whole, and its plan is made again at accept, so a batch that no longer fits the store is stale (FB0820). Dated rules, quotes and rate sets are record types, not new stores. There is no snapshot_id column: observations are immutable manifests, and an issued snapshot is an ordinary row plus its carrier, under the financial-work contract.

**Citing one figure of a record.** The locator kind `field` names one column of a record version: `[@CM-000012-v3, field burden_eng]`. In a company store a claim's `source_version` may name a record version when its `locator_kind` is `record_field`. FB0311 fires when the version has no such column or the cell is empty. Every computed field has sidecar evidence that resolves a durable canonical run; a calc_run cell may display the reference, and a page that shows the figure cites the field and names the run. A model is given the locator and the run and never recomputes a figure. The repository Corpus refuses `record_field` (FB0210), and a citation that names a record outside a company store has the status "outside context".

### 10. Identity

**Company ids and public ids cannot collide, by shape.** Public ids keep their family-specific patterns: sources and claims use four sequence digits, questions three, and conflicts at least two with no four-digit cap. Every id the company store defines has six to ten digits. Company areas start with `x`. Company glossary terms start with `x_`. One const `FAMILIES` table holds every family. The id parsers read it, and tests compare it with the lexer, the tracker-key exemptions and the Identifiers table of the Corpus format.

| Family | Public shape | Company shape |
|---|---|---|
| Source | `S-0042` | `SO-000042` |
| Source version | `S-0042-v1` | `SO-000042-v1` |
| Claim | `CL-CMP-0011` | `CL-XES-000012` (area starts with `x`) |
| Question | `Q-025` | `QO-000003` |
| Conflict | `XC-14` | `XO-000002` |
| Record, record version | none | `CM-000012`, `CM-000012-v2` |
| Page, term | `fnd-...`, snake_case | area starts with `x`; term starts with `x_` |

A company number is the writer digits followed by exactly six sequence digits. Writer 0 has no extra digits: `SO-000042`. Writer 1 is `SO-1000042`. The writer index runs 0 to 9999. A sequence runs 000001 to 999999 per family and writer. Company ids sort by length, then bytes. The lexer gains three generic rows, so a new record type needs no lexer change: a company claim, a company versioned id and a company id.

`store.toml` holds `store_uid` (random, minted by `init --overlay`), `writer` (default 0) and, after a restore, `restored_from`. It lives with the data, so a restored store keeps its writer. That is why a restore is guarded (section 4): no id is allocated in a restored store until `init --overlay --adopt` records that the old machine is retired, or a new `--writer N`. Allocation reads only the rows of that family and writer. The overlay and the edition no longer share a number space.

**Identity of an item is a display id plus two exact keys.** The display id is allocated, sequential, never reused and never renamed. `origin_key` is the system's own key, and finds "the same item again". `sha256` is the bytes, and finds "the same file again". A ULID is unreadable in a citation and a file name. A derived id breaks "never renamed" the first time a system renames a key.

### 11. Actor, session and authorisation

**Actor.** Every verb runs for an `Actor { principal, role, via }`.

- `Principal` is written `os:<DOMAIN>\<user>`. It is read from the operating-system token of the process (the user's SID and domain) through `World::principal()`, not from an environment variable that any process can set. In tests and CI it is `local`. On the laptop preset an unreadable token is refused (FB0822). Fieldbook does not authenticate it: the operating-system logon does. The prefix lets a later identity provider add `idp:` without a new key.
- `Role` stays owner, lead and agent. It says what the actor may accept. On the laptop preset it is not read from `settings.toml` for `label add`; that verb needs an interactive console and a typed confirmation (section 2).
- `Via` is `cli`, `chat` or `mcp`.
- The chat Bridge turns its per-run token into a `Session { actor }`, and each route takes a `&Session`. The tool server's environment holds the actor. A hosted listener later replaces the token check and returns the same `Session`. The Bridge answers every route with `Cache-Control: no-store`.

**Authz port.** `Authz` answers who may see or do what. Labels answer where data may go. Both stay.

```rust
trait Authz {
    fn allow(&self, who: &Actor, act: Act, on: &Scope) -> Decision;   // Allow | Deny(Word)
}
enum Act { Read, Propose, Accept, Export, Admin }
```

- The default adapter is `policy-readers@1`. It allows `Read` on a store when the store's row names no `readers`, or the logon token is in the `readers` group; it allows `Admin` for `label add` on a store when the row names no `changers`, or the token is in that group. The check runs before the store is opened. A person outside the group gets no index, site or pack from that store, because the store is never opened for them. It is checked against the operating-system group, so the company's directory, not Fieldbook, decides who may read restricted data. A later adapter reads a source's own access list.
- Four call sites, all in places that exist: where a store is opened; `queue propose`; `queue accept` (after the terminal and role test, which stay); and export and backup.
- A `Deny` appends a `refuse` event with a closed-list `reason` (FB0807).
- Authz only narrows. It is not a step of the model gate. A request is built only from a view of stores the actor may read, so content the actor cannot see cannot reach a request.
- A restricted label (`restricted = true`) may be held only by a store whose row names `readers`. Until a store is shared, the group is the owner alone or the owner's team. A personal store holds what its owner could see when it was fetched, because it was fetched with the owner's own access. A personal store is never shared. A shared store at Stage B needs `readers` before it holds a restricted label, and a per-item access rule (`later-source-access`) is a format amendment, not an adapter.

**Store port.** `Store` names the seam that `World` already is. The default adapter is `folder@1`. Later adapters are `share@1` and `served@1`. Write sets stay store-relative. A write set that holds an absolute path or `..` is refused. Contract suite S gathers tests that exist already: original first and row last; a failed write leaves no row; listing in byte order; a hash on read equals the hash on write.

**Stages.** The seams above are built now. The modules wait for their triggers (section "Deferred").

| | Stage A: one owner | Stage B: IT-deployed to more people | Stage C: company-hosted service |
|---|---|---|---|
| Who writes a store | The owner's program | Each person's program, to a personal store. A shared read-only store has one named writer | The service, for shared stores. Laptops propose |
| Identity | `os:` principal | The same; the company may set it by policy | The company's identity source |
| Access | `policy-readers@1`: a `readers` group per store | The same. Each store holds only what its owner could see. A store with a restricted label names its `readers` before a second person is added | A source-access adapter, which needs a collection cell in the format first |
| Policy | One file, if IT provides one | One file per machine, distributed by IT | The same two layers, held by the service |
| Journal | One chain per install | One chain per install; the company may collect copies | One chain per writer; the service's chain is central |
| Network | Loopback Bridge and an approved endpoint only | The same | A listener on the company network, a separate build under its own ADR |
| New in the program | The seams | Nothing | `served@1`, a source-access adapter, a service build, `via = service` |

What stays the same across all three: the Corpus format, the journal format (events only gain kinds), the review queue, the intake stages, the model gate, the id grammars and the port table (rows are added, none removed). The proof is a test: run the existing suite with `policy-readers@1` and no `readers` row, and `actor = local`, and every golden outside the journal's new keys is byte-identical.

### 12. Audit

The journal answers: who did what to which labelled record, under which policy, and where did it go? It answers that as evidence only once it is exported off the machine. A file in the user's folder can be replaced whole, and the hash chain does not stop that. So a model call with a non-public label is refused when the last export is older than the policy's limit (section 5), unless IT has turned the requirement off. The setting is in the policy file, and `ai.begin` and the call record of every model call carry the hash of the file in force, so an auditor reads it from IT's copy of that revision.

**Envelope.** Every event gets `actor`, `store` and `via`. `store` replaces the draft's `zone` key. `role` stays. `journal.open` gets `writer`, `event_schema`, `tool_version` and `stores` (the `store_uid` of every store the run opened). This goes into the first journal card, because the parser denies unknown keys and the hash covers each line. Adding a key to a built journal would mean a new journal version.

**Fields on events that move or change content.** Typed values only (ids, hashes, counts and labels). A field that names one thing in the event's own store holds an `Id`. A list that can span stores (the results of a tool call, the context of a pack) holds `RecordRef` values, a store and an id written `overlay/SO-000042`. `dest` is a kind of `DEST_KINDS`. `policy` is the SHA-256 of the policy in force; for the floor it is the SHA-256 of the text `fieldbook-floor-policy-1`.

| Kind | Added fields |
|---|---|
| `write.begin`, `write.end` | `labels`. For an export: `dest`, `path_sha256` and `policy` |
| `refuse` | `dest`, `labels`, `reason` (closed list: `label`, `unlabelled`, `marked`, `no-row`, `expired`, `url`, `client`, `closed`, `mismatch`, `store`, `git`, `authz`, `shipped`, `root`, `place`, `adopt`) |
| `queue.propose`, `queue.accept`, `queue.reject` | `labels` of the proposal |
| `ai.begin`, `ai.pack` | `dest`, `labels`, `policy` (sha256) |
| `ai.paste`, `ask` | `labels` |
| `mcp.call` | `dest`, `labels`, `policy` |
| `backup.begin` | `labels`, `dest`, `policy` |
| `restore.begin` | `labels`; when the restored folder lies outside the data root also `dest` and `policy` |

**New kinds.** `policy.change` (`policy`, `prev`, `revision`, `outcome`, `labels_n`, `rows_n`); `label.change` (`ids`, `added`, `removed`, `origin`); `audit.export` (`from_seq`, `to_seq`, `head`, `policy`, `store_uid`); `store.missing` (`store_uid`); `store.adopted` (`store_uid`, `mode`, `writer`); `sync.begin` (`system`, `stream`, `adapter`, `root_sha256`, `policy`); `sync.end` (`system`, `stream`, `scope`, `seen`, `proposed`, `present`, `skipped`, `refused`). No key, name or path goes in the journal.

**Questions the journal answers.** `audit show --id ID` (who touched this record), `audit show --label L` (who touched anything with this label), `audit show --kind ai.begin` and the other egress kinds (what left, to where, under which policy), the last `policy.change` before a `seq` (what the policy said then), and `refuse` events (what was refused).

**Export to company logging.** `fieldbook audit export [--to DIR] [--since SEQ]` writes the journal's own lines byte for byte, and a manifest with the first and last `seq`, the `prev` hash, the `head`, the hash of the lines, the policy hash, the tool version and the `store_uid`. The folder is the policy's `[audit] sink`, which IT should make a place the user can append to but not rewrite. It is a deterministic verb: it makes no connection and starts no process. IT collects the files, verifies the chain from `prev` and compares `head` with `audit verify --anchor-seq N --anchor-head HEX`. That is the tamper evidence the local file alone cannot give, provided IT keeps the `head` somewhere the user cannot reach. The exported journal names labels, record ids, counts and times, so it is as sensitive as the most sensitive label it names, and the sink's audience is IT-35's question. Fieldbook installs no scheduled task. IT schedules the export with its own tools, and while the requirement is on (it is by default) the model gate refuses a non-public call when the schedule lapses.

**What the journal does not show.** A page opened in the browser, a file read by hand, a screenshot, a copy by hand, and a user with rights to the data root who deletes the journal. The last is why the head must be exported.

### 13. What stays refused

These are code, not policy. The policy file cannot widen them.

| Refused | Why (the failure it prevents) |
|---|---|
| Writing any non-public record, extraction, original or derived text under a git work tree (FB0415) | Company data on a host the company does not operate. The repositories are developed by an autonomous agent and hosted outside the company |
| Shipping any non-public label in an edition | The same failure through the release path |
| Stripping, editing or hiding a marking | A copy that has lost its marking is how a controlled file gets out |
| A label assigned by an agent; `public` assigned by a default or by a bare say-so; `owner-supplied` kept on a record | A hurried or autonomous change of a label defeats every later check |
| A policy file read from the data root, the Corpus or a git work tree (FB0414), or a policy file or its folder that a non-administrator can write (FB0418) | A widened copy next to the owner's own files |
| A stream whose folder lies outside its source's `root` (FB0835) | The owner repoints a `public` source at any folder and everything arrives as public |
| An id allocated in a restored store that has not been adopted (FB0836) | Two machines issue the same id, and ids are never reused |
| A policy whose `revision` is lower than the highest recorded (FB0413) | Roll-back to a more permissive file |
| A key in `settings.toml`; a key, prompt or reply in the journal, a log, an error or a status line | Credentials and content in the files that get backed up and exported |
| A policy key the program does not know (FB0413) | A restriction the program cannot enforce |
| Network access in the silent build; any listener but the loopback Bridge | Unchanged from before |

What the policy decides, and the code does not: whether a model call with a non-public label must wait for a recent audit export (FB0925). IT sets it with `[audit] require_fresh_export`, and it is on when the key is absent (owner decision D-25 in [ADR 0001](0001-foundation.md)). While it is on it prevents the failure of section 12: a journal that never leaves the machine is not evidence of what left it. The company decides whether to hold that line (IT-35).

What stops being refused: a file with a marking banner, in a company store. If the policy maps the marking, it is labelled with the mapped label and admitted by a store whose `holds` lists that label. With no policy on a machine that has a company store, it is labelled `marked` and admitted, because the floor holds `marked` there; it goes out by no door. It is still refused in the repository Corpus (FB0400), and under a loaded policy in a store whose `holds` lacks the label (FB0412).

### 14. Check codes

| Code | Range | Meaning |
|---|---|---|
| FB0215 | 02 | An Origin or `doc_kind` cell is wrong, or either is set in the repository Corpus |
| FB0216 | 02 | A series is inconsistent (revision or vintage) |
| FB0240 | 02 | `types.toml` is invalid |
| FB0241 | 02 | A record version breaks a rule of the record file that the cell checks do not cover (versions, `supersedes`, origin key, stream load columns) |
| FB0311 | 03 | A `rec`, `sheet` or `field` locator does not resolve |
| FB0380 | 03 | A numeric field lacks its computed inputs or matching stored run, or its judgment evidence lacks a rationale. Reserved before, active with `r2-records` |
| FB0381 | 03 | A quoted numeric field lacks one typed evidence pointer or available target/text, or its value, unit or quotation disagrees with the evidence. Invalid cell and field locators use FB0311. Reserved before, active with `r2-records` |
| FB0382 | 03 | A computed record rests on a superseded or withdrawn input. Warning |
| FB0411 | 04 | A record carries a label that is not in the table. Warning; treated as unresolved |
| FB0412 | 04 | A store may not hold these labels. Intake refused |
| FB0413 | 04 | Policy file invalid, an example file, or its `revision` is lower than recorded. Closed |
| FB0414 | 04 | Policy file read from a place the owner can edit |
| FB0415 | 04 | On the laptop layout, a store, state or drop folder lies in a git work tree, or fails the place test (a sync root, a reparse point, or a network drive where the rule needs local). On any layout, a record with a label other than `public` would be written under a git work tree. At home, `.fieldbook/` is not ignored |
| FB0416 | 04 | A company id shape is in a public file or register |
| FB0417 | 04 | A row of the company store defines an id of a public shape |
| FB0418 | 04 | The policy file or its folder can be written by a non-administrator. Closed |
| FB0419 | 04 | On the laptop preset, a flag names a state, overlay or working folder other than the fixed laptop folders |
| FB0420 | 04 | A store folder lacks the not-content-indexed attribute |
| FB0504 | 05 | A calculation case or a `calc_run` cell names a function that is not in `CALC_FNS` |
| FB0806 | 08 | `audit export` target unusable, or a file would be overwritten with different bytes |
| FB0807 | 08 | An `Authz` denial |
| FB0808 | 08 | A store recorded in the previous `journal.open` is missing. Warning |
| FB0830 | 08 | `systems.toml` is invalid, or `sync` names something the layout cannot use |
| FB0831 | 08 | A pulled item, or a row of a loaded file, is not proposed: `older`, `duplicate` or `unmatched`. Warning |
| FB0832 | 08 | A cursor is unreadable or newer than the listing. The stream is listed again. Warning |
| FB0833 | 08 | A pulled key or path leaves its stream folder, or is empty |
| FB0834 | 08 | A Pull adapter breaks the contract. Raised by the contract test only |
| FB0835 | 08 | A stream folder lies outside the `root` of its policy source |
| FB0836 | 08 | A restored store has not been adopted, so ids are not allocated |
| FB0918 | 09 | No approval for this destination: no row, expired, URL differs, client not listed |
| FB0925 | 09 | A model call with a non-public label was refused because the policy requires a recent export and the journal was not exported within `max_unshipped_hours` |

FB0901 keeps its meaning (data above a destination's approval) and reads "labels". FB0902 keeps its meaning (a marking in outbound text) and reads "not covered by the request's labels". FB0942 keeps its meaning and drops the class name from its text. No existing code changes meaning. FB0380 and FB0381 were reserved for a figure in a page; they become active for record versions, because a company figure is a record version (decision 9), and a number typed into a page is not checked.

### 15. Recurring changes

Each recurring change has one recipe, in [the cards README](../cards/README.md), one place to edit and one check that fails on drift. This table is the index. The recipe steps are not repeated here.

| Change | The one place | The check that fails on drift |
|---|---|---|
| Adding a label | The company's `policy.toml`, `[[label]]`. The repository holds only the synthetic example and the fixture policy. A built-in label is an amendment to this record | `leak_matrix_covers_every_label`: the matrix plants a canary for every label of the fixture policy and every destination kind. A label used in a row but not defined fails `policy check` (FB0413), and so does an id that is not opaque |
| Adding a destination | One row in the const `DEST_KINDS` (kind, fixed or policy row, advisory or enforced, `shows_left_out_count`, bound by path or url, token-taking API) | `dest_kinds_match_policy_docs_and_matrix`: compares `DEST_KINDS` with the kinds the loader accepts, the rows of `docs/reference/policy.md` and the matrix columns. `io_has_two_writer_doors`: `io::atomic` has `write_in_root(RootPath, ..)` and `write_outside(&Allowed, ..)` and no other public writer, so a new writer outside the data root cannot be added without a token. A release-time scan of the built binary is the second check |
| Adding a sync adapter | One row in `ADAPTERS` with port `Pull`, and the adapter in `crates/fieldbook/src/io/pull/` (a folder) or `crates/fieldbook/src/io/net/` (a network) | `adapters_match_registry_docs_and_fixtures` and contract suite P. An adapter that writes to its source fails the suite |
| Adding a record type | One block in the company's `types.toml`. The repository holds fixture types only | `record_types_have_fixture_and_known_bad`: every fixture type has valid rows and known-bad rows for FB0204 and FB0241 |
| Adding an id family | One row in `FAMILIES` | `families_have_shapes_goldens_and_docs`. A company-shaped id in a public fixture fails FB0416 |
| Adding a calculation function | One row in `CALC_FNS` (id with version, input and output units, rounding stages, origin), and a positive and a negative case | `calc_fns_have_cases` and `calc_epoch_matches_the_case_set`. FB0504 for a case or a `calc_run` cell that names a function not in the table |
| Adding a unit | One variant of `Unit` in `fieldbook-calc`, with its basis family | `unit_words_match_the_reference`: the enum, the non-currency words of the `unit` vocabulary of the Corpus format and the time words of the number-unit lint (FB0309) agree |
| Adding an intake target | One row in `TARGETS` (id, kind: source, record or dataset, key shapes, required columns) | `targets_have_known_bad`: every target has valid proposals (the queue fixtures of the kinds that write it) and known-bad proposals (`tests/fixtures/pipeline/<target>/known-bad/`) |
| Adding a module | One row of AD-24 in ADR 0001 (its trigger and the seams it uses), then the rows of the three recipes above that it needs and its record types in the company's `types.toml` | `later_slug`, and the checks of those recipes |
| Adding an extraction method | One row in `METHODS` | `methods_only_grow` and `methods_match_extractors` |

### 16. Leaving and hand-over

The failure: the owner changes role, leaves or loses access, and the company cannot read its own records, or cannot change the policy.

- The company's records system is the record. A Fieldbook store is a working copy, and each figure in it keeps its field evidence and stored calculation run where applicable, alongside the record's aggregate source lineage.
- IT holds the policy file and the audit sink from the first day. Nothing in the program lets the owner change the policy (section 3).
- A store and its keys are issued to a role that the company can reassign, not to a person's name. IT reissues the folder's access, the `readers` group and any connector credential, which sits in the credential store (section 8).
- A second named person can read the store before the owner goes: IT gives them read access to the folders or, for a store issued to a role logon, adds the people who sign in as that role to the `readers` group. The program opens only the data root of the account that runs it, so another account's program does not open the owner's store (a shared live store is `later-shared-store`).
- The store is plain files (Markdown, CSV and TOML). A person with access to the folder reads it with a text editor or a spreadsheet, without the program. The runbook `hand-over.md` lists the folders.
- The company keeps the source, the build recipe and the signing identity. Fieldbook does not build or sign for the company.
- At leave or at the end of a retention period, the folders to dispose of are `overlay\`, `working\`, `originals\`, `state\` and the derived `state\site\`. Fieldbook promises no secure overwrite. The disposal method is the company's (IT-37).
- `hold = true` on a store row refuses purge and label changes, and `store.missing` shows a store that went away.

## Consequences

**What this gives.**

- The owner can hold real company data and still be able to show IT one file that decides what leaves, and one journal that, once it is exported, shows what did.
- The AI is useful from the first day over public, synthetic and owner-pasted text, and company records join it the moment the company approves an endpoint, with no code change.
- Nothing built now needs rework when the tool grows from one owner to a company application. The seams are the actor, the store identity, the `Store` and `Authz` ports, the policy layer and the journal envelope.
- Sync from company sources starts with a folder of exports. It adds neither network code nor a crate.

**What it costs.**

- Format edits before the freeze: `handling` becomes a list of label ids, `sources.csv` gains five columns (three of origin, `doc_kind` and `series`), ids gain company shapes, the lexer gains three rows, the locator gains the kind `field`, the `applies_to` cell gets a grammar, and `extraction.toml` gains keys. After the freeze each of these would be an amendment.
- The journal gains an envelope on every event. It must be in the first journal card.
- The model gate, the packs, the tool server, backup and the cards that name `DataClass` all change. They are cards that are not built yet.
- There are 39 IT questions instead of 32.
- Label correctness is only as good as the source's label, the file's markings and the company's rules. A wrong source label gets through the labeller and the subset test. The mismatch scan sees only a marking that is in the text.
- A permissive policy is possible. Fieldbook enforces the company's file. It does not judge it. `policy show` and `policy.change` make a change visible.
- With no policy and no company store (floor A), the AI build can send public text to an endpoint named in settings. With a company store and no policy (floor B), it sends nothing to a model, and it holds `marked` files with the rest, where the owner reads, searches and drafts over them without sending them anywhere. The existence of the AI build on a company laptop is IT's decision (IT-27). `ai status` prints "policy: none (floor)" and the journal records it.
- The journal sits in the user's folder, so a user with rights to it can delete it. Only an exported head gives tamper evidence, and, while the policy requires it, a model call with a non-public label waits for the export.
- A label id tells a reader that a counterparty's record exists. Ids go to the journal and into any bug report that includes it, so the loader accepts only opaque codes (two to four letters, a dash, four digits).
- Without `label remove`, a wrongly restrictive item stays restrictive. Re-importing does not fix it, because the planner writes nothing for bytes it holds. The remedy in version 1 is the source rule for new items, and IT. The ledger's `kind` column means `label remove` is a later line kind and not a format change (`later-label-remove`).
- Fieldbook is for one owner at Stage A. The first approval IT is asked for is the silent build with Handoff, so a smaller review can start before the larger ones.
- Print and clipboard controls are advisory. A reader can still select text.
- The security code is written by an agent. Mitigation: one choke point, a token type, two writer doors, a leak matrix, negative controls, a release scan of the built binary and a mutation gate over the label and egress modules. A mutant that changes no behaviour (an equivalent mutant) goes in a skip file, with one reason line per mutant, that the owner reviews. Any other survivor fails the gate.
- Pricing outputs are a high-value leak. They carry labels and go out only through an export row. The label and export design lands before any pricing card.

**Evidence IT can ask for.**

- The **label-leak matrix**: a synthetic policy with several labels and every destination kind. For each label, plant a canary string in a record. Run every output path (search, pack, tool server, export, backup, print and clipboard flags, call record, journal). Assert that the canary appears only where the policy allows its label. A `marked` record held at floor B is planted too, and no destination takes it.
- **Negative controls**, each of which must fail when the control is removed: skip `egress::check` in the model send; build an `Allowed` by hand; drop the subset test; treat `unlabelled` as allowed for every destination, or under a loaded policy; let `marked` pass step 2; add `marked` to the floor B `backup` row; let a floor A store hold `marked`; let the owner's list widen; read the policy from the data root; accept a lower `revision`; write a company-labelled file under a `.git` ancestor; run `label add` with input from a pipe; repoint a `public` source at a canary folder; take the policy path from a flag in a release build; send with a stale audit export; read a missing `require_fresh_export` as off; add a stray file to a root and back it up; ask the same question with 0 and with 3 restricted records and compare the replies to `mcp` and the model byte for byte.
- A **mutation gate** over `label` and `policy::egress`: no surviving mutant outside the owner-reviewed skip file.
- **Journal checks**: every egress kind has `dest`, `labels` and `policy`. A fixture journal verifies. One changed byte gives FB0800. A truncated copy fails `--expect-head` (FB0802).

## Build order

The owner's order is the knowledge base, then estimating, pricing and the indirect rate book, then estimate at completion and earned value, then the rest. This record does not change that order. It fixes the parts that cannot be changed later, and builds them inside the knowledge-base phases.

- **P1.** `core-labels` and `core-policy` (the label types, the policy parser, the floor, `egress::check`, `LabelledView`, the leak matrix), with the envelope keys in `audit-journal` and the id-family table in `core-ids-and-dates`.
- **P4.** `policy-load` (the policy path, `policy show`, `policy check`, the closed state), `audit-export`, and `extract-csv-rows` after `pipeline-ports`.
- **P5.** `it-company-story` (the IT documents, IT-33 through IT-39) and `mutation-report` (the mutation gate over the label and egress modules).
- **P6 (R2).** `r2-company-ids`, `r2-pull-and-sync` and `r2-records`, with the company store and the hand-over runbook. Company data is held only after `mutation-report` of P5 has merged.
- **After P6.** Estimating, pricing and the rate book use the record envelope and `Register`, then estimate at completion and earned value. Each module is scheduled when the owner is in the seat and its preconditions have merged.

Pricing needs `core-policy` and the `export` destination first. No pricing card is scheduled before them.

## Deferred, with triggers

Each row below is a Later row of ADR 0001 (AD-24), which holds its trigger. This table names the slug and says which seam is built now. The two refusals that have no trigger are in the Never table of ADR 0001.

| Slug | Deferred | Seam already in place |
|---|---|---|
| `later-company-source-read` | Web, database and report adapters under `Pull`, and `GuardedPull` | One adapter by the adapter recipe. A destination kind for the host. The `Secrets` port over the Windows credential store, which serves the model key too if IT-38 asks for that. The feature `net-pull`, added by this row's card, which also gives the model client `net-model` |
| `later-feeds` | Scheduled snapshots of public sources, with their as-of date and freshness | No new port: a public feed is a `Pull` stream whose `[[source]]` row labels it `public`. The feature `net-pull` (added by `later-company-source-read`), vintage streams, `as_of` and `review_by` |
| `later-record-views` | Pages for typed records: list, record view, compare | The `title`, `search` and `show` keys of `types.toml`, the envelope and `rests_on` |
| `later-quotes` | Vendor quotes: breaks, normalisation, comparison, selection | The source kind `vendor_quote`, `TARGETS`, record types, `calc_run` and the `field` locator |
| `later-docx-import` | A Word extractor and its reader | Text supplied by hand |
| `later-pdf-native-ocr` | Native PDF and OCR extractors | Text supplied by hand |
| `later-project-reader` | MS Project reader | The first reader takes the XML export |
| `later-dataset-tier` | Dataset tier for ledger-scale tables | Original preserved and cited by hash |
| `later-snapshot-certificate` | Snapshot and certificate | The envelope already holds what it needs |
| `later-second-device` | Second device: `--new-device`, store join, per-device drop paths | The `writer` digits |
| `later-shared-store` | Server store, shared store on a share, writer-index handout | `Store` port, `share@1`, `served@1` |
| `later-accounts` | Accounts, single sign-on, an admin screen | `Principal` prefix `idp:`; `Session` constructor |
| `later-source-access` | Source access rules inside one store (an `Authz` adapter over collections) | `Authz::allow` and the `readers` group per store. A collection cell is a format amendment (ADR 0002), made when the first shared store needs it |
| `later-hosted-service` | A company-hosted service | A third build under its own ADR; `via = service` |
| `later-signed-journal` | A central or signed journal | A sink that tees events; the chain stays per writer |
| `later-separation-of-duties` | Separation of duties | `proposer` on `queue.accept` |
| `later-roles` | More roles and approval bands | New `Role` words with an `event_schema` bump |
| `later-label-remove` | `label remove` | The `kind` column of `labels.csv`; `label.change.removed`; the `changers` group; a `removable_by` key in policy schema 2 |
| `later-banner-allowance` | An allowance for a public document with a printed banner in a company store | `[[allow]]` keyed by hash, as `scan-allow.toml` is |
| `later-signed-policy` | Signed policy | `PolicySource::read()` can return a signature |
| `later-label-readers` | Readers per label and per item (need to know inside one store). Stage B adds a second person only to a store whose row names `readers` | `readers` on the store row (a Windows group), checked before the store opens |
| `later-sensitivity-properties` | Document properties that match the company's sensitivity-label scheme | `[label.properties]` in schema 2 |
| `later-audit-sink-network` | Audit sink beyond files | A network row, owner gate |
| `later-unshipped-limit` | `max_unshipped_hours` for egress kinds other than `model` | Built for `model` (section 5), where IT can turn it off. `audit.export` records `head` and `to_seq` |
| `later-mcp-company-data` | Company labels over `mcp` | The `mcp` row and `clients`. Reopen when a row can bind the launching program by path and publisher read from the process, and names the endpoint IT approved for it |
| `later-read-audit` | Read-access audit for local views | `mcp.call`, `refuse` and `ai.*` already carry ids and labels |
| `later-entity-resolution` | Correlation by record keys; entity resolution across systems | `origin_key` is the first entity key |
| `later-key-alias` | Alias column for a duplicate found under a second key | `FB0831` reports it |
| `later-register-specs` | The five hand-written registers as specs | `types.toml` |
| `later-batch-edit` | Per-row edits of a record batch at accept | Batch accept lists every item |
| `later-records-verify` | `records verify` (re-derive from the original) | `source_ref` on every row |
| `later-network-backup` | Network backup | `BackupTarget` |
| `later-remote-tools` | Remote tool server and chat for other computers | New transports return a `Session` |
| Never table | Encryption of a store by Fieldbook; sending a non-public label back to the home side or GitHub | Not planned. The fixed rows `git` and `edition` |

## What IT owns

| Area | Company IT or security owner | Where Fieldbook touches it |
|---|---|---|
| Device | Disk encryption, patching, endpoint detection, application allow-listing of `fieldbook.exe` and of the applications that may start `fieldbook mcp`, browser and operating-system features that read page text (IT-39) | Relies on it. Marks `working\` and `overlay\` as not content-indexed, and `check` fails when the mark is missing |
| Identity | Windows logon, access to the laptop, the groups named in `readers` and `changers`, any later single sign-on | Reads the account name from the process token for `actor`. Does no authentication |
| Network | Proxy, TLS inspection, which hosts the laptop may reach | Connects only to a policy-approved endpoint named in settings |
| Policy | Writes, signs off, versions and distributes `policy.toml`; sets its access-control entry | Reads, hashes, journals changes, `policy show`, `policy check` |
| Label scheme | Defines labels, markings, source rules, which store may hold which label, NDA counterparties, competitions | Applies them |
| Controlled data | Decides whether any laptop or store may hold controlled unclassified or export-controlled data, and which people may access it | Holds a label only where `holds` lists it; sends it nowhere unless a row says so |
| Endpoints | Contracts, retention, training terms, operator and location, approval and expiry | Sends only under a row |
| Logging | Collector, alerting, retention of the exported journal, the sink's audience, keeping the `head` apart, and whether a model call with a non-public label must wait for a recent export (`require_fresh_export`) | Writes the export files, and, unless IT turned the requirement off, refuses a model call with a non-public label when the export lapses |
| Records retention | Which system is the record, retention schedules, legal hold, the disposal method, hand-over (IT-36, IT-37) | Deletes nothing of the company's. Purges only call records, by `keep_days`, capped by the row. `hold = true` refuses that too. Promises no secure overwrite |
| Backup | Where backups may go, who may read them | Writes only under a `backup` row |
| Connectors to data sources | Service accounts or delegated access, the secrets store (IT-38) | Reads a credential from the Windows credential store. A credential is never in Fieldbook's files, the journal or an error line |
| Incident response | Detection and response | The journal and `refuse` events are the evidence |

**What Fieldbook does not claim.**

- Compliance, certification or authorisation under any scheme.
- That it is an enclave, or a place where controlled data may be held. The policy's `holds` says what a store may hold. The company decides whether the laptop may hold it.
- That it is a data-loss-prevention product. It checks the paths it owns. A screenshot, a copy by hand, a photo or a file copy by the user are outside it.
- That a label is correct.
- That it authenticates users or tool clients. The operating-system logon does. `clientInfo` is self-declared.
- That it encrypts data at rest. It relies on device encryption.
- That it decides whether content is controlled or export-controlled. It reads declared labels and markings.
- That it checks who is a US person. It has no user attribute.
- That an endpoint's operator keeps or trains on nothing. That is a contract.
- That the policy file cannot be changed by the user. The program refuses a file that a non-administrator can write (FB0418). An administrator who writes it is outside the program, and the hash in an exported journal is what shows it.
- That the egress token is a boundary. It stops honest mistakes in the crates the tests cover. The check is the release scan of the built binary, and IT's endpoint controls are the boundary.
- That a pack, a printed page or a copied text stays where it was sent. Those kinds are advisory.
- That a user outside a `readers` group cannot see company text. The group, which the company's directory keeps, decides. Fieldbook checks it before it opens the store.

Terminal presence, a writable role setting and Git author text are accidental-use checks and attribution, not proof of a human identity. The agent must not run acceptance, label changes or certification; consequential decisions use the owner review flow outside these writable settings. The scripted PTY drill tests mechanics and grants no authority to approve real work.

A nonempty backup selection containing only known, validated text-free controls is public. Unknown or malformed files in a selected subtree contribute `unlabelled`; ordinary data keeps its effective label union. This permits an audit-only backup without treating unknown state as public.

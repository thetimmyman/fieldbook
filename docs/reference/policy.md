# The company policy file

This is the reference for the policy file of [ADR 0004](../decisions/0004-company-boundary.md): where it is read from, what it may say, how the program merges it with the owner's settings and what every destination kind enforces. The file belongs to the company. Fieldbook reads it and never writes it. IT can read this document without reading the code; Codex builds from it.

Cards that build it: `core-labels` (the label table, subsets, the labeller, the label ledger), `core-policy` (the parser, the floor, `egress::check`, `LabelledView`, the leak matrix, the `Authz` port), `policy-load` (the path, the writer check, `policy show`, `policy check`, the closed state) and `audit-export` (the `[audit]` sink and the freshness limit). Every value in the examples is synthetic.

## What the file decides

| The file decides | The file cannot decide |
|---|---|
| The label table: ids, titles, banners, and which labels are restricted | Whether a label is correct. The program applies the table and the rules; it does not read meaning |
| Which text marks a file, and which labels a marking gives | A built-in label. `public`, `unlabelled`, `marked` and `owner-supplied` are in code |
| For each company data source: the folder tree it may read, its default labels, path rules and the map from its own label field | The things the program refuses whatever the file says (ADR 0004, section 13) |
| Which labels each store may hold, the labels of a note written in it, the Windows group that may open it, the group that may run `label add` on it, and a hold | That `unlabelled` or `marked` may go anywhere. A row that lists either is invalid |
| For each destination, the labels it may receive, with an optional expiry | A fixed row: `git`, `edition` and `audit` |
| Where exports and backups may be written, which tool clients may call, how long call records may be kept | A company label for the tool server or the clipboard in version 1 |
| The folder `audit export` writes to, whether a model call with a non-public label needs a recent export, and how old the last export may be | What an operator does with data it receives. That is a contract |

A store default or global `[[marking]].labels` that normalizes to public-only is invalid (FB0413): neither is a public act. Public permission lists and root-bound public `[[source]]` rules remain valid. Deriving only from public inputs also remains public.

## Where it is read from

The preset is decided by what is on the machine, never by the name of a folder. `Layout::decide` picks the **Laptop** preset when a company store (`overlay\store.toml`) exists under the data root, or the machine file exists, or the state folder lies under the user's local application data. Anything else is **Home or CI**.

| Preset | Path | Can the owner redirect it? |
|---|---|---|
| Laptop | `%ProgramData%\Fieldbook\policy.toml`. The folder is asked of Windows by its known-folder call. It is not read from an environment variable | No. The release binary has no flag that names a policy or moves a laptop folder. A machine file wins over everything else |
| Home and CI | None: the floor applies | Only in a test build (feature `test-hooks`), which accepts `--policy FILE`, `--program-data DIR` and `--local-app-data DIR` |

- The path comes from `Layout::policy_path()`. `PolicySource::read()` is the one function that returns the bytes. A central source later replaces that function and nothing else. A test fails any other read of a policy path.
- A policy inside the data root or the Corpus is refused (FB0414), and so is one inside a git work tree. Those are places the owner can edit. A test build may name a fixture inside a checkout.
- Before the file is read, the program asks the operating system who can write the file and its folder. If anyone but administrators and the system account can, the state is Closed (FB0418). So the owner's write access is checked, not assumed. The file is at most 1 MiB.
- On the Laptop preset a flag that names a state, overlay or working folder other than the fixed laptop folders is refused (FB0419). It would start a state folder with no record of the policy.
- An administrator can still write the file. Designated egress and outward-write events carry the policy hash, and `policy.change` records changes to the observed policy ([event contract](audit-contract.md)); `policy show` prints the path, the hash and the result of the writer check, and the first line of every AI verb prints `policy: <state> <first eight hex digits of the hash>`. A different file shows in an exported journal.
- `policy check FILE` reads a draft in any release build and installs nothing.

## States

| State | How it arises | What Fieldbook does |
|---|---|---|
| Floor | No policy file is found, and no policy has been loaded here before | The floor policy, below |
| Loaded | The file is found, passes the writer check, parses, every rule below holds and its `revision` is not lower than the highest recorded | Uses it. If its hash differs from the last recorded one, appends `policy.change` |
| Closed | A file is present and unreadable, invalid, writable by a non-administrator (FB0418) or an example file; or its `revision` is lower than recorded; or a policy was loaded here before (a revision is recorded) and the file is now missing | Every destination is denied except the audit sink. `check`, `build`, `search` and `status` still run. `policy.change` is appended with `outcome = absent` or `invalid` |

`revision` is the company's own counter. Fieldbook records the highest one it has loaded in the state folder. It prevents someone putting back an older, more permissive file. IT can still roll back: a roll-back is a new revision with old content.

## The floor policy

The floor is built into the program. It applies when there is no policy file to load and none was loaded before. There are two floors, chosen by one fact: whether a company store is on the machine.

| Part | Floor A: no company store on the machine | Floor B: a company store and no policy |
|---|---|---|
| Labels | The four built-ins | The same |
| Stores | `corpus` holds `public` only. `overlay`, `working` and `state` hold `public` and `unlabelled`. `state` also holds `owner-supplied`, because its logs do | The same, and `overlay`, `working` and `state` also hold `marked`. The owner reads, searches, calculates and drafts over it in place. No destination takes it |
| `handoff` | Allows `public` and `owner-supplied` | The same |
| `model` | A model endpoint named in `settings.toml` allows `public` | Nothing. No model endpoint receives anything |
| `mcp`, `bridge`, `print`, `clipboard` | Allow `public` | The same |
| `export` | Allows `public` and `owner-supplied` | Refused. It needs a policy row with a path |
| `backup` | Allows `public` and `owner-supplied`. A snapshot that holds `unlabelled` or `marked` items is refused (FB0901), because neither is in that set | One route: a target that the owner marked `company_storage = true` in its `[[backup.target]]` table of `settings.toml`, that the place test finds local or on the network. Through it a snapshot may hold `public`, `unlabelled` and `owner-supplied`, and never `marked`, because a backup target is a new place with its own readers. A snapshot that would hold `marked` is refused (FB0901) with the count. The journal records `dest`, `labels` and `policy = floor` |
| Fixed rows | As below | As below |

Floor B is the laptop that IT has not reached yet. The laptop is company infrastructure and the files on it are ones the owner can already open, so floor B holds them, `marked` files included, and the owner reads, searches and drafts over them in place. What leaves is closed: no destination takes `marked`, a thing derived from a marked file is marked too and stops at the same doors, and the on-disk overlay site includes `marked` pages because it is a derived copy in the same per-user tree and is never exported or backed up. The one backup route is there so that the owner does not copy company data by hand to a place no one checked. It never takes `marked`, so one marked file stops the tool's own backup until a policy maps the marking and `label reapply` has run.

The floor in one sentence. Every other statement of it quotes this block word for word, and `Policy::floor_sentence()` writes it from the floor table in code:

> **Floor.** Until a policy file has been loaded on this machine, the floor applies. The stores `overlay`, `working` and `state` hold `public` and `unlabelled`, `state` also holds `owner-supplied`, and `corpus` holds `public`. Handoff, exports and backups receive `public` and `owner-supplied`. A model endpoint named in the owner's settings, the tool server, the chat Bridge, print and the clipboard receive `public`. On a machine that holds a company store, the stores `overlay`, `working` and `state` also hold `marked`, no model endpoint receives anything, exports are refused, and a backup to a folder the owner recorded as company storage may also hold `unlabelled`. No endpoint is approved, and nothing marked leaves.

A loaded policy gives a store with no `[[store]]` row its floor row without `marked`: under a loaded policy only a `[[store]]` row makes a store hold `marked`. Files already held stay where they are, `check` reports each one whose store no longer holds its labels (FB0412), and IT either lists `marked` in the store rows or maps the marking in a `[[marking]]` row so that `label reapply` resolves it. `ai status` prints "policy: none (floor)" while the floor applies. The policy hash in the journal is the SHA-256 of the file's bytes; for the floor it is the SHA-256 of the text `fieldbook-floor-policy-1`, a constant of the program.

## Schema, version 1

The parser denies unknown keys. A newer policy may carry a restriction an older program cannot enforce, so an unknown key or table, or a `policy_schema` the program does not know, makes the whole file invalid and closes egress (FB0413).

| Table | Keys | Meaning |
|---|---|---|
| top level | `policy_schema` (integer, required, `1`), `revision` (integer, required, at least 1), `example` (boolean, optional) | The schema and the company's counter. `example = true` marks the shipped example. No program loads a file that has it |
| `[[label]]` | `id` (required), `title` (required), `banner` (optional), `restricted` (optional, default false) | A label. The title is for people and is never journaled. The banner is printed on pages, exports and prints. A title and a banner are at most 80 code points with no control character. A restricted label may be held only by a store row that names `readers` |
| `[[marking]]` | `match` (required), `labels` (required, at least one) | Text found in a file maps to labels. `match` is literal text of 3 to 80 characters, compared case-insensitively with runs of whitespace collapsed |
| `[[source]]` | `id` (required), `root` (required), `default` (required, at least one label), `[[source.rule]]` (`prefix`, `labels`), `[[source.map]]` (`native`, `labels`) | A company data source. `id` is the system id in `systems.toml`. `root` binds the id to a folder tree |
| `[[store]]` | `id` (required), `holds` (required; may be empty), `default` (optional; a subset of `holds`), `readers` (optional), `changers` (optional), `hold` (optional, default false) | What a store may hold, the labels given to a note the owner writes in it, the Windows group that may open it, the group that may run `label add` on it, and a hold that refuses purge and label changes |
| `[[destination]]` | `kind` (required), `allow` (required; may be empty), and by kind `name`, `url`, `expires`, `keep_calls_days_max`, `ref`, `clients`, `paths` | Where labels may go |
| `[audit]` | `sink` (required if the table is present, unless `require_fresh_export` is false), `max_unshipped_hours` (optional), `require_fresh_export` (boolean, optional, default true) | The folder `audit export` writes to, how old the last export may be, and whether a model call with a non-public label needs a recent export. Described [below](#audit) |

A company label id matches `[a-z]{2,4}-[0-9]{4}` (`np-0007`, `bid-0003`). It is an opaque code. The id goes to the journal; the title, which may name a counterparty, does not. The four built-in ids are the only other ids.

### Labels, markings and sources

- A `[[marking]]` row that matches adds its labels to the file. The built-in banner shapes of FB0400 are also markings. A built-in shape that no row covers gives the file the label `marked`.
- A `[[source]]` row applies only to a stream whose folder lies inside its `root`. `root` is an absolute path, matched on whole components of the canonical path, or the word `drop`, which means the default drop folder of that system (`state\drop\<system>\`). `sync` refuses a stream whose folder lies outside the root (FB0835) and journals a `refuse` with the reason `root`. It records optional `root_sha256` and the policy hash on `sync.begin`, using [the source-root fingerprint](audit-contract.md#source-root-fingerprint). The failure this prevents: the owner repoints a source whose default is `public` at any folder, and everything in it arrives as public.
- A `[[source.rule]]` `prefix` is compared with the start of the item's key as the stream writes it, with `/` as the separator, case-sensitive. Every rule that matches adds its labels. `default` always adds its labels.
- A `[[source.map]]` `native` is the exact text of the source's own classification field, trimmed. An item whose native label has no map row gets `marked`.
- The results of the markings, the map, the rules and the default are unioned, then the store default applies to text the owner writes. Nothing found gives `unlabelled`. The labeller never lowers a label and never edits or removes a marking.
- `public` is never a default of the program. It comes from an act: the edition build, a `[[source]]` row for a stream inside its `root`, or, on a machine with no company store, a person's typed confirmation to `corpus-add`, which the journal records.
- `owner-supplied` labels what a person typed or pasted into one request, and the log lines and call records of that request. No page, source version, record, original or note carries it. It is refused in a `default`, a `[[source]]` row and a `[[marking]]` row, and in the `holds` of every store but `state`. `label add` and `label reapply` never give it.

### Stores

| Key | Rule |
|---|---|
| `id` | `overlay`, `working` or `state`. A row for `corpus` is invalid: the repository Corpus is public by definition |
| `holds` | Label ids, and the built-ins `public`, `unlabelled` (and `owner-supplied` for `state` only; `marked` too if the company wants it held). A write is refused if any label of the record is outside `holds` (FB0412) |
| `default` | The labels of a note written in the store. A subset of `holds` |
| `readers` | A Windows group (name or SID). A person whose logon token is not in it cannot open the store: no search, index, site or pack is built from it for them. Required when `holds` lists a restricted label. The company's directory, not Fieldbook, decides who is in the group |
| `changers` | A Windows group that may run `label add` on items of the store. Absent means any person who passes the console test |
| `hold` | `true` refuses purge and label changes in the store |

`state` is a transit store. `sync` and `add --propose` copy a file into the inbox and write staging text and a proposal there. Such an item is admitted by the `holds` of the store it is bound for, so a label is listed once and an intake does not refuse every file because `state` was forgotten. The `holds` of `state` itself governs its logs, the question log and call records, and the derived site: `build --overlay` is refused (FB0412) when `state` does not hold a label the site carries, so a policy that lets `overlay` hold `marked` lists it for `state` too if the site is to include those pages.

### Destinations

| Key | Kinds | Rule |
|---|---|---|
| `kind` | all | `model`, `handoff`, `mcp`, `bridge`, `export`, `backup`, `print` or `clipboard`. `git`, `edition` and `audit` are fixed rows and invalid here |
| `allow` | all | Label ids from the table or the built-ins `public` and `owner-supplied`. `unlabelled` and `marked` are invalid. For `mcp` and `clipboard` only `public` in version 1 |
| `name` | `model` (required), `handoff` (optional) | The endpoint name in `settings.toml`. Other kinds: invalid |
| `url` | `model` (required) | `https`, or a loopback address. It must equal the endpoint's `base_url` in settings |
| `expires` | all | A TOML local date such as `2027-06-30`, not a quoted string. The row applies through that date and is expired the day after |
| `keep_calls_days_max` | `model` | An integer, at least 0. The longest call records may be kept. 0 means none |
| `ref` | all | The company's change or approval id. Up to 80 code points. Shown by `policy show`; never journaled |
| `clients` | `mcp` | A list of client hashes: the SHA-256, in lowercase hex, of the client name as the client declares it. A refused session prints the hash on the terminal so that IT can list it. Absent means any client. The name is self-declared, so the list guards against a different program by accident, not against an attack |
| `paths` | `export`, `backup` | A list of absolute path prefixes, matched on whole components of the canonical path (junctions and links resolved; case-folded on Windows). Other kinds: invalid |

A destination with no row is denied. If more than one `export` row's `paths` contains a target, the allowed set is the union of those rows.

An absolute path is a drive letter and a separator (`C:\Example\Out` or `C:/Example/Out`), a `\\server\share` form, or a path that starts with `/`. The form is judged as text on every host, so the file that IT writes on Windows is valid on the owner's home machine too; a prefix that a host cannot reach matches nothing there. A relative path, an empty path and a path with a `..` component are invalid.

A path is also tested as a place, not by its name. `World::place` reports a reparse point, a cloud-file attribute or sync-root registration, and a network drive. A live store and the state folder must be local. A drop folder may be on the network but not a sync root. An export or backup path may be on the network, and a sync root only when the row's `paths` names it. The result is journaled with the write (FB0415). At home no row exists, so a backup target in a sync root is the warning FB0813 (a one-line notice when the target says `kind = "synced"`), not a refusal.

### Audit

| Key | Type | Default | Effect |
|---|---|---|---|
| `sink` | Text: an absolute path | None. Required when the table is present, unless `require_fresh_export` is false | The folder `audit export` writes to. With no sink the verb exits 1 with FB0806 |
| `max_unshipped_hours` | Integer | None. Required by a `model` row that allows a non-public label while `require_fresh_export` is on | The longest the last `audit.export` may be old. Step 6 of [the decision](#the-decision) reads it |
| `require_fresh_export` | Boolean | `true` | IT sets it. On (`true`, or the key absent): a model call with a non-public label needs a recent export, and step 6 denies it with FB0925 when the last export is older than `max_unshipped_hours` or there has been none. Off (`false`): step 6 never denies, FB0925 is never raised, a `model` row needs no `sink` and no `max_unshipped_hours`, and a `max_unshipped_hours` left in the table does nothing |

A value of `require_fresh_export` that is not `true` or `false` is invalid (FB0413). The key does not change `audit export`, which still writes to the sink when there is one.

IT turns the requirement off in a new `revision`. An auditor sees that it was off through the policy hash that Fieldbook already records. `ai.begin` and the call record of every model call carry the hash of the file in force, `policy.change` marks each revision with its hash, and `policy show` prints the `[audit]` table with the requirement as `on` or `off`. The file at that hash, from IT's copy of the revision, holds the line `require_fresh_export = false`. Fieldbook keeps no other record of the setting.

## Rules the loader enforces

Every rule in the table raises FB0413, except the writer check, which raises FB0418.

| Rule | Example |
|---|---|
| `unlabelled` or `marked` in any `allow` | `allow = ["np-0007", "unlabelled"]` |
| A destination of kind `git`, `edition` or `audit` | `kind = "edition"` |
| A store row for `corpus` | `id = "corpus"` |
| An `allow`, `holds`, `default` or `labels` id that is not in the table or the built-ins | `allow = ["np-0099"]` with no such `[[label]]` |
| A company label id that is not opaque, or is a built-in; a duplicate id in `[[label]]`, `[[store]]` or `[[source]]` | `id = "acme-widget"`, `id = "public"` |
| `owner-supplied` in a `default`, a `[[source]]` row, a `[[marking]]` row, or the `holds` of any store but `state` | `default = ["owner-supplied"]` |
| A restricted label in the `holds` of a store with no `readers`; `readers`, `changers` or `hold` on `corpus` | |
| A `default` that is not a subset of `holds` | |
| A store `default` or global `[[marking]].labels` that normalizes to public-only | `default = ["public"]` in a store row |
| A `[[source]]` row with no `root`, or a relative `root` | `root = "exports"` |
| A non-public label in an `mcp` or `clipboard` row | `kind = "mcp"`, `allow = ["np-0007"]` |
| A `model` row that allows a non-public label, when `require_fresh_export` is on and `[audit]` has no `sink` or no `max_unshipped_hours` | |
| A `require_fresh_export` that is not a boolean | `require_fresh_export = "no"` |
| A model row without a `url`, or with a `url` that is neither https nor loopback | `url = "http://gateway.example"` |
| A `paths` key on any kind but `export` and `backup`; a relative path or one with a `..` component | `paths = ["exports"]` |
| A `clients` key on any kind but `mcp` | |
| `example = true` | The shipped example file |
| An unknown key or table, or a `policy_schema` the program does not know | |
| A `revision` lower than the highest recorded | |
| A file over 1 MiB, text that is not valid UTF-8, or a second byte order mark after the first (one leading mark is skipped before the parse, because Windows tools write one; the hash is of the bytes as read, mark included) | |
| A file or folder that a non-administrator can write (FB0418) | |

A label used in a row but not defined fails here, which is the check that fails on drift when someone adds a label in one place only.

`policy check` also prints a `warning` line, with the line number, for each value that still looks like a placeholder: a host that ends `.example`, a path that starts `C:\Example` and a ref that starts `CHG-EXAMPLE-`. A half-edited copy is easy to see.

## Narrow only: the owner's settings

The effective allow set of a destination is the policy row intersected with the owner's list in `settings.toml`. The owner can narrow and cannot widen. Every settings key that meets the policy declares a merge rule in `KEY_TABLE`; a key with no merge rule fails a test.

| Settings key | Merge rule | Policy side |
|---|---|---|
| `[model.endpoint.<name>] allow` | List: intersect | The `model` row's `allow` |
| `[model.handoff] allow` | List: intersect | The `handoff` row's `allow` |
| `[mcp] allow` | List: intersect | The `mcp` row's `allow` |
| `[model] keep_calls` | Switch: AND | `keep_calls_days_max` above 0 |
| `[model] keep_days` | Number: minimum | `keep_calls_days_max` |
| `[model] order` | A word list: the policy decides which entries are usable | Each entry's effective allow set |

An endpoint with no `allow` in settings accepts nothing. `ai status` and `policy show` print both sets and which side cut each label. The `role` word in `settings.toml` is not a policy key. On the Laptop preset the program does not read it to decide who may run `label add`.

## Destination kinds

The const `DEST_KINDS` in `fieldbook-core` is the one list of kinds. A test, `dest_kinds_match_policy_docs_and_matrix`, compares it with the kinds the loader accepts, with the rows of this table and with the columns of the leak matrix. Each kind also says whether it is advisory and whether it shows the count of records left out.

| Kind | Source | Left-out count shown | Covers | Row matched by | What Fieldbook enforces | What it cannot enforce |
|---|---|---|---|---|---|---|
| `model` | Policy row | No | An HTTPS model endpoint named in settings | `name`, and `url` equal to `base_url` | Allow set, URL binding, expiry, call-record retention cap, journal before send, and for a non-public label a recent audit export (FB0925) unless IT turned the requirement off | What the operator keeps or trains on. That is a contract |
| `handoff` (advisory) | Policy row | Yes | A pack file the owner carries to an approved assistant | Optional `name` | Allow set. The pack's first line prints its labels. Records outside the set are left out and counted | Which assistant the owner pastes into |
| `mcp` | Policy row | No | Results returned over `fieldbook mcp` | The kind; optional `clients` | `public` only. Journal before the reply. The `clients` list | Who the client really is, and where it sends what it receives. So no company label is served |
| `bridge` | Policy row | No | The chat Bridge listener on the loopback address | The kind | Whether the listener starts. Allow set for what the page shows. `Cache-Control: no-store` on the page and every `/api` route. Not offered for a non-public label when the machine has more than one interactive logon | Another program of the same user that reads the port |
| `export` | Policy row | Yes | Any file written outside the data root | `paths` prefix on the canonical path | Allow set, path prefix, the place test, the label banner and metadata in the file | What happens to the file after it lands |
| `backup` | Policy row | n/a | A backup target | `paths` prefix | Allow set, path prefix, the place test (FB0415 on the laptop, FB0813 at home), the refusal FB0812 | The target's own access control |
| `print` (advisory) | Policy row | Yes | The print view of a page | The kind | Whether the page renders a print form, and a banner on every printed page | The browser's own print |
| `clipboard` (advisory) | Policy row | No | The copy controls in pages | The kind | `public` only. Whether the page shows them | Text selection and copy by hand |
| `audit` | Fixed | n/a | The journal export | | Journal lines only, no labelled content | |
| `git` | Fixed: `public` only | n/a | A source repository work tree | | Refuses labels outside public/owner-supplied under a `.git` ancestor (FB0415); owner-supplied is confined to ignored, never-committed home State and its restore drills, never Corpus content or commits; laptop live, State and drop roots stay outside Git | |
| `edition` | Fixed: `public` only | No | The public edition that ships | | The build refuses any other label | |

Writes inside the data root are store writes. Writes outside it are exports.

An advisory kind is a statement of what the page offers. Once a pack is shown, printed or copied, the program has no say. `policy show` marks these kinds "advisory". The assistant page carries the policy hash and an expiry time, and hides copy and print once it passes. A pack is journaled as `ai.pack`, which records what was put in the pack and not where it went.

The kinds that do not show the count give a fixed line, `Some records were omitted.`, with no number. A count tells a reader how many restricted records matched a query. The matrix asks the same question with 0 and with 3 restricted records and requires byte-identical replies from `mcp` and the model.

## The decision

`egress::check(policy, dest, labels, delivery, now, shipped) -> Decision` is the only function that decides.

`labels` is `Option<&LabelSet>` and `delivery` is `Option<&Delivery>`. With both absent, check only destination availability (state, row, expiry, URL, client and path) and return its complete narrowed `LabelAllow` for view selection; no data labels are invented. With labels present and no delivery, also check the data labels and model freshness, but return no sink authority. With both present, check the complete delivery and bind its exact operations. A delivery without labels is FB0902 (`mismatch`). An empty permission set stays empty and selects no content. Selection never converts permissions to data labels. The order below applies to a data preflight or final delivery; the audit destination retains its existing Closed-state exception.

The function is pure. `now` comes from the system clock through one function, and no flag or variable reaches it in a release build. `shipped` is the age of the last `audit.export`. It decides in this order.

1. The policy state is Closed: deny (FB0413).
2. Any label is `marked` or not in the table: deny (FB0901). A label `unlabelled` is denied too, with one exception: a `backup` to a target with `company_storage = true`, at floor B, goes on to steps 3 and 4, where the floor row decides. A loaded policy has no such exception, because no row may list `unlabelled`.
3. There is no row for the destination, or the row is expired, or the model URL differs from settings, or the tool client is not listed: deny (FB0918).
4. The labels are not all in the row's `allow` intersected with the owner's list: deny (FB0901).
5. A marking in the outbound text maps to a label outside the request's set: deny (FB0902). The record was mislabelled.
6. A `model` call carries a non-public label, `[audit] require_fresh_export` is on, and the last export is older than `max_unshipped_hours`, or there has been none: deny (FB0925). With it off this step never denies.
7. Otherwise return an `Allowed` with private destination, request labels, permitted `LabelAllow`, policy hash and delivery binding. `delivery = None` is preflight-only; it cannot reach a sink. `Some(delivery)` binds exact canonical targets, operations, hashes and byte lengths, which each sink compares with its actual operation.

The caller appends the journal event first. Every outbound API takes the `Allowed` token. The rule is in the types: `io::atomic` has two doors, `write_in_root(RootPath, ..)` for a path under the data root, whose `RootPath` is made in one file, `io/root.rs`, from the `Layout`, and `write_outside(&Allowed, path, ..)`. A test fails if any other public writer exists. This is a design aid and not a boundary: a dependency, a macro or an unsafe block can still reach the operating system. So the release also scans the built binary for network, process-start and clipboard calls outside the gate crates, and that scan is the check. A `LabelledView` is built for one destination from an `Allowed` token: a record is in it only if all of its labels are in the allowed set, and a record outside is left out whole, never redacted. A pack ends with "Left out (policy): N records" and names none of them.

## Commands

| Command | What it does |
|---|---|
| `fieldbook policy show` | Prints the state, the path, the hash, the result of the writer check, the revision, every label (id and title), every rule, every row, the `[audit]` table with the export requirement as `on` or `off`, the fixed rows marked "built in", the advisory kinds marked "advisory", and the effective table after the owner's narrowing. Prints "policy: none (floor)" with no policy. It exits 0 for the floor and a loaded policy and 1 for a closed one, and it writes nothing |
| `fieldbook policy check FILE` | Validates a draft with no side effects. It exits 0 when the file is valid and could be loaded, 1 with one FB0413 line for each finding when it is not (an example file is a finding), and 2 when the file cannot be read. On success its first line starts `policy ok:` and its second line is the `hash:` of the file. It also says whether the file would close the program because its `revision` is lower than the recorded one, and warns about placeholders. IT can test a file before installing it |
| `fieldbook label add` | A person at a console adds a company label to a page, a source version or a record, or resolves `unlabelled` by choosing company labels. The program needs an interactive console on input and output and a typed confirmation that names the number of items, so redirected input or output is refused (FB0822); a terminal and role setting do not prove human identity. If the store row names `changers`, the logon token must be in that group. The change is a line in the store's `labels.csv`, and a `label.change` event records the before and after sets. It never adds `public`, `unlabelled`, `marked` or `owner-supplied`. There is no `label remove` in version 1 |
| `fieldbook label reapply` | Re-runs the labeller under the current policy. It can only add. It follows the same console rule |

## Evidence

- The label-leak matrix runs a synthetic policy with several labels and every destination kind. For each label it plants a canary string in a record and runs every output path. The canary appears only where the policy allows its label. A `marked` record held at floor B is planted too, and no destination takes it. It also plants a canary in `working\`, in the site folder and in a stray file in a root, and asserts that a backup is refused or labelled for each.
- Each control has a negative control that fails when the control is removed: skip `egress::check` in the model send; build an `Allowed` by hand; drop the subset test; treat `unlabelled` as allowed for every destination, or under a loaded policy; let `marked` pass step 2; add `marked` to the floor B `backup` row; let a floor A store hold `marked`; let the owner's list widen; read the policy from the data root; accept a lower `revision`; write a company-labelled file under a `.git` ancestor; run `label add` with input from a pipe; repoint a `public` source at a canary folder; take the policy path from a flag in a release build; send with a stale audit export; read a missing `require_fresh_export` as off.
- A mutation gate runs over the label and egress modules. A mutant that changes no behaviour goes in a skip file, with one reason line for each, that the owner reviews. No other mutant survives.
- `docs/intake/policy.example.toml` is a synthetic example for IT to start from. The company's real file never enters a repository.

## Delivery and refresh

Permission lists use LabelAllow, retaining empty and mixed-public sets. Data labels use LabelSet. A view-selection token cannot leave the process. The final egress decision binds the actual canonical destination, operation, payload hash and length; each sink checks them. Read the current policy, writer facts, actor, clock and audit-export head before each content reply or outbound attempt, including retries. A preview also binds its content generation and is invalid after either generation changes. Revocation stops the next attempt; it cannot recall a request already sent.

## Consistent reads

With no state, a read-only command snapshots the initial tree and confirms state is still absent afterward; if state appeared, discard and retry under its lock. A writer publishes state/store.lock before any content mutation. A present state with incomplete lock setup yields retry/refusal. Readers never create state. Ordinary `World::read_tree` reads a generation under a shared StoreLease. It loads the source rows, records, evidence, labels and index inputs before releasing the lease; the resulting owned model carries the generation hash. A writer takes `World::write_lease()` and loads through `World::read_tree_under(&lease)`, which checks the roots and never reacquires the lock. It retains the lease for its whole journaled batch, writes the generation marker last, then releases it. Read and write leases use the same state/store.lock and cover the home Corpus/state roots initially and every Layout root once company storage exists. An in-root PendingWrite intent makes a fresh read unavailable until recovery completes; a reader never treats partially written files as a committed generation. The exclusive read-under-lease path may expose a pending tree only to the matching recovery handler, which validates the persisted intent and before/after hashes before resuming. The descriptor holds the ordered relative paths and before/after hashes, labels, verb, begin sequence and source generation, never file text; its relative paths still make it labelled metadata for backup. A third live hash or changed recovery plan refuses. A successful end with leftover bookkeeping is completed only after verifying all after hashes. Pending trees cannot enter rendering, search or outbound preparation. An unfinished outward export has an audit warning but no pending input-store generation; state-only atomic rewrites need no multi-file intent. No caller holds a store or journal lease across a prompt or network wait. An open Journal is an idle handle: initialization and ordinary appends take short journal leases, while begin/end and state-only writes retain an operation lease. Appends within a transaction reuse its lease. After confirmation, take the store write lease and recheck the displayed plan and current authorization; a change refuses the write. Lock order is store lease, then journal lease; never acquire a store lease while holding a journal lease.

Before an outward operation, refresh policy bytes, writer facts, actor facts, owner narrowing, current clock and audit-export head. A preview records policy hash and content generation; either changing invalidates it. Each retry and schema-repair attempt repeats preparation, budget/marking checks and exact-payload audit. Already sent requests cannot be recalled.

Derived State writes use `Journal::state_scope(&StoreLease) -> Result<Audited, JournalError>` after validating an exclusive lease for the same roots. Acquisition validates the journal head and retains the journal lease through the write without adding an event; active operations reuse their lease. An optional saved-report failure warns and preserves the prior report and command exit. A `status --page` failure exits 3. Text-bearing metadata retains labels and current input provenance; only strictly validated text-free controls use the control-file exception.

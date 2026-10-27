# The policy file: what IT decides and writes

This guide is for the people at the company who own the policy file for Fieldbook. The file is one text file. It says which labels exist, which stores may hold them, which company data sources may be read, and which destinations may receive which labels. Fieldbook reads the file and never writes it. Fieldbook enforces what the file says and does not judge whether the file is right: the decisions are the company's, and Fieldbook makes no claim of compliance with any scheme.

The first edition (Tier 0) contains no program and reads no policy file. The file matters from the signed program (Tier 1), and the questions it answers are IT-33 through IT-39 in [it-questions.md](it-questions.md). The keys are described in full in the [policy reference](../reference/policy.md); this guide does not repeat them. A synthetic file to start from is [policy.example.toml](policy.example.toml). Every code, name, address and path in it is made up.

## What you decide

| Decision | Where in the file |
|---|---|
| Which labels exist, how they are named, and which are restricted | `[[label]]` |
| What marks a file, and which labels a mark gives | `[[marking]]` |
| Which company data sources are read, from which folder tree, and what their items are labelled | `[[source]]` |
| What each store may hold, who may open it and who may change labels in it | `[[store]]` |
| Where each label may go | `[[destination]]` |
| Where a copy of the audit journal is written, whether a model call with a non-public label must wait for a recent copy, and how old the copy may be | `[audit]` |
| Who can change the file | An access-control entry on the folder. It is not in the file |

Without the file the built-in labels are `public`, `unlabelled`, `marked` and `owner-supplied`, and:

> **Floor.** Until a policy file has been loaded on this machine, the floor applies. The stores `overlay`, `working` and `state` hold `public` and `unlabelled`, `state` also holds `owner-supplied`, and `corpus` holds `public`. Handoff, exports and backups receive `public` and `owner-supplied`. A model endpoint named in the owner's settings, the tool server, the chat Bridge, print and the clipboard receive `public`. On a machine that holds a company store, the stores `overlay`, `working` and `state` also hold `marked`, no model endpoint receives anything, exports are refused, and a backup to a folder the owner recorded as company storage may also hold `unlabelled`. No endpoint is approved, and nothing marked leaves.

The floor is described in the [policy reference](../reference/policy.md#the-floor-policy). A banner that Fieldbook recognises and no `[[marking]]` row covers gives a file the label `marked`, and text that nothing labels is `unlabelled`. On a machine that holds a company store and has no policy file, both are held in the company store for the owner's own use. With a policy loaded, neither goes to any destination. Without one, `marked` goes nowhere, and `unlabelled` goes only by the backup route in the floor sentence above.

Three destinations are fixed and cannot be changed by the file: the Git work tree and the edition take `public` only, and the journal export takes journal lines only. A label can be used in a row only if it is defined in `[[label]]` or is one of the built-ins, and `unlabelled` and `marked` can never be listed in a row's `allow`.

## Who can change the file, and what shows

- The program reads the file from `%ProgramData%\Fieldbook\policy.toml` on the laptop. The folder comes from the Windows known-folder call, not from an environment variable, and the release program has no flag that names a policy or moves a laptop folder. A policy inside the data folder or the Corpus is refused (FB0414), and so is one inside a Git work tree, because those are places the owner can edit.
- Before it reads the file, the program asks Windows who can write the file and its folder. If anyone but administrators and the system account can, every destination closes (FB0418). So the owner's write access is checked, not assumed.
- An administrator can still write the file. The evidence is the policy hash: designated egress authorization and outward-write events carry the hash used for authorization, and `fieldbook policy show` prints the observed hash. The next journal-writing operation records a changed observation with `policy.change`; read-only commands append nothing. A copy of the journal that IT collects from the audit sink, written by `fieldbook audit export`, shows a replaced journal (IT-35).
- `revision` is the company's own counter. Fieldbook records the highest one it has loaded, and a file with a lower one closes every destination. This stops an old, more permissive file being put back by accident, for example from a restored copy. A roll-back is a new revision that carries the old content.
- The file is not read from a test build's `--policy` flag in the release program. Those flags exist only in a build that the company never receives.

## Write, check, install, verify

1. Copy [policy.example.toml](policy.example.toml). The example holds `example = true`, and Fieldbook will not load a file that has it. Replace the codes with the labels from your answer to IT-34, set `revision = 1`, delete the line `example = true`, and delete the rows you do not need.
2. Check the copy on any machine that has the program: `fieldbook policy check FILE`. It changes nothing. It prints `policy ok` and exits 0 when the file is valid. It prints one `error[FB0413]` line for each finding and exits 1 when it is not. It exits 2 when the file cannot be read. It also says `would close` when the file's `revision` is lower than the one recorded on that machine.
3. Install the file as `%ProgramData%\Fieldbook\policy.toml`, with an access-control entry that lets only your administrators write it.
4. On the laptop run `fieldbook policy show`. The first line says `policy: loaded`. The path, the hash and the result of the writer check are printed; compare the hash with `Get-FileHash -Algorithm SHA256` of the installed file. `policy show` also prints each label, each row and the effective table after the owner's own settings have narrowed it.
5. After the next authorized operation that opens the journal for writing under this policy, run `fieldbook audit show --kind policy.change`. Its line carries the new observation hash, the previous hash (zeros the first time) and the revision. The show/check commands above are read-only; absence of that event before a writing operation is expected.

## Rules that surprise people

| Rule | What happens | What to do |
|---|---|---|
| A label is used in a row but is not defined | The whole file is invalid (FB0413) and every destination closes | Define it in `[[label]]`, or take it out of the row. `policy check` finds it |
| `unlabelled` or `marked` in an `allow` list | Invalid (FB0413) | Label the data instead. Those two labels mean "nobody has decided" |
| A label ID that is not two to four letters, a hyphen and four digits | Invalid (FB0413) | Use an opaque code. The id is written to the journal, so it must not name a counterparty. The title may |
| A `restricted` label in a store that names no `readers` | Invalid (FB0413) | Add a Windows group as `readers`. A person outside the group cannot open the store, and nothing from it is searched, indexed, built into a site or put in a pack for them |
| A `[[source]]` with no `root`, or a stream whose folder lies outside it | The first is invalid. The second is refused when the stream is read (FB0835) and recorded | Name the folder tree the source may read. A source whose default is `public` can then not be repointed at another folder |
| A public-only store default or global marking rule | Invalid (FB0413) | Neither is an act declaring arbitrary new text public; a root-bound public source rule remains valid |
| `owner-supplied` in a `default`, a `[[source]]` or a `[[marking]]` | Invalid (FB0413) | That label is for text a person types into one request. It is never given to a record |
| A file with an unknown key or table, or a newer `policy_schema` | Invalid. A newer file may carry a restriction that this program cannot enforce, so none of it is applied | Use only the keys in the reference |
| `example = true` in the file | Invalid. The shipped example is never loaded | Delete the line in your copy |
| The `revision` is lower than the recorded one | The state is Closed | Raise the number. A roll-back is a new revision with old content |
| The file is removed after it was loaded | The state is Closed, not Floor. The recorded revision stays | Put the file back, or install a new one with a higher revision |
| A policy is loaded on a laptop that already holds `marked` files | The floor held them until now. A loaded policy decides what each store holds: a store holds `marked` only if its row lists it, and a store with no row holds `public` and `unlabelled` (and `owner-supplied` for `state`). The files stay where they are, `check` reports each one whose store no longer holds its labels (FB0412), and a write that carries `marked` to that store is refused | List `marked` in the `holds` of `overlay` and `working` if the company wants the laptop to keep holding those files, and in `state` too if the overlay site is to include them. Or add a `[[marking]]` row that maps the marking, and have the owner run `label reapply` |
| A destination with no row | It receives nothing (FB0918) | Add a row |
| A row past its `expires` date | The row applies through that date and is expired the day after (FB0918) | Renew the row with a new date and a new `revision` |
| A model row whose `url` differs from the endpoint's `base_url` in the owner's settings | Denied (FB0918) | Make them equal. Both the settings and the row must name the endpoint |
| A model row that allows a non-public label, with the export requirement on (the default) and no `[audit]` sink or no `max_unshipped_hours` | Invalid (FB0413) | Name a sink and a limit, or turn the requirement off (see Common changes) |
| A model call with a non-public label when the requirement is on and the last audit export is older than the limit, or there has been none | Denied (FB0925) | Run `fieldbook audit export`, or have your tools run it on a schedule |
| `require_fresh_export` that is not `true` or `false` | Invalid (FB0413) | Write `true` or `false`, without quotes |
| A label outside the row's `allow`, or cut by the owner's own list | Denied (FB0901). The owner's settings can narrow a row and can never widen it | Add the label to the row, or have the owner remove the narrowing |
| Text that carries a marking for a label outside the request | Denied (FB0902). The record was mislabelled | Fix the label. The marking is evidence that the labeller missed something |
| A record whose labels are not all allowed for a pack, a tool reply or a model request | The record is left out whole, never redacted. A pack, an export and a print end with `Left out (policy): N records`. A model request, a tool reply, the chat page, the clipboard and the edition page say only `Some records were omitted.` with no number, so a count does not show how many restricted records matched | Nothing, unless the record should have been allowed |
| A non-public label in an `mcp` or `clipboard` row | Invalid (FB0413). Version 1 serves public text only over the tool server, because any program that can start it passes what it receives to a model nobody approved | Leave `allow = ["public"]` |
| A label that was set too restrictively | It stays. Fieldbook adds labels and never removes or lowers one, and importing the file again does not help, because nothing is written for bytes it already holds | Add a source rule so that new items are labelled as you want, or decide it with the owner. `label remove` is not in version 1 |

## Common changes

Each change is an edit, a new `revision`, `fieldbook policy check FILE`, an install, and a look at `fieldbook policy show`.

- **Add a label.** Add a `[[label]]`, add the id to the `holds` of the store that should keep it, and to the `allow` of any row that should receive it. A label that no `holds` lists is refused at intake, which is the right setting for data that must not enter. Inbox, staging and queue use the destination store admission rule. State needs its own holds for labels retained in logs, asked rows, call records or eligible derived sites. A State row holding restricted labels needs readers; stores with readers do not produce a static overlay site.
- **Approve a model endpoint.** Add a `model` row with the endpoint's `name`, its `url` (https or a loopback address), the labels in `allow`, an `expires` date and a `ref` (your change number). The owner's `settings.toml` must name the same endpoint. `keep_calls_days_max = 0` allows no call records. Take the first approval small: `public` alone, to prove the route, then widen the row in a new revision after the contract review. A row that allows a non-public label also needs the `[audit]` sink and limit, unless you turn the export requirement off.
- **End an approval.** Set `expires` to the last day it applies, or delete the row. A deleted row stops applying when the new file is installed; an expired row stops the day after its date.
- **Allow an export folder or a backup target.** Add an `export` or `backup` row with `allow` and `paths`. A path is an absolute prefix, matched on whole path components after links and junctions are resolved. A snapshot that holds a label outside `allow` is refused (FB0901). A network share is valid. A folder that a sync client watches is valid only when the row's `paths` names it, and the program decides that from the folder's properties, not its name. A restore into a folder inside the data folder needs no row. A restore into any other folder is an export: it needs an `export` row that lists the folder and every label in the snapshot, whether or not a `backup` row allowed the same labels, and it is refused (FB0901 or FB0918) otherwise.
- **Let a tool client in.** Add the client's hash to the `clients` list of the `mcp` row. A refused session prints the hash on the terminal. Leave `clients` out to allow any client. Version 1 serves public text only.
- **Mark a new data source.** Add a `[[source]]` with its `root`, its `default` labels, and rules by key prefix or maps from the source's own classification text. Start with a broad `default` and narrow with rules. A native classification with no `map` row gives the item `marked`, which nothing allows.
- **Limit who can open a store.** Add `readers` to its `[[store]]` row, with a Windows group. The company's directory decides who is in the group. Add `changers` to limit who can run `label add`.
- **Hold a store.** `hold = true` refuses purge and label changes in the store while a legal hold or a retention rule applies.
- **Collect the journal.** Set `[audit] sink` to an absolute folder outside the data folder, that your tools collect from and the owner cannot rewrite. `fieldbook audit export` writes the journal's own lines and a manifest there. On the machine where you collect them, `fieldbook audit verify --export MANIFEST` checks a copy, and `fieldbook audit verify --anchor-seq N --anchor-head HEX` compares the live journal with a recorded head (IT-35). Set `max_unshipped_hours` to the age at which a model call with a non-public label must stop.
- **Turn the export requirement off.** It is on unless you say otherwise: a model call with a non-public label waits for a recent export (FB0925). To end that, write `require_fresh_export = false` in `[audit]` and raise `revision`. A model row then needs no sink and no limit, a stale or missing export never stops a call, and a `max_unshipped_hours` left in the file does nothing. The hash of the file in force is on every model call in the journal, and `fieldbook policy show` prints the requirement as on or off. So a reader of the journal finds the setting in your copy of that revision, which you should keep. Fieldbook records nothing else about it.

## When the owner leaves

The company's records system, not Fieldbook, is the record. Fieldbook holds a working copy, and each figure in it names its source. What you hold, and what to do at hand-over, is in the [hand-over runbook](../runbooks/hand-over.md). In short: you hold the policy file and the audit sink; the owner names a second person who can read the store (folder access, and the `readers` group for a role logon); the folders to dispose of are `overlay\`, `working\`, `originals\`, `state\` and the derived `state\site\`; and the disposal method is yours, because Fieldbook promises no secure overwrite (IT-36, IT-37).

## What the Closed state does

When a file is present but unreadable or invalid, when it is writable by someone other than an administrator, when its `revision` is lower than the recorded one, or when a loaded policy has gone missing, every destination is denied except the journal export. `check`, `build`, `search` and `status` still run. The program prints a warning line on standard error, naming the reason, for every verb except `policy`, `status`, `check`, `build` and `search`. A `policy.change` event with the outcome `absent` or `invalid` is appended once. Fix the file and install it; the next run loads it.

## What the file cannot do

- It cannot say that a label is correct. Fieldbook applies the table and the rules and does not read meaning. It never lowers a label and never edits or removes a marking.
- It cannot stop what an endpoint's operator does with text it receives. That is a contract with the operator.
- It cannot stop a person with write access to the folder from replacing it. The hash in the journal and a collected copy show that it happened.
- It cannot bind a tool client. The client's name is self-declared, so the `clients` list guards against accident and not against attack.
- It cannot change the fixed rows or the built-in labels.
- It cannot make a screenshot, a copy made by hand, a browser's side panel, a translation feature or a screen capture disappear. The program does not see them. Whether those features may run on a window that shows company text is a device decision (IT-39). The print and clipboard rows are advisory in the same way: they decide what the page offers, and they do not stop a person who copies by hand.
- It cannot give a person who is outside a store's `readers` group a view of the store. The group, which the company's directory keeps, decides, and Fieldbook checks it before it opens the store.

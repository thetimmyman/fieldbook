# Hand-over runbook

This runbook is for the day the owner of a company laptop changes role, leaves or loses access, and for a move to a new laptop. It makes sure the company can still read its own records, change its policy and show what the program did. The decision behind it is [ADR 0004, section 16](../decisions/0004-company-boundary.md#16-leaving-and-hand-over). The questions IT answers first are IT-36 (who holds the store), IT-37 (disposal) and IT-38 (credentials).

The company's records system is the record. A Fieldbook store is a working copy, and each figure in it names its source. Nothing here moves a record out of its source system.

| Part | Who | When |
|---|---|---|
| A. Before the owner goes | The owner and IT | At least a week before, or at once if the notice is short |
| B. The new owner or the new laptop | The new owner and IT | After A |
| C. Disposal | IT, with the owner present if possible | After B, and at the end of a retention period |

The folders, all under the data root (`%LOCALAPPDATA%\Fieldbook\`):

| Folder | Holds | Rebuilt from the source system? |
|---|---|---|
| `overlay\` | The company store: pages, source versions, records, the label ledger, `store.toml` | Sources: yes, by a new sync. Notes the owner wrote: no |
| `working\` | The owner's notes and drafts | No |
| `originals\` | The originals paired with the public edition | Yes, from the accepted edition archive |
| `state\` | Inbox, queue, journal, settings, backup settings, question log, call records, sync cursors | The journal and the question log: no |
| `state\site\overlay\` | The derived company site and search index | Yes, by `build --overlay` |
| `edition\` | The public edition | Yes, by installing it again |

The store is plain files: Markdown, CSV and TOML. A person with access to the folder can read it with a text editor or a spreadsheet, without the program.

## A. Before the owner goes

### Step 1. Name a second reader

IT names a second person who can read the store, before the owner goes. The program opens only the data root of the account that runs it (`%LOCALAPPDATA%\Fieldbook\` of that Windows user), and nothing moves that root (FB0419), so the second person's own logon does not open the owner's store whatever group they are in. They read it one of two ways. IT gives them read access to the folders (Step 4), and they read the plain files with a text editor or a spreadsheet. Or the store is issued to a role logon (Step 4 item 1), IT adds the people who sign in as that role to the Windows group that the `overlay` store row names in `readers`, and the program opens the store for them. Either is done before the owner goes, because a store that only one account can read stops being readable the day that account goes.

```powershell
fieldbook policy show
```

Expected: for a store issued to a role logon, the `overlay` row lists the `readers` group, and the people who sign in as that role are members of it; the second person has read access to the folders (IT checks the membership and the access with its own tools). If the row names no `readers`, any token that runs the program under that data root can open the store; IT decides whether that is what it wants.

### Step 2. Export the journal

```powershell
fieldbook audit export
```

Expected: the files land in the policy's audit folder, and the command prints the first and last sequence number and the head hash. IT takes N from manifest.last_seq and HEX from the full manifest.head, not the abbreviated terminal line, and compares the live prefix with `fieldbook audit verify --anchor-seq N --anchor-head HEX` on the live journal. If the export is refused, the message names the cause (no `[audit]` table in the policy, a folder that is not writable, or a file that would be overwritten with other bytes: FB0806).

### Step 3. Make and prove a backup

```powershell
fieldbook backup
fieldbook backup list
fieldbook restore --snapshot <N> --drill
```

Expected: the backup goes to the target the owner recorded as company storage (or one that a `backup` row of the policy lists), and the drill reports the same fingerprint as the backup. `backup list` shows the snapshot number N of the backup just made; add `--target <ID>` to the drill if `settings.toml` names more than one target. The commands follow Drill 5 of [restore-drill.md](restore-drill.md). A backup that holds `unlabelled` or `marked` items is refused (FB0901) with the count, unless no policy is loaded and the target is the one recorded as company storage, which takes `unlabelled` and never `marked`. Give `unlabelled` items a real label first (`label add`, at a console). A `marked` item stays `marked` until the policy maps its marking and `label reapply` runs.

### Final copy before access changes

For either copy-based hand-over, close working files and stop content writes before source access is removed. Repeat Step 2’s journal export and anchor verification, then create and verify a final backup as in Step 3. Record that explicit final snapshot number and target in the owner’s tracker; Step 3’s earlier backup is a rehearsal only. Keep the source without further content changes until destination checks pass. Confirm retirement before choosing --adopt --retired; use a distinct writer if the source remains active. The role-logon route needs no copy restore.

### Step 4. Hand over the access

IT does these, with its own tools. Fieldbook does none of them.

1. Reissue the folder's access to the role that the company reassigns, not to the leaver's name.
2. Move the policy file and the audit folder to the new owner's role if they are held by the leaver's account. They should not be: IT holds both from the first day.
3. Revoke or rotate each connector credential in the Windows credential store, and the service account behind it (IT-38). A model key is the owner's, in the environment; remove it from the leaver's profile.
4. If a legal hold applies, set `hold = true` on the store rows in the policy. That refuses purge and label changes and does not stop reading.

### Step 5. Tell the new owner what is open

Give the new owner this list: the labels in the policy with their titles (`fieldbook policy show` prints them), waiting proposals (`fieldbook queue list`, then `fieldbook queue show <id>`), configured streams and their current plan (`fieldbook sync --dry-run` writes nothing), recent completed stream runs (`fieldbook audit show --kind sync.end`; the default is the last 50 matching events, so increase `--last N` when needed), and any hold. Nothing in this list needs the leaver.

## B. The new owner or the new laptop

### Step 6. Same laptop, new owner

The new owner signs in with their own logon. Their data root is their own `%LOCALAPPDATA%\Fieldbook\` and holds none of the leaver's folders, and nothing moves it (FB0419), so `fieldbook status` does not open the leaver's store whatever groups the new owner is in. Choose one. (1) Restore the verified final snapshot from Final copy before access changes into the new owner's data root and adopt it, after following items 1 to 3 of Step 7 (`--adopt --retired`, because the leaver's store is retired). (2) If the store was issued to a role logon (Step 4 item 1) and the new owner signs in as that role, `fieldbook status` opens the store when that logon is in the `readers` group; only a member of the `changers` group, where the row names one, may run `label add`. A person with folder access and no program can also read the files in a text editor. Run `fieldbook check` and read the warnings.

### Step 7. New laptop

1. Install the program and the policy file by the company's own method. Install or verify the accepted public edition and its matching originals in this account’s data root, following [transfer.md](transfer.md). If the exact pair is unavailable locally, recover it from the verified independent release archive; retain all quarantine exclusions and never restore a quarantined payload. Until the policy is loaded the floor applies (IT-33).
2. Restore the explicit final snapshot number from Final copy before access changes into a new folder under the data root: `fieldbook restore --snapshot <N> --from "<the backup target folder>" --to "$env:LOCALAPPDATA\Fieldbook\restored"`. A new laptop has no `settings.toml`, so `--from` names the folder that holds the snapshot; `--target <ID>` works only for a target that `settings.toml` names. A folder under the data root is a store write and needs no `export` row. A folder anywhere else is an export, and is refused (FB0901 or FB0918, exit 2) unless an `export` row of the policy lists that folder and every label in the snapshot. A folder that is not empty, or is or holds a live root, is refused (FB0815). The command prints the rename lines for PowerShell. Run them: each moves a root that is already live aside (usually `state\`, once the program has run) and moves the restored root into place. Then delete the empty `restored` folder. The program stamps `store.toml` with `restored_from` and refuses to allocate an id (FB0836) until you adopt the store.
3. Adopt it. If the old laptop is retired, keep its writer number:

```powershell
fieldbook init --overlay --adopt --retired
```

If the old laptop stays in use, give the new one its own writer number instead (`--writer N`). Two laptops that issue ids with the same writer number would create two records with the same id, and an id is never reused or renamed.

Expected: `store.adopted` is appended to the journal, `fieldbook sync --dry-run` lists streams without error, and `fieldbook check` has no error.

## C. Disposal

### Step 8. Decide the method

The folders to dispose of are `overlay\`, `working\`, `originals\`, `state\` and the derived `state\site\overlay\` of every laptop and every backup target that held them. IT's method decides how (IT-37). Fieldbook deletes nothing of the company's and promises no secure overwrite. A device with disk encryption and a managed disposal method is the control.

Clear Fieldbook page storage through IT’s browser-clearing procedure too: localStorage holds preferences and pack metadata; sessionStorage may hold a typed question and thread notes. Check that those entries are gone in the affected browser profiles. This is not a secure-erasure claim.

Before disposal, `fieldbook clean --site` removes the derived site and index, so the second copy of company text goes first.

### Step 9. Check what is left

Search the laptop and the backup targets for the folders above, for a `restored` or `restore-check` folder under the data root, and for the exported journal copies. Journal copies in the audit folder are IT's evidence and follow IT's retention, not the store's.

## What can go wrong

| Symptom | Cause | What to do |
|---|---|---|
| `fieldbook status` cannot open the store (FB0807) | The logon is not in the `readers` group | IT adds the person to the group |
| Ids are refused after a restore (FB0836) | The restored store has not been adopted | Step 7, `init --overlay --adopt` |
| `store.missing` in the journal and a warning from `check` (FB0808) | A store folder recorded in the last run is not there | Restore it from the backup, or accept the loss and record it with IT |
| A backup is refused (FB0901) | It holds `unlabelled` or `marked` items, or a label the target's row does not list | Give `unlabelled` items a real label (`label add`). For `marked` items, map the marking in the policy and run `label reapply`. Or ask IT for a target whose row lists the label |
| A restore is refused (FB0815) | The folder is not empty, or is or holds a live root | Use a new folder under the data root |
| A restore is refused (FB0901 or FB0918, exit 2) | The folder is outside the data root, so it is an export, and no `export` row lists it with every label of the snapshot | Restore into a folder under the data root, or ask IT for an `export` row |

## Rehearsal before R2

Run Parts A and B with an empty destination profile and a synthetic company claim citing a public source. Install the accepted public pair, restore and adopt the company roots, then require the claim’s citation to resolve and its original to open with the recorded hash. Remove edition or originals separately: each must prevent rehearsal completion. Keep the laptop backup roots overlay, state and working unchanged.

The copy rehearsal creates identifiable synthetic work after the preliminary backup, then performs the final-copy procedure. Require that work and its evidence in the checked destination. Restoring only the preliminary snapshot must fail this completeness assertion. Keep the accepted public edition/originals and citation checks; a clean stale copy is not a completed hand-over.

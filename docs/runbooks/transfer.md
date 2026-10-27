# Transfer runbook

How an edition reaches the laptop, and how to put the previous one back. The owner does every step. The laptop is Windows 11 with allowlisting and no admin rights; at Tier 0 it runs no Fieldbook program. Edge opens `edition\index.html` from disk and Excel opens the workbooks. The first transfer is E1.

Four rules hold for every step:

1. **Approved channel only.** Files move by the channel IT approved for this purpose (IT-9), and by no other route. If IT has not answered, nothing moves.
2. **Hashes travel separately.** The zips come through the channel. `SHA256SUMS` comes by the out-of-band route IT-9 settled. A hash list that arrived inside a zip proves nothing.
3. **The Mark-of-the-Web stays.** Nothing here removes it.
4. **Nothing returns.** No file, screenshot, text or note is carried from the laptop to home or to GitHub, because the laptop holds the owner's working files and, once the program is installed, company data, and the home side and the repositories must never hold any. Company data that arrives in the company store from the company's own data sources (`sync`, from R2) is a different flow: it stays inside the company.

Folder layout after a transfer (D-22 in [ADR 0001](../decisions/0001-foundation.md)):

| Folder under `%LOCALAPPDATA%\Fieldbook\` | Holds | Replaced by a transfer |
|---|---|---|
| `edition\` | The site, `data\`, `templates\`, the Corpus text | Yes, in place |
| `rollback/<edition-id>/` | The previous verified edition and originals, with matching hashes and evidence | Kept after success until the next accepted update and verified recovery copy |
| `originals\` | Public originals, byte-exact | Switched and verified with its edition, even when contents are unchanged |
| `working\` | Your copies of the templates and your own files | Never |

The edition zip carries a `README.txt` with the short form of the laptop steps. If it differs from this runbook, this runbook is right; report the difference as a card.

## At home

### Step 1. Confirm the channel

Check that you hold IT's written answer to IT-9: the channel, its size limit, and the route for `SHA256SUMS`.

Expected: all three are written down. If any is missing, stop. Ask IT; do not send anything.

### Step 2. Check the files you will send

From the folder holding the files fetched in [release.md](release.md) (edition step 7):

```sh
sha256sum -c SHA256SUMS
```

Expected: `fieldbook-edition-NNN.zip: OK` and `fieldbook-originals-NNN.zip: OK`.

If a line fails, do not send. Fetch the files again. If it still fails, open a card with the run id.

### Step 3. Send

Send the two zips through the channel. Send `SHA256SUMS` by the out-of-band route, as a separate delivery.

If the edition zip is above the channel's size limit, stop and open a card. If the originals zip is above half the limit, stop and open a card for `later-split-originals`; do not split an archive by hand. The limit is question IT-25 in the [intake questions](../intake/it-questions.md); 500 MB per zip is assumed until IT answers, so half is 250 MB. `just edition` prints each zip's size in whole megabytes, one line for each zip (`ls -l` shows bytes), so compare those numbers with the limit. The card names both sizes and the limit.

If the originals zip has the same hash on its `SHA256SUMS` line as the one you last extracted, the originals have not changed and you may send the edition zip alone; before Step 4 on the laptop, copy the retained matching archive as described below.

## On the laptop

Commands are for PowerShell; they also run in Constrained Language Mode. Replace `NNN` with the edition number. Set the location to the folder where the channel put the files.

Before Step 4, if only the edition zip arrived, compare the retained originals archive hash with the new out-of-band list. Only on an exact match, copy it into the delivery folder as `fieldbook-originals-NNN.zip`, using the new edition number. Leave the retained rollback pair untouched. If no matching archive exists, transfer both archives. Equal extracted contents do not prove archive equality.

### Step 4. Check each zip against the out-of-band list

```powershell
foreach ($n in 'fieldbook-edition-NNN.zip','fieldbook-originals-NNN.zip') { $want = ((Get-Content .\SHA256SUMS | Select-String -SimpleMatch $n).Line -split ' ')[0]; "$n " + ((Get-FileHash -Algorithm SHA256 -LiteralPath ".\$n").Hash -eq $want) }
```

Expected: `fieldbook-edition-NNN.zip True` and `fieldbook-originals-NNN.zip True`. Case does not matter; every character does. An unchanged originals set still has to match the new edition manifest before the pair is switched.

If any line says `False`, do not extract. Delete the delivered file and ask for it again through the channel. If the second copy also fails, stop and tell IT; a file that changes in transit is IT's concern, and nothing from it is opened.

### Step 5. Close what holds the old files

Close any workbook opened from `edition\` (for example `data\registers.xlsx`) and any Edge tab you want to keep as a reference. Working copies in `working\` are not touched by this step.

Expected: no Fieldbook file is open in Excel.

### Step 6. Stage the new pair

Follow steps 1 to 3 of the [Edition update recipe](#edition-update-recipe). Extract both trees into the fresh stage and verify them before moving the installed edition. A missing originals zip is valid only when the manifest identifies the unchanged originals set and that exact set is copied into staging and verified.

### Step 7. Switch and verify

Follow steps 4 and 5 of the same recipe. Keep the previous edition and originals as one pair. If interrupted, restore that pair or finish from the verified stage; do not merge trees.

### Step 8. Create `working\` and exclude it from indexing (first transfer only)

```powershell
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\Fieldbook\working" | Out-Null
attrib +I "$env:LOCALAPPDATA\Fieldbook\working" /S /D
attrib "$env:LOCALAPPDATA\Fieldbook\working"
```

Expected: the last command prints one line containing `I` before the path. If a file you add later lacks the flag, run the `attrib +I` line again.

Working copies are yours alone. Material marked as controlled unclassified information or export-controlled goes into a working copy or any Fieldbook folder only if the company has authorised this laptop and that folder for it (IT-2, IT-34). Fieldbook never strips or edits a marking and makes no claim of compliance (AD-18). At Tier 0 no program reads the folder, so this rule is the owner's.

### Step 9. Create the one bookmark (first transfer only)

1. In File Explorer open `%LOCALAPPDATA%\Fieldbook\edition`, right-click `index.html`, choose Open with, then Microsoft Edge.
2. Press Ctrl+D, name it `Fieldbook`, and choose Done.

Expected: the address bar starts `file:///C:/Users/` and the Fieldbook home page loads. The path never changes between editions, so this bookmark serves every edition. Do not bookmark pages inside the edition.

### Step 10. Check the edition

Open the bookmark, then the About page.

Expected: the edition number on the About page is `NNN`. If it shows the previous number, step 7 did not complete; redo it. If the page looks unstyled or stale after a correct number, press Ctrl+F5.

Then check every file against the hash lists inside the zips:

```powershell
$dest = "$env:LOCALAPPDATA\Fieldbook"
foreach ($dir in 'edition','originals') { Get-Content "$dest\$dir\SHA256SUMS" | ForEach-Object { if ($_) { $h, $p = $_ -split '\s+\*?', 2; if ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path "$dest\$dir" $p)).Hash -ne $h) { "MISMATCH $dir $p" } } } }
```

Expected: no output. A `MISMATCH` line names a file that differs from its recorded hash. Do not use the edition: follow Rollback below, then open a card that names the file; the edition is rebuilt at home. Check originals even when their contents did not change.

Then run the laptop checks in [restore-drill.md](restore-drill.md#drill-3-the-owners-laptop-drill-e1).

### Step 11. Keep the rollback pair

Keep the previous verified edition and originals together, including their matching zips, published hash list and transfer evidence, after the new pair passes step 10. Follow step 6 of the [Edition update recipe](#edition-update-recipe). A successful update does not delete its rollback pair. On the first transfer there is no prior pair.

Exception: a held source version. When you quarantine a source version at home, delete every edition zip and originals zip in the delivery folder that contains it, and any retained pair if it holds that version. Replace the extracted `originals\` with the one from the replacement originals zip (step 7), so that no extracted original of the version stays. A quarantined version ships only its register row, its hash and the reason, so the replacement edition carries no text of it. Work from the replacement edition and do not roll back to a copy that holds the version. [release.md](release.md#put-a-source-version-on-hold-after-issue) says how to issue the replacement.

## Working copies

Templates ship in `edition\templates\` from E1. Never edit them there: the next transfer replaces `edition\`.

```powershell
$workingCopy = "$env:LOCALAPPDATA\Fieldbook\working\discovery-log.xlsx"
if (Test-Path -LiteralPath $workingCopy) { throw "Working copy already exists; choose an unused name" }
Copy-Item "$env:LOCALAPPDATA\Fieldbook\edition\templates\discovery-log.xlsx" $workingCopy
```

Expected: the copy appears in `working\`. Open and save only the copy. Each template's Read-me sheet says how to save a sheet as CSV UTF-8 for a later import.

## Back up `working\`

`working\` is the only folder Fieldbook cannot rebuild: the edition and the originals come from the two zips. Copy it to the place IT's policy covers (question IT-26) after each piece of work and before each new edition. When the program is installed (R2), the company's backup covers the whole data root, including the company store, and `backup` writes to the target that the policy names (IT-26, IT-33); at Tier 0 there is no program and this copy of `working\` is the owner's step. Close any workbook open from `working\` first.

```powershell
$src = "$env:LOCALAPPDATA\Fieldbook\working"
$dst = Join-Path '<the folder IT named, or a folder on this laptop you chose>' ("working-" + (Get-Date -Format 'yyyy-MM-dd'))
New-Item -ItemType Directory -Force $dst | Out-Null
Copy-Item -Path "$src\*" -Destination $dst -Recurse -Force -Exclude '~$*'
foreach ($f in Get-ChildItem $src -Recurse -File | Where-Object { $_.Name -notlike '~$*' }) { $rel = $f.FullName.Substring($src.Length).TrimStart('\'); if ((Get-FileHash -Algorithm SHA256 -LiteralPath $f.FullName).Hash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $dst $rel)).Hash) { "MISMATCH $rel" } }
```

Expected: no output. A `MISMATCH` line names a file that changed while it was copied, or was open; close it and run the commands again. Each day's copy goes into its own folder and nothing is deleted from the destination; remove old copies by hand and keep the last few.

After you approve a decision package, use Save As to keep a dated copy of the approved workbook alongside the working copy.

From R2 the program does this for you (`fieldbook backup`), and Drill 5 proves it.

## Mark-of-the-Web

Files that arrive through the channel, and files extracted from them, may carry a Zone.Identifier stream. This is normal and is IT's control. To see it, which changes nothing:

```powershell
Get-Content "$env:LOCALAPPDATA\Fieldbook\edition\index.html" -Stream Zone.Identifier
```

Expected: `[ZoneTransfer]` with a `ZoneId` line, or an error saying the stream does not exist. Both are fine.

Do not run `Unblock-File`, do not use the Unblock box in a file's Properties, and do not delete the stream. Excel may open a workbook in Protected View. That is Excel's own behaviour and the Read-me sheets explain it. If a prompt or policy stops you from using a workbook, note which step stopped you and tell IT; do not work around it.

## Rollback to the previous edition

Close open Fieldbook files and follow step 7 of the [Edition update recipe](#edition-update-recipe). Restore the matching edition and originals together, after an interrupted switch or a successful update that later proves faulty. Never keep the newer originals beside an older edition. Verify both internal manifests and the About number before reopening the bookmark. Working copies are untouched.

If the local previous pair is unavailable, retrieve the exact previous pair and its matching evidence from [the independent release archive](release.md#durable-release-archive). Verify the published hashes before staging it through the same recipe. Do not rebuild a released artifact as a substitute. If neither verified copy exists, stop the rollback until the archived pair is recovered.

Do not roll back to a pair that contains a quarantined source version (step 11).

## Nothing returns

Nothing on the laptop goes back. If reading on the laptop shows that public content is wrong, the owner writes the correction again at home from the public source, as a normal content change. Questions, notes, decisions, working files and every company record stay on the laptop, in `working\` and in the company store, and are never carried home or to GitHub. This rule is a control IT relies on (invariant 1 in ADR 0001).

## Edition update recipe

<!-- edition-update:start -->
1. Keep the current edition and originals together as the previous accepted pair. Stop readers before switching them.
2. For an edition-only update, compare the retained originals archive hash with the new out-of-band list. Only an exact match permits copying it under the new originals filename; preserve the rollback pair and its archive. Otherwise obtain both incoming archives before proceeding. Verify both ZIP hashes. Extract both incoming zips into a fresh staging folder. Verify their published hashes, the full internal manifests and matching edition identifiers before using either tree. Never extract over the current originals.
3. Open the staged edition's index and a stored original, and check the edition identifier. If verification fails, remove staging only; the current pair remains usable.
4. Use a versioned rollback folder containing `edition/` and `originals/`. Move both current trees into it, then move both staged trees to the fixed installed paths. Keep a small switch record naming the old/new edition identifiers and each completed move. If interrupted between moves, use that record and the verified manifests to finish the new pair or put both old trees back; never merge trees or overwrite an unidentified tree. Readers remain closed until one complete pair verifies.
5. Verify both installed manifests and the About number before reopening the edition. An original absent from the new manifest must be absent from the installed originals. A failed check restores the previous pair.
6. Retain that complete previous pair locally and in the independent archive after success. Remove an older rollback pair only after a later accepted update and its recovery copy are verified, keeping the current and previous supported pairs. A quarantined source version is the exception: remove every retained pair containing it and record that it cannot be used for rollback.
7. To roll back, verify the retained old pair, close readers, move the current pair aside together and restore both old trees to the installed paths. Use the same switch record for interruption recovery and verify both manifests again. If the local old pair is incomplete, stage the exact archived pair instead. Never roll back only one tree.
<!-- edition-update:end -->

The shipped integrity guide includes this marked block verbatim. `cargo xtask docs --check` compares the generated block; edits are made here only. The transfer harness interrupts every move, rolls back after a successful update, verifies both trees against the prior pair, and plants an original removed by the new edition.

Tests require the marked recipe to contain the edition-only preparation and cover matching-archive reuse and mismatched refusal without changing the rollback pair.

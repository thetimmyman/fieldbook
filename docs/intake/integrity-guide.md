# Fieldbook integrity guide

Edition {edition}. Tool version {tool_version}.

This guide shows how to check that the two zip files you received are the ones the sender built, how to extract them, and how to update or roll back. The edition is Tier 0: files only. It needs no program, no administrator rights and no command other than `Get-FileHash`.

## 1. What you should have

| Item | Name | Arrives by |
|---|---|---|
| Edition zip | `fieldbook-edition-{edition_number}.zip` | The transfer channel IT approved |
| Originals zip | `fieldbook-originals-{edition_number}.zip` | The same channel |
| Hash list | `SHA256SUMS`, two lines, one per zip | A different route from the zips, settled with IT |

The hash list is not inside the zips and the zips do not contain their own hashes. A zip cannot vouch for itself. If the zips were changed or swapped in transit, the hash list, which took another route, would no longer match.

## 2. Check each zip

Open PowerShell in the folder that holds the zips. Run these two commands, one for each zip.

```powershell
Get-FileHash -Algorithm SHA256 .\fieldbook-edition-{edition_number}.zip
Get-FileHash -Algorithm SHA256 .\fieldbook-originals-{edition_number}.zip
```

Compare the `Hash` value with the matching line in the hash list. Windows prints capital letters and the list uses small letters. Ignore the case. Every character must match.

| Result | What to do |
|---|---|
| Both match | Go to section 3 |
| Either differs | Do not extract. Delete the zips. Tell the contact in `TRANSFER-STATEMENT.txt`, or whoever sent them, and ask for the zips to be sent again |
| `Get-FileHash` is blocked | Do not look for another way. Ask IT which hash tool it approves and check with that |

## 3. Extract

Both zips go to the same place, `%LOCALAPPDATA%\Fieldbook\`. Each zip already holds one top folder (`edition` or `originals`), so extracting to the Fieldbook folder gives `edition\` and `originals\` without a doubled folder.

1. Open File Explorer. Type `%LOCALAPPDATA%` in the address bar and press Enter.
2. Create a folder named `Fieldbook`. Open it. Click the address bar and copy the full path.
3. Right-click the edition zip and choose **Extract All**.
4. Replace the destination in the box with the path you copied. The box fills in a folder named after the zip, so delete that last part. Choose **Extract**.
5. Do the same for the originals zip.
6. Open `%LOCALAPPDATA%\Fieldbook\edition\index.html` in Microsoft Edge. Make a bookmark for it.

Create `working\` beside them when you first need it. Copy templates there before you edit them.

If Extract All shows a long-path error, the destination is too deep. Use the path above and nothing longer.

## 4. Mark-of-the-Web

Files that come from outside carry a Windows marker, the Mark-of-the-Web. Leave it in place.

- Do not use **Unblock** in file Properties, `Unblock-File`, or any tool that strips the marker.
- Excel opens marked workbooks in Protected View. The read-only workbook `data\registers.xlsx` reads fully in Protected View. Fillable templates in `templates\` need editing: copy one to `working\` and open the copy. Whether your Excel lets you enable editing is IT's policy.
- Edge opens the pages from disk with the marker present.

## 5. Check one file later

Each zip has a `SHA256SUMS` file inside it, in the sha256sum text format: one line per file, with the hash, two spaces and the path relative to the folder that holds `SHA256SUMS`. It lists every file in that zip except itself. To check one file, run `Get-FileHash -Algorithm SHA256` on it and find its line. IT may check every file with its own tools.

## 6. Update and roll back

Follow the generated Edition update procedure below. Keep edition and originals as one verified pair. Updates and rollback never change working notes.

## 7. What the hashes show

The check shows that the zips you hold are byte-for-byte the files the sender hashed. It does not show who built them. Tier-0 zips are unsigned. A later release that adds a program will be signed, and this guide will say how IT checks the signature and the signed file catalog.

## 8. What a later release adds to this guide

For the signed program (Tier 1), this guide will add: install, uninstall and detection commands for the endpoint manager (detection by path and file version), return codes, the allowlist rule inputs (a publisher rule with a minimum version and a hash fallback), the signer's durable identity, and the inputs for Defender indicators.

## Edition update

<!-- generated:edition-update:start -->
1. Keep the current edition and originals together as the previous accepted pair. Stop readers before switching them.
2. For an edition-only update, compare the retained originals archive hash with the new out-of-band list. Only an exact match permits copying it under the new originals filename; preserve the rollback pair and its archive. Otherwise obtain both incoming archives before proceeding. Verify both ZIP hashes. Extract both incoming zips into a fresh staging folder. Verify their published hashes, the full internal manifests and matching edition identifiers before using either tree. Never extract over the current originals.
3. Open the staged edition's index and a stored original, and check the edition identifier. If verification fails, remove staging only; the current pair remains usable.
4. Use a versioned rollback folder containing `edition/` and `originals/`. Move both current trees into it, then move both staged trees to the fixed installed paths. Keep a small switch record naming the old/new edition identifiers and each completed move. If interrupted between moves, use that record and the verified manifests to finish the new pair or put both old trees back; never merge trees or overwrite an unidentified tree. Readers remain closed until one complete pair verifies.
5. Verify both installed manifests and the About number before reopening the edition. An original absent from the new manifest must be absent from the installed originals. A failed check restores the previous pair.
6. Retain that complete previous pair locally and in the independent archive after success. Remove an older rollback pair only after a later accepted update and its recovery copy are verified, keeping the current and previous supported pairs. A quarantined source version is the exception: remove every retained pair containing it and record that it cannot be used for rollback.
7. To roll back, verify the retained old pair, close readers, move the current pair aside together and restore both old trees to the installed paths. Use the same switch record for interruption recovery and verify both manifests again. If the local old pair is incomplete, stage the exact archived pair instead. Never roll back only one tree.
<!-- generated:edition-update:end -->

This block is generated from the edition update recipe in the transfer runbook.

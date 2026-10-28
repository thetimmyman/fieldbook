# Restore drill runbook

A restore drill takes a copy of Fieldbook, restores it the way it will be restored for real, and proves the result works. Five drills cover the places a copy is restored: the edition zips at home, on a Windows runner and on the laptop, and the program's own backups at home and on the laptop. Each is a numbered list; every step has a command, an expected result, and what to do when it fails.

| Drill | Where | Who | When | Arrives with |
|---|---|---|---|---|
| 1. Home drill | Linux, at home | Owner | Day 1 (home gate item 9 in [ADR 0001](../decisions/0001-foundation.md)), and before every edition until E1 | The cards `add-new-and-edition` and `edition-zips` for the zips; the card `extract-and-locators` for the locator links |
| 2. Windows drill | windows-2025 runner, headless Edge | CI; the owner dispatches it | From E1: every release tag, weekly, and before tagging an edition | The card `edge-harness` for the harness; the E1 card `transfer-and-windows-drill` for the job |
| 3. Laptop drill | The owner's laptop, Edge and Excel | Owner | At E1, then after every transfer | The E1 card `transfer-and-windows-drill`; the templates ship at E1 |
| 4. Backup drill | Linux, at home, on the real working copy | Owner | Day 1 (home gate item 9), and before every tag | Cards `backup-snapshot` and `restore-and-drill` |
| 5. Laptop program drill | The owner's laptop, with the signed exe | Owner | R2, then after every program release | Card `r2-state-and-journal` |

A failing drill blocks the tag or the transfer. Fix the cause in the Corpus, the renderer or the build, never in the zips, and rerun the whole drill from step 1.

## Drill 1. The home drill on Linux (Day 1)

Needs `git`, `just`, `unzip`, `sha256sum`, and `pdfinfo` from the pinned poppler (the one `cargo xtask extract` uses; see [dependencies.md](../reference/dependencies.md#developer-tools)). It also needs Microsoft Edge, because `just edition` runs the Edge harness. Run it from a fresh clone so it sees only what is committed.

### Step 1. Clone into a path with spaces and build the edition

```sh
D="$(mktemp -d -t 'field book drill.XXXXXX')"
git clone --quiet <repository path or URL> "$D/clone"
git -C "$D/clone" checkout --quiet <the commit to be tagged>
cd "$D/clone" && just doctor && just edition
```

Expected: exit code 0, and `out/dist/` holds `fieldbook-edition-NNN.zip`, `fieldbook-originals-NNN.zip`, `SHA256SUMS`, `corpus-support.toml` and `corpus-support.sha256`. The allowlist (AD-18) passes.

If `just edition` fails, its message names the cause (see [release.md](release.md), edition step 3). If `just doctor` fails, install what it names with `just setup`.

### Step 2. Check the zips against their hash list

```sh
(cd out/dist && sha256sum -c SHA256SUMS)
(cd out/dist && sha256sum -c corpus-support.sha256)
```

Expected: two `OK` lines, then `corpus-support.toml: OK`. If a line fails the build wrote the list wrongly; open a card.

### Step 3. Extract both zips into a path with spaces

```sh
ROOT="$D/Local App Data/Fieldbook"
mkdir -p "$ROOT"
unzip -q "out/dist/fieldbook-edition-"*.zip -d "$ROOT"
unzip -q "out/dist/fieldbook-originals-"*.zip -d "$ROOT"
ls "$ROOT"
```

Expected: `ls` prints `edition` and `originals`, and nothing doubled such as `edition/edition`. If `edition/edition` appears, the zip has a wrong top folder; open a card.

### Step 4. Check every file against the hash lists inside the zips

```sh
(cd "$ROOT/edition" && sha256sum --quiet -c SHA256SUMS) && (cd "$ROOT/originals" && sha256sum --quiet -c SHA256SUMS) && echo "inner hashes ok"
```

Expected: `inner hashes ok` and nothing before it. If a file is named, the build wrote it differently from its recorded hash; open a card.

### Step 5. Check that every link target exists

```sh
cd "$ROOT/edition"
missing=$(find . -name '*.html' -print0 | while IFS= read -r -d '' f; do
  grep -oE '(href|src)="[^"#?]+' "$f" | sed -E 's/^(href|src)="//' | grep -vE '^[a-z][a-z0-9+.-]*:' | while IFS= read -r t; do
    [ -e "$(dirname "$f")/$t" ] || printf 'MISSING %s -> %s\n' "$f" "$t"
  done
done)
if [ -z "$missing" ]; then echo "links ok"; else printf '%s\n' "$missing"; false; fi
```

Expected: `links ok`. Links to `originals/` resolve because both folders sit side by side. Links with a scheme (`data:`) and in-page anchors are skipped on purpose.

If lines say `MISSING`, each shows the page and the target. A missing `originals/` target is a Corpus or `add` problem; a missing site file is a renderer problem. Fix at the source.

### Step 6. Check that every page locator is inside its PDF

```sh
cd "$ROOT/edition"
bad=$(grep -rhoE 'originals/[^"#]+\.pdf#page=[0-9]+' --include='*.html' . | sort -u | while IFS='#' read -r p frag; do
  n=${frag#page=}
  pages=$(pdfinfo "../$p" 2>/dev/null | awk '/^Pages:/ {print $2}')
  if [ -z "$pages" ] || [ "$n" -lt 1 ] || [ "$n" -gt "$pages" ]; then printf 'OUT OF RANGE %s page %s (pages: %s)\n' "$p" "$n" "${pages:-unreadable}"; fi
done)
if [ -z "$bad" ]; then echo "locators ok"; else printf '%s\n' "$bad"; false; fi
```

`pdfinfo` reads the page count from the PDF's page tree (the `/Count` entry). Expected: `locators ok`.

If a line says `OUT OF RANGE`, the claim's locator is wrong or the wrong version of the original is linked. Fix the claim (a content change the owner reviews); never trim the check. `unreadable` means the PDF is damaged or not a PDF; re-add the original from its source.

### Step 7. Read it in a browser

```sh
xdg-open "$ROOT/edition/index.html"
```

Expected: the home page loads from disk with styling and the edition number in the header. Open one source page and follow its link to the original; the PDF opens at the cited page. Open `day-one.html`; it reads as the day-one binder. If any page is blank or unstyled, note the page and fix it as in step 5.

### Step 8. Clean up and record

```sh
cd "$HOME" && rm -rf "$D"
```

Record "home drill passed" for the commit as a comment on the drill's work item. That is home gate item 9.

## Drill 2. The windows-2025 drill in headless Edge (from E1)

The `windows` job runs this on a hosted windows-2025 runner. It stands in for Explorer's Extract All with `Expand-Archive`; the owner's laptop drill covers Explorer. The Edge harness arrives with the card `edge-harness` and the drill steps of the job with the E1 card `transfer-and-windows-drill`. The job's full run happens weekly and on release tags. To run it on the commit you are about to tag:

```sh
gh workflow run scheduled.yml --ref main
gh run list --workflow scheduled.yml --limit 1
gh run watch <run-id> --exit-status
```

Expected: success. The steps below are what the job does, in order, as PowerShell. They also run by hand on any Windows 11 machine with the repository checked out.

### Step 1. Build the zips

```powershell
cargo xtask edition
```

Expected: exit code 0, and `out\dist\` holds the two zips, `SHA256SUMS`, `corpus-support.toml` and `corpus-support.sha256`. The command owns the stamped build, the harness that writes `out\harness\network-silence-summary.txt` and the packaging, in that order, so a separate site build or harness invocation is not a step of its own. `just edition` runs the same command at home; the runner has no `just`. Failure handling is the same as in drill 1, step 1.

### Step 2. Check the zips with the owner's command

Run the loop from [transfer.md](transfer.md) step 4 inside `out\dist`, with the two real file names.

```powershell
$dist = 'out\dist'
foreach ($n in (Get-ChildItem "$dist\fieldbook-*.zip").Name) { $want = ((Get-Content "$dist\SHA256SUMS" | Select-String -SimpleMatch $n).Line -split ' ')[0]; "$n " + ((Get-FileHash -Algorithm SHA256 -LiteralPath "$dist\$n").Hash -eq $want) }
```

Expected: two lines, `fieldbook-edition-<N>.zip True` and `fieldbook-originals-<N>.zip True`. Then check the support sidecar:

```powershell
$want = ((Get-Content "$dist\corpus-support.sha256" | Select-Object -First 1) -split ' ')[0]
"support catalogue " + ((Get-FileHash -Algorithm SHA256 -LiteralPath "$dist\corpus-support.toml").Hash -eq $want)
```

Expected: `support catalogue True`. A `False` means the documented command and the real support sidecar disagree; fix whichever is wrong. These are packaging-consistency checks and claim no public provenance.

### Step 3. Extract into a path with spaces

```powershell
$dist = 'out\dist'
$runRoot = Join-Path $env:RUNNER_TEMP ('fieldbook-drill-' + [guid]::NewGuid().ToString('N'))
$dest = Join-Path (Join-Path $runRoot 'Local App Data') 'Fieldbook'
New-Item -ItemType Directory -Force $dest | Out-Null
Expand-Archive -LiteralPath (Get-ChildItem "$dist\fieldbook-edition-*.zip").FullName -DestinationPath $dest
Expand-Archive -LiteralPath (Get-ChildItem "$dist\fieldbook-originals-*.zip").FullName -DestinationPath $dest
(Test-Path "$dest\edition\index.html"), (Test-Path "$dest\edition\edition"), (Test-Path "$dest\originals\SHA256SUMS")
```

Expected: `True`, `False`, `True`. If the second is `True` the zip has a wrong top folder; open a card.

### Step 4. Check every file against the hash lists inside the zips

```powershell
foreach ($dir in 'edition','originals') { Get-Content "$dest\$dir\SHA256SUMS" | ForEach-Object { if ($_) { $h, $p = $_ -split '\s+\*?', 2; if ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path "$dest\$dir" $p)).Hash -ne $h) { "MISMATCH $dir $p" } } } }
```

Expected: no output. A `MISMATCH` line names a file whose bytes differ from its recorded hash, which is a build fault; open a card. The same loop works on the laptop (drill 3, step 1) to check a whole extraction.

### Step 5. Run the Edge harness on the extracted tree

```powershell
cargo xtask harness --edition "$dest\edition"
```

Expected: exit code 0. The harness runs headless Edge on every page and on every renderable original (shipped as `.txt`), with the net-log and without `--allow-file-access-from-files`. It asserts that no request leaves `file:`, that the selftest writes its results into the page, that the negative page's request to `https://example.invalid/x.js` is blocked and reported, and that the search and cite-check goldens pass. It also times 10 lookups (advisory) and, on Windows, runs the print check that each brief prints on one page (a brief on more than one page is FB0607, which is an error here and a warning on Linux).

If it fails, the log names the page and the assertion. A request outside `file:` or a missing block on the negative page is a security fault: stop and ask the owner. If Edge rejects the Content Security Policy on a page, the fallback rungs are in AD-12; that is a decision for the owner.

### Step 6. Check the working folder commands

```powershell
New-Item -ItemType Directory -Force "$dest\working" | Out-Null
attrib +I "$dest\working" /S /D
if ((attrib "$dest\working") -notmatch '\sI\s') { throw 'working is not excluded from indexing' }
```

Expected: no output and no error. This runs the commands the transfer runbook gives. If it throws, the documented command no longer works; fix [transfer.md](transfer.md).

```powershell
Set-Content -LiteralPath "$dest\working\synthetic.csv" -Value 'id,value','1,a'
```

Then run the copy and compare block of [transfer.md](transfer.md#back-up-working), with `$src = "$dest\working"` and `$dst` a new folder under `$env:TEMP`.

Expected: no output.

```powershell
Add-Content -LiteralPath (Join-Path $dst 'synthetic.csv') -Value 'x'
```

Then run only the last line of that block, the compare loop, again.

Expected: exactly one line, `MISMATCH synthetic.csv`. The job fails if the first comparison prints anything or the second prints nothing. If it does, the documented command no longer works; fix [transfer.md](transfer.md).

## Drill 3. The owner's laptop drill (E1)

Run it after every transfer; the first run at E1 adds steps 8 and 9. Only the owner runs it. Write down only pass or fail by step number.

### Step 1. Restore the edition

Follow [transfer.md](transfer.md) steps 4 to 10. Step 10 includes the check of every file against the hash lists inside the zips (the loop from drill 2, step 4).

Expected: both hashes `True`, the folder layout checks `True`, `False`, `True`, and the About page shows the edition number. If any fails, stop; the transfer runbook says what to do.

### Step 2. Open the home page and search

Open the `Fieldbook` bookmark. Press `/`, type `S-0001`, and press Enter.

Expected: the first result is the record for `S-0001`; Esc closes the box. If `/` does nothing, scripts are blocked: open `ids.html` from the home page instead and find the id there. Note it as a finding; every page must still work without scripts.

### Step 3. Follow an original

Open `sources.html`, choose a source whose original is a PDF, open its source page, and follow the link at its page locator.

Expected: the PDF opens at the cited page, either in Edge or in the external viewer if policy sets one. If it opens at page 1, note it; the locator text beside the link still gives the page.

### Step 4. Open the read-only workbook

```powershell
Invoke-Item "$env:LOCALAPPDATA\Fieldbook\edition\data\registers.xlsx"
```

Expected: Excel opens it, possibly in Protected View. One sheet per register, a frozen header row, an autofilter, and no repair message. If Excel says it found unreadable content, note it; do not click Repair.

### Step 5. Open a template copy (E1 onward)

```powershell
$drillCopy = "$env:LOCALAPPDATA\Fieldbook\working\drill-discovery-log.xlsx"
if (Test-Path -LiteralPath $drillCopy) { throw "Drill copy already exists; choose an unused name" }
Copy-Item "$env:LOCALAPPDATA\Fieldbook\edition\templates\discovery-log.xlsx" $drillCopy
Invoke-Item $drillCopy
```

Expected: the copy opens with its Read-me sheet and no repair message. If policy blocks editing, note which prompt stopped you. Close Excel, then remove only the disposable drill copy:

```powershell
Remove-Item -LiteralPath $drillCopy
```

The ordinary working discovery-log.xlsx is never changed. The transfer test runs this twice with a synthetic filled working workbook and requires its hash unchanged.

### Step 6. Print the day-one binder

Open `day-one.html` and press Ctrl+P with Edge's default settings.

Expected: Letter pages; each brief on one page; the footer shows the product, the edition, the page id and "Check the current edition"; no `file:` address is printed. Citations print as footnotes and status words print.

### Step 7. Check that nothing was written into `edition\`

```powershell
Get-ChildItem "$env:LOCALAPPDATA\Fieldbook\edition" -Recurse -Force -File | Where-Object { $_.Name -like '~$*' }
```

Expected: no output. Excel lock files (names starting `~$`) mean a workbook was opened from `edition\` and still is; close it. Work only on copies in `working\`.

### Step 8. First transfer only: the spikes

Run S5, S6 and S2 as listed in the E1 gate of [ADR 0001](../decisions/0001-foundation.md). Each has a fallback in that record if it fails.

Expected: a pass or fail per spike. A fail is not a failed drill; it chooses the fallback.

### Step 9. Back up `working\` and compare

Close any workbook open from `working\`. Run the copy and compare commands from [transfer.md](transfer.md#back-up-working), with the folder IT named in answer to IT-26 as the destination (or the folder you chose, if IT has not answered).

Expected: no output from the comparison. A `MISMATCH` line names a file that changed during the copy or was open; close it and run the commands again. Delete the test copy if it holds anything you do not want to keep.

### Step 10. Record at home

At home, type "laptop drill passed" or the failing step number as a comment on the laptop-drill work item. Do not copy a file, screenshot or text from the laptop: it holds company data, and the work item and the repositories must never hold any.

## Drill 4. The backup drill on Linux (Day 1)

Run it in your own checkout on a clean tree, not in a fresh clone: the journal and the state folder are not committed, and the drill must see them. The commands call `fieldbook`; use `cargo run -q --locked -p fieldbook --` in its place if the program is not on your path. Needs `sha256sum` and `diff`.

### Step 1. Check the live journal and make a working area

```sh
fieldbook audit verify
S="$(cut -d' ' -f1 out/dist/corpus-support.sha256)"
fieldbook backup verify --raw . --support-catalogue out/dist/corpus-support.toml --expect-support-sha256 "$S"
D="$(mktemp -d -t 'field book drill.XXXXXX')"; mkdir "$D/target" "$D/fault"
```

Expected: `journal ok`, the event count and the first 12 characters of the head; exit code 0. Then `raw copy consistent` and `public support catalogue verified: ` with 12 hex characters, from the catalogue and separately delivered digest that Drill 1 or release step 3 left in `out/dist`; if those files are absent, run `just edition` first, because the digest is an independent input and is never read from the copy it describes. A `FB0800` or `FB0801` line means the chain is broken at the line it names. Run `fieldbook audit verify --repair`: it moves the whole journal, unchanged, into a folder `journal.broken-<seq>` beside it, starts a new chain, prints an `FB0804` warning that names the folder, and exits 1. Then run `fieldbook audit verify` again, which must print `journal ok`, and open a card that names the first bad line, because something changed the file. Keep the folder; nothing is deleted. A schema-valid broken chain can be backed up when all snapshot labels and target permissions allow it; before repair that snapshot records `journal_state = "broken"` and exits 1. Schema-invalid journal or retained repair bytes contribute unlabelled. Under the home floor, backup refuses those bytes with FB0901/exit 2 and creates no snapshot, including after repair. `FB0803` (an unfinished write) or `FB0804` (a torn last line) are warnings: run the verb that was interrupted again, then repeat this step.

### Step 2. Make a snapshot outside the working copy

```sh
fieldbook backup --to "$D/target"
ls "$D/target"
```

Expected: `backup 1 written` with the file count and bytes; `ls` shows `fieldbook-backup-0001` and `LATEST`, and no folder ending `.partial`. If it refuses with `FB0812`, the target is inside a folder being backed up; choose another. Under the home floor, schema-invalid journal or repair-archive bytes cause FB0901/exit 2 with no new snapshot, even after journal repair. Preserve all files and leave this drill incomplete for owner recovery review; never delete or exempt the archive to obtain success. The existing malformed-archive negative case and schema-valid chain-only positive case cover these branches. This home rule does not change the floor B backup exception.

### Step 3. Verify the snapshot two ways

```sh
fieldbook backup verify --in "$D/target"
(cd "$D/target/fieldbook-backup-0001" && sha256sum --quiet -c SHA256SUMS) && echo "snapshot hashes ok"
```

Expected: `snapshot 1 verified`, then `snapshot hashes ok`. A named file is an `FB0810` fault in the snapshot writer; open a card.

### Step 4. Restore into the drill folder and check it

```sh
fieldbook restore --snapshot 1 --from "$D/target" --drill
```

Expected: the last line is `drill passed: check clean, journal ok, fingerprint <12 hex> matches`; exit code 0. The drill restores into `drill/` under the state folder (`.fieldbook/drill/`), replaces what an earlier run left there and removes the folder when it ends. A failure names the step and the code (`FB0810`, `FB0811`, `FB0802`, `FB0816`). Fix the cause, never the snapshot, and rerun from step 1.

### Step 5. Plant a fault; the check must catch it

```sh
cp -R "$D/target/fieldbook-backup-0001" "$D/target/LATEST" "$D/fault/"
f="$(find "$D/fault/fieldbook-backup-0001/data" -type f -name '*.csv' | sort | head -n 1)"
printf 'x' >> "$f"
fieldbook backup verify --in "$D/fault"; echo "exit $?"
```

Expected: one `FB0810` line naming that file, and `exit 1`. If `exit 0` prints, verification is not reading the bytes: stop and open a card.

### Step 6. Restore for real into an empty folder

```sh
fieldbook restore --snapshot 1 --from "$D/target" --to "$D/restored"
diff -rq corpus "$D/restored/corpus" && echo "corpus identical"
fieldbook restore --snapshot 1 --from "$D/target" --to "$D/restored"; echo "exit $?"
```

Expected: the first command names the folder it filled and prints the rename steps; it changes nothing in the working copy. `corpus identical` follows. The second restore exits 1 with `FB0815` (the destination is not empty). The diff differs only if you changed the Corpus after step 2; commit or discard and rerun.

### Step 7. Clean up and record

```sh
cd "$HOME" && rm -rf "$D"
```

Record "backup drill passed" for the commit as a comment on the drill's work item. That is the second half of home gate item 9.

## Drill 5. The laptop program drill (R2)

Run it with the signed exe, after a transfer and after every program release. Only the owner runs it. The commands run in PowerShell; they use only cmdlets and string methods that Constrained Language Mode allows.

### Step 1. Check the journal

```powershell
fieldbook audit verify
```

Expected: `journal ok`, the event count and the head. Note the head and event count in your own notes; they are the outside copy of the chain.

### Step 2. Back up to the target

```powershell
fieldbook backup verify --raw "$env:LOCALAPPDATA\Fieldbook" --support-catalogue "$env:LOCALAPPDATA\Fieldbook\edition\evidence\corpus-support.toml" --expect-support-sha256 '<the digest retained from corpus-support.sha256>'
fieldbook backup --target it
fieldbook backup verify --target it
```

Expected: `raw copy consistent`, then `public support catalogue verified: ` with 12 hex characters, then `backup N written`, then `snapshot N verified`. The optional verification is the owner-import step: it supplies the separately delivered digest explicitly, once, and never reads it from the extracted edition or from the catalogue's own folder. A failure names FB0811 or FB0810 and grants nothing; stop and keep the pair. The target `it` is the one `settings.toml` names for IT's answer to IT-26 (or the same-disk folder, if IT has not answered). A synced path prints `FB0813`; declare the target synced in `settings.toml` only if IT agrees.

### Step 3. Run the restore drill

```powershell
fieldbook restore --snapshot <N> --target it --drill
```

Expected: `drill passed: check clean, journal ok, fingerprint <12 hex> matches`.

### Step 4. Restore into an empty folder and compare

```powershell
$live = "$env:LOCALAPPDATA\Fieldbook"
$new = "$env:LOCALAPPDATA\Fieldbook\restore-check"
fieldbook restore --snapshot <N> --target it --to $new
foreach ($dir in 'working','overlay') { foreach ($f in Get-ChildItem "$live\$dir" -Recurse -File | Where-Object { $_.Name -notlike '~$*' -and $_.FullName -ne "$live\overlay\store.toml" }) { $rel = $f.FullName.Substring($live.Length).TrimStart('\'); if ((Get-FileHash -Algorithm SHA256 -LiteralPath $f.FullName).Hash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $new $rel)).Hash) { "MISMATCH $rel" } } }
if (Test-Path -LiteralPath "$live\overlay\store.toml") { Select-String -LiteralPath "$new\overlay\store.toml" -Pattern '^restored_from' }
```

Expected: no output from the loop and, if the laptop has a company store, one line from the last command that names `$new\overlay\store.toml` and holds `restored_from`. The loop leaves `overlay\store.toml` out on purpose: `restore --to` stamps the copy with `restored_from` (FB0836), so its bytes always differ from the live file, and the last command shows the stamp is there. A `MISMATCH` names a file changed since the snapshot; make a new snapshot and repeat. If `restore` prints FB0815, delete `$new` and run it again. Delete `$new` when done: `Remove-Item -Recurse -LiteralPath $new`. A restored company store carries `restored_from` in its `store.toml` and issues no id until it is adopted (FB0836). This drill deletes the copy and never adopts it. To bring a store up on a replacement laptop, follow Step 7 of the [hand-over runbook](hand-over.md).

### Step 5. Start and stop the chat listener (AI build with `bridge` only)

Run this step only if the AI build is installed, it is built with `bridge`, and IT has answered IT-32. With the silent program, skip it and write "chat skipped" in your notes.

In a second PowerShell window, run:

```powershell
fieldbook-ai.exe chat --site "$env:LOCALAPPDATA\Fieldbook\edition"
```

Expected: the window prints the address of the chat page, on 127.0.0.1, then `chat: listening on 127.0.0.1 only; type q and press Enter to stop`, and keeps running. `--site` names the extracted Site whose required native assets are verified and retained. `GET /` serves the compiled, content-free chat shell; Corpus and company content arrives only through authenticated API routes. Write down whether Windows or endpoint protection showed any prompt about the listener, and what it said. If a prompt offers access wider than this computer, decline it and note that. Open the address in Edge and check that the Assistant page loads. Then type `q` and press Enter in that window. The window prints `chat: stopped after N calls` and returns to the prompt. Close the window only after that line appears: a window closed earlier leaves a start event with no stop event.

Back in the first window:

```powershell
fieldbook audit verify
```

Expected: `journal ok`. Then run `fieldbook audit show --kind chat.session --last 2`: it prints two events, the start and the stop, and the `calls` of the stop equals the N that the chat window printed. If a second `fieldbook chat` is refused with `FB0954`, the first one is still running; stop it and repeat this step.

### Step 6. Check that nothing was left running

```powershell
Get-Process -Name fieldbook,fieldbook-ai -ErrorAction SilentlyContinue
```

Expected: no output (NFR-F-08). The program starts no task or service; a line here is a fault.

### Step 7. If IT copies the data folder itself

Ask IT to restore its copy of `%LOCALAPPDATA%\Fieldbook\` into a scratch folder, then:

```powershell
fieldbook backup verify --raw "<scratch folder>"
```

Expected: `raw copy consistent`. It checks the journal chain, the originals against their hashes and the extractions. A fault names the file; it is a finding for IT's backup, not for Fieldbook. This is a generic integrity check: it uses no support catalogue or expected digest, claims no public provenance and grants no recovery or delivery fact. Skip this step if IT does not copy the folder.

### Step 8. Record at home

At home, type "program drill passed" or the failing step number as a comment on the drill's work item, and say whether Windows asked about the chat listener, or that the step was skipped. Do not copy a file, screenshot or text from the laptop: it holds company data, and the work item and the repositories must never hold any.

## Replacement-machine recovery acceptance

Before company deployment, the owner and IT accept these targets or record replacements in the owner's tracker: at most one working day of unissued work lost, no issued snapshot lost after the delivery gate, and a usable replacement machine within four hours. Configure a daily backup and a verified independent copy before the end of each working day; verify a copy of every issued snapshot before delivery. A reminder or same-disk snapshot alone cannot meet these targets.

Start with an empty replacement data root and the archived accepted executable. Time retrieval of the independent copy, signature/hash checks, restore, audit verify, strict check and reopening an issued snapshot with all referenced runs and originals. Compare the snapshot manifest exactly and record elapsed time and last recoverable generation. A missed loss or time target blocks company deployment until the cadence, location or recovery procedure is corrected and drilled again. The marked/unresolved backup refusals remain in force; resolve them by the approved policy and label process before promising coverage.

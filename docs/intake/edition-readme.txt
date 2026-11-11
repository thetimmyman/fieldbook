FIELDBOOK EDITION {edition}
Edition {edition}. Tool version {tool_version}. Source commit {commit}.
Source date (UTC): {source_date}.

WHAT YOU RECEIVED

Through the approved transfer channel:
  fieldbook-edition-{edition_number}.zip    {n_files_edition} files
  fieldbook-originals-{edition_number}.zip  {n_files_originals} files
By the separate hash route that your IT department settled:
  SHA256SUMS             the SHA-256 of each zip, one line per zip
  corpus-support.sha256  the SHA-256 of the support-file catalogue,
                         edition\evidence\corpus-support.toml, one line

CHECK, THEN EXTRACT

1. Open PowerShell in the folder that holds the files.
2. Run Get-FileHash -Algorithm SHA256 on each zip. Compare each value
   with the matching line in SHA256SUMS. Ignore letter case. Every
   character must match. If one differs, do not extract. Delete the
   files and ask the sender.
3. Extract both zips into a new staging folder, not over the installed
   edition. Each zip holds one top folder, so you get edition\ and
   originals\ side by side.
4. Check every file against the SHA256SUMS inside each zip, and run
   Get-FileHash -Algorithm SHA256 on
   edition\evidence\corpus-support.toml. Compare that value with
   corpus-support.sha256. If anything differs, do not use the edition.
   Ask the sender.
5. Move the staged edition\ and originals\ to
   %LOCALAPPDATA%\Fieldbook\ together, keeping the previous pair as
   the rollback. Never mix the two editions.
6. Open edition\index.html in Microsoft Edge from disk, then the About
   page. The edition number must be {edition_number}.

The transfer runbook has the commands for each step. If this file and
the runbook differ, the runbook is right.

MARK-OF-THE-WEB
Files that arrive through the approved channel, and files taken
out of the zips, may carry a Windows marker, the Mark-of-the-Web.
It is normal and it is IT's control. Leave it in place: do not
run Unblock-File, do not use the Unblock box in a file's
Properties, and do not delete the stream. Excel may open a
workbook in Protected View; that is Excel's own behaviour. The
transfer runbook explains it under "Mark-of-the-Web".

BACK UP YOUR WORK
Your working copies live in Fieldbook\working\. It is the only folder
that cannot be rebuilt: Fieldbook\edition\ and Fieldbook\originals\
come from the two zips. Copy working\ to the place your IT policy
covers after each piece of work and before each new edition. The
commands, with a hash comparison that uses Get-FileHash, are in the
transfer runbook under "Back up working". This edition makes no backup
of its own. After you approve a decision package, use Save As to keep
a dated copy of the approved workbook alongside the working copy.

SUPPORT EVIDENCE
The edition holds evidence\corpus-support.toml, a catalogue of the
public support files used to build it. Its digest travels with
SHA256SUMS by the separate route. Keep both with the pair. The
catalogue is evidence of bytes, not proof that your copy is public
or admitted.

Read TRANSFER-STATEMENT.txt first. Its contact is in section 7.

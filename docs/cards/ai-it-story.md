# ai-it-story

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

The documents that go to IT with the first edition say what the program does and does not do, sentence by sentence true of the first edition: the silent build is described as it is, the AI build is described as a separate program that is not in this edition, the company deployment is described as what either build is part of (company-managed devices, the company network, company storage and a policy file that IT owns), and every sentence about the journal, backup, a model, the tool server or the chat listener names the build and the tool version it holds for. The count of IT questions is 39 in all six places that state it (the table in Scope). Nothing but text changes.

## Why

- Review is `owner` because the card edits the IT-facing statements in `docs/intake/` and `SECURITY.md`, which the release gate covers ([Gates](../decisions/0001-foundation.md#gates)). The owner reads each changed sentence before the first edition goes to IT.
- What the program may and may not do is [AD-18](../decisions/0001-foundation.md#ad-18-security-and-confidentiality-guardrails) and the controls NS-01 to NS-07, AL-01 to AL-05 and HG-09 to HG-15 in [controls.md](../controls.md). The two builds, and the posture sentences that say what each one does, are the "Two builds" paragraph of [The architecture in one page](../decisions/0001-foundation.md#the-architecture-in-one-page); a document quotes one of the forms there or names the build.
- The deployment is an extension of the company's infrastructure ([ADR 0004](../decisions/0004-company-boundary.md)). The device, the network, the storage and the people with access are the company's, and so are the policy file, the approved endpoints, the label table and the choice of what Fieldbook may connect to. Fieldbook carries labels, applies the policy at one gate, writes an audit journal, backs up under the policy and syncs from the data sources the company names. The documents say who decides each of these and make no claim of compliance with any scheme.
- The text was written before the program existed. `audit-journal` now makes the journal real, so the statements about it can name the commands and the generated field table ([audit.md](../reference/audit.md)) instead of describing an intention. The first edition holds no AI build, so a sentence about it must not read as a claim about the edition.
- The documents are the intake pack ([README](../intake/README.md)); the lints `posture_wording_names_the_build` and `it_count_matches_rows` of `docs-checks` are the drift check.

## Scope

- Files: `docs/intake/{README.md,ai-development-statement.md,data-handling.md,it-questions.md,network-silence.md,transfer-statement.txt}`, `SECURITY.md`.
- Docs to update: the files above, and `CHANGELOG.md` (`- Changed: the IT documents describe the silent build and the AI build separately.`).
- The count of IT questions is stated in six places. A change in the number of questions edits all six:

  | File | Section | Form | Read by |
  | --- | --- | --- | --- |
  | `docs/intake/it-questions.md` | the opening paragraph | `all 39 questions` | `it_count_matches_rows` |
  | `docs/intake/README.md` | Files in this folder | `IT-1 to IT-39` | `it_count_matches_rows` |
  | `docs/decisions/0001-foundation.md` | What exists when | `All 39 IT questions` | `it_count_matches_rows` |
  | `docs/decisions/0001-foundation.md` | Owner decisions | `The 39 questions` | `it_count_matches_rows` |
  | `docs/decisions/0001-foundation.md` | Requirements referenced | `IT-1 to IT-39` | `it_count_matches_rows` |
  | `docs/decisions/0004-company-boundary.md` | Consequences | `There are 39 IT questions instead of 32` | the owner's reading; no lint reads this form |

  This card edits only the first two, which are in Files; it checks the other four and does not change them.

## Out of scope

- Any change to the program, the Corpus, `docs/controls.md` or `docs/design/`. A statement the program cannot support is a finding, not an edit of the program.
- `docs/intake/edition-readme.txt` (`edition-zips`) and the runbooks under `docs/runbooks/` (the cards that own the drills).
- `docs/intake/policy.md`, `docs/intake/policy.example.toml` and the checks of `intake_policy.rs`. They belong to `it-company-story`, which follows this card (it names this card under Cards merged). The grep of step 1 also finds sentences in `policy.md`; a failing one is left for that card. This card words each sentence of steps 2 to 5 for the build and the tool version it holds for. `it-company-story` then re-reads each sentence that names the policy file, a label, a store, a destination, the audit export or the company store against the commands built by then, and fixes the policy words; it does not reword the build or the tool version.
- Writing the Windows capture and import-list results of `network-silence.md`; they join the report at the signed-program release.
- Asking IT anything. The questions are text; the owner sends them.

## Preconditions

- Cards merged: `docs-checks`, `audit-journal`.

## Steps

1. List every sentence that names the AI build, the journal, `backup`, `restore`, `fieldbook chat` or `fieldbook mcp`: `grep -nE 'AI build|journal|policy|label|fieldbook (backup|restore|chat|mcp|sync|policy)' docs/intake/*.md docs/intake/transfer-statement.txt SECURITY.md`. For each, check three things and fix the sentence when one fails. It names the build or the edition it holds for. It names the tool version when it is about the AI build (`v0.2.0`, not part of the first edition). It names a command or control that exists, or says which Day it arrives.
2. `ai-development-statement.md`, sections 1, 3 and 4: section 1 says no version contains a model and the first edition has no network client; section 3 (Handoff) says the person copies and pastes and the program never touches the clipboard; section 4 describes the AI build in the present tense of a later program, with the gate, the audit line before the send, the 30-day call records and the read-only tool server, each with its limit. Section 1 keeps what is true of each build (no silent build contains a model, and the first edition has no network client). Section 5, now "The company application", is added: the stores and labels, the policy file and its three states, the destinations, the connectors (`Pull` from a folder in every build, over a network only in the AI build and only on a channel the company has approved) and what the company's IT and security owners decide and operate (stores, approved endpoints, labels, access), each with the control or the card that builds it. The present "Questions IT may want to settle" becomes section 6.
3. `data-handling.md`, sections 1, 3, 5 and 7: section 1 is Labels (the four built-ins; the rest of the table is the company's), section 3 lists the destinations, says that the policy file is IT's and that a destination it does not name receives nothing, section 5 says that a backup is checked against the policy's `backup` rows; in section 7 name `audit verify`, `audit show` and `audit export`, link the generated [audit.md](../reference/audit.md) for the fields, say plainly that a page opened in Edge, a hand edit and a developer-tool build step are not in the journal, and keep the sentence that the question log can be cleared, reduced to hashes or turned off without touching the journal.
4. `network-silence.md`, sections 3 and 5: the first edition and the silent build make no connection and open no socket. The AI build section lists what a capture and an import list will show and what they cannot show, and the channels the company approves for it: the one HTTPS endpoint its policy names, the loopback Bridge page, and a sync from a company source over a network. The list itself is not written here.
5. `it-questions.md`: 39 rows, IT-1 to IT-39. Part C is IT-27 through IT-39. The default cells say: no endpoint approved and Handoff only, two programs, and the question asks which endpoints the company approves for which labels (IT-27); the key from an environment variable (IT-28); no client approved (IT-29); direct HTTPS on 443 with no proxy (IT-30); call records off on the laptop (IT-31); no listener approved, the laptop's AI build is built without `bridge`, chat stays on Handoff (IT-32); IT owns the policy file and, without it, the built-in floor applies (IT-33); only `public`, `unlabelled`, `marked` and `owner-supplied` exist until IT names more labels (IT-34, which an opaque code names); no journal export, the journal stays local, and a model call with a non-public label is refused (IT-35); IT holds the policy and the sink when the owner leaves, and the owner names a second person who can read the store (IT-36); disposal and retention are the company's method (IT-37); no connector until IT names a service account (IT-38); assistant side panels, page translation and screen capture are treated as on (IT-39). IT-26, IT-28 through IT-31 are asked before the signed program; IT-27 and IT-32 through IT-39 go with the Day 60 to 75 request.
6. `README.md` of the pack and `transfer-statement.txt`: the rows and section 6 agree with steps 2 to 5 and with `SECURITY.md`'s scope and "never" bullets. Quote a canonical form or name the build; do not write the bare four-part sentence.
7. Run the lints. A finding is fixed in the document, never by changing a lint.

## Acceptance

```sh
cargo nextest run --locked -p xtask docs_lints:: links::
cargo xtask cards --check
grep -c '^| IT-' docs/intake/it-questions.md
grep -rniE 'cards? [0-9]+|owner gates? [0-9]|\bgate [0-9]|five (owner )?gates' --include='*.md' --include='*.txt' SECURITY.md README.md docs | grep -v '^docs/design/'; echo "exit $?"
just check
```

Expected: the tests pass; `cards --check` prints `cards: ok`; the count prints `39`; the last grep prints nothing and `exit 1`; `just check` exits 0.

## Negative control

On the branch, write a sentence of the old four-part form (network, socket, listener and model, each led by "no") into `SECURITY.md` with no build named: `posture_wording_names_the_build` must fail and name the line. Restore the file. Then lower by one the last number of the IT range in the file list of `docs/intake/README.md`: `it_count_matches_rows` must fail and give the row count. Restore the file.

## Rollback

Revert the merge commit. The intake documents return to their earlier text, which is correct for the silent build and less exact about the AI build. A copy already sent to IT keeps the text it was sent with; send the corrected files with the next transfer.

## BLOCKED

A statement and the program disagree and the document cannot be made true by rewording (for example the document claims a control the program does not have). Set the item to Blocked with `BLOCKED: <the sentence> is not true of the program`, and stop. Do not weaken the statement to fit and do not change the program in this card.

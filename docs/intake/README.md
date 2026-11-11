# Intake pack

This folder is what corporate IT security reads. It holds the pack's index, the sources of the files that ship in every edition, the questions the owner sends IT, and a guide to the policy file that IT writes if it approves the program (`policy.md`). Fieldbook arrives in tiers, smallest first: Tier 0 is files only. A program, if IT approves one, is a separate, later request.

The words edition, transfer statement, intake pack, day-one binder and context pack have fixed meanings. They are defined in the [README](../../README.md). Decision ids (AD-nn, D-n) are defined in [ADR 0001](../decisions/0001-foundation.md). Every control named here (ZN-01, NS-03 and so on) has a row in the [control register](../controls.md).

## For a ticket

The paragraph below can be pasted into an intake ticket as it stands.

> Fieldbook is a finance and government-contracting tool for one user, built to run on a company laptop and network as an extension of company infrastructure. The first deliveries are static files: public finance and government-contracting sources, summaries of them, and registers, with a citation to the original public document for each statement. They are delivered as two zip archives (web pages with JavaScript and style sheets, CSV files, Excel workbooks without macros, text, and public PDFs), opened from local disk in Microsoft Edge and Excel. The first deliveries contain no executable, installer, macro, service, driver or scheduled task. Tier 0 and every edition open no socket and make no request. Each delivery lists every file with its SHA-256 hash and a count by file type. Its pages are loaded in a test browser with the network log checked, and the result is included. The files hold public sources and synthetic examples only. They never hold company, customer, personal, controlled unclassified or export-controlled information, and nothing is sent back from the laptop. No AI model is part of Tier 0 or of any edition. We ask IT to name the approved transfer channel, and to say whether zip archives that contain `.html` and `.js` files may cross it. A signed program is a later request and is not part of this one. If IT approves it, that program keeps the company's own records (quotes, estimates, specifications, schedules, transactions) in a store on the laptop, never in an edition or a source repository, and can read from company data sources that the owner names. The AI build (a separate program from tool version 0.2.0, not part of the first edition) sends text to an AI endpoint, and `fieldbook mcp` (from tool version 0.2.0) answers a tool client, only as a policy file written and owned by IT allows, for the labels that file lists, and the program builds a Handoff pack for the owner to paste (from tool version 0.2.0) only from the labels that file allows; with no such file it works on public text and the owner's own words. Those are further requests with their own questions (IT-27 through IT-39). Fieldbook makes no claim of certification or compliance under any scheme.

## What reaches the laptop

| Tier | When | What arrives | Kind of channel | IT action |
|---|---|---|---|---|
| 0 | E1, the first approved transfer | Two unsigned zips: the edition (site, registers, templates, `intake\`) and the originals (public documents, byte-exact). The hash list for both | A file-transfer channel IT already approves. The hash list goes by a different route. Fieldbook proposes no channel and uses none that IT has not named | Name the channel (IT-9). No software rule needed: the files are data |
| 1 | R2, after approval | `fieldbook.exe`, signed, installed as a managed copy in `Program Files\Fieldbook`, plus the SBOM and a signed file catalog. If IT approves an AI build (IT-27), it arrives as a second signed program, so the first stays as approved | IT's own software distribution (an endpoint-manager package). The owner does not carry it | Intake, allowlist rule, package |
| 2 | Only if IT requires it | An MSI with the same content | The same | The same |

Nothing else reaches the laptop from Fieldbook. Two things can arrive from elsewhere and are not Fieldbook deliveries: the policy file, which IT writes and places in `%ProgramData%\Fieldbook\` if it approves the program (`policy.md`), and the files of the company data sources that the installed program reads (step 9 below). The source, the Rust toolchain and the vendored crates are review material for IT: from the Day 60–75 intake request, IT can receive them as archives to inspect and rebuild. They are not installed or run on the laptop. Handoff (from Day 30) adds no file. It is text the owner copies by hand.

## Data flows and where data rests

| Step | From to | Data | How | Control |
|---|---|---|---|---|
| 1 | Public sources to the owner | Public documents | The owner downloads them in a browser on personal equipment and adds them with `fieldbook add`, which hashes them and refuses marking banners | SC-02, ED-03 |
| 2 | Owner to a private GitHub repository | Public Corpus, code, documents | `git push` after the pre-push checks | SC-01 to SC-05, ZN-01 |
| 3 | Repository to hosted CI runners | The same | Build, test, scan. The workflow token is read-only | CI-01, DP-01 to DP-06 |
| 4 | CI to release files | Two zips and a hash list | A tag by the owner starts the release | RL-01, ED-01 to ED-03, IN-01 |
| 5 | Release files to the laptop | The same | The channel IT names. The hash list goes another way | SG-01 |
| 6 | On the laptop | Edge and Excel read files from disk. The owner saves working copies | Local only | NS-03, WB-01, XL-01, DR-01 |
| 7 | Laptop to an assistant (from Day 30) | A context pack plus the owner's question. The pack carries only the labels that the policy allows for Handoff: with no policy file, public text and the owner's own words, and its first line lists them | The owner copies and pastes by hand into an assistant IT approved (Handoff). In Tier 0 and every edition Fieldbook never contacts it | HG-01 to HG-06 |
| 8 | Laptop to anywhere else, in Tier 0, every edition and the silent build | Nothing in Tier 0 or any edition, and nothing from the silent build | No code path in them sends anything. The AI build (from `v0.2.0`, only if IT approves it) makes the requests of step 11, to an endpoint that both the owner's settings and the company's policy file name (IT-27, IT-33) | NS-01 to NS-07 |
| 9 | Company data sources to the company store (the program, from R2) | The files and exports that the owner can already read: quotes, estimates, specifications, schedules, transactions | `sync` reads a folder that the company's own tools fill (a read port), as the owner's account, and writes nothing to the source. It runs only for a source the owner lists. What it reads is labelled by the policy's source rules, or is `unlabelled` and goes nowhere | SY-01, GD-01, LB-01 to LB-03 |
| 10 | Company store to a file outside the data root (the program, from R2) | A backup (from tool version 0.1.0), an export, a print, or the journal's own lines (from tool version 0.2.0) | Only to a place the policy names (from tool version 0.2.0; the built-in floor before that), for the labels the file carries. An export lists its labels in the file. `audit export` (from tool version 0.2.0) writes the journal to the folder that the policy's `[audit]` table names, and IT collects it with its own tools (IT-35) | EG-01, PL-01, AL-06 |
| 11 | Company store to an AI endpoint (the AI build, from `v0.2.0`) | A prompt: a context pack and the owner's question, with the labels that each part carries | HTTPS to an endpoint that both the owner's settings and the company's policy file name, for the labels that file lists for it, after the audit line is written. Handoff follows the same rule for its row, and the tool server serves `public` text only | EG-01, NS-01 to NS-07, HG-01 to HG-14 |

In Tier 0 and every edition the only data that leaves the laptop does so in step 7, by the owner's hand, and only if IT has approved an assistant. The AI build adds a second path, a call to an endpoint that the company's policy file names, for the labels it lists, written to the audit journal first (IT-27). Where each place keeps data, by label, is in the data-handling statement (`data-handling.md`, section 2). Data carries labels. `public` is the only label that appears in git or in an edition. A company label (an NDA, a competition, a bid, a controlled category) is held only in the company store under the data root, and only where the company's policy lets that store hold it. Controlled unclassified and export-controlled material is held only where the company has authorised the laptop and the store for it; Fieldbook never classifies it and never strips a marking.

## The 17 artifacts

Tier 0 ships artifacts 1, 2, 5, 6, 7, 8, 9 and 14 in every edition. The others come in the Day 60–75 intake request and at R2. `cargo xtask edition` fills the Tier-0 sources in this folder with versions, hashes and counts. `cargo xtask intake` produces all 17 at R2. If IT supplies its own templates (IT-3), the content moves into those.

| # | Artifact | Purpose | How produced | Day 1 Tier-0 form | Day 60–75 form | R2 form |
|---|---|---|---|---|---|---|
| 1 | Overview | A one-page yes or no for triage | A template filled by `cargo xtask edition` | `TRANSFER-STATEMENT.txt`, the Tier-0 variant | | Full overview |
| 2 | Application profile | Fills IT's inventory record | The file inventory comes from the edition build; at R2, `cargo xtask intake` reads the signed program | File inventory and hashes: `SHA256SUMS` in each zip, plus the counts by type | | Profile of the signed exe |
| 3 | Threat model and data-flow diagram, with Handoff, the company data sources and the policy file included | Shows the trust boundaries and the residual risks | Written by the lead and reviewed by the owner | | Draft | Final |
| 4 | Control matrix (NIST SP 800-171 Rev 2, with a Rev 3 crosswalk) | Maps each property to IT's system security plan, with "not supported" rows stated | Written from the [control register](../controls.md), whose rows are its evidence links | | Draft | Final |
| 5 | Data handling | Answers the data review in advance | `data-handling.md`, filled with edition and tool version | The statement | | Final |
| 6 | SBOM, VEX, dependency report | Supply-chain visibility | Day 1: the file-type counts in the transfer statement. Later: CycloneDX 1.5 with the serial number and timestamp normalised, plus a licence report. VEX only when an advisory applies | Files only; no executable content, shown by the counts and the edition allowlist | CycloneDX 1.5 and the licence report | Final; VEX only when an advisory applies |
| 7 | Build provenance | Lets IT see how the files were built | `edition.toml` and `SHA256SUMS`; later `BUILD-INFO.toml` (commit, `SOURCE_DATE_EPOCH`, toolchain and build-tools versions and hashes) | `edition.toml` and `SHA256SUMS` | Two builds and the offline rebuild | Final |
| 8 | Signing and integrity guide | Lets IT check the files and write an allowlist rule quickly | `integrity-guide.md` | `Get-FileHash -Algorithm SHA256` on each zip, the out-of-band hash, Extract All to `%LOCALAPPDATA%\Fieldbook\` | | Plus Intune install, uninstall and detection (path plus file version), return codes, file-publisher XML with a minimum version and a hash fallback, the durable-identity EKU, Defender indicator inputs |
| 9 | Network-silence report | Shows the tested edition makes no network request in the tested interactions | The Edge net-log harness (`network-silence.md`, `network-silence-summary.txt`) | Edge net-log summary | | Plus the Windows capture and the PE import check |
| 10 | Code analysis | Secure-by-design evidence | Lint output and a count of first-party `unsafe` code; later BinSkim and fuzzing | | clippy results; first-party `unsafe` count: the blocks in `crates/fieldbook-win/src/sys.rs`, none elsewhere | Plus BinSkim and fuzzing |
| 11 | Install and uninstall diff | Shows a clean footprint | Generated from the signed release | | | Managed-copy file list; an MSI diff only if IT asks |
| 12 | Vulnerability and patch policy | Disclosure and patching | [`SECURITY.md`](../../SECURITY.md) and the patch targets | | Known-exploited flaws within 14 days, high within 30 | Final |
| 13 | Release and change process | Shows controlled change | The release runbook and `CHANGELOG.md` | | Draft | Final |
| 14 | AI development statement | Settles the AI policy question early | `ai-development-statement.md` | Yes, describing Handoff and stating that the AI build is not in the edition | | Final (IT-7, IT-27) |
| 15 | Evaluation guide | A 15-minute review in a disposable environment | Written, with a Windows Sandbox configuration that disables networking | | | Yes |
| 16 | Support and continuity | Maintenance confidence | Written | | Draft | Final |
| 17 | Authorisation record | The sponsor, the approval reference, the statement on ownership and the labels this laptop is authorised to hold (IT-2, IT-34) | IT completes it | | | IT completes |

## The Tier-0 files and their formats

Each edition carries `TRANSFER-STATEMENT.txt` at its root and the four statements and the net-log summary in `intake\`. `it-questions.md`, `policy.md`, `policy.example.toml` and this README stay in the repository. `{name}` is reserved for placeholders in the five source files; the edition build fails if one is left unfilled.

| File | Format |
|---|---|
| `SHA256SUMS` in each zip | sha256sum text: a lowercase 64-character hash, two spaces, a path with forward slashes relative to the file's folder; sorted by path; LF; every file in the zip except itself |
| `SHA256SUMS` beside the zips (the out-of-band hash list) | The same format, two lines, one per zip |
| `edition.toml` | TOML: edition, tool version, commit, schema version, minimum tool version, Corpus fingerprint, counts. Field names are in the [corpus format reference](../reference/corpus-format.md) |
| `network-silence-summary.txt` | Plain text, `name: value` per line; fields and how to read them are in `network-silence.md`. It records the Edge version. DT-01 first verifies both complete packages, then excludes only this summary, TRANSFER-STATEMENT.txt and their derived checksum/container differences from deterministic payload comparison |
| `BUILD-INFO.toml` (Day 60–75) | TOML, as the table above lists |

Placeholders in the five source files, and where `cargo xtask edition` takes each value:

| Placeholder | Source |
|---|---|
| `{edition}`, `{edition_number}` | The tag, such as `edition-007`, and its number |
| `{tool_version}`, `{commit}`, `{schema_version}`, `{corpus_fingerprint}` | `edition.toml` |
| `{source_date}` | `SOURCE_DATE_EPOCH` from the commit, as a UTC date |
| `{n_files_edition}`, `{n_files_originals}`, `{n_html}`, `{n_js}`, `{n_css}`, `{n_csv}`, `{n_xlsx}`, `{n_md_toml}`, `{n_txt}`, `{n_pdf}`, `{n_other}` | Counted from the two trees as packed |
| `{n_pages}`, `{n_sources}`, `{n_claims}`, `{n_questions}` | `edition.toml` counts |
| `{edge_version}`, `{n_pages_checked}`, `{n_text_originals_checked}`, `{n_nonlocal_requests}` | The harness result for this edition |
| `{security_contact}` | `FIELDBOOK_SECURITY_CONTACT` from the invoking environment; the owner binds the same-named repository secret for CI packaging. Missing or blank input fails IN-01. Real values are never committed; tests use synthetic text |

## What IT commonly rejects, and how the design avoids it

| Trigger | How Fieldbook avoids it | Controls |
|---|---|---|
| Needs administrator rights, installs a service, driver, scheduled task or startup entry, or writes outside its own folder | Tier 0 is data and nothing runs. The later program runs as the invoking user, installs no service or startup entry, and writes only to its data folder and to the backup targets named in its settings or on the command line (`backup --to`). A snapshot holds only the labels that the policy's `backup` row allows. The program reads the policy file and never writes it | ED-01, CD-01, DR-01, SG-04 |
| Ships or needs an interpreter or macro host, or downloads code | JavaScript runs only in Edge. No Windows Script Host, no macros (macro-enabled workbooks are refused), no runtime. The program is native Rust. If IT refuses `.js` files in a zip, the fallbacks are inline scripts under hash-based policy, then a no-script edition (IT-12) | ED-01, WB-01 to WB-03 |
| Updates itself, phones home, or sends telemetry or crash reports | The silent build has no network code. The AI build (from `v0.2.0`) has one HTTPS client, confined to one file, that connects only to an endpoint named in the owner's settings and in the company's policy file, on an explicit command. An updater, update check, version ping, telemetry or crash upload is never built. Updates are new editions and releases through IT's channel | NS-01 to NS-07 |
| Opens a listening socket or runs a local server | The silent build and every edition: none. The AI build built with `bridge` (from tool version 0.3.0) opens one listener, on 127.0.0.1, only while the owner runs `fieldbook chat`, behind a per-session token (IT-32). `fieldbook mcp` uses standard input and output. The signed program's imports are checked (NS-06) | NS-01, NS-02, NS-04, NS-06, HG-13, HG-14 |
| Unsigned, self-signed, asks for a root certificate, Defender exclusions or weaker SmartScreen | Tier 0 holds no executable, so there is nothing to sign. Its zips are protected by the out-of-band hash. The program is signed with a timestamp and one subject. Fieldbook asks for no root, no exclusion and no setting change | SG-01 to SG-03 |
| Uses cloud AI, cloud sync or third-party services with company data | AI is optional and under the company's controls. The default route is Handoff, by hand, with an assistant IT approved. An endpoint is called only if IT's policy file names it, for the labels that file lists for it, with every call audited. A sync reads a source only after the owner lists it, as the owner's account, and writes nothing to it. The source repository holds public data only | HG-01 to HG-03, HG-09 to HG-12, AL-04, ZN-01 |
| Custom cryptography, home-made authentication, stored passwords | None of its own. SHA-256 for integrity only, and no accounts. The AI build (from `v0.2.0`) uses a maintained TLS library, a random per-session token for the chat page (from tool version 0.3.0), and reads an endpoint key from an environment variable without storing it (IT-28) | CD-01, HG-14 |
| Obfuscated code, injection, hooking, screen or keystroke capture, reading other applications' data, clipboard history or browser data | None. Scripts are readable, unminified, first-party and few. The command-line tool never reads or writes the clipboard. A page writes to it only when the reader clicks Copy | WB-03, AG-01 |
| Unknown provenance: no source, no SBOM, cannot be rebuilt, unpinned or abandoned dependencies | Source review offered. Pinned toolchain and lockfile, crates.io only, a 14-day cooldown, 12 direct runtime crates at Day 1, each justified (14 once workbooks are read and still 14 at R2; 17 for the listed AI stack; `embed-resource` is build-only). SBOM and offline rebuild in the Day 60–75 rehearsal | DP-01 to DP-06, DT-03, SG-05 |
| Delivered by personal mail, personal cloud or unapproved media; built with company data | Only the channel IT names. The source repositories are built on personal equipment from public sources and synthetic data. Once installed, the program runs on company equipment and holds company data in the company store, never in the repositories | SG-01, ZN-01 |
| Shell-outs, DLL sideloading, writes to Run keys | Shipped code starts no process, in either build. The program is built with the safe DLL search flag, and writes only to its data folder and the backup targets named in its settings or on the command line, each of which the policy must also allow | NS-02, SG-04 |
| No uninstall, residue, silent deletion of user data, no export | Delete the data folder under the retention procedure, and clear page storage through IT’s browser procedure. Open formats, and `backup` writes plain files. `working\` is never touched by an update | DR-01 |
| Claims to be certified or compliant | No such claim anywhere. A check fails the build on listed phrases in Corpus prose | SC-05 |
| No sponsor or authorisation for personally written software | IT-11 is asked before any executable request. IT completes artifact 17 | IT-11, artifact 17 |
| Strips markings or classifies content | Never. It never classifies and never strips or edits a marking. For the public Corpus it refuses a file with a marking banner and keeps the markings found in public originals. In the company store it gives the file a label, holds it, and leaves the decision about where it may go to the policy | SC-02 |

## Renderable originals

An HTML or SVG original can run script when Edge opens it. So `.html`, `.htm`, `.svg` and `.xhtml` originals ship as `<version-id>.<ext>.txt`, with the same bytes and the same hash recorded in the sources register. Edge shows them as text. The edition build refuses any such file outside the site (ED-01). The harness opens every one of them (ED-02, NS-03). Links to section anchors in them go to the page that Fieldbook builds for that source version. PDFs are kept byte-exact as published and open in the viewer the laptop already uses.

## Mark-of-the-Web and Protected View

Files that arrive from outside carry the Mark-of-the-Web. Fieldbook never removes it, and no document tells the reader to. README.txt in each edition says so. Excel opens marked workbooks in Protected View. `data\registers.xlsx` is read-only by design and reads fully there. The fillable templates are copied to `working\` before they are edited. A spike on the laptop checks how a marked test workbook behaves under the corporate Excel policy. Until it passes, the decision package (Day 30) is labelled "preview".

## Update, rollback and removal

A new edition replaces `edition\` whole, at the same path, behind one bookmark. The previous zip is the rollback. `working\` is not touched. Removal deletes `%LOCALAPPDATA%\Fieldbook\` under the retention procedure; clear Fieldbook page storage separately through IT’s browser procedure. Neither action promises secure erasure. There is no installer, registry entry or service at Tier 0. Steps are in `integrity-guide.md`. The program (Tier 1) never updates itself. A new version arrives only through IT's channel, and an edition never needs a new program.

## Signing route options

| | Route | Notes |
|---|---|---|
| R1 (Tier 0) | None. Unsigned zips, with SHA-256 hashes sent by a separate route | The hash is the anchor until R2. Signed checksums at Tier 0 were considered and not adopted |
| R2, option 1 | IT, or an employer signing service IT names, signs the build or a catalog of it | The preferred route. Fieldbook delivers the unsigned program, its digest and the SBOM |
| R2, option 2 | A cloud signing service, with the identity of the publisher, if it is open to the owner then | The allowlist rule uses the profile's durable identity, never the shared intermediate certificate |
| R2, option 3 | An OV certificate with a 3072-bit RSA key, in a cloud HSM or on a hardware token | The key never sits on a runner as a file. The allowlist accepts RSA signer keys up to 4096 bits and no ECDSA |

Whatever the route: Authenticode with an RFC 3161 timestamp, one subject for the life of the product, and a Windows file catalog signed with the same identity that covers every release file and the SBOM. IT checks the catalog with `Test-FileCatalog`. No self-signed certificate and no SSH signature is used. The owner dispatches the signing workflow by hand. It checks the actor and runs in a protected environment (SG-03).

## The AI position

- AI is designed in and optional. Tier 0, the first edition and `v0.1.0` contain no model and make no connection to an AI service. Checking, building, searching, calculating and editions never call a model, in any build.
- The default route is Handoff: the owner pastes a context pack into an assistant IT has approved. The pack is public text unless the company's policy lets Handoff carry more, and its first line lists its labels, and Fieldbook checks the answer's citations with `cite-check`. From `v0.2.0`, `ask` does this in one command. Fieldbook never calls the assistant in that route. It arrives at Day 30.
- From `v0.2.0` an AI build can send the same pack to an endpoint that the company's policy file names, with only the labels that file lists for it, after an explicit command, with a line in the audit journal written first. Every output is a proposal a person accepts. The AI build is not part of the first edition.
- From tool version 0.2.0, `fieldbook mcp` serves read-only search of public text to an approved AI client over standard input and output, and the policy's `mcp` row can list the clients allowed.
- IT decides the endpoint (IT-27), the credential (IT-28), the client (IT-29), the route (IT-30), the record retention (IT-31), the chat listener (IT-32; the chat listener is from tool version 0.3.0) and, above them, the policy file (IT-33), what marks company data (IT-34), where the journal is exported (IT-35), who holds the store when the owner leaves (IT-36), disposal and retention (IT-37), the credential store and the connector account (IT-38) and the features that read page text (IT-39). The company's approval opens an endpoint for the labels its policy row lists, and no more. Until it answers, the defaults apply: no policy file, so the floor; no endpoint, Handoff only, no client on the laptop, and `chat` is not run. The AI works over public text, synthetic text and what the owner pastes.
- The company application (what the program is part of when the company runs it: its stores and labels, the policy file, the destinations, the connectors and who decides each) is section 5 of `ai-development-statement.md`.
- How assistants built the product is in `ai-development-statement.md`.

## Files in this folder

| File | In every edition | Purpose |
|---|---|---|
| `README.md` | No | This index |
| `transfer-statement.txt` | As `TRANSFER-STATEMENT.txt`, at the edition root | Artifact 1 |
| `data-handling.md` | In `intake\` | Artifact 5 |
| `ai-development-statement.md` | In `intake\` | Artifact 14 |
| `integrity-guide.md` | In `intake\` | Artifact 8 |
| `network-silence.md` | In `intake\` | Artifact 9, with the generated summary |
| `it-questions.md` | No | The questions for IT: [IT-1 to IT-39](it-questions.md) |
| `policy.md` | No | What IT decides and writes for the program: the policy file, its labels, stores and destinations (IT-33 through IT-39) |
| `policy.example.toml` | No | A synthetic example of the policy file for IT to start from. A test parses it |

## Raw edition floor

When the plan selects the raw floor, run `cargo xtask edition-raw --out out/dist`. This owner-reviewed path uses the public file selection and paired checksum rules, with strict-check and regression evidence, the original files and Tier-0 copy instructions. It ships no site, executable or browser summary. The intake text states those limits; it makes no browser-test claim. Verify both archive hashes and internal manifests, then inspect representative raw Markdown, CSV and text and verify the originals against their register hashes. The edition archive holds `evidence/check.json`, `evidence/regression.json` and `evidence/corpus-support.toml`; the same catalogue bytes are also written to `out/dist/corpus-support.toml` with a one-line `corpus-support.sha256` that names them, and both are kept as release evidence. The catalogue and its digest are evidence of bytes, not of public provenance. Keep the previous pair for rollback. The normal site edition still requires its renderer and browser gate. The tag steps are in the [release runbook](../runbooks/release.md#raw-edition-floor); the generated `README.txt` and `TRANSFER-STATEMENT.txt` of a raw edition are written by `xtask/src/edition_raw.rs`; the [raw edition reference](../reference/raw-edition.md) lists the delivered files and the content rules.

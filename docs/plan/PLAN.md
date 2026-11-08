# Plan

This is the execution plan for Fieldbook (AD-21 in [ADR 0001](../decisions/0001-foundation.md#ad-21-codex-execution-plan)). It lists every card in order, and the owner's and the lead's steps. It holds order and phases only: the status of each item is in the owner's tracker, not here.

## How Codex uses this plan

1. Take the first item whose card file exists in [docs/cards/](../cards/) (a finished card's file is deleted; `docs/cards/drafts/` holds cards for phases not yet reached) and whose preconditions have merged, and whose work item in the tracker is not In Progress or Blocked. Skip items marked `(owner)`: they are the owner's. An item marked `(lead)` is drafting work that any agent may take.
2. Read [AGENTS.md](../../AGENTS.md), then the card file. Find the card's work item by its slug and set it In Progress. Work on the branch `card/<slug>`, as an agent author ("Codex (agent)", or a `Co-authored-by` trailer naming the assistant).
3. Follow the card: Steps, Acceptance, Negative control. Keep to its Scope. About 400 to 600 changed lines is normal; split a bigger card at a seam, as [docs/cards/README.md](../cards/README.md#size-and-splitting) says. Close the card as [docs/cards/README.md](../cards/README.md#closing-a-card) says.
4. This file does not change when a card closes. Status is kept in the tracker and nowhere else.

**The tracker.** Work-item status lives in the owner's tracker, not in this repository. Each card is one work item titled by its slug; the slug is the only link. The repository holds no tracker key, item address or status. Find your item by its slug with the tracker command your own agent instructions give. Owner and lead steps are work items too. This file has no status mark.

**Line forms.** An item is `- <slug>: <one line>`. A step for the owner ends `(owner)` and a step for the lead ends `(lead)`; these have no card file. Every slug here is the name of a card file, written or still to be written, or of one that a merged pull request deleted when the card closed. A `later-` slug is the exception: it is a row of the Later table in [AD-24](../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later), it has no card file until the lead writes one, and Codex skips the line because step 1 needs a card file. The `later-` lines follow the owner's four groups: group 1 (the whole knowledge base) under P6 after `release-1-0-0`, group 2 (pricing, estimating and the rate book) under P7, groups 3 and 4 (estimate at completion and earned value, then the rest) under P8. The cards of P4, P5 and P6 are drafts in `docs/cards/drafts/`; a draft moves to `docs/cards/` by one `git mv` when the phase before it ends. A P4 to P8 line in this file names a draft or is a one-line item with no file.

**`workspace-skeleton` is the one item with no card file and no deleting commit.** It was built
before card files existed, so it counts as merged for every precondition, and the plan lint
exempts it. Its line stays for the record, and nothing waits on it.

**Readiness.** A green `just check` shows that the code works; it is not readiness. Authorisation is IT's ([intake artifact 17](../intake/README.md#the-17-artifacts)).

**Merging (D-5).** Codex merges `auto` cards on a green `ci-ok`, including CI and test cards that touch no permissions or secrets. Commits carry the agent author "Codex (agent)". A card marked `Review: owner` waits for the owner. The owner reads the week's merged work in the weekly review; nothing waits for it.

**The three owner gates** (AD-21). The release gate: tags, releases, signing, CI permissions, secrets, the network rows of `deny.toml`, anything under `crates/fieldbook/src/io/net*`, anything in `crates/fieldbook-win`, the label, policy and egress modules of `fieldbook-core` with their leak matrix, and the IT-facing statements in `docs/intake/` and `SECURITY.md`. The status gate: verifying a consequential claim, and accepting a page as `stable`. The format gate: a Corpus format change after the freeze, which needs an accepted ADR 0002 amendment first. A card that touches a gate is `Review: owner`, and so is a card whose Acceptance needs the owner at the laptop; its Why says which. A dependency or a lock refresh is not on the list: it is `auto` with passing committed-lock Cargo/cargo-deny and required exact-source MAX/JEV pin/profile/release-age review, with the dependency row in the same pull request. Security/cooldown and licence exceptions retain owner review. A crate past the 15th direct crate of the AI build needs a card whose Review is `owner`.

**Flow.** Take the first card whose Preconditions have merged. A card's Preconditions name another card only when it uses what that card builds. Cards in flight share no path in their Scope; `docs-checks` fails two cards whose Scope overlaps with no precondition path between them. The shared paths are this file, `CHANGELOG.md`, `crates/fieldbook/src/check/mod.rs` and the other registration files that many cards extend by a line (the list is in [Size and splitting](../cards/README.md#size-and-splitting)): rebase onto `main` before the last commit and keep both sides of a conflict. The lead sets how many cards run at once; start at four and raise it while the queue is deep.

**Cut line and floor.** When time is short, cut from the bottom of this table up; the same table is in [Cut line and floor](../decisions/0001-foundation.md#cut-line-and-floor). Must never moves. Search is Must.

| Priority | Scope |
|---|---|
| Must | `check`; `build` (home, area, page, source version, one generic register template, about, ids, day-one binder); `add` (one file or a folder, with a per-file report); `new`; text extraction; the Day-1 calculation core; deterministic zips; `search` in the CLI and the site, with the regression set; the Edge net-log harness (`edge-harness`); `guard-statuses`; the audit journal; the label, policy and egress core of `fieldbook-core` (`core-labels`, `core-policy`); `backup` and `restore` with the restore drill |
| Could | `verify`, `status`, `fmt`, page context packs, the question log (`ask-log`). They ship by Day 30 if not by Day 1; page context packs go first. `verify`, `status` and `ask-log` are Preconditions of `review-queue`, `model-handoff-ask` and `handoff-context`, so they are built before those cards and not cut |
| `edition-002` | `changes.html` |
| After R2 | P7: pricing, estimating and the indirect rate book. P8: estimate at completion and earned value. They are not on the Day-1 cut line |

**Floor.** If Must is not done by Day −1, `cargo xtask edition-raw --out out/dist` builds `edition-001` with the raw files, `SHA256SUMS`, a clean `check`, the regression result, the written Tier-0 copy steps and the transfer statement, with no site. The site follows in `edition-002`. The floor triggers on the whole Must set, not on search, with one exception: `backup`, `restore` and the restore drill do not trigger it. If they are the only Must items not done, `edition-001` ships on the home gate with the written Tier-0 copy steps in place of Drill 4, and they ship in a `v0.1.x` patch before Day 30. The audit journal is not exempt, because `add` depends on it.

**Day 30 cut order.** If Day 30 is at risk, cut the P4 cards in this order; a cut card moves to P5 unchanged and the Day-30 gate lists it as not built. First `audit-export`, then `stage-command`, then `mcp-server`, then `pipeline-model-adapters`, then `model-adapter-openai` together with `net-stack-spike`. Never cut: `ai-kernel`, `ai-config-and-keys`, `model-gate`, `model-handoff-ask`, `pipeline-ports`, `pipeline-rules-classify`, `pipeline-correlate`, `review-queue`, `ai-drill`, and the Handoff cards already in P4. Cutting the endpoint adapter leaves Handoff only; nothing built earlier depends on a later card. The P5 cards are outside this order, and `index-output-ports` is Could.

The lead may move `build-charts` after P3; edition packaging, release, backup and restore do not depend on charts; the area pages then keep their empty-state lines. At each phase exit the lead measures cards merged per day against the plan and raises concurrency first, merges adjacent cards second, and applies the floor last.

**Reserved ADR numbers.** 0001 and 0002 are taken. 0003: Corpus format v2 (schema 2, the `note` page type, Day 30). 0004: the company boundary (labels, the policy file, egress, stores, sync, ids, records, identity and audit), which holds the R2 shape. 0005: the accepted financial-work foundation. Its work scope, periods, evidence and versioned methods apply before `calc-day1` and `r2-records`; later pricing modules use them. A new number is reserved here before its ADR is written.

**Owner minutes.** The weekly budget is upkeep 30 to 60 minutes and review 60 to 120. Each week the owner adds one row to [docs/maintenance.csv](../maintenance.csv); this file holds no weekly line. The log is a measure: the owner reads it, and nothing stops when it runs over.

## Stop and ask

Stop and ask the owner, with a `BLOCKED: <what>` comment on the card's work item, when:

- a Corpus format change is needed after the freeze;
- a change touches a CI permission, a secret, signing, a tag or a release step;
- a negative control stops failing;
- a change adds a network path or a listener;
- anything would bring company, controlled or personal material into the repository (it belongs in company storage, under the data root).

Report and carry on (set the item Blocked, push the branch, take the next card whose Scope is disjoint) when:

- a precondition is missing, or an Acceptance command cannot pass;
- a seed golden case fails: name the case id; never edit a `seed-*` file;
- the lead's committed content fails a check that has landed: name the code and the file;
- ADR 0001 or the reference documents disagree with [docs/design/](../design/), or a card needs a token, string or component the design does not give: report the conflict; do not invent and do not edit `docs/design/`.

Everything else is your call: decide, say why in the pull request, and carry on. The owner clears a block by answering on the work item or in the pull request, or by changing the card.

## F0 Freeze

Exit: see [Phases and exits](../decisions/0001-foundation.md#phases-and-exits), ending Day −9: ADR 0002, ADR 0004 and ADR 0001 accepted and merged, AGENTS.md, PLAN.md and `corpus/ASSISTANT.md` merged, the repository on Pro with its ruleset, hand acquisition started.

- Apply format-changing amendments before accepting ADR 0002 (lead)
- Accept and merge ADR 0002, then ADR 0004, then ADR 0001, by Day −9 (owner)
- Put the repository on GitHub Pro with a ruleset on `main` that requires `ci-ok`, by Day −9 (D-4) (owner)
- Merge AGENTS.md, this plan and `corpus/ASSISTANT.md` by Day −9 (lead)
- Accept each `docs/design/` revision from the UI lane by pull request (owner)
- Create one tracker item per row of the Later table in AD-24, titled by the row's slug; skip a row that already has one (lead)
- Create one tracker item per card in `docs/cards/drafts/`, titled by its slug (lead)
- Acquire public sources without the tool (download, `sha256sum` plus a `sources.csv` row) from F0 until `add-new-and-edition` merges (lead)

## P1 Read

Exit: see [Phases and exits](../decisions/0001-foundation.md#phases-and-exits), ending Day −6: every P1 card in this file merged; `check --strict` clean on the lead's first content; pages render. Content pull requests merge from here.

- workspace-skeleton: the workspace, the toolchain and tool pins, `just check` and the CI skeleton
- hook-environment-isolation: keep push-hook Git context out of scratch-repository checks
- product-first-delivery: standard-Cargo dependency work order with actual review evidence; complete custom automation deferred
- kb-foundation-guard: exact authority and source-free foundation transitions, since retired with GS-01
- kb-foundation-content: the pinned draft onboarding page, 32 open questions and 51 pending definitions
- core-types-and-strings: standard-library hexadecimal rendering and schema-version lookup
- day1-dependencies: complete catalog/workflow reference facts with source locations, without a policy gate
- day1-dependencies-inputs: workspace/tool/replay configuration and the complete strict input aggregate
- day1-dependencies-checks: strict Cargo metadata shapes, identities and complete declaration/edge facts
- day1-dependencies-checks-2: complete target/profile measurement and raw declaration pin facts
- day1-dependencies-checks-3: complete profile budgets and measurement reports
- day1-dependencies-3: initial crate adoption with committed-lock Cargo/cargo-deny and exact-source MAX/JEV pin/profile/release-age evidence
- core-types-and-strings-2: complete frozen words/chips, `strings::js()`, docs generation and the literal control
- calc-day1: the decimal grammar, `Money`, `Value<T>`, the `Unit` words, the civil and partial date, the period and the duration, and the seeded `Rng` with its one-copy test
- core-ids-and-dates: every identifier form and the `FAMILIES` table, next-id allocation, and the dates of `fieldbook-calc` re-exported
- core-schema-pages: typed `corpus.toml` and page front matter with error positions
- core-csv: read and write the five registers in normal form
- core-schema-registers: typed rows for sources, questions, conflicts and glossary
- core-schema-claims: typed claim rows and the raise-or-not test for statuses
- guard-statuses: the field-level status guard replaces the path rule
- corpus-mini-fixture: the synthetic Corpus every negative control copies
- Write `tests/golden/search/seed-ranked.json`, 10 ranked cases, after `corpus-mini-fixture` merges and before `search-ranking-and-index` (lead)
- check-v0: the diagnostic model and generated `errors.md`
- check-run: `check::run` over a Corpus value, FB0100, the fixed pass list
- check-command: `fieldbook check` with `--strict`, `--today`, `--json` and exit codes
- check-known-bad: one known-bad case per code, and every code accounted for
- check-ci-and-docs: `just check` step 7, the `changes` and `corpus` CI jobs, `ci_ok_needs_every_job`, generated `cli.md`
- docs-checks: card and plan lint, link check, `just new-card`
- check-files: FB0120 to FB0125, files, layout, normal form and paths
- check-registers: FB0200 to FB0203, register files, CSV syntax and keys
- check-register-cells: FB0114, FB0204 to FB0206 and FB0210, cell and cross-column rules
- check-claim-cells: FB0306 and FB0307, locator and quote of consequential claims
- check-register-links: FB0207, FB0208, FB0211 and FB0214, references, supersession, original hashes
- check-promotion: FB0209, FB0213, FB0220 to FB0222 and FB0230, promotion and dates
- check-pages: FB0101 to FB0109, FB0114 and FB0116, front matter
- check-page-links: FB0110 to FB0113, FB0115 and FB0136, page references and links
- check-page-parts: FB0130 to FB0133 and FB0137, the part order and the Change note
- check-page-body: FB0134, FB0135, FB0138 and FB0139, required content and body rules
- check-citations: the citation lexer with the seed goldens, FB0300 to FB0303 and FB0305
- check-citation-rules: FB0117, FB0118, FB0309, FB0310 and FB0320
- check-scans: the marking, identity, claim-language and formula scans, FB0400 to FB0405 and FB0410
- core-labels: the label types and sets, the store rules and admission, the labeller, the placement rules, FB0411, FB0412 and FB0415 (owner review)
- calc-ops: `RoundingPolicy`, exact checked operations, `div_at_stage` and unit conversion
- calc-cases: the Day-1 calculation cases CALC-01 to CALC-04 (see [FM0](../decisions/0001-foundation.md#calculation-core-fm0)), the function table `CALC_FNS`, the run record and `CALC_EPOCH`, FB0500, FB0503 and FB0504
- windows-facts: the Windows system calls in fieldbook-win (known folder, principal, group, interactive logons, place test, writers, not-indexed mark), their wrappers in `io/win.rs`, and the windows CI job (owner review)
- audit-journal: the hash-chained journal and its lock, `audit verify` (with `--repair`) and `audit show` (both with `--json`), the `Audited` token, FB0800 to FB0805
- add-new-and-edition: `fieldbook add`, byte-exact, hashed, idempotent, scanned, journaled
- core-policy: the policy types and the floor, `egress::check` and its `Allowed` token, `LabelledView`, the `Authz` port, the leak matrix, FB0413, FB0807, FB0918 and FB0925 (owner review)
- edition-manifest: shared public selection, fingerprint, checksums, allowlist and ZIP helpers; may merge after check-command and core-labels without renderer work
- build-v0: `fieldbook build`, CSP rung 1, the determinism test, `just site`
- build-tokens: the design tokens in `assets/app.css` with the contrast test
- build-frame: the global frame on every page and `app.js`
- build-site-lints: FB0603 to FB0605 on the built site
- build-page: `p/<page-id>.html` in the frozen part order with Sources from citations
- build-page-types: briefs, the 30/60/90 checklist with its timeline, curriculum stages, MKT pages
- build-registers: the five register pages from one template, and `ids.html`
- build-source-pages: `s/<version-id>.html` and the originals, renderable ones as `.txt`
- build-home-and-areas: `index.html` and the 13 area pages with coverage bars
- build-about: `about.html` and the command chip
- build-charts: inline SVG charts and the calc table, `Unavailable` where data is missing

### Day-1 content

Each item is its own pull request that the owner reviews. The first four are here; the briefs, the 30/60/90 checklist and the clause cards are in P2, and the regression set is in P3.

- The status vocabularies (generated from the enums), the 13 areas and the 3 views in `corpus.toml` and `corpus/README.md` (lead)
- The glossary: at least 50 terms with aliases (lead)
- Questions Q-001 to Q-032, with the 16 day-one questions flagged (lead)
- The top 15 consequential claims, entered `unchecked` with a fine locator and a quote (lead)

## P2 Print

Exit: see [Phases and exits](../decisions/0001-foundation.md#phases-and-exits), ending Day −3: every P2 card in this file merged; the day-one binder prints; `registers.xlsx` reads back.

- add-folder-mode: `add <dir>` with a per-file report, `--recursive` and `--json`, the last report in `reports/last-add.json`, FB0821
- new-verbs: `fieldbook new page|claim|question|conflict|term`
- edge-harness: the headless Edge harness with net-log and the negative page, `just browser`
- edition-zips: `cargo xtask edition` and `just edition`, the two zips and their files
- release-workflow: `just release` and `release.yml`
- extract-and-locators: `cargo xtask extract`, the `Extractor` port, `extraction.toml` with `[locators]`, FB0212
- quote-match-and-anchors: quote normalisation, FB0304 and FB0308, `s/` locator anchors
- day-one-binder-and-registers-xlsx: `day-one.html` and the print contract
- registers-xlsx: `data/registers.xlsx` with read-back tests, and the CSV copies
- ai-it-story: the IT documents and `SECURITY.md` for the AI build, which is not in the first edition (docs only)
- Five briefs, BJ-001, BJ-002, BJ-003, BJ-004 and BJ-006, each one printed page (lead)
- The 30/60/90 checklist in relative days only, and curriculum stages 1 and 2 (lead)
- About 30 clause cards, a stretch (lead)

## P3 Find

Exit: see [Phases and exits](../decisions/0001-foundation.md#phases-and-exits), ending Day −1: every P3 card in this file merged except the Could cards, which follow; the home gate rehearsed.

- Write `corpus/eval/regression.toml`, 20 to 30 questions including negatives, before `search-cli-and-regression` (lead)
- Write `tests/golden/search/seed-tokenize-company.json`, 12 company-shape cases, before `search-core-and-cli` (lead)
- search-core-and-cli: the tokenizer with the seed tokenize goldens
- search-ranking-and-index: pure BM25F integer impacts and the query function; no renderer prerequisite
- search-cli-and-regression: `fieldbook search` and the regression runner, FB0126 to FB0128
- search-page-view: `fieldbook search` lists pages first, above an unchanged ranking; `--units` keeps the flat list
- raw-edition: owner-reviewed raw paired archives and floor drill, independent of renderer and browser work
- search-index-budget: the edition search index holds page and section units only under a 15 MB cap; the budget and the text, not the index
- search-site-index: site serialization of the pure index, after renderer prerequisites
- scale-fixture: `cargo xtask scale`, a synthetic Corpus at 1, 5 and 20 times Day 1 size, and an advisory table
- scheduled-workflow: the weekly `scheduled.yml` with the `lint-controls` and `scale` jobs
- search-js: `search.js` and `search.html`, held to Rust by the goldens
- search-palette: the command palette, search trigger and shortcuts dialog
- backup-snapshot: `backup`, `backup list` (with `--json`) and `backup verify`, the `[backup]` settings, retention, and the `backup_age` reminder
- restore-and-drill: `restore`, `restore --drill` and Drill 4 in CI on the disposable build-checkout Corpus
- verify-status-fmt: `fieldbook verify` (Could)
- status-command: `fieldbook status` and `fieldbook status --page`, the desk pages (Could)
- fmt-command: `fieldbook fmt [--check]` (Could)
- page-context-packs: a context pack on each page (Could)
- ask-log: the question log `asked/asked.csv`, written by `search` (Could)
- Verify the 15 consequential claims by Day −2, with `verify` or by hand edit (owner)
- Rehearse the home gate with the lead by Day −1 (owner)

## R1

Exit: see [Gates](../decisions/0001-foundation.md#gates), Day 1: the home gate holds; the owner tags `edition-001` and `v0.1.0`.

- Run the home drill (Drill 1) and the backup drill (Drill 4) and comment "home drill passed" with the commit on its work item (gate item 9) (owner)
- Tag `edition-001` and `v0.1.0`, and print the day-one binder, on Day 1 (owner)
- Send [docs/intake/it-questions.md](../intake/it-questions.md) at the first IT contact (owner)

## P4

Exit: see [Phases and exits](../decisions/0001-foundation.md#phases-and-exits), ending Day 30: the E1 gate whenever IT-9 is answered, and the Day-30 gate; `v0.2.0`.

- transfer-and-windows-drill: drill 2 added to the `windows` job
- claims-register-split: the claims register as one page per area with an index, so every page loads within the harness's 60 seconds and the register-page exception ends (placement: lead)
- Run spikes S2, S5 and S6 on the laptop at E1 (owner)
- Run Windows restore drill 3 on the laptop after the first transfer, at E1 (owner)
- Write the 20 cite-check seed cases in `tests/golden/handoff/` before `handoff-cite-check`, at least three of them with a `context` list (lead)
- e1-workbook-templates: the three data-only templates for E1
- ai-kernel: kernel types, the adapter registry, `Ctx` and `AiCtx`, the scans for network code outside `io/net*` and for a model call in a deterministic verb
- policy-load: the policy path, `fieldbook policy show` and `policy check`, the closed state, `policy.change` and FB0414
- ai-config-and-keys: the AI settings tables, the `Key` type, `ai status`
- model-gate: the Model port, `Guarded` (the one audited gate), call records, retry and fallback, contract suite M
- audit-export: `fieldbook audit export` to the policy's audit sink, the manifest and `audit verify --anchor-seq N --anchor-head HEX`, FB0806
- handoff-context: `fieldbook context`, the `ai.pack` event, the context items with their reasons, and `apply_edits` and `resolve_pack` for the page and the Bridge
- handoff-cite-check: `fieldbook cite-check` with nine statuses, `outside context` among them; the engine becomes the chat's back end, and its seed cases double as `ask` fixtures and as the vectors of the page's JavaScript port
- pipeline-ports: the Classifier, Correlator and Admit traits, `staged@1`, the contract suites
- extract-csv-rows: the `csv-rows` extraction method for tables, FB0311
- extract-xlsx-sheets: the `xlsx-sheets` extraction method for workbooks, read with `calamine`; `sheet` and `rec` locators on a workbook
- pipeline-rules-classify: `corpus/pipeline/rules.toml`, the `rules@1` classifier, FB0826
- Write about 10 starting rules with examples in `corpus/pipeline/rules.toml`; `check` replays each on its example (lead)
- pipeline-correlate: the `exact-ids+bm25@1` correlator
- review-queue: `add --propose` and `queue list|show|review|accept|reject`, the accept path through `Admit`, the queue desk pages, FB0820 to FB0825 and FB0917
- Write 10 `ask` fixtures from the 20 cite-check seeds, as `tests/fixtures/state-ask/replies/NN-<slug>.txt` (the pasted reply) with `NN-<slug>.toml` (`seed` the seed id, `exit`, `total`, `unresolved`, `statuses` the status words in reply order, `ai_paste = true`), before `model-handoff-ask` (lead)
- model-handoff-ask: the `handoff@1` adapter and the `ask` verb, with `--via handoff` and `--resume`
- Write ADR 0003, Corpus format v2: the `note` page type and `schema_version` 2 (lead)
- Accept ADR 0003 (owner)
- handoff-new-note: `new note --ai-drafted` with the `ai.paste` event, the `note` page type (needs ADR 0003) and the `note` row of the review queue's `KINDS` (this card follows `review-queue`)
- ai-drill: `cargo xtask ai-drill`, the Day-30 gate drill
- net-stack-spike: the TLS and HTTP crates, `src/io/net.rs`, and the `deny.toml` change
- Record the measured transitive counts and binary size of `net-stack-spike` on its work item (lead)
- model-adapter-openai: the `openai-compatible@1` endpoint adapter, only if the spike passes
- pipeline-model-adapters: the `model@1` classifier and correlator, compiled prompt templates, the Handoff file contract
- stage-command: `cargo xtask stage`, which runs poppler into `staging/` and then `add --propose`
- mcp-server: `fieldbook mcp`, read only
- build-watch: `build --watch`
- assistant-page: `assistant.html`, `assist.js` and `assist-data.js`, the Ask drawer and the entry points: Handoff on a file, and the seam through which `chat-bridge` adds live answers
- decision-package: the decision package workbook, after S2 or labelled "preview"; its builder is two functions, `decision_workbook(..) -> Result<Workbook, XlsxError>` and `save_workbook(&mut Workbook) -> Result<Vec<u8>, XlsxError>`, and its entry calls both, so that `decide-port` can add a sheet between them
- views-and-day-n: the CPY, CPT and PGM views, Day-N and the plan's today line
- app-map: `map.html`
- day30-strict-lints: the quote match and the number-unit lint become errors; proptest
- rust-xlsxwriter-bump: `rust_xlsxwriter` 0.101.x with `zip` 9.0.5, and the `calamine` release that is on `zip` 9
- release-0-2-0: the `v0.2.0` release pull request, with the Day-30 drill for the parts that merged
- changes-page: `changes.html` with `edition-002`

## P5

Exit: see [Phases and exits](../decisions/0001-foundation.md#phases-and-exits), ending Day 90: the Day 60–75 rehearsal and the Day-90 gate; `v0.3.x`.

- ask-promote: asked questions promoted to regression cases or gap questions through `queue review`
- index-output-ports: `IndexPort` and `OutputPort` around the existing code, no change in behaviour (Could)
- decide-port: the Decide port, the `decide` verb, spec files and the AI sheet of the decision package
- chat-bridge: `fieldbook chat` (Cargo feature `bridge`), the loopback Bridge and the chat page
- toolchain-bump-1-100: the Rust pin to 1.100 by the toolchain-bump runbook
- rehearsal-archives: source and vendor archives and the offline rebuild
- rehearsal-two-build-compare: the exe built twice with matching hashes
- full-fm0: the 21 rules and the 13 edge cases
- sbom-and-licences: the SBOM and the licence report
- it-company-story: the IT documents for company data (the policy file, labels, stores, sync, IT-33 through IT-39), docs only
- intake-phase: the Day 60–75 intake request
- Run the trademark check before the R2 intake request (D-3) (owner)
- Confirm ADR 0004 against IT's answers and send the one intake request, on Day 75 (owner)
- Run the PDF-native spike against the committed pdftotext output and record the result in the PDF row of AD-24 (lead)
- Grow the regression set by promotion from the question log, by Day 90 (lead)
- axe-core-checks: no axe-core violations
- mutation-report: the cargo-mutants report on `fieldbook-calc`, and a gate on the label and egress modules of `fieldbook-core` (no surviving mutant)
- ai-integration-doc: `docs/ai-integration.md` as the ports guide, and the SM-03 proof test
- release-0-3-0: the `v0.3.x` release pull request

## P6

Exit: see [Gates](../decisions/0001-foundation.md#gates) (R2), after IT approves: the R2 gate; `v1.0.0` for the knowledge base. Company use starts store by store, as the company's policy lists the labels each store may hold. The IT questions go out at the first IT contact and run in parallel with P1 to P5, so the answers are in hand before P6 starts. Company data is held only after `mutation-report` of P5 has merged with its gate on the label and egress modules.

- r2-state-and-journal: the laptop state folder, `backup` and `restore` over the overlay, state and working folders, Drill 5
- r2-overlay: `init --overlay`, `build --overlay` and the overlay CSV bridge
- Write `tests/golden/search/seed-ranked-company.json`, 4 company-shape ranked cases, from the contract, before `r2-company-ids` (lead)
- r2-company-ids: `store.toml` and `init --overlay --writer`, the id families with six to ten digits, the company index and `--store`, FB0416 and FB0417
- r2-org-intake: `supplied@1` and `overlay-add@1`, with `staged@1` wired for the laptop; company proposals under labels
- Write `tests/golden/sync/{dry-run.txt,first-run.txt,full-run.txt}` for `sync-mini`, from the contract, before `r2-pull-and-sync` (lead)
- r2-pull-and-sync: `systems.toml`, the `Pull` port with `file-drop@1`, `sync` and its proposals, FB0215, FB0216 and FB0830 to FB0834
- Write `tests/golden/records/{dry-run.txt,sync.txt}` and `tests/golden/queue/show-new_record_batch.txt` for `records-mini`, and three `field` cases in `tests/golden/citations/citations-company.json` (a valid `field` citation, one with a column in capitals, one with no column), from the contract, before `r2-records` (lead)
- Confirm the work-object foundation and its three synthetic workflow fixtures before the records port is built (owner)
- r2-records: `types.toml`, record files, the `Register` port with `csv-folder@1`, FB0240, FB0241 and FB0380 to FB0382
- r2-migrate: `migrate`, journaled, and the rehearsal of release checklist item 6
- r2-binskim-fuzz-pe: cargo-auditable, BinSkim, cargo-fuzz (a second target: the Bridge `read_request`) and the PE import check
- r2-sign-workflow: `sign.yml`, Authenticode and the signed catalog
- r2-intake-pack: `cargo xtask intake`, all 17 artifacts
- Rehearse hand-over.md parts A and B on a test laptop before R2, including its empty-profile public-citation/original check (owner)
- release-1-0-0: the ten-item release checklist and `v1.0.0`
- later-effective-as-of: the dated rules library, and the version in force at a past date (group 1)
- later-company-source-read: a company data source read by a web, database or report adapter, one per named system (group 1)
- later-docx-import: Word import for company files (group 1)
- later-pdf-native-ocr: native PDF text, or OCR for scanned originals (group 1)
- later-company-doc-profiles: profiles for company quotes, transactions, bids and estimates (group 1)
- later-semantic-search: a semantic tier after the exact tier (group 1)
- later-feeds: scheduled snapshots of public sources, each a stream of the `Pull` port (group 1)
- later-regulatory-watch: hash and diff of regulation pages, a change sent to the review queue (group 1)

## P7 Price

Exit: set by the owner when P7 starts. P7 follows P6 and the company's approval of the storage. It is the second group of the owner's order: pricing, estimating and the indirect rate book, built on the typed records and the `Register` port of ADR 0004, with the synthetic reference cases first. `core-policy` and the `export` destination have merged before any P7 card starts, because a priced bid is a high-value leak. The recipes "Adding a module", "Adding a calculation function", "Adding a unit" and "Adding an intake target" are already in [docs/cards/README.md](../cards/README.md); a P7 card follows them and adds no recipe of its own.

- Confirm module methods against the accepted work-object foundation before the first P7 card (owner)
- Write the P7 cards from ADR 0005: pricing, estimating, the indirect rate book (lead)
- later-calc-functions: the named functions beyond Day 1, each with a positive and a negative case (group 2)
- later-work-objects: bids, estimates and projects as typed records (group 2)
- later-rate-book: the indirect rate book as dated rate sets (group 2)
- later-quotes: vendor quotes, normalisation of unit basis and currency, comparison of suppliers (group 2)
- later-record-views: the list, record and compare pages for typed records (group 2)
- later-pricing: bid and proposal pricing, computed by the calculation core and drafted by Fieldbook (group 2)
- later-ai-calc-tools: the assistant calls named calculation functions as tools and quotes each run (group 2)

## P8 Complete

Exit: set by the owner when P8 starts. P8 follows P7 in use: estimate at completion and earned value, then the other finance areas by the triggers of the Later table in AD-24.

- Write the P8 cards from ADR 0005 and the AD-24 triggers (lead)
- later-finance-eac-evm: estimate at completion and earned value (group 3)
- later-dependency-checks: complete DEPS command and all process controls, only when its AD-24 trigger is met
- later-lock-replay: complete bounded capture/bootstrap/offline replay, only when its AD-24 trigger is met
- The other rows of group 4 of AD-24, one at a time, by their triggers (lead)

## Any time

- Read the week's merged work (owner)

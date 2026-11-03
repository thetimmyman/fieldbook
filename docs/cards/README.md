# Cards

A card is one unit of work for Codex. This folder holds the open cards of P1 to P3, one file each, named `<slug>.md`; `drafts/` holds the cards of P4 to P6, which move here by one `git mv` when the phase before them ends. [PLAN.md](../plan/PLAN.md) lists the cards in order and says how Codex works through them. The status of each card is on its work item in the owner's tracker, titled by the card's slug. The decision behind this is AD-21 in [ADR 0001](../decisions/0001-foundation.md#ad-21-codex-execution-plan). The pull request template is [.github/pull_request_template.md](../../.github/pull_request_template.md).

Other documents name a card by its slug and a gate by its name: release, status or format. A part of a split card is a card of its own, with its own slug and its own file.

## The template

`just new-card <slug>` (from the `docs-checks` card) copies the text between the markers into `docs/cards/<slug>.md`. Until then, copy it by hand.

<!-- card-template:begin -->
````markdown
# <slug>

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

One or two sentences that say what is true after the merge, in terms a reader can check.

## Why

Links to the ADR sections and reference documents that decide this card.

## Scope

- Files: the paths this card may change, and nothing else.
- Docs to update: each document the change touches.

## Out of scope

- What a reader might expect here but a later card brings.

## Preconditions

- Cards merged: the slugs this card needs.

## Steps

1. Numbered, in order, each small enough to do and check alone.

## Acceptance

```sh
just check
```

Expected: the exit code and the line that shows the command worked.

## Negative control

What to break on purpose, the command that must then fail, and the line that shows it.

## Rollback

How to undo the merge, and what it leaves behind.

## BLOCKED

When to stop, and the `BLOCKED: <what>` comment to leave on the work item.
````
<!-- card-template:end -->

## The three header fields

| Field | Values | Meaning |
|---|---|---|
| Runs where | `home`, `CI Linux`, `CI Windows`, `owner on laptop` (a comma-separated list) | Every place the Acceptance commands run, subject to an explicit platform split in Acceptance. `home` is the development machine; the two `CI` values are GitHub-hosted runners |
| Schema impact | `none`, `additive`, `breaking` | The effect on the [Corpus format](../reference/corpus-format.md#scope-and-versioning). A change after the freeze is the format gate, which needs an accepted ADR 0002 amendment first, so the card is `owner` reviewed |
| Review | `auto`, `owner` | `auto`: Codex merges on a green `ci-ok`. `owner`: Codex opens the pull request and waits. A card is `owner` when it touches a gate: the release gate (tags, releases, signing, CI permissions, secrets, the network rows of `deny.toml`, anything under `crates/fieldbook/src/io/net*`, anything in `crates/fieldbook-win`, the label, policy and egress modules of `fieldbook-core` with their leak matrix, and the IT-facing statements in `docs/intake/` and `SECURITY.md`), the status gate (verifying a consequential claim, accepting a page as `stable`) or the format gate. A card is also `owner` when its Acceptance needs the owner at the laptop, or when it adds a crate past the 15th direct crate of the AI build; its Why says which. The owner reads the week's merged work |

## Writing a card

The lead writes the cards. A one-line item of P4, P5 or P6 in PLAN.md that has no draft becomes a card file when the phase before it ends.

- The title is the slug, and the slug is the file name. Slugs are lowercase words joined by hyphens.
- The ten sections follow the template in order. A section with nothing to say says so in one line; it is never left out.
- One outcome. If the Outcome needs "and" between two observable results, make two cards.
- Why and Scope link documents; they do not copy them. A card never restates a format, a string or a rule that a reference document owns.
- Preconditions name another card only when this card uses what that card builds. Cards in flight share no path in Scope, apart from the registration files named under [Size and splitting](#size-and-splitting), and `docs-checks` fails two cards whose Scope overlaps with no precondition path between them.
- A card that builds anything a reader sees (templates, `assets/app.css`, `app.js`, `search.js`, `assist.js`, strings shown in the UI, page types, inline SVG charts, print) has a "Design:" bullet in Why. It names its [MAP.md](../design/MAP.md) rows by route or component name, the [HANDOFF.md](../design/HANDOFF.md) sections and the token groups it implements. Its Acceptance checks what can be checked without a browser: the CSP meta, no `style=""` attribute, tokens defined once, frozen strings, an empty-state line or "Unavailable" where data is missing. A value no token covers is a gap: use the nearest token and follow "Adding a token".
- Scope names every file or folder the card may change in its `Files:` bullet. A line the card adds to a file that another card owns (a `pub mod` line, a registry row, a workflow job entry) is named in a `Registration lines:` bullet instead, with the file and the line; the scope-overlap lint reads only `Files:`. CI also checks the pull request against both bullets: `cargo xtask scope BASE HEAD` (card `docs-checks`) fails on a changed path that neither bullet names, and listing the path in the card in the same pull request is the way through. Docs to update names each document the change touches, including `AGENTS.md` when a command arrives, `README.md` when a verb arrives, and `CHANGELOG.md` when the program does something new for the owner (a line under `## Unreleased` that starts `- Added:` or `- Changed:`). A hand-edited document of Docs to update (not `CHANGELOG.md`, not a generated file) is named in Files if one card alone owns it, or under Registration lines with the file and line when other cards also extend it (the `AGENTS.md` Commands line, the README quick start line, a controls clause, a corpus-format table row, an intake paragraph, a runbook, `dependencies.md`).
- Four reference files are generated and never written by hand: `docs/reference/ports.md`, `errors.md`, `cli.md` and `audit.md`. Each is a placeholder of three lines, a title and a pair of markers `<!-- generated:ports -->` and `<!-- /generated:ports -->` (the marker name is `ports`, `errors`, `cli` or `audit`). `cargo xtask docs` fills it, and every other table of adapters in `docs/` sits between the same markers. A card that changes what they list runs `cargo xtask docs` and commits the result; `docs_generated_in_sync` fails otherwise.
- Every command in Acceptance exists when the card runs. If a command comes from a later card, the card does not use it. Each command runs from the repository root and can be pasted as written. Expected gives the exit code and the line to look for.
- Use `cargo nextest run --locked -p <crate> <filter>` for tests and name every filter in Steps. A bare word is matched against the name of a test function and its module path, never against a file name, and nextest exits 4 when nothing matches. A filter is therefore one of three forms. `--test <file>` runs the integration test file `tests/<file>.rs`; a name or module path after it narrows the run to the tests of that file that match it. A module path such as `check::pages::` runs the tests of that `mod`, whether the `mod` is in `src` or inside a test file. A bare word is allowed only when it is the whole name of one test, such as `planted_ps1_is_fb0601`. A file stem such as `known_bad` or `audit` is never a bare word. A command that needs two of these forms is two commands, because `--test` and a filter select the tests that match both. Run the program with `cargo run -q --locked -p fieldbook -- <args>`. Pass `--today` to `check` so a run does not depend on the clock. Over the real `corpus/` from the repository root it is `--today "$(git log -1 --format=%cs)"`. Over `tests/fixtures/corpus-mini` or a copy of it it is `--today "$(cat TODAY)"` from the fixture root (`"$(cat tests/fixtures/corpus-mini/TODAY)"` from the repository root). Over a company-store fixture such as `tests/fixtures/records-mini` or a copy of it the date is the one line of `tests/fixtures/COMPANY_TODAY`, `--today "$(cat tests/fixtures/COMPANY_TODAY)"` from the repository root: a store has no place for a file at its root, so the date of its fixtures sits beside them. A sequence that first runs a verb that stamps today's date (`add`, `new`, `queue accept`) passes `--today "$(date -u +%F)"`.
- `xtask` is a binary crate. A file in `xtask/tests/` can run the built program (`env!("CARGO_BIN_EXE_xtask")`) or read files, and it cannot call a function of `xtask/src`. A test of such a function is in `mod tests` of its module and runs as `cargo nextest run --locked -p xtask <module>::`.
- A shell smoke that loads an ordinary temporary policy runs at home and CI Linux. Its Windows equivalent is an integration test through `cargo xtask policy-tests`: create positive policies with `policy_support::write_policy` beneath the provisioned root, outside State and Corpus, and assert Loaded before the behavior. Keep the default-feature `policy_release` run separate from the wrapper’s all-feature run. Pure core tests and real writer-refusal cases remain; deliberate Closed cases and draft-only `policy check FILE` do not assert Loaded. Mark corresponding shell negative controls home/CI Linux too.
- The last Acceptance command is `just check`.
- The Negative control breaks one thing in a temporary copy or on the branch, shows the command failing with the expected line, and restores the file. A control that cannot fail is not a control: if it stops failing, the card is BLOCKED.
- Test rules (where tests live, when each runs, seeded input, state fixtures, timing figures) are in [docs/reference/testing.md](../reference/testing.md). A card does not restate them.
- A test that is the negative control of a dagger row in [docs/controls.md](../controls.md) carries `// control: <ID>` within three lines above its `fn`. The lint `controls_have_tests` (card `docs-checks`) fails a merged card whose row has no such test.
- A timing test prints its measured figure and fails only above ten times the card's target. The target is advisory, because a shared runner can be several times slower than the home machine.
- Mutation job: a surviving mutant fails the job. Add a test that kills it, or a row in `tests/mutants-skip.toml` with a reason and list that path in Files. Do not run mutants in Acceptance, except in `mutation-report`, the card that builds the verb.
- A card that builds a Later row of [AD-24](../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later) names its slug in Why and gives the way back in Rollback. See "Adding a Later item".
- Synthetic data only, marked synthetic. No personal names, host names, tracker keys, ticket ids or dates for Day 1. Fixture policies, label ids, company ids, vendor names and figures are made up and say so; a real policy file or company record never enters a repository.
- A card that reads company data, or adds a place data can leave (an output, a tool reply, an export, a pack, a backup), names in Why the label rule it follows in [ADR 0004](../decisions/0004-company-boundary.md). It takes the `&Allowed` token or a `LabelledView` and has one positive test (a record whose labels the destination allows is present) and one negative test (a record with a label outside the allowed set is absent, counted and not redacted). Every output adds a real-handler row to `SURFACES` in `crates/fieldbook/tests/leak_outputs.rs`, with permitted presence, forbidden absence and a negative plant in the final handler or sink for content the surface emits. A content-free shell instead proves successful public delivery and binding refusal; the card first adding page content or counts adds their filter tests in the same change. `output_surfaces_cover_built_sinks` fails a built sink with no row. Audit export is a text-free special case: preserve every selected journal line, including Closed-state lines, and test both approved file paths and hashes; do not filter journal events by source labels. The core matrix tests decisions separately.

## Recipes for recurring changes

Each recurring change has one recipe here and one drift check that fails when a step is missed. The recurring changes of the company boundary (a label, a destination, a sync adapter, a record type, an id family, an extraction method) are indexed in [ADR 0004](../decisions/0004-company-boundary.md#15-recurring-changes) and have their recipes below. A recipe says what to touch and nothing else. The recurring changes of the calculation core, of intake and of the plan (a calculation function, a unit, an intake target, a module) are in the same index and have their recipes below.

**Recipes kept in another file.** Three recipes sit next to what they change, and this section names them so that one place lists every recipe: [`docs/reference/corpus-format.md#adding-a-version`](../reference/corpus-format.md#adding-a-version) (a source version), [`tests/golden/README.md#adding-cases`](../../tests/golden/README.md#adding-cases) (golden cases) and `crates/fieldbook-calc/tests/cases/README.md#adding-a-case` (a calculation case; the card `calc-cases` writes the file). A fourth heading that starts "Adding" outside this file is a failure of `adding_recipes_are_indexed`.

### Adding a verb

A new CLI verb touches four places, and a test fails when one is missed:

1. [AD-17](../decisions/0001-foundation.md#ad-17-authoring-experience) names the verb and what it does, and the fenced `verbs` block under the table lists its path, one leaf verb per line (a group such as `audit` is not listed). A verb that is not in the AD-17 table needs a superseding ADR (`Review: owner`); a verb that is already in the table leaves the `NOT_BUILT` list of `verbs_match_ad17` in `crates/fieldbook/tests/cli.rs`, and the card names that file under Registration lines (the `NOT_BUILT` entries of its own verbs only).
2. The [Verb map](../reference/corpus-format.md#strings) in the Corpus format gives its chip string, or `NO_CHIP` in `crates/fieldbook/tests/chips.rs` lists it. Register that test file in the card Scope and remove each exact full chip string from `PENDING` when it becomes parseable. A folder placeholder is a path argument even before folder traversal is built. Run the chip suite; restoring a newly parseable entry must fail. Keep still-unparsable flag variants pending. When a chip or flag variant becomes parseable, its producer registers the exact source-chip case in `tests/support/chip_cases.rs`, with its existing synthetic fixture, setup, execution context and specific expected outcome, and lists that path under Registration lines. The ordinary Windows drill runs the Home cases in cmd and Constrained Language Mode; an R2 provider extends the existing provisioned drill5 cases for Laptop success. A documented refusal case asserts its diagnostic and unchanged tree; it does not permit other exit-2 results. No case adds an independent list of built verbs or changes a chip string.
3. The clap `Cli` in `crates/fieldbook/src/cli.rs` defines its flags. A verb that writes a file takes the `Audited` token from fallible `Journal::begin(&lease, &intent, &write_set)` and appends its write events (`io_writes_only_through_atomic` and the compiler fail a write without one). A verb that reads the store and then writes takes the exclusive store lease first, opens the journal, then reads through `World::read_tree_under(&lease)` and plans before `Journal::begin` (or `begin_export` for an outward write). Keep the store lease through commit. Declare its normalized request fields and OperationKey in its card; include every input and behavior-changing option through the journal's versioned length-framed hash. Persist only the hash, original allocations and planning date in the operation intent and journal, never request text there. A confirmed queue acceptance separately retains its final field edits and source choice in labelled proposal recovery state, hash-bound to AcceptanceIntent; review-queue owns that snapshot and its retirement after durable completion. Read pending_intent with its original WriteSet before normal planning: only the matching recovery handler can inspect a pending tree and resume the same paths/hashes; a different verb refuses. Outward writes never mark their input store pending. Collect input and confirmation with neither lease held, then reload and compare the displayed plan under the write lease; a changed plan requires a fresh confirmation. Read-only verbs take the shared lease through their read, using the no-state retry rule of the store contract; journal serialization alone does not make a read coherent.
4. From `ai-kernel` on, put the verb in exactly one of `AI_VERBS` or `DETERMINISTIC_VERBS` in `crates/fieldbook/src/verbs.rs`: a verb that may reach a model goes to `AI_VERBS`; the deterministic set is the Determinism bullet of [AD-01](../decisions/0001-foundation.md#ad-01-architecture). A deterministic verb also gets a row in the `ENTRIES` table of `crates/fieldbook/tests/deterministic_verbs.rs` (the table of `model-gate`), or a name with a one-line reason in its `NO_ENTRY` list (a verb that asks on a terminal or needs a target folder), and the Verb map of step 2 lists every verb. Before `ai-kernel`, `verbs.rs` does not exist: register the verb in the clap `Cli` of step 3 only.

`cargo xtask docs` regenerates `cli.md`. The verb's `CHANGELOG.md` line is the general card rule under "Writing a card", and `AGENTS.md` and `README.md` point to the Verb map, so they need no edit.

Drift check: `verbs_match_ad17` compares the verbs of the clap definition with the lines of the `verbs` block and fails on one that is in only one of them. `every_verb_has_a_chip` fails on a verb with neither a chip row nor a `NO_CHIP` entry. `every_verb_is_in_exactly_one_list` (card `ai-kernel`) fails on a verb that is in neither `AI_VERBS` nor `DETERMINISTIC_VERBS`, or in both. `every_built_deterministic_verb_has_a_row_or_a_reason` (card `model-gate`) fails, and names the verb, on a subcommand of the clap `Cli` that is in `DETERMINISTIC_VERBS` and has neither an `ENTRIES` row nor a `NO_ENTRY` name.

### Adding an adapter to a port

An adapter is one implementation of a port trait: Model, Decide, Index, Output, the intake traits Extractor, Classifier, Correlator and Admit, `BackupTarget`, and the four ports of the company boundary ([ADR 0004](../decisions/0004-company-boundary.md)): `Pull` (lists and fetches items from a company data source), `Register` (reads and appends typed records), `Authz` (who may see or do what) and `Store` (where files live). Ports are traits with a default each; there is no plugin loader, so adding an adapter is a code change. The one list of adapters is `ADAPTERS` in `crates/fieldbook/src/adapters/registry.rs`. Id allocation is not a port: it is the const `FAMILIES` table (see "Adding an id family"). The registry of calculation functions is a const table, `CALC_FNS`, and not a port (see "Adding a calculation function").

1. Pick the port and an id, `<name>@<n>`. A change to the wire format or the output bytes is a new `@n`; the old one stays.
2. Implement the trait where its needs allow: `fieldbook-core` if it only transforms bytes (`src/ports/index.rs`, `src/pipeline/`, `src/backup/target.rs`); `fieldbook-render` for an Output (`src/out/`); `crates/fieldbook/src/adapters/` if it holds a model, a key or writes JSON; `crates/fieldbook/src/io/` if it reads or writes files, and only `io/net*` for the network; `xtask` if it starts a process. The metadata test and the clippy bans fail it anywhere else.
3. Add one row to `ADAPTERS`: port, id, whether it is the default, state (`Built` or `Later`), build (`Silent` or `Ai`), the crate that holds the code, needs (`None`, `Files`, `Process` or `Net`), what it sees (`public` or `labelled`), whether it calls a model, the settings keys it reads, the fixture folder `tests/fixtures/ports/<port>/<id>/`, the port's contract suite and the anchor in `docs/reference/ports.md`.
4. Settings: add each new key to the parser's key table in `settings.rs`. An unknown key is FB0900.
5. Add `contract_<port>_<id>`, a test that runs the port's shared suite on the adapter. Suites: M (Model), D (Decide), I (Index), O (Output), P (Pull), R (Register), S (Store), A (Authz); the intake traits and `BackupTarget` have their own. A suite is the same for every adapter of a port; do not copy its cases. It requires that the same input gives the same output, that random bytes do not panic, that a refusal names the adapter, that a model adapter's suggestion is dropped when its quote or id is not found, and that a backup target writes `LATEST` last and refuses a snapshot folder that is not empty.
6. Set `sees` in the row. `public`: the adapter is only ever given text labelled `public`. `labelled`: it may be given a `LabelledView` or the original of a labelled item. An adapter that sends what it is given to a destination takes the `&Allowed` token with the view; one that only reads it and keeps the result in the same store (an extractor, a classifier, a correlator) takes none and gives its output the labels of its input. Which labels it may receive is not the adapter's to say: the destination row of the company's policy decides, and the owner's list in `settings.toml` can only narrow it ([ADR 0004](../decisions/0004-company-boundary.md), section 7). An adapter row has no label list, because a list in code would be a second place to widen.
7. Put the scripted request and the expected record in the fixture folder. An intake adapter's folder holds `input.*` and `expect.toml`.
8. Run `cargo xtask docs`. It regenerates `docs/reference/ports.md` from the registry. Commit the result.
9. If the adapter adds a journal event field or a proposal kind, follow "Adding an audit event kind" and "Adding a proposal kind".
10. An adapter that uses the network, or that opens a new way for data to leave the machine (a new row of `DEST_KINDS`; see "Adding a destination"), is `Review: owner`, edits `docs/intake/network-silence.md` and `docs/intake/ai-development-statement.md` in the same pull request, and adds its step to `cargo xtask ai-drill`. If it is a `Pull` adapter that needs the network, it also edits the block under "Changing the network-posture sentence".

Drift check: `adapters_match_registry_docs_and_fixtures` fails when a `Built` row has no `contract_` test, fixture or doc line; when an id is in the registry but not in `ports.md`, or the reverse; when a settings key named by a row is not in the parser; when a source file implements a port trait and has no row, or a row has no implementation; when a row names a different crate from the one the file is in; when a `Built` model row has no drill step; or when a source file that takes a `LabelledView` belongs to a row whose `sees` is `public`. Negative control: an `impl ModelAdapter` with no row fails the test.

### Adding or revising a prompt template

1. A template is a text file `crates/fieldbook-core/src/ports/templates/<purpose>@<n>.txt`, compiled in by `include_str!` and listed in the `TEMPLATES` table in `crates/fieldbook-core/src/ports/templates.rs`. Revising text means a new `@n`; the old one stays so that a waiting Handoff ticket can be resumed.
2. Add the purpose, the parts in their fixed order and the output schema (if any) to the table row. A template has no label floor: the labels of a request are those of its parts, and the destination row of the policy decides what may leave. A part's input ids are store-qualified ItemRefs from the current view; retain them in input_refs and audit context_refs before projecting bare context_ids for citation membership. A fresh typed question and fixed rules have no input references.
3. Add a snapshot test: the rendered prompt for a fixture request equals `tests/golden/prompts/<purpose>@<n>.txt`.
4. Run `cargo xtask docs`; the template row appears in `docs/reference/ports.md`.
5. A template contains no example drawn from real data. Fixtures are synthetic and marked.

Drift check: `templates_have_snapshot_and_doc_row` fails when a template id has no snapshot, no row in `ports.md`, or no file. Negative control: editing a template file without the snapshot fails the snapshot test.

### Adding an MCP tool

1. A tool is a row in the `TOOLS` table in `crates/fieldbook/src/adapters/mcp.rs`: name, description, input schema (the in-house JSON-Schema subset), effect, and a handler that takes the read-only core view and returns text.
2. The effect is always `Read`. There is no write effect; a tool that would write is a verb, not a tool.
3. Add the request and expected response to `tests/golden/mcp/<tool>.json`.
4. Run `cargo xtask docs`; the tool appears in `ports.md`.
5. A tool returns text only from a `LabelledView` built from the `Allowed` token that `egress::check` gives for the `mcp` destination: the policy row, intersected with the owner's `[mcp] allow` list. It journals `mcp.call` with `labels` and `policy`; a refused call names no label and no title; `shown N of M` counts permitted hits only. A tool that returns a new kind of unit adds that kind to the leak matrix before it merges, and edits `docs/intake/data-handling.md` if that file does not already list the kind. A tool that opens a new way to reach company data (a write effect, a second transport) is `Review: owner`. In version 1 the destination is `public` only, because a row approves labels and not the program that started the server (`later-mcp-company-data`). Who may read a store is decided before the store is opened (`Authz::allow`, adapter `policy-readers@1`), so a person outside a store's `readers` group gets nothing from it.

Drift check: `mcp_tools_match_docs_and_fixtures` fails when a tool has no golden or no doc row, and the leak matrix (`leak_matrix_covers_every_label`) plants a canary for every label and fails when a tool's reply holds one the `mcp` row does not allow. `mcp_lists_no_write_tool` fails when any tool's effect is not `Read` or its handler can reach a writer. Negative control: a tool with effect `Write` fails the second test.

### Adding a Bridge route

1. Add one row to `ROUTES` in `crates/fieldbook/src/io/net/routes.rs`: method, path, handler, whether it needs the token (every `/api/*` route does), whether it may reach a model (then it goes through `Guarded` and is in the file list of `AI_VERBS`).
2. Add the route to the Bridge table in `docs/reference/ports.md` by running `cargo xtask docs`.
3. Add a test in `crates/fieldbook/tests/bridge.rs` that sends the request with no token, a foreign `Host` and a foreign `Origin` and expects 403 and FB0951 each time, and one test for the success case.
4. A route that writes, or reaches the network, is `Review: owner` and edits `docs/intake/network-silence.md`.

Drift check: `bridge_routes_match_docs_and_tests` fails when a row has no 403 test, no doc line, or when `bridge.rs` or `chat_api.rs` handles a path that is not in `ROUTES`.

### Adding a desk page

1. Add the report struct to `crates/fieldbook-core/src/report.rs` first (see "Adding a JSON report"), with `report_version = 1`.
2. Add one row to `PAGES` in `crates/fieldbook-render/src/desk/mod.rs` and the renderer, which takes the report struct and returns a page with `CSP_STATIC`. A page shows a state the program reads; a change is a `CommandChip` whose string is in the Verb map.
3. Use status-command’s shared binary Desk writer for every PAGES entry and expanded detail route: one coherent read supplies retained labels and current effective input labels; their union must pass current State admission before replacement. FB0412 exits 3 and preserves the affected old file. Missing provenance excludes source content and uses safe Missing/Unavailable diagnostics, never public by default. Queue transit remains destination-admitted; this derived output uses State.
4. Add a golden page to `tests/golden/pages/` and a test that the page has no inline script and no `style=""`.

Drift check: `pages_write_nothing_outside_pages` (`status-command`) fails when a renderer writes a path outside `pages/`, for every row of `PAGES`. Negative control: an inline `<script>` in the frame fails `pages_are_static_and_have_the_csp`.

### Adding a JSON report

1. Add the struct `<Verb>Report` to `crates/fieldbook-core/src/report.rs` (made by `check-command`; the card names it under Registration lines with `json_reports.rs`): `Serialize` only, `report_version` first, snake_case words, and no free-text control field: a diagnostic's `message` and `help` are derived from the catalogue by code, while a report may keep the informational note lines its producer defines (`CheckReport.notes` is the existing case, tested by `pass_notes_survive_json`).
2. Add the golden `tests/golden/json/<verb>.json` (under Files) and one row to the table in `crates/fieldbook/tests/json_reports.rs`. The card's Steps say "Follow Adding a JSON report".
3. Run `cargo xtask docs`; the report appears in the "JSON reports" section of `cli.md`.

Drift check: `json_goldens_match_reports` fails when a report differs from its golden or a golden has no report. Negative control: rename a field in the struct and the test fails.

### Adding an audit event kind

1. Add a variant to `audit::Kind` with typed fields only (`Id`, `RecordRef`, `CorpusPath`, `Sha256`, `Count`, `Role`, `Word`, `Verb`, `StreamKey`, `LabelSet`, `LabelAllow`, `StoreId`, `EndpointName`, `ModelId`). There is no string field. The envelope (`actor`, `store`, `via`) is on every event and is written by `Journal`, not by the kind. The kinds that move content out (`ai.begin`, `ai.pack`, `mcp.call` and `backup.begin`, listed in `Kind::EGRESS`) always carry `dest`, `labels` and `policy`. An export is a `write.begin` that carries `dest` and `policy` as well, and the parser refuses a `dest` on any event other than `refuse` that lacks `labels` or `policy`; a `refuse` may carry a `dest` with `labels` where known, and never carries `policy`. The kinds that exist, with their fields, are listed in [the audit event contract](../reference/audit-contract.md#kinds-and-fields); a kind there that says "envelope only; the raising card adds fields" takes its fields from the card that raises it.
2. Update the hand-written event contract, add it to `Kind::ALL` and add one sample line to `crates/fieldbook-core/tests/audit-samples.jsonl`.
3. Emit it from one place, through `Journal`. A verb is a `&'static str` given to `Journal::begin` or `Journal::refuse`; there is no list to extend.
4. `cargo xtask docs` regenerates `docs/reference/audit.md`, the field table. CI diffs it.
5. A new refusal reason is a word of the closed `reason` list of `refuse` (`audit.md` shows the list) and gets a code in range 08, or in range 09 for a refusal of egress, and its own file, `tests/known-bad/<CODE>.toml` (see "Adding a check code").

Drift check: `every_kind_has_sample_and_no_free_text` fails when a kind in `Kind::ALL` has no sample line that parses, no row in the generated table, or a free-text field. `egress_kinds_carry_dest_labels_policy` fails when a kind in `Kind::EGRESS` has a sample without `dest`, `labels` and `policy`, and when the parser accepts an event other than `refuse` with a `dest` and no `labels` or `policy`. The separate refusal sample checks its optional labels and absent policy.

### Adding a proposal kind

Text-bearing proposal bodies persist their own labels and input ItemRefs and register their backup parser. If acceptance creates a derived item, retain its dependencies after the source question log expires; the accepted proposal and typed queue.accept target ids supply that mapping. The real-handler test relabels an input after acceptance and checks the reopened item and backup.

1. Add a variant to `pipeline::ProposalKind`.
2. Add its `Admit` handler. It returns `AdmitOutcome::Present { id }` or `Planned { files, affected }` from pipeline-ports. Its input includes optional VerifiedExtraction loaded and checked through World for source admission; other kinds take None. The caller adds labels, qualifies affected ItemRefs as audit::RecordRef and builds WriteSet from the planned files and, when it writes a source, orders the original first and the row last. The handler is an `Admit` adapter. Give it an `id()` of the form `<name>@1`, a row in `ADAPTERS` (port Admit, not default, status Built, Silent, Needs::None, sees `labelled`, suite `admit_suite`, crate of the file that holds it), a fixture folder `tests/fixtures/ports/admit/<id>/`, and a test `contract_admit_<name>_1` in the crate that holds it; the drift test fails without them.
3. Add `tests/fixtures/queue/<kind>.toml`.
4. Add the kind to the `queue show` text. No document lists proposal kinds by hand.
5. Name the store the proposal writes to and the labels of what it would write. The `Admit` handler checks store admission and refuses a write that the store's `holds` do not allow (FB0412). The journal envelope supplies `store`; the shared [queue payload](../reference/audit-contract.md#queue-payloads) supplies `labels`, `proposal_kind`, proposal identity and the recorded adapter-role map. Every queue producer uses that typed schema.

Drift check: `every_proposal_kind_accepts_through_one_commit` accepts each fixture and requires a tree that passes `check --strict`, exactly one `write.begin` and `write.end` pair and one `queue.accept`, requires a run without a terminal to be refused (FB0822) for a company store and to land at entry values in the repository Corpus, and requires a fixture whose labels are not all in the store's `holds` to be refused with FB0412 and to write nothing.

### Adding a pipeline rule

Append a `[[rule]]` to `corpus/pipeline/rules.toml` with `when`, `set`, `example` and `expect`, then run `fieldbook check`. Drift check: FB0826 runs every rule on its own example and fails a rule that does not reproduce it.

### Adding a label

A label is data that the company owns. It lives in one place, the `[[label]]` table of the company's `policy.toml`, so this recipe tells the company's side what to write and tells a card what to put in the repository's fixtures.

1. In the company's `policy.toml` (the real file is never in a repository): add `[[label]]` with `id` (an opaque code of two to four letters, a hyphen and four digits, such as `np-0007`; the id goes to the journal and names no counterparty), `title` and, if pages and exports should print one, `banner`. Set `restricted = true` if the label is need-to-know, and name a Windows group in `readers` on each store that holds it (FB0413 otherwise). Then say where the label comes from and where it may go: a `[[marking]]` or `[[source]]` rule that assigns it, the `holds` of each `[[store]]` that may keep it and the `allow` of each `[[destination]]` that may receive it. A destination left out receives nothing with this label.
2. Run `fieldbook policy check FILE`. A label used in a row and not defined, or a new label that reuses a built-in id, is FB0413.
3. In this repository, add the label to the fixture policy `tests/fixtures/policy/policy.toml` only when a test needs it, with one planted record that carries it in `tests/fixtures/leak/records.toml`. `docs/intake/policy.example.toml` gets a row only when it teaches something the file does not.
4. A new built-in label (`public`, `unlabelled`, `marked` and `owner-supplied` are the four) is a change to ADR 0004 and `Review: owner`. This recipe does not add one.

Drift check: `leak_matrix_covers_every_label` fails when a label of the fixture policy has no planted record, and when a planted canary appears at a destination whose row does not allow its label. FB0413 fails a label that is used and not defined. Negative control: add a `[[label]]` row to the fixture policy with no planted record, and the matrix fails naming the label.

### Adding a destination

A destination is a place that bytes can go. The one list of kinds is the const `DEST_KINDS` in `crates/fieldbook-core/src/egress/dest.rs`. A new kind is rare and is `Review: owner`.

1. Add one row to `DEST_KINDS`: the kind, whether its row is fixed or comes from the policy, whether it is advisory or enforced, how a policy row is matched (`name`, `url`, `paths`, `clients` or the kind alone), whether a reply for it shows a left-out count (`shows_left_out_count`: true only where the owner reads the output, such as a pack, an export or a print, and false where a count would show how many restricted records matched) and the one function that sends bytes there.
2. That function takes `&Allowed`. A function that writes a file outside the data root calls `write_outside(&Allowed, ..)` of `io::atomic` and no other writer. Its caller appends the journal event first, with `dest`, `labels` and `policy`. Only `egress::check` makes an `Allowed`.
3. Teach the loader the kind: its keys and rules in `policy.rs`, and its row in the "Destination kinds" table of [policy.md](../reference/policy.md#destination-kinds).
4. Add the kind as a column of the leak matrix (`crates/fieldbook-core/tests/leak_matrix.rs`) and add a known-bad file for each refusal it can raise (FB0901, FB0918).
5. Edit `docs/intake/data-handling.md` and `docs/intake/network-silence.md` in the same pull request, and add a step to `cargo xtask ai-drill` if the kind can reach a model.
6. If the kind has a row in the floor (`Policy::floor(FloorKind)`), give it a phrase in `floor_sentence` (an exhaustive `match`, so the build fails until it has one) and edit the Floor block of the policy reference, IT-33's default cell and `docs/intake/policy.md` to the new sentence.

Drift check: `dest_kinds_match_policy_docs_and_matrix` compares `DEST_KINDS` with the kinds the loader accepts, the rows of `policy.md` and the columns of the matrix; `floor_sentence_matches_the_reference` is available from `core-policy` and compares the floor with the reference. Once `it-company-story` merges, `it_33_and_the_guide_quote_the_floor_sentence` also compares the words IT reads. `io_has_two_writer_doors` fails when `io::atomic` has any public writer besides `write_in_root(RootPath, ..)` and `write_outside(&Allowed, ..)`, so a new writer outside the data root cannot be added without a token and no list of writers is kept. The token is a design aid and not a boundary, so the release scan of the built binary (`cargo xtask binscan`) is the second check: a name for a socket, a process start or the clipboard on neither list fails it, whatever the function's signature is. Negative control: add a third public writer to `io/atomic.rs` and the first test fails; add an import of a network function to the silent build and the scan fails.

### Adding a sync adapter

A sync adapter is a `Pull` adapter. It lists and fetches the items of a company data source and writes nothing. `file-drop@1` is the first. Follow "Adding an adapter to a port" for the port `Pull`, and add what is particular to a source.

1. The port, the planner and the shared types are in `fieldbook-core` (`src/ports/pull.rs`, `src/sync/plan.rs`). An adapter that reads a folder is in `crates/fieldbook/src/io/pull/`, because only `io` touches the disk. An adapter that needs the network (`Needs::Net`) is in `io/net/`, is built only with the feature `net-pull` (added by the first connector card, which also gives the model client `net-model`, so IT can approve a read-only connector and refuse a model, or the reverse), passes `GuardedPull`, has a destination kind of its own (see "Adding a destination"), reads its credential through the `Secrets` port over the Windows credential store and never holds the value in a file, the journal or an error line. It is `Review: owner` and edits the block under "Changing the network-posture sentence".
2. Describe a system and its streams in `tests/fixtures/ports/pull/<id>/systems.toml`. `versioning` and `as_of` are required and have no default.
3. Put in the fixture one file for each planner outcome (`Present`, `NewSource`, `NewVersion`, `Older`, `Duplicate`, `Unmatched`) and the expected report in `expect.toml`.
4. Add the system's `[[source]]` row (its `root`, default labels, path rules, native-label map) to the fixture policy. A system with no row labels everything `unlabelled`, and nothing leaves. A stream whose folder lies outside the row's `root` is refused (FB0835), so a source whose default is `public` cannot be repointed at another folder.
5. Add `contract_pull_<id>`, which runs suite P.

Drift check: `adapters_match_registry_docs_and_fixtures` (row, `ports.md`, fixture, contract test) and `pull_fixtures_cover_every_outcome`, which fails when the fixture of a `Built` Pull adapter has no case for an outcome. Negative control: an adapter whose `fetch` writes into its source fails suite P.

### Adding a record type

A record type is data: one `[[type]]` block in the company store's `types.toml`. It needs no Rust and no lexer change.

1. Add the block: `name`, `revision` (1 for a new type), `title`, scope and optional period columns, optional `search`, `key` and `basis`, and one `[[type.column]]` for each column (`kind`, `required`, and `values` or `to` where the kind needs them). The 15 envelope columns are not repeated. The rules are in [the company store](../reference/company-store.md#typestoml-and-records).
2. In this repository, add the type to `tests/fixtures/records-mini/overlay/types.toml`, a file of valid rows `tests/fixtures/records-mini/overlay/records/<type>.csv` and a file of known-bad cases `tests/fixtures/records-known-bad/<type>.toml` with at least one case for FB0204 (a cell outside its kind) and one for FB0241 (a rule of the record file). A case is a `code`, a `row` to append to the valid file and, optionally, `replace` (the line to replace).
3. To load it from a stream, set `load = "<type>"` on the stream in `systems.toml`. The incoming header must name every required load column. An omitted optional column keeps the current version’s value, or is empty for a new record; the stored revision file always has its exact declared header; the stored revision file always has its exact declared header.
4. A column kind the reader does not have is not a record-type change. It changes the register reader and the Corpus format, so it follows "Adding a column".
5. To add an optional column, retain the old definition and file byte-for-byte, add the next immutable type revision and create `records/<type>@<revision>.csv` with its exact header. New appends use that revision; readers join all revisions. Removing, renaming or changing a column's meaning creates a new type. Follow [Observations and revisions](../reference/financial-work.md#observations-and-revisions). Add a two-revision fixture: old bytes stay equal, both versions remain readable, and new appends use the new header.

Drift check: `record_types_have_fixture_and_known_bad` fails when a type of the fixture `types.toml` has no valid-row file, or no known-bad case for FB0204 and for FB0241, or when a case does not raise exactly its code on the valid file with the row appended. Negative control: add a type to the fixture with no known-bad cases, and the test fails naming it. Rewrite an old revision while adding a column: the two-revision preservation test must fail.

### Adding an id family

An id family is a kind of thing that has an id: a source, a claim, a question, a record. The one list is the const `FAMILIES` in `crates/fieldbook-core/src/ids/families.rs`.

1. Add a row to `FAMILIES`: `name`, `prefixes`, `public_shape` and `example` (none for a company-only family), and `company_prefix`, `company_shape` and `company_example` (none for a public-only family). Who holds an id and how it sorts are columns of the Identifiers table (step 2), not fields of the row.
2. Add the family to the Identifiers table of [the Corpus format](../reference/corpus-format.md#identifiers) with its regular expression and an example.
3. Allocation of a company family reads only the rows of that family and the store's writer, so it needs no code. Add `tests/golden/ids/<family>.json`: the first id, the next, writer 1, a gap, the largest number (an error) and a malformed id.
4. A company-shaped id in a public fixture is FB0416, and a public shape in the company store is FB0417. Add a known-bad case for each.
5. The lexer reads company ids by three generic shapes. A family whose id does not fit them changes the lexer table in `search.md`, which is a card of `search-core-and-cli`.

Drift check: `families_have_shapes_goldens_and_docs` fails when a family has no golden, no row in the Identifiers table, no expression that its example matches, or a prefix the tracker-key exemption does not list (`scan_lists::tracker_exemption_lists_every_family_prefix` of `check-scans`). `lexer_covers_every_family` fails when the example of a family is not found as one id by a lexer shape. Negative control: a family with a three-letter prefix fails `lexer_covers_every_family`.

From company reclassification onward, proposal fixtures may name a synthetic root through the shared test loader in `tests/support/proposal.rs`. Strip harness metadata before production parsing, serialization comparison or copying a hand-written proposal into the queue. The production schema stays strict. Every built kind's shared acceptance case uses a root where that kind can actually write.

## Adding an extraction method

An extraction method turns an original into text and locators. The one list is the append-only const `METHODS` in `crates/fieldbook-core/src/extraction/methods.rs`.

1. Add a row to `METHODS`: the method name, the file types it reads, the locator form it gives (`p`, `rec`, `sheet`, `line`), the `extraction.toml` keys it writes, the id of its extractor (none for `supplied`) and the path of its fixture original. Never edit or delete an existing row. Append one line to `tests/golden/extract/methods.txt` in the same commit.
2. Write the extractor. A core extractor adds no crate. A method that needs a crate follows "Adding a crate, a tool or an action", and it is `Review: owner` when the crate parses a format an outside party controls.
3. Add the method's keys to the `extraction.toml` section of the Corpus format, its locator form to the locator section and, if a check needs one, its row to the Check catalogue (FB0311).
4. Add the fixture the row names, `tests/fixtures/extract/<method>/` unless the method already has one (`pdftotext-layout` uses `xtask/tests/fixtures/`): an original, the expected `extraction.toml` and text files, and a known-bad original.

Drift check: `methods_only_grow` fails when a line of `methods.txt` is missing from `METHODS` or changed. `methods_match_extractors` fails when a row (other than `supplied`) names no extractor registered in `ADAPTERS`, a method-producing extractor appears in no row (staged@1 is a replay adapter and the sole exemption), the fixture path does not exist, or a key is one the Corpus format does not document. Negative control: rename a method in `METHODS`, and `methods_only_grow` fails.

### Adding or changing a backup target

Edit `[[backup.target]]` in `settings.toml` (with a policy loaded, a snapshot that holds any label but `public` also needs a `backup` row of the company's policy whose `paths` contain the target; the owner cannot add that row), then run `fieldbook backup --dry-run`, `backup`, `backup verify` and `restore --snapshot <n> --drill`. If IT's tool copies the data folder, run `backup verify --raw` on its restored copy. On a checkout that carries the public support catalogue, verify its separately delivered digest through the optional raw flags before the first backup; without that provenance, support files stay unlabelled and the floor refuses them. Drift check: the drill itself, FB0812 and FB0813 at `backup`, and FB0814 in `status` when no verified backup is recent.

For a text-bearing Corpus support file, register its exact recognized role and path through the supported-path inputs in `manifest.rs` when its producer lands, and extend the catalogue registration/coverage test. Never classify by suffix, and never add its bytes or current hash to code. The catalogue is generated as data for each packaging run; completed public-write evidence may update its proven hash, and a changed byte outside that write needs public provenance again. A content update refreshes the catalogue without rebuilding or replacing the executable.

### Adding a check code

A new check takes the next free code in its range. It is an additive change (see [Scope and versioning](../reference/corpus-format.md#scope-and-versioning)) unless it rejects something valid before. Do these in one pull request. A missing step fails a named test.

1. Add the row to the Check catalogue in `corpus-format.md` in its range: code, severity, raised-when and one example.
2. Add the code to the `Code` table in `fieldbook-core`.
3. Raise it from one place, in `check/` or in the module of the verb.
4. Add the case that raises it. Ranges 01 to 07: a `[[case]]` in `tests/known-bad/cases.toml`, and delete the code's row in `pending.toml`; a code that `check` over the fixture cannot raise has one `[[elsewhere]]` row in `tests/known-bad/elsewhere.toml` (`code`, `card`, and `test`, the name of the test that raises it). `check-known-bad` writes the row of every code that the catalogue holds when it merges, and the card that owns the code sets the `test` key of that row and adds no row; a code added later gets its row in the pull request that adds it. A code is in one row only. Ranges 08 and 09: `tests/known-bad/<CODE>.toml`, one file per code, with the keys of [Known-bad files](../reference/corpus-format.md#known-bad-files) (`verb`, `args`, `state` and `expect`, or `code` and `in_process` replacing those four fields for an in-process case; `http` is an additional field on a complete invocation for an endpoint case). A case that is the negative control of a security control also names the control in `control`, because a control row must have a negative control.
5. Run `cargo xtask docs`, so that the generated error list and `cli.md` follow.

Drift checks: `every_code_has_a_known_bad_file` (ranges 08 and 09) and `every_code_is_accounted_for` (ranges 01 to 07).

### Adding a crate, a tool or an action

1. Add the row to `docs/reference/dependencies.md`: name, exact pin, licence, "Used for", and the review sentence. For an action, the 40-hex commit; for a tool, a pin in `.config/tools.toml` and a row.
2. Declare a crate once in `[workspace.dependencies]`. A member manifest then adds only a line such as `serde_json = { workspace = true }`.
3. Run the existing `just lock` for a manifest/resolution-input change, review and commit the lock diff, and follow [Review evidence](../reference/dependencies.md#review-evidence): actual committed-lock Cargo builds, both cargo-deny shapes, and exact-source MAX/JEV pin/profile/release-age review. A valid nightly-only `[lock]` stays valid; do not require a new helper or record.
4. A crate past the 15th direct crate of the AI build needs a card whose Review is `owner`.
5. A network crate changes `deny.toml` and is the card `net-stack-spike`; no other card edits `deny.toml`.
6. A tool that `just doctor` checks (a row of `TOOLS` in `xtask/src/tools.rs`) is installed by the `rust` job of `.github/workflows/ci.yml` before the step `Check the tools`, either in the `tool:` line of the `taiki-e/install-action` step or in its own step.

Present checks: committed-lock Cargo and cargo-deny, with required review evidence. Complete DEPS drift controls are deferred to `later-dependency-checks` and bounded replay to `later-lock-replay`; no automated negative is claimed passed.

### Adding a CI job

A job that a docs-only change need not run is gated by the job `changes` (card `check-ci-and-docs`). Do these in one pull request:

1. Add the job to `.github/workflows/ci.yml` with `needs: [changes]` (and its other needs), the pinned actions of [dependencies.md](../reference/dependencies.md), a `timeout-minutes`, and an `if` on every step after checkout that reads `needs.changes.outputs.code` (and `.corpus` when the job depends on the Corpus). Never a job-level `if` and never a path list of its own: a job whose steps skip still finishes with success, and `ci-ok` accepts only success. A job that reads a file of the repository or a git ref starts with the checkout of `ci.yml` (`fetch-depth: 0`, `persist-credentials: false`), and a guard step that decides whether the rest runs comes second.
2. Add the job id to `needs` of `ci-ok`, as a one-line list.
3. Add its row to the table of [AD-08](../decisions/0001-foundation.md#ad-08-ci-design).

Drift check: `ci_ok_needs_every_job` (in `xtask/tests/ci_workflow.rs`) fails naming a job that is missing from `needs` of `ci-ok`.

### Adding a Later item

1. Add one row to the AD-24 table of [ADR 0001](../decisions/0001-foundation.md#ad-24-what-is-not-built-yet-later), in the group of the owner's order that it belongs to: `later-<slug>`, the port or module it extends, the trigger (it ends with "(group N)") and the notes for any candidate adapter. The table has no state column; the tracker item says whether a row is later, scheduled or built.
2. The lead creates one tracker item titled `later-<slug>`.
3. A card that builds the row names its slug in Why and gives the way back in Rollback.
4. When the row is scheduled, the lead adds `- later-<slug>: <one line>` to [PLAN.md](../plan/PLAN.md), in the order of the groups: group 1 under P6, group 2 under P7, groups 3 and 4 under P8.

Drift check: the `later_slug` lint of `docs-checks` fails on a `later-` slug named anywhere in `docs/` that is not a row of the table, on a duplicate row, and on a slug that is not `^later-[a-z0-9-]+$`. CI does not compare the table with the tracker; the lead's step does, and the pull request that adds a row says "tracker item created".

### Adding a regression case

1. Append a `[[case]]` to `corpus/eval/regression.toml`: `id`, `query` and `expect` (ids and rank).
2. Promotion from the question log is an interactive accept (`ask-promote`), which also adds the `from` key (the asked-question id). An agent may append a case by commit; it never edits or removes a case, or sets `critical`.
3. Run `fieldbook check`: the set runs inside `check` (FB0126 to FB0128).

Drift check: `regression_cases_have_ids_and_expectations` and `regression_toml_never_panics_on_random_and_mutated_bytes`, which `search-cli-and-regression` adds.

### Adding a column

A new column goes last, is optional, and says in its "Empty means" cell what an empty value means. That makes it an additive schema change with a card that needs the owner. Do these in one pull request:

1. Append the column to the table of its register in the [Corpus format](../reference/corpus-format.md#registers), never in the middle, and change the column count in the table of registers.
2. Add it to `Register::columns()` and as a field of the row type in `fieldbook-core`.
3. Change `schema_version` and add its migration.
4. Add it to the header of the `registers.xlsx` sheet and of any E1 template that copies the register; both are built from `columns()`.
5. Add it to a fixture row.

A column added after the freeze is an ADR 0002 amendment and the format gate. Drift check: `schema::rows::every_column_survives_from_row_to_cells` takes a row with every column filled, passes it through `from_row` and `to_cells`, and requires the same cells, for every register.

### Adding a page type

A page type is a row of the page types table, a variant in `fieldbook-core` and a template. It is an additive change with a card that needs the owner (see [Scope and versioning](../reference/corpus-format.md#scope-and-versioning)). Do these in one pull request:

1. Add the row to the page types table in [Page files](../reference/corpus-format.md#page-files): the type and its parts in order.
2. Add the variant and its arm of `template(PageType)` in `crates/fieldbook-core/src/schema/page_types.rs`. The `match` is exhaustive, so the variant does not compile without its template.
3. Add the template `corpus/templates/<type>.md` and its copy under `tests/fixtures/corpus-mini/corpus/templates/`.
4. Add the required-content rule (FB0134) and the known-bad case of a missing part (FB0130).
5. Add the render of the type with a golden.

Drift check: `schema::page_types::every_type_is_complete` fails for a variant with no template that parses and names the type, or no row in the table; `every_page_type_has_a_render_golden` fails for a variant with no golden.

### Adding a vocabulary value

A value added to a vocabulary is an additive schema change. Do these in one pull request:

1. Add the value to its row in the vocabulary tables of the [Corpus format](../reference/corpus-format.md#status-vocabularies-and-promotion-rules).
2. Add the variant to the enum and to its `ALL` in `fieldbook-core`.
3. Add the status chip in the reader if the value shows as one.
4. Add a known-bad case if a check uses the value.

Drift check: `schema::vocab::every_enum_matches_the_reference` compares the `ALL` of each vocabulary enum with the values of its row in the tables.

### Adding an IT question

Add a row to `docs/intake/it-questions.md` and the release it gates. Drift check: the `docs-checks` lint `it_count_matches_rows` names every place that states the count.

### Adding a token

A value no token covers is a gap. Use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF section 4. Codex never invents a token. `build-tokens` then copies the new table row into `assets/app.css`.

Drift check: `values_match_handoff` reads every section 4 table of HANDOFF whose header starts "| Token |" and fails on a difference with `app.css`. The token-use lint fails a raw number in a token-only property (`px`, `ms`, `z-index`, font weight and radius) outside the token blocks; the allowed literals are 0, `auto`, `inherit`, `none`, `normal`, percentages and 50% circles.

### Changing the network-posture sentence

The canonical sentence and its three shortened forms are the block below, and every other statement of the posture quotes one of them. A change edits this block and each place that quotes it. Sync from a company data source needs no change today: in the silent build and the AI build alike, `sync` with `file-drop@1` reads a folder, which may be on a company share that the operating system opens, and the program opens no socket for it. A card that adds a `Pull` adapter with `Needs::Net` (Later row `later-company-source-read`) adds its form to this block and to each place that quotes it, and is `Review: owner`.

> **Canonical.** The silent build (the default, and the only build in an edition) opens no socket, starts no process, contains no model and makes no outbound connection. The AI build (feature `net`) adds outbound HTTPS to endpoints that the company's policy lists and the settings name, only on an explicit command and after the audit record is written. With the further feature `bridge` it also holds, only while `fieldbook chat` runs, one listener on 127.0.0.1 behind a per-session token. `fieldbook mcp` uses standard input and output and opens no socket in any build. Neither build starts a process; only the dev-only `xtask` crate does.
>
> **Short.** The silent build opens no socket, starts no process, contains no model and makes no outbound connection. The AI build adds an HTTPS client for configured endpoints and, built with `bridge`, one loopback listener while `fieldbook chat` runs (IT-27, IT-32).
>
> **Edition.** Tier 0 and every edition open no socket and make no request.
>
> **Backup.** In the silent build and the AI build alike, `backup` and `restore` start no process, install no scheduled task and make no network connection.

Drift check: `posture_wording_names_the_build` fails on a sentence of the four-part form (network, socket, listener and model, each with "no"; a sentence that holds one of the words, such as "Exact search needs no model", is not the claim) in `docs/` (except `docs/design/`), `README.md`, `SECURITY.md` or `AGENTS.md` that does not name the silent build or an edition and is not one of the two state labels `No model here` and `No model connected here`, and on a canonical form that has been edited in one place only.

### Adding a drill step or a settings key

A `Built` model, MCP or decide row needs a step in `cargo xtask ai-drill`. The drill runs on the fixture Corpus in a temporary state folder with no real network, prints one PASS or FAIL line per step, and a step whose card has not merged prints "not built" and is never counted as passed. Drift check: `ai_drill_covers_registry`.

A settings key is added to the parser's key table in `settings.rs` and named in its adapter's row, as step 4 of "Adding an adapter to a port" says. A key that meets a row of the company's policy also declares its merge rule in `KEY_TABLE`: a list intersects, a switch is AND, a number is the minimum, a word is the policy's. Drift check: `settings_keys_match_docs`, `settings_keys_have_merge_rules` and FB0900.

### Adding a calculation function

A calculation function is a named, versioned computation of `fieldbook-calc`, such as `build_up@1`. The one list is the const table `CALC_FNS` in `crates/fieldbook-calc/src/fns.rs`. It is a table like `ADAPTERS`, not a port, and there is no plugin loader.

1. Add one row to `CALC_FNS`: the id with its version (`name@N`), the units of its inputs and of its output, its rounding stages and its origin (`computed`). A change to what a function returns for the same inputs is a new `@N`; the old row stays, because a stored `calc_run` cell names the version that computed it.
2. Write the function on `Dec`, `Money` and `Quantity` with the checked operations of `ops.rs` and the stages of `rounding.rs`, and no other rounding. A power or a logarithm uses the decimal series of the crate and states a `tolerance` in its cases; there is no float.
3. Add a positive case, a file `crates/fieldbook-calc/tests/cases/<prefix>-<nn>.toml` (lower case, named by the case id) with `fn = "<id>"` and the expected outputs, and a negative case: a case file with `fn = "<id>"` and `refuse = "<FB05xx>"`, or a `[[budget]]` row that expects `refused`. The recipe is in `crates/fieldbook-calc/tests/cases/README.md`.
4. Compare the case-set binding with the explicit compatibility baseline. If it changed, raise `CALC_EPOCH` above the baseline epoch, then write that epoch and the current digest to `crates/fieldbook-calc/tests/cases/EPOCH`. The guard prints the minimum permitted epoch and current digest; copying a digest without increasing a changed set's epoch fails. A figure that was computed under an earlier epoch is reported as such and is not an error.
5. Persist the canonical run through the record planner with full hash, dimensions, rounding and epoch, and attach field evidence naming its output. A calc_run column is optional display data; every computed field must resolve its evidence run. Run the published compatibility guard and the financial-work fixture for the new function.

Drift check: `calc_fns_have_cases` fails a row with no positive or no negative case, and `calc_epoch_matches_the_case_set` fails a stale `EPOCH` line; `changed_cases_require_a_new_epoch` also rejects a changed case-set binding with no increase over the selected release baseline. Negative control: add a row to `CALC_FNS` with no case, and the first test fails naming it.

### Adding a unit

`Unit` in `crates/fieldbook-calc/src/unit.rs` is a closed enum with a basis family; a conversion exists only inside a family, with an exact factor. Do these in one pull request:

1. Add the variant to `Unit` with its basis family and, when the family has a base, the exact factor to it.
2. Add its word to the `unit` row of the [claim vocabularies](../reference/corpus-format.md#claim-vocabularies). A time word is also added to the word list of the number-unit lint (FB0309).
3. Add a conversion test inside the family and a test that a conversion across families is refused.

Drift check: `unit_words_match_the_reference` compares the enum, the non-currency words of the `unit` vocabulary and the time words of the lint, and fails on a difference.

### Adding an intake target

An intake target says what a proposal of the intake pipeline may write: the rows of `sources.csv`, the rows of a record type, or a dataset. The one list is the const `TARGETS` in `crates/fieldbook-core/src/pipeline/targets.rs` (card `pipeline-ports`). A target is a kind of destination and not a company's type: `sources` is the only row at Day 1, the card `r2-records` adds one row, `records`, that serves every type of `types.toml`, and a dataset target is added by the card that needs one.

1. Add one row to `TARGETS`: the id, the kind (`source`, `record` or `dataset`), the key shapes it accepts and the required columns.
2. A record type is not a target. The row `records` serves every type, so a new type is a block in `types.toml` ("Adding a record type") and adds no row here. A row of kind `record` or `dataset` names the proposal kind that writes it.
3. Add the folder `tests/fixtures/pipeline/<target>/known-bad/` with known-bad proposals for the target, each with a first line `# expect: parse` or `# expect: admit <word>`. Its valid proposals are the queue fixtures of the kinds that write it (`ProposalKind::target()`). A new kind of proposal follows "Adding a proposal kind".

Drift check: `targets_have_known_bad` fails a target that no proposal kind writes, a target with no known-bad proposal, and a known-bad proposal that is not refused in the way its first line names. Negative control: add a row to `TARGETS` with no folder, and the test fails naming it.

### Adding a module

A module is a Later row of AD-24 that a person builds, such as pricing, estimating or the rate book. The seams exist before it: the function table, units, dates and periods, record types, intake targets, the `field` locator, the `calc_run` column, the `show` key and [Financial work](../reference/financial-work.md). A module uses them and adds no path around them.

1. The module has a row in AD-24 whose trigger ends "(group N)". A module with no row first follows "Adding a Later item".
2. Its calculations: one row per function in `CALC_FNS` ("Adding a calculation function") and any new unit ("Adding a unit").
3. Its records: the record types in the company's `types.toml` ("Adding a record type"), with explicit scope and optional period columns, and, for what intake writes, the targets ("Adding an intake target"). The `show` columns of a type decide what the reader's record view shows.
4. Its financial writes use WorkScope and StoreLocks, immutable per-field evidence and durable runs for computed figures. Stream loads retain observation manifests under the existing system/stream/source-version paths. A period, method or approval is an ordinary immutable record version with its normal type definition and named method; issuing retains one `issued_snapshot` row with its `manifest_sha256` and one `snapshots/<manifest_sha256>.toml` carrier, with the exact closure and its verified recovery copy ([Financial work](../reference/financial-work.md#issued-snapshot-carrier-and-canonical-identity)). The module invents no second manifest, lock path, writer, ref kind or executor. Extend the existing binary integration suite `crates/fieldbook/tests/finance_workflows.rs` with a permitted write, a refusal and an identical restore for the module; reuse the shared contracts, never a second lock or evidence path.
5. Its cards, in the order of PLAN.md. Each names the row's slug in Why and gives the way back in Rollback. A card that builds anything a reader sees follows the Design bullet of "Writing a card".
6. Its verbs, if any: follow "Adding a verb". A verb that is not in the AD-17 table needs a superseding ADR first (`Review: owner`).

Drift check: `later_slug` for the slug, `finance_workflows` for scope, evidence and issued snapshots, and the checks of the recipes this one points to.

### Changing a golden or snapshot

1. The card's Files names the file, and its Why names the sentence of the specification that the new value follows.
2. Edit the expected value by hand, from the specification. Never copy the program's output over it. For an `insta` snapshot, run the test, read `<name>.snap.new`, and rename it over `<name>.snap` only when it matches the specification.
3. A `seed-*` case is not changed here; see [tests/golden/README.md](../../tests/golden/README.md).

Drift check: the golden or snapshot test fails on any other value.

### Adding a schema keyword

Add the keyword to the pure schema_json representation and its one registry, emitter and validator. Add boundary, invalid-type and unknown-keyword cases to tests/fixtures/schema-json.json. Both MCP argument and model-reply profiles consume these vectors. `schema_keywords_have_vectors` and caller parity fail drift. Binary JSON parsing stays at the edge; no second validator is added.

## Size and splitting

- About 400 to 600 changed lines. Count added and deleted lines in the pull request, and leave out `Cargo.lock`, generated reference files, fixtures under `tests/fixtures/`, goldens, snapshot files and the deleted card file.
- Split at a seam that keeps `just check` green after each part. The first part keeps the slug of the card in PLAN.md. A later part has its own slug.
- Codex splits when the work passes 600 lines: it stops, moves the unfinished steps into a new card file `<slug>-2.md`, adds `- <slug>-2: <one line>` after the first part in PLAN.md and a work item titled `<slug>-2`, and says so in the pull request. It never pushes a card that is twice its size.
- Cards in flight together share no path in Scope. The registration files in `SHARED_PATHS` of `cards.rs` are the exceptions (the list is under [Shared scope paths](#shared-scope-paths)); a card rebases onto `main` before its last commit and keeps both sides of a conflict there. The lead sets how many cards run at once; start at four and raise it while the queue is deep.

A split is one atomic graph change: assign each provided interface and control test to exactly one part; keep every enabled handler's required authorization, invariants and negative tests in its first shipping part, moving a late definition forward when needed; redirect every downstream Cards merged dependency to the part that now provides it; update PLAN, scopes, registrations and the owner's tracker together. Consumers are eligible only after all their required providers merge. `split_preserves_prerequisite_providers` tests a two-part fixture with a consumer of part two and fails if part one alone releases it. No completed status is stored in the repository.

## Dependencies and the lock file

A dependency or a lock refresh is not a gate. It is `auto` only with its row plus the actual committed-lock Cargo/cargo-deny and exact-source MAX/JEV evidence in [Review evidence](../reference/dependencies.md#review-evidence). Security/cooldown and licence exceptions retain owner review. A crate past the 15th direct crate of the AI build needs a card whose Review is `owner`. The steps are in "Adding a crate, a tool or an action".

## Closing a card

1. The Acceptance commands pass on the branch head, and the Negative control was seen failing.
2. The documents named in Docs to update are changed or regenerated. Before deleting a completed card, move any enduring contract into a reference and redirect every inbound document link to it. Card names in precondition history may stay as plain code. `cargo xtask cards --check --closing <slug>` validates a temporary document tree with that card absent, including link, scope, control ownership and dependency checks. For a card closed before `docs-checks`, manually review the proposed tree without the card: enduring contracts, inbound links, scope, control ownership and prerequisites; record the result in the pull request. The `docs-checks` card itself, and every card closed after it merges, must pass `cargo xtask cards --check --closing <slug>` before the final deletion commit. The last commit deletes the card only after the applicable review or check passes.
3. Codex waits for a green `ci-ok` on that head. A card with `Review: auto` is merged when it is green. A card with `Review: owner` waits for the owner. The owner reads the week's merged work in the weekly review; nothing waits for it.
4. After the merge, Codex sets the card's work item Done with the pull request linked. Nothing else is recorded.
5. PLAN.md does not change, its line stays, and the card file does not come back. Corrections go in a new card.

## BLOCKED

A card is blocked when a precondition is missing, an Acceptance command cannot pass, a Negative control stops failing, or a stop-and-ask case in [PLAN.md](../plan/PLAN.md#stop-and-ask) applies. Codex sets the card's work item to Blocked with a comment `BLOCKED: <what>`, pushes the branch, and takes up the next card whose paths are disjoint. The owner clears the block by answering on the work item or in the pull request, or by changing the card.

## Published compatibility

The strict manifest, baseline descriptor, validated consumer types and historical-vector dispatch are defined once in [compatibility.md](../reference/compatibility.md). `compat/published.toml` binds each released adapter id, prompt/template version and calculation function version to its per-version contract files and historical input/output vectors by full hashes. `cargo xtask compat --base FILE` compares those bindings against the archived/base release: old rows, files and expected outputs cannot be rewritten to justify changed behavior. Adding a new version is allowed; the old version still passes its historical vectors and remains resolvable for stored tickets and runs. A deliberate removal needs an owner migration card. Before a first release the baseline is empty, never silently substituted when a configured baseline is unavailable. Every adapter, prompt, schema keyword and calculation recipe adds its version/vector row and runs this check.

Before historical comparison, the guard binds the published full case-set digest to recomputed current cases and its epoch to compiled CALC_EPOCH. Archived calculation inputs use ReplayCase’s pinned epoch through current version dispatch; ordinary evaluation uses current CALC_EPOCH. Both compare complete canonical bytes.

## Shared scope paths

Add the scope-overlap lint to `cards.rs`. For each card file and draft, read the paths of the `Files:` bullet of Scope (a `Registration lines:` bullet is not read, as the cards README says): literal repository-relative file paths, directory prefixes ending `/`, or brace alternatives such as `{a.rs,b.rs}` expanded before comparison. Wildcards (`*`, `?`, character classes) and placeholders (`<family>`) are unsupported and fail with file and line. A directory covers descendants by whole path components only; equality covers a file. This grammar also governs changed-path scope checking; both use the same parser. Broader golden directories never grant permission to edit seed goldens. Two cards overlap when a path of one equals, contains or lies under a path of the other. The pair passes when one card reaches the other through the slugs under `Cards merged` in Preconditions, followed any number of steps, or when every shared path is in the `SHARED_PATHS` constant of `cards.rs`: `docs/plan/PLAN.md`, `CHANGELOG.md`, `Cargo.lock`, `justfile`, `xtask/Cargo.toml`, `xtask/src/main.rs`, every `lib.rs`, every `check/mod.rs`, `crates/fieldbook/src/{cli.rs,io/mod.rs}`, `crates/fieldbook/tests/chips.rs`, `crates/fieldbook-render/assets/{app.css,app.js}`, `tests/known-bad/{cases.toml,pending.toml,elsewhere.toml}`, `.github/workflows/scheduled.yml` and `.github/workflows/ci.yml`. Each is a registration file that many cards extend by a line, so git merges the lines; a card rebases onto `main` before its last commit. Otherwise it prints both file names and the shared path.

Version bindings name dedicated per-version files, hashed whole as the compatibility manifest specifies, never shared registry files or movable sections. Adding a new row or version must leave the old binding unchanged. `adding_a_version_does_not_change_old_bindings` tests this seam.

Edition-only transfer follows the single [update recipe](../runbooks/transfer.md#edition-update-recipe): on the laptop, copy a retained originals archive under the new filename only after its hash matches the new out-of-band list, before checking the pair. Keep the rollback pair unchanged; otherwise transfer both. Generated guides and tests cover matching reuse and mismatched refusal.

Extraction extensions are additive: follow the existing extraction-method recipe, append the method and any locator row, declare its metadata key schema and supply original/locator round-trip fixtures. `methods_only_grow` rejects changed old meanings and accepts appended rows; an older reader must report the extension unavailable, never verified. Reserved structured-path and email-part meanings and all supplied-text routes live in [Additive input formats](../reference/corpus-format.md#additive-input-formats).

Every content WriteSet supplies sorted distinct affected `audit::RecordRef` values for the records it changes. Both write events retain them, so audit ID filtering finds direct and queued writes. Store-level configuration and derived artifacts without a record subject may use an empty list; no filenames or content enter these event fields.

After release acceptance, an owner-reviewed follow-up advances the compatibility binding in release-baseline.toml and the accepted stream’s tool or edition binding, preserving the other stream, to the newly accepted verified archive before another implementation card merges. The tagged descriptor stays unchanged. The release lifecycle tests prove ordinary PR checks then protect newly published bindings, including an implementation and golden changed together.

Admit extraction input is based on whether the kind admits a source, not its target table. Source-bearing record batches follow the same verified staging and post-confirmation recheck as source proposals; kinds with no source pass None. Preserve both files and affected in every Planned outcome, including empty-file decisions.

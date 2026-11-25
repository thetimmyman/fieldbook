# Fieldbook

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="crates/fieldbook-render/assets/fieldbook-logo-dark.svg">
  <img src="crates/fieldbook-render/assets/fieldbook-logo.svg" width="225" height="64" alt="Fieldbook">
</picture>

Fieldbook is a cited finance library plus one small Rust program. The default (silent) build is offline, and so is every edition; the optional AI build can call a model endpoint that the company's policy lists and the owner configures, through one audited gate. The library (the Corpus) holds pages, tables and byte-exact public documents, and every consequential statement traces to a public source. The program checks the Corpus, renders it as static pages and Excel-safe tables, and writes each edition as two zip files.

It is the job tool of one person, an incoming Finance Director at a US defense-munitions contractor, and it is meant to become a company application. The owner reads and prints from it, adds to it at home, and carries it to the company laptop as plain files. Once installed there, the program holds company data in company storage, on the company network, as an extension of company infrastructure. Everything in this repository is public information or synthetic. Company data never enters this repository or an edition: code and public content go to the laptop by the approved channel, and company data sources sync into company storage through ports ([ADR 0004](docs/decisions/0004-company-boundary.md)).

## Bootstrap

```text
cargo install --locked just@1.58.0
just setup
```

`just setup` installs the other pinned tools and enables the git hooks. `just doctor` checks them.

## Quick start

**The owner, reading.** Open `edition/index.html` from an edition in Edge or Chrome. No server or install is needed. Run `just site` or `fieldbook build --out out` to build `out/edition` locally; `just edition` arrives with the card `edition-zips`. The first is `edition-001`.

**The owner, authoring at home.** Run `fieldbook add <file> --confirm-public` to add a public source, `fieldbook new page --confirm-public <area>-<slug> --type <type> --subarea <subarea>` to start a page from its template, fill it in, then run `fieldbook fmt`, `fieldbook check` and `fieldbook build`. Cite a source with `fieldbook new claim --confirm-public --area <a> --source <version-id> --at "<locator>" --quote "<text>"`. Find an id or a word with `fieldbook search <query>`; a source id such as `S-0002` lists every version. Build a context pack for a question with `fieldbook context "<question>"`. The owner verifies a claim at a terminal with `fieldbook verify <claim-id> --on YYYY-MM-DD`. Find the next thing to do with `fieldbook status`; it lists what needs work and the commands to copy, and `fieldbook status --page` writes the desk pages into the state folder. The page rules are in [corpus/README.md](corpus/README.md).

**A developer or an agent.** Read [AGENTS.md](AGENTS.md), then run `just check`. Pick the next card from [PLAN.md](docs/plan/PLAN.md).

## Where things live

| Path | Holds |
|---|---|
| `corpus/` | The Corpus: pages, registers, public originals, templates |
| `crates/` | The program: the crates of [AD-05](docs/decisions/0001-foundation.md#ad-05-crate-layout-and-enforcement) |
| `xtask/` | Development automation; never shipped |
| `docs/decisions/` | [ADR 0001](docs/decisions/0001-foundation.md) (architecture), [ADR 0002](docs/decisions/0002-corpus-format-v1.md) (Corpus format) and [ADR 0004](docs/decisions/0004-company-boundary.md) (the company boundary) and [ADR 0005](docs/decisions/0005-work-objects.md) (financial work, accepted for the build package), [ADR 0006](docs/decisions/0006-edition-search-index.md) (the edition search index), [ADR 0007](docs/decisions/0007-hosted-edition.md) (the hosted read-only site) |
| `docs/reference/` | [Corpus format](docs/reference/corpus-format.md), [search](docs/reference/search.md), [dependencies](docs/reference/dependencies.md), [policy file](docs/reference/policy.md), [company store](docs/reference/company-store.md), [testing](docs/reference/testing.md), [financial work](docs/reference/financial-work.md), [audit contract](docs/reference/audit-contract.md), [citation check](docs/reference/citation-check.md), [compatibility](docs/reference/compatibility.md) |
| `docs/runbooks/` | [Release](docs/runbooks/release.md), [transfer](docs/runbooks/transfer.md), [toolchain bump](docs/runbooks/toolchain-bump.md), [restore drill](docs/runbooks/restore-drill.md), [hand-over](docs/runbooks/hand-over.md) |
| `docs/intake/` | What corporate IT security reads: [index](docs/intake/README.md) |
| `docs/controls.md` | Each control, its evidence and its negative control |
| `docs/design/` | The interface design the build follows |
| `docs/plan/PLAN.md` | The cards in order, by phase; status is kept in the owner's tracker |

Security reports and policy: [SECURITY.md](SECURITY.md). Tool releases: [CHANGELOG.md](CHANGELOG.md).

## Releases and tiers

| Release | Tags | When | Tier | What reaches the laptop |
|---|---|---|---|---|
| R1 | `edition-001`, `v0.1.0` | Day 1 | Home only | Nothing yet; read and printed at home |
| E1 | An edition | When IT approves a transfer channel | Tier 0 | Two zips of files: pages, workbooks, public originals |
| Day 30 | `v0.2.0` | Day 30 | Tier 0 | Same files, plus the Assistant page and views; Handoff runs at home |
| Day 90 | `v0.3.x` | Day 90 | Tier 0 | Same |
| R2 | `v1.0.0` | After IT approves | Tier 1 | The signed `fieldbook.exe`, installed by IT |
| R3 | Not set | After R2 | Tier 1 | Modules in the owner's order: pricing, estimating and the indirect rate book, then estimate at completion and earned value, then the rest |

Tier 2 is an installer, only if IT requires one. The AI build is the same source built with the Cargo feature `net` (and `bridge` for the chat listener), from `v0.2.0`. It is not part of any edition, and on the laptop it needs IT's answers to IT-27 to IT-33 (the policy file is IT-33). Editions and tool releases are separate streams: an edition never changes the program ([release runbook](docs/runbooks/release.md)).

## Words we use

| Word | Meaning |
|---|---|
| Corpus | The library: pages, registers, sources and templates under `corpus/` |
| Toolkit | The `fieldbook` program and its checks; the signed Toolkit is R2 |
| edition | One build of the Corpus as two zips, tagged `edition-NNN` |
| transfer statement | `TRANSFER-STATEMENT.txt` in each edition: what the files are and how to check them |
| intake pack | The documents corporate IT security reads; Tier 0 has a subset, R2 has all 17 |
| day-one binder | The `brief` pages plus the stage 1 and 2 pages, printed |
| context pack | The excerpts of the Corpus built for one question, to paste into an approved assistant |
| Handoff | Using an approved assistant by copy and paste: Fieldbook builds the context pack and checks the answer, and never calls a model. It is also the default route of `ask` |
| silent build | The default build: no network client, no model. `v0.1.0` is a silent build. An edition holds no program |
| AI build | The same source with Cargo feature `net` (and `bridge` for the chat listener): one HTTPS client for an endpoint IT approves, behind the audit gate, and, while `fieldbook chat` runs, one loopback listener |
| label | A tag from the company's table on a page, a source version or a record: an opaque code such as `np-0007`. `public`, `unlabelled`, `marked` and `owner-supplied` are built in, and `owner-supplied` is never kept on a record. A destination takes a request only if every label it carries is in that destination's allowed set |
| store | A root of the data layout with its own rules: `corpus` (public only), `overlay` (the company store), `working` and `state` |
| policy | The company's file `policy.toml`: the label table, what each store may hold and where each label may go. Fieldbook reads it and never writes it |
| tier | What reaches the laptop: Tier 0 files, Tier 1 the signed program, Tier 2 an installer |
| overlay | The company store on the laptop, from R2: the folder `overlay\` of the data root, which holds company pages, originals, extracted text and records under the labels the policy lets it hold ([company store](docs/reference/company-store.md)) |
| data root | The folder that holds every store and the state: `%LOCALAPPDATA%\Fieldbook\` on the laptop, and `.fieldbook/` beside the checkout at home |
| page | One Markdown file in the Corpus, with TOML front matter and fixed parts |
| brief | A one-page page type: question, recommendation, evidence, options, unknowns, next action |
| register | One of five CSV tables: sources, claims, questions, conflicts, glossary |
| source version | One edition of a public document, such as `S-0042-v2`; originals are kept byte for byte |
| claim | One sentence a source version supports, at a locator, with a short quote; cited as `[@CL-CMP-0011]` |
| consequential claim | A claim a decision rests on; it carries a quote and the owner's check |
| locator | The place in a source version: a page, section, sheet range or record |
| verified | Checked by the owner against the original; the program never raises it |
| synthetic | Invented for an example; marked as such, never taken from real data |
| Day N | A day counted from the owner's first day; no calendar date is stored |
| card | One unit of work for Codex, in `docs/cards/`, with an outcome and a negative control |
| Codex | The coding agent that executes cards |
| seed golden | A reference test file, added in a pull request of its own; only the lead or owner changes one once merged |
| negative control | A deliberate breakage that shows a check can fail |
| `ci-ok` | The one required CI check; it passes only when every job does |
| the lead, the owner, IT, the UI lane | The author of the foundation and content; the Finance Director who owns every judgment; corporate IT security; the owner of layout and visual design |

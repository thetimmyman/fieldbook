# ADR 0003 Corpus format v2

Status: Proposed. The owner accepts it at the format gate. Until then nothing in this record is in force, and no reference document is edited to match it.

## Context

[ADR 0002](0002-corpus-format-v1.md) froze version 1 with nine page types and said that `note` "joins at Day 30 and makes the version 2" (AD-09). Its reopen trigger fires: content needs something the format cannot say, a saved assistant answer ("a saved answer is a note", [HANDOFF.md](../design/HANDOFF.md) line 108). The reference already describes part of the type ([Object types beyond v1](../reference/corpus-format.md#object-types-beyond-v1), `origin` and the FB0708 and FB0709 rows), the controls HG-02 and HG-06 and the card `handoff-new-note` rely on it, and the cards disagree with the reference in places (see Reconciled). This record decides the type and the version in one place, so that the card builds what the owner accepted. It amends AD-09 only; AD-10 is unchanged.

## Decision

**A note is the saved text of an assistant answer, kept in the repository Corpus as an unreviewed draft. It is never a source and never evidence.** Everything below follows from that sentence.

| Topic | Decision |
|---|---|
| What it is | One page with `type = "note"` whose body is an AI-drafted answer, with the question it answered, the ids of the context it was given and a fingerprint of that context. Provenance is `origin = "ai_drafted"`, the only value in version 2. A note written by hand is an ordinary page, not a note; a later `origin` value is an additive vocabulary change. |
| What it is not | Not a source, a claim, a conflict, an answer in a register or a curriculum page. It has no parts, no `Sources` part and no stage or reading path. `stage = 8` and `readiness = "PUBLIC-NOW"` are inert fixed values that keep the page schema one shape; a reader leaves notes out of every stage and prerequisite list. Its computed `review_by` and FB0117 do not apply: a note carries `reviewed` instead. |
| Never supports a claim | A claim row whose `source_id` is a note id is FB0204. A page body that cites `[@<area>-note-<nnn>]` is FB0300 (the form is answer-only, [corpus-format.md](../reference/corpus-format.md#syntax-at-a-glance)). No page names a note in `prereq`, `links`, `supersedes` or `superseded_by` (FB0710). Nothing a note says raises a status, fills a cell or puts a figure on a page. Reviewing it changes none of this. |
| Cited in an answer | Only in the tolerant, answer-only form. `cite-check` gives it `weak` whenever it is visible, reviewed or not, and keeps its normal precedence: absent is `broken`, outside a supplied context `outside_context`. |
| Id and place | `<area>-note-<nnn>`, three digits from 001 to 999, the next free number of that area, never reused. The file is `corpus/pages/<area-slug>/<subarea>/<id>.md` like any page, in the repository Corpus only. The prefix `<area>-note-` with three digits is reserved: a page of another type with such an id, or a note with another id form, is FB0105. A company store holds no note page; its saved answers are records of a type in its `types.toml` ([ADR 0004](0004-company-boundary.md)). A note in a company store tree is FB0710. |
| Front matter | The page fields `id`, `title`, `area`, `subarea`, `type`, `stage`, `readiness`, `status`, `as_of` and `handling`, then `origin`, `reviewed`, `question`, `context_fingerprint` and `inputs = [{ store, id }]`. All five note fields are required and belong to no other type (FB0102 elsewhere). `terms`, `prereq`, `questions`, `conflicts`, `links`, `volatility`, `supersedes` and `superseded_by` are not allowed on a note. `handling` is `["public"]` (FB0410); `inputs` is sorted by store and id and may be empty only for a confirmed public answer with no context. A missing or malformed note field, a forbidden field, or a `status` other than `draft` is FB0709 (the code now also covers `inputs`, which the card and reference name in different places). |
| Body | Free text after the front matter, with no required parts (FB0134 and FB0130 skip it). Every other body rule holds: scans FB0400 to FB0403, no raw HTML, no outside link, canonical citations of claims and source versions, and each citation resolves. `as_of` is not later than the oldest claim cited (FB0118, strict). |
| Status values | `status` is always `draft`: the page maturity words `reviewed` and `stable` would say the same as `reviewed` below, or claim an acceptance a note cannot have. `reviewed` is `false` when written. The label is "AI-drafted · unreviewed" while false and "AI-drafted · reviewed" after (`ai_note_label`, already in the frozen strings). A person, the owner or the lead, sets `reviewed = true`; the verb, an agent and `queue accept` never do. A commit that raises it under an agent author fails `guard-statuses` like a raise of `verification` (GS-02). An edit of the body, question, `inputs` or fingerprint of a reviewed note sets `reviewed = false` in the same commit. |
| `schema_version = 2` | Version 2 is version 1 plus the `note` page type, its five fields, FB0710 and the reading of `type = "note"`. It is an additive change ([Scope and versioning](../reference/corpus-format.md#scope-and-versioning)): nothing valid in version 1 changes meaning, and no existing byte is rewritten except the one stamp. A Corpus needs no note to be at version 2. |
| Reading and migrating | A tool built for version 2 reads version 1 and version 2 (`SCHEMA_VERSION_MAX = 2`; the first tool for version 2 is `0.2.0`). Every v1 page, register, original and citation stays valid unchanged, and a v1 Corpus needs no migration to be read, searched, built or checked. The Corpus moves to 2 only by the one-off stamp, `cargo xtask migrate-1-to-2`, which changes `schema_version` in `corpus/corpus.toml` and nothing else, and runs once at home before the first note; the shipped `fieldbook migrate` from R2 inherits the step. A company store is stamped and migrated on its own; its stamp never has to equal the Corpus's. A tool built only for version 1 refuses a version 2 Corpus (FB0100). |
| `check` on mixed or older input | The version is one stamp per Corpus; no file carries its own. A `note` page under `schema_version = 1` is FB0108, and `new note` there writes nothing and says to set the stamp. A version 2 Corpus with no notes is clean. A stamp of `0` or above the tool's maximum is FB0100 as today. FB0108 becomes schema-gated (today it fires for any unknown `type`). |
| Codes | Reserved: **FB0710** (error), a note outside its place: in a company store tree, named by another page in a graph field, or a non-note page with a note-shaped id (the last is FB0105). FB0709 widens as above; FB0708 stays the scan refusal at the verb, for the repository Corpus only. FB0100 to FB0709 keep their meaning. |
| Search | Unchanged. A note is indexed as a page unit and section units (`search.md` already indexes every type), no ranking change, `status` shown is `draft`. Notes count toward the edition index budget of ADR 0006. The reader shows the label chip beside every note result. |
| Edition | A note ships as a page in the edition, with its label, its question and its body, and in no stage list. The edition's `schema_version` is copied from the Corpus and `min_tool_version` from the table in `version.rs`. |
| Context packs | HG-02 stands: a note is left out unless the question names its id or `--include notes` is given; when included it is marked as an AI-drafted note and brings no closure of its own. |
| Cite-check | A note is a `weak` fact of kind `note` (HG-05, FB0704 "the cited text is a note"). A note's own citations are checked as in any page and count for no claim's use. |
| Labels, boundary | A Corpus note is public: the `public` label, confirmed by the person at the verb or at `queue accept`, with every input public. Nonpublic or unresolved inputs are refused, never relabelled ([ADR 0004](0004-company-boundary.md): a note does not become public by citing public claims). No label table, policy or egress rule changes. |
| Left out | A hand-written note; a note in a company store (a record type); a note as a source, a claim or a status input; threads (still session-only); editing tools beyond ordinary file edits; any `origin` other than `ai_drafted`; a ranking, filter or query form for notes; the `inputs` field on other types. |

## Consequences

**What this gives.** The saved answer has one defined home, one id family and one set of refusals, and `handoff-new-note`, `review-queue`, `handoff-cite-check` and `assistant-page` can each be built against the same text.

**What it costs.** It follows "Adding a page type" in [docs/cards/README.md](../cards/README.md#adding-a-page-type): a row in the page types table, `PageType::Note` (the enum and its count of 9 go to 10) with its `template()` arm, the template and its fixture copy, and a render with a golden; the part rules are skipped, so FB0130 and FB0134 gain no rule. Beyond the recipe: `SCHEMA_VERSION_MAX` and the `0.2.0` row in `version.rs` (its test asserts 1), schema-gated FB0108, the note check pass (FB0709, FB0710), the `guard-statuses` rule, the one-off `migrate-1-to-2` with a fixture tree, and the context class. Migration cost is one stamp line; there is no rewrite and no re-verification. Rollback is reverting the stamp while the Corpus holds no note; once a note exists the owner deletes the notes first, because a version 1 tool refuses version 2.

**Documents and cards that change when this is accepted.**
- Reference: [corpus-format.md](../reference/corpus-format.md) (Scope and versioning, the `schema_version` text, Page types and front matter with `inputs`, Layout of corpus/ templates, Identifiers for the note id, FB0108, FB0708, FB0709 and FB0710, Object types beyond v1, Verb map); [errors.md](../reference/errors.md) and [cli.md](../reference/cli.md) by `cargo xtask docs`; [search.md](../reference/search.md) (the line "arrives at Day 30"); [citation-check.md](../reference/citation-check.md) if it states the weak predicate.
- Decisions and controls: ADR 0002 (a pointer that version 2 amends AD-09), ADR 0001 (AD-17 rows for `new note` without `--confirm-public`, the weak predicate near line 566, the documents table line 634), [controls.md](../controls.md) (HG-02, HG-06, AL-04, GS-02, row 10), `AGENTS.md` (Where things live and the certify stop for `reviewed`), `corpus/AGENTS.md`.
- Design: [HANDOFF.md](../design/HANDOFF.md) (the laptop note line) and [MAP.md](../design/MAP.md) (the `note` render and stage lists).
- Cards: `handoff-new-note` (the fields, `inputs`, FB0710, the status rule, the guard, the company-store refusal, `migrate-1-to-2`), `review-queue` (the `note` row), `handoff-cite-check`, `assistant-page`, `model-handoff-ask`, `transfer-and-windows-drill`, `r2-migrate`. PLAN.md already holds the owner's acceptance step before `handoff-new-note`.

## Reconciled

| Where | Says | Against | Decision |
|---|---|---|---|
| `corpus-format.md` 1437 (FB0708) | In a company store a note takes the store's default labels | `handoff-new-note.md` 31 (a company-store note is a record, never reaches the Corpus); [HANDOFF.md](../design/HANDOFF.md) 1250 ("Laptop notes are ORG and never go home", an older label word) | No note page in a company store; FB0708 is for the repository Corpus; the record type uses ADR 0004 labels. |
| `corpus-format.md` 387 to 401 and 1438 | Note fields are `origin`, `reviewed`, `question`, `context_fingerprint` | `corpus-format.md` 1639 and `handoff-new-note.md` 57 (`inputs` required); `handoff-new-note.md` 43 and 45 (`NoteMeta` and FB0709 without `inputs`) | Five fields; FB0709 covers all. |
| `decisions/0001-foundation.md` 167 and 481 | `new note --ai-drafted --area <a> --from <file>` | `corpus-format.md` 207 and `handoff-new-note.md` 9 (`--confirm-public` required) | `--confirm-public` is required; ADR 0001 is corrected at acceptance. |
| `handoff-new-note.md` 42 | `check` refuses a note under schema 1 with FB0108 | `check/pages.rs` 81 (FB0108 is raised for any unknown `type` regardless of schema) | FB0108 is gated by the stamp. |
| `corpus-format.md` 401 | `origin` is "note only" and `status` is the page maturity (`reviewed` or `stable` allowed) | `handoff-new-note.md` 43 (`status = "draft"`, `reviewed` boolean) | Two words for one fact are refused: a note's `status` is `draft`. |

Chosen without a settled source in the repository (the owner confirms these by accepting this record):

- Notes carry no `terms`, `prereq`, `questions`, `conflicts`, `links`, `volatility`, `supersedes` or `superseded_by`; no `review_by` and no FB0117; no stage or reading-path role.
- FB0710 and what it covers; the reserved `<area>-note-` id prefix and its 999 limit per area.
- `status = draft` always, and `reviewed` reset on edit with the `guard-statuses` rule.
- A version 2 Corpus may hold no note, and a company store's stamp is independent.
- A note brings no closure into a context pack, and its citations count for no claim's use.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Keep notes outside the Corpus (state folder or a register) | They would be unsearchable and not in the edition; the owner needs to read, review and cite an answer's question and context in one place, and the state folder is never committed. |
| A note as a register (`notes.csv`) | A new register is a breaking column set for long text, loses Markdown and the scans' file model, and Excel round trips damage it. |
| Hold the type back until R2 and the migrate verb | The saved answer is the Day-30 Handoff outcome; a one-line stamp is cheaper than a second release path. |
| Bump to 2 now and require every Corpus to move | Forces a stamp on a Corpus that holds no note; the tool reads both and the stamp can wait for the first note. |
| Let a reviewed note be citable or become `stable` | Review of an answer is not verification of its claims; GS-02 and HG-06 keep a person's certification on claims, not on text from a model. |
| A hand-written note (`origin = "owner"`) | It is a page; a second origin adds a vocabulary value with no trigger. |
| One id space with ordinary pages | A reader cannot tell a note from a page by id, and `check` could not refuse a note-shaped page. |
| A note in a company store as a page | Labels and ids there are the record store's; ADR 0004 already gives it records. |
| Let a note carry optional page fields | They open a path into prerequisite graphs, volatility and supersession, all of which assume reviewed content. |

## Reopen trigger

A change to this format is a new ADR that amends or supersedes this one and raises `schema_version`. Any of these fires it: a second note `origin` is needed, the same workaround for notes appears in three content pull requests, a company store needs a note page rather than a record, or a dependency change leaves the format unreadable.

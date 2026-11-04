# core-types-and-strings-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-core` holds the complete frozen strings. The preceding `core-types-and-strings` card supplies hex/version only; it does not make this provider available. `strings::js()` returns the text of `assets/strings.js`. `cargo xtask docs` regenerates the three tables of the Strings section and, on its first run, changes nothing. It also fills every `<!-- generated:NAME -->` pair that has a registered generator. `just check` runs `cargo xtask docs --check` as step 8.

## Why

- The strings are one source with two outputs, Rust and `window.FB_STRINGS` ([AD-10](../decisions/0001-foundation.md#ad-10-identifiers-citations-and-frozen-strings)).
- The text and the three tables are in [Strings](../reference/corpus-format.md#strings). The generated documents are in [AD-22](../decisions/0001-foundation.md#ad-22-agentsmd-and-the-docs-set).
- A change to a frozen string is a schema change and goes through the format gate, so this card mirrors the text; it does not edit it.
- Design: the [MAP.md](../design/MAP.md) row Frozen strings (`fieldbook-core::strings` and `strings.js`). The verb map's chip strings live here; `build-about` and the verb cards render them. [HANDOFF.md](../design/HANDOFF.md) §0 Frozen strings and §12 Implementation notes (Strings). Token groups: none; the strings carry no styling.

## Scope

- Files: `crates/fieldbook-core/src/strings.rs`, `xtask/src/{docs.rs,literals.rs,graph.rs,deps_budget.rs,deps_graph.rs,deps_metadata.rs}`, `crates/fieldbook-core/src/snapshots/` (the `insta` snapshot of `strings::js`), `justfile`, `crates/fieldbook/src/lib.rs`.
- Registration lines: `crates/fieldbook-core/src/lib.rs` (strings module export only); `xtask/src/main.rs` (module, usage and docs-command dispatch only); `AGENTS.md` (the `cargo xtask docs` line of Commands).
- Docs to update: [corpus-format.md](../reference/corpus-format.md) (between `<!-- strings:begin -->` and `<!-- strings:end -->`, which must not change), `AGENTS.md` (the `cargo xtask docs` command), and the `justfile` header comment.

## Out of scope

- Hexadecimal rendering and schema-version lookup (the preceding primitive provider).

- The generators of `errors.md`, `cli.md`, `audit.md` and `ports.md`: each is one row of `GENERATORS`, added by `check-v0`, `check-ci-and-docs`, `audit-journal` and `ai-kernel`.
- Writing `assets/strings.js` to disk (`build-v0`) and the chip parse test (`build-about`).
- Any change to the wording of a string.

## Preconditions

- Cards merged: `core-types-and-strings` (hex/version primitives) and `day1-dependencies-3` (including xtask’s path dependency on fieldbook-core and the snapshot dependency).

## Steps

1. Add `strings` with the three tables as data in the order of the document, continuation rows included: the key table, the verb map (`intent`, `chip`, `from` as an enum `Day1`, `Day30`, `Day90`, `R2` (`Day90` prints exactly `Day 90`)) and the other fixed wording (`key`, `wording`, `decision`); `assistant_labels` also has `strings::ASSISTANT_LABELS`, eight (id, text) pairs in the order of the row (`ask`, `send`, `stop`, `copy_pack`, `check_citations`, `save_note`, `copy_draft_note`, `this_session_only`), so that the render crate writes them into the Assistant page; `ai_note_label` has `strings::AI_NOTE_LABELS`, two texts ("AI-drafted · unreviewed" first, "AI-drafted · reviewed" second), which the `note` render reads. Give each of the 23 string keys (the 9 of the key table and the 14 of Other fixed wording) a typed accessor that returns the string without its surrounding quotes; `chips` holds the verb map. `binary` is `pub const fn binary() -> &'static str`, so a const table can use the accessor.
2. Add `strings::js()`: the text of `assets/strings.js`, one assignment to `window.FB_STRINGS` with all 24 keys in the order of the document: the 9 of the key table, `chips`, and the 14 of Other fixed wording. Escape strings with a small function of your own (core has no JSON crate) and write `</` as `<\/`. Pin the output with an `insta` snapshot, and add the test `strings_js_has_every_key_of_the_document`, which reads the key cells of the key table and of Other fixed wording from `corpus-format.md` with `include_str!`, adds `chips`, and requires them to equal the keys in `strings::js()`.
3. Add `cargo xtask docs [--check]`. `xtask/src/docs.rs` holds `GENERATORS`, a table of `Generator { file, name, begin, end, render }`, and `fill(text, begin, end, body)`, which replaces the text between the marker lines `begin` and `end` and leaves all other text alone. The row of a generated document has `<!-- generated:NAME -->` and `<!-- /generated:NAME -->`; the row of `strings` has `<!-- strings:begin -->` and `<!-- strings:end -->`. This card registers the first generator, `strings`: it replaces each of the three tables between `<!-- strings:begin -->` and `<!-- strings:end -->` with the table printed from `strings`, finding a table by its header row. Later cards add a row for their own file (`check-v0`, `errors.md`; `check-ci-and-docs`, `cli.md`; `audit-journal`, `audit.md`; `ai-kernel`, `ports.md`) and no other code in `docs.rs`. A generated file is a title and the two empty markers until its generator exists. `--check` writes nothing, exits 1 and names the first differing line when a file would change, and otherwise prints `docs: up to date`. A missing file or marker pair is exit 2 and names it. Tests: `fill_replaces_only_between_markers`, `check_names_the_first_differing_line`, `missing_marker_is_exit_2`.
4. Before enabling the lint, remove the local `UNRELEASED` constant in `crates/fieldbook/src/lib.rs`; use `fieldbook_core::strings::unreleased_build()` for its fallback and both version-test expectations. Preserve version behavior and the frozen wording; the core dependency already exists. Replace the six frozen `"fieldbook"` literals in `xtask/src/graph.rs` (in `SHIPPED`, `MEMBERS`, the three `GOOD` rows and the extra-edge test) with `fieldbook_core::strings::binary()` before enabling the lint, so neither an identifier nor a test retypes the string. Add the literal test in `xtask`: it reads every `.rs` and `.js` file under `crates/` and `xtask/` except `crates/fieldbook-core/src/strings.rs`, snapshots and goldens, and fails when one holds a frozen phrase or heading between double quotes. It names the file and the key. For `assistant_labels` the test checks the five labels of more than one word; the one-word labels `Ask`, `Send` and `Stop` are common words, and the golden of `assistant.html` pins them.
5. In `justfile`, replace the step 8 comment with `cargo xtask docs --check`, and correct the header comment.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core strings::
cargo nextest run --locked -p xtask docs::
cargo nextest run --locked -p xtask literals::
cargo xtask docs --check
just check
```

Expected: every test passes, and the `strings::js` snapshot holds one assignment to `window.FB_STRINGS`, all 24 keys in the order of the document, and no `</`; the literal test passes, so no `.rs` or `.js` file retypes a frozen string; `docs --check` exits 0 and prints `docs: up to date`; `git status --short docs/` is empty after `cargo xtask docs`; `just check` exits 0.

## Negative control

Change `Day 1` to `Day 2` in one chip row of the verb map in `corpus-format.md`. `cargo xtask docs --check` must exit 1 and name that line. Restore the file. Then add `const X: &str = "Not established.";` to `crates/fieldbook-render/src/lib.rs`: the literal test must fail and name the file and the key `empty_part`. Remove the line.

Restore one `graph.rs` literal in a `GOOD` row as `"fieldbook"`: the literal test must fail and name `graph.rs` and `binary`. Restore the accessor call.

Change the `decide` row's release tag from `Day90` to `Day30`: `cargo xtask docs --check` must fail and identify that row. Restore it.

## Rollback

Revert the merge commit. Step 8 of `just check` returns to a comment and the document is unchanged.

## BLOCKED

The first `cargo xtask docs` run changes the document. The data and the text disagree. Do not edit the document to fit: write the first differing line and stop.

# page-context-packs

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

Each `p/<page-id>.html` carries its page context pack in a `<details><pre>`, readable and copyable with scripts blocked. The pack is plain text: `ASSISTANT.md` first, then one block per item with its id, title, as-of date, text with locators and a `cite as:` line. Items are the page, the claims it cites with their quotes, and its top 8 related units from the search query function, with their closure, all from a `LabelledView` built for the `edition` destination, whose set is `public`. A selected claim keeps its conflict and the opposing claim: they are in the pack together or left out together. The same inputs give the same bytes.

## Why

- Owner review covers this card’s egress view and leak-matrix changes.

- The pack builder and `LabelledView` (the view of [ADR 0004](../decisions/0004-company-boundary.md#5-egress), built for one destination from an `Allowed` token) are in [AD-01](../decisions/0001-foundation.md#ad-01-architecture) (One core module, Adapters) and [AD-18](../decisions/0001-foundation.md#ad-18-security-and-confidentiality-guardrails) (Handoff design). Day 30's `context` reuses this builder and its tests.
- The page part is in [AD-12](../decisions/0001-foundation.md#ad-12-site-generation-and-page-types) (`p/<page-id>.html`, a Could). The form of a `cite as:` line is the citation grammar in [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance); "Left out (budget)" is the frozen `left_out_budget` string.
- Controls: HG-01 (the page part) and the selection and budget of HG-02, in [controls.md](../controls.md). The closure keeps a selected claim's conflict and opponent, as AD-18 (Handoff design) and HG-02 say. NFR-D-12 is in [Requirements referenced](../decisions/0001-foundation.md#requirements-referenced).
- The label rule: a record whose labels are outside the destination's set is left out whole, never redacted, and announced in the last line of the pack: with a count for a destination the owner reads, and with the fixed line `Some records were omitted.` for a model, a tool client and the edition, because a count tells a reader how many restricted records matched. The failure it prevents is a pack that reads as complete when records were dropped. The same builder serves Handoff and the tool server in later cards; this card uses it for the edition, where the set is `public`.
- This is a Could and the first to cut ([Cut line and floor](../decisions/0001-foundation.md#cut-line-and-floor)).
- Design: the [MAP.md](../design/MAP.md) row Page context pack (`<details><pre>`). [HANDOFF.md](../design/HANDOFF.md) §9 Context pack (Selection, Excluded by default, Format). Token groups: type (`--font-mono`), colour (`--color-surface-2`).

## Scope

- Files: `crates/fieldbook-core/src/assist/{mod.rs,pack.rs}`, `crates/fieldbook-core/tests/pack.rs`, `crates/fieldbook-render/templates/parts/context_pack.html`, `crates/fieldbook-render/templates/page.html` and `crates/fieldbook-render/templates/types/` (where the part is included), `crates/fieldbook-render/src/{page.rs,site.rs,lib.rs}` (`site.rs` and `lib.rs` pass the build's `&LabelledView` down to `page.rs`), `crates/fieldbook-render/tests/pack_markup.rs`, `crates/fieldbook-render/assets/app.css` (the pack rules only).
- Registration lines: `crates/fieldbook-core/src/lib.rs` (the assist module line); two rows in `OUTPUT_PATHS` of `crates/fieldbook-core/tests/leak_matrix.rs` (`page_pack_edition` and `page_pack_handoff`, step 8); `crates/fieldbook-core/src/egress/view.rs` (the read accessors for kept pages, source versions, the tree and the glossary that `assist::index_of` uses, this card's entries only); `core-policy` made both files; `crates/fieldbook/tests/leak_outputs.rs` (page packs surface rows and their negative plants, this card's entries only).
- Docs to update: none. No command arrives; `CHANGELOG.md` gets `- Added: a context pack on each page.`

## Out of scope

- `fieldbook context`, the Assistant page and `assist-data.js` (Day 30).
- A copy button: the pack is selectable text. Ticking excluded items back in (Day 30).
- `cite_check`.

## Preconditions

- Cards merged: `search-ranking-and-index`, `build-v0` (the edition `LabelledView` that `render` takes), `build-page`, `build-page-types`, `build-registers` (both cards edit `site.rs`), `core-policy`, `audit-journal` (`Now`).
- A pack sits on every page, the page types included, and both cards edit `page.rs`.

## Steps

1. `LabelledView::new(&Model, &Allowed, &ReadableStores) -> LabelledView` (from `core-policy`, `egress/view.rs`): keeps only the pages and source versions whose every label is in the token's allowed set, and the rows that point only at kept ones. A record outside the set is left out whole, never redacted, and `left_out()` counts it. The edition build selects the public view and passes it to `render(&view, &info)` before binding the final output Delivery. This card makes no token and reads no clock; pages pass that view to `assist`. Nothing in `assist` reads a `Model` directly.
2. `assist::page_pack(view, &dest, page_id, budget) -> ContextPack`: the page's own block, its cited claims, then the top 8 units from `search::query` on the page title (the page's own units left out), then the closure of AD-18, de-duplicated in that order. The closure adds, for each selected claim, its conflict row (`conflict_id`) and every claim in that row's `claim_ids`, marked contradicted even though §9 excludes contradicted claims by default. Leave out the rest of what §9 excludes by default. The index is not an argument: `assist::index_of(view: &LabelledView) -> Index` builds it from the pages and source versions that `view` kept, with the unit builder of `search-ranking-and-index` step 1 (a `keep` that accepts any label set, because the view has already dropped the rest) over `shipped_corpus(tree, quarantined)` (the tree and the glossary are read through the view, and `quarantined` is the ids of the kept source versions whose `lifecycle` is `quarantined`), then `Index::build(units, glossary)`. Nothing the view dropped is indexed or ranked, and a held (quarantined) version gives no unit, so no pack holds its text.
3. Blocks are whole. A selected claim and its conflict dependencies form one unit: the unit is kept or dropped together, never in part. Stop adding at `[assistant] pack_budget_chars` from `corpus.toml`, and list each block left out, a dropped unit's blocks included, under the `left_out_budget` line.
4. `ContextPack::to_text(&self) -> String`: `ASSISTANT.md` verbatim, a blank line, the line `Labels: <ids>` (the frozen string `pack_labels`: the union of the labels of the included items together with the labels of the `ASSISTANT.md` preamble block the pack opens with, sorted and joined by a comma and a space. The preamble is a real included constituent: `corpus/ASSISTANT.md` is a Corpus file and the `corpus` store rule of `core-labels` holds `public` only, so a page pack of public items prints `Labels: public`, and an item-empty pack prints it only because that included public preamble is one of its constituents. That is known input provenance and no empty-to-public conversion: a fold with no label at all is `unlabelled`, as `LabelSet::from_ids` gives it, and an absent or missing classification is never turned into `public`), a blank line, then the blocks in order, then the `left_out_budget` list of step 3 when a block was dropped, and last the stored `left_out_line`. The pack takes it from `view.left_out_line(&dest)` when it is built (`core-policy`): `Left out (policy): N records` (the frozen string `left_out_policy`) for `handoff`, `export` and `print`, and the fixed line `Some records were omitted.`, with no digit, for `model`, `mcp`, `bridge`, `clipboard` and `edition`. An edition page therefore ends with the fixed line, so that one builder serves every route and no pack shows how many records a reader may not see. The view's allowed set is the selection filter only: it decides which items are included and is never printed as if it were the pack's labels. The labels of an included input are printed as they are: `unlabelled` and `marked` among them stay in the line beside every other label, as `LabelSet::union` keeps them, and no reader infers a permission set from the line or a label from the permission set. LF only, one final newline, no time stamp.
5. `page.rs` and `parts/context_pack.html`: a `<details class="context-pack"><summary>` with the frozen words "context pack", and the text in `<pre>`, escaped by askama. No script, no `style=""`. `templates/page.html` and each file of `templates/types/` include the part. Tests in `crates/fieldbook-render/tests/pack_markup.rs`: `pack_markup_is_escaped` (a page whose claim quote holds `<script>x</script>` and `&` renders them as `&lt;script&gt;x&lt;/script&gt;` and `&amp;` inside the `<pre>`, and the page holds no `<script` from the quote) and `pack_has_no_script` (the pack part of a rendered page holds no `<script`, no `on...=` attribute and no `style=`).
6. `app.css`: `pre` in the pack uses `var(--font-mono)` on `var(--color-surface-2)`; long lines wrap. A value no token covers is a gap: use the nearest token, record the gap on the card's work item, and the UI lane adds the token to HANDOFF.
7. `tests/pack.rs`, on the fixture plus an in-memory page and source version whose `handling` is `["np-0007"]` and which carry a synthetic marker string (the page has the title of the page under test, so that an index built from the whole Model would rank it in the top 8), over the policy `tests/fixtures/policy/policy.toml` and `Dest::Handoff` (the `edition` row is fixed and public-only, so it cannot allow `np-0007`; the token is `egress::check(&eff, &Dest::Handoff, None, None, now, shipped)` and the test narrows its view with `restrict(&permissions)` with a fixed `Now` written out in the test: the text of `tests/fixtures/corpus-mini/TODAY` and `43_200` seconds): with `permissions` equal to `public` the marker appears in no pack and the pack ends `Left out (policy): 2 records` (`a_marker_outside_the_allowed_set_never_reaches_a_pack`); with `permissions` equal to `public` and `np-0007` the marker appears and the line says 0 (`a_record_inside_the_allowed_set_is_present`); `ASSISTANT.md` comes first; every claim block has a `cite as:` line; a small budget drops whole blocks and lists them; two runs give equal text; `a_selected_claim_brings_its_conflict_and_opponent` (the opposing claim is `contradicted`, so excluded by default, and still appears with the conflict row); `over_budget_drops_claim_with_its_conflict` (a budget that fits the claim but not its conflict leaves out the claim, the conflict row and the opponent, and lists all three after the last block and before the last line); `a_model_pack_ends_with_the_fixed_line_and_no_digit` (the same records built for `Dest::Model` and for `Dest::Edition`: the last line is `Some records were omitted.` with 0 and with 3 records left out, and no digit appears in it; built for `Dest::Handoff` it is the count line); `held_version_not_in_a_pack` (a temporary copy of the fixture with one source version's `lifecycle` set to `quarantined` and its text set to the title of the page under test followed by a marker string that only that text holds, so that an index read from the raw tree, without `shipped_corpus`, would rank it first: the marker appears in no pack built for `Dest::Model`, `Dest::Edition` or `Dest::Handoff`).
8. Add two rows to `OUTPUT_PATHS` in `crates/fieldbook-core/tests/leak_matrix.rs`, `page_pack_edition` (kind `edition`) and `page_pack_handoff` (kind `handoff`), each a function from a view to `assist::page_pack(view, &dest, <the fixture page>, <the default budget>).to_text()`, so that the matrix finds a canary in a pack only where `egress::check` allows its label.
9. Mark the tests that carry a control row of [the controls table](../controls.md#handoff-and-ai) with the line `// control: <ID>` directly above the test, as `controls_have_tests` reads it: `// control: HG-01` on `a_model_pack_ends_with_the_fixed_line_and_no_digit`.

10. Follow the output recipe for page packs: register its real handler and final sink in `SURFACES`, exercise permitted and forbidden canaries, and check destination/payload binding. Persisted output metadata keeps the input ItemRefs needed by `labels::effective_item`; reopening after source relabelling re-evaluates them. Run `output_surfaces_cover_built_sinks` and `real_outputs_keep_permitted_canaries_and_refuse_others`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test pack
cargo nextest run --locked -p fieldbook-render --test pack_markup
cargo nextest run --locked -p fieldbook-core --test leak_matrix
just site
grep -L '<details class="context-pack">' out/edition/p/*.html
grep -c 'style="' out/edition/p/*.html | grep -v ':0$'
cargo nextest run --locked -p fieldbook --test leak_outputs
just check
```

Expected: the tests pass; `just site` exits 0; both greps print nothing, so every page has a pack and none has a `style="` attribute; `just check` exits 0, with FB0604 and the determinism test still passing.

## Negative control

Make `LabelledView::new` keep every page whatever its labels. The test `a_marker_outside_the_allowed_set_never_reaches_a_pack` must fail and print the marker's page id. Restore it. Then make the view drop the record without counting it: the `Left out (policy): 2 records` assertion must fail. Restore it. Then make the closure skip the conflict row: `a_selected_claim_brings_its_conflict_and_opponent` must fail. Restore it. Then make `left_out_line` print the number for `Dest::Model`: `a_model_pack_ends_with_the_fixed_line_and_no_digit` must fail. Restore it. Then build the index from the whole Model in place of `index_of(view)`: `a_marker_outside_the_allowed_set_never_reaches_a_pack` must fail. Restore it. Then pass an empty `quarantined` list to `shipped_corpus` in `index_of`: `held_version_not_in_a_pack` must fail. Restore it.

Bypass the label filter at the page packs handler: `real_outputs_keep_permitted_canaries_and_refuse_others` must fail. Restore it.

## Rollback

Revert the merge commit. Pages lose the pack part; the rest of the page is unchanged.

## BLOCKED

The packs push a page over a budget in FB0605 or the edition over the index cap, or the query for related units needs a rule no document gives. Write the page and the number, and stop.

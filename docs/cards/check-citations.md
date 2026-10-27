# check-citations

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check` reads every citation in the page bodies with one lexer in `fieldbook-core` and raises FB0300 to FB0303 and FB0305. All 30 seed citation goldens pass in Rust, and each page's resolved citations are kept in `Model` for the later rules and for `build`.

## Why

- The grammar, the locator kinds, "strict writer, tolerant reader" and the `][@` trap are in [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance). The codes are in [03: citations](../reference/corpus-format.md#03-citations). The decision is [AD-10](../decisions/0001-foundation.md#ad-10-identifiers-citations-and-frozen-strings).
- The vector format, the loader rules and what a citation vector pins are in [tests/golden/README.md](../../tests/golden/README.md#citation). A failing seed case is a stop-and-ask case in [PLAN.md](../plan/PLAN.md#stop-and-ask).
- Design: the [MAP.md](../design/MAP.md#routes-and-components) rows `#page-<id>` (CitationMarker, SourcesBlock) and CitationChip, and [HANDOFF.md §5 Inline token kinds](../design/HANDOFF.md#inline-token-kinds). This card renders nothing. It supplies the parsed and resolved citations that `build-page` and `build-registers` draw. No token group.

## Scope

- Files: `crates/fieldbook-core/src/cite/{mod.rs,lexer.rs,resolve.rs}`, `crates/fieldbook-core/src/check/{mod.rs,model.rs,page_body.rs,citations.rs}`, `crates/fieldbook-core/src/lib.rs` (the module line only), `crates/fieldbook-core/tests/{golden_citations.rs,cite_no_panic.rs}`, `tests/golden/citations/citations-strict.json` (new), `tests/known-bad/{cases.toml,pending.toml}`, `CHANGELOG.md`, and `tests/fixtures/corpus-mini/` only to fix a fault a new rule finds.
- Docs to update: `CHANGELOG.md` (`- Added: check reads every citation in a page body and reports unreadable, uncanonical, touching and unresolved citations, and locators on claim keys (FB0300 to FB0303 and FB0305).`). `errors.md` does not change; the codes already exist there.

## Out of scope

- FB0304 and FB0308 (`quote-match-and-anchors`). FB0306 and FB0307 (`check-claim-cells`).
- FB0117, FB0118, FB0309, FB0310 and FB0320 (`check-citation-rules`).
- The JavaScript port and `cite-check` (Day 30). The lexer reads both modes now, because the seeds pin both.
- Drawing citations (`build-page`, `build-registers`).

## Preconditions

- Cards merged: `check-page-body`.

## Steps

1. Write `cite::lexer`. One function reads a text in `Mode::Strict` or `Mode::Tolerant`. It returns `LexOutcome { citations: Vec<Citation>, findings: Vec<LexFinding> }`: citations (key, key kind, locator kind and text, span) and all findings (code and span), each in reading order. After a malformed attempt, resume at the next citation opener; skip code spans and fenced code. Valid citations after a fault survive. Use a hand-written matcher, as [dependencies.md](../reference/dependencies.md) requires; no regex.
   - Strict follows the EBNF exactly. Tolerant accepts only the repairs listed under "Strict writer, tolerant reader", marks each repaired citation `non_canonical` and returns the canonical text.
   - A bracket pair is a citation attempt when it is empty, holds `@`, or starts with a key shape. Other brackets are prose.
2. Write `cite::resolve`. It looks each key up in the registers the model already holds: a version in `sources.csv`, a claim, a question or a conflict. An unknown key is FB0303 at the citation's line and column.
3. Write the pass in `check/citations.rs`. It reads the precomputed citation slice and saved findings from `Model`, populated by `check::load`, and emits diagnostics with `path:line:col` without reparsing. Add the pass to the list in `check/mod.rs`.
4. `check::load` computes `Model::citations(&self, page: &PageId) -> &[Citation]` after pages, parts and registers are loaded and before passes run. The slice holds resolved citations in reading order. Load also keeps lexer and resolution findings with their page paths and spans in the private Model state, including attempts that yielded no Citation. The pass reads those saved findings and the slice, and raises FB0300 to FB0303 and FB0305 without reparsing or dropping failed attempts. Add `cite::claims_of(page)`, the sorted claim ids cited once each, the computed `claims` field of [Page files](../reference/corpus-format.md#page-files). Replace the key-presence approximation in `check/page_body.rs` with the tolerant citation lexer, preserving each rule's requested key kinds: claim for a clause-card value, claim or source version for brief Evidence, and question for brief Unknowns. This checks presence, not resolution; citation faults stay with this pass. Keep the existing body codes and tests. Test `malformed_and_touching_attempts_survive_load` in `check::citations` (both errors survive even when the resolved slice is empty), `citations_are_loaded_in_reading_order` in `cite::resolve` and `body_uses_the_requested_key_kind` in `check::page_body`, including each accepted kind, a wrong kind and a tolerant repair.
5. Add `tests/golden/citations/citations-strict.json` (`kind` citation, `version` 1): the strict reading of repairs that the tolerant reader accepts as non-canonical, which [Syntax at a glance](../reference/corpus-format.md#syntax-at-a-glance) settles as FB0301. Case `cit-001`: the input of `cit-s-024` (a missing comma), strict, FB0301. Case `cit-002`: the input of `cit-s-026`, strict, FB0301. Follow [Adding cases](../../tests/golden/README.md#adding-cases).
6. Write `crates/fieldbook-core/tests/golden_citations.rs` with its tests in `mod golden_citations`. Include each JSON file with `include_str!`. The loader uses serde with `deny_unknown_fields` and rejects an unknown `kind` or `version`; `note` is read and ignored. Tests: `seed_cases_pass` (all 30, reporting every failing id), `other_cases_pass`, `loader_rejects_unknown_fields` (inline JSON).
7. Move FB0300, FB0301, FB0302, FB0303 and FB0305 from `pending.toml` to `cases.toml`. Plant each in the fixture's concept page with the catalogue's known-bad example; FB0303 cites a claim id the fixture does not hold.
8. Write `crates/fieldbook-core/tests/cite_no_panic.rs` with `random_and_mutated_bytes_never_panic`. It includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 96 from `Rng::new(1)`, and takes the valid citation `[@S-0042-v2, p. 14]` and the input of `cit-s-024` with each byte once flipped and once deleted. Also include the explicit boundary input `[@`: in each mode it returns no citations, an FB0300 finding and in-bounds spans. Each goes through `String::from_utf8_lossy` into `cite::lexer` in both modes. The result is citations and findings whose spans lie inside the text, and never a panic. The company key and the `field` locator are not read here; `r2-company-ids` and `r2-records` add their own inputs to this test.

9. Use `schema::locator` for canonical extension parsing and formatting. Add non-seed vectors and `unknown_extension_citation_round_trips`: preserve kind and payload in both modes and after formatting, including escaped delimiters and Unicode; reject mismatched/invalid escapes and malformed known locators. `cite::resolve` preserves the resolved source plus the unsupported locator fact; it never manufactures an anchor or match. Tests distinguish a missing source from an available source with an unavailable extension.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core cite:: citations:: check::page_body::
cargo nextest run --locked -p fieldbook-core --test golden_citations --test cite_no_panic
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, `random_and_mutated_bytes_never_panic` runs, and `golden_citations::seed_cases_pass` reports 30 cases; `check` prints its summary and `exit 0`; `just check` exits 0.

## Negative control

Let the strict lexer accept a missing comma before the locator. `golden_citations::other_cases_pass` must fail and name `cit-001`. Restore the line. Then delete the FB0302 case from `cases.toml`: `known_bad::every_code_is_accounted_for` must fail and name FB0302. Restore the file. Then make the lexer call `unwrap()` on the character after `[@` immediately after recognizing the opener and before an early malformed-bracket return: `random_and_mutated_bytes_never_panic` must fail. Restore the line.

Return the citations in key order or restore the key-presence approximation: `citations_are_loaded_in_reading_order` or `body_uses_the_requested_key_kind` must fail. Restore each.

Make the body presence helper accept only claim keys, or accept any key kind: `body_uses_the_requested_key_kind` must fail in each case. Restore it.

Discard a lexer finding when no citation resolves: `malformed_and_touching_attempts_survive_load` must fail. Restore it.

Drop an unsupported locator or mark it matched: the extension round-trip/resolution tests must fail. Restore it.

## Rollback

Revert the merge commit. The five codes go back to `pending.toml`, and `Model::citations` goes with it.

## BLOCKED

- A seed case fails and the code follows the specification: write `BLOCKED: seed case cit-s-NNN` with the failing line. Never edit `seed-citations.json`.
- The lead's content in `corpus/` raises one of these codes: name the code and the file. Do not edit `corpus/`.
- The specification does not settle the expected value of a new case: do not add it; ask the lead.

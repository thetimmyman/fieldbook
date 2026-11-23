# pipeline-rules-classify

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`rules@1`, the default `Classifier`, reads `corpus/pipeline/rules.toml` and turns a candidate's extracted text into suggestions for the ten columns the pipeline may fill, each with the quote that caused it and the id of its rule. `fieldbook check` replays every rule on its own `example` and raises FB0826 for a rule that does not reproduce it, has no example or cannot be read. `check` accepts the path in the layout. A synthetic rules file in `corpus-mini` and a known-bad fixture prove both directions.

## Why

- The rules file is how the lead teaches intake without a model: each rule says "this phrase on the first page means this value", and the suggestion carries the phrase as its reason. A rule that stops matching, or never had an example, is an error, because an unchecked rule is a guess that looks like a fact. The code is in [08: audit, backup and pipeline](../reference/corpus-format.md#08-audit-backup-and-pipeline). FB0826 sits in range 08 because ADR 0002 freezes the checks FB0100 to FB0709; the rule is [Adding a pipeline rule](README.md#adding-a-pipeline-rule).
- A classifier never fills `source_role`, `source_family`, `family_basis`, `as_of`, `lifecycle` or `handling`: `SuggestField` from `pipeline-ports` has no variant for them. A person reads the original to call it primary evidence, a blank family means unknown and never independent, and the program never chooses a handling ([Source vocabularies](../reference/corpus-format.md#source-vocabularies)).
- Rules need no model, no connection and no clock, so this is the classifier that works in every build and on the laptop. It is the default of the `Classifier` port ([AD-01](../decisions/0001-foundation.md#ad-01-architecture)), and `rules@1` never defers to Handoff.
- No page, template or string shown in a browser is built here, so there is no Design bullet. Adding `corpus/pipeline/rules.toml` to `corpus-mini` brings its backed-up file count from 33 to 34 (30 corpus files plus 4 state files), so `tests/golden/pages/backup.html` is updated to `34` per [HANDOFF.md](../design/HANDOFF.md#desk-pages-importreport-activitylog-and-backupstatus) (`BackupStatus` snapshots table file count).

## Scope

- Files: `crates/fieldbook-core/src/pipeline/rules.rs`, `crates/fieldbook-core/src/check/{mod.rs,pipeline.rs,files.rs}`, `crates/fieldbook-core/src/versions/{mod.rs,rules_v1.rs}`, `crates/fieldbook-core/tests/{rules.rs,contract_rules.rs}`, `tests/fixtures/corpus-mini/corpus/pipeline/rules.toml` (new, synthetic), `tests/fixtures/corpus-rule-fault/` (a copy of `corpus-mini` whose rules file has a rule with no example), `tests/fixtures/ports/classifier/rules@1/`, `tests/golden/pages/backup.html`, `tests/known-bad/FB0826.toml`, `tests/known-bad/pending.toml` (the FB0826 row is deleted), `compat/adapter/classifier/rules@1/`, `compat/published.toml`, `CHANGELOG.md`.
- Registration lines: the `rules@1` row in `ADAPTERS` (`crates/fieldbook/src/adapters/registry.rs`), the `pub mod rules;` line in `crates/fieldbook-core/src/pipeline/mod.rs`, `crates/fieldbook/tests/adapters.rs` (`contract_rules` dispatch for `Port::Classifier`), `crates/fieldbook/tests/backup.rs` (`data/corpus/pipeline/rules.toml` in `EXPECTED_DATA_FILES`), `xtask/src/ports_doc.rs` (`ADAPTERS.len()` count), `xtask/src/compat.rs` (`("classifier", "rules@1")` runner arm), and `docs/reference/corpus-format.md` (the file table row of `pipeline/rules.toml`).
- Docs to update: `docs/reference/corpus-format.md`, in the file table row of `pipeline/rules.toml`: the five keys of a rule and the sentence that a rule which does not reproduce its example is FB0826, and the FB0826 row of the 08 catalogue, expanded to its full trigger set (unreadable rules, a repeated id, a missing `example` or `expect`, a `set` value outside the vocabulary and a result that differs from `expect`); `docs/reference/ports.md` through `cargo xtask docs`; `CHANGELOG.md` (`- Added: check replays each intake rule on its own example.`).

## Out of scope

- Writing the real rules. The lead writes about ten starting rules with examples (a PLAN.md item, not a card). This card edits no line of the lead's file.
- Any model suggestion, the vocabulary drop count and FB0823 (`pipeline-model-adapters`), and the use of the classifier by a verb (`review-queue`).
- Suggestions from the correlator, such as `purpose` from an open question (`pipeline-correlate`). A rule may still set `purpose` to a constant.
- Shipping the rules file in an edition. The reference says it does not ship, and `edition-zips` owns the allowlist.

## Preconditions

- Cards merged: `pipeline-ports` (`Vocab::fields()` and the closed `SuggestField`), `quote-match-and-anchors` (`quote::find`, which step 2 calls), `check-known-bad` (the harness and the FB0826 row of `pending.toml` that step 7 deletes), `calc-day1` (the seeded generator `tests/support/rng.rs` that step 8 includes) and `edition-zips` (the `edition::` tests that Acceptance runs).
- The lead writes the rules file after this card has merged, because `check` refuses the path (FB0121) until step 5 is in. This card's first test is the synthetic file in `corpus-mini`; a rule of the lead's file that does not reproduce its example is the lead's to fix, not this card's.

## Steps

1. `pipeline/rules.rs`, the file. `RulesFile` is read with error positions and unknown keys denied. Each `[[rule]]` has `id` (`[a-z0-9-]{1,40}`, unique), `when`, `set`, `example` and `expect`, and the line it starts on. `when` is exactly one of `phrase = "..."`, `id_shape = "..."` (the name of a lexer shape from [The lexer](../reference/search.md#the-lexer); only `clause_number` is useful here) or `publisher = "..."`, with an optional `pages = "any"` (the default is the first page only). `set` is `{ field, value }` with `field` one of the names that `Vocab::fields()` lists: the ten `SuggestField` names with `Vocab::from_schema()`, which is what the Corpus uses. The vocabulary of a company store adds `doc_kind`, and `check` of that store then accepts it in `set` without an edit here (`r2-org-intake` tests it); a `set` that names a field the vocabulary does not offer is FB0826. `example` is the text of one page; `expect` is `{ field, value }` with an optional `quote`.
2. The matcher, pure. A `phrase` or `publisher` is found with `quote::find` of `quote-match-and-anchors`, which returns the span it matched, so the quote in a suggestion is the page's own text. Pass `within = 1..=1` for the default first-page rule, or the complete `1..=page_count` range for `pages = any`; empty page sets have no match. That match keeps case, and so does a rule. An `id_shape` is found by the lexer. The first match in page order is the support. The `publisher` kind differs from `phrase` only in what `queue show` prints as the reason.
3. `rules@1` implements `Classifier`. It takes the rules in file order and, for each field, keeps the first rule that matches and that passes `validate` from `pipeline-ports`; a later rule for the same field adds nothing. Every suggestion has `by = rules@1` and a `Support::Quote`. If no rule sets `title`, it suggests `Candidate::first_line()` when that line has at most 160 characters, with that line as the quote. It drops an `authority_tier` that the suggested `source_kind` does not allow ([Source vocabularies](../reference/corpus-format.md#source-vocabularies)). A `licence` comes only from a rule. It returns `Ok` with empty `Suggestions` (no fields, no hints) when nothing matches, and never `Deferred`.
4. `check/pipeline.rs`, registered in `check/mod.rs`. If `corpus/pipeline/rules.toml` is absent nothing is raised. Otherwise each rule is run on its own `example` as the only page of a synthetic candidate, and the result must contain `expect`. FB0826 is raised, at the rule's line, for a rule that cannot be read, a repeated id, a rule with no `example` or no `expect`, a `set` value outside the vocabulary and a result that differs from `expect`. The message names the rule id and says what it produced.
5. `check/files.rs`: the layout accepts `pipeline/rules.toml`, and no other path under `pipeline/`. The file is a text file, so FB0122 holds it to normal form like the other hand-written files.
6. Fixtures. `corpus-mini` gets a rules file with three synthetic rules: `reg-text` (`when = { phrase = "Synthetic Regulation Text" }`, `set = { field = "source_kind", value = "primary_law" }`, `example = "Synthetic Regulation Text, part one. Made up for tests."`, `expect = { field = "source_kind", value = "primary_law", quote = "Synthetic Regulation Text" }`), `fixture-agency` for `publisher`, and `fixture-licence` for `licence`. The fixture stays strict clean. `corpus-rule-fault` is a copy of `corpus-mini` whose rules file also holds a rule with no `example`. `tests/fixtures/ports/classifier/rules@1/` holds `input.txt` and an `expect.toml` with the suggestions.
7. `tests/known-bad/FB0826.toml`: `verb = "check"`, `args = ["--strict"]`, `state = "corpus-rule-fault"`, `expect` FB0826 and exit 1. Delete the FB0826 row of `pending.toml`.
8. Tests in `tests/rules.rs`: `rules_file_parses_with_positions`, `unknown_key_is_fb0826`, `duplicate_id_is_fb0826`, `rule_without_example_is_fb0826`, `rule_whose_example_does_not_match_is_fb0826`, `value_outside_the_vocabulary_is_fb0826`, `reserved_field_in_set_is_refused`, `doc_kind_in_set_is_refused_for_the_corpus` (FB0826: the vocabulary of the Corpus does not offer it), `first_matching_rule_wins_per_field`, `quote_is_text_on_the_page`, `tier_the_kind_does_not_allow_is_dropped`, `title_falls_back_to_the_first_line`, `no_rules_file_gives_only_a_title`, `same_input_same_output`, `layout_allows_the_rules_file_and_nothing_else_under_pipeline`, and `fault_fixture_differs_from_corpus_mini_only_in_the_rules_file`. `tests/rules.rs` also has `random_and_mutated_bytes_never_panic`: it includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 512 from `Rng::new(1)`, and takes the text of `tests/fixtures/corpus-mini/corpus/pipeline/rules.toml` with each of its bytes once flipped and once deleted. Each goes through `String::from_utf8_lossy` into `RulesFile::parse`, and the result is a `RulesFile` or a list of errors that each have a line of at least 1, never a panic. Each random string is also the only page of a candidate that goes through `rules@1` with the fixture rules, and the result is `Ok` with suggestions that all pass `validate`. In `tests/contract_rules.rs`, `contract_classifier_rules_1` runs the classifier suite of `pipeline-ports`.
9. Add the `rules@1` row (Classifier, the default, `Built`, `Silent`, crate `fieldbook-core`, `Needs::None`, `sees` labelled, `calls_model` false, no settings keys, suite `classifier_suite`). Run `cargo xtask docs` and commit `ports.md`.

10. Emit the optional `RuleRef` of the pipeline contract for phrase, publisher and ID-shape rules, including distinct IDs for identical support text. The title fallback omits it. Test the pure proposal serialization round trip here; dropping the metadata must fail the distinct-rule test. The later review-queue card owns the binary run_pipeline persistence test.

11. Make tier_allowed consume the same source-kind/tier table as register validation: eligible public kinds permit discovery-only D, synthetic_fixture and company kinds remain untiered. Test every table row, including valid trade_press/D and refused synthetic_fixture/D and company_record/D; disabling the restriction must fail.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core --test rules --test contract_rules
cargo nextest run --locked -p fieldbook --test known_bad --test adapters
cargo nextest run --locked -p xtask edition::
cargo build -q --locked -p fieldbook && B="$PWD/target/debug/fieldbook" && T="$(mktemp -d)" && cp -R tests/fixtures/corpus-mini/. "$T"
(cd "$T" && "$B" check --strict --today "$(cat TODAY)"; echo "exit $?")
sed -i 's/value = "primary_law", quote/value = "trade_press", quote/' "$T/corpus/pipeline/rules.toml"
(cd "$T" && "$B" check --strict --today "$(cat TODAY)"; echo "exit $?")
cargo xtask docs --check
just check
```

Expected: the tests pass, and the edition tests still pass, which shows the new file is not shipped; the first `check` prints nothing and `exit 0`; after the edit the second prints one FB0826 diagnostic that names `reg-text` and `exit 1`; `docs --check` and `just check` exit 0. Remove the temporary folder afterwards.

## Negative control

In the temporary copy, delete the `example` line of one rule: `check` must print FB0826 and exit 1. On the branch, make `check/pipeline.rs` skip rules that have no `example`: `rule_without_example_is_fb0826` and the FB0826 known-bad case must fail. Restore it. Then remove the `title` fallback from `rules@1`: `title_falls_back_to_the_first_line` must fail. Restore it. Then let `RulesFile::parse` index the first character of an empty line: `random_and_mutated_bytes_never_panic` must fail. Restore it. Then offer `doc_kind` in `Vocab::from_schema()`: `doc_kind_in_set_is_refused_for_the_corpus` must fail. Restore it. Last, give a rule `set = { field = "lifecycle", value = "active" }`: the file must be refused with FB0826, because `lifecycle` is not one of the ten fields.

Remove the `rules@1` row from `ADAPTERS`: `adapters_match_registry_docs_and_fixtures` must fail. Restore it.

## Rollback

Revert the merge commit. `check` stops replaying rules, the layout no longer accepts `pipeline/rules.toml` (so remove the lead's file or revert with it), and `rules@1` leaves the registry and `ports.md`. No proposal file holds a `rules@1` suggestion yet unless `review-queue` has merged; revert that card first in that case.

## BLOCKED

The span that `quote::find` returns is an offset in the normalised text, so the page's own words cannot be cut out for a suggestion's quote. Set the item to Blocked with `BLOCKED: quote span is in normalised text`, and stop. Do not store the normalised text as the quote, and do not write a second normaliser in `rules.rs`.

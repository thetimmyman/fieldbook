# core-schema-pages

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook-core::schema` reads `corpus.toml` and page files into typed values. Unknown keys are refused, each page vocabulary is an enum, and a page splits into front matter and body with the line and column of every parse error. One test covers every page type: each has a template and a row in the reference.

## Why

- The fields and rules are in [Areas and views](../reference/corpus-format.md#areas-and-views), [Page files](../reference/corpus-format.md#page-files) and the page rows of [Status vocabularies and promotion rules](../reference/corpus-format.md#page-vocabularies) ([AD-09](../decisions/0001-foundation.md#ad-09-corpus-formats-and-the-schema-source-of-truth)).
- The Rust types are the schema source of truth; the reference documents describe them.

## Scope

- Files: `crates/fieldbook-core/src/schema/{mod.rs,vocab.rs,corpus_toml.rs,page.rs,page_types.rs}`, `crates/fieldbook-core/src/lib.rs`, `crates/fieldbook-core/tests/schema_no_panic.rs`.
- Registration lines: `docs/reference/corpus-format.md` (the intro of the Page types table: one sentence that names the test of step 8).
- Docs to update: the intro of the Page types table in `docs/reference/corpus-format.md` names the test of step 8 (one sentence).

## Out of scope

- Register rows and their vocabularies (`core-schema-registers`).
- Turning parse errors into diagnostics and codes (`check-v0` and `check-pages`).
- The body: parts, links and citations (`check-page-body`, `check-citations`). The test of step 8 does not check a template's parts or a type's render: `check-page-parts` adds the parts order to it, and `build-page-types` adds a render-golden test over `PageType::ALL`.

## Preconditions

- Cards merged: `core-ids-and-dates`.

## Steps

1. In `vocab.rs`, write one enum for each page vocabulary: page type (the nine v1 types, no `note`), readiness, page status, volatility (`pub use crate::date::Volatility`, not a second enum), and the change classes. Each enum has `ALL`, `as_str` and parsing. `PageMeta::handling` is `Vec<String>`, kept as written: a missing key is a missing-field fault, a non-array or non-string member is a wrong-type fault; empty arrays and malformed strings remain available to FB0410. Define `vocab::label_form(&str) -> bool` for the four built-ins and `[a-z]{2,4}-[0-9]{4}`; the parser does not call it for handling. `schema::vocab::every_enum_matches_the_reference` compares each enum, including the re-exported Volatility, with its reference vocabulary row read by `include_str!`. Later enums join that test by [Adding a vocabulary value](README.md#adding-a-vocabulary-value).
2. In `corpus_toml.rs`, write `CorpusToml` with `schema_version`, `[assistant]` (`pack_budget_chars`), `[[area]]` and `[[view]]`, all with `deny_unknown_fields`. Add `validate()` that returns one problem per fault: a malformed or repeated `code` or `slug`, a repeated subarea, a view code that is not three capitals or equals an area code, a view filter that names no area or no subarea of that area, and a `schema_version` above `version::SCHEMA_VERSION_MAX`. Test it with `schema::corpus_toml::a_view_filter_must_name_a_real_subarea` (a view whose filter names a subarea its area does not have gives one problem), and one test for each other fault `validate()` reports.
3. In `page.rs`, write `PageMeta` with every front-matter field of the reference table except the four note-only fields (`origin`, `reviewed`, `question` and `context_fingerprint`), and `deny_unknown_fields`. The note-only fields belong to `note` pages, which `handoff-new-note` adds; until then they are unknown fields on every page (FB0102). `claims` and `review_by` are optional fields that exist only so that writing them can be reported later. Read dates as TOML local dates and convert them to `CivilDate`.
4. Write `split(&str) -> Result<Split, SplitError>`: the first line is exactly `+++`, the closing line is `+++`, then one blank line, then the body. Report a missing opening fence, a missing closing fence and a missing blank line. `Split` keeps the line where the body starts.
5. Write `parse_meta(&str) -> Result<PageMeta, MetaError>`. Sort each failure into one kind: syntax, unknown field, wrong type, missing field, calendar date, or value outside its vocabulary or form. Keep the declared date key and span even when the TOML reader rejects its date token; malformed/impossible date tokens or partial dates where a full date is required are `MetaError::CalendarDate`, distinct from unrelated TOML syntax and wrong types. Keep the 1-based line and column of the failure, counting code points, offset by the lines of the opening fence.
6. Test with the synthetic front matter of the reference and with one case for each error kind, including `titel = "…"` (the test `schema::page::an_unknown_field_is_refused`), `note_only_field_is_an_unknown_field` (a `concept` page with `origin = "ai_drafted"` fails with the unknown-field kind), `stage = "3"`, a missing `as_of`, `status = "final"`, `type = "note"`, a first line of `---`, and an unbalanced fence. `calendar_date_errors_keep_their_kind` covers an impossible TOML local date, a partial full-date field and a valid leap day; an unrelated malformed array still has the syntax kind.
7. Test `CorpusToml` against the real file by including `corpus/corpus.toml` with `include_str!` and requiring zero problems and exactly 13 areas and 3 views.
8. In `page_types.rs`, a function `template(PageType) -> &'static str` that is an exhaustive `match` returning the `include_str!` of `corpus/templates/<type>.md`, so a new variant does not compile until its template exists. The test `schema::page_types::every_type_is_complete` loops over `PageType::ALL` and requires for each type: its template splits with `split`; its front matter raises no syntax or unknown-field error (a quoted placeholder may be a wrong-type or vocabulary error); its `type` line names the type; and the reference's Page types table has a row for the type, with no row for a type outside `ALL`.
9. Write `tests/schema_no_panic.rs` with `random_and_mutated_bytes_never_panic`. It includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 96 from `Rng::new(1)`, and mutates a valid example for each parser: a complete fenced page for `split`, the front matter of step 6 for `parse_meta`, and the included `corpus.toml` for the CorpusToml parser. Flip and delete each byte once; each example goes to its own parser after `String::from_utf8_lossy`, and every parser also receives every random string. Include each enum's valid word in the same mutation loop for its parser. The result is a value or a typed error whose line and column are at least 1, and never a panic.

10. Retain parser-recorded written-key positions on successful `PageMeta` values. `field_location(&self, key: &str) -> Option<(u32, u32)>` returns the start of the key, with 1-based page-file lines including the opening fence and code-point columns; absent keys return `None`. Skip this metadata during serialization. In `schema::page::successful_meta_retains_field_locations`, assert exact positions with reordered keys, comments, multibyte text, optional/computed fields, and empty or malformed-string handling lists that parse successfully for later checking. Existing missing-key and wrong-type faults remain errors. The existing `schema::` Acceptance run includes it.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core schema::
cargo nextest run --locked -p fieldbook-core --test schema_no_panic
just check
```

Expected: both succeed; the `schema::` run includes one test for each error kind of step 6, `every_type_is_complete` and `every_enum_matches_the_reference`, and `random_and_mutated_bytes_never_panic` runs.

## Negative control

Remove `#[serde(deny_unknown_fields)]` from `PageMeta`. The test `schema::page::an_unknown_field_is_refused` must fail. Restore it. Then let `validate()` accept a view filter that names a subarea its area does not have: `schema::corpus_toml::a_view_filter_must_name_a_real_subarea` must fail. Restore the line. Then add a variant to `PageType` without a template: the crate must not compile; with a template but no table row, `every_type_is_complete` must fail and name the type. Revert both. Then remove one value from the readiness enum: `every_enum_matches_the_reference` must fail and name the vocabulary. Restore the line. Then make `split` call `unwrap()` on the first line: `random_and_mutated_bytes_never_panic` must fail. Restore the line.

Drop a recorded position: `schema::page::successful_meta_retains_field_locations` must fail. Restore it.

## Rollback

Revert the merge commit. Nothing outside `fieldbook-core` uses the module yet.

## BLOCKED

The real `corpus/corpus.toml` fails to parse with the types as the reference describes them. The file is the lead's: write the line and the error and stop. Do not edit it.

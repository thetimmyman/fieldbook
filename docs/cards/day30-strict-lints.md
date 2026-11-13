# day30-strict-lints

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`fieldbook check` reports FB0308 (a quote not found at its locator) as an error on a consequential claim and as a warning on any other claim, and FB0309 (a figure with no claim citation in its sentence) as an error everywhere, with or without `--strict`. The number-unit lint no longer reads a regulation abbreviation such as `FAR 4.702`, `CAS 404` or `29 CFR` as a currency amount. `fieldbook check --strict` on the real Corpus exits 0 with no FB0308 or FB0309 finding.

## Why

- The severities are decided: [ADR 0002](../decisions/0002-corpus-format-v1.md) ("The quote match warns on Day 1 and is an error for a consequential claim from Day 30. The number-unit lint warns on Day 1 and is an error from Day 30"), the Check catalogue rows FB0308 and FB0309 in [corpus-format.md](../reference/corpus-format.md#check-catalogue), and the Day-30 gate of [Phases and exits](../decisions/0001-foundation.md#phases-and-exits) ("The quote match and the number-unit lint are errors and clean"). `docs/reference/errors.md` is generated from the catalogue.
- "From Day 30" has no date and no setting: the release is the switch. This card is the change, and it must merge with a Corpus that is already clean, so that `just check` stays green. A read of the real Corpus at the time of writing gave no FB0308 finding and four FB0309 findings, `FAR 4.702`, `CAS 404`, `FAR 31.302` and `29 CFR`. All four are the same fault of the lint, not of a page: `scan_currency_code` in `crates/fieldbook-core/src/check/citation_rules.rs` accepts any three capital letters because `Currency::parse` takes the ISO 4217 shape, so `FAR`, `CAS` and `CFR` before a number read as a currency code. The catalogue says "a currency sign or code", and the `unit` vocabulary says a code is one "from ISO 4217" ([claim vocabularies](../reference/corpus-format.md#claim-vocabularies)), so the lint is narrowed to the ISO 4217 codes and no page is rewritten. This is a lead decision where the repository was silent on the codes; the narrower reading is the only one that does not force a citation of a claim that does not exist onto a clause number.
- A diagnostic takes its severity from `Code::severity` ([Check catalogue](../reference/corpus-format.md#check-catalogue); the severity text is the catalogue cell). FB0308 needs a per-claim severity, which a code-wide level cannot give, so its diagnostic is built with an explicit severity.
- The number-unit lint's exempt places and its sentence rule do not change ([Notes on the 03 range](../reference/corpus-format.md#check-catalogue)). `unit_words_match_the_reference` in `crates/fieldbook-calc/tests/units.rs` reads the lint's sentence and must keep passing: this card keeps the lint's wording in the catalogue note and only adds the ISO 4217 reading.
- `proptest` is the other half of the PLAN line and has its own card, `day30-proptest`, so that each card stays near 500 lines ([Size and splitting](README.md#size-and-splitting)).
- Design: none. No page, string or style changes.

## Scope

- Files: `crates/fieldbook-core/src/check/{code.rs,diag.rs,mod.rs,quotes.rs,citation_rules.rs}`, `crates/fieldbook-core/src/check/currency_codes.rs`, `crates/fieldbook-core/tests/known_bad_severity.rs`, `CHANGELOG.md`.
- Registration lines: `crates/fieldbook-core/src/check/mod.rs` (the `mod currency_codes;` line); `tests/known-bad/cases.toml` is not changed (both cases already exist and keep their codes); `docs/reference/corpus-format.md` (the FB0308 and FB0309 rows and the number-unit note of the Check catalogue, this card's rows only)
- Docs to update: `docs/reference/corpus-format.md` (the FB0308 and FB0309 severity cells, and the number-unit sentence: a currency code is one of ISO 4217), `docs/reference/errors.md` (generated: run `cargo xtask docs`, never hand-edit; named only here), `CHANGELOG.md` (`- Changed: the quote match on a consequential claim and the number-unit lint are errors.` under `## Unreleased`).

## Out of scope

- A new check code, a change of any other code's severity, and `--strict` itself (it still raises the `strict` codes only).
- Rewording a page, a claim or a quote. If a finding remains after step 2 that is a real uncited figure, the page cites an existing claim in the same pull request; no claim is edited and none is added ([AGENTS.md](../../AGENTS.md) hard stops on certification).
- The unit words, `Unit`, `Currency::parse` and everything in `fieldbook-calc`: the narrowing is in the lint only.
- `proptest` (`day30-proptest`).

## Preconditions

- Cards merged: `extract-csv-rows` and `extract-xlsx-sheets` (both also change `crates/fieldbook-core/src/check/quotes.rs`; this card adds the severity call after their `rec` and `sheet` branches), `check-citations`, `check-citation-rules` (the FB0309 pass), `quote-match-and-anchors` (FB0308), `check-known-bad` (the case runner), `check-ci-and-docs` (generated `errors.md`).

## Steps

1. `currency_codes.rs`: `pub const ISO_4217: &[&str]` (the active alphabetic codes of ISO 4217, sorted, upper case) and `pub fn is_iso_4217(code: &str) -> bool` by binary search. `citation_rules.rs`: `scan_currency_code` returns a code only when it is both `Currency::parse`-valid and `is_iso_4217`; nothing else about the figure scan changes. Tests in `currency_codes.rs`: `the_list_is_sorted_and_three_capitals` and `usd_eur_gbp_jpy_are_codes_and_far_cas_cfr_are_not`.
2. Run `cargo run --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"` on the Corpus before the severities change and read the FB0308 and FB0309 lines: expect none of either. A remaining FB0309 on a page is fixed by citing an existing claim that supports the figure; a remaining FB0308 means a quote differs from its extraction, and the claim stops the card (BLOCKED), because a quote edit resets a check and is a Corpus change.
3. `code.rs` and the catalogue: the FB0309 severity text becomes `error` in both. The FB0308 text becomes `error on a consequential claim, warning on any other claim` in both, with the row's meaning sentence unchanged except that it no longer says "from Day 30". Keep the help sentences. `diag.rs`: add `Diagnostic::with_severity(self, severity: Severity) -> Self`. `quotes.rs`: `match_claim` builds the FB0308 diagnostic and sets `Severity::Warning` when the claim's `consequential` cell is not `true`. `mod.rs`: the unit test that pins FB0308 and FB0309 levels is replaced by one that pins `Level::Error` for both.
4. Tests, run as `cargo nextest run --locked -p fieldbook-core check::quotes:: check::citation_rules:: check::currency_codes:: check::tests::` and `--test known_bad_severity`: `fb0308_is_an_error_on_a_consequential_claim` (the existing changed-word case); `fb0308_stays_a_warning_on_another_claim` (the same case with `consequential` false; exit 0); `fb0309_is_an_error_without_strict`; `fb0309_ignores_regulation_abbreviations` (`FAR 4.702(a)`, `CAS 404, 407`, `29 CFR 4.1` in a sentence with no citation raise nothing; `USD 100,000` and `$5M` still raise); and `crates/fieldbook-core/tests/known_bad_severity.rs`, `known_bad_fb0308_and_fb0309_report_an_error`, which runs the two existing cases of `tests/known-bad/cases.toml` through the check library and requires an error-level diagnostic with the case's code for each; the exit code 1 is left to `cargo nextest run -p fieldbook --test known_bad`, which Acceptance runs.
5. Run `cargo xtask docs` and commit the changed `errors.md`; run `cargo xtask docs --check`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::quotes:: check::citation_rules:: check::currency_codes:: check::tests::
cargo nextest run --locked -p fieldbook-core --test known_bad_severity
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)" | grep -c 'FB0308\|FB0309'; echo "exit ${PIPESTATUS[0]}"
cargo xtask docs --check
just check
```

Expected: the tests pass; the `grep` count is `0` and the exit is `0`; `docs --check` and `just check` exit 0.

## Negative control

In `citation_rules.rs`, let `scan_currency_code` skip `is_iso_4217`: `fb0309_ignores_regulation_abbreviations` must fail, and the Corpus `check` must print the four findings again. Restore it. Then make the FB0309 severity text `warning` in `code.rs` and the catalogue: `fb0309_is_an_error_without_strict` and the generated-docs check must fail. Then drop the `with_severity` call in `quotes.rs`: `fb0308_stays_a_warning_on_another_claim` must fail. Then drop the `consequential` test so every FB0308 is an error: the same test must fail. Restore each.

## Rollback

Revert the merge commit. Both codes warn again, the lint again reads `FAR 4.702` as an amount, and `check --strict` on the Corpus again prints four warnings and exits 0. Nothing in the Corpus changes.

## BLOCKED

A real FB0308 finding on a consequential claim exists on `main` when the card runs (a quote edit is Corpus work and resets a check): stop and report the claim id. The Corpus shows an FB0309 finding that step 2 cannot clear by citing an existing claim: stop and report the page and line. A negative control that stops failing stops the card.

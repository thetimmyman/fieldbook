# check-citation-rules

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check` raises FB0117, FB0118, FB0309, FB0310 and FB0320 at their catalogue severities, from the citations `check-citations` stored in the model. A page's `review_by` is computed by one function that `build` reuses.

## Why

- FB0117 and FB0118 are in [01: pages, corpus.toml and files](../reference/corpus-format.md#01-pages-corpustoml-and-files). FB0309, FB0310 and FB0320 are in [03: citations](../reference/corpus-format.md#03-citations), with the figure and the exempt places under its notes.
- The review intervals and the page rule are in [Dates](../reference/corpus-format.md#dates). FB0117 never fails, not even under `--strict`. What counts as superseded is in [Supersession](../reference/corpus-format.md#supersession).
- The strict rules are listed in the severity table of the [Check catalogue](../reference/corpus-format.md#check-catalogue).

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,citation_rules.rs}`, `crates/fieldbook-core/src/review.rs` (new), `crates/fieldbook-core/src/lib.rs` (the module line only), `tests/known-bad/{cases.toml,pending.toml}`, `CHANGELOG.md`, and `tests/fixtures/corpus-mini/` only to fix a fault a new rule finds.
- Docs to update: `CHANGELOG.md` (`- Added: check reports the citation and review-date rules (FB0117, FB0118, FB0309, FB0310 and FB0320).`). `errors.md` does not change; the codes already exist there.

## Out of scope

- The Day-30 change of FB0309 to an error (`day30-strict-lints`). FB0308 (`quote-match-and-anchors`).
- A claim's `review_by` and FB0221 (`check-promotion`).
- Showing `review_by` on a page (`build-page`).

## Preconditions

- Cards merged: `check-citations`.

## Steps

1. Write `review::page_review_by(page, cited claims) -> CivilDate` by the page rule in Dates. Unit tests: the page's own `volatility` wins; else the most volatile cited claim; else `V3`. The volatility argument is `crate::date::Volatility`, re-exported by the vocabulary module.
2. FB0117: a page whose `review_by` is before `Model::today()`. A warning, with and without `--strict`.
3. FB0118: a page `as_of` later than the oldest `as_of` among its cited claims. A partial claim date compares as its first day. Compare only known claim dates; if none is known, no date comparison is possible. Test `unknown_claim_date_is_not_invented` with an allowed stale claim and with a second, dated claim. Strict.
4. FB0309: the number-unit lint. Take the figure and the exempt places from the notes of the 03 range. The time words of the lint are not listed again here: they are exactly the words of `fieldbook_calc::Unit::ALL` for which `is_time()` is true and whose `word()` has no underscore (`days`, `weeks`, `months`, `years`, `hours`), the same set that `unit_words_match_the_reference` reads from the reference's number-unit sentence. `business_days` stays a real unit but is not a lint word, because the reference's sentence is a set of plain time words. The lint, the claim `unit` cell and the reference stay in step. The money and scale words (`dollars`, `million`, `billion`, `thousand`, `percent`) come from the notes of the 03 range. A sentence ends at `.`, `!` or `?` followed by white space or the end of the block; a decimal point does not end one. A warning.
5. FB0310: a page cites a superseded, withdrawn or quarantined source version, or a claim with `superseded_by` set. A warning.
6. FB0320: a `stable` page cites a consequential claim whose `verification` is not `primary_source_supported`. Strict.
7. Add the passes to `check/mod.rs`. Put the unit tests in `mod citation_rules`, one per code, plus `fb0117_never_fails_strict` and `fb0320_is_an_error_only_under_strict`.
8. Move the five codes from `pending.toml` to `cases.toml`. FB0118 and FB0320 set `strict = true`. The FB0117 plant moves a page's `as_of` more than its interval before the date in `TODAY`.
9. `the_fixture_is_strict_clean` counts warnings. Keep `tests/fixtures/corpus-mini/TODAY` on or after every fixture date and on or before every fixture page's `review_by`. If that is not possible, change the fixture dates, not the rule.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core review:: citation_rules::
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass; `check` prints its summary and `exit 0`; `just check` exits 0.

## Negative control

Apply the FB0320 case's edits from `cases.toml` by hand to a temporary copy, then run `check` there twice:

```sh
root="$PWD"; today="$(cat tests/fixtures/corpus-mini/TODAY)"; tmp="$(mktemp -d)"
cp -r tests/fixtures/corpus-mini/. "$tmp"
# edit "$tmp" as the FB0320 case says
(cd "$tmp" && cargo run -q --locked --manifest-path "$root/Cargo.toml" -p fieldbook -- check --today "$today"; echo "exit $?")
(cd "$tmp" && cargo run -q --locked --manifest-path "$root/Cargo.toml" -p fieldbook -- check --strict --today "$today"; echo "exit $?")
rm -rf "$tmp"
```

The first run prints an FB0320 warning and `exit 0`. The second prints `error[FB0320]` and `exit 1`. Then make FB0117 an error under `--strict`: `citation_rules::fb0117_never_fails_strict` must fail. Restore the line.

## Rollback

Revert the merge commit. The five codes go back to `pending.toml`.

## BLOCKED

- The lead's content in `corpus/` raises FB0118 or FB0320 under `--strict`: name the code and the file. Do not edit `corpus/`.
- The notes of the 03 range do not settle whether a token is a figure: write the token and ask the lead.

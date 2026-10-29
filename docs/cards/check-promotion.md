# check-promotion

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** owner

## Outcome

`check::run` raises FB0209 (a broken promotion rule), FB0213 (a supported claim on a quarantined or withdrawn version), FB0220 and FB0221 (a claim's `as_of` after today, a claim's `review_by` passed), FB0222 (a claim on a superseded version) and FB0230 (an incomplete day-one question). FB0230 is a warning, and an error under `--strict`.

## Why

- The codes and severities are in [02: registers and sources](../reference/corpus-format.md#02-registers-and-sources); the meaning of `strict` is in the severity table of the [Check catalogue](../reference/corpus-format.md#check-catalogue).
- The conditions for `primary_source_supported` and who may verify what are in [Promotion rules](../reference/corpus-format.md#promotion-rules). Review dates, partial dates and today are in [Dates](../reference/corpus-format.md#dates); a superseded version is computed as [Supersession](../reference/corpus-format.md#supersession) says.
- The card checks statuses; it never raises one. It writes the promotion rule that FB0209 enforces, which is the status gate ([AD-21](../decisions/0001-foundation.md#ad-21-codex-execution-plan)), so the review is the owner's.

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,register_cells.rs,promotion.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (dates, `TODAY`, and fixes a new rule finds), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports broken promotion rules, claim dates, claims on superseded versions and incomplete day-one questions (FB0209, FB0213, FB0220 to FB0222, FB0230).`). `docs/reference/cli.md` already lists FB0230 as strict (`check-ci-and-docs`).

## Out of scope

- Raising, lowering or setting any status, `verified_on` or `verified_by_role`, here or in the fixture's claims beyond their dates.
- Whether a quote matches the extracted text (FB0308, `quote-match-and-anchors`). FB0209 checks here that the quote is present.
- FB0223, a consequential claim on a metadata-only version or a version with pages of little or no text: `extract-and-locators` raises it, because it reads the extraction.
- Page dates (FB0116 in `check-pages`, FB0117 and FB0118 in `check-citation-rules`) and the `verify` verb (`verify-status-fmt`).

## Preconditions

- Cards merged: `check-register-links`.

## Steps

1. FB0230 is the first strict code to be raised. `Code::severity(strict)` of `check-v0` already reports a `Strict` code as a warning without `--strict` and as an error with it, so this card changes no code in `check/diag.rs`. The unit test of step 8 shows it for FB0230.
2. Add `promotion.rs` with one pass over the typed claim and question rows and the `Index`. Point the FB0213 and FB0230 entries of the rules table in `register_cells.rs` at it.
3. FB0209: for each claim whose `verification` is `primary_source_supported`, report each of conditions 1 to 5 of Promotion rules that does not hold, and report a consequential claim verified by a role other than the owner. Condition 6 is FB0213, not FB0209. Read "a regulatory claim" as a claim with an `instrument`; read "a number" as a claim with a `value` or a `unit`.
4. FB0213 from the version's `lifecycle`, at the claim's `version` cell. FB0222 from the computed superseded state, at the same cell.
5. FB0220 and FB0221 with `Model::today()` and the review dates of Dates; a partial date compares as its first day; a `review_by` equal to today has not passed. Both stay warnings under `--strict`. FB0220 requires a known `as_of`; claim `review_by` uses the later known `as_of` or `verified_on`, or is absent when both are absent. FB0221 does not compare an absent review date. Test `unknown_claim_dates_remain_unknown` with an allowed stale claim whose note begins `as_of unknown`.
6. FB0230 at the first empty cell of a day-one question.
7. Keep the fixed date in `tests/fixtures/corpus-mini/TODAY` (which `known_bad.rs` reads) on or after every `as_of` and on or before every `review_by` of the fixture; a `review_by` equal to today has not passed. Move fixture dates only if no such date exists.
8. Unit tests named `check::promotion::...`: each code; each condition of FB0209 alone; a lead-verified consequential claim; a partial `as_of`; the boundary day of `review_by`; FB0230 as a warning without `--strict` and an error with it.
9. Move the six codes from `pending.toml` to `cases.toml`, the FB0230 case with `strict = true`. The catalogue's FB0209 example also breaks a rule across columns, so its case lists `also = ["FB0206"]`. If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request.

10. Apply the permanent instrument-numbering table in Promotion rules. Add `check::promotion::promotion_numbering_requirements_match_instrument` through `check::run`: a valid supported claim for every instrument, each required cell empty in turn, every permitted empty counterpart and a non-regulatory claim. Assert FB0209 for the promotion failure and retain any independent FB0206 diagnostics from the three unconditional instrument rules.

11. An unsupported extension locator never meets the available fine-locator condition for support. Preserve its parsed kind/payload and report FB0209 for a supported claim using it; ordinary unchecked rows remain readable. `unsupported_extension_cannot_support_a_claim` checks this through the pass.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::promotion check::register_cells
cargo nextest run --locked -p fieldbook --test known_bad
cargo xtask docs --check
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with six new known-bad cases; `docs --check` prints `docs: up to date`; `check` on the real Corpus prints `exit 0` (warnings may print); `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini` and empty the `default_assumption` cell of its day-one question. In `target/nc/corpus-mini`, `cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"` must print a `warning[FB0230]` line and exit 0; the same command with `--strict` must print `error[FB0230]` and exit 1. Remove `target/nc/`.

Disable a required numbering-cell check: `promotion_numbering_requirements_match_instrument` must fail. Restore it.

## Rollback

Revert the merge commit. The six codes go back to `pending.toml`, and strict findings return to how `check-v0` reported them.

## BLOCKED

- The lead's committed claims or questions raise an error code of this card under `--strict`: write the code and the row and stop. Fix the content in a pull request of its own (GS-02 judges it), then continue; no agent edits a status.
- A promotion condition cannot be read from the row without a rule the reference does not give: write the condition and stop (status gate).

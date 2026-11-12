# day30-proptest

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`proptest` is a dev-dependency of the workspace, and the decimal parser and operations of `fieldbook-calc` and the tokenizer of `fieldbook-core` each have property tests that run in `just check`. The seeded `*_no_panic.rs` tests stay beside them.

## Why

- The crate, its pin, its features and its place are decided: the `proptest` row of [dependencies.md](../reference/dependencies.md#test-only-and-dev-only-crates) (1.12.x, `default-features = false`, `std`, dev-dependency, MIT OR Apache-2.0, MSRV 1.88) and the Day 30 row of [testing.md](../reference/testing.md). Both say "Day 30" and "properties for `fieldbook-calc` and the tokenizer"; the Bridge `read_request` property is P5 and stays with `chat-bridge`, which adds only its own `[dev-dependencies]` line.
- The sentence in [Test data](../reference/testing.md#test-data) says `proptest` replaces the seeded parser test "at Day 30". Replacing 200 seeded cases per parser would remove tested corpora of mutations of each card's valid example, which a generator does not produce, so this card adds properties and keeps the seeded tests (lead decision); the sentence is reworded to say so.
- It split from `day30-strict-lints` so that each card stays near 500 lines ([Size and splitting](README.md#size-and-splitting)); `chat-bridge` now names this card as the one that puts `proptest` in the workspace.
- A dependency is not a gate: the card is `auto` with its row and the actual evidence of [Review evidence](../reference/dependencies.md#review-evidence), and it is a dev-dependency, so it is outside the direct-crate budget of the shipped build and past no 15th-crate threshold.
- Design: none.

## Scope

- Files: `crates/fieldbook-calc/Cargo.toml`, `crates/fieldbook-core/Cargo.toml`, `crates/fieldbook-calc/tests/props_decimal.rs`, `crates/fieldbook-calc/tests/props_ops.rs`, `crates/fieldbook-core/tests/props_tokenize.rs`, `docs/reference/dependencies.md`, `docs/reference/testing.md`.
- Registration lines: `Cargo.toml` (the one `proptest` line under `[workspace.dependencies]`, this card's line only); `Cargo.lock` (the lock diff of `just lock`). `CHANGELOG.md` does not change: nothing the owner sees.
- Docs to update: `docs/reference/dependencies.md` (a Measurements row, the `proptest` row's "Day 30" marker, and the Young lockfile table if the lock holds a young entry), `docs/reference/testing.md` (the Day 30 row and the Test data sentence).

## Out of scope

- The Bridge `read_request` property and the `crates/fieldbook/Cargo.toml` line (`chat-bridge`).
- Replacing or deleting any seeded `*_no_panic.rs` test or `tests/support/rng.rs`.
- Any other crate's properties, any change of source code under `src/`, and any `proptest` feature beyond `std`.
- A regression file policy: a failing case found by a property is fixed in its own card and pinned as a plain test.

## Preconditions

- Cards merged: `calc-day1` (`Dec`), `calc-ops` (the operations), `search-core-and-cli` (the tokenizer).

## Steps

1. Dependency. In `Cargo.toml` `[workspace.dependencies]`: `proptest = { version = "1.12.0", default-features = false, features = ["std"] }`, the version being the newest release that clears the 14-day cooldown on the day the card runs, never below the row's floor. Add `proptest = { workspace = true }` under `[dev-dependencies]` of `fieldbook-calc` and `fieldbook-core`. Run `just lock` and review the lock diff.
2. Evidence, as the "Review evidence" list orders: `cargo build --workspace --locked`, `cargo build --workspace --frozen`, `cargo deny --locked check bans licenses sources` and the same with `--all-features`, the metadata counts for both profiles (the shipped counts must not change: a dev-dependency is outside them), the release dates of every new registry package, and exact-source MAX/JEV review bound to those bytes. Append the Measurements row.
3. `fieldbook-calc/tests/props_decimal.rs`: strategies for decimal text (`sign? digits (. digits)?` with up to 40 digits, and arbitrary short strings). Properties named `parse_never_panics_on_any_text`, `parse_then_display_then_parse_is_the_same_value` and `parse_rejects_what_the_grammar_rejects` (text with a second point, an exponent, a space or a group separator gives a typed `CalcError`, not a value).
4. `fieldbook-calc/tests/props_ops.rs`: `add_is_commutative_and_associative_without_overflow`, `sub_undoes_add`, `mul_by_one_is_identity`, `div_then_mul_is_within_one_unit_at_the_rounding_scale`, `percent_of_100_is_identity`, and `no_operation_panics_on_extreme_scales` (values at the largest and smallest scale give `Ok` or the typed overflow error). Money operations use one currency code per case.
5. `fieldbook-core/tests/props_tokenize.rs`: `lex_never_panics`, `index_tokens_are_normalised_and_nonempty`, `query_tokens_is_a_subset_of_index_tokens_for_the_same_text`, `lexing_the_joined_tokens_gives_the_same_tokens` and `normalise_is_idempotent` over arbitrary Unicode strings, with the case count set to 256 in each test and a fixed `PROPTEST_RNG_SEED`-free configuration (`ProptestConfig { cases: 256, failure_persistence: None, .. }`) so a run writes no file.
6. `testing.md`: the Day 30 row names the three files; the Test data sentence ends "`proptest` adds properties beside them at Day 30; the seeded tests stay."
7. Run `cargo nextest run --locked -p fieldbook-calc --test props_decimal --test props_ops` and `-p fieldbook-core --test props_tokenize`, then the whole `just check`.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-calc --test props_decimal --test props_ops
cargo nextest run --locked -p fieldbook-core --test props_tokenize
cargo deny --locked check bans licenses sources
cargo deny --locked --all-features check bans licenses sources
git status --short | grep -c proptest-regressions; true
just check
```

Expected: the property tests pass; both deny runs exit 0; the `grep` count is `0` (no regression file is written); `just check` exits 0.

## Negative control

Change `Dec::parse` in a scratch copy to accept a second point: `parse_rejects_what_the_grammar_rejects` must fail and shrink to a short input. Change `add_dec` to drop the carry: `add_is_commutative_and_associative_without_overflow` or `sub_undoes_add` must fail. Make `normalise` lower-case only the first character: `normalise_is_idempotent` must fail. Restore each. Last, set the `proptest` entry to a version younger than 14 days: the cooldown check of the lock must fail.

## Rollback

Revert the merge commit. The dependency, the lock lines and the three test files go; the seeded tests are untouched, so nothing else loses coverage.

## BLOCKED

`cargo deny` rejects `proptest` or a crate it pulls in (a licence or source not allowed): stop and leave `BLOCKED: <crate and rule>`; do not edit `deny.toml`. No release of `proptest` that clears the cooldown and fits MSRV 1.88 exists: stop. A property finds a real fault in `fieldbook-calc` or the tokenizer: stop and report the shrunk input; the fix is its own card.

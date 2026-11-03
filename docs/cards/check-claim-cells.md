# check-claim-cells

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` raises FB0306 when a consequential claim has no fine locator and FB0307 when a consequential claim has no quote. The two entries of the claims rules that `check-register-cells` listed for this card now run.

## Why

- The codes are in [03: citations](../reference/corpus-format.md#03-citations). The rules are rows of the "Rules across columns" table of [claims.csv](../reference/corpus-format.md#claimscsv), and the meaning of `consequential` is in its column table.
- A consequential claim is what the owner's decisions rest on, so its locator and quote are checked from Day 1 ([Promotion rules](../reference/corpus-format.md#promotion-rules), condition 2).

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,register_cells.rs,claim_cells.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports a consequential claim without a fine locator or a quote (FB0306, FB0307).`). `errors.md` does not change.

## Out of scope

- Whether the quote is found in the extracted text (FB0308, `quote-match-and-anchors`).
- Whether the locator resolves in the extraction (FB0304, `quote-match-and-anchors`).
- Any other rule across columns. If `check-register-cells` left another claims entry without a pass, say so in the pull request and leave it.

## Preconditions

- Cards merged: `check-register-cells`.

## Steps

1. Add `claim_cells.rs` with one pass over the typed claim rows. Point the FB0306 and FB0307 entries of the rules table in `register_cells.rs` at it, so `rules_match_the_reference` still passes and no entry is left for this card.
2. FB0306 at the `locator_kind` cell; FB0307 at the `quote` cell. A row that already has FB0204 or FB0205 in `consequential`, `locator_kind` or `quote` raises nothing more here.
3. Unit tests named `check::claim_cells::...`: each code raised once; a consequential claim with a fine locator and a quote raises nothing; a claim that is not consequential with `no_fine_locator` and no quote raises nothing.
4. Move FB0306 and FB0307 from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture. If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::claim_cells check::register_cells
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with two new known-bad cases; `check` on the real Corpus prints `exit 0`; `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini`. On its unchecked consequential claim, set `locator_kind` to `no_fine_locator` and empty `locator`. Run `(cd target/nc/corpus-mini && cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"; echo "exit $?")`: it must print an `error[FB0306]` line for that claim and `exit 1`. Remove `target/nc/`.

## Rollback

Revert the merge commit. The two codes go back to `pending.toml`, and their entries in the rules table point at this card again.

## BLOCKED

The lead's committed claims raise FB0306 or FB0307: write the claim id and the code and stop. Fix the content in a pull request of its own (GS-02 judges it), then continue.

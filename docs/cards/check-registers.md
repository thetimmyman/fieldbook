# check-registers

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` reads the five registers through `core-csv` and raises FB0200 to FB0203: a missing file or a wrong header, a CSV syntax fault, a register that is not UTF-8, and a malformed or repeated key, each at the line and column of the fault. The rows it reads stay in the model for the later register passes.

## Why

- The codes are in [02: registers and sources](../reference/corpus-format.md#02-registers-and-sources). The files, keys and CSV rules are in [Registers](../reference/corpus-format.md#registers); the key forms are in [Identifiers](../reference/corpus-format.md#identifiers).
- `core-csv` returns its read errors as values and leaves the codes to this card. Excel's habits (a byte-order mark, CRLF, cp1252) are a trap in [AGENTS.md](../../AGENTS.md); the reader is tolerant of the first two.

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,model.rs,registers.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports register files, CSV syntax, encoding and keys (FB0200 to FB0203).`). `errors.md` does not change.

## Out of scope

- Cell types, vocabularies, required cells and the rules across columns (`check-register-cells`).
- References between rows, supersession and originals (`check-register-links`).
- Normal form and row order (FB0122, `check-files`).

## Preconditions

- Cards merged: `check-files`.

## Steps

1. Add the registers to the model: for each of `Register::ALL`, the result of `table::read` on its bytes, with each row's line and each cell's column. A register that fails FB0200, FB0201 or FB0202 offers no rows to later passes.
2. Add `registers.rs` with one pass. FB0200: the file is missing (at 1:1), or the first line is not exactly `Register::columns()` in order (at the first differing cell). Compare the header before reading rows, and report FB0200 alone when it is wrong, so a missing column does not also raise FB0201 on every row.
3. FB0201 and FB0202 from the kinds of `TableError`. Turn a byte offset into a line and a column. The FB0202 message carries the wording the catalogue row gives it.
4. FB0203: read each key cell with the id types of `core-ids-and-dates` (for sources, `source_id` with `version` as an integer from 1). Report a malformed key at its cell, and a repeated key at the second row with the line of the first in the message.
5. Unit tests named `check::registers::...` for each code, including a register with a byte-order mark and CRLF that reads without a finding here, and a missing column that raises FB0200 once.
6. Move FB0200 to FB0203 from `pending.toml` to `cases.toml`, from the catalogue's examples adapted to the fixture. If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::registers
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, with four new known-bad cases; `check` on the real Corpus prints `exit 0`; `just check` exits 0.

## Negative control

Copy `tests/fixtures/corpus-mini` to `target/nc/corpus-mini` and repeat one claim row of its `claims.csv`. Run `(cd target/nc/corpus-mini && cargo run -q --locked -p fieldbook -- check --today "$(cat TODAY)"; echo "exit $?")`. It must print an `error[FB0203]` line at the second row and `exit 1`. Remove `target/nc/`.

## Rollback

Revert the merge commit. The four codes go back to `pending.toml`, and the model no longer holds registers.

## BLOCKED

The lead's committed registers raise one of these codes: write the code, the file and the line and stop. Fix the content in a pull request of its own (GS-02 judges it), then continue.

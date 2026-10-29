# check-files

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check::run` raises FB0120 to FB0125 for the files under `corpus/`: text that is not UTF-8, a file outside the layout, a file not in normal form, a path that breaks the path rules, a malformed `changes.md` and a rule file out of form. Each finding carries the path, line and column of the fault.

## Why

- The codes and their known-bad examples are in [01: pages, corpus.toml and files](../reference/corpus-format.md#01-pages-corpustoml-and-files).
- The rules are in [Layout of corpus/](../reference/corpus-format.md#layout-of-corpus) (the layout table, Rule files, File rules and Path rules) and in [Change records](../reference/corpus-format.md#change-records). This card reads them there; it does not restate them.
- `fmt` (a later card) and FB0122 must agree, so registers use the normal-form writer of `core-csv`.

## Scope

- Files: `crates/fieldbook-core/src/check/{mod.rs,files.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` (only to fix a fault a new rule finds there), `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports file, layout, normal-form and path problems (FB0120 to FB0125).` under `## Unreleased`). `errors.md` does not change.

## Out of scope

- Register files, headers, CSV syntax and register encoding (FB0200 to FB0202, `check-registers`). A register that is not UTF-8 is FB0202, never FB0120.
- Front matter and page bodies (`check-pages` and the cards after it).
- Hashes of originals and extraction files (`check-register-links`, `extract-and-locators`).
- The edition path test with the 100-character prefix (FB0602, `edition-zips`), and the `fmt` verb (`fmt-command`).

## Preconditions

- Cards merged: `check-ci-and-docs`.
- It may run beside `docs-checks`; their paths are disjoint.

## Steps

1. Add `files.rs` with one pass, appended to the fixed pass list of `check/mod.rs`. It reads the `Tree` and the parsed `corpus.toml` through `Model::tree()` and `Model::corpus()`, and no front matter.
2. FB0120: every entry under `corpus/` except originals and the five registers is UTF-8. Report the line and column of the first bad byte. A file with FB0120 gets no other finding from this pass.
3. FB0121: build the allowed paths from the layout table, with the area slugs and subareas of `corpus.toml` and the page types of `core-schema-pages` for template names. Report every path that fits none of them, at 1:1. Add `files::page_paths(&Model)`: the page paths this pass accepts. The page passes of later cards read only those, so a misplaced page is reported once.
4. FB0122: for a register that `table::read` accepts, report FB0122 at 1:1 exactly when `table::is_normal(register, bytes)` is false; apply no separate line-level form check. Skip unreadable registers; `check-registers` owns their parse diagnostics. For other text files outside `corpus/sources/`, report the first fault at its line: a byte-order mark, a CR, text that is not NFC, trailing whitespace, or not exactly one final newline. Leave `corpus/sources/` out: its bytes are hashed (FB0211, FB0212). Say this in a comment and in the pull request.
5. FB0123: apply the Path rules to every path under `corpus/`, originals included. Count a path's length as it stands under the repository root (`corpus/...`), which is also its length from the edition root. Report at most one finding per path, for the first rule it breaks.
6. FB0124 for `changes.md` and FB0125 for `ASSISTANT.md` and `AGENTS.md`, each at the offending line.
7. Unit tests named `check::files::...`, with in-memory trees: one for each rule and each normal-form fault (for example `check::files::trailing_whitespace_is_not_normal`), `check::files::a_register_out_of_order_is_not_normal` (rows in the wrong order and no line-level fault: FB0122 at 1:1), and one that the fixture raises nothing.
8. Move FB0120 to FB0125 from `pending.toml` to `cases.toml`, one case each, from the catalogue's examples adapted to the fixture (FB0122: a fixture register given CRLF endings and a byte-order mark). Choose plants that raise only their code. If a case of an earlier card now also raises one of these codes, add it to that case's `also` and say so in the pull request.

9. Add `check::files::register_normal_form_agrees_with_checker`: an otherwise valid claim whose last note cell is `Synthetic note ` preserves that exact cell through normalization, satisfies `table::is_normal` and produces no FB0122 through `check::run`. Keep non-register whitespace and register-sorting cases.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core check::files
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass, and the `known_bad` run includes the six new cases and `the_fixture_is_strict_clean`; `check` on the real Corpus prints its summary and `exit 0`; `just check` exits 0.

## Negative control

The six known-bad cases are controls. In addition, skip the `table::is_normal` call for registers: `check::files::a_register_out_of_order_is_not_normal` must fail. Restore the line. Then disable the trailing-whitespace rule: `check::files::trailing_whitespace_is_not_normal` must fail. Restore it.

Restore a line-level trailing-whitespace check for registers: `check::files::register_normal_form_agrees_with_checker` must fail. Restore the canonical-register check.

## Rollback

Revert the merge commit. The six codes go back to `pending.toml`, and `check` no longer reads file form.

## BLOCKED

- The lead's committed Corpus raises one of these codes: write the code and the file and stop. Fix the content in a pull request of its own (GS-02 judges it), then continue.
- The layout table and the real Corpus disagree, for example about a file the table does not list: write the path and stop.

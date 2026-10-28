# check-scans

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

`check` runs the marking, identity, claim-language, formula and label scans over the repository Corpus and raises FB0400 to FB0405 and FB0410. Each scan is a pure function over text or a cell, so `add` and `new note` can call the same code on one file's text. The scans read the repository Corpus, where a marking is an error. In a company store the same marking matcher feeds the labeller of `core-labels` (built after this card), which turns a marking into a label instead of an error, so the store-aware part is that card and not this one.

## Why

- What each scan reads, catches and allows, the pattern lists and the `scan-allow.toml` rules are in [Scans](../reference/corpus-format.md#scans). The codes are in [04: scans, labels and stores](../reference/corpus-format.md#04-scans-labels-and-stores). The label rule is in [Handling and labels](../reference/corpus-format.md#handling-and-labels).
- The formula rule and its decimal exception are in [Registers](../reference/corpus-format.md#registers).
- The scans are part of the confidentiality guardrails of [AD-18](../decisions/0001-foundation.md#ad-18-security-and-confidentiality-guardrails) and the [labels and scans controls](../controls.md#labels-and-scans).

## Scope

- Files: `crates/fieldbook-core/src/scan/{mod.rs,marking.rs,identity.rs,claim_language.rs,formula.rs,allow.rs}`, `crates/fieldbook-core/src/check/{mod.rs,scans.rs}`, `crates/fieldbook-core/src/lib.rs` (the module line only), `crates/fieldbook-core/tests/{scan_lists.rs,scan_no_panic.rs}`, `tests/known-bad/{cases.toml,pending.toml}`, `tests/fixtures/corpus-mini/` only to fix a fault a new rule finds, and `CHANGELOG.md`.
- Docs to update: `CHANGELOG.md` (`- Added: check reports marking, identity, claim-language and formula problems and non-public labels (FB0400 to FB0405, FB0410).`). `errors.md` does not change.

## Out of scope

- The secrets scan and the personal-name denylist. They run outside `check`.
- Calling the scans from `add` (`add-new-and-edition`) and from `new note` (Day 30).
- A new pattern. That needs the owner's review and a line in Scans.
- Labels in a company store: the label type, the labeller, the store check, FB0411 and FB0412. They are `core-labels`, which wraps `scan::marking` and is built after this card.

## Preconditions

- Cards merged: `check-citation-rules`.

## Steps

1. Write one module per scan under `scan/`. Each exposes a public function that takes text (or one register cell) and returns its findings with the matched text and its byte span. Each also exposes its pattern list as a `pub const`, in the order of the tables in Scans. Use hand-written matchers; no regex.
   - `marking`: the banner shapes, case-sensitive except where Scans says "in any case".
   - `identity`: the identity shapes with their "Does not match" exceptions, including the tracker-key prefix and family lists.
   - `claim_language`: the phrases in any case. The caller passes only the text Scans says to read.
   - `formula`: a text cell that starts with a trigger character. A cell that parses as a decimal is not a text cell.
2. `allow.rs`: parse `scan-allow.toml` with serde and `deny_unknown_fields`. FB0404 for a malformed entry, an empty `reason`, or a repeated `sha256` and `match`. An allowance applies when the file's `sha256` (lower hex, from `hex::lower`) and the `match` both equal. FB0405 for an entry that matched nothing in this run.
3. `check/scans.rs`: one pass that reads the files each row of Scans names, from `Model::tree()` and the model, and reports `path:line:col`. FB0410 reads the `handling` list of every page and source version (raw strings preserved by `core-schema-pages` and `core-schema-registers`, with no label-form validation there) and requires it to be exactly `["public"]`. Add the pass to `check/mod.rs`. FB0410 requires each page and source version handling value to be exactly `["public"]`; every other parsed value fails at its handling line and column (a source row: its handling cell).
4. Unit tests in `mod scan`: one test per shape that matches and one per "Does not match" entry; `lower_case_running_text_does_not_match`; `precise_and_code_are_not_read_for_claim_language`; `minus_five_is_not_a_formula`; `an_edit_ends_the_allowance`; `an_unused_allowance_is_fb0405`.
5. `crates/fieldbook-core/tests/scan_lists.rs` with `mod scan_lists`: include `docs/reference/corpus-format.md` with `include_str!("../../../docs/reference/corpus-format.md")`, read the Scans tables and the claim-language cell, and compare them with the `pub const` lists. A difference names the table row. Add the case `tracker_shaped_key_fails_on_the_exempt_list`: a key of the tracker shape (capital letters, a hyphen and digits) built in the test from a synthetic prefix fails the identity scan, even in a file that is on the identity exempt list. No tracker key prefix is exempt; the list in `identity.rs` is unchanged and the test writes no real prefix. Add the case `tracker_exemption_lists_every_family_prefix`: every prefix of every row of `ids::FAMILIES` is in the family prefix list of `identity.rs`, so a new id family cannot ship without being exempt from FB0401.
6. Move FB0400 to FB0405 and FB0410 from `pending.toml` to `cases.toml`. FB0404 and FB0405 plant a `scan-allow.toml` in the fixture copy. Take each plant from the catalogue's known-bad example. Check that `handling = ["np-0007"]` (a well-formed label id that is not `public`) raises FB0410 and nothing else, and that `handling = ["public"]` raises nothing. A company store's FB0411 and FB0412 are checked in `core-labels`. Set `control = "SC-02"` on the FB0400 case, `control = "SC-03"` on the FB0401 case, `control = "SC-05"` on the FB0402 case, `control = "SC-06"` on the FB0403 case and `control = "ZN-01"` on the FB0410 case, and delete the SC-02, SC-03, SC-05, SC-06 and ZN-01 rows of `[[pending_control]]` in `pending.toml` (the check refuses a control that is named in both places). `cases.toml` holds one case per code, so add in `mod tests` of `crates/fieldbook-core/src/check/scans.rs` the test `fb0410_fires_for_every_non_public_label_kind`, with the line `// control: ZN-01` directly above it, over `handling = []`, `["marked"]`, `["owner-supplied"]` and `["np-0007"]`; each raises FB0410 and nothing else. The same test also covers `handling = ["Np 7"]` and a source row with an empty handling cell, each producing FB0410 alone.
7. Write `crates/fieldbook-core/tests/scan_no_panic.rs` with `parser_never_panics_on_200_seeded_inputs`. It includes the generator with `#[path = "../../../tests/support/rng.rs"] mod rng;`, builds 200 byte strings of length 0 to 128 from `Rng::new(1)`, and takes a valid `scan-allow.toml` (an entry with `sha256`, `match` and `reason`) with each of its bytes once flipped and once deleted. Each goes through `String::from_utf8_lossy` into the reader of `allow.rs`. The result is a value or an FB0404 problem and nothing panics.

8. For diagnostics on a successfully parsed front-matter field, use `PageMeta::field_location(key)` from the page parser. Do not infer a line by searching the raw text; comments and reordered keys keep their parser-recorded positions.

## Acceptance

```sh
cargo nextest run --locked -p fieldbook-core scan:: check::scans::
cargo nextest run --locked -p fieldbook-core --test scan_lists --test scan_no_panic
cargo nextest run --locked -p fieldbook --test known_bad
cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"; echo "exit $?"
just check
```

Expected: the tests pass; `check` prints its summary and `exit 0`; `just check` exits 0. After this card `pending.toml` names none of the seven codes.

## Negative control

In `allow.rs`, compare only `match` and ignore `sha256`. `scan::an_edit_ends_the_allowance` must fail. Restore the file. Then delete one family prefix from the tracker-key exception list in `identity.rs`: `scan_lists` must fail and name the Identity shapes row, and `tracker_exemption_lists_every_family_prefix` must fail and name the prefix. Restore the file. Then add the test's synthetic prefix to the exempt list: `tracker_shaped_key_fails_on_the_exempt_list` must fail. Restore the file. Then make FB0410 accept any one-element list: the FB0410 case with `handling = ["np-0007"]` must fail. Restore the file. Then make FB0410 accept `["marked"]`: `fb0410_fires_for_every_non_public_label_kind` must fail and name the list. Restore the file. Then make the reader of `allow.rs` call `unwrap()` on its first read: `scan_no_panic::parser_never_panics_on_200_seeded_inputs` must fail. Restore the file.

## Rollback

Revert the merge commit. The seven codes go back to `pending.toml`. Revert `add-new-and-edition` first if it has merged, since it calls `scan::marking`.

## BLOCKED

- The lead's content in `corpus/` raises one of these codes: name the code and the file, without the matched text. Do not edit `corpus/` and do not add an allowance.
- `handling = ["np-0007"]` also raises a code from an earlier card: name both codes and stop.
- A Scans table cannot be read by the list test without guessing its cell format: write the table name and ask the lead.
